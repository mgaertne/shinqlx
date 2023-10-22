use crate::ffi::c::GameItem;
use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, PyResult, Python};

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
#[pyo3(signature = (item_id, x, y, z))]
pub(crate) fn minqlx_spawn_item(
    py: Python<'_>,
    item_id: i32,
    x: i32,
    y: i32,
    z: i32,
) -> PyResult<bool> {
    let max_items: i32 = GameItem::get_num_items();
    if !(1..max_items).contains(&item_id) {
        return Err(PyValueError::new_err(format!(
            "item_id needs to be a number from 1 to {}.",
            max_items - 1
        )));
    }

    py.allow_threads(move || {
        let mut gitem = GameItem::try_from(item_id).unwrap();
        gitem.spawn((x, y, z));
    });

    Ok(true)
}
