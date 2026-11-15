use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread,
};

use log::{
    Level, Record,
    kv::{Error as KvError, Key, Value, VisitSource},
};
use log4rs::append::Append;

const QUEUE_CAPACITY: usize = 8192;
const BACKLOG_CAPACITY: usize = 10_000;
const URGENT_WAIT: Duration = Duration::from_millis(50);
const RETRY_PAUSE: Duration = Duration::from_millis(1);

struct Entry {
    level: Level,
    target: String,
    message: String,
    module_path: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    key_values: Vec<(String, String)>,
    written: Option<SyncSender<()>>,
}

#[derive(Default)]
struct KeyValueCollector(Vec<(String, String)>);

impl<'kvs> VisitSource<'kvs> for KeyValueCollector {
    fn visit_pair(&mut self, key: Key<'kvs>, value: Value<'kvs>) -> Result<(), KvError> {
        self.0.push((key.to_string(), value.to_string()));
        Ok(())
    }
}

impl Entry {
    fn from_record(record: &Record<'_>, written: Option<SyncSender<()>>) -> Self {
        let mut collector = KeyValueCollector::default();
        let _ = record.key_values().visit(&mut collector);
        Self {
            level: record.level(),
            target: record.target().to_owned(),
            message: record.args().to_string(),
            module_path: record.module_path().map(str::to_owned),
            file: record.file().map(str::to_owned),
            line: record.line(),
            key_values: collector.0,
            written,
        }
    }

    fn dropped_notice(count: u64) -> Self {
        Self {
            level: Level::Warn,
            target: "shinqlx".to_owned(),
            message: format!("{count} log lines dropped, the logging queue was full"),
            module_path: None,
            file: None,
            line: None,
            key_values: vec![("func".to_owned(), "log".to_owned())],
            written: None,
        }
    }

    fn write_to(&self, appender: &dyn Append) -> anyhow::Result<()> {
        let key_values: Vec<(&str, &str)> = self
            .key_values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        appender.append(
            &Record::builder()
                .level(self.level)
                .target(&self.target)
                .args(format_args!("{}", self.message))
                .module_path(self.module_path.as_deref())
                .file(self.file.as_deref())
                .line(self.line)
                .key_values(&key_values.as_slice())
                .build(),
        )
    }
}

enum Message {
    Entry(Box<Entry>),
    Attach(Box<dyn Append>),
    Flush(SyncSender<()>),
}

/// Hands log records over to a writer thread so that the caller never waits for file I/O.
///
/// Until [`AsyncAppender::attach`] provides the appender that does the actual writing, the writer
/// thread keeps the records back; this way messages of the start-up phase end up in the log file
/// although its location is not known yet. When the queue is full, records below `Warn` are dropped
/// and counted, `Warn` and `Error` records wait a short time for room and for the write.
#[derive(Clone)]
pub(crate) struct AsyncAppender {
    sender: SyncSender<Message>,
    dropped: Arc<AtomicU64>,
}

impl fmt::Debug for AsyncAppender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncAppender")
            .field("dropped", &self.dropped.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AsyncAppender {
    /// Starts the writer thread; it ends when the last clone of the appender is dropped.
    pub(crate) fn spawn() -> Self {
        Self::spawn_with_capacity(QUEUE_CAPACITY)
    }

    fn spawn_with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = sync_channel(capacity);
        let dropped = Arc::new(AtomicU64::new(0));
        let writer_dropped = Arc::clone(&dropped);
        thread::Builder::new()
            .name("shinqlx-log".to_owned())
            .spawn(move || write_entries(&receiver, &writer_dropped))
            .expect("the operating system provides a thread for the log writer");
        Self { sender, dropped }
    }

    /// Sets the appender that writes the records, and passes it everything held back so far.
    pub(crate) fn attach(&self, appender: Box<dyn Append>) {
        let _ = self.sender.send(Message::Attach(appender));
    }

    /// Waits until everything enqueued before has been written, at most for `timeout`.
    pub(crate) fn wait_until_written(&self, timeout: Duration) {
        let (acknowledge, acknowledged) = sync_channel(1);
        if self.sender.send(Message::Flush(acknowledge)).is_ok() {
            let _ = acknowledged.recv_timeout(timeout);
        }
    }

    fn enqueue(&self, mut message: Message, urgent: bool) -> bool {
        let mut waited = Duration::ZERO;
        loop {
            match self.sender.try_send(message) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(returned)) => {
                    if !urgent || waited >= URGENT_WAIT {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        return false;
                    }
                    message = returned;
                    thread::sleep(RETRY_PAUSE);
                    waited += RETRY_PAUSE;
                }
            }
        }
    }
}

impl Append for AsyncAppender {
    fn append(&self, record: &Record<'_>) -> anyhow::Result<()> {
        let urgent = record.level() <= Level::Warn;
        let (written, acknowledged) = if urgent {
            let (sender, receiver) = sync_channel(1);
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let entry = Message::Entry(Box::new(Entry::from_record(record, written)));
        if self.enqueue(entry, urgent)
            && let Some(acknowledged) = acknowledged
        {
            let _ = acknowledged.recv_timeout(URGENT_WAIT);
        }
        Ok(())
    }

    fn flush(&self) {
        self.wait_until_written(URGENT_WAIT);
    }
}

fn write_entries(receiver: &Receiver<Message>, dropped: &AtomicU64) {
    let mut appender: Option<Box<dyn Append>> = None;
    let mut backlog: VecDeque<Entry> = VecDeque::new();
    let mut failure_reported = false;

    let mut write = |appender: &dyn Append, entry: &Entry| {
        if let Err(error) = entry.write_to(appender)
            && !failure_reported
        {
            failure_reported = true;
            eprintln!("shinqlx could not write a log entry: {error}");
        }
    };

    for message in receiver {
        match message {
            Message::Attach(new_appender) => {
                for entry in backlog.drain(..) {
                    write(new_appender.as_ref(), &entry);
                }
                appender = Some(new_appender);
            }
            Message::Entry(mut entry) => {
                let written = entry.written.take();
                match appender.as_deref() {
                    Some(appender) => write(appender, &entry),
                    None => {
                        if backlog.len() >= BACKLOG_CAPACITY {
                            backlog.pop_front();
                        }
                        backlog.push_back(*entry);
                    }
                }
                if let Some(written) = written {
                    let _ = written.send(());
                }
            }
            Message::Flush(acknowledge) => {
                if let Some(appender) = appender.as_deref() {
                    appender.flush();
                }
                let _ = acknowledge.send(());
            }
        }

        let missed = dropped.swap(0, Ordering::Relaxed);
        if missed > 0
            && let Some(appender) = appender.as_deref()
        {
            write(appender, &Entry::dropped_notice(missed));
        }
    }

    if let Some(appender) = appender.as_deref() {
        appender.flush();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pretty_assertions::assert_eq;

    use super::*;

    #[derive(Debug, Default, Clone)]
    struct Collecting {
        lines: Arc<Mutex<Vec<String>>>,
        delay: Duration,
    }

    impl Append for Collecting {
        fn append(&self, record: &Record<'_>) -> anyhow::Result<()> {
            thread::sleep(self.delay);
            let func = record
                .key_values()
                .get(Key::from("func"))
                .map(|value| value.to_string())
                .unwrap_or_default();
            self.lines
                .lock()
                .expect("the test is the only user")
                .push(format!(
                    "{} {} {} {}",
                    record.level(),
                    record.target(),
                    func,
                    record.args()
                ));
            Ok(())
        }

        fn flush(&self) {}
    }

    fn log_through(appender: &AsyncAppender, level: Level, target: &str, message: &str) {
        appender
            .append(
                &Record::builder()
                    .level(level)
                    .target(target)
                    .args(format_args!("{message}"))
                    .key_values(&[("func", "test")].as_slice())
                    .build(),
            )
            .expect("the async appender never fails");
    }

    #[test]
    fn records_are_written_in_order_with_their_key_values() {
        let collecting = Collecting::default();
        let appender = AsyncAppender::spawn();
        appender.attach(Box::new(collecting.clone()));

        log_through(&appender, Level::Debug, "shinqlx::plugin", "first");
        log_through(&appender, Level::Info, "shinqlx", "second");
        appender.wait_until_written(Duration::from_secs(5));

        assert_eq!(
            *collecting.lines.lock().unwrap(),
            vec![
                "DEBUG shinqlx::plugin test first".to_owned(),
                "INFO shinqlx test second".to_owned()
            ]
        );
    }

    #[test]
    fn records_before_the_attach_are_held_back_and_written_afterwards() {
        let collecting = Collecting::default();
        let appender = AsyncAppender::spawn();

        log_through(&appender, Level::Debug, "shinqlx", "early debug");
        log_through(&appender, Level::Error, "shinqlx", "early error");
        assert_eq!(collecting.lines.lock().unwrap().len(), 0);

        appender.attach(Box::new(collecting.clone()));
        log_through(&appender, Level::Info, "shinqlx", "late");
        appender.wait_until_written(Duration::from_secs(5));

        assert_eq!(
            *collecting.lines.lock().unwrap(),
            vec![
                "DEBUG shinqlx test early debug".to_owned(),
                "ERROR shinqlx test early error".to_owned(),
                "INFO shinqlx test late".to_owned()
            ]
        );
    }

    #[test]
    fn a_full_queue_drops_low_levels_and_reports_how_many() {
        let collecting = Collecting {
            delay: Duration::from_millis(20),
            ..Collecting::default()
        };
        let appender = AsyncAppender::spawn_with_capacity(4);
        appender.attach(Box::new(collecting.clone()));

        let started = std::time::Instant::now();
        (0..200).for_each(|i| log_through(&appender, Level::Debug, "shinqlx", &format!("{i}")));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "the caller must not wait for the slow writer"
        );
        appender.wait_until_written(Duration::from_secs(10));

        let lines = collecting.lines.lock().unwrap().clone();
        let notice_total: u64 = lines
            .iter()
            .filter_map(|line| line.split(" log lines dropped").next())
            .filter(|line| line.starts_with("WARN shinqlx log "))
            .filter_map(|line| line.rsplit(' ').next()?.parse::<u64>().ok())
            .sum();
        let written = lines
            .iter()
            .filter(|line| line.starts_with("DEBUG"))
            .count() as u64;
        assert!(notice_total > 0, "{lines:?}");
        assert_eq!(written + notice_total, 200);
    }

    #[test]
    fn errors_wait_until_they_are_written() {
        let collecting = Collecting {
            delay: Duration::from_millis(10),
            ..Collecting::default()
        };
        let appender = AsyncAppender::spawn();
        appender.attach(Box::new(collecting.clone()));

        log_through(&appender, Level::Error, "shinqlx", "visible at once");

        assert_eq!(collecting.lines.lock().unwrap().len(), 1);
    }
}
