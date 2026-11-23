//! The server files `servers/<name>.toml` of the config directory, which `ql_rcon` and `ql_stats` read.
//!
//! The values at the top of a file are those of the rcon connection. The table `[stats]` holds the stats connection,
//! because the two ports and passwords usually differ.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use toml_edit::{DocumentMut, Item, value};

/// The config directory, `~/.config/ql_rcon/` on Linux.
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "ql_rcon").map(|dirs| dirs.config_dir().to_path_buf())
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
    is_server_name(name)
        .then(config_dir)
        .flatten()
        .map(|dir| dir.join("servers").join(format!("{name}.toml")))
}

/// How to reach the stats interface of a server; what the file does not say stays `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StatsEndpoint {
    /// The ZMQ URI of the stats interface.
    pub host: Option<String>,
    /// The `zmq_stats_password` of the server.
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

fn document(path: &Path) -> io::Result<DocumentMut> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .parse::<DocumentMut>()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(err) => Err(err),
    }
}

/// The stats connection of a server file: `[stats] host` or, with `[stats] port`, the rcon host at that port.
pub fn stats_endpoint(path: &Path) -> io::Result<StatsEndpoint> {
    let document = document(path)?;
    let stats = document.get("stats").and_then(Item::as_table);
    let text = |table: Option<&toml_edit::Table>, key: &str| {
        table
            .and_then(|table| table.get(key))
            .and_then(Item::as_str)
            .map(str::to_string)
    };
    let port = stats
        .and_then(|table| table.get("port"))
        .and_then(Item::as_integer);
    let host = text(stats, "host").or_else(|| {
        let rcon_host = text(Some(document.as_table()), "host")?;
        port.map(|port| with_port(&rcon_host, port))
    });

    Ok(StatsEndpoint {
        host,
        password: text(stats, "password"),
    })
}

/// Writes the stats connection into `[stats]` of the file, readable for the owner only. Other content stays.
pub fn save_stats_endpoint(path: &Path, endpoint: &StatsEndpoint) -> io::Result<()> {
    let mut document = document(path)?;
    let mut table = document
        .get("stats")
        .and_then(Item::as_table)
        .cloned()
        .unwrap_or_default();
    for (key, setting) in [("host", &endpoint.host), ("password", &endpoint.password)] {
        if let Some(setting) = setting {
            table.insert(key, value(setting.as_str()));
        }
    }
    document["stats"] = Item::Table(table);
    write_private(path, &document.to_string())
}

fn write_private(path: &Path, text: &str) -> io::Result<()> {
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

    let has_password = document(path).is_ok_and(|document| {
        let top = document.get("password").and_then(Item::as_str);
        let stats = document
            .get("stats")
            .and_then(Item::as_table)
            .and_then(|table| table.get("password"))
            .and_then(Item::as_str);
        [top, stats]
            .into_iter()
            .flatten()
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
        StatsEndpoint, is_server_name, password_is_exposed, save_stats_endpoint, stats_endpoint,
        with_port,
    };

    #[test]
    fn server_names_are_plain_file_names() {
        assert!(is_server_name("crimson-arena_2.test"));
        assert!(!is_server_name(""));
        assert!(!is_server_name(".hidden"));
        assert!(!is_server_name("a/b"));
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
    fn the_stats_port_goes_with_the_rcon_host() {
        let dir = tempdir("port");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "host = \"tcp://192.0.2.1:27961\"\npassword = \"rcon\"\n[stats]\nport = 27962\npassword = \"stats\"\n",
        )
        .expect("the file can be written");

        assert_eq!(
            stats_endpoint(&path).expect("the file is valid"),
            StatsEndpoint {
                host: Some("tcp://192.0.2.1:27962".to_string()),
                password: Some("stats".to_string()),
            }
        );
    }

    #[test]
    fn a_stats_host_wins_over_the_port() {
        let dir = tempdir("host");
        let path = dir.join("arena.toml");
        fs::write(
            &path,
            "host = \"tcp://192.0.2.1:27961\"\n[stats]\nhost = \"tcp://192.0.2.9:30000\"\nport = 27962\n",
        )
        .expect("the file can be written");

        let endpoint = stats_endpoint(&path).expect("the file is valid");

        assert_eq!(endpoint.host.as_deref(), Some("tcp://192.0.2.9:30000"));
        assert_eq!(endpoint.password, None);
    }

    #[test]
    fn a_file_without_stats_gives_nothing() {
        let dir = tempdir("none");
        let path = dir.join("arena.toml");
        fs::write(&path, "host = \"tcp://192.0.2.1:27961\"\n").expect("the file can be written");

        assert_eq!(
            stats_endpoint(&path).expect("the file is valid"),
            StatsEndpoint::default()
        );
    }

    #[test]
    fn saving_keeps_the_rest_of_the_file_and_is_private() {
        let dir = tempdir("save");
        let path = dir.join("arena.toml");
        fs::write(&path, "# my server\nhost = \"tcp://192.0.2.1:27961\"\n").expect("writable");

        save_stats_endpoint(
            &path,
            &StatsEndpoint {
                host: Some("tcp://192.0.2.1:27962".to_string()),
                password: Some("stats".to_string()),
            },
        )
        .expect("the file can be saved");

        let text = fs::read_to_string(&path).expect("readable");
        assert!(text.contains("# my server"));
        assert!(text.contains("host = \"tcp://192.0.2.1:27961\""));
        assert!(text.contains("[stats]"));
        assert!(!password_is_exposed(&path));
        assert_eq!(
            stats_endpoint(&path)
                .expect("the file is valid")
                .host
                .as_deref(),
            Some("tcp://192.0.2.1:27962")
        );
    }

    #[test]
    fn a_stats_password_in_a_readable_file_is_exposed() {
        let dir = tempdir("exposed");
        let path = dir.join("arena.toml");
        fs::write(&path, "[stats]\npassword = \"stats\"\n").expect("writable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
            assert!(password_is_exposed(&path));
        }
    }

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ql_zmq_servers_{name}_{}", std::process::id()));
        fs::create_dir_all(&dir).expect("a temp dir can be created");
        dir
    }
}
