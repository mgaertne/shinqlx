//! The server files `servers/<name>.toml` of the config directory, which `ql_rcon` and `ql_stats` read.
//!
//! A file has one table per interface of the server: `[rcon]` and `[stats]`. Both take `host`, `port` and `password`;
//! without `host` the host of the other table is used with the own `port`, because the two ports and passwords usually
//! differ while the machine is the same. `[rcon]` also holds `identity` and `color`. Files from before the tables
//! existed keep the values of `[rcon]` at the top and are still read that way.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use directories::ProjectDirs;
pub use toml_edit::DocumentMut;
use toml_edit::{Item, Key, Table, value};

const CONFIG_DIR_NAME: &str = "ql_zmq";
const LEGACY_CONFIG_DIR_NAME: &str = "ql_rcon";
const LEGACY_RCON_KEYS: [&str; 4] = ["host", "password", "identity", "color"];

/// The config directory, `~/.config/ql_zmq/` on Linux.
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", CONFIG_DIR_NAME).map(|dirs| dirs.config_dir().to_path_buf())
}

/// Moves the config directory of earlier versions, `~/.config/ql_rcon/` on Linux, to [`config_dir`] unless that exists.
///
/// The programs call it at their start; it says on stderr when it moved the directory.
pub fn migrate_legacy_config_dir() {
    let (Some(old), Some(new)) = (
        ProjectDirs::from("", "", LEGACY_CONFIG_DIR_NAME)
            .map(|dirs| dirs.config_dir().to_path_buf()),
        config_dir(),
    ) else {
        return;
    };

    if move_directory(&old, &new).is_ok_and(|moved| moved) {
        eprintln!("Moved {} to {}.", old.display(), new.display());
    }
}

fn move_directory(old: &Path, new: &Path) -> io::Result<bool> {
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(old, new)?;

    Ok(true)
}

/// A server name becomes a file name, so it may not contain path separators or start with a dot.
pub fn is_server_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The file `servers/<name>.toml` in the config directory.
pub fn server_file(name: &str) -> Option<PathBuf> {
    config_dir().and_then(|dir| server_file_in(&dir, name))
}

/// The file `servers/<name>.toml` below `dir`, if `name` is a valid server name.
pub fn server_file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    is_server_name(name).then(|| dir.join("servers").join(format!("{name}.toml")))
}

/// The interface of a server, which has its own table in the server file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    /// The ZMQ rcon interface, table `[rcon]`.
    Rcon,
    /// The ZMQ stats interface, table `[stats]`.
    Stats,
}

impl Interface {
    const fn table(self) -> &'static str {
        match self {
            Self::Rcon => "rcon",
            Self::Stats => "stats",
        }
    }

    const fn other(self) -> Self {
        match self {
            Self::Rcon => Self::Stats,
            Self::Stats => Self::Rcon,
        }
    }
}

/// How to reach an interface of a server; what the file does not say stays `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The ZMQ URI of the interface.
    pub host: Option<String>,
    /// The password of the interface.
    pub password: Option<String>,
}

/// `endpoint` with its port replaced, `tcp://127.0.0.1:27961` and `27962` give `tcp://127.0.0.1:27962`.
pub fn with_port(endpoint: &str, port: i64) -> String {
    let base = endpoint
        .rsplit_once(':')
        .filter(|(_, last)| last.chars().all(|c| c.is_ascii_digit()))
        .map_or(endpoint, |(base, _)| base);
    format!("{base}:{port}")
}

/// Reads the file; a file that does not exist is an empty document.
pub fn read_document(path: &Path) -> io::Result<DocumentMut> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .parse::<DocumentMut>()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(err) => Err(err),
    }
}

fn table_of(document: &DocumentMut, interface: Interface) -> Option<&Table> {
    document.get(interface.table()).and_then(Item::as_table)
}

/// The value `key` of the table of `interface`; for `[rcon]` the value at the top of an older file counts too.
///
/// A value of another type than `convert` accepts is an error that names the table, the key and what is expected.
fn typed<T>(
    document: &DocumentMut,
    interface: Interface,
    key: &str,
    expected: &str,
    convert: impl Fn(&Item) -> Option<T>,
) -> io::Result<Option<T>> {
    let own = table_of(document, interface).and_then(|table| table.get(key));
    let legacy = (interface == Interface::Rcon)
        .then(|| document.get(key))
        .flatten();

    own.or(legacy)
        .map(|item| {
            convert(item).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("[{}] {key} must be {expected}", interface.table()),
                )
            })
        })
        .transpose()
}

/// The text `key` of the table of `interface`.
pub fn text(document: &DocumentMut, interface: Interface, key: &str) -> io::Result<Option<String>> {
    typed(document, interface, key, "a text", |item| {
        item.as_str().map(str::to_string)
    })
}

/// The switch `key` of the table of `interface`, `true` or `false`.
pub fn flag(document: &DocumentMut, interface: Interface, key: &str) -> io::Result<Option<bool>> {
    typed(document, interface, key, "true or false", Item::as_bool)
}

/// The list of texts `key` of the table of `interface`.
pub fn list(
    document: &DocumentMut,
    interface: Interface,
    key: &str,
) -> io::Result<Option<Vec<String>>> {
    typed(document, interface, key, "a list of texts", |item| {
        item.as_array()?
            .iter()
            .map(|entry| entry.as_str().map(str::to_string))
            .collect()
    })
}

/// The value `key` of the table of `interface` as one of the names of `T`, the case does not matter.
pub fn choice<T: ValueEnum>(
    document: &DocumentMut,
    interface: Interface,
    key: &str,
) -> io::Result<Option<T>> {
    text(document, interface, key)?
        .map(|text| {
            T::from_str(&text, true).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown {key} '{text}' in [{}]", interface.table()),
                )
            })
        })
        .transpose()
}

fn number(document: &DocumentMut, interface: Interface, key: &str) -> io::Result<Option<i64>> {
    typed(document, interface, key, "a number", Item::as_integer)
}

/// The host and password of `interface`; without a `host` the one of the other interface with the own `port` is used.
pub fn endpoint(document: &DocumentMut, interface: Interface) -> io::Result<Endpoint> {
    let host = match text(document, interface, "host")? {
        Some(host) => Some(host),
        None => number(document, interface, "port")?
            .zip(text(document, interface.other(), "host")?)
            .map(|(port, other)| with_port(&other, port)),
    };

    Ok(Endpoint {
        host,
        password: text(document, interface, "password")?,
    })
}

/// The connection of `interface` in the file, see [`endpoint`].
pub fn read_endpoint(path: &Path, interface: Interface) -> io::Result<Endpoint> {
    endpoint(&read_document(path)?, interface)
}

fn adopt_legacy_rcon_keys(document: &mut DocumentMut) {
    let legacy: Vec<(Key, Item)> = LEGACY_RCON_KEYS
        .into_iter()
        .filter_map(|key| document.remove_entry(key))
        .collect();
    if legacy.is_empty() {
        return;
    }

    let mut table = table_of(document, Interface::Rcon)
        .cloned()
        .unwrap_or_default();
    for (key, item) in legacy {
        if !table.contains_key(key.get()) {
            table.insert_formatted(&key, item);
        }
    }
    document[Interface::Rcon.table()] = Item::Table(table);
}

/// Writes the settings that are `Some` into the table of `interface`, readable for the owner only.
///
/// Other content stays, and values at the top of an older file move into `[rcon]`.
pub fn save_settings(
    path: &Path,
    interface: Interface,
    settings: &[(&str, Option<&str>)],
) -> io::Result<()> {
    let mut document = read_document(path)?;
    adopt_legacy_rcon_keys(&mut document);

    let mut table = table_of(&document, interface).cloned().unwrap_or_default();
    for (key, setting) in settings {
        if let Some(setting) = setting {
            table.insert(key, value(*setting));
        }
    }
    document[interface.table()] = Item::Table(table);

    write_private(path, &document.to_string())
}

/// Writes the host and password of `endpoint` into the table of `interface`, see [`save_settings`].
pub fn save_endpoint(path: &Path, interface: Interface, endpoint: &Endpoint) -> io::Result<()> {
    save_settings(
        path,
        interface,
        &[
            ("host", endpoint.host.as_deref()),
            ("password", endpoint.password.as_deref()),
        ],
    )
}

/// Writes `text` to the file, creates its directory and lets only the owner read it.
pub fn write_private(path: &Path, text: &str) -> io::Result<()> {
    use io::Write;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?
            .write_all(text.as_bytes())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        fs::File::create(path)?.write_all(text.as_bytes())
    }
}

/// `true` when other users can read a file that holds a password.
#[cfg(unix)]
pub fn password_is_exposed(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let has_password = read_document(path).is_ok_and(|document| {
        [Interface::Rcon, Interface::Stats]
            .into_iter()
            .filter_map(|interface| text(&document, interface, "password").ok().flatten())
            .any(|password| !password.is_empty())
    });
    has_password && fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o077 != 0)
}

#[cfg(not(unix))]
pub fn password_is_exposed(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod servers_tests {
    use std::fs;

    use super::{
        Endpoint, Interface, choice, flag, is_server_name, list, move_directory,
        password_is_exposed, read_document, read_endpoint, save_endpoint, save_settings,
        server_file_in, text, with_port,
    };

    #[test]
    fn switches_and_lists_are_read_with_their_types() {
        let document = "[stats]\ntimestamp = true\ntypes = [\"A\", \"B\"]\n"
            .parse()
            .expect("the document is valid");

        assert_eq!(
            flag(&document, Interface::Stats, "timestamp").expect("a switch"),
            Some(true)
        );
        assert_eq!(
            list(&document, Interface::Stats, "types").expect("a list"),
            Some(vec!["A".to_string(), "B".to_string()])
        );
        assert_eq!(
            flag(&document, Interface::Stats, "missing").expect("nothing is set"),
            None
        );
        assert_eq!(
            flag(&document, Interface::Rcon, "timestamp").expect("the table is missing"),
            None
        );
    }

    #[test]
    fn a_choice_is_read_by_name_and_an_unknown_one_is_reported() {
        use crate::ColorChoice;

        let document = "[stats]\ncolor = \"Never\"\n[rcon]\ncolor = \"rainbow\"\n"
            .parse()
            .expect("the document is valid");

        assert_eq!(
            choice::<ColorChoice>(&document, Interface::Stats, "color").expect("a known name"),
            Some(ColorChoice::Never)
        );
        assert_eq!(
            choice::<ColorChoice>(&document, Interface::Stats, "missing").expect("nothing is set"),
            None
        );
        assert_eq!(
            choice::<ColorChoice>(&document, Interface::Rcon, "color")
                .expect_err("an unknown name")
                .to_string(),
            "unknown color 'rainbow' in [rcon]"
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_names_the_table_and_the_key() {
        let document = "[stats]\ntimestamp = \"yes\"\ntypes = [1]\nhost = 3\nport = \"x\"\n"
            .parse()
            .expect("the document is valid");

        for message in [
            flag(&document, Interface::Stats, "timestamp")
                .expect_err("not a switch")
                .to_string(),
            list(&document, Interface::Stats, "types")
                .expect_err("not a list of texts")
                .to_string(),
            text(&document, Interface::Stats, "host")
                .expect_err("not a text")
                .to_string(),
            super::endpoint(&document, Interface::Stats)
                .expect_err("the host is not a text")
                .to_string(),
        ] {
            assert!(message.starts_with("[stats] "), "{message}");
        }
    }

    fn endpoint(host: Option<&str>, password: Option<&str>) -> Endpoint {
        Endpoint {
            host: host.map(str::to_string),
            password: password.map(str::to_string),
        }
    }

    #[test]
    fn server_names_are_plain_file_names() {
        assert!(is_server_name("crimson-arena_2.test"));
        assert!(!is_server_name(""));
        assert!(!is_server_name(".hidden"));
        assert!(!is_server_name("a/b"));
    }

    #[test]
    fn a_server_file_lies_in_the_servers_directory_below_the_given_one() {
        let dir = std::path::Path::new("/config");

        assert_eq!(
            server_file_in(dir, "crimson-arena"),
            Some(std::path::PathBuf::from(
                "/config/servers/crimson-arena.toml"
            ))
        );
        assert_eq!(server_file_in(dir, "../crimson-arena"), None);
    }

    #[test]
    fn with_port_replaces_the_port_of_an_endpoint() {
        assert_eq!(
            with_port("tcp://192.0.2.1:27961", 27962),
            "tcp://192.0.2.1:27962"
        );
        assert_eq!(
            with_port("tcp://arena.invalid", 27962),
            "tcp://arena.invalid:27962"
        );
    }

    #[test]
    fn each_interface_reads_its_own_table() {
        let dir = tempdir("tables");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "[rcon]\nhost = \"tcp://192.0.2.1:28960\"\npassword = \"rcon\"\n[stats]\nhost = \"tcp://192.0.2.1:27961\"\npassword = \"stats\"\n",
        )
        .expect("the file can be written");

        assert_eq!(
            read_endpoint(&path, Interface::Rcon).expect("the file is valid"),
            endpoint(Some("tcp://192.0.2.1:28960"), Some("rcon"))
        );
        assert_eq!(
            read_endpoint(&path, Interface::Stats).expect("the file is valid"),
            endpoint(Some("tcp://192.0.2.1:27961"), Some("stats"))
        );
    }

    #[test]
    fn the_sample_server_file_is_valid_and_says_what_it_documents() {
        let dir = tempdir("sample");
        let path = dir.join("crimson-arena.toml");
        fs::write(&path, include_str!("../servers.example.toml")).expect("writable");

        assert_eq!(
            read_endpoint(&path, Interface::Rcon).expect("the sample is valid"),
            endpoint(Some("tcp://192.0.2.1:28960"), Some("change-me-rcon"))
        );
        assert_eq!(
            read_endpoint(&path, Interface::Stats).expect("the sample is valid"),
            endpoint(Some("tcp://192.0.2.1:27960"), Some("change-me-stats"))
        );
    }

    #[test]
    fn a_port_goes_with_the_host_of_the_other_interface() {
        let dir = tempdir("port");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "[rcon]\nhost = \"tcp://192.0.2.1:28960\"\npassword = \"rcon\"\n[stats]\nport = 27962\npassword = \"stats\"\n",
        )
        .expect("the file can be written");

        assert_eq!(
            read_endpoint(&path, Interface::Stats).expect("the file is valid"),
            endpoint(Some("tcp://192.0.2.1:27962"), Some("stats"))
        );

        fs::write(
            &path,
            "[stats]\nhost = \"tcp://192.0.2.1:27961\"\n[rcon]\nport = 28960\n",
        )
        .expect("the file can be written");

        assert_eq!(
            read_endpoint(&path, Interface::Rcon)
                .expect("the file is valid")
                .host
                .as_deref(),
            Some("tcp://192.0.2.1:28960")
        );
    }

    #[test]
    fn a_host_wins_over_the_port() {
        let dir = tempdir("host");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "[rcon]\nhost = \"tcp://192.0.2.1:28960\"\n[stats]\nhost = \"tcp://192.0.2.9:30000\"\nport = 27962\n",
        )
        .expect("the file can be written");

        let stats = read_endpoint(&path, Interface::Stats).expect("the file is valid");

        assert_eq!(stats.host.as_deref(), Some("tcp://192.0.2.9:30000"));
        assert_eq!(stats.password, None);
    }

    #[test]
    fn a_file_without_the_table_gives_nothing() {
        let dir = tempdir("none");
        let path = dir.join("arena.toml");
        fs::write(&path, "[rcon]\nhost = \"tcp://192.0.2.1:28960\"\n").expect("writable");

        assert_eq!(
            read_endpoint(&path, Interface::Stats).expect("the file is valid"),
            Endpoint::default()
        );
        assert_eq!(
            read_endpoint(&dir.join("missing.toml"), Interface::Rcon).expect("no file is fine"),
            Endpoint::default()
        );
    }

    #[test]
    fn values_at_the_top_of_an_older_file_count_for_rcon_only() {
        let dir = tempdir("legacy_read");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "host = \"tcp://192.0.2.1:28960\"\nidentity = \"tester\"\n[stats]\nport = 27961\n",
        )
        .expect("the file can be written");
        let document = read_document(&path).expect("the file is valid");

        assert_eq!(
            text(&document, Interface::Rcon, "identity")
                .expect("the value is a text")
                .as_deref(),
            Some("tester")
        );
        assert_eq!(
            text(&document, Interface::Stats, "identity").expect("nothing is set"),
            None
        );
        assert_eq!(
            read_endpoint(&path, Interface::Stats)
                .expect("the file is valid")
                .host
                .as_deref(),
            Some("tcp://192.0.2.1:27961")
        );
    }

    #[test]
    fn saving_moves_the_values_of_an_older_file_into_rcon() {
        let dir = tempdir("legacy_save");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "# my server\nhost = \"tcp://192.0.2.1:28960\"\npassword = \"rcon\"\n[completions]\nnames = [\"kick\"]\n",
        )
        .expect("writable");

        save_settings(&path, Interface::Rcon, &[("color", Some("never"))])
            .expect("the file can be saved");

        let text = fs::read_to_string(&path).expect("readable");
        assert!(text.contains("# my server"), "{text}");
        assert!(text.contains("[completions]"), "{text}");
        let document = read_document(&path).expect("the file is valid");
        assert_eq!(
            document["rcon"]["host"].as_str(),
            Some("tcp://192.0.2.1:28960")
        );
        assert_eq!(document["rcon"]["password"].as_str(), Some("rcon"));
        assert_eq!(document["rcon"]["color"].as_str(), Some("never"));
        assert!(document.get("host").is_none());
        assert!(document.get("password").is_none());
    }

    #[test]
    fn saving_keeps_the_rest_of_the_file_and_is_private() {
        let dir = tempdir("save");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "# my server\n[rcon]\nhost = \"tcp://192.0.2.1:28960\"\n",
        )
        .expect("writable");

        save_endpoint(
            &path,
            Interface::Stats,
            &endpoint(Some("tcp://192.0.2.1:27962"), Some("stats")),
        )
        .expect("the file can be saved");

        let text = fs::read_to_string(&path).expect("readable");
        assert!(text.contains("# my server"));
        assert!(text.contains("host = \"tcp://192.0.2.1:28960\""));
        assert!(text.contains("[stats]"));
        assert!(!password_is_exposed(&path));
        assert_eq!(
            read_endpoint(&path, Interface::Stats)
                .expect("the file is valid")
                .host
                .as_deref(),
            Some("tcp://192.0.2.1:27962")
        );
    }

    #[test]
    fn a_password_in_a_readable_file_is_exposed() {
        let dir = tempdir("exposed");
        for (name, text) in [
            ("stats.toml", "[stats]\npassword = \"stats\"\n"),
            ("rcon.toml", "[rcon]\npassword = \"rcon\"\n"),
            ("legacy.toml", "password = \"rcon\"\n"),
        ] {
            let path = dir.join(name);
            fs::write(&path, text).expect("writable");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
                assert!(password_is_exposed(&path), "{name}");
            }
        }
    }

    #[test]
    fn the_config_directory_of_earlier_versions_moves_once() {
        let base = tempdir("move");
        let old = base.join("ql_rcon");
        let new = base.join("config").join("ql_zmq");
        fs::create_dir_all(old.join("servers")).expect("the directory can be created");
        fs::write(old.join("servers").join("arena.toml"), "x").expect("writable");

        assert!(move_directory(&old, &new).expect("the directory moves"));
        assert!(new.join("servers").join("arena.toml").is_file());
        assert!(!old.exists());
        assert!(!move_directory(&old, &new).expect("nothing is left to move"));
    }

    #[test]
    fn an_existing_config_directory_is_not_replaced() {
        let base = tempdir("keep");
        let old = base.join("ql_rcon");
        let new = base.join("ql_zmq");
        fs::create_dir_all(&old).expect("the directory can be created");
        fs::create_dir_all(&new).expect("the directory can be created");

        assert!(!move_directory(&old, &new).expect("nothing to do"));
        assert!(old.is_dir());
    }

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ql_zmq_servers_{name}_{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).expect("a temp dir can be created");
        dir
    }
}
