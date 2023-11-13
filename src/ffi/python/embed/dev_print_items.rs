use crate::prelude::*;
use crate::quake_live_engine::{ComPrintf, SendServerCommand};
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::{pyfunction, PyResult, Python};

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
pub(crate) fn pyshinqlx_dev_print_items(py: Python<'_>) -> PyResult<()> {
    let formatted_items: Vec<String> = py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
            })
            .map(|game_entity| {
                format!(
                    "{} {}",
                    game_entity.get_entity_id(),
                    game_entity.get_classname()
                )
            })
            .collect()
    });
    let mut str_length = 0;
    let printed_items: Vec<String> = formatted_items
        .iter()
        .take_while(|&item| {
            str_length += item.len();
            str_length < 1024
        })
        .map(|item| item.into())
        .collect();

    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if printed_items.is_empty() {
            main_engine
                .send_server_command(None::<Client>, "print \"No items found in the map\n\"");
            return Ok(());
        }
        main_engine.send_server_command(
            None::<Client>,
            &format!("print \"{}\n\"", printed_items.join("\n")),
        );

        let remaining_items: Vec<String> = formatted_items
            .iter()
            .skip(printed_items.len())
            .map(|item| item.into())
            .collect();

        if !remaining_items.is_empty() {
            main_engine.send_server_command(
                None::<Client>,
                "print \"Check server console for other items\n\"\n",
            );
            remaining_items
                .into_iter()
                .for_each(|item| main_engine.com_printf(&item));
        }

        Ok(())
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod dev_print_items_tests {
    use super::pyshinqlx_dev_print_items;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use crate::MAIN_ENGINE;
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn dev_print_items_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| false);
            mock_game_entity
        });

        Python::with_gil(|py| {
            let result = pyshinqlx_dev_print_items(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn dev_print_items_for_unused_game_item() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|opt_client, cmd| {
                opt_client.is_none() && cmd == "print \"No items found in the map\n\""
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| false);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_dev_print_items);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn dev_print_items_for_non_et_item() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|opt_client, cmd| {
                opt_client.is_none() && cmd == "print \"No items found in the map\n\""
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| false);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| false);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_dev_print_items);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn dev_print_items_prints_single_item() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|opt_client, cmd| {
                opt_client.is_none() && cmd == "print \"2 super important entity\n\""
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|entity_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_entity_id()
                    .returning(move || entity_id);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "super important entity".into());
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| false);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| false);
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_dev_print_items);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn dev_print_items_with_too_many_items_notifies_players_and_prints_remaining_items() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|opt_client, cmd| {
                opt_client.is_none()
                    && cmd.starts_with(
                        "print \"0 super important entity 0\n1 super important entity 1\n",
                    )
            })
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|opt_client, cmd| {
                opt_client.is_none() && cmd == "print \"Check server console for other items\n\"\n"
            })
            .times(1);
        mock_engine.expect_com_printf().times(1..);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|entity_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| true);
            mock_game_entity
                .expect_get_entity_id()
                .returning(move || entity_id);
            mock_game_entity
                .expect_get_classname()
                .returning(move || format!("super important entity {}", entity_id));
            mock_game_entity
        });

        let result = Python::with_gil(pyshinqlx_dev_print_items);
        assert!(result.is_ok());
    }
}
