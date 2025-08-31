use derive_more::Display;
use pyo3::{
    BoundObject,
    basic::CompareOp,
    exceptions::PyNotImplementedError,
    intern,
    types::{IntoPyDict, PyBool, PyDict, PyNotImplemented, PyType},
};
use rayon::prelude::*;
use regex::Regex;
use tap::TapOptional;

use super::{Teams, prelude::*};

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
#[pyclass(module = "_commands", name = "AbstractChannel", subclass, frozen, str)]
#[derive(Display)]
#[display("{}", name.read())]
pub(crate) struct AbstractChannel {
    name: parking_lot::RwLock<String>,
}

#[pymethods]
impl AbstractChannel {
    #[new]
    #[pyo3(signature = (name = "AbstractChannel", *args, **kwargs), text_signature = "(name)")]
    fn py_new(
        name: &str,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> Self {
        AbstractChannel {
            name: name.to_string().into(),
        }
    }

    #[pyo3(name = "__init__")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, name: &str) {
        *slf.get().name.write() = name.to_string();
    }

    fn __repr__(&self) -> String {
        format!("{self}")
    }

    fn __richcmp__<'py>(
        &self,
        other: &Bound<'py, PyAny>,
        op: CompareOp,
        py: Python<'py>,
    ) -> Borrowed<'py, 'py, PyAny> {
        match op {
            CompareOp::Eq => other.str().map(|other_str| other_str.to_string()).map_or(
                other
                    .repr()
                    .map_or(PyBool::new(py, false).into_any(), |other_repr| {
                        PyBool::new(py, self.__repr__() == other_repr.to_string()).into_any()
                    }),
                |other_channel| PyBool::new(py, *self.name.read() == other_channel).into_any(),
            ),
            CompareOp::Ne => other.str().map(|other_str| other_str.to_string()).map_or(
                other
                    .repr()
                    .map_or(PyBool::new(py, true).into_any(), |other_repr| {
                        PyBool::new(py, self.__repr__() != other_repr.to_string()).into_any()
                    }),
                |other_channel| PyBool::new(py, *self.name.read() != other_channel).into_any(),
            ),
            _ => PyNotImplemented::get(py).into_any(),
        }
    }

    #[getter(name)]
    fn get_name(slf: &Bound<'_, Self>) -> String {
        slf.get_name()
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")"
    )]
    fn reply(
        slf: &Bound<'_, Self>,
        #[allow(unused_variables)] msg: &str,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: &str,
    ) -> PyResult<()> {
        slf.reply(msg, limit, delimiter)
    }

    #[classmethod]
    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")"
    )]
    fn split_long_lines(
        cls: &Bound<'_, PyType>,
        msg: &str,
        limit: i32,
        delimiter: &str,
    ) -> Vec<String> {
        cls.py().detach(|| {
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
        })
    }
}

pub(crate) trait AbstractChannelMethods {
    fn get_name(&self) -> String;
    fn reply(&self, msg: &str, limit: i32, delimiter: &str) -> PyResult<()>;
}

impl AbstractChannelMethods for Bound<'_, AbstractChannel> {
    fn get_name(&self) -> String {
        self.get().name.read().to_owned()
    }

    fn reply(
        &self,
        #[allow(unused_variables)] msg: &str,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: &str,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not implemented"))
    }
}

#[cfg(test)]
mod abstract_channel_tests {
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyNotImplementedError, PyTypeError};
    use rstest::*;

    use super::AbstractChannelMethods;
    use crate::{ffi::python::prelude::*, prelude::serial};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abstract_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_channel_constructor = py.run(
                cr#"
import shinqlx
shinqlx.AbstractChannel("abstract")
            "#,
                None,
                None,
            );
            assert!(abstract_channel_constructor.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abstract_channel_can_be_subclassed(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            py.run(
                cr#"
from shinqlx import AbstractChannel

class ConcreteChannel(AbstractChannel):
    def __init__(self, str_attr, int_attr):
        self.str_attr = str_attr
        self.int_attr = int_attr

        super().__init__("ConcreteChannel")

ConcreteChannel("asdf", 42)
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abstract_channel_repr_representation(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_channel_repr_assert = py.run(
                cr#"
import shinqlx
assert repr(shinqlx.AbstractChannel("abstract")) == "abstract"
            "#,
                None,
                None,
            );
            assert!(abstract_channel_repr_assert.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abstract_channel_str_representation(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_channel_str_assert = py.run(
                cr#"
import shinqlx
assert str(shinqlx.AbstractChannel("abstract")) == "abstract"
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
        Python::attach(|py| {
            let abstract_channel_eq_assert = py.run(
                cr#"
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
        Python::attach(|py| {
            let abstract_channel_ne_assert = py.run(
                cr#"
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
        Python::attach(|py| {
            let abstract_channel_cmp_assert = py.run(
                cr#"
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_name(_pyshinqlx_setup: ()) {
        let abstract_channel = AbstractChannel {
            name: "abstract".to_string().into(),
        };

        Python::attach(|py| {
            let bound_channel = Bound::new(py, abstract_channel).expect("this should not happen");
            assert_eq!(bound_channel.get_name(), "abstract");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn reply_is_not_implemented(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let abstract_channel = Bound::new(
                py,
                AbstractChannel::py_new("abstract", py.None().bind(py), None),
            )
            .expect("this should not happen");
            let result = abstract_channel.reply("asdf", 100, " ");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn split_long_lines_with_short_string(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            AbstractChannel::split_long_lines(&py.get_type::<AbstractChannel>(), "short", 100, " ")
        });

        assert_eq!(result, vec!["short".to_string()]);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn split_long_lines_with_string_that_is_split(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            AbstractChannel::split_long_lines(
                &py.get_type::<AbstractChannel>(),
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
        let result = Python::attach(|py| {
            AbstractChannel::split_long_lines(
                &py.get_type::<AbstractChannel>(),
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
    #[pyo3(signature = (*args, **kwargs), text_signature = "()")]
    pub(crate) fn py_new(
        py: Python<'_>,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("console", py.None().bind(py), None))
            .add_subclass(Self {})
    }

    #[pyo3(name = "__init__")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>) {
        AbstractChannel::initialize(slf.as_super(), "console");
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")"
    )]
    pub(crate) fn reply(
        slf: &Bound<'_, Self>,
        msg: &str,
        limit: i32,
        delimiter: &str,
    ) -> PyResult<()> {
        slf.reply(msg, limit, delimiter)
    }
}

impl AbstractChannelMethods for Bound<'_, ConsoleChannel> {
    fn get_name(&self) -> String {
        self.as_super().get_name()
    }

    fn reply(
        &self,
        msg: &str,
        #[allow(unused_variables)] limit: i32,
        #[allow(unused_variables)] delimiter: &str,
    ) -> PyResult<()> {
        pyshinqlx_console_print(self.py(), msg);
        Ok(())
    }
}

#[cfg(test)]
mod console_channel_tests {
    use mockall::predicate;
    use rstest::*;

    use crate::{
        ffi::python::prelude::*, hooks::mock_hooks::shinqlx_com_printf_context, prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn console_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let console_channel_constructor = py.run(
                cr#"
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
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_prints_text_to_console(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        let result = Python::attach(|py| {
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                    .expect("this shouöld not happen");
            console_channel.reply("asdf", 100, " ")
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
)]
pub(crate) struct ChatChannel {
    fmt: parking_lot::RwLock<String>,
}

#[pymethods]
impl ChatChannel {
    #[new]
    #[pyo3(signature = (name = "chat", fmt = "print \"{}\n\"\n", *args, **kwargs), text_signature = "(name = \"chat\", fmt = \"print \"{}\n\"\n\")")]
    pub(crate) fn py_new(
        py: Python<'_>,
        name: &str,
        fmt: &str,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name, py.None().bind(py), None))
            .add_subclass(Self {
                fmt: fmt.to_string().into(),
            })
    }

    #[pyo3(name = "__init__", signature = (name = "chat", fmt = "print \"{}\n\"\n"), text_signature = "(name = \"chat\", fmt = \"print \"{}\n\"\n\")"
    )]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, name: &str, fmt: &str) {
        *slf.get().fmt.write() = fmt.into();
        AbstractChannel::initialize(slf.as_super(), name);
    }

    #[getter(fmt)]
    fn get_fmt(slf: Bound<'_, Self>) -> String {
        slf.get().fmt.read().to_owned()
    }

    fn recipients(slf: &Bound<'_, Self>) -> PyResult<Option<Vec<i32>>> {
        slf.recipients()
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")"
    )]
    pub(crate) fn reply(
        slf: &Bound<'_, Self>,
        msg: &str,
        limit: i32,
        delimiter: &str,
    ) -> PyResult<()> {
        slf.reply(msg, limit, delimiter)
    }
}

impl AbstractChannelMethods for Bound<'_, ChatChannel> {
    fn get_name(&self) -> String {
        self.as_super().get_name()
    }

    fn reply(&self, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        let re_color_tag = Regex::new(r"\^[0-7]").unwrap();
        let fmt = self.get().fmt.read().to_owned();
        let cleaned_msg = msg.replace('"', "'");
        let targets: Option<Vec<i32>> = self
            .call_method0(intern!(self.py(), "recipients"))?
            .extract()?;

        let split_msgs = AbstractChannel::split_long_lines(
            &self.py().get_type::<AbstractChannel>(),
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
                    if s_new.len() > MAX_MSG_LENGTH as usize {
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
            let server_command = self
                .py()
                .eval(
                    c"fmt.format(message)",
                    None,
                    Some(
                        &[
                            (intern!(self.py(), "fmt"), fmt.to_owned()),
                            (intern!(self.py(), "message"), message.to_owned()),
                        ]
                        .into_py_dict(self.py())?,
                    ),
                )?
                .extract::<String>()?;

            let next_frame_tasks = self
                .py()
                .import(intern!(self.py(), "shinqlx"))?
                .getattr(intern!(self.py(), "next_frame_tasks"))?;
            let called_func = wrap_pyfunction!(pyshinqlx_send_server_command, self.py())?;
            match targets {
                None => {
                    next_frame_tasks.call_method1(
                        intern!(self.py(), "put_nowait"),
                        ((
                            &called_func,
                            (self.py().None(), &server_command),
                            PyDict::new(self.py()),
                        ),),
                    )?;
                }
                Some(ref cids) => {
                    cids.iter()
                        .map(|&cid| {
                            next_frame_tasks.call_method1(
                                intern!(self.py(), "put_nowait"),
                                ((&called_func, (cid, &server_command), PyDict::new(self.py())),),
                            )
                        })
                        .collect::<PyResult<Vec<_>>>()?;
                }
            }

            re_color_tag
                .find_iter(&message)
                .last()
                .tap_some(|color_tag| {
                    last_color = color_tag.as_str().to_string();
                });
        }

        Ok(())
    }
}

pub(crate) trait ChatChannelMethods {
    fn recipients(&self) -> PyResult<Option<Vec<i32>>>;
}

impl ChatChannelMethods for Bound<'_, ChatChannel> {
    fn recipients(&self) -> PyResult<Option<Vec<i32>>> {
        Err(PyNotImplementedError::new_err("ChatChannel recipients"))
    }
}

#[cfg(test)]
mod chat_channel_tests {
    use pyo3::{
        exceptions::{PyNotImplementedError, PyValueError},
        prelude::*,
    };
    use rstest::*;

    use crate::{
        ffi::{
            c::prelude::{MockClient, clientState_t},
            python::{
                prelude::*,
                pyshinqlx_test_support::{
                    default_test_player, default_test_player_info, run_all_frame_tasks,
                },
            },
        },
        hooks::mock_hooks::shinqlx_send_server_command_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn chat_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let chat_channel_constructor = py.run(
                cr#"
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
        Python::attach(|py| {
            let chat_channel = Bound::new(
                py,
                ChatChannel::py_new(py, "chat", "print \"{}\n\"\n", py.None().bind(py), None),
            )
            .expect("this should not happen");
            let result = chat_channel.recipients();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn chat_channel_subclasses_can_overwrite_recipients(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let test_reply_to_recipients = py.run(
                cr#"
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
                test_reply_to_recipients.is_err_and(|err| err.is_instance_of::<PyValueError>(py))
            );
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_default_limit_and_delimiter(_pyshinqlx_setup: ()) {
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
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let tell_channel = Bound::new(
                    py,
                    TellChannel::py_new(py, &player, py.None().bind(py), None),
                )
                .expect("this should not happen");
                let result = tell_channel.as_super().reply("asdf", 100, " ");
                assert!(result.is_ok());

                let _ = run_all_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_custom_limit_param(_pyshinqlx_setup: ()) {
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
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let tell_channel = Bound::new(
                    py,
                    TellChannel::py_new(py, &player, py.None().bind(py), None),
                )
                .expect("this should not happen");

                let result = tell_channel
                    .as_super()
                    .reply("These are four lines", 5, " ");
                assert!(result.is_ok());

                let _ = run_all_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_custom_delimiter_parameter(_pyshinqlx_setup: ()) {
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
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let chat_channel = Bound::new(
                    py,
                    TellChannel::py_new(py, &player, py.None().bind(py), None),
                )
                .expect("this should not happen");

                let result = chat_channel
                    .as_super()
                    .reply("These_are_four_lines", 5, "_");
                assert!(result.is_ok());

                let _ = run_all_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_various_color_tags(_pyshinqlx_setup: ()) {
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
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let chat_channel = Bound::new(
                    py,
                    TellChannel::py_new(py, &player, py.None().bind(py), None),
                )
                .expect("this should not happen");

                let result = chat_channel.as_super().reply(
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
        });
    }
}

#[pyclass(
    extends = ChatChannel,
    module = "_commands",
    name = "TellChannel",
    subclass,
    frozen,
)]
pub(crate) struct TellChannel {
    client_id: parking_lot::RwLock<i32>,
}

#[pymethods]
impl TellChannel {
    #[new]
    #[pyo3(signature = (player, *args, **kwargs), text_signature = "(player)")]
    pub(crate) fn py_new(
        py: Python<'_>,
        player: &Player,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new("tell", py.None().bind(py), None))
            .add_subclass(ChatChannel {
                fmt: "print \"{}\n\"\n".to_string().into(),
            })
            .add_subclass(Self {
                client_id: player.id.into(),
            })
    }

    #[pyo3(name = "__init__")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, player: &Player) {
        *slf.get().client_id.write() = player.id;
        AbstractChannel::initialize(slf.as_super().as_super(), "tell");
    }

    fn __repr__(&self) -> String {
        format!("tell {}", self.client_id.read())
    }

    #[getter(client_id)]
    fn get_client_id(slf: &Bound<'_, Self>) -> i32 {
        *slf.get().client_id.read()
    }

    #[getter(recipient)]
    fn get_recipient(slf: &Bound<'_, Self>) -> PyResult<Player> {
        slf.get_recipient()
    }

    fn recipients(slf: &Bound<'_, Self>) -> PyResult<Option<Vec<i32>>> {
        slf.recipients()
    }
}

impl AbstractChannelMethods for Bound<'_, TellChannel> {
    fn get_name(&self) -> String {
        self.as_super().get_name()
    }

    fn reply(&self, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        self.as_super().reply(msg, limit, delimiter)
    }
}

impl ChatChannelMethods for Bound<'_, TellChannel> {
    fn recipients(&self) -> PyResult<Option<Vec<i32>>> {
        Ok(Some(vec![*self.get().client_id.read()]))
    }
}

pub(crate) trait TellChannelMethods {
    fn get_recipient(&self) -> PyResult<Player>;
}

impl TellChannelMethods for Bound<'_, TellChannel> {
    fn get_recipient(&self) -> PyResult<Player> {
        Player::py_new(*self.get().client_id.read(), None)
    }
}

#[cfg(test)]
mod tell_channel_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    use crate::{
        ffi::{
            c::prelude::*,
            python::{prelude::*, pyshinqlx_test_support::*},
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::attach(|py| {
            let tell_channel_constructor = py.run(
                cr#"
import shinqlx
tell_channel = shinqlx.TellChannel(player)
            "#,
                None,
                Some(
                    &vec![(
                        "player",
                        Bound::new(py, player).expect("this should not happen"),
                    )]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert!(tell_channel_constructor.is_ok());
        });
    }

    #[test]
    #[serial]
    fn repr_returns_tell_client_id() {
        let tell_channel = TellChannel {
            client_id: 42.into(),
        };
        assert_eq!(tell_channel.__repr__(), "tell 42");
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_recipient_returns_player_with_client_id(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
            .with_team(|| team_t::TEAM_SPECTATOR, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let tell_channel = PyClassInitializer::from(AbstractChannel::py_new(
                        "tell",
                        py.None().bind(py),
                        None,
                    ))
                    .add_subclass(ChatChannel {
                        fmt: "print \"{}\n\"\n".to_string().into(),
                    })
                    .add_subclass(TellChannel {
                        client_id: 42.into(),
                    });

                    assert!(
                        Bound::new(py, tell_channel)
                            .expect("this should not happen")
                            .get_recipient()
                            .is_ok_and(|player| player.id == 42)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn recipients_returns_vec_with_client_id(_pyshinqlx_setup: ()) {
        let player = default_test_player();
        Python::attach(|py| {
            let py_tell_channel = Bound::new(
                py,
                TellChannel::py_new(py, &player, py.None().bind(py), None),
            )
            .expect("this should not happen");
            assert!(
                py_tell_channel
                    .recipients()
                    .is_ok_and(|recipients| recipients == Some(vec![2,]))
            );
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
)]
pub(crate) struct TeamChatChannel {
    team: parking_lot::RwLock<String>,
}

#[pymethods]
impl TeamChatChannel {
    #[new]
    #[pyo3(signature = (team="all", name="chat", fmt="print \"{}\n\"\n", *args, **kwargs), text_signature = "(team=\"all\", name=\"chat\", fmt=\"print \"{}\n\"\n\")")]
    pub(crate) fn py_new(
        py: Python<'_>,
        team: &str,
        name: &str,
        fmt: &str,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(name, py.None().bind(py), None))
            .add_subclass(ChatChannel {
                fmt: fmt.to_string().into(),
            })
            .add_subclass(Self {
                team: team.to_string().into(),
            })
    }

    #[pyo3(name = "__init__", signature = (team="all", name="chat", fmt="print \"{}\n\"\n"), text_signature = "(team=\"all\", name=\"chat\", fmt=\"print \"{}\n\"\n\")"
    )]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, team: &str, name: &str, fmt: &str) {
        *slf.get().team.write() = team.to_string();
        ChatChannel::initialize(slf.as_super(), name, fmt);
    }

    #[getter(team)]
    fn get_team(slf: &Bound<'_, Self>) -> String {
        slf.get().team.read().to_owned()
    }

    fn recipients(slf: &Bound<'_, Self>) -> PyResult<Option<Vec<i32>>> {
        slf.recipients()
    }
}

impl AbstractChannelMethods for Bound<'_, TeamChatChannel> {
    fn get_name(&self) -> String {
        self.as_super().get_name()
    }

    fn reply(&self, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        self.as_super().reply(msg, limit, delimiter)
    }
}

impl ChatChannelMethods for Bound<'_, TeamChatChannel> {
    fn recipients(&self) -> PyResult<Option<Vec<i32>>> {
        if *self.get().team.read() == "all" {
            return Ok(None);
        }

        let filtered_team = Teams::from(self.get().team.read().as_str()) as i32;
        let players_info = pyshinqlx_players_info(self.py())?;
        Ok(Some(
            players_info
                .par_iter()
                .filter_map(|opt_player_info| match opt_player_info.as_ref() {
                    Some(player_info) if player_info.team == filtered_team => {
                        Some(player_info.client_id)
                    }
                    _ => None,
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod team_chat_channel_tests {
    use rstest::*;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn team_chat_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let team_chat_channel_constructor = py.run(
                cr#"
import shinqlx
tell_channel = shinqlx.TeamChatChannel()
tell_channel.__init__("all")
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
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn recipients_returns_client_ids(
        _pyshinqlx_setup: (),
        #[case] team: &str,
        #[case] expected_ids: Option<Vec<i32>>,
    ) {
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
                        0 | 4 => team_t::TEAM_FREE,
                        1 | 5 => team_t::TEAM_RED,
                        2 | 6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default().with_max_clients(8).run(|| {
            let result = Python::attach(|py| {
                let team_chat_channel = Bound::new(
                    py,
                    TeamChatChannel::py_new(
                        py,
                        team,
                        "chat",
                        "print \"{}\n\"\n",
                        py.None().bind(py),
                        None,
                    ),
                )
                .expect("this should not happen");
                team_chat_channel.recipients()
            });
            assert!(result.is_ok_and(|ids| ids == expected_ids));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn recipients_for_invalid_team_chat_channel_name(_pyshinqlx_setup: ()) {
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
                        0 | 4 => team_t::TEAM_FREE,
                        1 | 5 => team_t::TEAM_RED,
                        2 | 6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default().with_max_clients(8).run(|| {
            let result = Python::attach(|py| {
                let team_chat_channel = Bound::new(
                    py,
                    TeamChatChannel::py_new(
                        py,
                        "invalid",
                        "chat",
                        "print \"{}\n\"\n",
                        py.None().bind(py),
                        None,
                    ),
                )
                .expect("this should not happen");
                team_chat_channel.recipients()
            });
            assert!(result.is_ok_and(|ids| ids == Some(vec![])));
        });
    }
}

/// Wraps a TellChannel, but with its own name.
#[pyclass(
    extends = AbstractChannel,
    module = "_commands",
    name = "ClientCommandChannel",
    subclass,
    frozen,
)]
pub(crate) struct ClientCommandChannel {
    client_id: parking_lot::RwLock<i32>,
}

#[pymethods]
impl ClientCommandChannel {
    #[new]
    #[pyo3(signature = (player, *args, **kwargs), text_signature = "(player)")]
    pub(crate) fn py_new(
        py: Python<'_>,
        player: &Player,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AbstractChannel::py_new(
            "client_command",
            py.None().bind(py),
            None,
        ))
        .add_subclass(Self {
            client_id: player.id.into(),
        })
    }

    #[pyo3(name = "__init__")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, player: &Player) {
        *slf.get().client_id.write() = player.id;
        AbstractChannel::initialize(slf.as_super(), "client_command");
    }

    fn __repr__(&self) -> String {
        format!("client_command {}", self.client_id.read())
    }

    #[getter(client_id)]
    fn get_client_id(slf: &Bound<'_, Self>) -> i32 {
        *slf.get().client_id.read()
    }

    #[getter(recipient)]
    fn get_recipient(slf: &Bound<'_, Self>) -> PyResult<Player> {
        slf.get_recipient()
    }

    #[getter(tell_channel)]
    fn get_tell_channel(slf: &Bound<'_, Self>) -> PyResult<Py<TellChannel>> {
        slf.get_tell_channel()
    }

    #[pyo3(signature = (msg, limit=100, delimiter=" "), text_signature = "(msg, limit=100, delimiter=\" \")")]
    fn reply(slf: &Bound<'_, Self>, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        slf.reply(msg, limit, delimiter)
    }
}

impl AbstractChannelMethods for Bound<'_, ClientCommandChannel> {
    fn get_name(&self) -> String {
        self.as_super().get_name()
    }

    fn reply(&self, msg: &str, limit: i32, delimiter: &str) -> PyResult<()> {
        let tell_channel = Bound::new(
            self.py(),
            PyClassInitializer::from(AbstractChannel {
                name: "tell".to_string().into(),
            })
            .add_subclass(ChatChannel {
                fmt: "print \"{}\n\"\n".to_string().into(),
            })
            .add_subclass(TellChannel {
                client_id: (*self.get().client_id.read()).into(),
            }),
        )?;

        tell_channel.as_super().reply(msg, limit, delimiter)
    }
}

impl TellChannelMethods for Bound<'_, ClientCommandChannel> {
    fn get_recipient(&self) -> PyResult<Player> {
        Player::py_new(*self.get().client_id.read(), None)
    }
}
pub(crate) trait ClientCommandChannelMethods {
    fn get_tell_channel(&self) -> PyResult<Py<TellChannel>>;
}

impl ClientCommandChannelMethods for Bound<'_, ClientCommandChannel> {
    fn get_tell_channel(&self) -> PyResult<Py<TellChannel>> {
        let player = self.get_recipient()?;
        let channel = Bound::new(
            self.py(),
            TellChannel::py_new(self.py(), &player, self.py().None().bind(self.py()), None),
        )?;

        Ok(channel.unbind())
    }
}

#[cfg(test)]
mod client_command_channel_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::types::IntoPyDict;
    use rstest::rstest;

    use crate::{
        ffi::{
            c::prelude::*,
            python::{prelude::*, pyshinqlx_test_support::*},
        },
        hooks::mock_hooks::shinqlx_send_server_command_context,
        prelude::*,
    };

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn client_command_channel_can_be_created_from_python(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::attach(|py| {
            let client_command_channel_constructor = py.run(
                cr#"
import shinqlx
shinqlx.ClientCommandChannel(player)
            "#,
                None,
                Some(
                    &vec![(
                        "player",
                        Bound::new(py, player).expect("this should not happen"),
                    )]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert!(client_command_channel_constructor.is_ok());
        });
    }

    #[test]
    fn repr_returns_tell_client_id() {
        let client_command_channel = ClientCommandChannel {
            client_id: 42.into(),
        };
        assert_eq!(client_command_channel.__repr__(), "client_command 42");
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_recipient_returns_player_with_client_id(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
            .with_team(|| team_t::TEAM_SPECTATOR, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let client_command_channel = PyClassInitializer::from(AbstractChannel::py_new(
                        "client_command",
                        py.None().bind(py),
                        None,
                    ))
                    .add_subclass(ClientCommandChannel {
                        client_id: 42.into(),
                    });
                    let py_client_command_channel =
                        Bound::new(py, client_command_channel).expect("this should not happen");
                    assert!(
                        py_client_command_channel
                            .get_recipient()
                            .is_ok_and(|player| player.id == 42)
                    );
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_tell_channel_returns_tell_channel_with_client_id(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
            .with_team(|| team_t::TEAM_SPECTATOR, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let client_command_channel = PyClassInitializer::from(AbstractChannel::py_new(
                        "client_command",
                        py.None().bind(py),
                        None,
                    ))
                    .add_subclass(ClientCommandChannel {
                        client_id: 42.into(),
                    });
                    let py_client_command_channel =
                        Bound::new(py, client_command_channel).expect("this should not happen");
                    let result = py_client_command_channel.get_tell_channel();
                    assert!(
                        result.is_ok_and(|tell_channel| *tell_channel.get().client_id.read() == 42)
                    );
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reply_with_default_limit_and_delimiter(_pyshinqlx_setup: ()) {
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
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let client_command_channel = Bound::new(
                    py,
                    ClientCommandChannel::py_new(py, &player, py.None().bind(py), None),
                )
                .expect("this should not happen");

                let result = client_command_channel.reply("asdf", 100, " ");
                assert!(result.is_ok());

                let _ = run_all_frame_tasks(py);
            });
        });
    }
}
