use core::hint::cold_path;
use std::{fs::create_dir_all, path::Path};

use chrono::Local;
use log::{Level, Record};
use log4rs::append::Append;
use pyo3::{exceptions::PyEnvironmentError, prelude::*};

use super::{create_cvar, cvar_int, register_hook};
use crate::{
    ffi::python::{
        AbstractChannel, AbstractChannelMethods, Command, Player, PlayerMethods, Plugin,
        clean_text, commands::CommandPriorities, events_enum::EventName, get_cvar,
        string_enums::ChatChannel,
    },
    logging::{RotationDefaults, open_log_file},
};

const CHATLOG_COUNT_CVAR: &str = "qlx_chatlogs";
const CHATLOG_SIZE_CVAR: &str = "qlx_chatlogsSize";

/// Writes the connects, disconnects, chat messages and commands of the players to a rotating
/// chat log below the home path of the server.
#[pyclass(name = "log", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct ChatLog {
    chatlog: Box<dyn Append>,
}

impl ChatLog {
    fn write(&self, message: &str) -> PyResult<()> {
        append_line(self.chatlog.as_ref(), message)
    }
}

fn append_line(chatlog: &dyn Append, message: &str) -> PyResult<()> {
    chatlog
        .append(
            &Record::builder()
                .level(Level::Info)
                .target("chat")
                .args(format_args!("{message}"))
                .build(),
        )
        .map_err(|error| PyEnvironmentError::new_err(error.to_string()))
}

#[pymethods]
impl ChatLog {
    fn handle_player_connect(slf: &Bound<'_, Self>, player: &Bound<'_, Player>) -> PyResult<()> {
        let message = format!(
            "{}:{}:{} connected.",
            player.get_clean_name(),
            player.get_steam_id(),
            player.get_ip(),
        );

        slf.get().write(&message)
    }

    fn handle_player_disconnect(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let reason_text = match reason.extract::<String>() {
            Ok(text) if !text.is_empty() && !text.ends_with(['?', '!', '.']) => format!("{text}."),
            Ok(text) => text,
            Err(_) => reason.str()?.to_string(),
        };
        let message = format!("{player}:{} {reason_text}", player.get_steam_id());

        slf.get().write(&clean_text(&message))
    }

    fn handle_chat(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: &str,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<()> {
        let name = channel.get_name();
        let channel_name = match ChatChannel::from_name(&name) {
            Some(ChatChannel::Chat) => String::new(),
            _ => format!("[{}] ", name.to_uppercase()),
        };
        let message = format!("{channel_name}<{player}:{}> {msg}", player.get_steam_id());

        slf.get().write(&clean_text(&message))
    }

    fn handle_command(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, Player>,
        _command: &Bound<'_, Command>,
        args: &str,
    ) -> PyResult<()> {
        let message = format!("[CMD] <{caller}:{}> {args}", caller.get_steam_id());

        slf.get().write(&clean_text(&message))
    }
}

const CHATLOG_PATTERN: &str = "[{d(%Y-%m-%d %H:%M:%S)}] {m}{n}";

fn create_chatlog() -> PyResult<Box<dyn Append>> {
    let home_path = get_cvar("fs_homepath")?.ok_or_else(|| {
        cold_path();
        PyEnvironmentError::new_err("cvar fs_homepath is not set")
    })?;
    let directory = Path::new(&home_path).join("chatlogs");
    create_dir_all(&directory)?;
    let file_path = directory.join("chat.log");

    let defaults = RotationDefaults {
        max_size: u64::try_from(cvar_int(CHATLOG_SIZE_CVAR)?).unwrap_or(0),
        count: u32::try_from(cvar_int(CHATLOG_COUNT_CVAR)?).unwrap_or(0),
    };
    let chatlog = open_log_file(&file_path, CHATLOG_PATTERN, defaults)
        .map_err(|error| PyEnvironmentError::new_err(format!("{error:#}")))?;

    let started = Local::now().format("%Y-%m-%d %H:%M:%S%.6f");
    append_line(
        chatlog.as_ref(),
        &format!(
            "============================= Logger started @ {started} ============================="
        ),
    )?;

    Ok(chatlog)
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, Plugin>> {
    create_cvar(py, CHATLOG_COUNT_CVAR, "3")?;
    create_cvar(py, CHATLOG_SIZE_CVAR, &(3 * 10_u32.pow(6)).to_string())?;
    let chatlog = create_chatlog()?;

    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(ChatLog { chatlog }),
    )?;
    for (event, handler) in [
        (EventName::PlayerConnect, "handle_player_connect"),
        (EventName::PlayerDisconnect, "handle_player_disconnect"),
        (EventName::Chat, "handle_chat"),
        (EventName::Command, "handle_command"),
    ] {
        register_hook(
            plugin.as_super(),
            event,
            handler,
            CommandPriorities::PRI_LOWEST,
        )?;
    }

    Ok(plugin.into_super())
}

#[cfg(test)]
mod chat_log_tests {
    use alloc::{ffi::CString, sync::Arc};
    use core::borrow::BorrowMut;
    use std::sync::Mutex;

    use log::Record;
    use log4rs::append::Append;
    use pyo3::{intern, prelude::*, types::PyString};
    use rstest::rstest;

    use super::{ChatLog, create};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                Command, EVENT_DISPATCHERS, Player, Plugin,
                events::{
                    ChatEventDispatcher, CommandDispatcher, EventDispatcherManager,
                    EventDispatcherManagerMethods, PlayerConnectDispatcher,
                    PlayerDisconnectDispatcher,
                },
                native_plugins::test_support::{named_channel, real_player},
                pyshinqlx_setup_fixture::pyshinqlx_setup,
            },
        },
        prelude::*,
    };

    #[derive(Debug, Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Append for Recorder {
        fn append(&self, record: &Record<'_>) -> anyhow::Result<()> {
            self.0
                .lock()
                .expect("the messages are not poisoned")
                .push(record.args().to_string());
            Ok(())
        }

        fn flush(&self) {}
    }

    struct Fixtures<'py> {
        plugin: Bound<'py, ChatLog>,
        recorder: Recorder,
        player: Bound<'py, Player>,
    }

    impl Fixtures<'_> {
        fn messages(&self) -> Vec<String> {
            self.recorder
                .0
                .lock()
                .expect("the messages are not poisoned")
                .clone()
        }
    }

    fn fixtures(py: Python<'_>) -> Fixtures<'_> {
        let recorder = Recorder::default();
        let plugin = Bound::new(
            py,
            PyClassInitializer::from(Plugin::py_new(py)).add_subclass(ChatLog {
                chatlog: Box::new(recorder.clone()),
            }),
        )
        .expect("could not create the plugin");
        let player = real_player(py, 1234, "^1Ada", "10.0.0.1");

        Fixtures {
            plugin,
            recorder,
            player,
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connecting_players_are_logged_with_name_steam_id_and_ip(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let fixtures = fixtures(py);

            ChatLog::handle_player_connect(&fixtures.plugin, &fixtures.player)
                .expect("could not handle the connect");

            assert_eq!(fixtures.messages(), ["Ada:1234:10.0.0.1 connected."]);
        });
    }

    #[rstest]
    #[case(Some("kicked"), "Ada:1234 kicked.")]
    #[case(Some("left!"), "Ada:1234 left!")]
    #[case(Some("timed out?"), "Ada:1234 timed out?")]
    #[case(Some("done."), "Ada:1234 done.")]
    #[case(Some(""), "Ada:1234 ")]
    #[case(None, "Ada:1234 None")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn disconnecting_players_are_logged_with_a_punctuated_reason(
        _pyshinqlx_setup: (),
        #[case] reason: Option<&str>,
        #[case] expected: &str,
    ) {
        Python::attach(|py| {
            let fixtures = fixtures(py);
            let reason = match reason {
                Some(text) => text
                    .into_pyobject(py)
                    .expect("this should not happen")
                    .into_any(),
                None => py.None().into_bound(py),
            };

            ChatLog::handle_player_disconnect(&fixtures.plugin, &fixtures.player, &reason)
                .expect("could not handle the disconnect");

            assert_eq!(fixtures.messages(), [expected]);
        });
    }

    #[rstest]
    #[case("chat", "hello", "<Ada:1234> hello")]
    #[case("red_team_chat", "hello", "[RED_TEAM_CHAT] <Ada:1234> hello")]
    #[case("chat", "^2colored ^7text", "<Ada:1234> colored text")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn chat_messages_are_logged_with_their_channel(
        _pyshinqlx_setup: (),
        #[case] channel: &str,
        #[case] msg: &str,
        #[case] expected: &str,
    ) {
        Python::attach(|py| {
            let fixtures = fixtures(py);

            ChatLog::handle_chat(
                &fixtures.plugin,
                &fixtures.player,
                msg,
                &named_channel(py, channel),
            )
            .expect("could not handle the chat message");

            assert_eq!(fixtures.messages(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn commands_are_logged_with_their_caller_and_arguments(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let fixtures = fixtures(py);

            let handler = py
                .import("builtins")
                .and_then(|builtins| builtins.getattr("print"))
                .expect("could not get a callable");
            let command = Command::py_new(
                fixtures.plugin.as_any(),
                PyString::new(py, "slap").as_any(),
                &handler,
                0,
                py.None().bind(py),
                py.None().bind(py),
                false,
                0,
                false,
                "",
            )
            .and_then(|command| Bound::new(py, command))
            .expect("could not create the command");

            ChatLog::handle_command(&fixtures.plugin, &fixtures.player, &command, "^3Bob 10")
                .expect("could not handle the command");

            assert_eq!(fixtures.messages(), ["[CMD] <Ada:1234> Bob 10"]);
        });
    }

    fn string_cvar(value: &CString) -> cvar_t {
        CVarBuilder::default()
            .string(value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_opens_the_chat_log_and_hooks_the_events(_pyshinqlx_setup: ()) {
        let home = tempfile::tempdir().expect("could not create the home path");
        let home_string = CString::new(home.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let zmq_string = CString::new("0").expect("this should not happen");
        let count_string = CString::new("3").expect("this should not happen");
        let size_string = CString::new("3000000").expect("this should not happen");
        let mut raw_home = string_cvar(&home_string);
        let mut raw_zmq = string_cvar(&zmq_string);
        let mut raw_count = string_cvar(&count_string);
        let mut raw_size = string_cvar(&size_string);

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "fs_homepath",
                move |_| CVar::try_from(raw_home.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_chatlogs",
                move |_| CVar::try_from(raw_count.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_chatlogsSize",
                move |_| CVar::try_from(raw_size.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let dispatchers = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    dispatchers
                        .add_dispatcher(&py.get_type::<PlayerConnectDispatcher>())
                        .expect("could not add the player connect dispatcher");
                    dispatchers
                        .add_dispatcher(&py.get_type::<PlayerDisconnectDispatcher>())
                        .expect("could not add the player disconnect dispatcher");
                    dispatchers
                        .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                        .expect("could not add the chat dispatcher");
                    dispatchers
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add the command dispatcher");
                    EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));

                    let plugin = create(py).expect("could not create the plugin");

                    assert!(
                        plugin
                            .getattr(intern!(py, "hooks"))
                            .and_then(|hooks| hooks.len())
                            .is_ok_and(|count| count == 4)
                    );
                    let log_file = home.path().join("chatlogs").join("chat.log");
                    let content =
                        std::fs::read_to_string(&log_file).expect("could not read the chat log");
                    assert!(content.contains("Logger started @"));
                    assert!(content.starts_with('['), "{content}");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_without_a_home_path_fails(_pyshinqlx_setup: ()) {
        let zero_string = CString::new("0").expect("this should not happen");
        let mut raw_zero = string_cvar(&zero_string);

        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "fs_homepath", |_| None, 1..)
            .with_find_cvar(
                |cmd| ["qlx_chatlogs", "qlx_chatlogsSize"].contains(&cmd),
                move |_| CVar::try_from(raw_zero.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::attach(|py| {
                    let result = create(py);

                    assert!(result.is_err());
                });
            });
    }
}
