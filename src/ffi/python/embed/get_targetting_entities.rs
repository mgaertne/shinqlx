use core::hint::cold_path;

use pyo3::exceptions::PyValueError;
use tap::TryConv;

use crate::ffi::{c::prelude::*, python::prelude::*};

/// get a list of entities that target a given entity
#[pyfunction]
#[pyo3(name = "get_targetting_entities")]
pub(crate) fn pyshinqlx_get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
    py.allow_threads(|| {
        if !(0..MAX_GENTITIES as i32).contains(&entity_id) {
            cold_path();
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
            )));
        }

        entity_id.try_conv::<GameEntity>().map_or_else(
            |_| Ok(vec![]),
            |entity| Ok(entity.get_targetting_entity_ids()),
        )
    })
}

#[cfg(test)]
mod get_entity_targets_tests {
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
    fn get_entity_targets_for_too_small_entity_id(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_entity_targets(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_entity_targets_for_too_large_entity_id(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_entity_targets(py, MAX_GENTITIES as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_entity_targets_for_valid_entity_id_with_no_targetting_entities(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_targetting_entity_ids(Vec::new, 1..)
            .run(predicate::eq(2), || {
                let result = Python::with_gil(|py| pyshinqlx_get_entity_targets(py, 2));
                assert_eq!(result.expect("result was not OK"), Vec::<u32>::new());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_entity_targets_for_valid_entity_id_with_targetting_entities(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_targetting_entity_ids(|| vec![42, 21, 1337], 1..)
            .run(predicate::eq(2), || {
                let result = Python::with_gil(|py| pyshinqlx_get_entity_targets(py, 2));
                assert_eq!(result.expect("result was not OK"), vec![42, 21, 1337]);
            });
    }
}
