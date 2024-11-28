use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use pyo3::exceptions::PyValueError;

/// Slay player with mean of death.
#[pyfunction]
#[pyo3(name = "slay_with_mod")]
pub(crate) fn pyshinqlx_slay_with_mod(
    py: Python<'_>,
    client_id: i32,
    mean_of_death: i32,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        let Ok(means_of_death): Result<meansOfDeath_t, _> = mean_of_death.try_into() else {
            return Err(PyValueError::new_err(
                "means of death needs to be a valid enum value.",
            ));
        };

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_game_entity = GameEntity::try_from(client_id)
            .ok()
            .filter(|game_entity| game_entity.get_game_client().is_ok());
        opt_game_entity.iter_mut().for_each(|game_entity| {
            if game_entity.get_health() > 0 {
                game_entity.slay_with_mod(means_of_death);
            }
        });
        Ok(opt_game_entity.is_some())
    })
}

#[cfg(test)]
mod slay_with_mod_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_slay_with_mod(py, 21, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result =
                    pyshinqlx_slay_with_mod(py, -1, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result =
                    pyshinqlx_slay_with_mod(py, 666, meansOfDeath_t::MOD_TRIGGER_HURT as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_invalid_means_of_death(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_slay_with_mod(py, 2, 12345);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_remaining_health(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
                .expect_slay_with_mod()
                .with(predicate::eq(meansOfDeath_t::MOD_PROXIMITY_MINE))
                .times(1);
            mock_game_entity
        });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32)
            });
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_no_remaining_health(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 0);
            mock_game_entity.expect_slay_with_mod().times(0);
            mock_game_entity
        });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32)
            });
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_CRUSH as i32)
            });
            assert_eq!(result.expect("result was not OK"), false);
        });
    }
}
