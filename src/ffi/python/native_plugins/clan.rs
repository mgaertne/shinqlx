use pyo3::prelude::*;

use super::{
    register_client_command, register_hook, target::find_connected_player, without_excessive_colors,
};
use crate::ffi::{
    c::prelude::{CS_PLAYERS, MAX_CLIENTS},
    python::{
        Player, PlayerMethods, Plugin, PythonReturnCodes, clean_text,
        commands::CommandPriorities,
        get_configstring, parse_variables,
        plugin::{PluginMethods, chat},
        set_configstring,
    },
};

const STOP_EVENT: i32 = PythonReturnCodes::RET_STOP_EVENT as i32;
const MAX_TAG_LENGTH: usize = 5;

/// Lets players choose their own clan tag.
#[pyclass(name = "clan", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Clan;

fn tag_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}:clantag")
}

fn client_id_of(index: u32) -> Option<i64> {
    index
        .checked_sub(CS_PLAYERS)
        .filter(|client_id| *client_id < MAX_CLIENTS)
        .map(i64::from)
}

fn configstring_without_clan_tag(configstring: &str) -> String {
    let remaining = parse_variables(configstring)
        .iter()
        .filter(|(key, _)| key != "cn" && key != "xcn")
        .map(|(key, value)| format!("\\{key}\\{value}"))
        .collect::<String>();
    remaining
        .strip_prefix('\\')
        .map_or(remaining.clone(), str::to_owned)
}

fn configstring_with_clan_tag(configstring: &str, tag: &str) -> String {
    let mut variables = parse_variables(configstring);
    variables.set("xcn", tag);
    variables.set("cn", tag);
    variables.into()
}

#[pymethods]
impl Clan {
    fn handle_set_configstring(
        slf: &Bound<'_, Self>,
        index: u32,
        value: &str,
    ) -> PyResult<Option<String>> {
        if value.is_empty() {
            return Ok(None);
        }
        let Some(client_id) = client_id_of(index) else {
            return Ok(None);
        };
        let Some(player) = find_connected_player(slf.py(), client_id)? else {
            return Ok(None);
        };

        let database = slf.as_super().get_db()?;
        let key = tag_key(player.get_steam_id());
        if !database.contains(&key)? {
            return Ok(None);
        }

        let tag = database.get_item(&key)?.extract::<String>()?;
        Ok(Some(format!("{value}\\cn\\{tag}\\xcn\\{tag}")))
    }

    fn cmd_clan(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let index = u16::try_from(CS_PLAYERS)? + u16::try_from(player.get_id())?;
        let database = slf.as_super().get_db()?;
        let key = tag_key(player.get_steam_id());

        let Some(requested_tag) = msg.get(1) else {
            if database.contains(&key)? {
                database.del_item(&key)?;
                set_configstring(
                    index,
                    &configstring_without_clan_tag(&get_configstring(index)?),
                )?;
                player.tell("The clan tag has been cleared.", None)?;
            } else {
                let command = msg.first().map_or("clan", String::as_str);
                player.tell(
                    &format!("Usage to set a clan tag: ^6{command} <clan_tag>"),
                    None,
                )?;
            }
            return Ok(Some(STOP_EVENT));
        };

        if clean_text(requested_tag).chars().count() > MAX_TAG_LENGTH {
            player.tell(
                "The clan tag can only be at most 5 characters long, excluding colors.",
                None,
            )?;
            return Ok(Some(STOP_EVENT));
        }

        let tag = without_excessive_colors(requested_tag);
        database.set_item(&key, &tag)?;
        set_configstring(
            index,
            &configstring_with_clan_tag(&get_configstring(index)?, &tag),
        )?;
        chat(py, &format!("{player} changed clan tag to {tag}"), 100, " ")?;
        Ok(Some(STOP_EVENT))
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Clan),
    )?;

    register_hook(
        plugin.as_super(),
        "set_configstring",
        "handle_set_configstring",
        CommandPriorities::PRI_NORMAL,
    )?;
    register_client_command(
        plugin.as_super(),
        &["clan", "setclan"],
        "cmd_clan",
        "<clan_tag>",
    )?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod clan_tests {
    use mockall::predicate;
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::{
        Clan, STOP_EVENT, client_id_of, configstring_with_clan_tag, configstring_without_clan_tag,
        tag_key,
    };
    use crate::{
        ffi::{
            c::prelude::CS_PLAYERS,
            python::{
                Player, Plugin,
                native_plugins::test_support::{ServerCommands, real_player},
                plugin::PluginMethods,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::{RestoredAttribute, with_connected_players},
            },
        },
        hooks::mock_hooks::shinqlx_set_configstring_context,
        prelude::*,
    };

    const STEAM_ID: i64 = 1002;
    const CLIENT_ID: u32 = 2;

    struct Fixtures<'py> {
        plugin: Bound<'py, Clan>,
        player: Bound<'py, Player>,
        channel: Bound<'py, PyAny>,
        database: Bound<'py, PyAny>,
        commands: ServerCommands,
    }

    impl Fixtures<'_> {
        fn printed(&self) -> Vec<String> {
            self.commands.printed(self.plugin.py())
        }

        fn stored(&self) -> Option<String> {
            self.database
                .getattr("data")
                .and_then(|data| data.get_item(tag_key(STEAM_ID)))
                .and_then(|value| value.extract())
                .ok()
        }

        fn store(&self, tag: &str) {
            self.database
                .getattr("data")
                .and_then(|data| data.set_item(tag_key(STEAM_ID), tag))
                .expect("could not store the tag");
        }
    }

    fn with_clan<F>(test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_clan_engine(MockEngineBuilder::default(), test);
    }

    fn with_clan_engine<F>(engine: MockEngineBuilder, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_connected_players(engine, &[(2, "Ada^7"), (3, "Eve^7")], || {
            let commands = ServerCommands::capture();
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        self.data = {}

    def __setitem__(self, key, value):
        self.data[key] = value

    def __getitem__(self, key):
        return self.data[key]

    def __contains__(self, key):
        return key in self.data

    def __delitem__(self, key):
        del self.data[key]
",
                    c"",
                    c"",
                )
                .expect("could not create the fixtures");
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not replace the database");

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Clan),
                )
                .expect("could not create the plugin");
                let database = plugin
                    .as_super()
                    .get_db()
                    .expect("could not get the database");
                test(Fixtures {
                    plugin,
                    player: real_player(py, STEAM_ID, "Ada^7", ""),
                    channel: py.None().into_bound(py),
                    database,
                    commands,
                });
            });
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn the_player_configstrings_start_where_the_original_plugin_expects_them() {
        assert_eq!(CS_PLAYERS, 529);
    }

    #[rstest]
    #[case(528, None)]
    #[case(529, Some(0))]
    #[case(532, Some(3))]
    #[case(592, Some(63))]
    #[case(593, None)]
    fn only_the_configstrings_of_clients_belong_to_a_player(
        #[case] index: u32,
        #[case] expected: Option<i64>,
    ) {
        assert_eq!(client_id_of(index), expected);
    }

    #[rstest]
    #[case(r"n\Ada\t\1", r"n\Ada\t\1")]
    #[case(r"n\Ada\cn\Old\xcn\Old\t\1", r"n\Ada\t\1")]
    #[case(r"cn\Old\xcn\Old", "")]
    #[case("", "")]
    fn the_clan_tag_is_removed_from_a_configstring(
        #[case] configstring: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(configstring_without_clan_tag(configstring), expected);
    }

    #[rstest]
    #[case(r"n\Ada\t\1", r"\n\Ada\t\1\xcn\Tag\cn\Tag")]
    #[case(r"n\Ada\cn\Old\xcn\Old", r"\n\Ada\xcn\Tag\cn\Tag")]
    fn the_clan_tag_replaces_the_one_in_a_configstring(
        #[case] configstring: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(configstring_with_clan_tag(configstring, "Tag"), expected);
    }

    #[rstest]
    #[case(0, "n\\Ada")]
    #[case(CS_PLAYERS + 64, "n\\Ada")]
    #[case(CS_PLAYERS + CLIENT_ID, "")]
    #[case(CS_PLAYERS + 3, "n\\Eve")]
    #[case(CS_PLAYERS + CLIENT_ID, "n\\Ada")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configstrings_are_left_alone_without_a_registered_tag_of_an_existing_player(
        _pyshinqlx_setup: (),
        #[case] index: u32,
        #[case] value: &str,
    ) {
        with_clan(|fixtures| {
            let result = Clan::handle_set_configstring(&fixtures.plugin, index, value);

            assert!(result.is_ok_and(|result| result.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn configstrings_of_players_with_a_tag_get_the_tag_appended(_pyshinqlx_setup: ()) {
        with_clan(|fixtures| {
            fixtures.store("^1Tag");

            let result =
                Clan::handle_set_configstring(&fixtures.plugin, CS_PLAYERS + CLIENT_ID, "n\\Ada");

            assert_eq!(
                result.ok().flatten().as_deref(),
                Some("n\\Ada\\cn\\^1Tag\\xcn\\^1Tag")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clan_without_a_tag_explains_how_to_set_one(_pyshinqlx_setup: ()) {
        with_clan(|fixtures| {
            let result = Clan::cmd_clan(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["setclan"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_EVENT)));
            assert_eq!(
                fixtures.printed(),
                ["Usage to set a clan tag: ^6setclan <clan_tag>"]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clan_without_a_tag_clears_the_registered_one(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(
                predicate::eq(CS_PLAYERS + CLIENT_ID),
                predicate::eq("n\\Ada\\t\\1"),
            )
            .times(1);

        let engine = MockEngineBuilder::default().with_get_configstring(
            (CS_PLAYERS + CLIENT_ID) as u16,
            "n\\Ada\\cn\\Tag\\xcn\\Tag\\t\\1",
            1,
        );
        with_clan_engine(engine, |fixtures| {
            fixtures.store("Tag");

            let result = Clan::cmd_clan(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["clan"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_EVENT)));
            assert_eq!(fixtures.stored(), None);
            assert_eq!(fixtures.printed(), ["The clan tag has been cleared."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clan_rejects_tags_longer_than_five_visible_characters(_pyshinqlx_setup: ()) {
        with_clan(|fixtures| {
            let result = Clan::cmd_clan(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["clan", "^1abcdef"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_EVENT)));
            assert_eq!(fixtures.stored(), None);
            assert_eq!(
                fixtures.printed(),
                ["The clan tag can only be at most 5 characters long, excluding colors."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn clan_sets_the_cleaned_tag_and_tells_everybody(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(
                predicate::eq(CS_PLAYERS + CLIENT_ID),
                predicate::eq("\\n\\Ada\\t\\1\\xcn\\^2ab\\cn\\^2ab"),
            )
            .times(1);

        let engine = MockEngineBuilder::default().with_get_configstring(
            (CS_PLAYERS + CLIENT_ID) as u16,
            "n\\Ada\\t\\1",
            1,
        );
        with_clan_engine(engine, |fixtures| {
            let result = Clan::cmd_clan(
                &fixtures.plugin,
                &fixtures.player,
                msg(&["clan", "^1^2ab"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_EVENT)));
            assert_eq!(fixtures.stored().as_deref(), Some("^2ab"));
            assert_eq!(fixtures.printed(), ["Ada^7 changed clan tag to ^2ab"]);
        });
    }
}

#[cfg(test)]
mod clan_creation_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::python::{
            COMMANDS, EVENT_DISPATCHERS, Plugin,
            commands::CommandInvoker,
            events::{
                EventDispatcherManager, EventDispatcherManagerMethods, SetConfigstringDispatcher,
            },
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, engine_with_cvars},
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_registers_the_hook_and_the_client_command(_pyshinqlx_setup: ()) {
        let mut engine = engine_with_cvars(&[("zmq_stats_enable", c"0")]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        pass
",
                    c"",
                    c"",
                )
                .expect("could not create the fixtures");
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                let dispatchers = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                dispatchers
                    .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                    .expect("could not add the dispatcher");
                EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));
                COMMANDS.store(Some(
                    Py::new(py, CommandInvoker::py_new())
                        .expect("could not create the command invoker")
                        .into(),
                ));

                let plugin = create(py).expect("could not create the plugin");

                assert!(
                    plugin
                        .getattr(intern!(py, "hooks"))
                        .and_then(|hooks| hooks.len())
                        .is_ok_and(|count| count == 1)
                );
                assert!(
                    plugin
                        .getattr(intern!(py, "commands"))
                        .and_then(|commands| commands.len())
                        .is_ok_and(|count| count == 1)
                );
            })
        });
    }
}
