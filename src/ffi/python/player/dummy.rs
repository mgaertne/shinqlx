use core::hint::cold_path;

use pyo3::{
    exceptions::{PyAttributeError, PyEnvironmentError, PyNotImplementedError},
    types::PyDict,
};

use super::{
    super::{CONSOLE_CHANNEL, owner, prelude::*},
    Player,
};
use crate::ffi::c::prelude::*;

static _DUMMY_USERINFO: &str = r#"
\ui_singlePlayerActive\0
\cg_autoAction\1
\cg_autoHop\0
\cg_predictItems\1
\model\bitterman/sport_blue
\headmodel\crash/red
\handicap\100
\cl_anonymous\0
\color1\4\color2\23
\sex\male
\teamtask\0
\rate\25000
\country\NO"#;

#[pyclass(module = "_player", name = "AbstractDummyPlayer", extends = Player, subclass, frozen)]
pub(crate) struct AbstractDummyPlayer;

#[pymethods]
impl AbstractDummyPlayer {
    #[new]
    #[pyo3(signature = (name = "DummyPlayer", *args, **kwargs), text_signature = "(name = \"DummyPlayer\")")]
    fn py_new(
        name: &str,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        args: &Bound<'_, PyAny>,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        let player_info = PlayerInfo {
            client_id: -1,
            name: name.to_string(),
            connection_state: clientState_t::CS_CONNECTED as i32,
            userinfo: _DUMMY_USERINFO.to_string(),
            steam_id: 0,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };
        PyClassInitializer::from(Player::from_client_id(-1, Some(player_info)))
            .add_subclass(AbstractDummyPlayer {})
    }

    #[pyo3(name = "__init__", signature = (name = "DummyPlayer"))]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, name: &str) {
        let player = slf.as_super();
        *player.get().name.write() = name.into();
    }

    #[getter(id)]
    fn get_id(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_id()
    }

    #[getter(steam_id)]
    fn get_steam_id(slf: &Bound<'_, Self>) -> PyResult<i64> {
        slf.get_steam_id()
    }

    fn update(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.update()
    }

    #[getter(channel)]
    fn get_channel<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_channel()
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell(slf: &Bound<'_, Self>, msg: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        slf.tell(msg, kwargs)
    }
}

pub(crate) trait AbstractDummyPlayerMethods<'py> {
    fn get_id(&self) -> PyResult<i32>;
    fn get_steam_id(&self) -> PyResult<i64>;
    fn update(&self) -> PyResult<()>;
    fn get_channel(&self) -> PyResult<Bound<'py, PyAny>>;
    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()>;
}

impl<'py> AbstractDummyPlayerMethods<'py> for Bound<'py, AbstractDummyPlayer> {
    fn get_id(&self) -> PyResult<i32> {
        Err(PyAttributeError::new_err(
            "Dummy players do not have client IDs.",
        ))
    }

    fn get_steam_id(&self) -> PyResult<i64> {
        Err(PyNotImplementedError::new_err(
            "steam_id property needs to be implemented.",
        ))
    }

    fn update(&self) -> PyResult<()> {
        Ok(())
    }

    fn get_channel(&self) -> PyResult<Bound<'py, PyAny>> {
        Err(PyNotImplementedError::new_err(
            "channel property needs to be implemented.",
        ))
    }

    fn tell(
        &self,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        msg: &str,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "tell() needs to be implemented.",
        ))
    }
}

#[cfg(test)]
mod pyshinqlx_abstract_dummy_player_tests {
    use pyo3::exceptions::{PyAttributeError, PyNotImplementedError};
    use rstest::*;

    use super::{AbstractDummyPlayer, AbstractDummyPlayerMethods};
    use crate::{
        ffi::python::prelude::*,
        prelude::{MockEngineBuilder, serial},
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dummy_player_is_a_player_instance(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            py.run(
                cr#"
import shinqlx
assert(isinstance(shinqlx.AbstractDummyPlayer(), shinqlx.Player))
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
    fn dummy_player_can_be_subclassed(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::attach(|py| {
                py.run(
                    cr#"
from shinqlx import AbstractDummyPlayer

class ConcreteDummyPlayer(AbstractDummyPlayer):
    def __init__(self, name):
        super().__init__(name)

player = ConcreteDummyPlayer("asdf")
            "#,
                    None,
                    None,
                )
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_id_returns_attribute_error(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_id();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_steam_id_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_steam_id();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn update_does_nothing(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.update();

            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_channel_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_channel();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.tell("asdf", None);

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn the_abstract_methods_report_their_errors_to_python(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

player = shinqlx.AbstractDummyPlayer("dummy")

for attribute, error in (
    ("id", AttributeError),
    ("steam_id", NotImplementedError),
    ("channel", NotImplementedError),
):
    try:
        getattr(player, attribute)
    except error:
        pass
    else:
        raise AssertionError(attribute)

assert player.update() is None

try:
    player.tell("asdf")
except NotImplementedError:
    pass
else:
    raise AssertionError("tell")
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok(), "{result:?}");
    }
}

#[pyclass(module = "_player", name = "RconDummyPlayer", extends = AbstractDummyPlayer, frozen)]
pub(crate) struct RconDummyPlayer;

#[pymethods]
impl RconDummyPlayer {
    #[new]
    #[pyo3(signature = (*args, **kwargs), text_signature = "()")]
    pub(crate) fn py_new(
        py: Python<'_>,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        args: &Bound<'_, PyAny>,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        AbstractDummyPlayer::py_new("RconDummyPlayer", py.None().bind(py), None)
            .add_subclass(Self {})
    }

    #[pyo3(name = "__init__")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>) {
        let player = slf.as_super().as_super();
        *player.get().name.write() = "RconDummyPlayer".into();
    }

    #[getter(steam_id)]
    fn get_steam_id(slf: &Bound<'_, Self>) -> PyResult<i64> {
        slf.get_steam_id()
    }

    #[getter(channel)]
    fn get_channel<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, ConsoleChannel>> {
        slf.get_channel()
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell<'py>(
        slf: &Bound<'py, Self>,
        msg: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        slf.tell(msg, kwargs)
    }
}

pub(crate) trait RconDummyPlayerMethods<'py> {
    fn get_steam_id(&self) -> PyResult<i64>;
    fn get_channel(&self) -> PyResult<Bound<'py, ConsoleChannel>>;
    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()>;
}

impl<'py> RconDummyPlayerMethods<'py> for Bound<'py, RconDummyPlayer> {
    fn get_steam_id(&self) -> PyResult<i64> {
        self.py()
            .detach(|| owner().map(|opt_value| opt_value.unwrap_or_default()))
    }

    fn get_channel(&self) -> PyResult<Bound<'py, ConsoleChannel>> {
        CONSOLE_CHANNEL.load().as_ref().map_or_else(
            || {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to CONSOLE_CHANNEL",
                ))
            },
            |console_channel| Ok(console_channel.bind(self.py()).to_owned()),
        )
    }

    fn tell(
        &self,
        msg: &str,
        #[allow(
            unused_variables,
            reason = "the parameter belongs to the Python signature"
        )]
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        CONSOLE_CHANNEL.load().as_ref().map_or_else(
            || {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to CONSOLE_CHANNEL",
                ))
            },
            |console_channel| console_channel.bind(self.py()).reply(msg, 100, " "),
        )
    }
}

#[cfg(test)]
mod pyshinqlx_rcon_dummy_player_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, prelude::*};
    use rstest::*;

    use super::{RconDummyPlayer, RconDummyPlayerMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{CONSOLE_CHANNEL, prelude::*},
        },
        hooks::mock_hooks::shinqlx_com_printf_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dummy_player_is_a_player_instance(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            py.run(
                cr#"
import shinqlx
assert(isinstance(shinqlx.RconDummyPlayer(), shinqlx.Player))
assert(isinstance(shinqlx.RconDummyPlayer(), shinqlx.AbstractDummyPlayer))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dummy_player_has_default_name(_pyshinqlx_setup: ()) {
        let result = Python::attach(|py| {
            py.run(
                cr#"
from shinqlx import RconDummyPlayer

rcon_player = RconDummyPlayer()

assert(rcon_player.name == "RconDummyPlayer^7")
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
    fn steam_id_return_owner_id(_pyshinqlx_setup: ()) {
        let owner = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let rcon_dummy_player =
                        Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                            .expect("this should not happen");

                    let result = rcon_dummy_player.get_steam_id();
                    assert!(result.is_ok_and(|value| value == 1234567890));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_channel_with_no_console_channel_initialized(_pyshinqlx_setup: ()) {
        CONSOLE_CHANNEL.store(None);

        Python::attach(|py| {
            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.get_channel();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_channel_with_console_channel_properly_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let console_channel = Py::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                .expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.into()));

            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.get_channel();
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_with_no_console_channel_initialized(_pyshinqlx_setup: ()) {
        CONSOLE_CHANNEL.store(None);

        Python::attach(|py| {
            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.tell("asdf", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_with_console_channel_properly_initialized(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        Python::attach(|py| {
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.unbind().into()));

            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.tell("asdf", None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn steam_id_channel_and_tell_can_be_used_from_python(_pyshinqlx_setup: ()) {
        let owner = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let console_channel =
                        Bound::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                            .expect("this should not happen");
                    CONSOLE_CHANNEL.store(Some(console_channel.unbind().into()));

                    let result = py.run(
                        cr#"
import shinqlx

player = shinqlx.RconDummyPlayer()

assert player.steam_id == 1234567890
assert player.channel is not None
assert player.tell("asdf") is None
                        "#,
                        None,
                        None,
                    );
                    assert!(result.is_ok(), "{result:?}");
                });
            });
    }
}
