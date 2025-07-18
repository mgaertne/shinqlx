use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's holdable item.
#[pyfunction]
#[pyo3(name = "set_holdable")]
pub(crate) fn pyshinqlx_set_holdable(
    py: Python<'_>,
    client_id: i32,
    holdable: i32,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                let ql_holdable: Holdable = holdable.into();
                game_client.set_holdable(ql_holdable);
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_holdable_tests {
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
    fn set_holdable_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_holdable(py, 21, Holdable::Kamikaze as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_holdable(py, -1, Holdable::Invulnerability as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_holdable(py, 666, Holdable::Teleporter as i32);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_existing_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_holdable()
                    .with(predicate::eq(Holdable::Kamikaze))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_set_holdable(py, 2, Holdable::Kamikaze as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| {
                        pyshinqlx_set_holdable(py, 2, Holdable::Invulnerability as i32)
                    });
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
