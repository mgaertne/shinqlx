use tap::TryConv;

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};
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
    py.detach(|| match client_id {
        None => {
            shinqlx_send_server_command(None, cmd);
            Ok(true)
        }
        Some(actual_client_id) => {
            validate_client_id(actual_client_id)?;

            let opt_client = actual_client_id
                .try_conv::<Client>()
                .ok()
                .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
            let returned = opt_client.is_some();
            if returned {
                shinqlx_send_server_command(opt_client, cmd);
            }
            Ok(returned)
        }
    })
}

#[cfg(test)]
mod send_server_command_tests {
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        hooks::mock_hooks::shinqlx_send_server_command_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_no_client_id(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "asdf")
            .times(1);
        let result = Python::attach(|py| pyshinqlx_send_server_command(py, None, "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::attach(|py| {
            let result = pyshinqlx_send_server_command(py, Some(0), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_client_id_below_zero(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_send_server_command(py, Some(-1), "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_client_id_above_max_clients(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_send_server_command(py, Some(42), "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_active_client(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::attach(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
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

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::attach(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }
}
