use alloc::sync::Arc;
use core::{sync::atomic::Ordering, time::Duration};
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::Result;
use directories::UserDirs;
use linefeed::{Completer, Completion, DefaultTerminal, Interface, Prompter, ReadResult, Signal};
use termcolor::{Buffer, Color, ColorChoice, ColorSpec, WriteColor};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{CONTINUE_RUNNING, CommandLineOptions};

struct HistoryCompleter {}

impl Completer<DefaultTerminal> for HistoryCompleter {
    fn complete(
        &self,
        _word: &str,
        prompter: &Prompter<DefaultTerminal>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let current_buffer = prompter.buffer();
        Some(
            prompter
                .history()
                .rev()
                .filter(|item| item.starts_with(current_buffer))
                .map(|item| Completion::simple(item[start..].to_string()))
                .collect(),
        )
    }
}

fn split_ql_colors(input: &str) -> Vec<(&str, &str)> {
    let split_regex = regex::Regex::new(r"\^[0-7]").unwrap();
    let mut result = vec![("reset", split_regex.split(input).next())];

    let capture_regex = regex::Regex::new(r"(\^[0-7])([^\^]+)").unwrap();
    for (_, [color, string]) in capture_regex
        .captures_iter(input)
        .map(|capture| capture.extract())
    {
        result.push((color, Some(string)));
    }

    result
        .into_iter()
        .filter_map(|(color, string)| match string {
            None => None,
            Some(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some((color, text))
                }
            }
        })
        .collect::<Vec<(&str, &str)>>()
}

fn write_formatted_ql_colors(buffer: &mut Buffer, text: &str) -> Result<()> {
    let stripped_msg = text.strip_prefix("broadcast: ").unwrap_or(text).trim();

    let is_print_msg = stripped_msg.starts_with("print \"");

    let stripped_text = stripped_msg
        .strip_prefix("print \"")
        .map_or(stripped_msg, |printed_text| {
            printed_text.trim_end_matches(['\"', '\n'])
        });

    let ql_colors = split_ql_colors(stripped_text);
    ql_colors.into_iter().try_for_each(|(color, substr)| {
        let mut color_spec = ColorSpec::new();
        color_spec.set_intense(is_print_msg);
        match color {
            "^0" => color_spec.set_fg(Some(Color::Black)),
            "^1" => color_spec.set_fg(Some(Color::Red)),
            "^2" => color_spec.set_fg(Some(Color::Green)),
            "^3" => color_spec.set_fg(Some(Color::Yellow)),
            "^4" => color_spec.set_fg(Some(Color::Blue)),
            "^5" => color_spec.set_fg(Some(Color::Cyan)),
            "^6" => color_spec.set_fg(Some(Color::Magenta)),
            "reset" => color_spec.set_reset(true),
            _ => color_spec.set_fg(Some(Color::White)),
        };

        buffer.set_color(&color_spec)?;
        write!(buffer, "{substr}")
    })?;
    buffer.set_color(ColorSpec::new().set_reset(true))?;

    Ok(())
}

fn get_history_file(args: &CommandLineOptions) -> Option<PathBuf> {
    UserDirs::new().map(|user_dirs| {
        let mut home_dir = user_dirs.home_dir().to_path_buf();
        match &args.host.strip_prefix("tcp://") {
            None => home_dir.push(".ql_zmq_rcon.history"),
            Some(hostname) => home_dir.push(format!(
                ".ql_zmq_rcon-{}.history",
                hostname.replace(".", "_").replace(":", "_")
            )),
        }
        home_dir
    })
}
fn terminal(args: &CommandLineOptions) -> Result<Interface<DefaultTerminal>> {
    let editor = Interface::new(env!("CARGO_PKG_NAME"))?;

    editor.set_prompt("")?;
    editor
        .lock_reader()
        .set_print_completions_horizontally(false);

    [Signal::Interrupt, Signal::Quit, Signal::Suspend]
        .into_iter()
        .for_each(|signal| {
            editor.set_report_signal(signal, true);
        });

    if let Some(history_file) = get_history_file(args)
        && history_file.exists()
    {
        editor.load_history(&history_file)?;
    };

    let history_completer = HistoryCompleter {};
    editor.set_completer(Arc::new(history_completer));

    Ok(editor)
}

fn terminal_buffer(color_choice: &ColorChoice) -> Buffer {
    match color_choice {
        ColorChoice::Always => Buffer::ansi(),
        ColorChoice::AlwaysAnsi => Buffer::ansi(),
        ColorChoice::Auto => {
            if io::stdout().is_terminal() {
                Buffer::ansi()
            } else {
                Buffer::no_color()
            }
        }
        ColorChoice::Never => Buffer::no_color(),
    }
}

pub(crate) async fn run_terminal(
    args: CommandLineOptions,
    zmq_sender: UnboundedSender<String>,
    mut display_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    let terminal = terminal(&args)?;

    while CONTINUE_RUNNING.load(Ordering::Acquire) {
        while let Ok(line) = display_receiver.try_recv() {
            let mut buffer = terminal_buffer(&args.color);
            write_formatted_ql_colors(&mut buffer, &line)?;
            terminal.lock_writer_erase().and_then(|mut writer| {
                writeln!(
                    writer,
                    "{}",
                    String::from_utf8(buffer.into_inner()).unwrap()
                )
            })?;
        }

        match terminal.read_line_step(Some(Duration::from_millis(250))) {
            Ok(None) => continue,
            Ok(Some(ReadResult::Input(line))) => {
                if !CONTINUE_RUNNING.load(Ordering::Acquire) {
                    break;
                }

                if line.is_empty() {
                    continue;
                }

                if let Ok(indexes) = terminal.lock_writer_append().map(|writer| {
                    writer
                        .history()
                        .enumerate()
                        .filter_map(|(index, item)| if item != line { None } else { Some(index) })
                        .collect::<Vec<usize>>()
                }) {
                    indexes
                        .iter()
                        .rev()
                        .for_each(|&index| terminal.remove_history(index));
                };
                terminal.add_history_unique(line.clone());
                if let Some(history_file) = get_history_file(&args) {
                    let _ = terminal.save_history(&history_file);
                }

                if ["exit"].contains(&line.to_lowercase().trim()) {
                    break;
                }

                if zmq_sender.send(line).is_err() {
                    break;
                }
            }
            Ok(Some(ReadResult::Eof)) => {
                writeln!(terminal, "{}", terminal.buffer())?;
                if terminal.buffer().is_empty() {
                    break;
                }

                let _ = terminal.set_buffer("");
            }
            Ok(Some(ReadResult::Signal(Signal::Interrupt))) => {
                writeln!(terminal, "{}^C", terminal.buffer())?;
                if terminal.buffer().is_empty() {
                    break;
                }

                terminal.set_buffer("")?;
            }
            Ok(Some(ReadResult::Signal(Signal::Quit))) => {
                break;
            }
            Ok(Some(ReadResult::Signal(signal))) => {
                writeln!(terminal, "received signal: {signal:?}")?;
            }
            Err(err) => {
                writeln!(terminal, "I/O error: {err:?}")?;
            }
        };
    }

    drop(zmq_sender);
    CONTINUE_RUNNING.store(false, Ordering::Release);

    while let Ok(line) = display_receiver.try_recv() {
        let mut buffer = terminal_buffer(&args.color);
        write_formatted_ql_colors(&mut buffer, &line)?;
        terminal.lock_writer_erase().and_then(|mut writer| {
            writeln!(
                writer,
                "{}",
                String::from_utf8(buffer.into_inner()).unwrap()
            )
        })?;
    }

    drop(display_receiver);

    Ok(())
}
