use crate::prelude::*;
use pyo3::exceptions::PyValueError;
use pyo3::{pyfunction, PyResult, Python};

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
#[pyo3(signature = (item_id, x, y, z))]
pub(crate) fn pyshinqlx_spawn_item(
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

    py.allow_threads(|| {
        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        GameItem::try_from(item_id)
            .iter_mut()
            .for_each(|gitem| gitem.spawn((x, y, z)));
    });

    Ok(true)
}

#[cfg(test)]
#[cfg(not(miri))]
mod spawn_item_tests {
    use super::pyshinqlx_spawn_item;
    use crate::ffi::c::game_item::MockGameItem;
    use crate::prelude::*;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn spawn_item_for_too_small_item_id() {
        let get_num_item_ctx = MockGameItem::get_num_items_context();
        get_num_item_ctx.expect().returning(|| 1);

        Python::with_gil(|py| {
            let result = pyshinqlx_spawn_item(py, -1, 0, 0, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn spawn_item_for_too_large_item_id() {
        let get_num_item_ctx = MockGameItem::get_num_items_context();
        get_num_item_ctx.expect().returning(|| 64);

        Python::with_gil(|py| {
            let result = pyshinqlx_spawn_item(py, 64, 0, 0, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[serial]
    fn spawn_item_spawns_item() {
        let get_num_item_ctx = MockGameItem::get_num_items_context();
        get_num_item_ctx.expect().returning(|| 64);

        let item_from_ctx = MockGameItem::from_context();
        item_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_| {
                let mut mock_item = MockGameItem::new();
                mock_item
                    .expect_spawn()
                    .with(predicate::eq((1, 2, 3)))
                    .times(1);
                mock_item
            });

        let result = Python::with_gil(|py| pyshinqlx_spawn_item(py, 42, 1, 2, 3));

        assert_eq!(result.expect("result was not OK"), true);
    }
}
