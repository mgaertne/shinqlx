use super::validate_client_id;
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Drops player's holdable item.
#[pyfunction]
#[pyo3(name = "drop_holdable")]
pub(crate) fn pyshinqlx_drop_holdable(py: Python<'_>, client_id: i32) -> PyResult<bool> {
    validate_client_id(py, client_id)?;

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .iter_mut()
            .for_each(|game_client| game_client.remove_kamikaze_flag());
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        let mut opt_game_entity_with_holdable =
            GameEntity::try_from(client_id).ok().filter(|game_entity| {
                game_entity
                    .get_game_client()
                    .ok()
                    .filter(|game_client| {
                        Holdable::from(game_client.get_holdable()) != Holdable::None
                    })
                    .is_some()
            });
        opt_game_entity_with_holdable
            .iter_mut()
            .for_each(|game_entity| game_entity.drop_holdable());
        Ok(opt_game_entity_with_holdable.is_some())
    })
}

#[cfg(test)]
mod drop_holdable_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use mockall::Sequence;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_drop_holdable(py, 21);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_client_id_too_small() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_drop_holdable(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_client_id_too_large() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = pyshinqlx_drop_holdable(py, 666);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_entity_with_no_game_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn drop_holdable_for_entity_with_no_holdable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
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
            .in_sequence(&mut seq)
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

        let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
        assert_eq!(result.expect("result was not OK"), false);
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
    fn drop_holdable_for_entity_with_holdable_dropped(#[case] holdable: &'static Holdable) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut seq = Sequence::new();

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .times(1)
            .in_sequence(&mut seq)
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
            .in_sequence(&mut seq)
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

        let result = Python::with_gil(|py| pyshinqlx_drop_holdable(py, 2));
        assert_eq!(result.expect("result was not OK"), true);
    }
}
