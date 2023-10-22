use crate::ffi::python::ALLOW_FREE_CLIENT;
#[cfg(test)]
use crate::ffi::python::DUMMY_MAIN_ENGINE as MAIN_ENGINE;
use crate::prelude::*;
#[cfg(not(test))]
use crate::MAIN_ENGINE;
use pyo3::exceptions::{PyEnvironmentError, PyValueError};
use pyo3::prelude::*;
use std::sync::atomic::Ordering;

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
pub(crate) fn minqlx_get_userinfo(py: Python<'_>, client_id: i32) -> PyResult<Option<String>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    if !(0..maxclients).contains(&client_id) {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            maxclients - 1
        )));
    }

    py.allow_threads(move || {
        let opt_client = Client::try_from(client_id).ok().filter(|client| {
            let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::SeqCst);
            client.get_state() != clientState_t::CS_FREE
                || allowed_free_clients & (1 << client_id as u64) != 0
        });
        Ok(opt_client.map(|client| client.get_user_info()))
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_userinfo_tests {
    use super::MAIN_ENGINE;
    use super::{minqlx_get_userinfo, ALLOW_FREE_CLIENT};
    use crate::ffi::c::client::MockClient;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_userinfo_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = minqlx_get_userinfo(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_below_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = minqlx_get_userinfo(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_client_id_above_max_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        Python::with_gil(|py| {
            let result = minqlx_get_userinfo(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn get_userinfo_for_existing_client() {
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
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| minqlx_get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }

    #[test]
    #[serial]
    fn get_userinfo_for_non_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(0, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| minqlx_get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, None);
    }

    #[test]
    #[serial]
    fn get_userinfo_for_allowed_free_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        ALLOW_FREE_CLIENT.store(1 << 2, Ordering::SeqCst);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client
        });

        let userinfo = Python::with_gil(|py| minqlx_get_userinfo(py, 2).unwrap());
        assert_eq!(userinfo, Some("asdf".into()));
    }
}
