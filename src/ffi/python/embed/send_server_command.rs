use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_send_server_command;
#[cfg(not(test))]
use crate::hooks::shinqlx_send_server_command;

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
        None => py.allow_threads(|| {
            shinqlx_send_server_command(None, cmd);
            Ok(true)
        }),
        Some(actual_client_id) => {
            validate_client_id(py, actual_client_id)?;

            py.allow_threads(|| {
                #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
                let opt_client = Client::try_from(actual_client_id)
                    .ok()
                    .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
                let returned = opt_client.is_some();
                if returned {
                    shinqlx_send_server_command(opt_client, cmd);
                }
                Ok(returned)
            })
        }
    }
}

#[cfg(test)]
mod send_server_command_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[test]
    #[cfg_attr(miri, ignore)]
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
    #[cfg_attr(miri, ignore)]
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
    #[cfg_attr(miri, ignore)]
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
    #[cfg_attr(miri, ignore)]
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
    #[cfg_attr(miri, ignore)]
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
    #[cfg_attr(miri, ignore)]
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
