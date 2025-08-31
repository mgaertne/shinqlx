use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's powerups.
#[pyfunction]
#[pyo3(name = "set_powerups")]
pub(crate) fn pyshinqlx_set_powerups(
    py: Python<'_>,
    client_id: i32,
    powerups: &Powerups,
) -> PyResult<bool> {
    py.detach(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.set_powerups((*powerups).into());
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_powerups_tests {
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
    fn set_powerups_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let powerups = Powerups(0, 0, 0, 0, 0, 0);

        Python::attach(|py| {
            let result = pyshinqlx_set_powerups(py, 21, &powerups);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_powerups_for_client_id_too_small(_pyshinqlx_setup: ()) {
        let powerups = Powerups(0, 0, 0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_powerups(py, -1, &powerups);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_powerups_for_client_id_too_large(_pyshinqlx_setup: ()) {
        let powerups = Powerups(0, 0, 0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_powerups(py, 666, &powerups);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_powerups_for_existing_game_client(_pyshinqlx_setup: ()) {
        let powerups = Powerups(1, 2, 3, 4, 5, 6);

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_powerups()
                    .with(predicate::eq([1, 2, 3, 4, 5, 6]))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_set_powerups(py, 2, &powerups));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_powerups_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let powerups = Powerups(0, 0, 0, 0, 0, 0);

        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_set_powerups(py, 2, &powerups));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
