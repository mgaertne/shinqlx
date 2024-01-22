use crate::quake_live_engine::{FindCVar, GetCVar};
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::prelude::*;

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar_once")]
#[pyo3(signature = (cvar, value, flags=0))]
pub(crate) fn pyshinqlx_set_cvar_once(
    py: Python<'_>,
    cvar: &str,
    value: Py<PyAny>,
    flags: i32,
) -> PyResult<bool> {
    let value_string = value.to_string();
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if main_engine.find_cvar(cvar).is_some() {
            return Ok(false);
        }

        main_engine.get_cvar(cvar, &value_string, Some(flags));
        Ok(true)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_once_tests {
    use super::pyshinqlx_set_cvar_once;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_once_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar_once(py, "sv_maxclients", "64".into_py(py), 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_once_for_not_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        mock_engine
            .expect_get_cvar()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            pyshinqlx_set_cvar_once(
                py,
                "sv_maxclients",
                64i32.into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[serial]
    fn set_cvar_once_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine.expect_get_cvar().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            pyshinqlx_set_cvar_once(
                py,
                "sv_maxclients",
                "64".into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, false);
    }
}
