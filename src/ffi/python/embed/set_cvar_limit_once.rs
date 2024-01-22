use crate::quake_live_engine::{FindCVar, SetCVarLimit};
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::{pyfunction, PyResult, Python};

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit_once")]
#[pyo3(signature = (cvar, value, min, max, flags=0))]
pub(crate) fn pyshinqlx_set_cvar_limit_once(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    min: &str,
    max: &str,
    flags: i32,
) -> PyResult<bool> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if main_engine.find_cvar(cvar).is_some() {
            return Ok(false);
        }
        main_engine.set_cvar_limit(cvar, value, min, max, Some(flags));

        Ok(true)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod set_cvar_limit_once_tests {
    use super::pyshinqlx_set_cvar_limit_once;
    use super::MAIN_ENGINE;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar_limit_once(py, "sv_maxclients", "64", "1", "64", 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[serial]
    fn set_cvar_limit_once_when_no_previous_value_is_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None);
        mock_engine
            .expect_set_cvar_limit()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq("1"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            pyshinqlx_set_cvar_limit_once(
                py,
                "sv_maxclients",
                "64",
                "1",
                "64",
                cvar_flags::CVAR_CHEAT as i32,
            )
        });
        assert!(result.is_ok_and(|value| value));
    }

    #[test]
    #[serial]
    fn set_cvar_limit_once_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine.expect_set_cvar_limit().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            pyshinqlx_set_cvar_limit_once(
                py,
                "sv_maxclients",
                "64",
                "1",
                "64",
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, false);
    }
}
