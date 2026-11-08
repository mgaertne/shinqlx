use std::{fs::create_dir_all, path::Path};

use pyo3::{
    PyTraverseError,
    exceptions::PyEnvironmentError,
    gc::PyVisit,
    intern,
    prelude::*,
    types::{PyDict, PyInt},
};

use super::register_hook;
use crate::ffi::python::{Plugin, clean_text, commands::CommandPriorities, get_cvar};

const CHATLOG_COUNT_CVAR: &str = "qlx_chatlogs";
const CHATLOG_SIZE_CVAR: &str = "qlx_chatlogsSize";

/// Writes the connects, disconnects, chat messages and commands of the players to a rotating
/// chat log below the home path of the server.
#[pyclass(name = "log", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct ChatLog {
    chatlog: Py<PyAny>,
}

impl ChatLog {
    fn write(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.chatlog
            .bind(py)
            .call_method1(intern!(py, "info"), (message,))
            .map(|_| ())
    }
}

#[pymethods]
impl ChatLog {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.chatlog)
    }

    fn handle_player_connect(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        let message = format!(
            "{}:{}:{} connected.",
            player.getattr(intern!(py, "clean_name"))?,
            player.getattr(intern!(py, "steam_id"))?,
            player.getattr(intern!(py, "ip"))?,
        );

        slf.get().write(py, &message)
    }

    fn handle_player_disconnect(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        let reason_text = match reason.extract::<String>() {
            Ok(text) if !text.is_empty() && !text.ends_with(['?', '!', '.']) => format!("{text}."),
            Ok(text) => text,
            Err(_) => reason.str()?.to_string(),
        };
        let message = format!(
            "{player}:{} {reason_text}",
            player.getattr(intern!(py, "steam_id"))?,
        );

        slf.get().write(py, &clean_text(&message))
    }

    fn handle_chat(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: &Bound<'_, PyAny>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        let channel_name = if channel.eq("chat")? {
            String::new()
        } else {
            format!("[{}] ", channel.str()?.to_string().to_uppercase())
        };
        let message = format!(
            "{channel_name}<{player}:{}> {msg}",
            player.getattr(intern!(py, "steam_id"))?,
        );

        slf.get().write(py, &clean_text(&message))
    }

    fn handle_command(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, PyAny>,
        _command: &Bound<'_, PyAny>,
        args: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        let message = format!(
            "[CMD] <{caller}:{}> {args}",
            caller.getattr(intern!(py, "steam_id"))?,
        );

        slf.get().write(py, &clean_text(&message))
    }
}

fn create_chatlog(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let home_path = get_cvar("fs_homepath")?
        .ok_or_else(|| PyEnvironmentError::new_err("cvar fs_homepath is not set"))?;
    let directory = Path::new(&home_path).join("chatlogs");
    create_dir_all(&directory)?;
    let file_path = directory.join("chat.log");

    let plugin_class = py.get_type::<Plugin>();
    let int_type = py.get_type::<PyInt>();
    let max_logs =
        plugin_class.call_method1(intern!(py, "get_cvar"), (CHATLOG_COUNT_CVAR, &int_type))?;
    let max_size =
        plugin_class.call_method1(intern!(py, "get_cvar"), (CHATLOG_SIZE_CVAR, &int_type))?;

    let logging = py.import(intern!(py, "logging"))?;
    let formatter = logging
        .getattr(intern!(py, "Formatter"))?
        .call1(("[%(asctime)s] %(message)s", "%Y-%m-%d %H:%M:%S"))?;
    let handler_options = PyDict::new(py);
    handler_options.set_item(intern!(py, "encoding"), "utf-8")?;
    handler_options.set_item(intern!(py, "maxBytes"), max_size)?;
    handler_options.set_item(intern!(py, "backupCount"), max_logs)?;
    let handler = py
        .import(intern!(py, "logging.handlers"))?
        .getattr(intern!(py, "RotatingFileHandler"))?
        .call((file_path.to_string_lossy(),), Some(&handler_options))?;
    handler.call_method1(intern!(py, "setFormatter"), (formatter,))?;

    let chatlog = logging.getattr(intern!(py, "Logger"))?.call1(("log",))?;
    chatlog.call_method1(intern!(py, "addHandler"), (handler,))?;
    let started = py
        .import(intern!(py, "datetime"))?
        .getattr(intern!(py, "datetime"))?
        .call_method0(intern!(py, "now"))?;
    chatlog.call_method1(
        intern!(py, "info"),
        (format!(
            "============================= Logger started @ {started} ============================="
        ),),
    )?;

    Ok(chatlog)
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin_class = py.get_type::<Plugin>();
    plugin_class.call_method1(intern!(py, "set_cvar_once"), (CHATLOG_COUNT_CVAR, "3"))?;
    plugin_class.call_method1(
        intern!(py, "set_cvar_once"),
        (CHATLOG_SIZE_CVAR, (3 * 10_u32.pow(6)).to_string()),
    )?;
    let chatlog = create_chatlog(py)?;

    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(ChatLog {
            chatlog: chatlog.unbind(),
        }),
    )?;
    for (event, handler) in [
        ("player_connect", "handle_player_connect"),
        ("player_disconnect", "handle_player_disconnect"),
        ("chat", "handle_chat"),
        ("command", "handle_command"),
    ] {
        register_hook(
            plugin.as_super(),
            event,
            handler,
            CommandPriorities::PRI_LOWEST,
        )?;
    }

    Ok(plugin.into_any())
}

#[cfg(test)]
mod chat_log_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::{ChatLog, create};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                EVENT_DISPATCHERS, Plugin,
                events::{
                    ChatEventDispatcher, CommandDispatcher, EventDispatcherManager,
                    EventDispatcherManagerMethods, PlayerConnectDispatcher,
                    PlayerDisconnectDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
            },
        },
        prelude::*,
    };

    struct Fixtures<'py> {
        plugin: Bound<'py, ChatLog>,
        recorder: Bound<'py, PyAny>,
        player: Bound<'py, PyAny>,
    }

    impl Fixtures<'_> {
        fn messages(&self) -> Vec<String> {
            self.recorder
                .getattr("messages")
                .and_then(|messages| messages.extract())
                .expect("could not get the messages")
        }
    }

    fn fixtures(py: Python<'_>) -> Fixtures<'_> {
        let module = PyModule::from_code(
            py,
            c"
class Recorder:
    def __init__(self):
        self.messages = []

    def info(self, message):
        self.messages.append(message)


class Player:
    clean_name = 'Ada'
    steam_id = 1234
    ip = '10.0.0.1'

    def __str__(self):
        return '^1Ada'
",
            c"",
            c"",
        )
        .expect("could not create the fixtures");
        let recorder = module
            .getattr("Recorder")
            .and_then(|recorder| recorder.call0())
            .expect("could not create the recorder");
        let plugin = Bound::new(
            py,
            PyClassInitializer::from(Plugin::py_new(py)).add_subclass(ChatLog {
                chatlog: recorder.clone().unbind(),
            }),
        )
        .expect("could not create the plugin");
        let player = module
            .getattr("Player")
            .and_then(|player| player.call0())
            .expect("could not create the player");

        Fixtures {
            plugin,
            recorder,
            player,
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
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
                &msg.into_pyobject(py)
                    .expect("this should not happen")
                    .into_any(),
                &channel
                    .into_pyobject(py)
                    .expect("this should not happen")
                    .into_any(),
            )
            .expect("could not handle the chat message");

            assert_eq!(fixtures.messages(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn commands_are_logged_with_their_caller_and_arguments(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let fixtures = fixtures(py);

            ChatLog::handle_command(
                &fixtures.plugin,
                &fixtures.player,
                &"slap"
                    .into_pyobject(py)
                    .expect("this should not happen")
                    .into_any(),
                &"^3Bob 10"
                    .into_pyobject(py)
                    .expect("this should not happen")
                    .into_any(),
            )
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
                    let log = plugin.cast::<ChatLog>().expect("not a chat log plugin");
                    let handlers = log
                        .get()
                        .chatlog
                        .bind(py)
                        .getattr("handlers")
                        .expect("could not get the handlers");
                    for handler in handlers.try_iter().expect("handlers are not iterable") {
                        handler
                            .and_then(|handler| handler.call_method0("close"))
                            .expect("could not close the chat log");
                    }
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
