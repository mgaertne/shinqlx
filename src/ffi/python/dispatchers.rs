use super::prelude::*;

use core::sync::atomic::Ordering;

use pyo3::types::{PyBool, PyString};

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().to_string());
    }

    Python::with_gil(|py| {
        let result = handle_client_command(py, client_id, cmd.as_ref());

        if result
            .bind(py)
            .downcast::<PyBool>()
            .is_ok_and(|bool_value| !bool_value.is_true())
        {
            None
        } else {
            result
                .bind(py)
                .downcast::<PyString>()
                .map_or(Some(cmd.as_ref().to_string()), |py_string| {
                    Some(py_string.to_string())
                })
        }
    })
}

pub(crate) fn server_command_dispatcher<T>(client_id: Option<i32>, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().to_string());
    }

    Python::with_gil(|py| {
        let result = handle_server_command(py, client_id.unwrap_or(-1), cmd.as_ref());

        if result.bind(py).is_instance_of::<PyBool>()
            && result
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)
        {
            None
        } else if result.bind(py).is_instance_of::<PyString>() {
            result.extract::<String>(py).ok()
        } else {
            Some(cmd.as_ref().to_string())
        }
    })
}

pub(crate) fn frame_dispatcher() {
    if !pyshinqlx_is_initialized() {
        return;
    }

    let _ = Python::with_gil(handle_frame);
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyshinqlx_is_initialized() {
        return None;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::Release);
    }

    let returned: Option<String> = Python::with_gil(|py| {
        let result = handle_player_connect(py, client_id, is_bot);

        if result
            .bind(py)
            .downcast::<PyBool>()
            .is_ok_and(|bool_value| !bool_value.is_true())
        {
            Some("You are banned from this server.".to_string())
        } else if result.bind(py).is_instance_of::<PyString>() {
            result.extract::<String>(py).ok()
        } else {
            None
        }
    });

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(
            allowed_clients & !(1 << client_id as u64),
            Ordering::Release,
        );
    }

    returned
}

pub(crate) fn client_disconnect_dispatcher<T>(client_id: i32, reason: T)
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::Release);
    }

    Python::with_gil(|py| {
        handle_player_disconnect(py, client_id, Some(reason.as_ref().to_string()))
    });

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(
            allowed_clients & !(1 << client_id as u64),
            Ordering::Release,
        );
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

    let _ = Python::with_gil(|py| handle_new_game(py, restart));
}

pub(crate) fn set_configstring_dispatcher<T, U>(index: T, value: U) -> Option<String>
where
    T: Into<u32>,
    U: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(value.as_ref().to_string());
    }

    Python::with_gil(|py| {
        let result = handle_set_configstring(py, index.into(), value.as_ref());

        if result.bind(py).is_instance_of::<PyBool>()
            && result
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)
        {
            None
        } else if result.bind(py).is_instance_of::<PyString>() {
            result.extract::<String>(py).ok()
        } else {
            Some(value.as_ref().to_string())
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

    Python::with_gil(|py| handle_rcon(py, cmd.as_ref()));
}

pub(crate) fn console_print_dispatcher<T>(text: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(text.as_ref().to_string());
    }

    Python::with_gil(|py| {
        let result = handle_console_print(py, text.as_ref());

        if result.bind(py).is_instance_of::<PyBool>()
            && result
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)
        {
            None
        } else if result.bind(py).is_instance_of::<PyString>() {
            result.extract::<String>(py).ok()
        } else {
            Some(text.as_ref().to_string())
        }
    })
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

    use rstest::*;

    use pretty_assertions::assert_eq;
    use pyo3::{
        IntoPyObjectExt,
        exceptions::PyException,
        types::{PyBool, PyString, PyTuple},
    };

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx.expect().times(0);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, cmd| PyString::new(py, cmd).into_any().unbind());

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| PyString::new(py, "qwertz").into_any().unbind());

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_context();
        handle_client_command_ctx.expect().returning(|py, _, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx.expect().times(0);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, cmd| PyString::new(py, cmd).into_any().unbind());

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| PyString::new(py, "qwertz").into_any().unbind());

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_context();
        handle_server_command_ctx.expect().returning(|py, _, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyString::new(py, "qwertz").into_any().unbind());

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, Some("You are banned from this server.".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().returning(|py, _, _| {
            PyException::new_err("asdf")
                .into_py_any(py)
                .expect("this should not happen")
        });

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().returning(|py, _, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx
            .expect()
            .returning(|py, _, _| py.None());

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx.expect().returning(|py, _, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx
            .expect()
            .returning(|py, _| py.None());

        client_loaded_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_new_game_ctx = handle_new_game_context();
        handle_new_game_ctx.expect().times(0);

        new_game_dispatcher(false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn new_game_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_new_game_ctx = handle_new_game_context();
        handle_new_game_ctx.expect().returning(|_, _| None);

        new_game_dispatcher(false);
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx.expect().times(0);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|py, _, value| PyString::new(py, value).into_any().unbind());

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|py, _, _| PyString::new(py, "qwertz").into_any().unbind());

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_context();
        handle_set_configstring_ctx.expect().returning(|py, _, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
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

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx.expect().times(0);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx
            .expect()
            .returning(|py, text| PyString::new(py, text).into_any().unbind());

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx
            .expect()
            .returning(|py, _| PyString::new(py, "qwertz").into_any().unbind());

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx
            .expect()
            .returning(|py, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx
            .expect()
            .returning(|py, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_context();
        handle_console_print_ctx.expect().returning(|py, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx
            .expect()
            .returning(|py, _| py.None());

        client_spawn_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();
        handle_kamikaze_use_ctx
            .expect()
            .returning(|py, _| py.None());

        kamikaze_use_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();

        handle_kamikaze_use_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx
            .expect()
            .returning(|py, _, _| py.None());

        kamikaze_explode_dispatcher(123, false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx.expect().returning(|py, _, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn damage_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
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
