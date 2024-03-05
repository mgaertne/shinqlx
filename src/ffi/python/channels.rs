use super::prelude::*;
use crate::ffi::c::prelude::*;

use pyo3::{basic::CompareOp, exceptions::PyNotImplementedError, intern, types::IntoPyDict};
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
    fn py_new(name: String) -> Self {
        AbstractChannel { name }
    }

    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __repr__(&self) -> String {
        self.name.clone()
    }

    fn __richcmp__(&self, other: &PyAny, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => {
                if let Ok(other_channel) = other.extract::<String>() {
                    (self.name == other_channel).into_py(py)
                } else {
                    let Ok(other_repr) = other.repr() else {
                        return false.into_py(py);
                    };
                    (self.__repr__() == other_repr.to_string()).into_py(py)
                }
            }
            CompareOp::Ne => {
                if let Ok(other_channel) = other.extract::<String>() {
                    (self.name != other_channel).into_py(py)
                } else {
                    let Ok(other_repr) = other.repr() else {
                        return true.into_py(py);
                    };
                    (self.__repr__() != other_repr.to_string()).into_py(py)
                }
            }
            _ => py.NotImplemented(),
        }
    }

    #[getter(name)]
    fn get_name(&self) -> String {
        self.name.clone()
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".into()))]
    fn reply(
        #[allow(unused_variables)] self_: PyRef<'_, Self>,
        #[allow(unused_variables)] msg: String,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: String,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not implemented"))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".into()))]
    fn split_long_lines(&self, msg: String, limit: i32, delimiter: String) -> Vec<String> {
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
                        next_string = item.into();
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
            let abstract_channel_constructor = py.run(
                r#"
import _shinqlx
abstract_channel = _shinqlx.AbstractChannel("abstract")
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
            let abstract_channel_repr_assert = py.run(
                r#"
import _shinqlx
abstract_channel = _shinqlx.AbstractChannel("abstract")
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
            let abstract_channel_str_assert = py.run(
                r#"
import _shinqlx
abstract_channel = _shinqlx.AbstractChannel("abstract")
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
            let abstract_channel_eq_assert = py.run(
                r#"
import _shinqlx

assert _shinqlx.AbstractChannel("abstract") == "abstract"
assert _shinqlx.AbstractChannel("abstract") == _shinqlx.AbstractChannel("abstract")
assert not (_shinqlx.AbstractChannel("abstract1") == _shinqlx.AbstractChannel("abstract2"))

class NoReprClass():
    def __repr__(self):
        raise NotImplementedError()
        
assert not (_shinqlx.AbstractChannel("abstract") == NoReprClass())
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
            let abstract_channel_ne_assert = py.run(
                r#"
import _shinqlx

assert _shinqlx.AbstractChannel("abstract1") != "abstract2"
assert _shinqlx.AbstractChannel("abstract1") != _shinqlx.AbstractChannel("abstract2")
assert not (_shinqlx.AbstractChannel("abstract") != _shinqlx.AbstractChannel("abstract"))

class NoReprClass():
    def __repr__(self):
        raise NotImplementedError()
        
assert _shinqlx.AbstractChannel("abstract") != NoReprClass()
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
            let abstract_channel_cmp_assert = py.run(
                r#"
import _shinqlx

_shinqlx.AbstractChannel("abstract") < 2
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
            name: "abstract".into(),
        };
        assert_eq!(abstract_channel.get_name(), "abstract");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn reply_is_not_implemented() {
        Python::with_gil(|py| {
            let abstract_channel = Py::new(py, AbstractChannel::py_new("abstract".into())).unwrap();
            let result = AbstractChannel::reply(
                abstract_channel.as_ref(py).borrow(),
                "asdf".into(),
                100,
                " ".into(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[test]
    fn split_long_lines_with_short_string() {
        let abstract_channel = AbstractChannel {
            name: "abstract".into(),
        };
        let result = abstract_channel.split_long_lines("short".into(), 100, " ".into());

        assert_eq!(result, vec!["short".to_string()]);
    }

    #[test]
    fn split_long_lines_with_string_that_is_split() {
        let abstract_channel = AbstractChannel {
            name: "abstract".into(),
        };
        let result = abstract_channel.split_long_lines(
            "asdf1 asdf2 asdf3 asdf4\nasdf5\nasdf6".into(),
            5,
            " ".into(),
        );

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

    #[test]
    fn split_long_lines_with_string_with_multiple_chunks() {
        let abstract_channel = AbstractChannel {
            name: "abstract".into(),
        };
        let result = abstract_channel.split_long_lines(
            "asdf1 asdf2 asdf3 asdf4\nasdf5\nasdf6".into(),
            15,
            " ".into(),
        );

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
        PyClassInitializer::from(AbstractChannel::py_new("console".into())).add_subclass(Self {})
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".into()))]
    fn reply(
        #[allow(unused_variables)] self_: PyRef<'_, Self>,
        py: Python<'_>,
        msg: String,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: String,
    ) -> PyResult<()> {
        pyshinqlx_console_print(py, &msg);
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
            let console_channel_constructor = py.run(
                r#"
import _shinqlx
console_channel = _shinqlx.ConsoleChannel()
            "#,
                None,
                None,
            );
            assert!(console_channel_constructor.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn reply_prints_text_to_console() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        let result = Python::with_gil(|py| {
            let console_channel = Py::new(py, ConsoleChannel::py_new()).unwrap();
            ConsoleChannel::reply(
                console_channel.as_ref(py).borrow(),
                py,
                "asdf".into(),
                100,
                " ".into(),
            )
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
    #[pyo3(signature = (name = "chat".into(), fmt = "print \"{}\n\"\n".into()))]
    fn py_new(name: String, fmt: String) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name)).add_subclass(Self { fmt })
    }

    fn receipients(&self) -> PyResult<Option<Vec<i32>>> {
        Err(PyNotImplementedError::new_err(""))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".into()))]
    fn reply(
        self_: &PyCell<Self>,
        py: Python<'_>,
        msg: String,
        limit: i32,
        delimiter: String,
    ) -> PyResult<()> {
        let re_color_tag = Regex::new(r"\^[0-7]").unwrap();
        let fmt = self_.borrow().fmt.clone();
        let cleaned_msg = msg.replace('"', "'");
        let targets: Option<Vec<i32>> =
            self_.call_method0(intern!(py, "receipients"))?.extract()?;

        let split_msgs =
            self_
                .borrow()
                .into_super()
                .split_long_lines(cleaned_msg, limit, delimiter);

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
                .eval(
                    "fmt.format(message)",
                    None,
                    Some(
                        [
                            (intern!(py, "fmt"), fmt.clone()),
                            (intern!(py, "message"), message.clone()),
                        ]
                        .into_py_dict(py),
                    ),
                )?
                .extract::<String>()?;

            let next_frame_reply: Py<PyAny> = PyModule::from_code(
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
                    for &cid in cids {
                        next_frame_reply.call1(py, (cid, &server_command))?;
                    }
                }
            }

            if let Some(color_tag) = re_color_tag.find_iter(&message).last() {
                last_color = color_tag.as_str().into();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod chat_channel_tests {
    use crate::ffi::python::prelude::*;

    use pyo3::exceptions::PyNotImplementedError;
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn console_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let chat_channel_constructor = py.run(
                r#"
import _shinqlx
chat_channel = _shinqlx.ChatChannel()
            "#,
                None,
                None,
            );
            assert!(chat_channel_constructor.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn receipients_is_not_implemented() {
        Python::with_gil(|py| {
            let chat_channel = ChatChannel {
                fmt: r#"print"{}\n"\n"#.into(),
            };
            let result = chat_channel.receipients();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
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
        PyClassInitializer::from(AbstractChannel::py_new("tell".into()))
            .add_subclass(ChatChannel {
                fmt: r#"print "{}\n"\n"#.into(),
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

    fn receipients(&self) -> PyResult<Option<Vec<i32>>> {
        Ok(Some(vec![self.client_id]))
    }
}

#[cfg(test)]
mod tell_channel_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    fn default_test_player() -> Player {
        Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "".into(),
                connection_state: clientState_t::CS_CONNECTED as i32,
                userinfo: "".into(),
                steam_id: 1234567890,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: privileges_t::PRIV_NONE as i32,
            },
            user_info: "".into(),
            steam_id: 1234567890,
            name: "".into(),
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let tell_channel_constructor = py.run(
                r#"
import _shinqlx
tell_channel = _shinqlx.TellChannel(player)
            "#,
                None,
                Some(vec![("player", player.into_py(py))].into_py_dict(py)),
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

    #[test]
    fn receipients_returns_vec_with_client_id() {
        let tell_channel = TellChannel { client_id: 42 };
        assert!(tell_channel
            .receipients()
            .is_ok_and(|receipients| receipients == Some(vec![42,])));
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
    #[pyo3(signature = (team="all".into(), name="chat".into(), fmt="print \"{}\n\"\n".into()))]
    pub(crate) fn py_new(team: String, name: String, fmt: String) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name))
            .add_subclass(ChatChannel { fmt })
            .add_subclass(Self { team })
    }

    fn receipients(&self, py: Python<'_>) -> PyResult<Option<Vec<i32>>> {
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
            let team_chat_channel_constructor = py.run(
                r#"
import _shinqlx
tell_channel = _shinqlx.TeamChatChannel("all")
            "#,
                None,
                None,
            );
            assert!(team_chat_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[case("all".into(), None)]
    #[case("red".into(), Some(vec![1, 5]))]
    #[case("blue".into(), Some(vec![2, 6]))]
    #[case("spectator".into(), Some(vec![3, 7]))]
    #[case("free".into(), Some(vec![0, 4]))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recipients_returns_client_ids(#[case] team: String, #[case] expected_ids: Option<Vec<i32>>) {
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
                    .returning(|| "Mocked Player".into());
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

        let team_chat_channel = TeamChatChannel { team };
        let result = Python::with_gil(|py| team_chat_channel.receipients(py));
        assert!(result.is_ok_and(|ids| ids == expected_ids));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recipients_for_invalid_team_chat_channel_name() {
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
                    .returning(|| "Mocked Player".into());
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
            team: "invalid".into(),
        };
        let result = Python::with_gil(|py| team_chat_channel.receipients(py));
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
        PyClassInitializer::from(AbstractChannel::py_new("client_command".into())).add_subclass(
            Self {
                client_id: player.id,
            },
        )
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

    #[pyo3(signature = (msg, limit=100, delimiter="".into()))]
    fn reply(&self, py: Python<'_>, msg: String, limit: i32, delimiter: String) -> PyResult<()> {
        let tell_channel = Py::new(
            py,
            PyClassInitializer::from(AbstractChannel::py_new("tell".into()))
                .add_subclass(ChatChannel {
                    fmt: r#"print "{}\n"\n"#.into(),
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
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    fn default_test_player() -> Player {
        Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "".into(),
                connection_state: clientState_t::CS_CONNECTED as i32,
                userinfo: "".into(),
                steam_id: 1234567890,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: privileges_t::PRIV_NONE as i32,
            },
            user_info: "".into(),
            steam_id: 1234567890,
            name: "".into(),
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_command_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let client_command_channel_constructor = py.run(
                r#"
import _shinqlx
tell_channel = _shinqlx.ClientCommandChannel(player)
            "#,
                None,
                Some(vec![("player", player.into_py(py))].into_py_dict(py)),
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

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_tell_channel_returns_tell_channel_with_client_id() {
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
