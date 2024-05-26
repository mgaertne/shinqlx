use super::prelude::*;
use crate::ffi::c::prelude::*;

use pyo3::{
    basic::CompareOp,
    exceptions::PyNotImplementedError,
    intern,
    types::{IntoPyDict, PyType},
};

use regex::Regex;

/// An abstract class of a chat channel. A chat channel being a source of a message.
///
/// Chat channels must implement reply(), since that's the whole point of having a chat channel
/// as a class. Makes it quite convenient when dealing with commands and such, while allowing
/// people to implement their own channels, opening the possibilites for communication with the
/// bot through other means than just chat and console (e.g. web interface).
///
/// Say "ChatChannelA" and "ChatChannelB" are both subclasses of this, and "cca" and "ccb" are instances,
/// the default implementation of "cca == ccb" is comparing __repr__(). However, when you register
/// a command and list what channels you want it to work with, it'll use this class' __str__(). It's
/// important to keep this in mind if you make a subclass. Say you have a web interface that
/// supports multiple users on it simulaneously. The right way would be to set "name" to something
/// like "webinterface", and then implement a __repr__() to return something like "webinterface user1".
#[pyclass(
    module = "_commands",
    name = "AbstractChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct AbstractChannel {
    #[pyo3(name = "_name")]
    name: String,
}

#[pymethods]
impl AbstractChannel {
    #[new]
    fn py_new(name: &str) -> Self {
        AbstractChannel {
            name: name.to_string(),
        }
    }

    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __repr__(&self) -> String {
        self.name.clone()
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => other.str().map(|other_str| other_str.to_string()).map_or(
                other.repr().map_or(false.into_py(py), |other_repr| {
                    (self.__repr__() == other_repr.to_string()).into_py(py)
                }),
                |other_channel| (self.name == other_channel).into_py(py),
            ),
            CompareOp::Ne => other.str().map(|other_str| other_str.to_string()).map_or(
                other.repr().map_or(true.into_py(py), |other_repr| {
                    (self.__repr__() != other_repr.to_string()).into_py(py)
                }),
                |other_channel| (self.name != other_channel).into_py(py),
            ),
            _ => py.NotImplemented(),
        }
    }

    #[getter(name)]
    fn get_name(&self) -> String {
        self.name.clone()
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    fn reply(
        &self,
        #[allow(unused_variables)] msg: &str,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: &str,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not implemented"))
    }

    #[classmethod]
    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    fn split_long_lines(
        _cls: &Bound<'_, PyType>,
        msg: &str,
        limit: i32,
        delimiter: &str,
    ) -> Vec<String> {
        let split_string = msg.split('\n').flat_map(|value| {
            if value.len() <= limit as usize {
                vec![value.to_string()]
            } else {
                let mut result = vec![];
                let mut next_string = "".to_string();
                for item in value.split_inclusive(&delimiter) {
                    if next_string.len() + item.len() <= limit as usize {
                        next_string.push_str(item);
                    } else {
                        if !next_string.is_empty() {
                            result.push(next_string);
                        }
                        next_string = item.to_string();
                    }
                }
                if !next_string.is_empty() {
                    result.push(next_string);
                }
                result
            }
        });
        split_string.collect()
    }
}

#[cfg(test)]
mod abstract_channel_tests {
    use crate::ffi::python::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyNotImplementedError, PyTypeError};
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_constructor = py.run_bound(
                r#"
import shinqlx
abstract_channel = shinqlx.AbstractChannel("abstract")
            "#,
                None,
                None,
            );
            assert!(abstract_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_repr_representation(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_repr_assert = py.run_bound(
                r#"
import shinqlx
abstract_channel = shinqlx.AbstractChannel("abstract")
assert repr(abstract_channel) == "abstract"
            "#,
                None,
                None,
            );
            assert!(abstract_channel_repr_assert.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_str_representation(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_str_assert = py.run_bound(
                r#"
import shinqlx
abstract_channel = shinqlx.AbstractChannel("abstract")
assert str(abstract_channel) == "abstract"
            "#,
                None,
                None,
            );
            assert!(abstract_channel_str_assert.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_eq_comparison(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_eq_assert = py.run_bound(
                r#"
import shinqlx

assert shinqlx.AbstractChannel("abstract") == "abstract"
assert shinqlx.AbstractChannel("abstract") == shinqlx.AbstractChannel("abstract")
assert not (shinqlx.AbstractChannel("abstract1") == shinqlx.AbstractChannel("abstract2"))

class NoReprClass():
    def __repr__(self):
        raise NotImplementedError()
        
assert not (shinqlx.AbstractChannel("abstract") == NoReprClass())
            "#,
                None,
                None,
            );
            assert!(abstract_channel_eq_assert.is_ok(),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_not_eq_comparison(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_ne_assert = py.run_bound(
                r#"
import shinqlx

assert shinqlx.AbstractChannel("abstract1") != "abstract2"
assert shinqlx.AbstractChannel("abstract1") != shinqlx.AbstractChannel("abstract2")
assert not (shinqlx.AbstractChannel("abstract") != shinqlx.AbstractChannel("abstract"))

class NoReprClass():
    def __repr__(self):
        raise NotImplementedError()
        
assert shinqlx.AbstractChannel("abstract") != NoReprClass()
            "#,
                None,
                None,
            );
            assert!(abstract_channel_ne_assert.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn abstract_channel_does_not_support_other_comparisons(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel_cmp_assert = py.run_bound(
                r#"
import shinqlx

shinqlx.AbstractChannel("abstract") < 2
            "#,
                None,
                None,
            );
            assert!(
                abstract_channel_cmp_assert.is_err_and(|err| err.is_instance_of::<PyTypeError>(py))
            );
        });
    }

    #[test]
    fn get_name() {
        let abstract_channel = AbstractChannel {
            name: "abstract".to_string(),
        };
        assert_eq!(abstract_channel.get_name(), "abstract");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn reply_is_not_implemented(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let abstract_channel = Py::new(py, AbstractChannel::py_new("abstract")).unwrap();
            let result = abstract_channel.bind(py).borrow().reply("asdf", 100, " ");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn split_long_lines_with_short_string(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            AbstractChannel::split_long_lines(
                &py.get_type_bound::<AbstractChannel>(),
                "short",
                100,
                " ",
            )
        });

        assert_eq!(result, vec!["short".to_string()]);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn split_long_lines_with_string_that_is_split(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            AbstractChannel::split_long_lines(
                &py.get_type_bound::<AbstractChannel>(),
                "asdf1 asdf2 asdf3 asdf4\nasdf5\nasdf6",
                5,
                " ",
            )
        });

        assert_eq!(
            result,
            vec![
                "asdf1 ".to_string(),
                "asdf2 ".to_string(),
                "asdf3 ".to_string(),
                "asdf4".to_string(),
                "asdf5".to_string(),
                "asdf6".to_string()
            ]
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn split_long_lines_with_string_with_multiple_chunks(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            AbstractChannel::split_long_lines(
                &py.get_type_bound::<AbstractChannel>(),
                "asdf1 asdf2 asdf3 asdf4\nasdf5\nasdf6",
                15,
                " ",
            )
        });

        assert_eq!(
            result,
            vec![
                "asdf1 asdf2 ".to_string(),
                "asdf3 asdf4".to_string(),
                "asdf5".to_string(),
                "asdf6".to_string()
            ]
        );
    }
}

/// A channel that prints to the console.
#[pyclass(
    extends = AbstractChannel,
    module = "_commands",
    name = "ConsoleChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct ConsoleChannel {}

#[pymethods]
impl ConsoleChannel {
    #[new]
    pub(crate) fn py_new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("console")).add_subclass(Self {})
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    pub(crate) fn reply(
        &self,
        py: Python<'_>,
        msg: &str,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: &str,
    ) -> PyResult<()> {
        pyshinqlx_console_print(py, msg);
        Ok(())
    }
}

#[cfg(test)]
mod console_channel_tests {
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;

    use mockall::predicate;
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn console_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel_constructor = py.run_bound(
                r#"
import shinqlx
console_channel = shinqlx.ConsoleChannel()
            "#,
                None,
                None,
            );
            assert!(console_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reply_prints_text_to_console(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        let result = Python::with_gil(|py| {
            let console_channel = Py::new(py, ConsoleChannel::py_new()).unwrap();
            console_channel
                .bind(py)
                .borrow()
                .reply(py, "asdf", 100, " ")
        });
        assert!(result.is_ok());
    }
}

pub(crate) const MAX_MSG_LENGTH: i32 = 1000;

#[pyclass(
    extends = AbstractChannel,
    module = "_commands",
    name = "ChatChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct ChatChannel {
    #[pyo3(name = "fmt")]
    fmt: String,
}

#[pymethods]
impl ChatChannel {
    #[new]
    #[pyo3(signature = (name = "chat", fmt = "print \"{}\n\"\n"), text_signature = "(name = \"chat\", fmt = \"print \"{}\n\"\n\")")]
    pub(crate) fn py_new(name: &str, fmt: &str) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name)).add_subclass(Self {
            fmt: fmt.to_string(),
        })
    }

    fn recipients(_slf: PyRef<'_, Self>) -> PyResult<Option<Vec<i32>>> {
        Err(PyNotImplementedError::new_err("ChatChannel recipients"))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    pub(crate) fn reply(
        slf_: PyRef<'_, Self>,
        py: Python<'_>,
        msg: &str,
        limit: i32,
        delimiter: &str,
    ) -> PyResult<()> {
        let re_color_tag = Regex::new(r"\^[0-7]").unwrap();
        let fmt = slf_.fmt.clone();
        let cleaned_msg = msg.replace('"', "'");
        let targets: Option<Vec<i32>> = slf_
            .into_py(py)
            .bind(py)
            .call_method0(intern!(py, "recipients"))?
            .extract()?;

        let split_msgs = AbstractChannel::split_long_lines(
            &py.get_type_bound::<AbstractChannel>(),
            &cleaned_msg,
            limit,
            delimiter,
        );

        let mut joined_msgs = vec![];
        for s in split_msgs {
            match joined_msgs.pop() {
                None => joined_msgs.push(s),
                Some(last_msg) => {
                    let s_new = format!("{last_msg}\n{s}");
                    if s_new.bytes().len() > MAX_MSG_LENGTH as usize {
                        joined_msgs.push(last_msg);
                        joined_msgs.push(s);
                    } else {
                        joined_msgs.push(s_new);
                    }
                }
            }
        }

        let mut last_color = "".to_string();
        for s in joined_msgs {
            let message = format!("{last_color}{s}");
            let server_command = py
                .eval_bound(
                    "fmt.format(message)",
                    None,
                    Some(
                        &[
                            (intern!(py, "fmt"), fmt.clone()),
                            (intern!(py, "message"), message.clone()),
                        ]
                        .into_py_dict_bound(py),
                    ),
                )?
                .extract::<String>()?;

            let next_frame_reply: Py<PyAny> = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def reply(targets, msg):
    shinqlx.send_server_command(targets, msg)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "reply"))?
            .into();

            match targets {
                None => {
                    next_frame_reply.call1(py, (py.None(), &server_command))?;
                }
                Some(ref cids) => {
                    cids.iter()
                        .map(|&cid| next_frame_reply.call1(py, (cid, &server_command)))
                        .collect::<PyResult<Vec<_>>>()?;
                }
            }

            if let Some(color_tag) = re_color_tag.find_iter(&message).last() {
                last_color = color_tag.as_str().to_string();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod chat_channel_tests {
    use super::ChatChannel;
    use mockall::predicate;

    use crate::ffi::python::prelude::*;
    use crate::ffi::python::pyshinqlx_test_support::{
        default_test_player, default_test_player_info, run_all_frame_tasks,
    };

    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::ffi::c::prelude::{clientState_t, privileges_t, team_t, MockClient};

    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;

    use rstest::*;

    use pyo3::exceptions::{PyNotImplementedError, PyValueError};
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn chat_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let chat_channel_constructor = py.run_bound(
                r#"
import shinqlx
chat_channel = shinqlx.ChatChannel()
            "#,
                None,
                None,
            );
            assert!(chat_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn receipients_is_not_implemented(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let chat_channel = Py::new(py, ChatChannel::py_new("asdf", "print\"{}\n\"\n"))
                .expect("this should not happen");
            let result = ChatChannel::recipients(chat_channel.bind(py).borrow());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn chat_channel_subclasses_can_overwrite_recipients(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let test_reply_to_recipients = py.run_bound(
                r#"
import shinqlx

class TestChatChannel(shinqlx.ChatChannel):
    def recipients(self):
        raise ValueError("asdf")

test_channel = TestChatChannel()
test_channel.reply("asdf")
            "#,
                None,
                None,
            );
            assert!(
                test_reply_to_recipients
                    .as_ref()
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py)),
                "{:?}",
                test_reply_to_recipients.as_ref()
            );
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_default_limit_and_delimiter(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_player_name()
                .return_const("UnnamedPlayer");
            mock_entity
                .expect_get_team()
                .return_const(team_t::TEAM_SPECTATOR);
            mock_entity
                .expect_get_privileges()
                .return_const(privileges_t::PRIV_NONE);
            mock_entity
        });

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| _client.is_some() && msg == "print \"asdf\n\"\n")
            .times(1);

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let tell_channel =
                Py::new(py, TellChannel::py_new(&player)).expect("this should not happen");

            let result =
                ChatChannel::reply(tell_channel.borrow(py).into_super(), py, "asdf", 100, " ");
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_custom_limit_param(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_player_name()
                .return_const("UnnamedPlayer");
            mock_entity
                .expect_get_team()
                .return_const(team_t::TEAM_SPECTATOR);
            mock_entity
                .expect_get_privileges()
                .return_const(privileges_t::PRIV_NONE);
            mock_entity
        });

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| msg == "print \"These \nare \nfour \nlines\n\"\n")
            .times(1);

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let tell_channel =
                Py::new(py, TellChannel::py_new(&player)).expect("this should not happen");

            let result = ChatChannel::reply(
                tell_channel.borrow(py).into_super(),
                py,
                "These are four lines",
                5,
                " ",
            );
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_custom_delimiter_parameter(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_player_name()
                .return_const("UnnamedPlayer");
            mock_entity
                .expect_get_team()
                .return_const(team_t::TEAM_SPECTATOR);
            mock_entity
                .expect_get_privileges()
                .return_const(privileges_t::PRIV_NONE);
            mock_entity
        });

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| msg == "print \"These_\nare_\nfour_\nlines\n\"\n")
            .times(1);

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let chat_channel =
                Py::new(py, TellChannel::py_new(&player)).expect("this should not happen");

            let result = ChatChannel::reply(
                chat_channel.borrow(py).into_super(),
                py,
                "These_are_four_lines",
                5,
                "_",
            );
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_various_color_tags(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_player_name()
                .return_const("UnnamedPlayer");
            mock_entity
                .expect_get_team()
                .return_const(team_t::TEAM_SPECTATOR);
            mock_entity
                .expect_get_privileges()
                .return_const(privileges_t::PRIV_NONE);
            mock_entity
        });

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| {
                msg == "print \"^0Lorem ipsum dolor sit amet, consectetuer \
            adipiscing elit. ^1Aenean commodo ligula eget dolor. \n^2Aenean massa. ^3Cum sociis \
            natoque penatibus et magnis dis parturient montes, nascetur ridiculus \nmus. ^4Donec \
            quam felis, ultricies nec, pellentesque eu, pretium quis, sem. ^5Nulla consequat massa \
            \nquis enim. ^6Donec pede justo, fringilla vel, aliquet nec, vulputate eget, arcu. \
            ^6In enim justo, \nrhoncus ut, imperdiet a, venenatis vitae, justo. ^7Nullam dictum \
            felis eu pede mollis pretium. \n^0Integer tincidunt. ^1Cras dapibus. ^2Vivamus \
            elementum semper nisi. ^3Aenean vulputate eleifend \ntellus. ^4Aenean leo ligula, \
            porttitor eu, consequat vitae, eleifend ac, enim. ^5Aliquam lorem \nante, dapibus in, \
            viverra quis, feugiat a, tellus. ^6Phasellus viverra nulla ut metus varius \nlaoreet. \
            Quisque rutrum. ^7Aenean imperdiet. ^0Etiam ultricies nisi vel augue. ^1Curabitur \
            \nullamcorper ultricies nisi. ^2Nam eget dui. ^3Etiam rhoncus. Maecenas tempus, \
            tellus eget \n\"\n"
            })
            .times(1);
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| {
                msg == "print \"^3condimentum rhoncus, sem quam semper libero, sit amet adipiscing \
                sem neque sed ipsum. ^4Nam quam \nnunc, blandit vel, luctus pulvinar, hendrerit \
                id, lorem. ^5Maecenas nec odio et ante tincidunt \ntempus. ^6Donec vitae sapien ut \
                libero venenatis faucibus. ^7Nullam quis ante. ^0Etiam sit amet \norci eget eros \
                faucibus tincidunt. ^1Duis leo. ^2Sed fringilla mauris sit amet nibh. ^3Donec \
                \nsodales sagittis magna. ^4Sed consequat, leo eget bibendum sodales, augue velit \
                cursus nunc, quis \ngravida magna mi a libero. ^5Fusce vulputate eleifend sapien. \
                ^6Vestibulum purus quam, scelerisque \nut, mollis sed, nonummy id, metus. ^7Nullam \
                accumsan lorem in dui. ^0Cras ultricies mi eu turpis \nhendrerit fringilla. \
                ^1Vestibulum ante ipsum primis in faucibus orci luctus et ultrices posuere \
                \ncubilia Curae; In ac dui quis mi consectetuer lacinia. ^2Nam pretium turpis et \
                arcu. ^3Duis arcu \ntortor, suscipit eget, imperdiet nec, imperdiet iaculis, \
                ipsum. ^4Sed aliquam ultrices mauris. \n\"\n"
            })
            .times(1);
        send_server_cmd_ctx
            .expect()
            .withf(|_client, msg| {
                msg == "print \"^4^5Integer ante arcu, accumsan a, consectetuer eget, posuere ut, \
                mauris. ^6Praesent adipiscing. \n^7Phasellus ullamcorper ipsum rutrum nunc. ^0Nunc \
                nonummy metus. ^1Vestibulum volutpat pretium \nlibero. ^2Cras id dui. ^3Aenea\n\"\n"
            })
            .times(1);

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let chat_channel =
                Py::new(py, TellChannel::py_new(&player)).expect("this should not happen");

            let result = ChatChannel::reply(
                chat_channel.borrow(py).into_super(),
                py,
                "^0Lorem ipsum dolor sit amet, consectetuer adipiscing elit. \
                ^1Aenean commodo ligula eget dolor. ^2Aenean massa. ^3Cum sociis natoque penatibus \
                et magnis dis parturient montes, nascetur ridiculus mus. ^4Donec quam felis, \
                ultricies nec, pellentesque eu, pretium quis, sem. ^5Nulla consequat massa quis \
                enim. ^6Donec pede justo, fringilla vel, aliquet nec, vulputate eget, arcu. \
                ^6In enim justo, rhoncus ut, imperdiet a, venenatis vitae, justo. ^7Nullam dictum \
                felis eu pede mollis pretium. ^0Integer tincidunt. ^1Cras dapibus. ^2Vivamus \
                elementum semper nisi. ^3Aenean vulputate eleifend tellus. ^4Aenean leo ligula, \
                porttitor eu, consequat vitae, eleifend ac, enim. ^5Aliquam lorem ante, dapibus \
                in, viverra quis, feugiat a, tellus. ^6Phasellus viverra nulla ut metus varius \
                laoreet. Quisque rutrum. ^7Aenean imperdiet. ^0Etiam ultricies nisi vel augue. \
                ^1Curabitur ullamcorper ultricies nisi. ^2Nam eget dui. ^3Etiam rhoncus. Maecenas \
                tempus, tellus eget condimentum rhoncus, sem quam semper libero, sit amet \
                adipiscing sem neque sed ipsum. ^4Nam quam nunc, blandit vel, luctus pulvinar, \
                hendrerit id, lorem. ^5Maecenas nec odio et ante tincidunt tempus. ^6Donec vitae \
                sapien ut libero venenatis faucibus. ^7Nullam quis ante. ^0Etiam sit amet orci \
                eget eros faucibus tincidunt. ^1Duis leo. ^2Sed fringilla mauris sit amet nibh. \
                ^3Donec sodales sagittis magna. ^4Sed consequat, leo eget bibendum sodales, augue \
                velit cursus nunc, quis gravida magna mi a libero. ^5Fusce vulputate eleifend \
                sapien. ^6Vestibulum purus quam, scelerisque ut, mollis sed, nonummy id, metus. \
                ^7Nullam accumsan lorem in dui. ^0Cras ultricies mi eu turpis hendrerit fringilla. \
                ^1Vestibulum ante ipsum primis in faucibus orci luctus et ultrices posuere cubilia \
                Curae; In ac dui quis mi consectetuer lacinia. ^2Nam pretium turpis et arcu. \
                ^3Duis arcu tortor, suscipit eget, imperdiet nec, imperdiet iaculis, ipsum. ^4Sed \
                aliquam ultrices mauris. ^5Integer ante arcu, accumsan a, consectetuer eget, \
                posuere ut, mauris. ^6Praesent adipiscing. ^7Phasellus ullamcorper ipsum rutrum \
                nunc. ^0Nunc nonummy metus. ^1Vestibulum volutpat pretium libero. ^2Cras id dui. \
                ^3Aenea",
                100,
                " ",
            );
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    }
}

#[pyclass(
    extends = ChatChannel,
    module = "_commands",
    name = "TellChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct TellChannel {
    client_id: i32,
}

#[pymethods]
impl TellChannel {
    #[new]
    pub(crate) fn py_new(player: &Player) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("tell"))
            .add_subclass(ChatChannel {
                fmt: "print \"{}\n\"\n".to_string(),
            })
            .add_subclass(Self {
                client_id: player.id,
            })
    }

    fn __repr__(&self) -> String {
        format!("tell {}", self.client_id)
    }

    #[getter(recipient)]
    fn get_recipient(&self) -> PyResult<Player> {
        Player::py_new(self.client_id, None)
    }

    fn recipients(slf_: PyRef<'_, Self>) -> PyResult<Option<Vec<i32>>> {
        Ok(Some(vec![slf_.client_id]))
    }
}

#[cfg(test)]
mod tell_channel_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::ffi::python::pyshinqlx_test_support::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let tell_channel_constructor = py.run_bound(
                r#"
import shinqlx
tell_channel = shinqlx.TellChannel(player)
            "#,
                None,
                Some(&vec![("player", player.into_py(py))].into_py_dict_bound(py)),
            );
            assert!(tell_channel_constructor.is_ok());
        });
    }

    #[test]
    fn repr_returns_tell_client_id() {
        let tell_channel = TellChannel { client_id: 42 };
        assert_eq!(tell_channel.__repr__(), "tell 42");
    }

    #[test]
    #[serial]
    fn get_recipient_returns_player_with_client_id() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_CONNECTED);
                mock_client.expect_get_user_info().return_const("");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let tell_channel = TellChannel { client_id: 42 };
        assert!(tell_channel
            .get_recipient()
            .is_ok_and(|player| player.id == 42));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn recipients_returns_vec_with_client_id(_pyshinqlx_setup: ()) {
        let player = default_test_player();
        Python::with_gil(|py| {
            let py_tell_channel =
                Py::new(py, TellChannel::py_new(&player)).expect("this should not happen");
            assert!(TellChannel::recipients(py_tell_channel.bind(py).borrow())
                .is_ok_and(|recipients| recipients == Some(vec![2,])));
        });
    }
}

/// A channel for chat to and from the server.
#[pyclass(
    extends = ChatChannel,
    module = "_commands",
    name = "TeamChatChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct TeamChatChannel {
    team: String,
}

#[pymethods]
impl TeamChatChannel {
    #[new]
    #[pyo3(signature = (team="all", name="chat", fmt="print \"{}\n\"\n"), text_signature = "(team=\"all\", name=\"chat\", fmt=\"print \"{}\n\"\n\")")]
    pub(crate) fn py_new(team: &str, name: &str, fmt: &str) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name))
            .add_subclass(ChatChannel {
                fmt: fmt.to_string(),
            })
            .add_subclass(Self {
                team: team.to_string(),
            })
    }

    fn recipients(&self, py: Python<'_>) -> PyResult<Option<Vec<i32>>> {
        if self.team == "all" {
            return Ok(None);
        }

        let filtered_team: i32 = match self.team.as_str() {
            "red" => team_t::TEAM_RED as i32,
            "blue" => team_t::TEAM_BLUE as i32,
            "free" => team_t::TEAM_FREE as i32,
            "spectator" => team_t::TEAM_SPECTATOR as i32,
            _ => -1,
        };

        let players_info = pyshinqlx_players_info(py)?;
        Ok(Some(
            players_info
                .iter()
                .filter_map(|opt_player_info| {
                    opt_player_info
                        .as_ref()
                        .iter()
                        .filter_map(|player_info| {
                            if player_info.team == filtered_team {
                                Some(player_info.client_id)
                            } else {
                                None
                            }
                        })
                        .next()
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod team_chat_channel_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn team_chat_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let team_chat_channel_constructor = py.run_bound(
                r#"
import shinqlx
tell_channel = shinqlx.TeamChatChannel("all")
            "#,
                None,
                None,
            );
            assert!(team_chat_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[case("all", None)]
    #[case("red", Some(vec![1, 5]))]
    #[case("blue", Some(vec![2, 6]))]
    #[case("spectator", Some(vec![3, 7]))]
    #[case("free", Some(vec![0, 4]))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recipients_returns_client_ids(
        _pyshinqlx_setup: (),
        #[case] team: &str,
        #[case] expected_ids: Option<Vec<i32>>,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 8);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || match client_id {
                        0 => team_t::TEAM_FREE,
                        1 => team_t::TEAM_RED,
                        2 => team_t::TEAM_BLUE,
                        4 => team_t::TEAM_FREE,
                        5 => team_t::TEAM_RED,
                        6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        let team_chat_channel = TeamChatChannel {
            team: team.to_string(),
        };
        let result = Python::with_gil(|py| team_chat_channel.recipients(py));
        assert!(result.is_ok_and(|ids| ids == expected_ids));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recipients_for_invalid_team_chat_channel_name(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 8);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || match client_id {
                        0 => team_t::TEAM_FREE,
                        1 => team_t::TEAM_RED,
                        2 => team_t::TEAM_BLUE,
                        4 => team_t::TEAM_FREE,
                        5 => team_t::TEAM_RED,
                        6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        let team_chat_channel = TeamChatChannel {
            team: "invalid".to_string(),
        };
        let result = Python::with_gil(|py| team_chat_channel.recipients(py));
        assert!(result.is_ok_and(|ids| ids == Some(vec![])));
    }
}

/// Wraps a TellChannel, but with its own name.
#[pyclass(
    extends = AbstractChannel,
    module = "_commands",
    name = "ClientCommandChannel",
    subclass,
    frozen,
    get_all
)]
pub(crate) struct ClientCommandChannel {
    client_id: i32,
}

#[pymethods]
impl ClientCommandChannel {
    #[new]
    pub(crate) fn py_new(player: &Player) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("client_command")).add_subclass(Self {
            client_id: player.id,
        })
    }

    fn __repr__(&self) -> String {
        format!("client_command {}", self.client_id)
    }

    #[getter(recipient)]
    fn get_recipient(&self) -> PyResult<Player> {
        Player::py_new(self.client_id, None)
    }

    #[getter(tell_channel)]
    fn get_tell_channel(&self, py: Python<'_>) -> PyResult<Py<TellChannel>> {
        let player = self.get_recipient()?;
        Py::new(py, TellChannel::py_new(&player))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    fn reply(&self, py: Python<'_>, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        let tell_channel = Py::new(
            py,
            PyClassInitializer::from(AbstractChannel::py_new("tell"))
                .add_subclass(ChatChannel {
                    fmt: "print \"{}\n\"\n".to_string(),
                })
                .add_subclass(TellChannel {
                    client_id: self.client_id,
                }),
        )?
        .to_object(py);

        tell_channel.call_method1(py, intern!(py, "reply"), (msg, limit, delimiter))?;
        Ok(())
    }
}

#[cfg(test)]
mod client_command_channel_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::ffi::python::pyshinqlx_test_support::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_command_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let client_command_channel_constructor = py.run_bound(
                r#"
import shinqlx
tell_channel = shinqlx.ClientCommandChannel(player)
            "#,
                None,
                Some(&vec![("player", player.into_py(py))].into_py_dict_bound(py)),
            );
            assert!(client_command_channel_constructor.is_ok());
        });
    }

    #[test]
    fn repr_returns_tell_client_id() {
        let client_command_channel = ClientCommandChannel { client_id: 42 };
        assert_eq!(client_command_channel.__repr__(), "client_command 42");
    }

    #[test]
    #[serial]
    fn get_recipient_returns_player_with_client_id() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_CONNECTED);
                mock_client.expect_get_user_info().return_const("");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let client_command_channel = ClientCommandChannel { client_id: 42 };
        assert!(client_command_channel
            .get_recipient()
            .is_ok_and(|player| player.id == 42));
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_tell_channel_returns_tell_channel_with_client_id(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_CONNECTED);
                mock_client.expect_get_user_info().return_const("");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let client_command_channel = ClientCommandChannel { client_id: 42 };
        let result = Python::with_gil(|py| client_command_channel.get_tell_channel(py));
        assert!(result.is_ok_and(|tell_channel| tell_channel.get().client_id == 42));
    }
}
