use arrayvec::ArrayVec;
use pyo3::exceptions::PyValueError;
use tap::TryConv;

use crate::ffi::{c::prelude::*, python::prelude::*};

fn determine_item_id(item: &Bound<PyAny>) -> PyResult<i32> {
    match item.extract::<i32>() {
        Ok(item_id) if (0..GameItem::get_num_items()).contains(&item_id) => Ok(item_id),
        Ok(_) => Err(PyValueError::new_err(format!(
            "item2 needs to be between 0 and {}.",
            GameItem::get_num_items() - 1
        ))),
        Err(_) => match item.extract::<String>() {
            Ok(item_classname) => item.py().allow_threads(|| {
                (1..GameItem::get_num_items())
                    .filter(|&i| {
                        i.try_conv::<GameItem>()
                            .is_ok_and(|game_item| game_item.get_classname() == item_classname)
                    })
                    .take(1)
                    .next()
                    .ok_or(PyValueError::new_err(format!(
                        "invalid item classname: {item_classname}"
                    )))
            }),
            Err(_) => Err(PyValueError::new_err(
                "item2 needs to be of type int or string.",
            )),
        },
    }
}

/// Replaces target entity's item with specified one.
#[pyfunction]
#[pyo3(name = "replace_items")]
pub(crate) fn pyshinqlx_replace_items(
    py: Python<'_>,
    item1: &Bound<'_, PyAny>,
    item2: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let item2_id = determine_item_id(item2)?;
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed

    match item1.extract::<i32>() {
        Ok(item1_id) if (0..GameItem::get_num_items()).contains(&item1_id) => {
            py.allow_threads(|| {
                item1_id
                    .try_conv::<GameEntity>()
                    .ok()
                    .filter(|game_entity| {
                        game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
                    })
                    .map_or(
                        Err(PyValueError::new_err(format!(
                            "entity #{item1_id} is not a valid game item"
                        ))),
                        |mut game_entity| {
                            game_entity.replace_item(item2_id);
                            Ok(true)
                        },
                    )
            })
        }
        Ok(_) => Err(PyValueError::new_err(format!(
            "item1 needs to be between 0 and {}.",
            GameItem::get_num_items() - 1
        ))),
        Err(_) => match item1.extract::<String>() {
            Ok(item1_classname) => py.allow_threads(|| {
                let mut matching_item1_entities: ArrayVec<
                    Box<GameEntity>,
                    { MAX_GENTITIES as usize },
                > = (0..MAX_GENTITIES)
                    .filter_map(|i| (i as i32).try_conv::<GameEntity>().ok().map(Box::new))
                    .filter(|game_entity| {
                        game_entity.in_use()
                            && game_entity.is_game_item(entityType_t::ET_ITEM)
                            && game_entity.get_classname() == item1_classname
                    })
                    .collect();

                matching_item1_entities
                    .iter_mut()
                    .for_each(|game_entity| game_entity.replace_item(item2_id));

                Ok(!matching_item1_entities.is_empty())
            }),
            Err(_) => Err(PyValueError::new_err(
                "item1 needs to be of type int or string.",
            )),
        },
    }
}

#[cfg(test)]
mod replace_items_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{
        exceptions::PyValueError,
        types::{PyInt, PyString, PyTuple},
    };
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_too_small_item1_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, -1i32).as_any(),
                PyInt::new(py, 1i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_too_large_item1_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 43i32).as_any(),
                PyInt::new(py, 1i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_too_small_item2_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, -1i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_too_large_item2_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, 43i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_item1_not_integer_nor_string(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyTuple::new(py, [1i32, 2i32])
                    .expect("this should not happen")
                    .as_ref(),
                PyInt::new(py, 1i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_item2_not_integer_nor_string(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyTuple::new(py, [1i32, 2i32])
                    .expect("this should not happen")
                    .as_ref(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_item1_string_not_existing_classname(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| true);
            mock_game_entity
                .expect_get_classname()
                .returning(|| "available_classname".into());
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            pyshinqlx_replace_items(
                py,
                PyString::intern(py, "not existing classname").as_ref(),
                PyInt::new(py, 1i32).as_any(),
            )
        });
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_item2_string_not_existing_classname(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_item_from_ctx = MockGameItem::from_context();
        game_item_from_ctx.expect().returning(|_| {
            let mut mock_game_item = MockGameItem::new();
            mock_game_item
                .expect_get_classname()
                .returning(|| "available_classname".into());
            mock_game_item
        });

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyString::intern(py, "not existing classname").as_ref(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_not_in_use_item(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, 2i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_non_game_item(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| false);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, 2i32).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_replaces_item1_by_item2_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_replace_item()
                    .with(predicate::eq(2))
                    .times(1);
                mock_game_entity
            });

        let result = Python::with_gil(|py| {
            pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, 2i32).as_any(),
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_replaces_item1_id_by_item2_clssname(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_item_from_ctx = MockGameItem::from_context();
        game_item_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_item = MockGameItem::new();
                mock_game_item
                    .expect_get_classname()
                    .returning(|| "weapon_bfg".into());
                mock_game_item
            });
        game_item_from_ctx.expect().returning(|_| {
            let mut mock_game_item = MockGameItem::new();
            mock_game_item
                .expect_get_classname()
                .returning(|| "available_classname".into());
            mock_game_item
        });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_replace_item()
                    .with(predicate::eq(2))
                    .times(1);
                mock_game_entity
            });

        let result = Python::with_gil(|py| {
            pyshinqlx_replace_items(
                py,
                PyInt::new(py, 1i32).as_any(),
                PyString::intern(py, "weapon_bfg").as_ref(),
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_replaces_item1_string_by_item2_clssname(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_item_from_ctx = MockGameItem::from_context();
        game_item_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_item = MockGameItem::new();
                mock_game_item
                    .expect_get_classname()
                    .returning(|| "weapon_bfg".into());
                mock_game_item
            });
        game_item_from_ctx.expect().returning(|_| {
            let mut mock_game_item = MockGameItem::new();
            mock_game_item
                .expect_get_classname()
                .returning(|| "available_classname".into());
            mock_game_item
        });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "weapon_railgun".into());
                mock_game_entity.expect_replace_item().times(0);
                mock_game_entity
            });
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
                    .expect_get_classname()
                    .returning(|| "weapon_railgun".into());
                mock_game_entity.expect_replace_item().times(0);
                mock_game_entity
            });
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(3))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "weapon_shotgun".into());
                mock_game_entity.expect_replace_item().times(0);
                mock_game_entity
            });
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "weapon_railgun".into());
                mock_game_entity
                    .expect_replace_item()
                    .with(predicate::eq(2))
                    .times(1);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| true);
            mock_game_entity
                .expect_get_classname()
                .returning(|| "other_classname".into());
            mock_game_entity.expect_replace_item().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            pyshinqlx_replace_items(
                py,
                PyString::intern(py, "weapon_railgun").as_ref(),
                PyString::intern(py, "weapon_bfg").as_ref(),
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_replaces_item1_string_items_by_item2_clssname(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        let game_item_from_ctx = MockGameItem::from_context();
        game_item_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_item = MockGameItem::new();
                mock_game_item
                    .expect_get_classname()
                    .returning(|| "weapon_bfg".into());
                mock_game_item
            });
        game_item_from_ctx.expect().returning(|_| {
            let mut mock_game_item = MockGameItem::new();
            mock_game_item
                .expect_get_classname()
                .returning(|| "available_classname".into());
            mock_game_item
        });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "weapon_railgun".into());
                mock_game_entity
                    .expect_replace_item()
                    .with(predicate::eq(2))
                    .times(1);
                mock_game_entity
            });
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_is_game_item()
                    .with(predicate::eq(entityType_t::ET_ITEM))
                    .returning(|_| true);
                mock_game_entity
                    .expect_get_classname()
                    .returning(|| "weapon_railgun".into());
                mock_game_entity
                    .expect_replace_item()
                    .with(predicate::eq(2))
                    .times(1);
                mock_game_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_in_use().returning(|| true);
            mock_game_entity
                .expect_is_game_item()
                .with(predicate::eq(entityType_t::ET_ITEM))
                .returning(|_| true);
            mock_game_entity
                .expect_get_classname()
                .returning(|| "other_classname".into());
            mock_game_entity.expect_replace_item().times(0);
            mock_game_entity
        });

        let result = Python::with_gil(|py| {
            pyshinqlx_replace_items(
                py,
                PyString::intern(py, "weapon_railgun").as_ref(),
                PyString::intern(py, "weapon_bfg").as_ref(),
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }
}
