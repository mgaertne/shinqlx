use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

use pyo3::exceptions::PyValueError;

/// get a list of entities that target a given entity
#[pyfunction]
#[pyo3(name = "get_targetting_entities")]
pub(crate) fn pyshinqlx_get_entity_targets(py: Python<'_>, entity_id: i32) -> PyResult<Vec<u32>> {
    py.allow_threads(|| {
        if !(0..MAX_GENTITIES as i32).contains(&entity_id) {
            return Err(PyValueError::new_err(format!(
                "entity_id need to be between 0 and {}.",
                MAX_GENTITIES - 1
            )));
        }

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        GameEntity::try_from(entity_id).map_or_else(
            |_| Ok(vec![]),
            |entity| Ok(entity.get_targetting_entity_ids()),
        )
    })
}

#[cfg(test)]
mod get_entity_targets_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyValueError;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_entity_targets_for_too_small_entity_id() {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_entity_targets(py, -1);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_entity_targets_for_too_large_entity_id() {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_entity_targets(py, MAX_GENTITIES as i32);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_entity_targets_for_valid_entity_id_with_no_targetting_entities() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_targetting_entity_ids()
                    .returning(Vec::new);
                mock_game_entity
            })
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_get_entity_targets(py, 2));
        assert_eq!(result.expect("result was not OK"), vec![]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_entity_targets_for_valid_entity_id_with_targetting_entities() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_targetting_entity_ids()
                    .returning(|| vec![42, 21, 1337]);
                mock_game_entity
            })
            .times(1);

        let result = Python::with_gil(|py| pyshinqlx_get_entity_targets(py, 2));
        assert_eq!(result.expect("result was not OK"), vec![42, 21, 1337]);
    }
}
