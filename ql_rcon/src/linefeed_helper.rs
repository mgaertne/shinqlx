use alloc::sync::Arc;
use core::{sync::atomic::Ordering, time::Duration};
use std::{
    collections::{HashMap, HashSet},
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use anyhow::Result;
use directories::{ProjectDirs, UserDirs};
use linefeed::{
    Completer, Completion, DefaultTerminal, Interface, Prompter, ReadResult, Signal, Suffix,
};
use termcolor::{Buffer, Color, ColorChoice, ColorSpec, WriteColor};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task,
};

use crate::{CONTINUE_RUNNING, CommandLineOptions, fetch::Fetched, learn::Learned};

/// Console commands of Quake Live, one per line.
const COMMANDS: &str = include_str!("commands.txt");
/// Console variables of Quake Live, one per line.
const CVARS: &str = include_str!("cvars.txt");

/// Console variables with their default and the values named in their usage: `name<TAB>default<TAB>value|value`.
const CVAR_VALUES: &str = include_str!("cvar_values.txt");

/// The default the server reported for a cvar, then the default and the values of its usage in the console guide, then
/// the range the server reported, without repeats.
fn known_values(cvar: &str, user: &UserLists) -> Vec<String> {
    let guide = CVAR_VALUES
        .lines()
        .map(|line| line.splitn(3, '\t'))
        .find(|fields| {
            fields
                .clone()
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(cvar))
        })
        .map(|mut fields| {
            let default = fields.nth(1).unwrap_or_default();
            let hints = fields.next().unwrap_or_default();
            core::iter::once(default)
                .chain(hints.split('|'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let learned = user.cvars.get(&cvar.to_lowercase());

    let mut values: Vec<String> = Vec::new();
    for value in learned
        .map(|cvar| cvar.default.clone())
        .into_iter()
        .chain(guide)
        .chain(
            learned
                .into_iter()
                .flat_map(|cvar| [cvar.minimum.clone(), cvar.maximum.clone()]),
        )
    {
        if !value.is_empty() && !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

fn is_set_command(command: &str) -> bool {
    ["set", "seta", "sets", "setu"]
        .iter()
        .any(|set| command.eq_ignore_ascii_case(set))
}

/// Names the user keeps in files or the server reported with `--fetch-completions`.
#[derive(Debug, Default)]
struct UserLists {
    names: Vec<String>,
    maps: Vec<String>,
    usages: HashMap<String, String>,
    factories: Vec<String>,
    cvars: HashMap<String, LearnedCvar>,
}

/// The default and the range of a cvar, as the server reported them.
#[derive(Debug, Default, Clone)]
struct LearnedCvar {
    default: String,
    minimum: String,
    maximum: String,
}

/// The usage the server reported for the command that the line starts with.
fn usage_hint(before: &str, user: &UserLists) -> Option<String> {
    let command = before.split_whitespace().next()?.to_lowercase();
    user.usages.get(&command).cloned()
}

fn is_map_command(command: &str) -> bool {
    ["map", "devmap"]
        .iter()
        .any(|map| command.eq_ignore_ascii_case(map))
}

/// Cvar names after `set`, the values of a cvar after `set <cvar>` or after the cvar itself, and map names after `map`,
/// `devmap` and `callvote map`, and the factories after the map.
fn argument_completions(before: &str, word: &str, user: &UserLists) -> Vec<String> {
    let word = word.to_lowercase();
    let starts_with_word = |name: &str| name.to_lowercase().starts_with(&word);

    match before.split_whitespace().collect::<Vec<_>>().as_slice() {
        [command] if is_map_command(command) => user
            .maps
            .iter()
            .filter(|name| starts_with_word(name))
            .cloned()
            .collect(),
        [callvote, map] if callvote.eq_ignore_ascii_case("callvote") && is_map_command(map) => user
            .maps
            .iter()
            .filter(|name| starts_with_word(name))
            .cloned()
            .collect(),
        [command, _map] if is_map_command(command) => user
            .factories
            .iter()
            .filter(|name| starts_with_word(name))
            .cloned()
            .collect(),
        [callvote, map, _map]
            if callvote.eq_ignore_ascii_case("callvote") && is_map_command(map) =>
        {
            user.factories
                .iter()
                .filter(|name| starts_with_word(name))
                .cloned()
                .collect()
        }
        [command] if is_set_command(command) => user
            .names
            .iter()
            .map(String::as_str)
            .chain(CVARS.lines())
            .filter(|name| starts_with_word(name))
            .map(str::to_string)
            .collect(),
        [command, cvar] if is_set_command(command) => known_values(cvar, user)
            .into_iter()
            .filter(|value| starts_with_word(value))
            .collect(),
        [cvar] => known_values(cvar, user)
            .into_iter()
            .filter(|value| starts_with_word(value))
            .collect(),
        _ => Vec::new(),
    }
}

/// History entries starting with `buffer` come first, followed by the known commands and cvars that start with `word`
/// when the first word of the line is being completed, or by the cvar names and values that fit the arguments.
fn completions<'a>(
    buffer: &str,
    word: &str,
    start: usize,
    history: impl Iterator<Item = &'a str>,
    user: &UserLists,
) -> Vec<String> {
    let mut found: Vec<String> = history
        .filter(|item| item.starts_with(buffer))
        .map(|item| item[start..].to_string())
        .collect();

    let known = if start == 0 {
        let word = word.to_lowercase();
        user.names
            .iter()
            .map(String::as_str)
            .chain(COMMANDS.lines())
            .chain(CVARS.lines())
            .filter(|name| name.to_lowercase().starts_with(&word))
            .map(str::to_string)
            .collect()
    } else {
        argument_completions(buffer.get(..start).unwrap_or_default(), word, user)
    };
    found.extend(known);

    let mut seen = HashSet::new();
    found.retain(|name| seen.insert(name.to_lowercase()));
    found
}

/// Names from a user file: one per line, empty lines and lines starting with `#` are skipped.
fn parse_user_names(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "ql_rcon").map(|dirs| dirs.config_dir().to_path_buf())
}

/// The file that `--fetch-completions` keeps for the connected server: `completions-<host>.txt` for the names and
/// `completions-maps-<host>.txt` for the maps.
fn fetched_file(args: &CommandLineOptions, kind: &str) -> Option<PathBuf> {
    let slug = host_slug(args)?;
    config_dir().map(|dir| dir.join(format!("{kind}-{slug}.txt")))
}

fn write_lines(file: &PathBuf, lines: &[String]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, lines.join("\n") + "\n")
}

/// Keeps what the server reported for the next completion and returns the file with the names.
pub(crate) fn save_completions(
    args: &CommandLineOptions,
    fetched: &Fetched,
) -> io::Result<PathBuf> {
    let missing = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no config directory or no tcp host",
        )
    };
    let names_file = fetched_file(args, "completions").ok_or_else(missing)?;
    let maps_file = fetched_file(args, "completions-maps").ok_or_else(missing)?;
    write_lines(&names_file, &fetched.names)?;
    write_lines(&maps_file, &fetched.maps)?;

    Ok(names_file)
}

/// `<stem>.txt` for all servers, `<stem>-<host>.txt` and the fetched `<fetched>-<host>.txt` for the connected one, all
/// in the config directory.
fn list_files(args: &CommandLineOptions, stem: &str, fetched: &str) -> Vec<PathBuf> {
    let Some(dir) = config_dir() else {
        return Vec::new();
    };

    [Some(format!("{stem}.txt"))]
        .into_iter()
        .chain([host_slug(args).map(|slug| format!("{stem}-{slug}.txt"))])
        .flatten()
        .map(|file_name| dir.join(file_name))
        .chain(fetched_file(args, fetched))
        .collect()
}

fn user_names_files(args: &CommandLineOptions) -> Vec<PathBuf> {
    list_files(args, "commands", "completions")
}

fn load_lines(files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            parse_user_names(&text)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One line per usage (`usage<TAB>command<TAB>text`) or cvar (`cvar<TAB>name<TAB>default<TAB>minimum<TAB>maximum`).
fn learned_line(learned: &Learned) -> (String, String) {
    match learned {
        Learned::Usage { command, text } => (
            format!("usage\t{command}\t"),
            format!("usage\t{command}\t{text}"),
        ),
        Learned::Factories(factories) => (
            "factories\t".to_string(),
            format!("factories\t{}", factories.join("\t")),
        ),
        Learned::Cvar {
            name,
            default,
            minimum,
            maximum,
        } => {
            let name = name.to_lowercase();
            (
                format!("cvar\t{name}\t"),
                format!("cvar\t{name}\t{default}\t{minimum}\t{maximum}"),
            )
        }
    }
}

/// Keeps what the server reported in `learned-<host>.txt`; a newer report replaces the older one of the same entry.
pub(crate) fn remember(args: &CommandLineOptions, learned: &Learned) -> io::Result<()> {
    let Some(file) = fetched_file(args, "learned") else {
        return Ok(());
    };
    let (key, line) = learned_line(learned);
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    if existing.lines().any(|known| known == line) {
        return Ok(());
    }

    let mut lines: Vec<String> = existing
        .lines()
        .filter(|known| !known.starts_with(&key))
        .map(str::to_string)
        .collect();
    lines.push(line);
    write_lines(&file, &lines)
}

fn load_learned(args: &CommandLineOptions, user: &mut UserLists) {
    let text = fetched_file(args, "learned")
        .and_then(|file| std::fs::read_to_string(file).ok())
        .unwrap_or_default();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["factories", factories @ ..] => {
                user.factories = factories.iter().map(|name| name.to_string()).collect();
            }
            ["usage", command, text] => {
                user.usages.insert(command.to_string(), text.to_string());
            }
            ["cvar", name, default, minimum, maximum] => {
                user.cvars.insert(
                    name.to_string(),
                    LearnedCvar {
                        default: default.to_string(),
                        minimum: minimum.to_string(),
                        maximum: maximum.to_string(),
                    },
                );
            }
            _ => (),
        }
    }
}

fn load_user_lists(args: &CommandLineOptions) -> UserLists {
    let mut user = UserLists {
        names: load_lines(&user_names_files(args)),
        maps: load_lines(&list_files(args, "maps", "completions-maps")),
        ..UserLists::default()
    };
    load_learned(args, &mut user);
    user
}

/// Two entries that insert nothing, because linefeed lists the candidates only when there are several.
fn usage_listing(usage: String) -> Vec<Completion> {
    [usage, "(reported by this server)".to_string()]
        .into_iter()
        .map(|display| Completion {
            completion: String::new(),
            display: Some(display),
            suffix: Suffix::None,
        })
        .collect()
}

struct HistoryCompleter {
    args: CommandLineOptions,
}

impl Completer<DefaultTerminal> for HistoryCompleter {
    fn complete(
        &self,
        word: &str,
        prompter: &Prompter<DefaultTerminal>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let user = load_user_lists(&self.args);
        let found = completions(
            prompter.buffer(),
            word,
            start,
            prompter.history().rev(),
            &user,
        );
        if found.is_empty()
            && start > 0
            && let Some(usage) =
                usage_hint(prompter.buffer().get(..start).unwrap_or_default(), &user)
        {
            return Some(usage_listing(usage));
        }

        Some(found.into_iter().map(Completion::simple).collect())
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

fn host_slug(args: &CommandLineOptions) -> Option<String> {
    args.host
        .strip_prefix("tcp://")
        .map(|hostname| hostname.replace(".", "_").replace(":", "_"))
}

fn get_history_file(args: &CommandLineOptions) -> Option<PathBuf> {
    UserDirs::new().map(|user_dirs| {
        let mut home_dir = user_dirs.home_dir().to_path_buf();
        match host_slug(args) {
            None => home_dir.push(".ql_zmq_rcon.history"),
            Some(slug) => home_dir.push(format!(".ql_zmq_rcon-{slug}.history")),
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

    let history_completer = HistoryCompleter { args: args.clone() };
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

async fn display_lines(
    terminal: &Interface<DefaultTerminal>,
    color: &ColorChoice,
    mut display_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    while let Some(line) = display_receiver.recv().await {
        let mut buffer = terminal_buffer(color);
        write_formatted_ql_colors(&mut buffer, &line)?;
        terminal.lock_writer_erase().and_then(|mut writer| {
            writeln!(
                writer,
                "{}",
                String::from_utf8(buffer.into_inner())
                    .expect("the buffer only receives text that was written as str")
            )
        })?;
    }

    Ok(())
}

/// Shows every received line at once and reads the input on a blocking thread, so the idle terminal costs no CPU.
pub(crate) async fn run_terminal(
    args: CommandLineOptions,
    zmq_sender: UnboundedSender<String>,
    display_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    let terminal = Arc::new(terminal(&args)?);

    let reader = task::spawn_blocking({
        let terminal = Arc::clone(&terminal);
        let args = args.clone();
        move || read_input(&terminal, &args, zmq_sender)
    });

    let displayed = display_lines(&terminal, &args.color, display_receiver).await;
    CONTINUE_RUNNING.store(false, Ordering::Release);

    reader.await??;
    displayed
}

fn read_input(
    terminal: &Interface<DefaultTerminal>,
    args: &CommandLineOptions,
    zmq_sender: UnboundedSender<String>,
) -> Result<()> {
    while CONTINUE_RUNNING.load(Ordering::Acquire) {
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
                if let Some(history_file) = get_history_file(args) {
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

    Ok(())
}

#[cfg(test)]
mod linefeed_helper_tests {
    use clap::Parser;
    use termcolor::{Buffer, ColorChoice, WriteColor};

    use super::{
        CommandLineOptions, LearnedCvar, UserLists, completions, get_history_file, known_values,
        parse_user_names, split_ql_colors, terminal_buffer, usage_hint, user_names_files,
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
    fn first_word_completes_known_commands_and_cvars() {
        let found = completions("sta", "sta", 0, [].into_iter(), &UserLists::default());

        assert!(found.iter().any(|name| name == "status"), "{found:?}");
    }

    #[test]
    fn first_word_completion_ignores_the_case() {
        let found = completions(
            "SV_HOST",
            "SV_HOST",
            0,
            [].into_iter(),
            &UserLists::default(),
        );

        assert!(found.iter().any(|name| name == "sv_hostname"), "{found:?}");
    }

    #[test]
    fn history_comes_first_and_is_not_repeated() {
        let found = completions(
            "kick",
            "kick",
            0,
            ["kick bob", "kick"].into_iter(),
            &UserLists::default(),
        );

        assert_eq!(found[..2], ["kick bob", "kick"]);
        assert_eq!(found.iter().filter(|name| *name == "kick").count(), 1);
    }

    #[test]
    fn repeats_between_history_and_lists_are_removed_regardless_of_the_case() {
        let user = UserLists {
            names: vec!["sv_padpackets".to_string(), "sv_padPackets".to_string()],
            ..UserLists::default()
        };

        let found = completions(
            "sv_pad",
            "sv_pad",
            0,
            ["sv_padPackets", "sv_padPackets"].into_iter(),
            &user,
        );

        assert_eq!(found, ["sv_padPackets"]);
    }

    #[test]
    fn user_names_are_completed_and_not_repeated() {
        let user = UserLists {
            names: vec!["qlx_myplugin".to_string(), "status".to_string()],
            maps: Vec::new(),
            ..UserLists::default()
        };

        let found = completions("qlx_my", "qlx_my", 0, [].into_iter(), &user);
        let status = completions("status", "status", 0, [].into_iter(), &user);

        assert_eq!(found, ["qlx_myplugin"]);
        assert_eq!(status.iter().filter(|name| *name == "status").count(), 1);
    }

    #[test]
    fn user_files_skip_comments_and_empty_lines() {
        let names =
            parse_user_names("# my server\n\n  !mycmd \nqlx_a\n  # indented\n").collect::<Vec<_>>();

        assert_eq!(names, ["!mycmd", "qlx_a"]);
    }

    #[test]
    fn user_files_are_named_after_the_host() {
        let args =
            CommandLineOptions::try_parse_from(["ql_rcon", "--host", "tcp://10.0.0.1:27961"])
                .expect("the options are valid");

        let files = user_names_files(&args);

        assert!(files.len() <= 3);
        assert!(files.iter().all(|file| file.ends_with("commands.txt")
            || file.ends_with("commands-10_0_0_1_27961.txt")
            || file.ends_with("completions-10_0_0_1_27961.txt")));
    }

    #[test]
    fn set_completes_cvar_names() {
        let found = completions(
            "set sv_host",
            "sv_host",
            4,
            [].into_iter(),
            &UserLists::default(),
        );

        assert_eq!(found, ["sv_hostname"]);
    }

    #[test]
    fn set_completes_the_default_and_the_values_of_the_cvar() {
        let found = completions(
            "seta r_debugSurface ",
            "",
            20,
            [].into_iter(),
            &UserLists::default(),
        );

        assert_eq!(found, ["0", "1", "2"]);
    }

    #[test]
    fn a_cvar_completes_its_values_without_set() {
        let found = completions(
            "cg_drawFPS 1",
            "1",
            11,
            [].into_iter(),
            &UserLists::default(),
        );

        assert_eq!(found, ["1"]);
    }

    #[test]
    fn map_commands_complete_map_names() {
        let user = UserLists {
            names: Vec::new(),
            maps: vec!["aerowalk".to_string(), "zen".to_string()],
            factories: vec!["ca".to_string(), "ctf".to_string()],
            ..UserLists::default()
        };

        assert_eq!(
            completions("map ", "", 4, [].into_iter(), &user),
            ["aerowalk", "zen"]
        );
        assert_eq!(
            completions("devmap Z", "Z", 7, [].into_iter(), &user),
            ["zen"]
        );
        assert_eq!(
            completions("callvote map a", "a", 13, [].into_iter(), &user),
            ["aerowalk"]
        );
        assert!(completions("kick ", "", 5, [].into_iter(), &user).is_empty());
        assert_eq!(
            completions("map zen ", "", 8, [].into_iter(), &user),
            ["ca", "ctf"]
        );
        assert_eq!(
            completions("callvote map zen ct", "ct", 17, [].into_iter(), &user),
            ["ctf"]
        );
    }

    #[test]
    fn learned_defaults_and_ranges_come_before_and_after_the_guide_values() {
        let mut user = UserLists::default();
        user.cvars.insert(
            "r_debugsurface".to_string(),
            LearnedCvar {
                default: "1".to_string(),
                minimum: "0".to_string(),
                maximum: "9".to_string(),
            },
        );

        assert_eq!(known_values("r_debugSurface", &user), ["1", "0", "2", "9"]);
    }

    #[test]
    fn the_learned_usage_is_found_by_the_command_of_the_line() {
        let mut user = UserLists::default();
        user.usages
            .insert("dir".to_string(), "dir <directory> [extension]".to_string());

        assert_eq!(
            usage_hint("DIR ", &user).as_deref(),
            Some("dir <directory> [extension]")
        );
        assert_eq!(usage_hint("kick ", &user), None);
    }

    #[test]
    fn the_default_of_a_cvar_without_value_hints_is_offered() {
        assert_eq!(known_values("g_gravity", &UserLists::default()), ["800"]);
        assert!(known_values("no_such_cvar", &UserLists::default()).is_empty());
    }

    #[test]
    fn later_words_only_complete_from_the_history() {
        let found = completions(
            "kick bo",
            "bo",
            5,
            ["kick bob"].into_iter(),
            &UserLists::default(),
        );

        assert_eq!(found, ["bob"]);
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
