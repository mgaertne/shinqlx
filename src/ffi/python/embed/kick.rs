use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_drop_client;
#[cfg(not(test))]
use crate::hooks::shinqlx_drop_client;

use pyo3::exceptions::PyValueError;

/// Kick a player and allowing the admin to supply a reason for it.
#[pyfunction]
#[pyo3(name = "kick")]
#[pyo3(signature = (client_id, reason=None))]
pub(crate) fn pyshinqlx_kick(py: Python<'_>, client_id: i32, reason: Option<&str>) -> PyResult<()> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_client = Client::try_from(client_id)
            .ok()
            .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
        let reason_str = reason
            .filter(|rsn| !rsn.is_empty())
            .unwrap_or("was kicked.");
        opt_client
            .iter_mut()
            .for_each(|client| shinqlx_drop_client(client, reason_str));
        if opt_client.is_some() {
            Ok(())
        } else {
            Err(PyValueError::new_err(
                "client_id must be the ID of an active player.",
            ))
        }
    })
}

#[cfg(test)]
mod kick_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::hooks::mock_hooks::shinqlx_drop_client_context;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use mockall::predicate;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_kick(py, 0, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_kick(py, -1, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_kick(py, 42, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_for_non_active_client(#[case] clientstate: clientState_t) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(clientstate);
                mock_client
            });

        Python::with_gil(|py| {
            let result = pyshinqlx_kick(py, 2, None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_for_active_client_without_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_kick(py, 2, None));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_for_active_client_with_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "please go away!")
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_kick(py, 2, Some("please go away!")));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_with_client_id_for_active_client_with_empty_kick_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_kick(py, 2, Some("")));
        assert!(result.is_ok());
    }
}
