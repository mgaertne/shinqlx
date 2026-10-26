use pyo3::{intern, prelude::*, types::PyTuple};

use super::register_command;
use crate::ffi::python::{
    NonexistentPlayerError, Plugin, PythonReturnCodes, plugin::PluginMethods, pyshinqlx_owner,
};

const INVALID_ID: &str = "Invalid ID. Use either a client ID or a SteamID64.";
const INVALID_CLIENT_ID: &str = "Invalid client ID. Use either a client ID or a SteamID64.";

/// Database agnostic way of getting and setting the permissions of players.
#[pyclass(name = "permission", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Permission;

enum Target<'py> {
    Id(i64),
    Player(i64, Bound<'py, PyAny>),
}

impl Target<'_> {
    fn steam_id(&self) -> i64 {
        match self {
            Target::Id(steam_id) | Target::Player(steam_id, _) => *steam_id,
        }
    }
}

fn reply(channel: &Bound<'_, PyAny>, message: &str) -> PyResult<Option<i32>> {
    channel
        .call_method1(intern!(channel.py(), "reply"), (message,))
        .map(|_| None)
}

fn usage() -> PyResult<Option<i32>> {
    Ok(Some(PythonReturnCodes::RET_USAGE as i32))
}

fn find_player<'py>(py: Python<'py>, client_id: i64) -> PyResult<Option<Bound<'py, PyAny>>> {
    match py
        .get_type::<Plugin>()
        .call_method1(intern!(py, "player"), (client_id,))
    {
        Ok(player) if !player.is_none() => Ok(Some(player)),
        Ok(_) => Ok(None),
        Err(err) if err.is_instance_of::<NonexistentPlayerError>(py) => Ok(None),
        Err(err) => Err(err),
    }
}

fn resolve_target<'py>(py: Python<'py>, ident: i64) -> PyResult<Option<Target<'py>>> {
    if !(0..64).contains(&ident) {
        return Ok(Some(Target::Id(ident)));
    }

    let Some(player) = find_player(py, ident)? else {
        return Ok(None);
    };
    let steam_id = player.getattr(intern!(py, "steam_id"))?.extract()?;
    Ok(Some(Target::Player(steam_id, player)))
}

fn display_name(target: &Target<'_>) -> PyResult<String> {
    match target {
        Target::Id(steam_id) => Ok(steam_id.to_string()),
        Target::Player(_, player) => player.getattr(intern!(player.py(), "name"))?.extract(),
    }
}

#[pymethods]
impl Permission {
    fn cmd_setperm(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 3 {
            return usage();
        }
        let Ok(ident) = msg[1].trim().parse::<i64>() else {
            return reply(channel, INVALID_ID);
        };
        let Some(target) = resolve_target(slf.py(), ident)? else {
            return reply(channel, INVALID_CLIENT_ID);
        };
        let Some(level) = msg[2]
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|level| (0..=5).contains(level))
        else {
            return reply(
                channel,
                "Invalid permission level. Use a level between 0 and 5.",
            );
        };

        slf.as_super().get_db()?.call_method1(
            intern!(slf.py(), "set_permission"),
            (target.steam_id(), level),
        )?;

        reply(
            channel,
            &format!(
                "^6{}^7 was given permission level ^6{level}^7.",
                display_name(&target)?
            ),
        )
    }

    fn cmd_getperm(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let Ok(ident) = msg[1].trim().parse::<i64>() else {
            return reply(channel, INVALID_ID);
        };
        let Some(target) = resolve_target(slf.py(), ident)? else {
            return reply(channel, INVALID_CLIENT_ID);
        };

        if pyshinqlx_owner(slf.py())? == Some(target.steam_id()) {
            return reply(channel, "That's my master.");
        }

        let permission = slf
            .as_super()
            .get_db()?
            .call_method1(intern!(slf.py(), "get_permission"), (target.steam_id(),))?;
        if permission.is_none() {
            return reply(channel, &format!("I do not know ^6{}^7.", msg[1]));
        }

        reply(
            channel,
            &format!(
                "^6{}^7 has permission level ^6{}^7.",
                display_name(&target)?,
                permission.str()?
            ),
        )
    }

    fn cmd_myperm(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let steam_id: i64 = player.getattr(intern!(slf.py(), "steam_id"))?.extract()?;
        if pyshinqlx_owner(slf.py())? == Some(steam_id) {
            return reply(channel, "You can do anything to me, master.");
        }

        let permission = slf
            .as_super()
            .get_db()?
            .call_method1(intern!(slf.py(), "get_permission"), (player,))?;
        if permission.is_none() {
            return reply(channel, "I do not know you.");
        }

        reply(
            channel,
            &format!("You have permission level ^6{}^7.", permission.str()?),
        )
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Permission),
    )?;
    let in_game_channels = PyTuple::new(
        py,
        [
            "chat",
            "red_team_chat",
            "blue_team_chat",
            "spectator_chat",
            "free_chat",
            "client_command",
        ],
    )?;

    register_command(
        plugin.as_super(),
        "setperm",
        "cmd_setperm",
        5,
        "<id> <level>",
        None,
    )?;
    register_command(plugin.as_super(), "getperm", "cmd_getperm", 5, "<id>", None)?;
    register_command(
        plugin.as_super(),
        "myperm",
        "cmd_myperm",
        0,
        "",
        Some(in_game_channels.into_any()),
    )?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod permission_tests {
    use core::borrow::BorrowMut;

    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::Permission;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                Plugin, PythonReturnCodes, plugin::PluginMethods,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::RestoredAttribute,
            },
        },
        prelude::*,
    };

    const OWNER_ID: i64 = 76_561_198_000_000_001;
    const OTHER_ID: i64 = 76_561_198_000_000_002;

    struct Fixtures<'py> {
        plugin: Bound<'py, Permission>,
        channel: Bound<'py, PyAny>,
        database: Bound<'py, PyAny>,
        player: Bound<'py, PyAny>,
    }

    impl Fixtures<'_> {
        fn replies(&self) -> Vec<String> {
            self.channel
                .getattr("replies")
                .and_then(|replies| replies.extract())
                .expect("could not get replies")
        }

        fn calls(&self) -> Vec<(i64, i32)> {
            self.database
                .getattr("calls")
                .and_then(|calls| calls.extract())
                .expect("could not get calls")
        }

        fn know(&self, steam_id: i64, level: i32) {
            self.database
                .getattr("permissions")
                .and_then(|permissions| permissions.set_item(steam_id, level))
                .expect("could not set permission");
        }
    }

    fn with_fixtures<F>(steam_id_of_caller: i64, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        let owner = c"76561198000000001";
        let mut raw_owner_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_owner_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::attach(|py| {
                    let module = PyModule::from_code(
                        py,
                        c"
class Channel:
    def __init__(self):
        self.replies = []

    def reply(self, message):
        self.replies.append(message)


class Database:
    def __init__(self, plugin):
        self.permissions = {}
        self.calls = []

    def get_permission(self, who):
        return self.permissions.get(getattr(who, 'steam_id', who))

    def set_permission(self, who, level):
        self.calls.append((who, level))
        self.permissions[who] = level


class Player:
    def __init__(self, steam_id):
        self.steam_id = steam_id
        self.name = 'Caller'
",
                        c"",
                        c"",
                    )
                    .expect("could not create fixtures");
                    let plugin_class = py.get_type::<Plugin>();
                    let _restore_database =
                        RestoredAttribute::capture(plugin_class.as_any(), "database");
                    plugin_class
                        .setattr(
                            intern!(py, "database"),
                            module.getattr("Database").expect("no Database"),
                        )
                        .expect("could not set database");

                    let plugin = Bound::new(
                        py,
                        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Permission),
                    )
                    .expect("could not create plugin");
                    let database = plugin.as_super().get_db().expect("could not get database");
                    test(Fixtures {
                        plugin,
                        channel: module
                            .getattr("Channel")
                            .and_then(|channel| channel.call0())
                            .expect("could not create channel"),
                        database,
                        player: module
                            .getattr("Player")
                            .and_then(|player| player.call1((steam_id_of_caller,)))
                            .expect("could not create player"),
                    });
                });
            });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[rstest]
    #[case(&["setperm"])]
    #[case(&["setperm", "12345678901"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setperm_without_enough_arguments_asks_for_usage(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_setperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(PythonReturnCodes::RET_USAGE as i32)));
            assert!(fixtures.calls().is_empty());
        });
    }

    #[rstest]
    #[case(&["setperm", "abc", "3"], "Invalid ID. Use either a client ID or a SteamID64.")]
    #[case(&["setperm", "12345678901", "6"], "Invalid permission level. Use a level between 0 and 5.")]
    #[case(&["setperm", "12345678901", "-1"], "Invalid permission level. Use a level between 0 and 5.")]
    #[case(&["setperm", "12345678901", "x"], "Invalid permission level. Use a level between 0 and 5.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setperm_with_invalid_arguments_replies_and_changes_nothing(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_setperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
            assert!(fixtures.calls().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setperm_stores_the_level_of_a_steam_id(_pyshinqlx_setup: ()) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_setperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["setperm", "76561198000000002", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.calls(), [(OTHER_ID, 3)]);
            assert_eq!(
                fixtures.replies(),
                ["^676561198000000002^7 was given permission level ^63^7."]
            );
        });
    }

    #[rstest]
    #[case(&["getperm"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getperm_without_enough_arguments_asks_for_usage(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_getperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(PythonReturnCodes::RET_USAGE as i32)));
        });
    }

    #[rstest]
    #[case("abc", "Invalid ID. Use either a client ID or a SteamID64.")]
    #[case("76561198000000001", "That's my master.")]
    #[case("76561198000000002", "I do not know ^676561198000000002^7.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getperm_replies_for_invalid_owner_and_unknown_ids(
        _pyshinqlx_setup: (),
        #[case] ident: &str,
        #[case] expected: &str,
    ) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_getperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["getperm", ident]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getperm_replies_with_the_known_level(_pyshinqlx_setup: ()) {
        with_fixtures(OTHER_ID, |fixtures| {
            fixtures.know(OTHER_ID, 4);

            let result = Permission::cmd_getperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["getperm", "76561198000000002"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["^676561198000000002^7 has permission level ^64^7."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn myperm_greets_the_owner(_pyshinqlx_setup: ()) {
        with_fixtures(OWNER_ID, |fixtures| {
            let result = Permission::cmd_myperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["myperm"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["You can do anything to me, master."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn myperm_does_not_know_unregistered_players(_pyshinqlx_setup: ()) {
        with_fixtures(OTHER_ID, |fixtures| {
            let result = Permission::cmd_myperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["myperm"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["I do not know you."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn myperm_replies_with_the_known_level(_pyshinqlx_setup: ()) {
        with_fixtures(OTHER_ID, |fixtures| {
            fixtures.know(OTHER_ID, 2);

            let result = Permission::cmd_myperm(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["myperm"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["You have permission level ^62^7."]);
        });
    }
}

#[cfg(test)]
mod permission_registration_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::python::{
            COMMANDS, commands::CommandInvoker, pyshinqlx_setup_fixture::pyshinqlx_setup,
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn create_registers_the_permission_commands(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            COMMANDS.store(Some(
                Py::new(py, CommandInvoker::py_new())
                    .expect("could not create command invoker")
                    .into(),
            ));

            let plugin = create(py).expect("could not create plugin");

            let commands = plugin
                .getattr(intern!(py, "commands"))
                .and_then(|commands| commands.len())
                .expect("could not get commands");
            assert_eq!(commands, 3);
        });
    }
}
