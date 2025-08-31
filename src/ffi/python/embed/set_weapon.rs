use core::hint::cold_path;

use pyo3::exceptions::PyValueError;
use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's current weapon.
#[pyfunction]
#[pyo3(name = "set_weapon")]
pub(crate) fn pyshinqlx_set_weapon(py: Python<'_>, client_id: i32, weapon: i32) -> PyResult<bool> {
    py.detach(|| {
        validate_client_id(client_id)?;

        if !(0..16).contains(&weapon) {
            cold_path();
            return Err(PyValueError::new_err(
                "Weapon must be a number from 0 to 15.",
            ));
        }

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.set_weapon(weapon);
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_weapon_tests {
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
    fn set_weapon_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_set_weapon(py, 21, weapon_t::WP_ROCKET_LAUNCHER.into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_weapon(py, -1, weapon_t::WP_GRAPPLING_HOOK.into());
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_weapon(py, 666, weapon_t::WP_PROX_LAUNCHER.into());
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_weapon_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_weapon(py, 2, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_weapon_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_weapon(py, 2, 42);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_existing_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapon()
                    .with(predicate::eq(weapon_t::WP_BFG as i32))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result =
                        Python::attach(|py| pyshinqlx_set_weapon(py, 2, weapon_t::WP_BFG.into()));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapon_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result =
                        Python::attach(|py| pyshinqlx_set_weapon(py, 2, weapon_t::WP_HMG.into()));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
