use crate::ffi::python::embed::{pyshinqlx_console_print, pyshinqlx_players_info};
use crate::ffi::python::player::Player;
use crate::prelude::team_t;
use pyo3::basic::CompareOp;
use pyo3::exceptions::PyNotImplementedError;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
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
#[pyclass(subclass)]
#[pyo3(module = "shinqlx", name = "AbstractChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
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

    #[pyo3(signature = (msg, limit=100, delimiter=" ".to_string()))]
    fn reply(
        #[allow(unused_variables)] self_: PyRef<'_, Self>,
        #[allow(unused_variables)] msg: String,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: String,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not implemented"))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".to_string()))]
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
                        result.push(next_string);
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
#[cfg(not(miri))]
mod abstract_channel_tests {
    use super::AbstractChannel;
    use crate::ffi::python::pyshinqlx_setup_fixture::pyshinqlx_setup;
    use crate::prelude::*;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyNotImplementedError, PyTypeError};
    use pyo3::{Py, Python};
    use rstest::rstest;

    #[rstest]
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
    fn abstract_channel_get_name() {
        let abstract_channel = AbstractChannel {
            name: "abstract".into(),
        };
        assert_eq!(abstract_channel.get_name(), "abstract");
    }

    #[test]
    #[serial]
    fn reply_prints_text_to_console() {
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
}

/// A channel that prints to the console.
#[pyclass(extends=AbstractChannel, subclass)]
#[pyo3(module = "shinqlx", name = "ConsoleChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct ConsoleChannel {}

#[pymethods]
impl ConsoleChannel {
    #[new]
    pub(crate) fn py_new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("console".to_string()))
            .add_subclass(Self {})
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".to_string()))]
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
#[cfg(not(miri))]
mod console_channel_tests {
    use super::ConsoleChannel;
    use crate::ffi::python::pyshinqlx_setup_fixture::pyshinqlx_setup;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::{Py, Python};
    use rstest::rstest;

    #[rstest]
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

#[pyclass(extends=AbstractChannel, subclass)]
#[pyo3(module = "shinqlx", name = "ChatChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct ChatChannel {
    #[pyo3(name = "fmt")]
    fmt: String,
}

#[pymethods]
impl ChatChannel {
    #[new]
    #[pyo3(signature = (name = "chat".to_string(), fmt = "print \"{}\n\"\n".to_string()))]
    fn py_new(name: String, fmt: String) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name)).add_subclass(Self { fmt })
    }

    fn receipients(&self) -> PyResult<Option<Vec<i32>>> {
        Err(PyNotImplementedError::new_err(""))
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" ".to_string()))]
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
        let targets: Option<Vec<i32>> = self_.call_method0("receipients")?.extract()?;

        let split_msgs: Vec<String> = self_
            .call_method1("split_long_lines", (cleaned_msg, limit, delimiter))?
            .extract()?;

        let mut joined_msgs = vec![];
        for s in split_msgs {
            match joined_msgs.pop() {
                None => joined_msgs.push(s),
                Some(last_msg) => {
                    let s_new = format!("{last_msg}\n{s}");
                    if s_new.bytes().len() > 1000 {
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
            let server_command: String = py
                .eval(
                    "fmt.format(message)",
                    None,
                    Some([("fmt", fmt.clone()), ("message", message.clone())].into_py_dict(py)),
                )?
                .extract()?;

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
            .getattr("reply")?
            .into();

            match targets {
                None => {
                    next_frame_reply.call1(py, (py.None(), server_command.as_str()))?;
                }
                Some(ref cids) => {
                    for &cid in cids {
                        next_frame_reply.call1(py, (cid, server_command.as_str()))?;
                    }
                }
            }

            if let Some(color_tag) = re_color_tag.find_iter(message.as_str()).last() {
                last_color = color_tag.as_str().to_string().clone();
            }
        }

        Ok(())
    }
}

#[pyclass(extends=ChatChannel, subclass)]
#[pyo3(module = "shinqlx", name = "TellChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct TellChannel {
    client_id: i32,
}

#[pymethods]
impl TellChannel {
    #[new]
    pub(crate) fn py_new(player: &Player) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("tell".to_string()))
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

    fn receipients(&self) -> PyResult<Option<Vec<i32>>> {
        Ok(Some(vec![self.client_id]))
    }
}

/// A channel for chat to and from the server.
#[pyclass(extends=ChatChannel, subclass)]
#[pyo3(module = "shinqlx", name = "TeamChatChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct TeamChatChannel {
    team: String,
}

#[pymethods]
impl TeamChatChannel {
    #[new]
    #[pyo3(signature = (team="all".to_string(), name="chat".to_string(), fmt="print \"{}\n\"\n".to_string()))]
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
                        .into_iter()
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

/// Wraps a TellChannel, but with its own name.
#[pyclass(extends=AbstractChannel, subclass)]
#[pyo3(module = "shinqlx", name = "ClientCommandChannel", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct ClientCommandChannel {
    client_id: i32,
}

#[pymethods]
impl ClientCommandChannel {
    #[new]
    pub(crate) fn py_new(player: &Player) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("client_command".to_string()))
            .add_subclass(Self {
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

    #[pyo3(signature = (msg, limit=100, delimiter="".to_string()))]
    fn reply(&self, py: Python<'_>, msg: String, limit: i32, delimiter: String) -> PyResult<()> {
        let tell_channel = Py::new(
            py,
            PyClassInitializer::from(AbstractChannel::py_new("tell".to_string()))
                .add_subclass(ChatChannel {
                    fmt: "print \"{}\n\"\n".to_string(),
                })
                .add_subclass(TellChannel {
                    client_id: self.client_id,
                }),
        )?
        .to_object(py);

        tell_channel.call_method1(py, "reply", (msg, limit, delimiter))?;
        Ok(())
    }
}
