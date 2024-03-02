use super::prelude::*;

use core::sync::atomic::Ordering;
use log::error;

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    Python::with_gil(|py| {
        let returned = handle_client_command(py, client_id, cmd.as_ref().to_string());
        match returned.extract::<String>(py) {
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
        }
    })
}

pub(crate) fn server_command_dispatcher<T>(client_id: Option<i32>, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().into());
    }

    Python::with_gil(|py| {
        let returned = handle_server_command(py, client_id.unwrap_or(-1), cmd.as_ref().to_string());
        match returned.extract::<String>(py) {
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
        }
    })
}

pub(crate) fn frame_dispatcher() {
    if !pyshinqlx_is_initialized() {
        return;
    }

    let _ = Python::with_gil(handle_frame);
}

#[allow(clippy::question_mark)]
pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyshinqlx_is_initialized() {
        return None;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::SeqCst);
    }

    let result: Option<String> = Python::with_gil(|py| {
        let returned = handle_player_connect(py, client_id, is_bot);
        match returned.extract::<String>(py) {
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
        }
    });

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

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::SeqCst);
    }

    Python::with_gil(|py| handle_player_disconnect(py, client_id, Some(reason.as_ref().into())));

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
        ALLOW_FREE_CLIENT.store(allowed_clients & !(1 << client_id as u64), Ordering::SeqCst);
    }
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    Python::with_gil(|py| handle_player_loaded(py, client_id));
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

    Python::with_gil(|py| handle_rcon(py, cmd.as_ref().to_string()));
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

    Python::with_gil(|py| handle_player_spawn(py, client_id));
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    Python::with_gil(|py| handle_kamikaze_use(py, client_id));
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    Python::with_gil(|py| handle_kamikaze_explode(py, client_id, is_used_on_demand));
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

    Python::with_gil(|py| {
        handle_damage(
            py,
            target_client_id,
            attacker_client_id,
            damage,
            dflags,
            means_of_death,
        )
    });
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
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyException;
    use rstest::rstest;

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx.expect().times(0);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, cmd| cmd.into_py(py));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| "qwertz".into_py(py));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| true.into_py(py));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| false.into_py(py));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| (1, 2, 3).into_py(py));

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx.expect().times(0);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, cmd| cmd.into_py(py));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| "qwertz".into_py(py));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("qwertz".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| true.into_py(py));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| false.into_py(py));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| (1, 2, 3).into_py(py));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_frame_ctx = handle_frame_context();
        handle_frame_ctx.expect().times(0);

        frame_dispatcher();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_frame_ctx = handle_frame_context();
        handle_frame_ctx.expect().returning(|_| None);

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().times(0);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| "qwertz".into_py(py));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, Some("qwertz".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| true.into_py(py));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| false.into_py(py));

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, Some("You are banned from this server.".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyException::new_err("asdf").into_py(py));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| (1, 2, 3).into_py(py));

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx.expect().times(0);

        client_disconnect_dispatcher(42, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx
            .expect()
            .returning(|py, _, _| py.None());

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx
            .expect()
            .returning(|py, _, _| PyException::new_err("").into_py(py));

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx.expect().times(0);

        client_loaded_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx
            .expect()
            .returning(|py, _| py.None());

        client_loaded_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx
            .expect()
            .returning(|py, _| PyException::new_err("").into_py(py));

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        new_game_dispatcher(false);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        NEW_GAME_HANDLER.store(None);

        new_game_dispatcher(true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn new_game_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn new_game_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        SET_CONFIGSTRING_HANDLER.store(None);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_rcon_ctx = handle_rcon_context();
        handle_rcon_ctx.expect().times(0);

        rcon_dispatcher("asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_rcon_ctx = handle_rcon_context();
        handle_rcon_ctx.expect();

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        CONSOLE_PRINT_HANDLER.store(None);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".into()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

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
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx.expect().times(0);

        client_spawn_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx
            .expect()
            .returning(|py, _| py.None());

        client_spawn_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx
            .expect()
            .returning(|py, _| PyException::new_err("").into_py(py));

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();
        handle_kamikaze_use_ctx.expect().times(0);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();
        handle_kamikaze_use_ctx
            .expect()
            .returning(|py, _| py.None());

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();

        handle_kamikaze_use_ctx
            .expect()
            .returning(|py, _| PyException::new_err("").into_py(py));
        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx.expect().times(0);

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx
            .expect()
            .returning(|py, _, _| py.None());

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx
            .expect()
            .returning(|py, _, _| PyException::new_err("").into_py(py));

        kamikaze_explode_dispatcher(123, true);
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_damage_ctx = handle_damage_context();
        handle_damage_ctx.expect().times(0);

        damage_dispatcher(
            123,
            None,
            666,
            DAMAGE_NO_PROTECTION as i32,
            meansOfDeath_t::MOD_TRIGGER_HURT as i32,
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn damage_dispatcher_dispatcher_works_properly() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_damage_ctx = handle_damage_context();
        handle_damage_ctx
            .expect()
            .returning(|_, _, _, _, _, _| None);

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }
}
