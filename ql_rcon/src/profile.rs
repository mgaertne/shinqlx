//! The server files `servers/<name>.toml` of the config directory: how to connect to a server and what the server has
//! reported so far.
use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(test)]
pub(crate) use ql_zmq::servers::is_server_name;
pub(crate) use ql_zmq::servers::{config_dir, server_file, server_file_in};
use ql_zmq::{
    ColorChoice,
    servers::{
        Endpoint, Interface, choice, endpoint, flag, password_is_exposed, read_document,
        save_settings, text, write_private,
    },
};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::{fetch::Fetched, learn::Learned};

/// The settings for the connection; every value that is not in the file stays `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Connection {
    pub(crate) host: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) identity: Option<String>,
    pub(crate) color: Option<ColorChoice>,
    pub(crate) fetch_completions: Option<bool>,
    pub(crate) timestamp: Option<bool>,
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

impl ServerFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn document(&self) -> io::Result<DocumentMut> {
        read_document(&self.path)
    }

    fn update(&self, change: impl FnOnce(&mut DocumentMut)) -> io::Result<()> {
        let mut document = self.document()?;
        change(&mut document);
        write_private(&self.path, &document.to_string())
    }

    /// `true` when other users can read a file that holds a password.
    pub(crate) fn password_is_exposed(&self) -> bool {
        password_is_exposed(&self.path)
    }

    pub(crate) fn connection(&self) -> io::Result<Connection> {
        let document = self.document()?;
        let Endpoint { host, password } = endpoint(&document, Interface::Rcon)?;

        Ok(Connection {
            host,
            password,
            identity: text(&document, Interface::Rcon, "identity")?,
            color: choice(&document, Interface::Rcon, "color")?,
            fetch_completions: flag(&document, Interface::Rcon, "fetch_completions")?,
            timestamp: flag(&document, Interface::Rcon, "timestamp")?,
        })
    }

    pub(crate) fn save_connection(&self, connection: &Connection) -> io::Result<()> {
        save_settings(
            &self.path,
            Interface::Rcon,
            &[
                ("host", connection.host.as_deref()),
                ("password", connection.password.as_deref()),
                ("identity", connection.identity.as_deref()),
                ("color", connection.color.map(ColorChoice::name)),
            ],
        )
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

    pub(crate) fn plugin_commands(&self) -> Vec<String> {
        self.completions("plugin_commands")
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
            merge_list(completions, "plugin_commands", &fetched.plugin_commands);
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

    use ql_zmq::ColorChoice;

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
            color: Some(ColorChoice::Never),
            ..Connection::default()
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
            plugin_commands: Vec::new(),
            flags: Vec::new(),
        })
        .expect("the file is written");
        file.merge_fetched(&Fetched {
            names: vec!["map".to_string(), "addbot".to_string()],
            maps: Vec::new(),
            plugin_commands: Vec::new(),
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
            plugin_commands: Vec::new(),
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
    fn an_unknown_color_in_the_server_file_is_an_error() {
        let path = scratch_file();
        fs::create_dir_all(path.parent().expect("the file has a directory"))
            .expect("the directory is created");
        fs::write(&path, "[rcon]\ncolor = \"rainbow\"\n").expect("the file is written");
        let file = ServerFile::new(path);

        let message = file
            .connection()
            .expect_err("the color is unknown")
            .to_string();

        assert_eq!(message, "unknown color 'rainbow' in [rcon]");
        cleanup(&file);
    }

    #[test]
    fn the_connection_lies_in_the_table_rcon() {
        let file = ServerFile::new(scratch_file());

        file.save_connection(&Connection {
            host: Some("tcp://192.0.2.1:28960".to_string()),
            identity: Some("tester".to_string()),
            ..Connection::default()
        })
        .expect("the file is written");

        let text = fs::read_to_string(file.path()).expect("the file is read");
        assert!(text.starts_with("[rcon]\n"), "{text}");
        assert!(text.contains("identity = \"tester\""), "{text}");
        cleanup(&file);
    }

    #[test]
    fn saving_the_color_keeps_the_rest_of_the_file() {
        let path = scratch_file();
        fs::create_dir_all(path.parent().expect("the file has a directory"))
            .expect("the directory is created");
        fs::write(
            &path,
            "# my server\n[rcon]\nhost = \"tcp://192.0.2.1:28960\"\npassword = \"secret\"\n[stats]\nport = 27961\n",
        )
        .expect("the file is written");
        let file = ServerFile::new(path);

        file.save_connection(&Connection {
            color: Some(ColorChoice::Never),
            ..Connection::default()
        })
        .expect("the file is written");

        let saved = file.connection().expect("the file is read");
        assert_eq!(saved.host.as_deref(), Some("tcp://192.0.2.1:28960"));
        assert_eq!(saved.password.as_deref(), Some("secret"));
        assert_eq!(saved.color, Some(ColorChoice::Never));
        let text = fs::read_to_string(file.path()).expect("the file is read");
        assert!(text.starts_with("# my server"), "{text}");
        assert!(text.contains("[stats]"), "{text}");
        cleanup(&file);
    }

    #[test]
    fn the_rcon_host_can_come_from_the_stats_host_and_the_rcon_port() {
        let path = scratch_file();
        fs::create_dir_all(path.parent().expect("the file has a directory"))
            .expect("the directory is created");
        fs::write(
            &path,
            "[stats]\nhost = \"tcp://192.0.2.1:27961\"\n[rcon]\nport = 28960\n",
        )
        .expect("the file is written");
        let file = ServerFile::new(path);

        assert_eq!(
            file.connection().expect("the file is read").host.as_deref(),
            Some("tcp://192.0.2.1:28960")
        );
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
