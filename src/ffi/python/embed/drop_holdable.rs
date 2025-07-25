use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
pub(crate) fn pyshinqlx_drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.remove_kamikaze_flag();
            });

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .filter(|game_entity| {
                game_entity
                    .get_game_client()
                    .ok()
                    .filter(|game_client| Holdable::None != game_client.get_holdable().into())
                    .is_some()
            })
            .tap_some_mut(|game_entity| {
                game_entity.drop_holdable();
            })
            .is_some())
    })
}

#[cfg(test)]
mod drop_holdable_tests {
    use core::borrow::BorrowMut;

    use mockall::Sequence;
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
    fn drop_holdable_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_drop_holdable(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_drop_holdable(py, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_drop_holdable(py, 666);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_entity_with_no_holdable(_pyshinqlx_setup: ()) {
        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(seq.borrow_mut())
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(seq.borrow_mut())
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_get_holdable().returning(|| 0);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(0);
                mock_game_entity
            });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }

    #[rstest]
    #[case(&Holdable::Teleporter)]
    #[case(&Holdable::MedKit)]
    #[case(&Holdable::Kamikaze)]
    #[case(&Holdable::Portal)]
    #[case(&Holdable::Invulnerability)]
    #[case(&Holdable::Flight)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_entity_with_holdable_dropped(
        #[case] holdable: &'static Holdable,
        _pyshinqlx_setup: (),
    ) {
        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(seq.borrow_mut())
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag().times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(seq.borrow_mut())
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_get_holdable()
                        .returning(|| *holdable as i32);
                    Ok(mock_game_client)
                });
                mock_game_entity.expect_drop_holdable().times(1);
                mock_game_entity
            });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
            assert_eq!(result.expect("result was not OK"), true);
        });
    }
}
