use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use core::sync::atomic::Ordering;

/// Returns a string with a player's userinfo.
#[pyfunction(name = "get_userinfo")]
pub(crate) fn pyshinqlx_get_userinfo(py: Python<'_>, client_id: i32) -> PyResult<Option<String>> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let opt_client = Client::try_from(client_id).ok().filter(|client| {
            let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
            client.get_state() != clientState_t::CS_FREE
                || allowed_free_clients & (1 << client_id as u64) != 0
        });
        Ok(opt_client.map(|client| client.get_user_info().into()))
    })
}

#[cfg(test)]
mod get_userinfo_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use core::sync::atomic::Ordering;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_userinfo(py, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_client_id_below_zero(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_get_userinfo(py, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_client_id_above_max_clients(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_get_userinfo(py, 42);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_existing_client(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let userinfo = Python::with_gil(|py| pyshinqlx_get_userinfo(py, 2));
            assert_eq!(
                userinfo.expect("result was not OK"),
                Some("asdf".to_string())
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_non_allowed_free_client(_pyshinqlx_setup: ()) {
        ALLOW_FREE_CLIENT.store(0, Ordering::Release);

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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let userinfo = Python::with_gil(|py| pyshinqlx_get_userinfo(py, 2));
            assert_eq!(userinfo.expect("result was not OK"), None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_allowed_free_client(_pyshinqlx_setup: ()) {
        ALLOW_FREE_CLIENT.store(1 << 2, Ordering::Release);

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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let userinfo = Python::with_gil(|py| pyshinqlx_get_userinfo(py, 2));
            assert_eq!(
                userinfo.expect("result was not OK"),
                Some("asdf".to_string())
            );
        });
    }
}
