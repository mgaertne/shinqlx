use crate::prelude::*;

use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, Py, PyAny, PyResult, Python};

fn determine_item_id(item: &PyAny) -> PyResult<i32> {
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
            let game_item = GameItem::try_from(i);
            game_item.is_ok() && game_item.unwrap().get_classname() == item_classname
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
pub(crate) fn minqlx_replace_items(
    py: Python<'_>,
    item1: Py<PyAny>,
    item2: Py<PyAny>,
) -> PyResult<bool> {
    let item2_id = determine_item_id(item2.as_ref(py))?;
    // Note: if item_id == 0 and item_classname == NULL, then item will be removed

    if let Ok(item1_id) = item1.extract::<i32>(py) {
        // replacing item by entity_id

        // entity_id checking
        if !(0..GameItem::get_num_items()).contains(&item1_id) {
            return Err(PyValueError::new_err(format!(
                "item1 needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }

        return py.allow_threads(move || {
            match GameEntity::try_from(item1_id) {
                Err(_) => return Err(PyValueError::new_err("game entity does not exist")),
                Ok(game_entity) => {
                    if !game_entity.in_use() {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not in use.",
                            item1_id
                        )));
                    }
                    if !game_entity.is_game_item(entityType_t::ET_ITEM) {
                        return Err(PyValueError::new_err(format!(
                            "entity #{} is not a game item. Cannot replace it",
                            item1_id
                        )));
                    }
                    let mut mut_game_entity = game_entity;
                    mut_game_entity.replace_item(item2_id);
                }
            }
            Ok(true)
        });
    }

    if let Ok(item1_classname) = item1.extract::<String>(py) {
        let item_found = py.allow_threads(move || {
            let matching_item1_entities: Vec<GameEntity> = (0..MAX_GENTITIES)
                .filter_map(|i| GameEntity::try_from(i as i32).ok())
                .filter(|game_entity| {
                    game_entity.in_use()
                        && game_entity.is_game_item(entityType_t::ET_ITEM)
                        && game_entity.get_classname() == item1_classname
                })
                .collect();
            let item_found = !matching_item1_entities.is_empty();
            matching_item1_entities
                .into_iter()
                .for_each(|mut game_entity| game_entity.replace_item(item2_id));
            item_found
        });
        return Ok(item_found);
    }

    Err(PyValueError::new_err(
        "item1 needs to be of type int or string.",
    ))
}

#[cfg(test)]
#[cfg(not(miri))]
mod replace_items_tests {
    use super::minqlx_replace_items;
    use crate::ffi::c::game_item::MockGameItem;
    use crate::prelude::*;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn replace_items_for_too_small_item1_id() {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = minqlx_replace_items(
                py,
                <i32 as IntoPy<Py<PyAny>>>::into_py(-1, py),
                1.into_py(py),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn replace_items_for_too_large_item1_id() {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = minqlx_replace_items(py, 43.into_py(py), 1.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn replace_items_for_too_small_item2_id() {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = minqlx_replace_items(
                py,
                1.into_py(py),
                <i32 as IntoPy<Py<PyAny>>>::into_py(-1, py),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn replace_items_for_too_large_item2_id() {
        let get_num_items_ctx = MockGameItem::get_num_items_context();
        get_num_items_ctx.expect().returning(|| 42);

        Python::with_gil(|py| {
            let result = minqlx_replace_items(py, 1.into_py(py), 43.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }
}
