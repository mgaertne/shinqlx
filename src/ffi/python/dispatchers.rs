use super::prelude::*;

use core::sync::atomic::Ordering;

use log::error;

use pyo3::types::{PyBool, PyString};

pub(crate) fn client_command_dispatcher<T>(client_id: i32, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().to_string());
    }

    CLIENT_COMMAND_HANDLER
        .load()
        .as_ref()
        .map_or(Some(cmd.as_ref().to_string()), |client_command_handler| {
                Python::with_gil(|py| {
                    client_command_handler
                        .bind(py)
                        .call1((client_id, cmd.as_ref()))
                        .map_or_else(|e| {
                            error!(target: "shinqlx", "client_command_handler returned an error: {:?}.", e);
                            Some(cmd.as_ref().to_string())
                        }, |returned| {
                            if returned.downcast::<PyBool>().is_ok_and(|bool_value| !bool_value.is_true()) {
                                None
                            } else {
                                returned.downcast::<PyString>()
                                    .ok()
                                    .map_or(
                                        Some(cmd.as_ref().to_string()),
                                        |py_string| Some(py_string.to_string())
                                    )
                            }
                        })
                })
            })
}

pub(crate) fn server_command_dispatcher<T>(client_id: Option<i32>, cmd: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(cmd.as_ref().to_string());
    }

    SERVER_COMMAND_HANDLER
        .load()
        .as_ref()
        .map_or(Some(cmd.as_ref().to_string()), |server_command_handler| {
            Python::with_gil(|py| {
                server_command_handler
                    .bind(py)
                    .call1((client_id, cmd.as_ref()))
                    .map_or_else(|e| {
                        error!(target: "shinqlx", "server_command_handler returned an error: {:?}.", e);
                        Some(cmd.as_ref().to_string())
                    }, |returned| {
                        if returned.downcast::<PyBool>().is_ok_and(|bool_value| !bool_value.is_true()) {
                            None
                        } else {
                            returned.downcast::<PyString>()
                                .ok()
                                .map_or(
                                    Some(cmd.as_ref().to_string()),
                                    |py_string| Some(py_string.to_string())
                                )
                        }
                    })
            })
        })
}

pub(crate) fn frame_dispatcher() {
    if !pyshinqlx_is_initialized() {
        return;
    }

    FRAME_HANDLER.load().iter().for_each(|frame_handler| {
        Python::with_gil(|py| {
            if let Err(e) = frame_handler.bind(py).call0() {
                error!(target: "shinqlx", "frame_handler returned an error: {:?}.", e);
            }
        });
    });
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyshinqlx_is_initialized() {
        return None;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::Release);
    }

    let returned = PLAYER_CONNECT_HANDLER
        .load()
        .as_ref()
        .and_then(|client_connect_handler| {
            let result =
                Python::with_gil(|py| {
                    client_connect_handler.bind(py).call1((client_id, is_bot)).map_or_else(|e| {
                        error!(target: "shinqlx", "client_connect_handler returned an error: {:?}.", e);
                        None
                    }, |returned| {
                        if returned
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                        {
                            Some("You are banned from this server.".to_string())
                        } else {
                            returned
                                .downcast::<PyString>()
                                .ok()
                                .map(|py_string| py_string.to_string())
                        }
                    })
                });

            result
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

    PLAYER_DISCONNECT_HANDLER
        .load()
        .iter()
        .for_each(|client_disconnect_handler| {
            if let Err(e) = Python::with_gil(|py| {
                client_disconnect_handler.call1(py, (client_id, reason.as_ref()))
            }) {
                error!(target: "shinqlx", "client_disconnect_handler returned an error: {:?}.", e);
            };
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

    PLAYER_LOADED_HANDLER
        .load()
        .iter()
        .for_each(|client_loaded_handler| {
            Python::with_gil(|py| {
                if let Err(e) = client_loaded_handler.bind(py).call1((client_id,)) {
                    error!(target: "shinqlx", "client_loaded_handler returned an error: {:?}.", e);
                }
            });
        });
}

pub(crate) fn new_game_dispatcher(restart: bool) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    NEW_GAME_HANDLER.load().iter().for_each(|new_game_handler| {
        Python::with_gil(|py| {
            if let Err(e) = new_game_handler.bind(py).call1((restart,)) {
                error!(target: "shinqlx", "new_game_handler returned an error: {:?}.", e);
            }
        });
    });
}

pub(crate) fn set_configstring_dispatcher<T, U>(index: T, value: U) -> Option<String>
where
    T: Into<u32>,
    U: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(value.as_ref().to_string());
    }

    SET_CONFIGSTRING_HANDLER
        .load()
        .as_ref()
        .map_or(Some(value.as_ref().to_string()), |set_configstring_handler| {
            Python::with_gil(|py| {
                set_configstring_handler
                    .bind(py)
                    .call1((index.into(), value.as_ref()))
                    .map_or_else(|e| {
                        error!(target: "shinqlx", "set_configstring_handler returned an error: {:?}.", e);
                        Some(value.as_ref().to_string())
                    }, |returned| {
                        if returned
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true()) {
                            None
                        } else {
                             returned.downcast::<PyString>()
                                .ok()
                                .map_or(
                                     Some(value.as_ref().to_string()),
                                     |py_string| Some(py_string.to_string())
                                 )
                         }
                    })
            })
        })
}

pub(crate) fn rcon_dispatcher<T>(cmd: T)
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return;
    }

    RCON_HANDLER.load().iter().for_each(|rcon_handler| {
        Python::with_gil(|py| {
            if let Err(e) = rcon_handler.bind(py).call1((cmd.as_ref(),)) {
                error!(target: "shinqlx", "rcon_handler returned an error: {:?}.", e);
            }
        });
    });
}

pub(crate) fn console_print_dispatcher<T>(text: T) -> Option<String>
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        return Some(text.as_ref().to_string());
    }

    CONSOLE_PRINT_HANDLER
        .load()
        .as_ref()
        .map_or(Some(text.as_ref().to_string()), |console_print_handler| {
            Python::with_gil(|py| {
                console_print_handler
                    .bind(py)
                    .call1((text.as_ref(),))
                    .map_or_else(|e| {
                        error!(target: "shinqlx", "console_print_handler returned an error: {:?}.", e);
                        Some(text.as_ref().to_string())
                    }, |returned| {
                        if returned
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true()) {
                            None
                        } else {
                            returned.downcast::<PyString>()
                                .ok()
                                .map_or(
                                    Some(text.as_ref().to_string()),
                                    |py_string| Some(py_string.to_string())
                                )
                        }
                    })
            })
        })
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    PLAYER_SPAWN_HANDLER
        .load()
        .iter()
        .for_each(|client_spawn_handler| {
            Python::with_gil(|py| {
                if let Err(e) = client_spawn_handler.bind(py).call1((client_id,)) {
                    error!(target: "shinqlx", "client_spawn_handler returned an error: {:?}.", e);
                }
            });
        });
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    KAMIKAZE_USE_HANDLER
        .load()
        .iter()
        .for_each(|kamikaze_use_handler| {
            Python::with_gil(|py| {
                if let Err(e) = kamikaze_use_handler.bind(py).call1((client_id,)) {
                    error!(target: "shinqlx", "kamikaze_use_handler returned an error: {:?}.", e);
                }
            });
        });
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !pyshinqlx_is_initialized() {
        return;
    }

    KAMIKAZE_EXPLODE_HANDLER.load().iter().for_each(|kamikaze_explode_handler| {
        Python::with_gil(|py| {
            if let Err(e) = kamikaze_explode_handler.bind(py).call1((client_id, is_used_on_demand)) {
                error!(target: "shinqlx", "kamikaze_explode_handler returned an error: {:?}.", e);
            }
        });
    });
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

    DAMAGE_HANDLER.load().iter().for_each(|damage_handler| {
        Python::with_gil(|py| {
            if let Err(e) = damage_handler.bind(py).call1((
                target_client_id,
                attacker_client_id,
                damage,
                dflags,
                means_of_death,
            )) {
                error!(target: "shinqlx", "damage_handler returned an error: {:?}.", e);
            }
        });
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
    use rstest::rstest;

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[test]
    #[serial]
    fn client_command_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        CLIENT_COMMAND_HANDLER.store(None);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return cmd
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return "qwertz"
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, Some("qwertz".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return False
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return (1, 2, 3)
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CLIENT_COMMAND_HANDLER.store(Some(client_command_handler.unbind().into()));

            let result = client_command_dispatcher(123, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        SERVER_COMMAND_HANDLER.store(None);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return cmd
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return "qwertz"
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, Some("qwertz".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return False
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, cmd):
    return (1, 2, 3)
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let server_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SERVER_COMMAND_HANDLER.store(Some(server_command_handler.unbind().into()));

            let result = server_command_dispatcher(Some(123), "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        FRAME_HANDLER.store(None);

        frame_dispatcher();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let frame_handler = pymodule.getattr("handler").expect("this should not happen");
            FRAME_HANDLER.store(Some(frame_handler.unbind().into()));

            frame_dispatcher();
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn frame_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler():
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let frame_handler = pymodule.getattr("handler").expect("this should not happen");
            FRAME_HANDLER.store(Some(frame_handler.unbind().into()));

            frame_dispatcher();
        });
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        PLAYER_CONNECT_HANDLER.store(None);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_bot):
    return "qwertz"
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_connect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.unbind().into()));

            let result = client_connect_dispatcher(42, false);
            assert_eq!(result, Some("qwertz".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_bot):
    return True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_connect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.unbind().into()));

            let result = client_connect_dispatcher(42, true);
            assert_eq!(result, None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_bot):
    return False
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_connect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.unbind().into()));

            let result = client_connect_dispatcher(42, true);
            assert_eq!(result, Some("You are banned from this server.".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_bot):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_connect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_CONNECT_HANDLER.store(Some(client_connect_handler.unbind().into()));

            let result = client_connect_dispatcher(42, false);
            assert_eq!(result, None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_bot):
    return (1, 2, 3)
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let player_connect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_CONNECT_HANDLER.store(Some(player_connect_handler.unbind().into()));

            let result = client_connect_dispatcher(42, false);
            assert_eq!(result, None);
        });
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        client_disconnect_dispatcher(42, "asdf");
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        PLAYER_DISCONNECT_HANDLER.store(None);

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, reason):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_disconnect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.unbind().into()));

            client_disconnect_dispatcher(42, "ragequit");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, reason):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_disconnect_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_DISCONNECT_HANDLER.store(Some(client_disconnect_handler.unbind().into()));

            client_disconnect_dispatcher(42, "ragequit");
        });
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        PLAYER_LOADED_HANDLER.store(None);

        client_loaded_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_loaded_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.unbind().into()));

            client_loaded_dispatcher(123);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_loaded_handler =
                pymodule.getattr("handler").expect("this should not happen");
            PLAYER_LOADED_HANDLER.store(Some(client_loaded_handler.unbind().into()));

            client_loaded_dispatcher(123);
        });
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

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(restart):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let new_game_handler = pymodule.getattr("handler").expect("this should not happen");
            NEW_GAME_HANDLER.store(Some(new_game_handler.unbind().into()));

            new_game_dispatcher(false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn new_game_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(restart):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let new_game_handler = pymodule.getattr("handler").expect("this should not happen");
            NEW_GAME_HANDLER.store(Some(new_game_handler.unbind().into()));

            new_game_dispatcher(true);
        });
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        SET_CONFIGSTRING_HANDLER.store(None);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    return cmd
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    return "qwertz"
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, Some("qwertz".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    return True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    return False
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, None);
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(index, value):
    return (1, 2, 3)
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let set_configstring_handler =
                pymodule.getattr("handler").expect("this should not happen");
            SET_CONFIGSTRING_HANDLER.store(Some(set_configstring_handler.unbind().into()));

            let result = set_configstring_dispatcher(123u32, "asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        RCON_HANDLER.store(None);

        rcon_dispatcher("asdf");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(cmd):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let rcon_handler = pymodule.getattr("handler").expect("this should not happen");
            RCON_HANDLER.store(Some(rcon_handler.unbind().into()));

            rcon_dispatcher("asdf");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(cmd):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let rcon_handler = pymodule.getattr("handler").expect("this should not happen");
            RCON_HANDLER.store(Some(rcon_handler.unbind().into()));

            rcon_dispatcher("asdf");
        });
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        CONSOLE_PRINT_HANDLER.store(None);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, Some("asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    return cmd
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    return "qwertz"
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, Some("qwertz".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    return True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    return False
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(text):
    return (1, 2, 3)
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let console_print_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CONSOLE_PRINT_HANDLER.store(Some(console_print_handler.unbind().into()));

            let result = console_print_dispatcher("asdf");
            assert_eq!(result, Some("asdf".to_string()));
        });
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        PLAYER_SPAWN_HANDLER.store(None);

        client_spawn_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_spawn_handler = pymodule.getattr("handler").expect("this should not happen");
            PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.unbind().into()));

            client_spawn_dispatcher(123);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let client_spawn_handler = pymodule.getattr("handler").expect("this should not happen");
            PLAYER_SPAWN_HANDLER.store(Some(client_spawn_handler.unbind().into()));

            client_spawn_dispatcher(123);
        });
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        KAMIKAZE_USE_HANDLER.store(None);

        kamikaze_use_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let kamikaze_use_handler = pymodule.getattr("handler").expect("this should not happen");
            KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.unbind().into()));

            kamikaze_use_dispatcher(123);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let kamikaze_use_handler = pymodule.getattr("handler").expect("this should not happen");
            KAMIKAZE_USE_HANDLER.store(Some(kamikaze_use_handler.unbind().into()));

            kamikaze_use_dispatcher(123);
        });
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        kamikaze_explode_dispatcher(123, false);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_dispatcher_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        KAMIKAZE_EXPLODE_HANDLER.store(None);

        kamikaze_explode_dispatcher(123, true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_used_on_demand):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let kamikaze_explode_handler =
                pymodule.getattr("handler").expect("this should not happen");
            KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.unbind().into()));

            kamikaze_explode_dispatcher(123, false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, is_used_on_demand):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let kamikaze_explode_handler =
                pymodule.getattr("handler").expect("this should not happen");
            KAMIKAZE_EXPLODE_HANDLER.store(Some(kamikaze_explode_handler.unbind().into()));

            kamikaze_explode_dispatcher(123, true);
        });
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

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
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);
        DAMAGE_HANDLER.store(None);

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn damage_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, attacker_id, damage, dflags, means_of_death):
    pass
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let damage_handler = pymodule.getattr("handler").expect("this should not happen");
            DAMAGE_HANDLER.store(Some(damage_handler.unbind().into()));

            damage_dispatcher(
                123,
                Some(456),
                100,
                DAMAGE_NO_TEAM_PROTECTION as i32,
                meansOfDeath_t::MOD_ROCKET as i32,
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn damage_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code(
                py,
                cr#"
def handler(client_id, attacker_id, damage, dflags, means_of_death):
    raise Exception
"#,
                c"",
                c"",
            )
            .expect("this should not happen");
            let damage_handler = pymodule.getattr("handler").expect("this should not happen");
            DAMAGE_HANDLER.store(Some(damage_handler.unbind().into()));

            damage_dispatcher(
                123,
                None,
                666,
                DAMAGE_NO_PROTECTION as i32,
                meansOfDeath_t::MOD_TRIGGER_HURT as i32,
            );
        });
    }
}
