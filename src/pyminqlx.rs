use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use crate::commands::cmd_py_command;
#[cfg(test)]
use crate::hooks::mock_hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
#[cfg(not(test))]
use crate::hooks::{
    shinqlx_client_spawn, shinqlx_com_printf, shinqlx_drop_client, shinqlx_execute_client_command,
    shinqlx_send_server_command, shinqlx_set_configstring,
};
#[cfg(test)]
use crate::pyminqlx::DUMMY_MAIN_ENGINE as MAIN_ENGINE;
#[cfg(not(test))]
use crate::MAIN_ENGINE;
use core::default::Default;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use once_cell::sync::Lazy;
use swap_arc::SwapArcOption;

#[cfg(not(test))]
use crate::current_level::CurrentLevel;
#[cfg(test)]
use crate::current_level::MockTestCurrentLevel as CurrentLevel;
use crate::game_item::GameItem;
#[cfg(test)]
use crate::quake_live_engine::MockQuakeEngine as QuakeLiveEngine;
use crate::quake_live_engine::{
    AddCommand, ComPrintf, ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SendServerCommand,
    SetCVarForced, SetCVarLimit,
};
use pyo3::basic::CompareOp;
use pyo3::exceptions::{PyEnvironmentError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::{append_to_inittab, prepare_freethreaded_python};

#[cfg(test)]
static DUMMY_MAIN_ENGINE: Lazy<SwapArcOption<QuakeLiveEngine>> =
    Lazy::new(|| SwapArcOption::new(None));

static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    let Some(ref client_command_handler) = *CLIENT_COMMAND_HANDLER.load() else {
        return Some(cmd.as_ref().into());
    };

    Python::with_gil(
        |py| match client_command_handler.call1(py, (client_id, cmd.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "client_command_handler returned an error.");
                Some(cmd.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn server_command_dispatcher<T>(client_id: Option<i32>, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    let Some(ref server_command_handler) = *SERVER_COMMAND_HANDLER.load() else {
        return Some(cmd.as_ref().into());
    };

    Python::with_gil(|py| {
        match server_command_handler.call1(py, (client_id.unwrap_or(-1), cmd.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "server_command_handler returned an error.");
                Some(cmd.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(cmd.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(cmd.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        }
    })
}

pub(crate) fn frame_dispatcher() {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref frame_handler) = *FRAME_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| frame_handler.call0(py));
    if result.is_err() {
        error!(target: "shinqlx", "frame_handler returned an error.");
    }
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyminqlx_is_initialized() {
        return None;
    }

    let Some(ref client_connect_handler) = *PLAYER_CONNECT_HANDLER.load() else {
        return None;
    };

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | client_id as u64, Ordering::SeqCst);
    }

    let result: Option<String> =
        Python::with_gil(
            |py| match client_connect_handler.call1(py, (client_id, is_bot)) {
                Err(_) => None,
                Ok(returned) => match returned.extract::<String>(py) {
                    Err(_) => match returned.extract::<bool>(py) {
                        Err(_) => None,
                        Ok(result_bool) => {
                            if !result_bool {
                                Some("You are banned from this server.".into())
                            } else {
                                None
                            }
                        }
                    },
                    Ok(result_string) => Some(result_string),
                },
            },
        );

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients & !client_id as u64, Ordering::SeqCst);
    }

    result
}

pub(crate) fn client_disconnect_dispatcher<T>(client_id: i32, reason: T)
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_disconnect_handler) = *PLAYER_DISCONNECT_HANDLER.load() else {
        return;
    };

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | client_id as u64, Ordering::SeqCst);
    }

    let result =
        Python::with_gil(|py| client_disconnect_handler.call1(py, (client_id, reason.as_ref())));
    if result.is_err() {
        error!(target: "shinqlx", "client_disconnect_handler returned an error.");
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients & !client_id as u64, Ordering::SeqCst);
    }
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_loaded_handler) = *PLAYER_LOADED_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| client_loaded_handler.call1(py, (client_id,)));
    if result.is_err() {
        error!(target: "shinqlx", "client_loaded_handler returned an error.");
    }
}

pub(crate) fn new_game_dispatcher(restart: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref new_game_handler) = *NEW_GAME_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| new_game_handler.call1(py, (restart,)));
    if result.is_err() {
        error!(target: "shinqlx", "new_game_handler returned an error.");
    }
}

pub(crate) fn set_configstring_dispatcher<T, U>(index: T, value: U) -> Option<String>
where
    T: Into<u32>,
    U: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(value.as_ref().into());
    }

    let Some(ref set_configstring_handler) = *SET_CONFIGSTRING_HANDLER.load() else {
        return Some(value.as_ref().into());
    };

    Python::with_gil(|py| {
        match set_configstring_handler.call1(py, (index.into(), value.as_ref())) {
            Err(_) => {
                error!(target: "shinqlx", "set_configstring_handler returned an error.");
                Some(value.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(value.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(value.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        }
    })
}

pub(crate) fn rcon_dispatcher<T>(cmd: T)
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref rcon_handler) = *RCON_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| rcon_handler.call1(py, (cmd.as_ref(),)));
    if result.is_err() {
        error!(target: "shinqlx", "rcon_handler returned an error.");
    }
}

pub(crate) fn console_print_dispatcher<T>(text: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyminqlx_is_initialized() {
        return Some(text.as_ref().into());
    }

    let Some(ref console_print_handler) = *CONSOLE_PRINT_HANDLER.load() else {
        return Some(text.as_ref().into());
    };

    Python::with_gil(
        |py| match console_print_handler.call1(py, (text.as_ref(),)) {
            Err(_) => {
                error!(target: "shinqlx", "console_print_handler returned an error.");
                Some(text.as_ref().into())
            }
            Ok(returned) => match returned.extract::<String>(py) {
                Err(_) => match returned.extract::<bool>(py) {
                    Err(_) => Some(text.as_ref().into()),
                    Ok(result_bool) => {
                        if !result_bool {
                            None
                        } else {
                            Some(text.as_ref().into())
                        }
                    }
                },
                Ok(result_string) => Some(result_string),
            },
        },
    )
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref client_spawn_handler) = *PLAYER_SPAWN_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| client_spawn_handler.call1(py, (client_id,)));
    if result.is_err() {
        error!(target: "shinqlx", "client_spawn_handler returned an error.");
    }
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref kamikaze_use_handler) = *KAMIKAZE_USE_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| kamikaze_use_handler.call1(py, (client_id,)));
    if result.is_err() {
        error!(target: "shinqlx", "kamikaze_use_handler returned an error.");
    }
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref kamikaze_explode_handler) = *KAMIKAZE_EXPLODE_HANDLER.load() else {
        return;
    };

    let result =
        Python::with_gil(|py| kamikaze_explode_handler.call1(py, (client_id, is_used_on_demand)));
    if result.is_err() {
        error!(target: "shinqlx", "kamikaze_explode_handler returned an error.");
    }
}

pub(crate) fn damage_dispatcher(
    target_client_id: i32,
    attacker_client_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) {
    if !pyminqlx_is_initialized() {
        return;
    }

    let Some(ref damage_handler) = *DAMAGE_HANDLER.load() else {
        return;
    };

    let result = Python::with_gil(|py| {
        damage_handler.call1(
            py,
            (
                target_client_id,
                attacker_client_id,
                damage,
                dflags,
                means_of_death,
            ),
        )
    });
    if result.is_err() {
        error!(target: "shinqlx", "damage_handler returned an error.");
    }
}

#[cfg(test)]
mod pyminqlx_dispatcher_tests {
    use super::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher, PYMINQLX_INITIALIZED,
    };
    use super::{
        CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, DAMAGE_HANDLER, FRAME_HANDLER,
        KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER, NEW_GAME_HANDLER, PLAYER_CONNECT_HANDLER,
        PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER, PLAYER_SPAWN_HANDLER, RCON_HANDLER,
        SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
    };
    use crate::prelude::*;
    #[cfg(not(miri))]
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use pyo3::prelude::*;
    #[cfg(not(miri))]
    use rstest::rstest;

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn client_command_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        CLIENT_COMMAND_HANDLER.store(None);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return cmd
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return "qwertz"
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return False
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return (1, 2, 3)
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        SERVER_COMMAND_HANDLER.store(None);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return cmd
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return "qwertz"
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return False
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, cmd):
    return (1, 2, 3)
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let server_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        FRAME_HANDLER.store(None);

        frame_dispatcher();
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let frame_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        FRAME_HANDLER.store(Some(frame_handler.into()));

        frame_dispatcher();
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn frame_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let frame_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        FRAME_HANDLER.store(Some(frame_handler.into()));

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_CONNECT_HANDLER.store(None);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_bot):
    return "qwertz"
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_connect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_bot):
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_connect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_bot):
    return False
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_connect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, Some("You are banned from this server.".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_bot):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_connect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_bot):
    return (1, 2, 3)
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let player_connect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_CONNECT_HANDLER.store(Some(player_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_disconnect_dispatcher(42, "asdf");
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_DISCONNECT_HANDLER.store(None);

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, reason):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_disconnect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.into()));

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, reason):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_disconnect_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.into()));

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_LOADED_HANDLER.store(None);

        client_loaded_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_loaded_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.into()));

        client_loaded_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_loaded_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.into()));

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        new_game_dispatcher(false);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        NEW_GAME_HANDLER.store(None);

        new_game_dispatcher(true);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn new_game_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(restart):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let new_game_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        NEW_GAME_HANDLER.store(Some(new_game_handler.into()));

        new_game_dispatcher(false);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn new_game_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(restart):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let new_game_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        NEW_GAME_HANDLER.store(Some(new_game_handler.into()));

        new_game_dispatcher(true);
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        SET_CONFIGSTRING_HANDLER.store(None);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    return cmd
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    return "qwertz"
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    return False
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(index, value):
    return (1, 2, 3)
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let set_configstring_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        RCON_HANDLER.store(None);

        rcon_dispatcher("asdf");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(cmd):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let rcon_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        RCON_HANDLER.store(Some(rcon_handler.into()));

        rcon_dispatcher("asdf");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn rcon_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(cmd):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let rcon_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        RCON_HANDLER.store(Some(rcon_handler.into()));

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        CONSOLE_PRINT_HANDLER.store(None);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    return cmd
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    return "qwertz"
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    return False
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(text):
    return (1, 2, 3)
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let console_print_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_SPAWN_HANDLER.store(None);

        client_spawn_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_spawn_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.into()));

        client_spawn_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let client_spawn_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.into()));

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        KAMIKAZE_USE_HANDLER.store(None);

        kamikaze_use_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let kamikaze_use_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.into()));

        kamikaze_use_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let kamikaze_use_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.into()));

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        KAMIKAZE_EXPLODE_HANDLER.store(None);

        kamikaze_explode_dispatcher(123, true);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_used_on_demand):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let kamikaze_explode_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.into()));

        kamikaze_explode_dispatcher(123, false);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, is_used_on_demand):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let kamikaze_explode_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.into()));

        kamikaze_explode_dispatcher(123, true);
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_python_not_initiailized() {
        PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        damage_dispatcher(
            123,
            None,
            666,
            DAMAGE_NO_PROTECTION as i32,
            meansOfDeath_t::MOD_TRIGGER_HURT as i32,
        );
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_dispatcher_not_initiailized() {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        DAMAGE_HANDLER.store(None);

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn damage_dispatcher_dispatcher_works_properly(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, attacker_id, damage, dflags, means_of_death):
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let damage_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        DAMAGE_HANDLER.store(Some(damage_handler.into()));

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn damage_dispatcher_dispatcher_throws_exception(_pyminqlx_setup: ()) {
        PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(client_id, attacker_id, damage, dflags, means_of_death):
    raise Exception
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let damage_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        DAMAGE_HANDLER.store(Some(damage_handler.into()));

        damage_dispatcher(
            123,
            None,
            666,
            DAMAGE_NO_PROTECTION as i32,
            meansOfDeath_t::MOD_TRIGGER_HURT as i32,
        );
    }
}

/// Information about a player, such as Steam ID, name, client ID, and whatnot.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerInfo", get_all)]
#[derive(Debug, PartialEq)]
#[allow(unused)]
struct PlayerInfo {
    /// The player's client ID.
    client_id: i32,
    /// The player's name.
    name: String,
    /// The player's connection state.
    connection_state: i32,
    /// The player's userinfo.
    userinfo: String,
    /// The player's 64-bit representation of the Steam ID.
    steam_id: u64,
    /// The player's team.
    team: i32,
    /// The player's privileges.
    privileges: i32,
}

#[pymethods]
impl PlayerInfo {
    fn __str__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
    }

    fn __repr__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
    }
}

impl From<i32> for PlayerInfo {
    fn from(client_id: i32) -> Self {
        let game_entity_result = GameEntity::try_from(client_id);
        match game_entity_result {
            Err(_) => PlayerInfo {
                client_id,
                name: Default::default(),
                connection_state: clientState_t::CS_FREE as i32,
                userinfo: Default::default(),
                steam_id: 0,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: -1,
            },
            Ok(game_entity) => {
                let Ok(client) = Client::try_from(client_id) else {
                    return PlayerInfo {
                        client_id,
                        name: game_entity.get_player_name(),
                        connection_state: clientState_t::CS_FREE as i32,
                        userinfo: Default::default(),
                        steam_id: 0,
                        team: game_entity.get_team() as i32,
                        privileges: game_entity.get_privileges() as i32,
                    };
                };
                PlayerInfo {
                    client_id,
                    name: game_entity.get_player_name(),
                    connection_state: client.get_state() as i32,
                    userinfo: client.get_user_info(),
                    steam_id: client.get_steam_id(),
                    team: game_entity.get_team() as i32,
                    privileges: game_entity.get_privileges() as i32,
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_info_tests {
    use super::PlayerInfo;
    use crate::client::MockClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use pretty_assertions::assert_eq;

    #[test]
    #[serial]
    fn player_info_python_string() {
        let player_info = PlayerInfo {
            client_id: 2,
            name: "UnknownPlayer".into(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".into(),
            steam_id: 42,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };

        assert_eq!(
            player_info.__str__(),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    #[serial]
    fn player_info_python_repr() {
        let player_info = PlayerInfo {
            client_id: 2,
            name: "UnknownPlayer".into(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".into(),
            steam_id: 42,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };

        assert_eq!(
            player_info.__repr__(),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    #[serial]
    fn player_info_from_existing_game_entity_and_client() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "UnknownPlayer".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_SPECTATOR);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });
        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 42);
            mock_client
        });

        assert_eq!(
            PlayerInfo::from(2),
            PlayerInfo {
                client_id: 2,
                name: "UnknownPlayer".into(),
                connection_state: clientState_t::CS_ACTIVE as i32,
                userinfo: "asdf".into(),
                steam_id: 42,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: privileges_t::PRIV_NONE as i32
            }
        );
    }
}

/// Returns a dictionary with information about a player by ID.
#[pyfunction(name = "player_info")]
fn get_player_info(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerInfo>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let Ok(client) = Client::try_from(client_id) else {
            return Ok(Some(PlayerInfo::from(client_id)));
        };

        let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        if allowed_free_clients & client_id as u64 == 0
            && client.get_state() == clientState_t::CS_FREE
        {
            warn!(
                target: "shinqlx",
                "WARNING: get_player_info called for CS_FREE client {}.",
                client_id
            );
            return Ok(None);
        }

        Ok(Some(PlayerInfo::from(client_id)))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_player_info_tests {
    use super::get_player_info;
    use super::PlayerInfo;
    use super::MAIN_ENGINE;
    use crate::client::MockClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::pyminqlx::ALLOW_FREE_CLIENT;
    use crate::quake_live_engine::MockQuakeEngine;
    use core::sync::atomic::Ordering;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_player_info_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_player_info(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_player_info(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_player_info(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_player_info_for_existing_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
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
        game_entity_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_RED);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(
            player_info,
            Some(PlayerInfo {
                client_id: 2,
                name: "Mocked Player".into(),
                connection_state: clientState_t::CS_ACTIVE as i32,
                userinfo: "asdf".into(),
                steam_id: 1234,
                team: team_t::TEAM_RED as i32,
                privileges: privileges_t::PRIV_NONE as i32
            })
        );
    }

    #[test]
    #[serial]
    fn get_player_info_for_non_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(0, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(player_info, None);
    }

    #[test]
    #[serial]
    fn get_player_info_for_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(2, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_RED);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

        let player_info = Python::with_gil(|py| get_player_info(py, 2).unwrap());
        assert_eq!(
            player_info,
            Some(PlayerInfo {
                client_id: 2,
                name: "Mocked Player".into(),
                connection_state: clientState_t::CS_FREE as i32,
                userinfo: "asdf".into(),
                steam_id: 1234,
                team: team_t::TEAM_RED as i32,
                privileges: privileges_t::PRIV_NONE as i32
            })
        );
    }
}

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
fn get_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    py.allow_threads(move || {
        let result: Vec<Option<PlayerInfo>> = (0..maxclients)
            .filter_map(|client_id| {
                Client::try_from(client_id).map_or_else(
                    |_| None,
                    |client| match client.get_state() {
                        clientState_t::CS_FREE => None,
                        _ => Some(Some(PlayerInfo::from(client_id))),
                    },
                )
            })
            .collect();

        Ok(result)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_players_info_tests {
    use super::get_players_info;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_players_info_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_players_info(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }
}

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
fn get_userinfo(py: Python<'_>, client_id: i32) -> PyResult<Option<String>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let opt_client = Client::try_from(client_id).ok().filter(|client| {
            let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
            client.get_state() != clientState_t::CS_FREE
                || allowed_free_clients & client_id as u64 != 0
        });
        Ok(opt_client.map(|client| client.get_user_info()))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_userinfo_tests {
    use super::MAIN_ENGINE;
    use super::{get_userinfo, ALLOW_FREE_CLIENT};
    use crate::client::MockClient;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_userinfo_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_userinfo(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_userinfo(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = get_userinfo(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_existing_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn get_userinfo_for_non_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(0, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, None);
    }

    #[test]
    #[serial]
    fn get_userinfo_for_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(2, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }
}

/// Sends a server command to either one specific client or all the clients.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
fn send_server_command(py: Python<'_>, client_id: Option<i32>, cmd: &str) -> PyResult<bool> {
    match client_id {
        None => {
            #[allow(clippy::unnecessary_to_owned)]
            shinqlx_send_server_command(None, cmd.to_string());
            Ok(true)
        }
        Some(actual_client_id) => {
            let maxclients = py.allow_threads(|| {
                let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                    return Err(PyEnvironmentError::new_err(
                        "main quake live engine not set",
                    ));
                };

                Ok(main_engine.get_max_clients())
            })?;

            if !(0..maxclients).contains(&actual_client_id) {
                return Err(PyValueError::new_err(format!(
                    "client_id needs to be a number from 0 to {}, or None.",
                    maxclients - 1
                )));
            }

            let opt_client = Client::try_from(actual_client_id)
                .ok()
                .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
            let returned = opt_client.is_some();
            #[allow(clippy::unnecessary_to_owned)]
            if returned {
                shinqlx_send_server_command(opt_client, cmd.to_string());
            }
            Ok(returned)
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod send_server_command_tests {
    use super::send_server_command;
    use super::MAIN_ENGINE;
    use crate::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn send_server_command_with_no_client_id() {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "asdf")
            .times(1);
        let result = Python::with_gil(|py| send_server_command(py, None, "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn send_server_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(0), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(-1), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = send_server_command(py, Some(42), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn send_server_command_for_active_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "asdf")
            .times(1);

        let result = Python::with_gil(|py| send_server_command(py, Some(2), "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn send_server_command_for_non_active_free_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        let result = Python::with_gil(|py| send_server_command(py, Some(2), "asdf")).unwrap();
        assert_eq!(result, false);
    }
}

/// Tells the server to process a command from a specific client.
#[pyfunction]
#[pyo3(name = "client_command")]
fn client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}, or None.",
            maxclients - 1
        )));
    }

    let opt_client = Client::try_from(client_id).ok().filter(|client| {
        ![clientState_t::CS_FREE, clientState_t::CS_ZOMBIE].contains(&client.get_state())
    });
    let returned = opt_client.is_some();
    if returned {
        shinqlx_execute_client_command(opt_client, cmd.to_string(), true);
    }
    Ok(returned)
}

#[cfg(test)]
#[cfg(not(miri))]
mod client_command_tests {
    use super::client_command;
    use super::MAIN_ENGINE;
    use crate::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_execute_client_command_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::*;

    #[test]
    #[serial]
    fn client_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = client_command(py, 0, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn client_command_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = client_command(py, -1, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn client_command_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = client_command(py, 42, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_ACTIVE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[serial]
    fn send_server_command_for_active_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd, &client_ok| client.is_some() && cmd == "asdf" && client_ok)
            .times(1);

        let result = Python::with_gil(|py| client_command(py, 2, "asdf")).unwrap();
        assert_eq!(result, true);
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn send_server_command_for_non_active_free_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        let result = Python::with_gil(|py| client_command(py, 2, "asdf")).unwrap();
        assert_eq!(result, false);
    }
}

/// Executes a command as if it was executed from the server console.
#[pyfunction]
#[pyo3(name = "console_command")]
fn console_command(py: Python<'_>, cmd: &str) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.execute_console_command(cmd);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod console_command_tests {
    use super::console_command;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = console_command(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn console_command_with_main_engine_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("asdf"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
fn get_cvar(py: Python<'_>, cvar: &str) -> PyResult<Option<String>> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        match main_engine.find_cvar(cvar) {
            None => Ok(None),
            Some(cvar_result) => Ok(Some(cvar_result.get_string())),
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_cvar_tests {
    use super::get_cvar;
    use super::MAIN_ENGINE;
    use crate::cvar::CVar;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use alloc::ffi::CString;
    use core::ffi::c_char;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_cvar(py, "sv_maxclients");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_cvar_when_cvar_not_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("asdf"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_cvar(py, "asdf")).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn get_cvar_when_cvar_is_found() {
        let cvar_string = CString::new("16").unwrap();
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .unwrap();
                let cvar = CVar::try_from(&mut raw_cvar as *mut cvar_t).unwrap();
                Some(cvar)
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_cvar(py, "sv_maxclients")).unwrap();
        assert_eq!(result, Some("16".into()));
    }
}

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
fn set_cvar(py: Python<'_>, cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        match main_engine.find_cvar(cvar) {
            None => {
                main_engine.get_cvar(cvar, value, flags);
                Ok(true)
            }
            Some(_) => {
                main_engine.set_cvar_forced(
                    cvar,
                    value,
                    flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
                );
                Ok(false)
            }
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_tests {
    use super::set_cvar;
    use super::MAIN_ENGINE;
    use crate::cvar::CVar;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_cvar(py, "sv_maxclients", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_for_not_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        mock_engine
            .expect_get_cvar()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_cvar_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                let cvar = CVar::try_from(&mut raw_cvar as *mut cvar_t).unwrap();
                Some(cvar)
            })
            .times(1);
        mock_engine
            .expect_set_cvar_forced()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(false),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
fn set_cvar_limit(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    min: &str,
    max: &str,
    flags: Option<i32>,
) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.set_cvar_limit(cvar, value, min, max, flags);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_limit_tests {
    use super::set_cvar_limit;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_cvar_limit(py, "sv_maxclients", "64", "1", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_limit_forwards_parameters_to_main_engine_call() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_cvar_limit()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq("1"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            set_cvar_limit(
                py,
                "sv_maxclients",
                "64",
                "1",
                "64",
                Some(cvar_flags::CVAR_CHEAT as i32),
            )
        });
        assert!(result.is_ok());
    }
}

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
fn kick(py: Python<'_>, client_id: i32, reason: Option<&str>) -> PyResult<()> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_client = Client::try_from(client_id)
            .ok()
            .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
        let reason_str = reason
            .filter(|rsn| !rsn.is_empty())
            .unwrap_or("was kicked.");
        #[allow(clippy::unnecessary_to_owned)]
        opt_client
            .iter_mut()
            .for_each(|client| shinqlx_drop_client(client, reason_str.to_string()));
        if opt_client.is_some() {
            Ok(())
        } else {
            Err(PyValueError::new_err(
                "client_id must be the ID of an active player.",
            ))
        }
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod kick_tests {
    use super::kick;
    use super::MAIN_ENGINE;
    use crate::client::MockClient;
    use crate::hooks::mock_hooks::shinqlx_drop_client_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn kick_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = kick(py, 0, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = kick(py, -1, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = kick(py, 42, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[serial]
    fn kick_with_client_id_for_non_active_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        Python::with_gil(|py| {
            let result = kick(py, 2, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_without_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, None));
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_with_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "please go away!")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, Some("please go away!")));
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn kick_with_client_id_for_active_client_with_empty_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| kick(py, 2, Some("")));
        assert!(result.is_ok());
    }
}

/// Prints text on the console. If used during an RCON command, it will be printed in the player's console.
#[pyfunction]
#[pyo3(name = "console_print")]
fn console_print(py: Python<'_>, text: &str) {
    py.allow_threads(move || {
        let formatted_string = format!("{}\n", text);
        shinqlx_com_printf(formatted_string.as_str());
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod console_print_tests {
    use super::console_print as py_console_print;
    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn console_print() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx.expect().with(predicate::eq("asdf\n"));

        Python::with_gil(|py| {
            py_console_print(py, "asdf");
        });
    }
}

/// Get a configstring.
#[pyfunction]
#[pyo3(name = "get_configstring")]
fn get_configstring(py: Python<'_>, config_id: u32) -> PyResult<String> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_configstring(config_id as u16))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_configstring_tests {
    use super::get_configstring;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_configstring_for_too_large_configstring_id() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_configstring(py, MAX_CONFIGSTRINGS + 1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_configstring_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = get_configstring(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_configstring_forwards_call_to_engine() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(666))
            .returning(|_| "asdf".into())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| get_configstring(py, 666)).unwrap();
        assert_eq!(result, "asdf");
    }
}

/// Sets a configstring and sends it to all the players on the server.
#[pyfunction]
#[pyo3(name = "set_configstring")]
fn set_configstring(py: Python<'_>, config_id: u32, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS).contains(&config_id) {
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    py.allow_threads(move || {
        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(config_id, value.to_string());

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_configstring_tests {
    use super::set_configstring;
    use crate::hooks::mock_hooks::shinqlx_set_configstring_context;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_configstring_with_index_out_of_bounds() {
        Python::with_gil(|py| {
            let result = set_configstring(py, 2048, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_configstring_with_proper_index() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(666), predicate::eq("asdf".to_string()))
            .times(1);

        let result = Python::with_gil(|py| set_configstring(py, 666, "asdf"));
        assert!(result.is_ok());
    }
}

/// Forces the current vote to either fail or pass.
#[pyfunction]
#[pyo3(name = "force_vote")]
fn force_vote(py: Python<'_>, pass: bool) -> PyResult<bool> {
    let vote_time = py.allow_threads(|| {
        CurrentLevel::try_get()
            .ok()
            .and_then(|current_level| current_level.get_vote_time())
    });
    if vote_time.is_none() {
        return Ok(false);
    }

    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    py.allow_threads(move || {
        (0..maxclients)
            .filter(|i| {
                Client::try_from(*i)
                    .ok()
                    .filter(|client| client.get_state() == clientState_t::CS_ACTIVE)
                    .is_some()
            })
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_state(pass));
    });

    Ok(true)
}

#[cfg(test)]
#[cfg(not(miri))]
mod force_vote_tests {
    use super::force_vote;
    use super::MAIN_ENGINE;
    use crate::client::MockClient;
    use crate::current_level::MockTestCurrentLevel;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn force_vote_when_main_engine_not_initialized() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = force_vote(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn force_vote_when_no_vote_is_running() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        MAIN_ENGINE.store(None);

        let result = Python::with_gil(|py| force_vote(py, false)).unwrap();
        assert_eq!(result, false);
    }

    #[rstest]
    #[case(clientState_t::CS_ZOMBIE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_PRIMED)]
    #[serial]
    fn force_vote_for_non_active_client(#[case] clientstate: clientState_t) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn force_vote_for_active_client_with_no_game_client() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
            });

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn force_vote_for_active_client_forces_vote() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_vote_state()
                        .with(predicate::eq(true))
                        .times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        let result = Python::with_gil(|py| force_vote(py, true)).unwrap();

        assert_eq!(result, true);
    }
}

/// Adds a console command that will be handled by Python code.
#[pyfunction]
#[pyo3(name = "add_console_command")]
fn add_console_command(py: Python<'_>, command: &str) -> PyResult<()> {
    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        #[allow(clippy::unnecessary_to_owned)]
        main_engine.add_command(command.to_string(), cmd_py_command);

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod add_console_command_tests {
    use super::add_console_command;
    use super::MAIN_ENGINE;
    use crate::commands::cmd_py_command;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn add_console_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = add_console_command(py, "slap");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn add_console_command_adds_py_command_to_main_engine() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_add_command()
            .withf(|cmd, &func| cmd == "asdf" && func as usize == cmd_py_command as usize)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| add_console_command(py, "asdf"));
        assert!(result.is_ok());
    }
}

static CLIENT_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static SERVER_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static FRAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static PLAYER_CONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_LOADED_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_DISCONNECT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
pub(crate) static CUSTOM_COMMAND_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static NEW_GAME_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static SET_CONFIGSTRING_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static RCON_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));
static CONSOLE_PRINT_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static PLAYER_SPAWN_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static KAMIKAZE_USE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static KAMIKAZE_EXPLODE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> =
    Lazy::new(|| SwapArcOption::new(None));
static DAMAGE_HANDLER: Lazy<SwapArcOption<Py<PyAny>>> = Lazy::new(|| SwapArcOption::new(None));

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
fn register_handler(py: Python<'_>, event: &str, handler: Option<Py<PyAny>>) -> PyResult<()> {
    if handler
        .as_ref()
        .is_some_and(|handler_function| !handler_function.as_ref(py).is_callable())
    {
        return Err(PyTypeError::new_err("The handler must be callable."));
    }

    let handler_lock = match event {
        "client_command" => &CLIENT_COMMAND_HANDLER,
        "server_command" => &SERVER_COMMAND_HANDLER,
        "frame" => &FRAME_HANDLER,
        "player_connect" => &PLAYER_CONNECT_HANDLER,
        "player_loaded" => &PLAYER_LOADED_HANDLER,
        "player_disconnect" => &PLAYER_DISCONNECT_HANDLER,
        "custom_command" => &CUSTOM_COMMAND_HANDLER,
        "new_game" => &NEW_GAME_HANDLER,
        "set_configstring" => &SET_CONFIGSTRING_HANDLER,
        "rcon" => &RCON_HANDLER,
        "console_print" => &CONSOLE_PRINT_HANDLER,
        "player_spawn" => &PLAYER_SPAWN_HANDLER,
        "kamikaze_use" => &KAMIKAZE_USE_HANDLER,
        "kamikaze_explode" => &KAMIKAZE_EXPLODE_HANDLER,
        "damage" => &DAMAGE_HANDLER,
        _ => return Err(PyValueError::new_err("Unsupported event.")),
    };

    py.allow_threads(move || {
        handler_lock.store(handler.map(|handler_func| handler_func.into()));
        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod register_handler_tests {
    use super::register_handler;
    use super::{
        CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, CUSTOM_COMMAND_HANDLER, DAMAGE_HANDLER,
        FRAME_HANDLER, KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER, NEW_GAME_HANDLER,
        PLAYER_CONNECT_HANDLER, PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER,
        PLAYER_SPAWN_HANDLER, RCON_HANDLER, SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
    };
    use crate::prelude::*;
    use once_cell::sync::Lazy;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;
    use swap_arc::SwapArcOption;

    #[rstest]
    #[case("client_command", &CLIENT_COMMAND_HANDLER)]
    #[case("server_command", &SERVER_COMMAND_HANDLER)]
    #[case("frame", &FRAME_HANDLER)]
    #[case("player_connect", &PLAYER_CONNECT_HANDLER)]
    #[case("player_loaded", &PLAYER_LOADED_HANDLER)]
    #[case("player_disconnect", &PLAYER_DISCONNECT_HANDLER)]
    #[case("custom_command", &CUSTOM_COMMAND_HANDLER)]
    #[case("new_game", &NEW_GAME_HANDLER)]
    #[case("set_configstring", &SET_CONFIGSTRING_HANDLER)]
    #[case("rcon", &RCON_HANDLER)]
    #[case("console_print", &CONSOLE_PRINT_HANDLER)]
    #[case("player_spawn", &PLAYER_SPAWN_HANDLER)]
    #[case("kamikaze_use", &KAMIKAZE_USE_HANDLER)]
    #[case("kamikaze_explode", &KAMIKAZE_EXPLODE_HANDLER)]
    #[case("damage", &DAMAGE_HANDLER)]
    #[serial]
    fn register_handler_setting_handler_to_none(
        #[case] event: &str,
        #[case] handler: &Lazy<SwapArcOption<Py<PyAny>>>,
    ) {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        handler.store(Some(py_handler.into()));

        let result = Python::with_gil(|py| register_handler(py, event, None));
        assert!(result.is_ok());

        let stored_handler = handler.load();
        assert!(stored_handler.is_none());
    }

    #[rstest]
    #[case("client_command", &CLIENT_COMMAND_HANDLER)]
    #[case("server_command", &SERVER_COMMAND_HANDLER)]
    #[case("frame", &FRAME_HANDLER)]
    #[case("player_connect", &PLAYER_CONNECT_HANDLER)]
    #[case("player_loaded", &PLAYER_LOADED_HANDLER)]
    #[case("player_disconnect", &PLAYER_DISCONNECT_HANDLER)]
    #[case("custom_command", &CUSTOM_COMMAND_HANDLER)]
    #[case("new_game", &NEW_GAME_HANDLER)]
    #[case("set_configstring", &SET_CONFIGSTRING_HANDLER)]
    #[case("rcon", &RCON_HANDLER)]
    #[case("console_print", &CONSOLE_PRINT_HANDLER)]
    #[case("player_spawn", &PLAYER_SPAWN_HANDLER)]
    #[case("kamikaze_use", &KAMIKAZE_USE_HANDLER)]
    #[case("kamikaze_explode", &KAMIKAZE_EXPLODE_HANDLER)]
    #[case("damage", &DAMAGE_HANDLER)]
    #[serial]
    fn register_handler_setting_handler_to_some_handler(
        #[case] event: &str,
        #[case] handler: &Lazy<SwapArcOption<Py<PyAny>>>,
    ) {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));
        handler.store(None);

        let result = Python::with_gil(|py| register_handler(py, event, Some(py_handler)));
        assert!(result.is_ok());

        let stored_handler = handler.load();
        assert!(stored_handler.is_some());
    }

    #[test]
    #[serial]
    fn register_handler_for_some_unknown_event() {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        Python::with_gil(|py| {
            let result = register_handler(py, "unknown_event", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn register_handler_for_uncallable_handler() {
        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
handler = True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let py_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        Python::with_gil(|py| {
            let result = register_handler(py, "client_command", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}

#[pyclass]
struct Vector3Iter {
    iter: vec::IntoIter<i32>,
}

#[pymethods]
impl Vector3Iter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<i32> {
        slf.iter.next()
    }
}

/// A three-dimensional vector.
#[pyclass(name = "Vector3", module = "minqlx", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, Default)]
struct Vector3(
    #[pyo3(name = "x")] i32,
    #[pyo3(name = "y")] i32,
    #[pyo3(name = "z")] i32,
);

#[pymethods]
impl Vector3 {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 3 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all three dimensions",
            ));
        }

        if values.len() > 3 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than three dimensions",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Vector3 values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __repr__(&self) -> String {
        format!("Vector3(x={}, y={}, z={})", self.0, self.1, self.2)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<Vector3Iter>> {
        let iter_vec = vec![slf.0, slf.1, slf.2];
        let iter = Vector3Iter {
            iter: iter_vec.into_iter(),
        };
        Py::new(slf.py(), iter)
    }
}

impl From<(f32, f32, f32)> for Vector3 {
    fn from(value: (f32, f32, f32)) -> Self {
        Self(value.0 as i32, value.1 as i32, value.2 as i32)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
pub(crate) mod pyminqlx_setup_fixture {
    use crate::pyminqlx::pyminqlx_module;
    use pyo3::ffi::Py_IsInitialized;
    use pyo3::{append_to_inittab, prepare_freethreaded_python};
    use rstest::fixture;

    #[fixture]
    #[once]
    pub(crate) fn pyminqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyminqlx_module);
            prepare_freethreaded_python();
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod vector3_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn vector3_tuple_test(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let minqlx_module = py.import("_minqlx").unwrap();
            let vector3 = minqlx_module.getattr("Vector3").unwrap();
            let tuple = py.import("builtins").unwrap().getattr("tuple").unwrap();
            assert!(vector3.is_instance(tuple.get_type()).unwrap());
        });
    }

    #[rstest]
    fn vector3_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let vector3_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Vector3((0, 42, 666))
            "#,
                None,
                None,
            );
            assert!(
                vector3_constructor.is_ok(),
                "{}",
                vector3_constructor.err().unwrap()
            );
        });
    }
}

/// A struct sequence containing all the weapons in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "Weapons", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Weapons(
    #[pyo3(name = "g")] i32,
    #[pyo3(name = "mg")] i32,
    #[pyo3(name = "sg")] i32,
    #[pyo3(name = "gl")] i32,
    #[pyo3(name = "rl")] i32,
    #[pyo3(name = "lg")] i32,
    #[pyo3(name = "rg")] i32,
    #[pyo3(name = "pg")] i32,
    #[pyo3(name = "bfg")] i32,
    #[pyo3(name = "gh")] i32,
    #[pyo3(name = "ng")] i32,
    #[pyo3(name = "pl")] i32,
    #[pyo3(name = "cg")] i32,
    #[pyo3(name = "hmg")] i32,
    #[pyo3(name = "hands")] i32,
);

impl From<[i32; 15]> for Weapons {
    fn from(value: [i32; 15]) -> Self {
        Self(
            value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
            value[8], value[9], value[10], value[11], value[12], value[13], value[14],
        )
    }
}

impl From<Weapons> for [i32; 15] {
    fn from(value: Weapons) -> Self {
        [
            value.0, value.1, value.2, value.3, value.4, value.5, value.6, value.7, value.8,
            value.9, value.10, value.11, value.12, value.13, value.14,
        ]
    }
}

#[pymethods]
impl Weapons {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 15 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 15 weapons",
            ));
        }

        if values.len() > 15 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 15 weapons",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Weapons values need to be boolean"));
        }

        Ok(Self::from(
            <Vec<i32> as TryInto<[i32; 15]>>::try_into(
                results
                    .into_iter()
                    .map(|value| value.unwrap_or(0))
                    .collect::<Vec<i32>>(),
            )
            .unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Weapons(g={}, mg={}, sg={}, gl={}, rl={}, lg={}, rg={}, pg={}, bfg={}, gh={}, ng={}, pl={}, cg={}, hmg={}, hands={})",
        self.0, self.1, self.2, self.3, self.4, self.5, self.5, self.7, self.8, self.9, self.10, self.11, self.12, self.13, self.14)
    }

    fn __repr__(&self) -> String {
        format!("Weapons(g={}, mg={}, sg={}, gl={}, rl={}, lg={}, rg={}, pg={}, bfg={}, gh={}, ng={}, pl={}, cg={}, hmg={}, hands={})",
        self.0, self.1, self.2, self.3, self.4, self.5, self.5, self.7, self.8, self.9, self.10, self.11, self.12, self.13, self.14)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod weapons_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn weapons_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let weapons_constructor =py.run(r#"
import _minqlx
weapons = _minqlx.Weapons((False, False, False, False, False, False, False, False, False, False, False, False, False, False, False))
            "#, None, None);
            assert!(
                weapons_constructor.is_ok(),
                "{}",
                weapons_constructor.err().unwrap()
            );
        });
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod ammo_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn ammo_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let ammo_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Weapons((0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14))
            "#,
                None,
                None,
            );
            assert!(
                ammo_constructor.is_ok(),
                "{}",
                ammo_constructor.err().unwrap()
            );
        });
    }
}

/// A struct sequence containing all the powerups in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "Powerups", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Powerups(
    #[pyo3(name = "quad")] i32,
    #[pyo3(name = "battlesuit")] i32,
    #[pyo3(name = "haste")] i32,
    #[pyo3(name = "invisibility")] i32,
    #[pyo3(name = "regeneration")] i32,
    #[pyo3(name = "invulnerability")] i32,
);

impl From<[i32; 6]> for Powerups {
    fn from(value: [i32; 6]) -> Self {
        Self(value[0], value[1], value[2], value[3], value[4], value[5])
    }
}

impl From<Powerups> for [i32; 6] {
    fn from(value: Powerups) -> Self {
        [value.0, value.1, value.2, value.3, value.4, value.5]
    }
}

#[pymethods]
impl Powerups {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 6 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 6 powerups",
            ));
        }

        if values.len() > 6 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 6 powerups",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Powerups values need to be integer"));
        }

        Ok(Self::from(
            <Vec<i32> as TryInto<[i32; 6]>>::try_into(
                results
                    .into_iter()
                    .map(|value| value.unwrap_or(0))
                    .collect::<Vec<i32>>(),
            )
            .unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!("Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5)
    }

    fn __repr__(&self) -> String {
        format!("Powerups(quad={}, battlesuit={}, haste={}, invisibility={}, regeneration={}, invulnerability={})",
            self.0, self.1, self.2, self.3, self.4, self.5)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod powerups_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn powerups_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let powerups_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Powerups((0, 1, 2, 3, 4, 5))
            "#,
                None,
                None,
            );
            assert!(
                powerups_constructor.is_ok(),
                "{}",
                powerups_constructor.err().unwrap(),
            );
        });
    }
}

#[pyclass]
#[pyo3(module = "minqlx", name = "Holdable")]
#[derive(PartialEq, Debug, Clone, Copy)]
enum Holdable {
    None = 0,
    Teleporter = 27,
    MedKit = 28,
    Kamikaze = 37,
    Portal = 38,
    Invulnerability = 39,
    Flight = 34,
    Unknown = 666,
}

impl From<i32> for Holdable {
    fn from(value: i32) -> Self {
        match value {
            0 => Holdable::None,
            27 => Holdable::Teleporter,
            28 => Holdable::MedKit,
            34 => Holdable::Flight,
            37 => Holdable::Kamikaze,
            38 => Holdable::Portal,
            39 => Holdable::Invulnerability,
            _ => Holdable::Unknown,
        }
    }
}

impl From<Holdable> for i32 {
    fn from(value: Holdable) -> Self {
        match value {
            Holdable::None => 0,
            Holdable::Teleporter => 27,
            Holdable::MedKit => 28,
            Holdable::Flight => 34,
            Holdable::Kamikaze => 37,
            Holdable::Portal => 38,
            Holdable::Invulnerability => 39,
            Holdable::Unknown => 0,
        }
    }
}

impl From<Holdable> for Option<String> {
    fn from(holdable: Holdable) -> Self {
        match holdable {
            Holdable::None => None,
            Holdable::Teleporter => Some("teleporter".into()),
            Holdable::MedKit => Some("medkit".into()),
            Holdable::Kamikaze => Some("kamikaze".into()),
            Holdable::Portal => Some("portal".into()),
            Holdable::Invulnerability => Some("invulnerability".into()),
            Holdable::Flight => Some("flight".into()),
            Holdable::Unknown => Some("unknown".into()),
        }
    }
}

#[cfg(test)]
mod holdable_tests {
    use super::Holdable;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case(0, Holdable::None)]
    #[case(27, Holdable::Teleporter)]
    #[case(28, Holdable::MedKit)]
    #[case(34, Holdable::Flight)]
    #[case(37, Holdable::Kamikaze)]
    #[case(38, Holdable::Portal)]
    #[case(39, Holdable::Invulnerability)]
    #[case(666, Holdable::Unknown)]
    fn holdable_from_integer(#[case] integer: i32, #[case] expected_holdable: Holdable) {
        assert_eq!(Holdable::from(integer), expected_holdable);
    }

    #[rstest]
    #[case(Holdable::None, 0)]
    #[case(Holdable::Teleporter, 27)]
    #[case(Holdable::MedKit, 28)]
    #[case(Holdable::Flight, 34)]
    #[case(Holdable::Kamikaze, 37)]
    #[case(Holdable::Portal, 38)]
    #[case(Holdable::Invulnerability, 39)]
    #[case(Holdable::Unknown, 0)]
    fn integer_from_holdable(#[case] holdable: Holdable, #[case] expected_integer: i32) {
        assert_eq!(i32::from(holdable), expected_integer);
    }

    #[rstest]
    #[case(Holdable::None, None)]
    #[case(Holdable::Teleporter, Some("teleporter".into()))]
    #[case(Holdable::MedKit, Some("medkit".into()))]
    #[case(Holdable::Flight, Some("flight".into()))]
    #[case(Holdable::Kamikaze, Some("kamikaze".into()))]
    #[case(Holdable::Portal, Some("portal".into()))]
    #[case(Holdable::Invulnerability, Some("invulnerability".into()))]
    #[case(Holdable::Unknown, Some("unknown".into()))]
    fn opt_string_from_holdable(
        #[case] holdable: Holdable,
        #[case] expected_result: Option<String>,
    ) {
        assert_eq!(Option::<String>::from(holdable), expected_result);
    }
}

/// A struct sequence containing parameters for the flight holdable item.
#[pyclass]
#[pyo3(module = "minqlx", name = "Flight", get_all)]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct Flight(
    #[pyo3(name = "fuel")] i32,
    #[pyo3(name = "max_fuel")] i32,
    #[pyo3(name = "thrust")] i32,
    #[pyo3(name = "refuel")] i32,
);

impl From<Flight> for [i32; 4] {
    fn from(flight: Flight) -> Self {
        [flight.0, flight.1, flight.2, flight.3]
    }
}

#[pymethods]
impl Flight {
    #[new]
    fn py_new(values: &PyTuple) -> PyResult<Self> {
        if values.len() < 4 {
            return Err(PyValueError::new_err(
                "tuple did not provide values for all 4 flight parameters",
            ));
        }

        if values.len() > 4 {
            return Err(PyValueError::new_err(
                "tuple did provide values for more than 4 flight parameters",
            ));
        }

        let results = values
            .iter()
            .map(|item| item.extract::<i32>().ok())
            .collect::<Vec<Option<i32>>>();

        if results.iter().any(|item| item.is_none()) {
            return Err(PyValueError::new_err("Flight values need to be integer"));
        }

        Ok(Self(
            results[0].unwrap(),
            results[1].unwrap(),
            results[2].unwrap(),
            results[3].unwrap(),
        ))
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => (self == other).into_py(py),
            CompareOp::Ne => (self != other).into_py(py),
            _ => py.NotImplemented(),
        }
    }

    fn __str__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "Flight(fuel={}, max_fuel={}, thrust={}, refuel={})",
            self.0, self.1, self.2, self.3
        )
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod flight_tests {
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use pyo3::Python;
    use rstest::rstest;

    #[rstest]
    fn flight_can_be_created_from_python(_pyminqlx_setup: ()) {
        Python::with_gil(|py| {
            let flight_constructor = py.run(
                r#"
import _minqlx
weapons = _minqlx.Flight((0, 1, 2, 3))
            "#,
                None,
                None,
            );
            assert!(
                flight_constructor.is_ok(),
                "{}",
                flight_constructor.err().unwrap()
            );
        });
    }
}

/// Information about a player's state in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerState", get_all)]
#[derive(Debug, PartialEq)]
struct PlayerState {
    /// Whether the player's alive or not.
    is_alive: bool,
    /// The player's position.
    position: Vector3,
    /// The player's velocity.
    velocity: Vector3,
    /// The player's health.
    health: i32,
    /// The player's armor.
    armor: i32,
    /// Whether the player has noclip or not.
    noclip: bool,
    /// The weapon the player is currently using.
    weapon: i32,
    /// The player's weapons.
    weapons: Weapons,
    /// The player's weapon ammo.
    ammo: Weapons,
    ///The player's powerups.
    powerups: Powerups,
    /// The player's holdable item.
    holdable: Option<String>,
    /// A struct sequence with flight parameters.
    flight: Flight,
    /// Whether the player is currently chatting.
    is_chatting: bool,
    /// Whether the player is frozen(freezetag).
    is_frozen: bool,
}

#[pymethods]
impl PlayerState {
    fn __str__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
            self.is_alive,
            self.position.__str__(),
            self.velocity.__str__(),
            self.health,
            self.armor,
            self.noclip,
            self.weapon,
            self.weapons.__str__(),
            self.ammo.__str__(),
            self.powerups.__str__(),
            match self.holdable.as_ref() {
                Some(value) => value,
                None => "None",
            },
            self.flight.__str__(),
            self.is_chatting,
            self.is_frozen)
    }

    fn __repr__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
            self.is_alive,
            self.position.__str__(),
            self.velocity.__str__(),
            self.health,
            self.armor,
            self.noclip,
            self.weapon,
            self.weapons.__str__(),
            self.ammo.__str__(),
            self.powerups.__str__(),
            match self.holdable.as_ref() {
                Some(value) => value,
                None => "None",
            },
            self.flight.__str__(),
            self.is_chatting,
            self.is_frozen)
    }
}

impl From<GameEntity> for PlayerState {
    fn from(game_entity: GameEntity) -> Self {
        let game_client = game_entity.get_game_client().unwrap();
        let position = game_client.get_position();
        let velocity = game_client.get_velocity();
        Self {
            is_alive: game_client.is_alive(),
            position: Vector3::from(position),
            velocity: Vector3::from(velocity),
            health: game_entity.get_health(),
            armor: game_client.get_armor(),
            noclip: game_client.get_noclip(),
            weapon: game_client.get_weapon().into(),
            weapons: Weapons::from(game_client.get_weapons()),
            ammo: Weapons::from(game_client.get_ammos()),
            powerups: Powerups::from(game_client.get_powerups()),
            holdable: Holdable::from(game_client.get_holdable()).into(),
            flight: Flight(
                game_client.get_current_flight_fuel(),
                game_client.get_max_flight_fuel(),
                game_client.get_flight_thrust(),
                game_client.get_flight_refuel(),
            ),
            is_chatting: game_client.is_chatting(),
            is_frozen: game_client.is_frozen(),
        }
    }
}

/// Get information about the player's state in the game.
#[pyfunction]
#[pyo3(name = "player_state")]
fn player_state(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerState>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        Ok(GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok())
            .map(PlayerState::from))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_state_tests {
    use super::{player_state, Holdable, PlayerState, Vector3};
    use super::{Flight, Powerups, Weapons, MAIN_ENGINE};
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_state_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = player_state(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_state_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_state(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_state_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_state(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_state_for_client_without_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
            });

        let result = Python::with_gil(|py| player_state(py, 2)).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn player_state_transforms_from_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_get_position()
                        .returning(|| (1.0, 2.0, 3.0));
                    mock_game_client
                        .expect_get_velocity()
                        .returning(|| (4.0, 5.0, 6.0));
                    mock_game_client.expect_is_alive().returning(|| true);
                    mock_game_client.expect_get_armor().returning(|| 456);
                    mock_game_client.expect_get_noclip().returning(|| true);
                    mock_game_client
                        .expect_get_weapon()
                        .returning(|| weapon_t::WP_NAILGUN);
                    mock_game_client
                        .expect_get_weapons()
                        .returning(|| [1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
                    mock_game_client
                        .expect_get_ammos()
                        .returning(|| [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
                    mock_game_client
                        .expect_get_powerups()
                        .returning(|| [12, 34, 56, 78, 90, 24]);
                    mock_game_client
                        .expect_get_holdable()
                        .returning(|| Holdable::Kamikaze.into());
                    mock_game_client
                        .expect_get_current_flight_fuel()
                        .returning(|| 12);
                    mock_game_client
                        .expect_get_max_flight_fuel()
                        .returning(|| 34);
                    mock_game_client.expect_get_flight_thrust().returning(|| 56);
                    mock_game_client.expect_get_flight_refuel().returning(|| 78);
                    mock_game_client.expect_is_chatting().returning(|| true);
                    mock_game_client.expect_is_frozen().returning(|| true);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_get_health().returning(|| 123);
                mock_game_entity
            });

        let result = Python::with_gil(|py| player_state(py, 2)).unwrap();
        assert_eq!(
            result,
            Some(PlayerState {
                is_alive: true,
                position: Vector3(1, 2, 3),
                velocity: Vector3(4, 5, 6),
                health: 123,
                armor: 456,
                noclip: true,
                weapon: weapon_t::WP_NAILGUN.into(),
                weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
                ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
                powerups: Powerups(12, 34, 56, 78, 90, 24),
                holdable: Some("kamikaze".into()),
                flight: Flight(12, 34, 56, 78),
                is_chatting: true,
                is_frozen: true,
            })
        );
    }
}

/// A player's score and some basic stats.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerStats", get_all)]
#[derive(Debug, PartialEq)]
struct PlayerStats {
    /// The player's primary score.
    score: i32,
    /// The player's number of kills.
    kills: i32,
    /// The player's number of deaths.
    deaths: i32,
    /// The player's total damage dealt.
    damage_dealt: i32,
    /// The player's total damage taken.
    damage_taken: i32,
    /// The time in milliseconds the player has on a team since the game started.
    time: i32,
    /// The player's ping.
    ping: i32,
}

#[pymethods]
impl PlayerStats {
    fn __str__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
            self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }

    fn __repr__(&self) -> String {
        format!("PlayerStats(score={}, kills={}, deaths={}, damage_dealt={}, damage_taken={}, time={}, ping={})",
            self.score, self.kills, self.deaths, self.damage_dealt, self.damage_taken, self.time, self.ping)
    }
}

impl From<GameClient> for PlayerStats {
    fn from(game_client: GameClient) -> Self {
        Self {
            score: game_client.get_score(),
            kills: game_client.get_kills(),
            deaths: game_client.get_deaths(),
            damage_dealt: game_client.get_damage_dealt(),
            damage_taken: game_client.get_damage_taken(),
            time: game_client.get_time_on_team(),
            ping: game_client.get_ping(),
        }
    }
}

/// Get some player stats.
#[pyfunction]
#[pyo3(name = "player_stats")]
fn player_stats(py: Python<'_>, client_id: i32) -> PyResult<Option<PlayerStats>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        Ok(GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .map(PlayerStats::from))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_stats_tests {
    use super::MAIN_ENGINE;
    use super::{player_stats, PlayerStats};
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_stats_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = player_stats(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_stats_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_stats(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_stats_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_stats(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_stats_for_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_score().returning(|| 42);
                mock_game_client.expect_get_kills().returning(|| 7);
                mock_game_client.expect_get_deaths().returning(|| 9);
                mock_game_client
                    .expect_get_damage_dealt()
                    .returning(|| 5000);
                mock_game_client
                    .expect_get_damage_taken()
                    .returning(|| 4200);
                mock_game_client.expect_get_time_on_team().returning(|| 123);
                mock_game_client.expect_get_ping().returning(|| 9);
                Ok(mock_game_client)
            });
            mock_game_entity
        });
        let result = Python::with_gil(|py| player_stats(py, 2)).unwrap();

        assert!(result.is_some_and(|pstats| pstats
            == PlayerStats {
                score: 42,
                kills: 7,
                deaths: 9,
                damage_dealt: 5000,
                damage_taken: 4200,
                time: 123,
                ping: 9,
            }));
    }

    #[test]
    #[serial]
    fn player_stats_for_game_entiy_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });
        let result = Python::with_gil(|py| player_stats(py, 2)).unwrap();

        assert_eq!(result, None);
    }
}

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
fn set_position(py: Python<'_>, client_id: i32, position: Vector3) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_position((position.0 as f32, position.1 as f32, position.2 as f32))
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_position_tests {
    use super::set_position;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::pyminqlx::Vector3;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_position_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_position(py, 21, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_position(py, -1, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_position(py, 666, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_position_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_position()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_position(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_position_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_position(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
fn set_velocity(py: Python<'_>, client_id: i32, velocity: Vector3) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_velocity((velocity.0 as f32, velocity.1 as f32, velocity.2 as f32))
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_velocity_tests {
    use super::set_velocity;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::pyminqlx::Vector3;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_velocity_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_velocity(py, 21, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_velocity(py, -1, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_velocity(py, 666, Vector3(1, 2, 3));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_velocity_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_velocity()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_velocity(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_velocity_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_velocity(py, 2, Vector3(1, 2, 3))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
fn noclip(py: Python<'_>, client_id: i32, activate: bool) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .filter(|game_client| game_client.get_noclip() != activate);
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_noclip(activate);
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod noclip_tests {
    use super::noclip;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn noclip_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = noclip(py, 21, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = noclip(py, -1, false);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = noclip(py, 666, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| noclip(py, 2, true)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_noclip_already_set_properly() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client.expect_set_noclip::<bool>().times(0);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| noclip(py, 2, true)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn noclip_for_entity_with_change_applied() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client
                    .expect_set_noclip::<bool>()
                    .with(predicate::eq(false))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| noclip(py, 2, false)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's health.
#[pyfunction]
#[pyo3(name = "set_health")]
fn set_health(py: Python<'_>, client_id: i32, health: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_entity = GameEntity::try_from(client_id).ok();
        opt_game_entity
            .iter_mut()
            .for_each(|game_entity| game_entity.set_health(health));
        Ok(opt_game_entity.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_health_tests {
    use super::set_health;
    use super::MAIN_ENGINE;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_health_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_health(py, 21, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_health(py, -1, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_health(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_health_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_set_health()
                .with(predicate::eq(666))
                .times(1);
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_health(py, 2, 666)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's armor.
#[pyfunction]
#[pyo3(name = "set_armor")]
fn set_armor(py: Python<'_>, client_id: i32, armor: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_armor(armor));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_armor_tests {
    use super::set_armor;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_armor_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_armor(py, 21, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_armor(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_armor(py, 666, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_armor_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_armor()
                    .with(predicate::eq(456))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_armor(py, 2, 456)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_armor_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_armor(py, 2, 123)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
fn set_weapons(py: Python<'_>, client_id: i32, weapons: Weapons) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_weapons(weapons.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_weapons_tests {
    use super::set_weapons;
    use super::Weapons;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_weapons_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_weapons(py, 21, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapons(py, -1, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapons(
                py,
                666,
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapons_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapons()
                    .with(predicate::eq([1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_weapons(py, 2, Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1))
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_weapons_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_weapons(py, 2, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
fn set_weapon(py: Python<'_>, client_id: i32, weapon: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    if !(0..16).contains(&weapon) {
        return Err(PyValueError::new_err(
            "Weapon must be a number from 0 to 15.",
        ));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_weapon(weapon));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_weapon_tests {
    use super::set_weapon;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_weapon_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_weapon(py, 21, weapon_t::WP_ROCKET_LAUNCHER.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, -1, weapon_t::WP_GRAPPLING_HOOK.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 666, weapon_t::WP_PROX_LAUNCHER.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_weapon_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 2, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_weapon_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_weapon(py, 2, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_weapon_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapon()
                    .with(predicate::eq(weapon_t::WP_BFG as i32))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_weapon(py, 2, weapon_t::WP_BFG.into())).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_weapon_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_weapon(py, 2, weapon_t::WP_HMG.into())).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's ammo.
#[pyfunction]
#[pyo3(name = "set_ammo")]
fn set_ammo(py: Python<'_>, client_id: i32, ammos: Weapons) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_ammos(ammos.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_ammo_tests {
    use super::set_ammo;
    use super::Weapons;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_ammo_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_ammo(py, 21, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_ammo(py, -1, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_ammo(
                py,
                666,
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_ammo_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_ammos()
                    .with(predicate::eq([
                        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
                    ]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_ammo(
                py,
                2,
                Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            )
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_ammo_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            set_ammo(py, 2, Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        })
        .unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
fn set_powerups(py: Python<'_>, client_id: i32, powerups: Powerups) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_powerups(powerups.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_powerups_tests {
    use super::set_powerups;
    use super::Powerups;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_powerups_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_powerups(py, 21, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_powerups(py, -1, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_powerups(py, 666, Powerups(0, 0, 0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_powerups_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_powerups()
                    .with(predicate::eq([1, 2, 3, 4, 5, 6]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| set_powerups(py, 2, Powerups(1, 2, 3, 4, 5, 6))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_powerups_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| set_powerups(py, 2, Powerups(0, 0, 0, 0, 0, 0))).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
fn set_holdable(py: Python<'_>, client_id: i32, holdable: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        let ql_holdable = Holdable::from(holdable);
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_holdable(ql_holdable));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_holdable_tests {
    use super::MAIN_ENGINE;
    use super::{set_holdable, Holdable};
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_holdable_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_holdable(py, 21, Holdable::Kamikaze as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_holdable(py, -1, Holdable::Invulnerability as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_holdable(py, 666, Holdable::Teleporter as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_holdable_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_holdable()
                    .with(predicate::eq(Holdable::Kamikaze))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_holdable(py, 2, Holdable::Kamikaze as i32)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_holdable_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| set_holdable(py, 2, Holdable::Invulnerability as i32)).unwrap();
        assert_eq!(result, false);
    }
}

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
fn drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .iter_mut()
            .for_each(|game_client| game_client.remove_kamikaze_flag());
        let mut opt_game_entity_with_holdable =
            GameEntity::try_from(client_id).ok().filter(|game_entity| {
                game_entity
                    .get_game_client()
                    .ok()
                    .filter(|game_client| {
                        Holdable::from(game_client.get_holdable()) != Holdable::None
                    })
                    .is_some()
            });
        opt_game_entity_with_holdable
            .iter_mut()
            .for_each(|game_entity| game_entity.drop_holdable());
        Ok(opt_game_entity_with_holdable.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod drop_holdable_tests {
    use super::MAIN_ENGINE;
    use super::{drop_holdable, Holdable};
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::Sequence;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn drop_holdable_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = drop_holdable(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = drop_holdable(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = drop_holdable(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn drop_holdable_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[serial]
    fn drop_holdable_for_entity_with_no_holdable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_get_holdable().returning(|| 0);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, false);
    }

    #[rstest]
    #[case(&Holdable::Teleporter)]
    #[case(&Holdable::MedKit)]
    #[case(&Holdable::Kamikaze)]
    #[case(&Holdable::Portal)]
    #[case(&Holdable::Invulnerability)]
    #[case(&Holdable::Flight)]
    #[serial]
    fn drop_holdable_for_entity_with_holdable_dropped(#[case] holdable: &'static Holdable) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_get_holdable()
                        .returning(|| *holdable as i32);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(1);
                mock_game_entity
            });

        let result = Python::with_gil(|py| drop_holdable(py, 2)).unwrap();
        assert_eq!(result, true);
    }
}

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
fn set_flight(py: Python<'_>, client_id: i32, flight: Flight) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_flight::<[i32; 4]>(flight.into()));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_flight_tests {
    use super::set_flight;
    use super::Flight;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_flight_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_flight(py, 21, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_flight(py, -1, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_flight(py, 666, Flight(0, 0, 0, 0));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_flight_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_flight::<[i32; 4]>()
                    .with(predicate::eq([12, 34, 56, 78]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_flight(py, 2, Flight(12, 34, 56, 78))).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_flight_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_flight(py, 2, Flight(12, 34, 56, 78))).unwrap();
        assert_eq!(result, false);
    }
}

/// Makes player invulnerable for limited time.
#[pyfunction]
#[pyo3(name = "set_invulnerability")]
fn set_invulnerability(py: Python<'_>, client_id: i32, time: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_invulnerability(time));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_invulnerability_tests {
    use super::set_invulnerability;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_invulnerability_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_invulnerability(py, 21, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_invulnerability(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_invulnerability(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_invulnerability()
                    .with(predicate::eq(42))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_invulnerability(py, 2, 42)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_invulnerability_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_invulnerability(py, 2, 42)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's score.
#[pyfunction]
#[pyo3(name = "set_score")]
fn set_score(py: Python<'_>, client_id: i32, score: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_score(score));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_score_tests {
    use super::set_score;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_score_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_score(py, 21, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_score(py, -1, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_score(py, 666, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_score_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_score()
                    .with(predicate::eq(42))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_score(py, 2, 42)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_score_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_score(py, 2, 42)).unwrap();
        assert_eq!(result, false);
    }
}

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
fn callvote(py: Python<'_>, vote: &str, vote_disp: &str, vote_time: Option<i32>) {
    py.allow_threads(move || {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.callvote(vote, vote_disp, vote_time));
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod callvote_tests {
    use super::callvote;
    use crate::current_level::MockTestCurrentLevel;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn callvote_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| callvote(py, "map thunderstruck", "map thunderstruck", None));
    }

    #[test]
    #[serial]
    fn callvote_with_current_level_calls_vote() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_callvote()
                .with(
                    predicate::eq("map theatreofpain"),
                    predicate::eq("map Theatre of Pain"),
                    predicate::eq(Some(10)),
                )
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| callvote(py, "map theatreofpain", "map Theatre of Pain", Some(10)));
    }
}

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
fn allow_single_player(py: Python<'_>, allow: bool) {
    py.allow_threads(move || {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.set_training_map(allow))
    });
}

#[cfg(test)]
#[cfg(not(miri))]
mod allow_single_player_tests {
    use super::allow_single_player;
    use crate::current_level::MockTestCurrentLevel;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn allow_single_player_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| allow_single_player(py, true));
    }

    #[test]
    #[serial]
    fn allow_single_player_sets_training_map() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_set_training_map()
                .with(predicate::eq(true))
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| allow_single_player(py, true));
    }
}

/// Spawns a player.
#[pyfunction]
#[pyo3(name = "player_spawn")]
fn player_spawn(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let Ok(game_entity) = GameEntity::try_from(client_id) else {
            return Ok(false);
        };
        let Ok(mut game_client) = game_entity.get_game_client() else {
            return Ok(false);
        };
        game_client.spawn();
        shinqlx_client_spawn(game_entity);
        Ok(true)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod player_spawn_tests {
    use super::player_spawn;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::hooks::mock_hooks::shinqlx_client_spawn_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn player_spawn_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = player_spawn(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_spawn(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = player_spawn(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn player_spawn_for_existing_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_spawn().times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(1);

        let result = Python::with_gil(|py| player_spawn(py, 2)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn player_spawn_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let client_spawn_ctx = shinqlx_client_spawn_context();
        client_spawn_ctx.expect().returning_st(|_| ()).times(0);

        let result = Python::with_gil(|py| player_spawn(py, 2)).unwrap();
        assert_eq!(result, false);
    }
}

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
fn set_privileges(py: Python<'_>, client_id: i32, privileges: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client
            .iter_mut()
            .for_each(|game_client| game_client.set_privileges(privileges));
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_privileges_tests {
    use super::set_privileges;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;
    use rstest::rstest;

    #[test]
    #[serial]
    fn set_privileges_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = set_privileges(py, 21, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_privileges_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_privileges(py, -1, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_privileges_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = set_privileges(py, 666, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(&privileges_t::PRIV_NONE)]
    #[case(&privileges_t::PRIV_MOD)]
    #[case(&privileges_t::PRIV_ADMIN)]
    #[case(&privileges_t::PRIV_ROOT)]
    #[case(&privileges_t::PRIV_BANNED)]
    #[serial]
    fn set_privileges_for_existing_game_client(#[case] privileges: &'static privileges_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_privileges()
                    .with(predicate::eq(*privileges as i32))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let result = Python::with_gil(|py| set_privileges(py, 2, *privileges as i32)).unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_privileges_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| set_privileges(py, 2, privileges_t::PRIV_NONE as i32)).unwrap();
        assert_eq!(result, false);
    }
}

/// Removes all current kamikaze timers.
#[pyfunction]
#[pyo3(name = "destroy_kamikaze_timers")]
fn destroy_kamikaze_timers(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        let mut in_use_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use())
            .collect();

        in_use_entities
            .iter()
            .filter(|&game_entity| game_entity.get_health() <= 0)
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.remove_kamikaze_flag());

        in_use_entities
            .iter_mut()
            .filter(|game_entity| game_entity.is_kamikaze_timer())
            .for_each(|game_entity| game_entity.free_entity());

        Ok(true)
    })
}

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
#[pyo3(signature = (item_id, x, y, z))]
fn spawn_item(py: Python<'_>, item_id: i32, x: i32, y: i32, z: i32) -> PyResult<bool> {
    let max_items: i32 = GameItem::get_num_items();
    if !(1..max_items).contains(&item_id) {
        return Err(PyValueError::new_err(format!(
            "item_id needs to be a number from 1 to {}.",
            max_items - 1
        )));
    }

    py.allow_threads(move || {
        let mut gitem = GameItem::try_from(item_id).unwrap();
        gitem.spawn((x, y, z));
    });

    Ok(true)
}

/// Removes all dropped items.
#[pyfunction]
#[pyo3(name = "remove_dropped_items")]
fn remove_dropped_items(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.has_flags() && game_entity.is_dropped_item()
            })
            .for_each(|mut game_entity| game_entity.free_entity());
    });

    Ok(true)
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "slay_with_mod")]
fn slay_with_mod(py: Python<'_>, client_id: i32, mean_of_death: i32) -> PyResult<bool> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    let Ok(means_of_death): Result<meansOfDeath_t, _> = mean_of_death.try_into() else {
        return Err(PyValueError::new_err(
            "means of death needs to be a valid enum value.",
        ));
    };

    py.allow_threads(move || {
        let mut opt_game_entity = GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok());
        opt_game_entity.iter_mut().for_each(|game_entity| {
            if game_entity.get_health() > 0 {
                game_entity.slay_with_mod(means_of_death);
            }
        });
        Ok(opt_game_entity.is_some())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod slay_with_mod_tests {
    use super::slay_with_mod;
    use super::MAIN_ENGINE;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn slay_with_mod_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = slay_with_mod(py, 21, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, -1, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, 666, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_invalid_means_of_death() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = slay_with_mod(py, 2, 12345);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_remaining_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_slay_with_mod()
                .with(predicate::eq(meansOfDeath_t::MOD_PROXIMITY_MINE))
                .times(1);
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32))
                .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_no_remaining_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 0);
            mock_game_entity.expect_slay_with_mod().times(0);
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32))
                .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn slay_with_mod_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result =
            Python::with_gil(|py| slay_with_mod(py, 2, meansOfDeath_t::MOD_CRUSH as i32)).unwrap();
        assert_eq!(result, false);
    }
}

fn determine_item_id(item: &PyAny) -> PyResult<i32> {
    if let Ok(item_id) = item.extract::<i32>() {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            return Err(PyValueError::new_err(format!(
                "item_id needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }
        return Ok(item_id);
    }

    let Ok(item_classname) = item.extract::<String>() else {
        return Err(PyValueError::new_err(
            "item needs to be type of int or string.",
        ));
    };

    (1..GameItem::get_num_items())
        .filter(|i| {
            let game_item = GameItem::try_from(*i);
            game_item.is_ok() && game_item.unwrap().get_classname() == item_classname
        })
        .take(1)
        .next()
        .ok_or(PyValueError::new_err(format!(
            "invalid item classname: {}",
            item_classname
        )))
}

/// Replaces target entity's item with specified one.
#[pyfunction]
#[pyo3(name = "replace_items")]
#[pyo3(signature = (item1, item2))]
fn replace_items(py: Python<'_>, item1: Py<PyAny>, item2: Py<PyAny>) -> PyResult<bool> {
    let item2_id = determine_item_id(item2.as_ref(py))?;
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed

    if let Ok(item1_id) = item1.extract::<i32>(py) {
        // replacing item by entity_id

        // entity_id checking
        if item1_id < 0 || item1_id >= MAX_GENTITIES as i32 {
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
            )));
        }

        return py.allow_threads(move || {
            match GameEntity::try_from(item1_id) {
                Err(_) => return Err(PyValueError::new_err("game entity does not exist")),
                Ok(game_entity) => {
                    if !game_entity.in_use() {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not in use.",
                            item1_id
                        )));
                    }
                    if !game_entity.is_game_item(entityType_t::ET_ITEM) {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not item. Cannot replace it",
                            item1_id
                        )));
                    }
                    let mut mut_game_entity = game_entity;
                    mut_game_entity.replace_item(item2_id);
                }
            }
            Ok(true)
        });
    }

    if let Ok(item1_classname) = item1.extract::<String>(py) {
        let item_found = py.allow_threads(move || {
            let matching_item1_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
                .filter_map(|i| GameEntity::try_from(i as i32).ok())
                .filter(|game_entity| {
                    game_entity.in_use()
                        && game_entity.is_game_item(entityType_t::ET_ITEM)
                        && game_entity.get_classname() == item1_classname
                })
                .collect();
            let item_found = !matching_item1_entities.is_empty();
            matching_item1_entities
                .into_iter()
                .for_each(|mut game_entity| game_entity.replace_item(item2_id));
            item_found
        });
        return Ok(item_found);
    }

    Err(PyValueError::new_err(
        "entity needs to be type of int or string.",
    ))
}

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
fn dev_print_items(py: Python<'_>) -> PyResult<()> {
    let formatted_items: Vec<String> = py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
            })
            .map(|game_entity| {
                format!(
                    "{} {}",
                    game_entity.get_entity_id(),
                    game_entity.get_classname()
                )
            })
            .collect()
    });
    let mut str_length = 0;
    let printed_items: Vec<String> = formatted_items
        .iter()
        .take_while(|&item| {
            str_length += item.len();
            str_length < 1024
        })
        .map(|item| item.into())
        .collect();

    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if printed_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"No items found in the map\n\"".to_string(),
            );
            return Ok(());
        }
        main_engine.send_server_command(
            None::<Client>,
            format!("print \"{}\n\"", printed_items.join("\n")),
        );

        let remaining_items: Vec<String> = formatted_items
            .iter()
            .skip(printed_items.len())
            .map(|item| item.into())
            .collect();

        if !remaining_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"Check server console for other items\n\"\n".to_string(),
            );
            remaining_items
                .into_iter()
                .for_each(|item| main_engine.com_printf(item));
        }

        Ok(())
    })
}

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "force_weapon_respawn_time")]
fn force_weapon_respawn_time(py: Python<'_>, respawn_time: i32) -> PyResult<bool> {
    if respawn_time < 0 {
        return Err(PyValueError::new_err(
            "respawn time needs to be an integer 0 or greater",
        ));
    }

    py.allow_threads(move || {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| game_entity.in_use() && game_entity.is_respawning_weapon())
            .for_each(|mut game_entity| game_entity.set_respawn_time(respawn_time))
    });

    Ok(true)
}

/// get a list of entities that target a given entity
#[pyfunction]
#[pyo3(name = "get_targetting_entities")]
fn get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
    if entity_id < 0 || entity_id >= MAX_GENTITIES as i32 {
        return Err(PyValueError::new_err(format!(
            "entity_id need to be between 0 and {}.",
            MAX_GENTITIES - 1
        )));
    }

    py.allow_threads(move || {
        GameEntity::try_from(entity_id).map_or_else(
            |_| Ok(vec![]),
            |entity| Ok(entity.get_targetting_entity_ids()),
        )
    })
}

// Used primarily in Python, but defined here and added using PyModule_AddIntMacro().
#[allow(non_camel_case_types)]
enum PythonReturnCodes {
    RET_NONE,
    RET_STOP,       // Stop execution of event handlers within Python.
    RET_STOP_EVENT, // Only stop the event, but let other handlers process it.
    RET_STOP_ALL,   // Stop execution at an engine level. SCARY STUFF!
    RET_USAGE,      // Used for commands. Replies to the channel with a command's usage.
}

#[allow(non_camel_case_types)]
enum PythonPriorities {
    PRI_HIGHEST,
    PRI_HIGH,
    PRI_NORMAL,
    PRI_LOW,
    PRI_LOWEST,
}

#[pymodule]
#[pyo3(name = "shinqlx")]
fn pyshinqlx_module(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    Ok(())
}

#[pymodule]
#[pyo3(name = "_minqlx")]
fn pyminqlx_module(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_player_info, m)?)?;
    m.add_function(wrap_pyfunction!(get_players_info, m)?)?;
    m.add_function(wrap_pyfunction!(get_userinfo, m)?)?;
    m.add_function(wrap_pyfunction!(send_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(client_command, m)?)?;
    m.add_function(wrap_pyfunction!(console_command, m)?)?;
    m.add_function(wrap_pyfunction!(get_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(set_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(set_cvar_limit, m)?)?;
    m.add_function(wrap_pyfunction!(kick, m)?)?;
    m.add_function(wrap_pyfunction!(console_print, m)?)?;
    m.add_function(wrap_pyfunction!(get_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(force_vote, m)?)?;
    m.add_function(wrap_pyfunction!(add_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(register_handler, m)?)?;
    m.add_function(wrap_pyfunction!(player_state, m)?)?;
    m.add_function(wrap_pyfunction!(player_stats, m)?)?;
    m.add_function(wrap_pyfunction!(set_position, m)?)?;
    m.add_function(wrap_pyfunction!(set_velocity, m)?)?;
    m.add_function(wrap_pyfunction!(noclip, m)?)?;
    m.add_function(wrap_pyfunction!(set_health, m)?)?;
    m.add_function(wrap_pyfunction!(set_armor, m)?)?;
    m.add_function(wrap_pyfunction!(set_weapons, m)?)?;
    m.add_function(wrap_pyfunction!(set_weapon, m)?)?;
    m.add_function(wrap_pyfunction!(set_ammo, m)?)?;
    m.add_function(wrap_pyfunction!(set_powerups, m)?)?;
    m.add_function(wrap_pyfunction!(set_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(drop_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(set_flight, m)?)?;
    m.add_function(wrap_pyfunction!(set_invulnerability, m)?)?;
    m.add_function(wrap_pyfunction!(set_score, m)?)?;
    m.add_function(wrap_pyfunction!(callvote, m)?)?;
    m.add_function(wrap_pyfunction!(allow_single_player, m)?)?;
    m.add_function(wrap_pyfunction!(player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(set_privileges, m)?)?;
    m.add_function(wrap_pyfunction!(destroy_kamikaze_timers, m)?)?;
    m.add_function(wrap_pyfunction!(spawn_item, m)?)?;
    m.add_function(wrap_pyfunction!(remove_dropped_items, m)?)?;
    m.add_function(wrap_pyfunction!(slay_with_mod, m)?)?;
    m.add_function(wrap_pyfunction!(replace_items, m)?)?;
    m.add_function(wrap_pyfunction!(dev_print_items, m)?)?;
    m.add_function(wrap_pyfunction!(force_weapon_respawn_time, m)?)?;
    m.add_function(wrap_pyfunction!(get_entity_targets, m)?)?;

    m.add("__version__", env!("SHINQLX_VERSION"))?;
    m.add("DEBUG", cfg!(debug_assertions))?;

    // Set a bunch of constants. We set them here because if you define functions in Python that use module
    // constants as keyword defaults, we have to always make sure they're exported first, and fuck that.
    m.add("RET_NONE", PythonReturnCodes::RET_NONE as i32)?;
    m.add("RET_STOP", PythonReturnCodes::RET_STOP as i32)?;
    m.add("RET_STOP_EVENT", PythonReturnCodes::RET_STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", PythonReturnCodes::RET_STOP_ALL as i32)?;
    m.add("RET_USAGE", PythonReturnCodes::RET_USAGE as i32)?;
    m.add("PRI_HIGHEST", PythonPriorities::PRI_HIGHEST as i32)?;
    m.add("PRI_HIGH", PythonPriorities::PRI_HIGH as i32)?;
    m.add("PRI_NORMAL", PythonPriorities::PRI_NORMAL as i32)?;
    m.add("PRI_LOW", PythonPriorities::PRI_LOW as i32)?;
    m.add("PRI_LOWEST", PythonPriorities::PRI_LOWEST as i32)?;

    // Cvar flags.
    m.add("CVAR_ARCHIVE", cvar_flags::CVAR_ARCHIVE as i32)?;
    m.add("CVAR_USERINFO", cvar_flags::CVAR_USERINFO as i32)?;
    m.add("CVAR_SERVERINFO", cvar_flags::CVAR_SERVERINFO as i32)?;
    m.add("CVAR_SYSTEMINFO", cvar_flags::CVAR_SYSTEMINFO as i32)?;
    m.add("CVAR_INIT", cvar_flags::CVAR_INIT as i32)?;
    m.add("CVAR_LATCH", cvar_flags::CVAR_LATCH as i32)?;
    m.add("CVAR_ROM", cvar_flags::CVAR_ROM as i32)?;
    m.add("CVAR_USER_CREATED", cvar_flags::CVAR_USER_CREATED as i32)?;
    m.add("CVAR_TEMP", cvar_flags::CVAR_TEMP as i32)?;
    m.add("CVAR_CHEAT", cvar_flags::CVAR_CHEAT as i32)?;
    m.add("CVAR_NORESTART", cvar_flags::CVAR_NORESTART as i32)?;

    // Privileges.
    m.add("PRIV_NONE", privileges_t::PRIV_NONE as i32)?;
    m.add("PRIV_MOD", privileges_t::PRIV_MOD as i32)?;
    m.add("PRIV_ADMIN", privileges_t::PRIV_ADMIN as i32)?;
    m.add("PRIV_ROOT", privileges_t::PRIV_ROOT as i32)?;
    m.add("PRIV_BANNED", privileges_t::PRIV_BANNED as i32)?;

    // Connection states.
    m.add("CS_FREE", clientState_t::CS_FREE as i32)?;
    m.add("CS_ZOMBIE", clientState_t::CS_ZOMBIE as i32)?;
    m.add("CS_CONNECTED", clientState_t::CS_CONNECTED as i32)?;
    m.add("CS_PRIMED", clientState_t::CS_PRIMED as i32)?;
    m.add("CS_ACTIVE", clientState_t::CS_ACTIVE as i32)?;

    // Teams.
    m.add("TEAM_FREE", team_t::TEAM_FREE as i32)?;
    m.add("TEAM_RED", team_t::TEAM_RED as i32)?;
    m.add("TEAM_BLUE", team_t::TEAM_BLUE as i32)?;
    m.add("TEAM_SPECTATOR", team_t::TEAM_SPECTATOR as i32)?;

    // Means of death.
    m.add("MOD_UNKNOWN", meansOfDeath_t::MOD_UNKNOWN as i32)?;
    m.add("MOD_SHOTGUN", meansOfDeath_t::MOD_SHOTGUN as i32)?;
    m.add("MOD_GAUNTLET", meansOfDeath_t::MOD_GAUNTLET as i32)?;
    m.add("MOD_MACHINEGUN", meansOfDeath_t::MOD_MACHINEGUN as i32)?;
    m.add("MOD_GRENADE", meansOfDeath_t::MOD_GRENADE as i32)?;
    m.add(
        "MOD_GRENADE_SPLASH",
        meansOfDeath_t::MOD_GRENADE_SPLASH as i32,
    )?;
    m.add("MOD_ROCKET", meansOfDeath_t::MOD_ROCKET as i32)?;
    m.add(
        "MOD_ROCKET_SPLASH",
        meansOfDeath_t::MOD_ROCKET_SPLASH as i32,
    )?;
    m.add("MOD_PLASMA", meansOfDeath_t::MOD_PLASMA as i32)?;
    m.add(
        "MOD_PLASMA_SPLASH",
        meansOfDeath_t::MOD_PLASMA_SPLASH as i32,
    )?;
    m.add("MOD_RAILGUN", meansOfDeath_t::MOD_RAILGUN as i32)?;
    m.add("MOD_LIGHTNING", meansOfDeath_t::MOD_LIGHTNING as i32)?;
    m.add("MOD_BFG", meansOfDeath_t::MOD_BFG as i32)?;
    m.add("MOD_BFG_SPLASH", meansOfDeath_t::MOD_BFG_SPLASH as i32)?;
    m.add("MOD_WATER", meansOfDeath_t::MOD_WATER as i32)?;
    m.add("MOD_SLIME", meansOfDeath_t::MOD_SLIME as i32)?;
    m.add("MOD_LAVA", meansOfDeath_t::MOD_LAVA as i32)?;
    m.add("MOD_CRUSH", meansOfDeath_t::MOD_CRUSH as i32)?;
    m.add("MOD_TELEFRAG", meansOfDeath_t::MOD_TELEFRAG as i32)?;
    m.add("MOD_FALLING", meansOfDeath_t::MOD_FALLING as i32)?;
    m.add("MOD_SUICIDE", meansOfDeath_t::MOD_SUICIDE as i32)?;
    m.add("MOD_TARGET_LASER", meansOfDeath_t::MOD_TARGET_LASER as i32)?;
    m.add("MOD_TRIGGER_HURT", meansOfDeath_t::MOD_TRIGGER_HURT as i32)?;
    m.add("MOD_NAIL", meansOfDeath_t::MOD_NAIL as i32)?;
    m.add("MOD_CHAINGUN", meansOfDeath_t::MOD_CHAINGUN as i32)?;
    m.add(
        "MOD_PROXIMITY_MINE",
        meansOfDeath_t::MOD_PROXIMITY_MINE as i32,
    )?;
    m.add("MOD_KAMIKAZE", meansOfDeath_t::MOD_KAMIKAZE as i32)?;
    m.add("MOD_JUICED", meansOfDeath_t::MOD_JUICED as i32)?;
    m.add("MOD_GRAPPLE", meansOfDeath_t::MOD_GRAPPLE as i32)?;
    m.add("MOD_SWITCH_TEAMS", meansOfDeath_t::MOD_SWITCH_TEAMS as i32)?;
    m.add("MOD_THAW", meansOfDeath_t::MOD_THAW as i32)?;
    m.add(
        "MOD_LIGHTNING_DISCHARGE",
        meansOfDeath_t::MOD_LIGHTNING_DISCHARGE as i32,
    )?;
    m.add("MOD_HMG", meansOfDeath_t::MOD_HMG as i32)?;
    m.add(
        "MOD_RAILGUN_HEADSHOT",
        meansOfDeath_t::MOD_RAILGUN_HEADSHOT as i32,
    )?;

    m.add("DAMAGE_RADIUS", DAMAGE_RADIUS as i32)?;
    m.add("DAMAGE_NO_ARMOR", DAMAGE_NO_ARMOR as i32)?;
    m.add("DAMAGE_NO_KNOCKBACK", DAMAGE_NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_NO_PROTECTION", DAMAGE_NO_PROTECTION as i32)?;
    m.add(
        "DAMAGE_NO_TEAM_PROTECTION",
        DAMAGE_NO_TEAM_PROTECTION as i32,
    )?;

    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;
    m.add_class::<Vector3>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Flight>()?;

    Ok(())
}

pub(crate) static PYMINQLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn pyminqlx_is_initialized() -> bool {
    PYMINQLX_INITIALIZED.load(Ordering::SeqCst)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum PythonInitializationError {
    MainScriptError,
    #[cfg_attr(test, allow(dead_code))]
    AlreadyInitialized,
    NotInitializedError,
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyminqlx_initialize() -> Result<(), PythonInitializationError> {
    if pyminqlx_is_initialized() {
        error!(target: "shinqlx", "pyminqlx_initialize was called while already initialized");
        return Err(PythonInitializationError::AlreadyInitialized);
    }

    debug!(target: "shinqlx", "Initializing Python...");
    append_to_inittab!(pyminqlx_module);
    prepare_freethreaded_python();
    match Python::with_gil(|py| {
        let minqlx_module = py.import("minqlx")?;
        minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(e) => {
            error!(target: "shinqlx", "{:?}", e);
            error!(target: "shinqlx", "loader sequence returned an error. Did you modify the loader?");
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(_) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            debug!(target: "shinqlx", "Python initialized!");
            Ok(())
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyminqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyminqlx_is_initialized() {
        error!(target: "shinqlx", "pyminqlx_finalize was called before being initialized");
        return Err(PythonInitializationError::NotInitializedError);
    }

    [
        &CLIENT_COMMAND_HANDLER,
        &SERVER_COMMAND_HANDLER,
        &FRAME_HANDLER,
        &PLAYER_CONNECT_HANDLER,
        &PLAYER_LOADED_HANDLER,
        &PLAYER_DISCONNECT_HANDLER,
        &CUSTOM_COMMAND_HANDLER,
        &NEW_GAME_HANDLER,
        &SET_CONFIGSTRING_HANDLER,
        &RCON_HANDLER,
        &CONSOLE_PRINT_HANDLER,
        &PLAYER_SPAWN_HANDLER,
        &KAMIKAZE_USE_HANDLER,
        &KAMIKAZE_EXPLODE_HANDLER,
        &DAMAGE_HANDLER,
    ]
    .into_iter()
    .for_each(|handler_lock| handler_lock.store(None));

    match Python::with_gil(|py| {
        let importlib_module = py.import("importlib")?;
        let minqlx_module = py.import("minqlx")?;
        let new_minqlx_module = importlib_module.call_method1("reload", (minqlx_module,))?;
        new_minqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    }) {
        Err(_) => {
            PYMINQLX_INITIALIZED.store(false, Ordering::SeqCst);
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(()) => {
            PYMINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
pub(crate) mod python {
    use crate::pyminqlx::PythonInitializationError;

    pub(crate) fn rcon_dispatcher<T>(_cmd: T)
    where
        T: AsRef<str> + 'static,
    {
    }

    pub(crate) fn client_command_dispatcher(_client_id: i32, _cmd: String) -> Option<String> {
        None
    }
    pub(crate) fn server_command_dispatcher(
        _client_id: Option<i32>,
        _cmd: String,
    ) -> Option<String> {
        None
    }
    pub(crate) fn client_loaded_dispatcher(_client_id: i32) {}

    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: String) -> Option<String> {
        None
    }

    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: String) {}

    pub(crate) fn console_print_dispatcher(_msg: String) -> Option<String> {
        None
    }

    pub(crate) fn new_game_dispatcher(_restart: bool) {}

    pub(crate) fn frame_dispatcher() {}

    pub(crate) fn client_connect_dispatcher(_client_id: i32, _is_bot: bool) -> Option<String> {
        None
    }

    pub(crate) fn client_spawn_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_use_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_explode_dispatcher(_client_id: i32, _is_used_on_demand: bool) {}

    pub(crate) fn damage_dispatcher(
        _target_client_id: i32,
        _attacker_client_id: Option<i32>,
        _damage: i32,
        _dflags: i32,
        _means_of_death: i32,
    ) {
    }

    pub(crate) fn pyminqlx_is_initialized() -> bool {
        false
    }

    pub(crate) fn pyminqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }

    pub(crate) fn pyminqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}
