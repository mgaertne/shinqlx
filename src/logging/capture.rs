use std::sync::{Mutex, Once};

use log::{Level, Log, Metadata, Record};

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
static INSTALL: Once = Once::new();

struct Capture(Mutex<Vec<(Level, String, String)>>);

impl Log for Capture {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        if let Ok(mut records) = self.0.lock() {
            records.push((
                record.level(),
                record.target().to_owned(),
                record.args().to_string(),
            ));
        }
    }

    fn flush(&self) {}
}

/// Records everything logged through the `log` crate for the tests of this process.
pub(crate) fn install() {
    INSTALL.call_once(|| {
        if log::set_logger(&CAPTURE).is_ok() {
            log::set_max_level(log::LevelFilter::Trace);
        }
    });
}

/// The records logged since the call of `mark`.
pub(crate) fn records_since(mark: usize) -> Vec<(Level, String, String)> {
    CAPTURE
        .0
        .lock()
        .map(|records| records.iter().skip(mark).cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn mark() -> usize {
    CAPTURE
        .0
        .lock()
        .map(|records| records.len())
        .unwrap_or_default()
}
