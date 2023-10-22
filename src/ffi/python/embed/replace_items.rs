use crate::ffi::c::GameItem;
use crate::prelude::*;

use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, Py, PyAny, PyResult, Python};

fn determine_item_id(item: &PyAny) -> PyResult<i32> {
    if let Ok(item_id) = item.extract::<i32>() {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            return Err(PyValueError::new_err(format!(
                "item_id needs to be between 0 and {}.",
                GameItem::get_num_items() - 1
            )));
        }
        return Ok(item_id);
    }

    let Ok(item_classname) = item.extract::<String>() else {
        return Err(PyValueError::new_err(
            "item needs to be type of int or string.",
        ));
    };

    (1..GameItem::get_num_items())
        .filter(|i| {
            let game_item = GameItem::try_from(*i);
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
        if item1_id < 0 || item1_id >= MAX_GENTITIES as i32 {
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
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
                            "entity #{} is not item. Cannot replace it",
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
        "entity needs to be type of int or string.",
    ))
}
