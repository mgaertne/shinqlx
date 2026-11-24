//! The server files `servers/<name>.toml` of the config directory: how to connect to a server and what the server has
//! reported so far.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(test)]
pub(crate) use ql_zmq::servers::is_server_name;
pub(crate) use ql_zmq::servers::{config_dir, server_file, server_file_in};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::{fetch::Fetched, learn::Learned};

/// The settings for the connection; every value that is not in the file stays `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Connection {
    pub(crate) host: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) identity: Option<String>,
    pub(crate) color: Option<String>,
}

/// What the server reported for a cvar: default, minimum and maximum.
pub(crate) type CvarRange = (String, String, String);

pub(crate) struct ServerFile {
    path: PathBuf,
}

fn implicit_table() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

fn table_mut<'a>(parent: &'a mut Table, key: &str) -> &'a mut Table {
    let item = parent.entry(key).or_insert_with(implicit_table);
    if !item.is_table() {
        *item = implicit_table();
    }
    let Item::Table(table) = item else {
        unreachable!("the item was replaced by a table above")
    };
    table
}

fn strings(table: Option<&Table>, key: &str) -> Vec<String> {
    table
        .and_then(|table| table.get(key))
        .and_then(Item::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn string_array(values: &[String]) -> Array {
    let mut array: Array = values.iter().map(String::as_str).collect();
    for entry in array.iter_mut() {
        entry.decor_mut().set_prefix("\n    ");
    }
    array.set_trailing("\n");
    array.set_trailing_comma(true);
    array
}

fn sorted_unique(mut names: Vec<String>) -> Vec<String> {
    names.sort_unstable_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

fn merge_list(table: &mut Table, key: &str, found: &[String]) {
    let mut names = strings(Some(table), key);
    names.extend_from_slice(found);
    table.insert(key, value(string_array(&sorted_unique(names))));
}

fn write_private(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?
            .write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        fs::File::create(path)?.write_all(text.as_bytes())
    }
}

impl ServerFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn document(&self) -> io::Result<DocumentMut> {
        match fs::read_to_string(&self.path) {
            Ok(text) => text
                .parse::<DocumentMut>()
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
            Err(err) => Err(err),
        }
    }

    fn update(&self, change: impl FnOnce(&mut DocumentMut)) -> io::Result<()> {
        let mut document = self.document()?;
        change(&mut document);
        write_private(&self.path, &document.to_string())
    }

    /// `true` when other users can read a file that holds a password.
    #[cfg(unix)]
    pub(crate) fn password_is_exposed(&self) -> bool {
        use std::os::unix::fs::PermissionsExt;

        let has_password = self.connection().is_ok_and(|connection| {
            connection
                .password
                .is_some_and(|password| !password.is_empty())
        });
        has_password
            && fs::metadata(&self.path).is_ok_and(|meta| meta.permissions().mode() & 0o077 != 0)
    }

    #[cfg(not(unix))]
    pub(crate) fn password_is_exposed(&self) -> bool {
        false
    }

    pub(crate) fn connection(&self) -> io::Result<Connection> {
        let document = self.document()?;
        let text = |key: &str| document.get(key).and_then(Item::as_str).map(str::to_string);

        Ok(Connection {
            host: text("host"),
            password: text("password"),
            identity: text("identity"),
            color: text("color"),
        })
    }

    pub(crate) fn save_connection(&self, connection: &Connection) -> io::Result<()> {
        self.update(|document| {
            for (key, setting) in [
                ("host", &connection.host),
                ("password", &connection.password),
                ("identity", &connection.identity),
                ("color", &connection.color),
            ] {
                if let Some(setting) = setting {
                    document[key] = value(setting.as_str());
                }
            }
        })
    }

    fn completions(&self, key: &str) -> Vec<String> {
        self.document()
            .map(|document| strings(document.get("completions").and_then(Item::as_table), key))
            .unwrap_or_default()
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.completions("names")
    }

    pub(crate) fn maps(&self) -> Vec<String> {
        self.completions("maps")
    }

    pub(crate) fn factories(&self) -> Vec<String> {
        self.completions("factories")
    }

    pub(crate) fn latched(&self) -> Vec<String> {
        self.completions("latched")
    }

    pub(crate) fn read_only(&self) -> Vec<String> {
        self.completions("read_only")
    }

    pub(crate) fn usages(&self) -> Vec<(String, String)> {
        let Ok(document) = self.document() else {
            return Vec::new();
        };

        document
            .get("usages")
            .and_then(Item::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(command, text)| {
                        text.as_str()
                            .map(|text| (command.to_string(), text.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn cvars(&self) -> Vec<(String, CvarRange)> {
        let Ok(document) = self.document() else {
            return Vec::new();
        };

        document
            .get("cvars")
            .and_then(Item::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(name, cvar)| {
                        let cvar = cvar.as_table()?;
                        let text = |key: &str| {
                            cvar.get(key)
                                .and_then(Item::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        Some((
                            name.to_string(),
                            (text("default"), text("minimum"), text("maximum")),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Adds the names that the server listed to the ones the file already holds.
    pub(crate) fn merge_fetched(&self, fetched: &Fetched) -> io::Result<()> {
        self.update(|document| {
            let completions = table_mut(document.as_table_mut(), "completions");
            merge_list(completions, "names", &fetched.names);
            merge_list(completions, "maps", &fetched.maps);
            if !fetched.flags.is_empty() {
                let flags = table_mut(document.as_table_mut(), "flags");
                for (name, letters) in &fetched.flags {
                    flags.insert(&name.to_lowercase(), value(letters.as_str()));
                }
            }
        })
    }

    /// The flag letters of the cvars that `cvarlist` reported, for example `("sv_hostname", "SA")`.
    pub(crate) fn cvar_flags(&self) -> Vec<(String, String)> {
        let Ok(document) = self.document() else {
            return Vec::new();
        };

        document
            .get("flags")
            .and_then(Item::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(name, letters)| {
                        letters
                            .as_str()
                            .map(|letters| (name.to_string(), letters.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Keeps what the server reported on its own; a newer report replaces the older one of the same entry.
    pub(crate) fn remember(&self, learned: &Learned) -> io::Result<()> {
        self.update(|document| {
            let root = document.as_table_mut();
            match learned {
                Learned::Usage { command, text } => {
                    table_mut(root, "usages").insert(command, value(text.as_str()));
                }
                Learned::Factories(factories) => {
                    table_mut(root, "completions").insert(
                        "factories",
                        value(string_array(&sorted_unique(factories.clone()))),
                    );
                }
                Learned::Latched(name) => {
                    merge_list(
                        table_mut(root, "completions"),
                        "latched",
                        &[name.to_lowercase()],
                    );
                }
                Learned::ReadOnly(name) => {
                    merge_list(
                        table_mut(root, "completions"),
                        "read_only",
                        &[name.to_lowercase()],
                    );
                }
                Learned::Cvar {
                    name,
                    default,
                    minimum,
                    maximum,
                } => {
                    let cvar = table_mut(table_mut(root, "cvars"), &name.to_lowercase());
                    cvar.insert("default", value(default.as_str()));
                    cvar.insert("minimum", value(minimum.as_str()));
                    cvar.insert("maximum", value(maximum.as_str()));
                }
            }
        })
    }
}

#[cfg(test)]
mod profile_tests {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::{fs, path::PathBuf};

    use super::{Connection, ServerFile, is_server_name};
    use crate::{fetch::Fetched, learn::Learned};

    fn scratch_file() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir()
            .join(format!(
                "ql_rcon_profile_test_{}_{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ))
            .join("servers")
            .join("crimson-arena.toml")
    }

    fn cleanup(file: &ServerFile) {
        if let Some(dir) = file.path().parent().and_then(|dir| dir.parent()) {
            fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn server_names_are_plain_file_names() {
        assert!(is_server_name("crimson-arena"));
        assert!(is_server_name("bus-station_2.test"));
        assert!(!is_server_name(""));
        assert!(!is_server_name(".hidden"));
        assert!(!is_server_name("../secret"));
        assert!(!is_server_name("a/b"));
    }

    #[test]
    fn a_missing_file_has_no_settings_and_no_names() {
        let file = ServerFile::new(scratch_file());

        assert_eq!(
            file.connection().expect("no file is fine"),
            Connection::default()
        );
        assert!(file.names().is_empty());
        assert!(file.cvars().is_empty());
    }

    #[test]
    fn the_connection_is_saved_and_read_back() {
        let file = ServerFile::new(scratch_file());
        let connection = Connection {
            host: Some("tcp://192.0.2.1:27961".to_string()),
            password: Some("secret".to_string()),
            identity: None,
            color: Some("never".to_string()),
        };

        file.save_connection(&connection)
            .expect("the file is written");

        assert_eq!(file.connection().expect("the file is read"), connection);
        cleanup(&file);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_is_private_and_a_readable_one_is_reported() {
        use std::os::unix::fs::PermissionsExt;

        let file = ServerFile::new(scratch_file());
        file.save_connection(&Connection {
            password: Some("secret".to_string()),
            ..Connection::default()
        })
        .expect("the file is written");

        assert!(!file.password_is_exposed());
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o644))
            .expect("the mode can be changed");
        assert!(file.password_is_exposed());
        cleanup(&file);
    }

    #[test]
    fn fetched_names_are_merged_sorted_and_unique() {
        let file = ServerFile::new(scratch_file());
        file.merge_fetched(&Fetched {
            names: vec!["kick".to_string(), "Map".to_string()],
            maps: vec!["zen".to_string()],
            flags: Vec::new(),
        })
        .expect("the file is written");
        file.merge_fetched(&Fetched {
            names: vec!["map".to_string(), "addbot".to_string()],
            maps: Vec::new(),
            flags: Vec::new(),
        })
        .expect("the file is written");

        assert_eq!(file.names(), ["addbot", "kick", "Map"]);
        assert_eq!(file.maps(), ["zen"]);
        cleanup(&file);
    }

    #[test]
    fn the_flags_of_the_cvars_are_kept_per_lowercase_name() {
        let file = ServerFile::new(scratch_file());
        file.merge_fetched(&Fetched {
            names: Vec::new(),
            maps: Vec::new(),
            flags: vec![("SV_Hostname".to_string(), "SA".to_string())],
        })
        .expect("the file is written");

        assert_eq!(
            file.cvar_flags(),
            [("sv_hostname".to_string(), "SA".to_string())]
        );
        cleanup(&file);
    }

    #[test]
    fn what_the_server_reports_is_remembered_in_its_tables() {
        let file = ServerFile::new(scratch_file());
        for learned in [
            Learned::Usage {
                command: "kick".to_string(),
                text: "kick <player>".to_string(),
            },
            Learned::Cvar {
                name: "SV_MaxClients".to_string(),
                default: "8".to_string(),
                minimum: "0".to_string(),
                maximum: "64".to_string(),
            },
            Learned::Factories(vec!["ctf".to_string(), "ca".to_string()]),
            Learned::Latched("bot_enable".to_string()),
            Learned::ReadOnly("sv_cheats".to_string()),
        ] {
            file.remember(&learned).expect("the file is written");
        }

        assert_eq!(
            file.usages(),
            [("kick".to_string(), "kick <player>".to_string())]
        );
        assert_eq!(
            file.cvars(),
            [(
                "sv_maxclients".to_string(),
                ("8".to_string(), "0".to_string(), "64".to_string())
            )]
        );
        assert_eq!(file.factories(), ["ca", "ctf"]);
        assert_eq!(file.latched(), ["bot_enable"]);
        assert_eq!(file.read_only(), ["sv_cheats"]);
        cleanup(&file);
    }

    #[test]
    fn comments_and_settings_survive_an_update() {
        let path = scratch_file();
        fs::create_dir_all(path.parent().expect("the file has a directory"))
            .expect("the directory is created");
        fs::write(
            &path,
            "# my crimson-arena server\nhost = \"tcp://192.0.2.1:27961\"\n",
        )
        .expect("the file is written");
        let file = ServerFile::new(path);

        file.remember(&Learned::Latched("bot_enable".to_string()))
            .expect("the file is written");

        let text = fs::read_to_string(file.path()).expect("the file is read");
        assert!(
            text.starts_with("# my crimson-arena server\nhost = "),
            "{text}"
        );
        cleanup(&file);
    }
}
