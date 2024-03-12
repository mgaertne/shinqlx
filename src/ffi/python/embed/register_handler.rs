use crate::ffi::python::prelude::*;

use pyo3::exceptions::{PyTypeError, PyValueError};

use alloc::sync::Arc;

/// Register an event handler. Can be called more than once per event, but only the last one will work.
#[pyfunction]
#[pyo3(name = "register_handler")]
#[pyo3(signature = (event, handler=None))]
pub(crate) fn pyshinqlx_register_handler(
    py: Python<'_>,
    event: &str,
    handler: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    if handler
        .as_ref()
        .is_some_and(|handler_function| !handler_function.is_callable())
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

    handler_lock.store(handler.map(|handler_func| Arc::new(handler_func.into_py(py))));
    Ok(())
}

#[cfg(test)]
mod register_handler_tests {
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use arc_swap::ArcSwapOption;
    use once_cell::sync::Lazy;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use rstest::rstest;

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
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_setting_handler_to_none(
        #[case] event: &str,
        #[case] handler: &Lazy<ArcSwapOption<PyObject>>,
    ) {
        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let py_handler = pymodule.getattr("handler").expect("this should not happen");
            handler.store(Some(py_handler.into_py(py).into()));

            let result = pyshinqlx_register_handler(py, event, None);
            assert!(result.is_ok());

            let stored_handler = handler.load();
            assert!(stored_handler.is_none());
        });
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
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_setting_handler_to_some_handler(
        #[case] event: &str,
        #[case] handler: &Lazy<ArcSwapOption<PyObject>>,
    ) {
        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let py_handler = pymodule.getattr("handler").expect("this should not happen");
            handler.store(None);

            let result = pyshinqlx_register_handler(py, event, Some(py_handler));
            assert!(result.is_ok());

            let stored_handler = handler.load();
            assert!(stored_handler.is_some());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_for_some_unknown_event() {
        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    pass
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let py_handler = pymodule.getattr("handler").expect("this should not happen");

            let result = pyshinqlx_register_handler(py, "unknown_event", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_handler_for_uncallable_handler() {
        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
handler = True
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let py_handler = pymodule.getattr("handler").expect("this should not happen");

            let result = pyshinqlx_register_handler(py, "client_command", Some(py_handler));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}
