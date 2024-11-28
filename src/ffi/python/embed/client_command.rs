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
    py.allow_threads(|| {
        validate_client_id(client_id)?;

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

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_client_command(py, 0, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_for_client_id_below_zero(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_client_command(py, -1, "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_for_client_id_above_max_clients(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_client_command(py, 42, "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[case(clientState_t::CS_ACTIVE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_active_client(
        #[case] clientstate: clientState_t,
        _pyshinqlx_setup: (),
    ) {
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_client_command(py, 2, "asdf"));
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_non_active_free_client(
        #[case] clientstate: clientState_t,
        _pyshinqlx_setup: (),
    ) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_execute_client_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_client_command(py, 2, "asdf"));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }
}
