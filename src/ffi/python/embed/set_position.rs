use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's position vector.
#[pyfunction]
#[pyo3(name = "set_position")]
pub(crate) fn pyshinqlx_set_position(
    py: Python<'_>,
    client_id: i32,
    position: &Vector3,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.set_position((position.0 as f32, position.1 as f32, position.2 as f32))
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_position_tests {
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
    fn set_position_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        Python::with_gil(|py| {
            let result = pyshinqlx_set_position(py, 21, &vector);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_position_for_client_id_too_small(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_position(py, -1, &vector);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_position_for_client_id_too_large(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_position(py, 666, &vector);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_position_for_existing_game_client(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_position()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_set_position(py, 2, &vector));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_position_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_set_position(py, 2, &vector));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
