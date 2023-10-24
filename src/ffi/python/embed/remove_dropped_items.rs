use crate::prelude::*;

use pyo3::{pyfunction, PyResult, Python};

/// Removes all dropped items.
#[pyfunction]
#[pyo3(name = "remove_dropped_items")]
pub(crate) fn minqlx_remove_dropped_items(py: Python<'_>) -> PyResult<bool> {
    py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.has_flags() && game_entity.is_dropped_item()
            })
            .for_each(|mut game_entity| game_entity.free_entity());
    });

    Ok(true)
}

#[cfg(test)]
mod remove_dropped_items_tests {
    use super::minqlx_remove_dropped_items;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn remove_dropped_items_for_unused_entity() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity.expect_has_flags().returning(|| true);
                mock_game_entity.expect_is_dropped_item().returning(|| true);
                mock_game_entity.expect_free_entity().times(0);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_has_flags().returning(|| false);
            mock_game_entity
                .expect_is_dropped_item()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| minqlx_remove_dropped_items(py));
        assert!(result.is_ok_and(|value| value));
    }

    #[test]
    #[serial]
    fn remove_dropped_items_for_entity_without_flags() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_has_flags().returning(|| false);
                mock_game_entity.expect_is_dropped_item().returning(|| true);
                mock_game_entity.expect_free_entity().times(0);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_has_flags().returning(|| false);
            mock_game_entity
                .expect_is_dropped_item()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| minqlx_remove_dropped_items(py));
        assert!(result.is_ok_and(|value| value));
    }

    #[test]
    #[serial]
    fn remove_dropped_items_for_non_dropped_entity() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_has_flags().returning(|| true);
                mock_game_entity
                    .expect_is_dropped_item()
                    .returning(|| false);
                mock_game_entity.expect_free_entity().times(0);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity.expect_has_flags().returning(|| false);
            mock_game_entity
                .expect_is_dropped_item()
                .returning(|| false);
            mock_game_entity.expect_free_entity().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| minqlx_remove_dropped_items(py));
        assert!(result.is_ok_and(|value| value));
    }

    #[test]
    #[serial]
    fn remove_dropped_items_for_removable_dropped_entities() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity.expect_has_flags().returning(|| true);
            mock_game_entity.expect_is_dropped_item().returning(|| true);
            mock_game_entity.expect_free_entity().times(1);
            mock_game_entity
        });

        let result = Python::with_gil(|py| minqlx_remove_dropped_items(py));
        assert!(result.is_ok_and(|value| value));
    }
}