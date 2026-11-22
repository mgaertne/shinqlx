use alloc::{borrow::Cow, sync::Arc};
use core::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::{HashMap, HashSet},
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{Mutex, mpsc::SyncSender},
};

use anyhow::Result;
use directories::UserDirs;
use reedline::{
    ColumnarMenu, Completer, CompletionResult, DefaultHinter, Emacs, ExternalPrinter,
    FileBackedHistory, History, KeyCode, KeyModifiers, MenuBuilder, Prompt, PromptEditMode,
    PromptHistorySearch, Reedline, ReedlineEvent, ReedlineMenu, SearchDirection, SearchQuery,
    Signal, Span, Suggestion, default_emacs_keybindings,
};
use termcolor::{Buffer, Color, ColorChoice, ColorSpec, WriteColor};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task,
};

use crate::{
    CONTINUE_RUNNING,
    cmd_line::CommandLineOptions,
    fetch::Fetched,
    guide::{flags_for, help_for, value_labels},
    learn::Learned,
    profile::{ServerFile, config_dir, server_file},
};

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
    let labelled: Vec<String> = user
        .labels
        .get(&cvar.to_lowercase())
        .into_iter()
        .flatten()
        .map(|(value, _)| value.clone())
        .chain(
            value_labels(cvar)
                .into_iter()
                .map(|(value, _)| value.to_string()),
        )
        .collect();

    let mut values: Vec<String> = Vec::new();
    for value in learned
        .map(|cvar| cvar.default.clone())
        .into_iter()
        .chain(guide)
        .chain(labelled)
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

/// What a value of a cvar means: the user's own label first, then the one of the console guide.
fn value_label(cvar: &str, value: &str, user: &UserLists) -> Option<String> {
    user.labels
        .get(&cvar.to_lowercase())
        .into_iter()
        .flatten()
        .find(|(known, _)| known == value)
        .map(|(_, label)| label.clone())
        .or_else(|| {
            value_labels(cvar)
                .into_iter()
                .find(|(known, _)| *known == value)
                .map(|(_, label)| label.to_string())
        })
}

/// The cvar whose value the cursor is at: `set <cvar> |` or `<cvar> |`.
fn cvar_of_value<'a>(words: &[&'a str]) -> Option<&'a str> {
    match words {
        [command, cvar] if is_set_command(command) => Some(cvar),
        [cvar] => Some(cvar),
        _ => None,
    }
}

/// The completions with the text for the list: the meaning for values, the description for names.
fn describe(
    found: Vec<String>,
    before: &str,
    start: usize,
    user: &UserLists,
) -> Vec<(String, Option<String>)> {
    let words = before.split_whitespace().collect::<Vec<_>>();
    let cvar = cvar_of_value(&words);
    let describe_names = start == 0;

    found
        .into_iter()
        .map(|item| {
            let text = if describe_names {
                let note = setting_note(&item, user);
                match (note, help_for(&item)) {
                    (Some(note), Some(help)) => Some(format!("{note} {help}")),
                    (Some(note), None) => Some(note),
                    (None, help) => help.map(str::to_string),
                }
            } else {
                cvar.and_then(|cvar| {
                    value_label(cvar, &item, user).or_else(|| help_for(cvar).map(str::to_string))
                })
            };
            (item, text)
        })
        .collect()
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
    labels: HashMap<String, Vec<(String, String)>>,
    cvars: HashMap<String, LearnedCvar>,
    latched: HashSet<String>,
    read_only: HashSet<String>,
    cvar_flags: HashMap<String, String>,
}

/// What setting the cvar does according to the guide and to what the server said: `latched`, `read-only` and so on.
fn setting_note(cvar: &str, user: &UserLists) -> Option<String> {
    let name = cvar.to_lowercase();
    let mut flags = flags_for(cvar).to_string();
    if let Some(reported) = user.cvar_flags.get(&name) {
        flags.push_str(reported);
    }
    if user.latched.contains(&name) {
        flags.retain(|flag| !matches!(flag, 'R' | 'W'));
        flags.push('L');
    } else if user.read_only.contains(&name) {
        flags.push('R');
    }
    let notes: Vec<&str> = ['C', 'I', 'L', 'R', 'W']
        .into_iter()
        .filter(|flag| flags.contains(*flag))
        .map(|flag| match flag {
            'C' => "cheat",
            'I' => "init only",
            'L' => "latched",
            'R' => "read-only",
            _ => "write-protected",
        })
        .collect();

    (!notes.is_empty()).then(|| format!("[{}]", notes.join(", ")))
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

/// The names that start with `word` first, then the ones that only contain it, so `gametype` finds `g_gametype`.
fn matching_names<'a>(names: impl Iterator<Item = &'a str>, word: &str) -> Vec<String> {
    let word = word.to_lowercase();
    let (prefixed, containing): (Vec<&str>, Vec<&str>) = names
        .filter(|name| name.to_lowercase().contains(&word))
        .partition(|name| name.to_lowercase().starts_with(&word));
    prefixed
        .into_iter()
        .chain(containing)
        .map(str::to_string)
        .collect()
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
        [command] if is_set_command(command) => matching_names(
            user.names.iter().map(String::as_str).chain(CVARS.lines()),
            &word,
        ),
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
        matching_names(
            user.names
                .iter()
                .map(String::as_str)
                .chain(COMMANDS.lines())
                .chain(CVARS.lines()),
            word,
        )
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

/// The server file of the session, if it was started with one.
fn profile_file(session: &SessionFiles) -> Option<ServerFile> {
    session
        .profile
        .as_deref()
        .and_then(server_file)
        .map(ServerFile::new)
}

/// The file that `--fetch-completions` keeps for the connected server: `completions-<host>.txt` for the names and
/// `completions-maps-<host>.txt` for the maps.
fn fetched_file(session: &SessionFiles, kind: &str) -> Option<PathBuf> {
    let slug = host_slug(session)?;
    config_dir().map(|dir| dir.join(format!("{kind}-{slug}.txt")))
}

fn write_lines(file: &Path, lines: &[String]) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, lines.join("\n") + "\n")
}

/// A list the user typed covers only one kind of names, so it is added to what the file already holds.
fn merged(file: &Path, found: &[String]) -> Vec<String> {
    let mut names = load_lines(&[file.to_path_buf()]);
    names.extend_from_slice(found);
    names.sort_unstable_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

/// Keeps what the server reported for the next completion and returns the file with the names.
pub(crate) fn save_completions(session: &SessionFiles, fetched: &Fetched) -> io::Result<PathBuf> {
    if let Some(file) = profile_file(session) {
        file.merge_fetched(fetched)?;
        return Ok(file.path().to_path_buf());
    }

    let missing = || {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no config directory or no tcp host",
        )
    };
    let names_file = fetched_file(session, "completions").ok_or_else(missing)?;
    let maps_file = fetched_file(session, "completions-maps").ok_or_else(missing)?;
    write_lines(&names_file, &merged(&names_file, &fetched.names))?;
    write_lines(&maps_file, &merged(&maps_file, &fetched.maps))?;

    Ok(names_file)
}

/// `<stem>.txt` for all servers, `<stem>-<host>.txt` and the fetched `<fetched>-<host>.txt` for the connected one, all
/// in the config directory.
fn list_files(session: &SessionFiles, stem: &str, fetched: &str) -> Vec<PathBuf> {
    let Some(dir) = config_dir() else {
        return Vec::new();
    };

    [Some(format!("{stem}.txt"))]
        .into_iter()
        .chain([host_slug(session).map(|slug| format!("{stem}-{slug}.txt"))])
        .flatten()
        .map(|file_name| dir.join(file_name))
        .chain(fetched_file(session, fetched))
        .collect()
}

fn user_names_files(session: &SessionFiles) -> Vec<PathBuf> {
    list_files(session, "commands", "completions")
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
        Learned::Latched(name) | Learned::ReadOnly(name) => {
            let kind = if matches!(learned, Learned::Latched(_)) {
                "latched"
            } else {
                "readonly"
            };
            let name = name.to_lowercase();
            (format!("{kind}\t{name}\t"), format!("{kind}\t{name}"))
        }
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
pub(crate) fn remember(session: &SessionFiles, learned: &Learned) -> io::Result<()> {
    if let Some(file) = profile_file(session) {
        return file.remember(learned);
    }

    let Some(file) = fetched_file(session, "learned") else {
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

fn load_learned(session: &SessionFiles, user: &mut UserLists) {
    let text = fetched_file(session, "learned")
        .and_then(|file| std::fs::read_to_string(file).ok())
        .unwrap_or_default();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["factories", factories @ ..] => {
                user.factories = factories.iter().map(|name| name.to_string()).collect();
            }
            ["latched", name] => {
                user.latched.insert(name.to_string());
            }
            ["readonly", name] => {
                user.read_only.insert(name.to_string());
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

fn load_profile(file: &ServerFile, user: &mut UserLists) {
    user.names.extend(file.names());
    user.maps.extend(file.maps());
    user.factories = file.factories();
    user.latched.extend(file.latched());
    user.read_only.extend(file.read_only());
    user.usages.extend(file.usages());
    user.cvar_flags.extend(file.cvar_flags());
    user.cvars.extend(
        file.cvars()
            .into_iter()
            .map(|(name, (default, minimum, maximum))| {
                (
                    name,
                    LearnedCvar {
                        default,
                        minimum,
                        maximum,
                    },
                )
            }),
    );
}

/// Lines `cvar<TAB>value<TAB>meaning` of `values.txt` and `values-<host>.txt`.
fn parse_labels(lines: &[String]) -> HashMap<String, Vec<(String, String)>> {
    let mut labels: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for line in lines {
        if let Some((cvar, value, label)) = line.split_once('\t').and_then(|(cvar, rest)| {
            rest.split_once('\t')
                .map(|(value, label)| (cvar, value, label))
        }) {
            labels
                .entry(cvar.trim().to_lowercase())
                .or_default()
                .push((value.trim().to_string(), label.trim().to_string()));
        }
    }
    labels
}

fn load_user_lists(session: &SessionFiles) -> UserLists {
    let mut user = UserLists {
        names: load_lines(&user_names_files(session)),
        maps: load_lines(&list_files(session, "maps", "completions-maps")),
        labels: parse_labels(&load_lines(&list_files(
            session,
            "values",
            "completions-values",
        ))),
        ..UserLists::default()
    };
    load_learned(session, &mut user);
    if let Some(file) = profile_file(session) {
        load_profile(&file, &mut user);
    }
    let queried: Vec<String> = user.cvars.keys().cloned().collect();
    user.names.extend(queried);
    user
}

/// The usage of a command as an entry that leaves the word as it is.
fn usage_listing(usage: String, word: &str, span: Span) -> Vec<Suggestion> {
    vec![Suggestion {
        value: word.to_string(),
        display_override: Some(usage),
        description: Some("reported by this server".to_string()),
        span,
        ..Suggestion::default()
    }]
}

/// The history, newest entry first, shared between the editor loop and the completer.
type SharedHistory = Arc<Mutex<Vec<String>>>;

/// What names the files of a session: the server it talks to and its server file, if it has one.
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionFiles {
    host: String,
    profile: Option<String>,
}

impl From<&CommandLineOptions> for SessionFiles {
    fn from(options: &CommandLineOptions) -> Self {
        Self {
            host: options.host.clone(),
            profile: options.profile.clone(),
        }
    }
}

struct ServerCompleter {
    session: SessionFiles,
    history: SharedHistory,
}

impl Completer for ServerCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult {
        let typed = line.get(..pos).unwrap_or(line);
        let start = typed.rfind(char::is_whitespace).map_or(0, |at| at + 1);
        let word = typed.get(start..).unwrap_or_default();
        let before = typed.get(..start).unwrap_or_default();
        let span = Span::new(start, pos);

        let user = load_user_lists(&self.session);
        let history = self
            .history
            .lock()
            .map(|history| history.clone())
            .unwrap_or_default();
        let found = completions(
            typed,
            word,
            start,
            history.iter().map(String::as_str),
            &user,
        );
        if found.is_empty()
            && start > 0
            && let Some(usage) = usage_hint(before, &user)
        {
            return CompletionResult::fresh(usage_listing(usage, word, span));
        }

        let suggestions: Vec<Suggestion> = describe(found, before, start, &user)
            .into_iter()
            .map(|(value, description)| Suggestion {
                value,
                description,
                span,
                ..Suggestion::default()
            })
            .collect();
        CompletionResult::fresh(suggestions)
    }
}

struct EmptyPrompt;

impl Prompt for EmptyPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _prompt_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("")
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

/// The color of `^1` to `^7` with the RGB values of the 8 color palette in the guide at <https://www.euere.eu/ql/>.
/// `^0` stays the black of the terminal theme, so that it remains readable.
fn ql_color(code: &str) -> Color {
    match code {
        "^0" => Color::Black,
        "^1" => Color::Rgb(0xFF, 0x00, 0x00),
        "^2" => Color::Rgb(0x00, 0xFF, 0x00),
        "^3" => Color::Rgb(0xFF, 0xFF, 0x00),
        "^4" => Color::Rgb(0x00, 0x00, 0xFF),
        "^5" => Color::Rgb(0x00, 0xFF, 0xFF),
        "^6" => Color::Rgb(0xFF, 0x00, 0xFF),
        _ => Color::Rgb(0xFF, 0xFF, 0xFF),
    }
}

fn write_formatted_ql_colors(buffer: &mut Buffer, text: &str) -> Result<()> {
    let stripped_msg = text.strip_prefix("broadcast: ").unwrap_or(text).trim();

    let stripped_text = stripped_msg
        .strip_prefix("print \"")
        .map_or(stripped_msg, |printed_text| {
            printed_text.trim_end_matches(['\"', '\n'])
        });

    let ql_colors = split_ql_colors(stripped_text);
    ql_colors.into_iter().try_for_each(|(color, substr)| {
        let mut color_spec = ColorSpec::new();
        match color {
            "reset" => color_spec.set_reset(true),
            code => color_spec.set_fg(Some(ql_color(code))),
        };

        buffer.set_color(&color_spec)?;
        write!(buffer, "{substr}")
    })?;
    buffer.set_color(ColorSpec::new().set_reset(true))?;

    Ok(())
}

fn host_slug(session: &SessionFiles) -> Option<String> {
    session
        .host
        .strip_prefix("tcp://")
        .map(|hostname| hostname.replace(".", "_").replace(":", "_"))
}

fn get_history_file(session: &SessionFiles) -> Option<PathBuf> {
    UserDirs::new().map(|user_dirs| {
        let mut home_dir = user_dirs.home_dir().to_path_buf();
        match host_slug(session) {
            None => home_dir.push(".ql_zmq_rcon.history"),
            Some(slug) => home_dir.push(format!(".ql_zmq_rcon-{slug}.history")),
        }
        home_dir
    })
}
/// Entries of the history file that the editor keeps.
const HISTORY_SIZE: usize = 1000;

/// The history entries, newest first.
fn history_lines(history: &dyn History) -> Vec<String> {
    history
        .search(SearchQuery::everything(SearchDirection::Backward, None))
        .map(|items| items.into_iter().map(|item| item.command_line).collect())
        .unwrap_or_default()
}

/// Deletes the older entries that equal the newest one, so that a command shows up once in the history.
fn drop_older_duplicates(history: &mut dyn History, line: &str) {
    let duplicates: Vec<_> = history
        .search(SearchQuery::everything(SearchDirection::Backward, None))
        .map(|items| {
            items
                .into_iter()
                .filter(|item| item.command_line == line)
                .skip(1)
                .filter_map(|item| item.id)
                .collect()
        })
        .unwrap_or_default();
    for id in duplicates {
        let _ = history.delete(id);
    }
}

fn line_editor(
    session: &SessionFiles,
    printer: ExternalPrinter<String>,
    break_signal: Arc<AtomicBool>,
) -> Result<(Reedline, SharedHistory)> {
    let history: Box<dyn History> = match get_history_file(session) {
        Some(file) => Box::new(FileBackedHistory::with_file(HISTORY_SIZE, file)?),
        None => Box::new(FileBackedHistory::new(HISTORY_SIZE)?),
    };
    let shared_history: SharedHistory = Arc::new(Mutex::new(history_lines(history.as_ref())));

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let completer = ServerCompleter {
        session: session.clone(),
        history: Arc::clone(&shared_history),
    };
    let menu = ColumnarMenu::default()
        .with_name("completion_menu")
        .with_columns(1);

    let editor = Reedline::create()
        .with_history(history)
        .with_completer(Box::new(completer))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_hinter(Box::new(
            DefaultHinter::default().with_style(reedline::Color::DarkGray.normal()),
        ))
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
        .with_external_printer(printer)
        .with_break_signal(break_signal);

    Ok((editor, shared_history))
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
    printer: &SyncSender<String>,
    color: &ColorChoice,
    mut display_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    while let Some(line) = display_receiver.recv().await {
        let mut buffer = terminal_buffer(color);
        write_formatted_ql_colors(&mut buffer, &line)?;
        let text = String::from_utf8(buffer.into_inner())
            .expect("the buffer only receives text that was written as str");
        if printer.send(text).is_err() {
            break;
        }
    }

    Ok(())
}

/// Shows every received line at once and reads the input on a blocking thread, so the idle terminal costs no CPU.
pub(crate) async fn run_terminal(
    args: CommandLineOptions,
    zmq_sender: UnboundedSender<String>,
    display_receiver: UnboundedReceiver<String>,
) -> Result<()> {
    let printer = ExternalPrinter::default();
    let break_signal = Arc::new(AtomicBool::new(false));
    let output = printer.sender();
    let (editor, history) = line_editor(
        &SessionFiles::from(&args),
        printer,
        Arc::clone(&break_signal),
    )?;

    let reader = task::spawn_blocking(move || read_input(editor, &history, zmq_sender));

    let displayed = display_lines(&output, &args.color, display_receiver).await;
    CONTINUE_RUNNING.store(false, Ordering::Release);
    break_signal.store(true, Ordering::Release);

    reader.await??;
    displayed
}

fn read_input(
    mut editor: Reedline,
    shared_history: &SharedHistory,
    zmq_sender: UnboundedSender<String>,
) -> Result<()> {
    while CONTINUE_RUNNING.load(Ordering::Acquire) {
        match editor.read_line(&EmptyPrompt)? {
            Signal::Success(line) => {
                if !CONTINUE_RUNNING.load(Ordering::Acquire) {
                    break;
                }

                if line.is_empty() {
                    continue;
                }

                drop_older_duplicates(editor.history_mut(), &line);
                editor.sync_history()?;
                if let Ok(mut history) = shared_history.lock() {
                    *history = history_lines(editor.history());
                }

                if line.trim().eq_ignore_ascii_case("exit") {
                    break;
                }

                if zmq_sender.send(line).is_err() {
                    break;
                }
            }
            Signal::CtrlC(typed) if !typed.is_empty() => {}
            Signal::CtrlC(_) | Signal::CtrlD => break,
            _ => {}
        }
    }

    drop(zmq_sender);
    CONTINUE_RUNNING.store(false, Ordering::Release);

    Ok(())
}

#[cfg(test)]
mod line_editor_tests {
    use reedline::Completer;
    use termcolor::{Buffer, ColorChoice, WriteColor};

    use super::{
        CommandLineOptions, LearnedCvar, ServerCompleter, SessionFiles, SharedHistory, UserLists,
        completions, describe, get_history_file, known_values, parse_labels, parse_user_names,
        setting_note, split_ql_colors, terminal_buffer, usage_hint, user_names_files, value_label,
        write_formatted_ql_colors,
    };
    use crate::guide::help_for;

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
    fn color_codes_have_the_rgb_values_of_the_guide() {
        let mut buffer = Buffer::ansi();
        write_formatted_ql_colors(&mut buffer, "^1red^6pink").expect("writing to a buffer works");

        let output = rendered(buffer);
        assert!(output.contains("38;2;255;0;0m"), "{output:?}");
        assert!(output.contains("38;2;255;0;255m"), "{output:?}");
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
    fn names_match_in_the_middle_after_the_ones_that_start_with_the_word() {
        let user = UserLists::default();

        let first = completions("gametype", "gametype", 0, [].into_iter(), &user);
        let set = completions("set gametype", "gametype", 4, [].into_iter(), &user);

        assert!(first.iter().any(|name| name == "g_gametype"), "{first:?}");
        assert!(set.iter().any(|name| name == "g_gametype"), "{set:?}");
        let ranked = completions("sv_", "sv_", 0, [].into_iter(), &user);
        let boundary = ranked.iter().position(|name| !name.starts_with("sv_"));
        assert!(
            boundary.is_none_or(|at| ranked[..at].iter().all(|name| name.starts_with("sv_"))),
            "{ranked:?}"
        );
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

        let files = user_names_files(&SessionFiles::from(&args));

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
    fn the_guide_flags_that_limit_setting_are_noted() {
        let user = UserLists::default();

        assert_eq!(
            setting_note("sv_cheats", &user),
            Some("[read-only]".to_string())
        );
        assert_eq!(
            setting_note("bot_enable", &user),
            Some("[latched]".to_string())
        );
        assert_eq!(setting_note("sv_hostname", &user), None);
    }

    #[test]
    fn the_flags_of_the_cvarlist_are_noted() {
        let mut user = UserLists::default();
        user.cvar_flags
            .insert("sv_hostname".to_string(), "SAC".to_string());

        assert_eq!(
            setting_note("sv_hostname", &user),
            Some("[cheat]".to_string())
        );
    }

    #[test]
    fn a_read_only_cvar_reported_by_the_server_is_noted() {
        let mut user = UserLists::default();
        user.read_only.insert("sv_hostname".to_string());

        assert_eq!(
            setting_note("sv_hostname", &user),
            Some("[read-only]".to_string())
        );
    }

    #[test]
    fn a_latched_cvar_reported_by_the_server_replaces_the_read_only_note() {
        let mut user = UserLists::default();
        user.latched.insert("sv_cheats".to_string());

        assert_eq!(
            setting_note("sv_cheats", &user),
            Some("[latched]".to_string())
        );
        assert_eq!(setting_note("unknown_cvar", &user), None);
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
    fn values_are_listed_with_their_meaning_but_inserted_without_it() {
        let user = UserLists::default();
        let found = completions("g_gametype ", "", 11, [].into_iter(), &user);

        let described = describe(found, "g_gametype ", 11, &user);

        let ca = described
            .iter()
            .find(|(value, _)| value == "4")
            .expect("the value 4 is offered");
        assert_eq!(ca.1.as_deref(), Some("Clan Arena"));
    }

    #[test]
    fn values_without_a_meaning_show_the_help_of_their_cvar() {
        let user = UserLists::default();

        let described = describe(vec!["800".to_string()], "g_gravity ", 10, &user);

        assert_eq!(
            described[0].1.as_deref(),
            help_for("g_gravity"),
            "{described:?}"
        );
        assert!(described[0].1.is_some());
    }

    #[test]
    fn names_are_listed_with_their_description_however_many_there_are() {
        let user = UserLists::default();

        let described = describe(vec!["timelimit".to_string(); 60], "", 0, &user);

        assert!(described.iter().all(|(_, description)| {
            description.as_deref() == Some("Sets the time limit on your server.")
        }));
    }

    #[test]
    fn the_completer_replaces_the_word_before_the_cursor() {
        let mut completer = ServerCompleter {
            session: SessionFiles::default(),
            history: SharedHistory::default(),
        };

        let result = completer.complete("g_gametype 4", 12);
        let usage = completer.complete("g_gametype ", 11);

        assert!(result.suggestions().iter().all(|s| s.span.start == 11));
        assert!(
            usage
                .suggestions()
                .iter()
                .any(|s| s.value == "4" && s.description.as_deref() == Some("Clan Arena"))
        );
    }

    #[test]
    fn user_values_get_their_own_meaning_before_the_guide() {
        let user = UserLists {
            labels: parse_labels(&[
                "g_gametype\t4\tmy clan arena".to_string(),
                "qlx_mode\t7\tfast".to_string(),
                "broken line".to_string(),
            ]),
            ..UserLists::default()
        };

        assert_eq!(
            value_label("G_GAMETYPE", "4", &user).as_deref(),
            Some("my clan arena")
        );
        assert_eq!(
            value_label("g_gametype", "5", &user).as_deref(),
            Some("Capture the Flag")
        );
        assert!(
            known_values("qlx_mode", &user)
                .iter()
                .any(|value| value == "7")
        );
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

        let tcp_file =
            get_history_file(&SessionFiles::from(&tcp)).expect("a home directory is set");
        let ipc_file =
            get_history_file(&SessionFiles::from(&ipc)).expect("a home directory is set");

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
