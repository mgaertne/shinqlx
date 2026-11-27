use alloc::string::String;
use core::time::Duration;
use std::path::PathBuf;

use super::{lifecycle::Rules, storage::Limits};

const MEBIBYTE: u64 = 1024 * 1024;

/// What the server cvars say about recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// When to record.
    pub rules: Rules,
    /// Where recordings are stored.
    pub directory: PathBuf,
    /// How long recordings are kept.
    pub limits: Limits,
}

impl Settings {
    /// Reads the settings with `cvar`, which returns the value of a cvar. Unset or invalid cvars mean
    /// their default, a limit of `0` means no limit.
    pub fn from_cvars(cvar: impl Fn(&str) -> Option<String>) -> Self {
        let number = |name: &str, default: u64| {
            cvar(name)
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(default)
        };
        let limit = |name, default, unit: u64| {
            Some(number(name, default))
                .filter(|&value| value > 0)
                .map(|value| value.saturating_mul(unit))
        };
        let directory = cvar("qlx_demoDir")
            .filter(|directory| !directory.is_empty())
            .map_or_else(
                || PathBuf::from(cvar("fs_homepath").unwrap_or_default()).join("demos"),
                PathBuf::from,
            );
        Self {
            rules: Rules {
                enabled: number("qlx_demoRecord", 0) != 0,
                require_human: number("qlx_demoRequireHuman", 1) != 0,
                max_length: limit("qlx_demoMaxMinutes", 90, 60).map(Duration::from_secs),
            },
            directory,
            limits: Limits {
                max_age: limit("qlx_demoKeepUnrequestedHours", 24, 3600).map(Duration::from_secs),
                max_requested_age: limit("qlx_demoKeepRequestedDays", 30, 86400)
                    .map(Duration::from_secs),
                max_total_bytes: limit("qlx_demoMaxTotalMB", 5120, MEBIBYTE),
                min_free_bytes: limit("qlx_demoMinFreeMB", 1024, MEBIBYTE),
            },
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use alloc::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;

    fn settings(cvars: &[(&str, &str)]) -> Settings {
        let cvars: BTreeMap<_, _> = cvars.iter().copied().collect();
        Settings::from_cvars(|name| cvars.get(name).map(|&value| value.into()))
    }

    #[test]
    fn without_cvars_nothing_is_recorded_and_the_defaults_apply() {
        let settings = settings(&[("fs_homepath", "/srv/quake")]);

        assert_eq!(
            settings,
            Settings {
                rules: Rules {
                    enabled: false,
                    require_human: true,
                    max_length: Some(Duration::from_secs(90 * 60)),
                },
                directory: "/srv/quake/demos".into(),
                limits: Limits {
                    max_age: Some(Duration::from_secs(24 * 3600)),
                    max_requested_age: Some(Duration::from_secs(30 * 86400)),
                    max_total_bytes: Some(5120 * MEBIBYTE),
                    min_free_bytes: Some(1024 * MEBIBYTE),
                },
            }
        );
    }

    #[test]
    fn cvars_replace_the_defaults() {
        let settings = settings(&[
            ("qlx_demoRecord", "1"),
            ("qlx_demoRequireHuman", "0"),
            ("qlx_demoMaxMinutes", "0"),
            ("qlx_demoDir", "/var/demos"),
            ("qlx_demoKeepUnrequestedHours", "2"),
            ("qlx_demoKeepRequestedDays", "3"),
            ("qlx_demoMaxTotalMB", "0"),
            ("qlx_demoMinFreeMB", "-5"),
        ]);

        assert!(settings.rules.enabled);
        assert!(!settings.rules.require_human);
        assert_eq!(settings.rules.max_length, None);
        assert_eq!(settings.directory, PathBuf::from("/var/demos"));
        assert_eq!(settings.limits.max_age, Some(Duration::from_secs(7200)));
        assert_eq!(
            settings.limits.max_requested_age,
            Some(Duration::from_secs(3 * 86400))
        );
        assert_eq!(settings.limits.max_total_bytes, None);
        assert_eq!(settings.limits.min_free_bytes, Some(1024 * MEBIBYTE));
    }
}
