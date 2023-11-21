#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_send_server_command;
#[cfg(not(test))]
use crate::hooks::shinqlx_send_server_command;
use crate::prelude::*;
use crate::MAIN_ENGINE;

use pyo3::exceptions::{PyEnvironmentError, PyValueError};
use pyo3::prelude::*;

/// Sends a server command to either one specific client or all the clients.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
pub(crate) fn pyshinqlx_send_server_command(
    py: Python<'_>,
    client_id: Option<i32>,
    cmd: &str,
) -> PyResult<bool> {
    match client_id {
        None => {
            shinqlx_send_server_command(None, cmd);
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

            #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
            let opt_client = Client::try_from(actual_client_id)
                .ok()
                .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
            let returned = opt_client.is_some();
            if returned {
                shinqlx_send_server_command(opt_client, cmd);
            }
            Ok(returned)
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod send_server_command_tests {
    use super::pyshinqlx_send_server_command;
    use super::MAIN_ENGINE;
    use crate::ffi::c::client::MockClient;
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
        let result = Python::with_gil(|py| pyshinqlx_send_server_command(py, None, "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[serial]
    fn send_server_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = pyshinqlx_send_server_command(py, Some(0), "asdf");
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
            let result = pyshinqlx_send_server_command(py, Some(-1), "asdf");
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
            let result = pyshinqlx_send_server_command(py, Some(42), "asdf");
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

        let result = Python::with_gil(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
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

        let result = Python::with_gil(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
        assert_eq!(result.expect("result was not OK"), false);
    }
}
