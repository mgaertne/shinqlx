use core::hint::cold_path;

use pyo3::exceptions::PyValueError;
use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

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

        mean_of_death.try_conv::<meansOfDeath_t>().map_or(
            {
                cold_path();
                Err(PyValueError::new_err(
                    "means of death needs to be a valid enum value.",
                ))
            },
            |means_of_death| {
                Ok(client_id
                    .try_conv::<GameEntity>()
                    .ok()
                    .filter(|game_entity| game_entity.get_game_client().is_ok())
                    .tap_some_mut(|game_entity| {
                        if game_entity.get_health() > 0 {
                            game_entity.slay_with_mod(means_of_death);
                        }
                    })
                    .is_some())
            },
        )
    })
}

#[cfg(test)]
mod slay_with_mod_tests {
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
        MockGameEntityBuilder::default()
            .with_game_client(|| Ok(MockGameClient::new()))
            .with_health(42, 1..)
            .with_slay_with_mod(predicate::eq(meansOfDeath_t::MOD_PROXIMITY_MINE), 1)
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_existing_game_client_with_no_remaining_health(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Ok(MockGameClient::new()))
            .with_health(0, 1..)
            .with_slay_with_mod(predicate::always(), 0)
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_PROXIMITY_MINE as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_with_mod_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_slay_with_mod(py, 2, meansOfDeath_t::MOD_CRUSH as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
