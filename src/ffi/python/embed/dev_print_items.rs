use core::hint::cold_path;

use arrayvec::ArrayVec;
use pyo3::exceptions::PyEnvironmentError;
use rayon::prelude::*;
use tap::TryConv;

use crate::{
    MAIN_ENGINE,
    ffi::{c::prelude::*, python::prelude::*},
    quake_live_engine::{ComPrintf, SendServerCommand},
};

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
pub(crate) fn pyshinqlx_dev_print_items(py: Python<'_>) -> PyResult<()> {
    py.allow_threads(|| {
        let formatted_items: ArrayVec<String, { MAX_GENTITIES as usize }> = (0..MAX_GENTITIES)
            .filter_map(|i| (i as i32).try_conv::<GameEntity>().ok().map(Box::new))
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
            .collect();
        let mut str_length = 0;
        let printed_items = formatted_items
            .iter()
            .take_while(|&item| {
                str_length += item.len();
                str_length < 1024
            })
            .map(|item| item.to_string())
            .collect::<Vec<_>>();

        MAIN_ENGINE.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ))
            },
            |main_engine| {
                if printed_items.is_empty() {
                    main_engine.send_server_command(
                        None::<Client>,
                        "print \"No items found in the map\n\"",
                    );
                    return Ok(());
                }
                main_engine.send_server_command(
                    None::<Client>,
                    &format!("print \"{}\n\"", printed_items.join("\n")),
                );

                let remaining_items = formatted_items
                    .par_iter()
                    .skip(printed_items.len())
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>();

                if !remaining_items.is_empty() {
                    main_engine.send_server_command(
                        None::<Client>,
                        "print \"Check server console for other items\n\"\n",
                    );
                    remaining_items
                        .par_iter()
                        .for_each(|item| main_engine.com_printf(item));
                }

                Ok(())
            },
        )
    })
}

#[cfg(test)]
mod dev_print_items_tests {
    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dev_print_items_with_no_main_engine(_pyshinqlx_setup: ()) {
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dev_print_items_for_unused_game_item(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default()
            .with_send_server_command(
                |opt_client, cmd| {
                    opt_client.is_none() && cmd == "print \"No items found in the map\n\""
                },
                1,
            )
            .run(|| {
                let result = Python::with_gil(pyshinqlx_dev_print_items);
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dev_print_items_for_non_et_item(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default()
            .with_send_server_command(
                |opt_client, cmd| {
                    opt_client.is_none() && cmd == "print \"No items found in the map\n\""
                },
                1,
            )
            .run(|| {
                let result = Python::with_gil(pyshinqlx_dev_print_items);
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dev_print_items_prints_single_item(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default()
            .with_send_server_command(
                |opt_client, cmd| {
                    opt_client.is_none() && cmd == "print \"2 super important entity\n\""
                },
                1,
            )
            .run(|| {
                let result = Python::with_gil(pyshinqlx_dev_print_items);
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn dev_print_items_with_too_many_items_notifies_players_and_prints_remaining_items(
        _pyshinqlx_setup: (),
    ) {
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
                .returning(move || format!("super important entity {entity_id}").into());
            mock_game_entity
        });

        MockEngineBuilder::default()
            .with_com_printf(predicate::always(), 1..)
            .with_send_server_command(
                |opt_client, cmd| {
                    opt_client.is_none()
                        && cmd.starts_with(
                            "print \"0 super important entity 0\n1 super important entity 1\n",
                        )
                },
                1,
            )
            .with_send_server_command(
                |opt_client, cmd| {
                    opt_client.is_none()
                        && cmd == "print \"Check server console for other items\n\"\n"
                },
                1,
            )
            .run(|| {
                let result = Python::with_gil(pyshinqlx_dev_print_items);
                assert!(result.is_ok());
            });
    }
}
