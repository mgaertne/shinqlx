use super::{
    pyshinqlx_is_initialized, ALLOW_FREE_CLIENT, CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER,
    DAMAGE_HANDLER, FRAME_HANDLER, KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER,
    NEW_GAME_HANDLER, PLAYER_CONNECT_HANDLER, PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER,
    PLAYER_SPAWN_HANDLER, RCON_HANDLER, SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
};
use core::sync::atomic::Ordering;
use log::error;
use pyo3::Python;

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
        return None;
    }

    let Some(ref client_connect_handler) = *PLAYER_CONNECT_HANDLER.load() else {
        return None;
    };

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::SeqCst);
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
        ALLOW_FREE_CLIENT.store(allowed_clients & !(1 << client_id as u64), Ordering::SeqCst);
    }

    result
}

pub(crate) fn client_disconnect_dispatcher<T>(client_id: i32, reason: T)
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return;
    }

    let Some(ref client_disconnect_handler) = *PLAYER_DISCONNECT_HANDLER.load() else {
        return;
    };

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::SeqCst);
    }

    let result =
        Python::with_gil(|py| client_disconnect_handler.call1(py, (client_id, reason.as_ref())));
    if result.is_err() {
        error!(target: "shinqlx", "client_disconnect_handler returned an error.");
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients & !(1 << client_id as u64), Ordering::SeqCst);
    }
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
    if !pyshinqlx_is_initialized() {
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
mod pyshinqlx_dispatcher_tests {
    use super::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
    };
    #[cfg(not(miri))]
    use crate::ffi::python::pyshinqlx_setup_fixture::*;
    use crate::ffi::python::PYSHINQLX_INITIALIZED;
    use crate::ffi::python::{
        CLIENT_COMMAND_HANDLER, CONSOLE_PRINT_HANDLER, DAMAGE_HANDLER, FRAME_HANDLER,
        KAMIKAZE_EXPLODE_HANDLER, KAMIKAZE_USE_HANDLER, NEW_GAME_HANDLER, PLAYER_CONNECT_HANDLER,
        PLAYER_DISCONNECT_HANDLER, PLAYER_LOADED_HANDLER, PLAYER_SPAWN_HANDLER, RCON_HANDLER,
        SERVER_COMMAND_HANDLER, SET_CONFIGSTRING_HANDLER,
    };
    use crate::prelude::*;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use pyo3::prelude::*;
    #[cfg(not(miri))]
    use rstest::rstest;

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn client_command_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        CLIENT_COMMAND_HANDLER.store(None);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.into()));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        SERVER_COMMAND_HANDLER.store(None);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let server_command_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SERVER_COMMAND_HANDLER.store(Some(server_command_handler.into()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        FRAME_HANDLER.store(None);

        frame_dispatcher();
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let frame_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        FRAME_HANDLER.store(Some(frame_handler.into()));

        frame_dispatcher();
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn frame_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let frame_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        FRAME_HANDLER.store(Some(frame_handler.into()));

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_CONNECT_HANDLER.store(None);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_connect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_connect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_connect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, Some("You are banned from this server.".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_connect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let player_connect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_CONNECT_HANDLER.store(Some(player_connect_handler.into()));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_disconnect_dispatcher(42, "asdf");
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_DISCONNECT_HANDLER.store(None);

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_disconnect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.into()));

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_disconnect_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.into()));

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_LOADED_HANDLER.store(None);

        client_loaded_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_loaded_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.into()));

        client_loaded_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_loaded_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.into()));

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        new_game_dispatcher(false);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        NEW_GAME_HANDLER.store(None);

        new_game_dispatcher(true);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn new_game_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let new_game_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        NEW_GAME_HANDLER.store(Some(new_game_handler.into()));

        new_game_dispatcher(false);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn new_game_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let new_game_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        NEW_GAME_HANDLER.store(Some(new_game_handler.into()));

        new_game_dispatcher(true);
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        SET_CONFIGSTRING_HANDLER.store(None);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let set_configstring_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.into()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        RCON_HANDLER.store(None);

        rcon_dispatcher("asdf");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let rcon_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        RCON_HANDLER.store(Some(rcon_handler.into()));

        rcon_dispatcher("asdf");
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn rcon_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let rcon_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        RCON_HANDLER.store(Some(rcon_handler.into()));

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        CONSOLE_PRINT_HANDLER.store(None);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, None);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let console_print_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.into()));

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        PLAYER_SPAWN_HANDLER.store(None);

        client_spawn_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_spawn_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.into()));

        client_spawn_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let client_spawn_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.into()));

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        KAMIKAZE_USE_HANDLER.store(None);

        kamikaze_use_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let kamikaze_use_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.into()));

        kamikaze_use_dispatcher(123);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let kamikaze_use_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.into()));

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_dispatcher_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
        KAMIKAZE_EXPLODE_HANDLER.store(None);

        kamikaze_explode_dispatcher(123, true);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let kamikaze_explode_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.into()));

        kamikaze_explode_dispatcher(123, false);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let kamikaze_explode_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
        KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.into()));

        kamikaze_explode_dispatcher(123, true);
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_python_not_initiailized() {
        PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);

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
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
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
    fn damage_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let damage_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
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
    fn damage_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);

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
            .expect("this should not happen")
            .into_py(py)
        });
        let damage_handler = Python::with_gil(|py| {
            pymodule
                .getattr(py, "handler")
                .expect("this should not happen")
                .into_py(py)
        });
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
