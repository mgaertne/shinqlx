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

fn is_color_code(bytes: &[u8], at: usize) -> bool {
    bytes.get(at) == Some(&b'^') && matches!(bytes.get(at + 1), Some(b'0'..=b'7'))
}

fn split_ql_colors(input: &str) -> Vec<(&str, &str)> {
    let bytes = input.as_bytes();
    let first_code = (0..bytes.len())
        .find(|&at| is_color_code(bytes, at))
        .unwrap_or(bytes.len());

    let mut result = vec![("reset", &input[..first_code])];
    let mut at = first_code;
    while at < bytes.len() {
        if !is_color_code(bytes, at) {
            at += 1;
            continue;
        }
        let text_start = at + 2;
        let text_end = input[text_start..]
            .find('^')
            .map_or(input.len(), |offset| text_start + offset);
        if text_end > text_start {
            result.push((&input[at..text_start], &input[text_start..text_end]));
            at = text_end;
        } else {
            at += 1;
        }
    }

    result.retain(|(_, text)| !text.is_empty());
    result
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

#[cfg(test)]
mod linefeed_helper_tests {
    use clap::Parser;
    use termcolor::{Buffer, ColorChoice, WriteColor};

    use super::{
        CommandLineOptions, get_history_file, split_ql_colors, terminal_buffer,
        write_formatted_ql_colors,
    };

    fn rendered(buffer: Buffer) -> String {
        String::from_utf8(buffer.into_inner()).expect("the output is valid UTF-8")
    }

    fn plain(text: &str) -> String {
        let mut buffer = Buffer::no_color();
        write_formatted_ql_colors(&mut buffer, text).expect("writing to a buffer works");
        rendered(buffer)
    }

    #[test]
    fn split_ql_colors_returns_unformatted_text_as_reset() {
        assert_eq!(split_ql_colors("hello"), vec![("reset", "hello")]);
    }

    #[test]
    fn split_ql_colors_splits_at_every_color_code() {
        assert_eq!(
            split_ql_colors("pre^1red^2green"),
            vec![("reset", "pre"), ("^1", "red"), ("^2", "green")]
        );
    }

    #[test]
    fn split_ql_colors_skips_the_empty_leading_part() {
        assert_eq!(split_ql_colors("^4blue"), vec![("^4", "blue")]);
    }

    #[test]
    fn split_ql_colors_of_empty_text_is_empty() {
        assert!(split_ql_colors("").is_empty());
    }

    #[test]
    fn split_ql_colors_ignores_consecutive_color_codes_without_text() {
        assert_eq!(split_ql_colors("^1^2green"), vec![("^2", "green")]);
    }

    #[test]
    fn split_ql_colors_drops_text_after_an_unknown_color_code() {
        assert_eq!(split_ql_colors("^1red^8lost"), vec![("^1", "red")]);
    }

    #[test]
    fn split_ql_colors_keeps_non_ascii_text() {
        assert_eq!(split_ql_colors("ä^3ö"), vec![("reset", "ä"), ("^3", "ö")]);
    }

    #[test]
    fn colored_text_loses_its_quake_color_codes() {
        assert_eq!(plain("^1red ^2green"), "red green");
    }

    #[test]
    fn broadcast_prefix_and_print_wrapper_are_removed() {
        assert_eq!(plain("broadcast: print \"^3hello\n\""), "hello");
        assert_eq!(plain("print \"plain text\""), "plain text");
    }

    #[test]
    fn ansi_buffers_contain_escape_sequences() {
        let mut buffer = Buffer::ansi();
        write_formatted_ql_colors(&mut buffer, "^1red").expect("writing to a buffer works");

        let output = rendered(buffer);
        assert!(
            output.contains("\u{1b}[") && output.contains("red"),
            "{output:?}"
        );
    }

    #[test]
    fn all_color_codes_are_rendered() {
        for code in 0..=7 {
            let mut buffer = Buffer::ansi();
            write_formatted_ql_colors(&mut buffer, &format!("^{code}x"))
                .expect("writing to a buffer works");

            assert!(rendered(buffer).contains('x'));
        }
    }

    #[test]
    fn terminal_buffer_follows_the_color_choice() {
        assert!(!terminal_buffer(&ColorChoice::Never).supports_color());
        assert!(terminal_buffer(&ColorChoice::Always).supports_color());
        assert!(terminal_buffer(&ColorChoice::AlwaysAnsi).supports_color());
    }

    #[test]
    fn terminal_buffer_with_auto_does_not_color_without_a_terminal() {
        assert!(!terminal_buffer(&ColorChoice::Auto).supports_color());
    }

    #[test]
    fn history_file_depends_on_the_host() {
        let tcp = CommandLineOptions::try_parse_from(["ql_rcon", "--host", "tcp://10.0.0.1:27961"])
            .expect("the options are valid");
        let ipc = CommandLineOptions::try_parse_from(["ql_rcon", "--host", "ipc:///tmp/ql"])
            .expect("the options are valid");

        let tcp_file = get_history_file(&tcp).expect("a home directory is set");
        let ipc_file = get_history_file(&ipc).expect("a home directory is set");

        assert_eq!(
            tcp_file.file_name().and_then(|name| name.to_str()),
            Some(".ql_zmq_rcon-10_0_0_1_27961.history")
        );
        assert_eq!(
            ipc_file.file_name().and_then(|name| name.to_str()),
            Some(".ql_zmq_rcon.history")
        );
    }
}
