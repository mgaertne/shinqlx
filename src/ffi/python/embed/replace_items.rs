use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use arrayvec::ArrayVec;

use pyo3::exceptions::PyValueError;

fn determine_item_id(item: &Bound<PyAny>) -> PyResult<i32> {
    if let Ok(item_id) = item.extract::<i32>() {
        if !(0..GameItem::get_num_items()).contains(&item_id) {
            return Err(PyValueError::new_err(format!(
                "item2 needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }
        return Ok(item_id);
    }

    let Ok(item_classname) = item.extract::<String>() else {
        return Err(PyValueError::new_err(
            "item2 needs to be of type int or string.",
        ));
    };

    (1..GameItem::get_num_items())
        .filter(|&i| {
            #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
            GameItem::try_from(i).is_ok_and(|game_item| game_item.get_classname() == item_classname)
        })
        .take(1)
        .next()
        .ok_or(PyValueError::new_err(format!(
            "invalid item classname: {}",
            item_classname
        )))
}

/// Replaces target entity's item with specified one.
#[pyfunction]
#[pyo3(name = "replace_items")]
#[pyo3(signature = (item1, item2))]
pub(crate) fn pyshinqlx_replace_items(
    py: Python<'_>,
    item1: &Bound<'_, PyAny>,
    item2: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let item2_id = determine_item_id(item2)?;
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed

    if let Ok(item1_id) = item1.extract::<i32>() {
        // replacing item by entity_id

        // entity_id checking
        if !(0..GameItem::get_num_items()).contains(&item1_id) {
            return Err(PyValueError::new_err(format!(
                "item1 needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }

        return py.allow_threads(|| {
            #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
            let mut opt_game_entity = GameEntity::try_from(item1_id).ok().filter(|game_entity| {
                game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
            });

            if opt_game_entity.is_none() {
                return Err(PyValueError::new_err(format!(
                    "entity #{} is not a valid game item",
                    item1_id
                )));
            }

            opt_game_entity
                .iter_mut()
                .for_each(|game_entity| game_entity.replace_item(item2_id));

            Ok(true)
        });
    }

    if let Ok(item1_classname) = item1.extract::<String>() {
        return py.allow_threads(|| {
            #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
            let mut matching_item1_entities: ArrayVec<
                Box<GameEntity>,
                { MAX_GENTITIES as usize },
            > = (0..MAX_GENTITIES)
                .filter_map(|i| GameEntity::try_from(i as i32).ok().map(Box::new))
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
        });
    }

    Err(PyValueError::new_err(
        "item1 needs to be of type int or string.",
    ))
}

#[cfg(test)]
mod replace_items_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use pyo3::IntoPyObjectExt;
    use pyo3::exceptions::PyValueError;
    use pyo3::types::{PyString, PyTuple};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn replace_items_for_too_small_item1_id(_pyshinqlx_setup: ()) {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = pyshinqlx_replace_items(
                py,
                (-1i32)
                    .into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                43i32
                    .into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                (-1i32)
                    .into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                43i32
                    .into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                PyString::new(py, "not existing classname").as_ref(),
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                PyString::new(py, "not existing classname").as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                2i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .into_any()
                    .as_ref(),
                2i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                2i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
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
                1i32.into_bound_py_any(py)
                    .expect("this should not happen")
                    .as_ref(),
                PyString::new(py, "weapon_bfg").as_ref(),
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
                PyString::new(py, "weapon_railgun").as_ref(),
                PyString::new(py, "weapon_bfg").as_ref(),
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
                PyString::new(py, "weapon_railgun").as_ref(),
                PyString::new(py, "weapon_bfg").as_ref(),
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }
}
