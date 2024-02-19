use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_execute_client_command;
#[cfg(not(test))]
use crate::hooks::shinqlx_execute_client_command;

/// Tells the server to process a command from a specific client.
#[pyfunction]
#[pyo3(name = "client_command")]
pub(crate) fn pyshinqlx_client_command(
    py: Python<'_>,
    client_id: i32,
    cmd: &str,
) -> PyResult<bool> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let opt_client = Client::try_from(client_id).ok().filter(|client| {
            ![clientState_t::CS_FREE, clientState_t::CS_ZOMBIE].contains(&client.get_state())
        });
        let returned = opt_client.is_some();
        if returned {
            shinqlx_execute_client_command(opt_client, cmd, true);
        }
        Ok(returned)
    })
}

#[cfg(test)]
mod client_command_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_execute_client_command_context;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_client_command(py, 0, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = pyshinqlx_client_command(py, -1, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        Python::with_gil(|py| {
            let result = pyshinqlx_client_command(py, 42, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_ACTIVE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[cfg_attr(miri, ignore)]
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

        let result = Python::with_gil(|py| pyshinqlx_client_command(py, 2, "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
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

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        let result = Python::with_gil(|py| pyshinqlx_client_command(py, 2, "asdf"));
        assert_eq!(result.expect("result was not OK"), false);
    }
}
