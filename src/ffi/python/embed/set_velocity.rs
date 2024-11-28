use super::validate_client_id;
use crate::ffi::python::*;

/// Sets a player's velocity vector.
#[pyfunction]
#[pyo3(name = "set_velocity")]
pub(crate) fn pyshinqlx_set_velocity(
    py: Python<'_>,
    client_id: i32,
    velocity: &Vector3,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        opt_game_client.iter_mut().for_each(|game_client| {
            game_client.set_velocity((velocity.0 as f32, velocity.1 as f32, velocity.2 as f32))
        });
        Ok(opt_game_client.is_some())
    })
}

#[cfg(test)]
mod set_velocity_tests {
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
    fn set_velocity_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        Python::with_gil(|py| {
            let result = pyshinqlx_set_velocity(py, 21, &vector);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_velocity_for_client_id_too_small(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_velocity(py, -1, &vector);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_velocity_for_client_id_too_large(_pyshinqlx_setup: ()) {
        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_velocity(py, 666, &vector);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_velocity_for_existing_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_velocity()
                    .with(predicate::eq((1.0, 2.0, 3.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_set_velocity(py, 2, &vector));
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_velocity_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let vector = Vector3(1, 2, 3);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_set_velocity(py, 2, &vector));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }
}
