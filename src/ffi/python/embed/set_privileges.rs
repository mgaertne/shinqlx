use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's privileges. Does not persist.
#[pyfunction]
#[pyo3(name = "set_privileges")]
pub(crate) fn pyshinqlx_set_privileges(
    py: Python<'_>,
    client_id: i32,
    privileges: i32,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.set_privileges(privileges);
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_privileges_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_privileges_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_privileges(py, 21, privileges_t::PRIV_MOD as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_privileges_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_privileges(py, -1, privileges_t::PRIV_MOD as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_privileges_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_privileges(py, 666, privileges_t::PRIV_MOD as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[case(&privileges_t::PRIV_NONE)]
    #[case(&privileges_t::PRIV_MOD)]
    #[case(&privileges_t::PRIV_ADMIN)]
    #[case(&privileges_t::PRIV_ROOT)]
    #[case(&privileges_t::PRIV_BANNED)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_privileges_for_existing_game_client(
        #[case] privileges: &'static privileges_t,
        _pyshinqlx_setup: (),
    ) {
        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_privileges()
                    .with(predicate::eq(*privileges as i32))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result =
                        Python::with_gil(|py| pyshinqlx_set_privileges(py, 2, *privileges as i32));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_privileges_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_set_privileges(py, 2, privileges_t::PRIV_NONE as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
