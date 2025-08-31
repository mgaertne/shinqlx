use core::hint::cold_path;

use pyo3::exceptions::PyValueError;
use tap::{TapFallible, TryConv};

use crate::ffi::{c::prelude::*, python::prelude::*};

/// Spawns item with specified coordinates.
#[pyfunction]
#[pyo3(name = "spawn_item")]
pub(crate) fn pyshinqlx_spawn_item(
    py: Python<'_>,
    item_id: i32,
    x: i32,
    y: i32,
    z: i32,
) -> PyResult<bool> {
    py.detach(|| {
        let max_items: i32 = GameItem::get_num_items();
        if !(1..max_items).contains(&item_id) {
            cold_path();
            return Err(PyValueError::new_err(format!(
                "item_id needs to be a number from 1 to {}.",
                max_items - 1
            )));
        }

        let _ = item_id.try_conv::<GameItem>().tap_ok_mut(|gitem| {
            gitem.spawn((x, y, z));
        });

        Ok(true)
    })
}

#[cfg(test)]
mod spawn_item_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyValueError;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_item_for_too_small_item_id(_pyshinqlx_setup: ()) {
        let get_num_item_ctx = MockGameItem::get_num_items_context();
        get_num_item_ctx.expect().returning(|| 1);

        Python::attach(|py| {
            let result = pyshinqlx_spawn_item(py, -1, 0, 0, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_item_for_too_large_item_id(_pyshinqlx_setup: ()) {
        let get_num_item_ctx = MockGameItem::get_num_items_context();
        get_num_item_ctx.expect().returning(|| 64);

        Python::attach(|py| {
            let result = pyshinqlx_spawn_item(py, 64, 0, 0, 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_item_spawns_item(_pyshinqlx_setup: ()) {
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

        let result = Python::attach(|py| pyshinqlx_spawn_item(py, 42, 1, 2, 3));

        assert_eq!(result.expect("result was not OK"), true);
    }
}
