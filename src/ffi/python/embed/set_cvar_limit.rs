use crate::ffi::python::prelude::*;
use crate::quake_live_engine::SetCVarLimit;
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit")]
#[pyo3(signature = (cvar, value, min, max, flags=None))]
pub(crate) fn pyshinqlx_set_cvar_limit(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    min: &str,
    max: &str,
    flags: Option<i32>,
) -> PyResult<()> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        main_engine.set_cvar_limit(cvar, value, min, max, flags);

        Ok(())
    })
}

#[cfg(test)]
mod set_cvar_limit_tests {
    use super::MAIN_ENGINE;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pyo3::exceptions::PyEnvironmentError;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar_limit(py, "sv_maxclients", "64", "1", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_forwards_parameters_to_main_engine_call() {
        let mut mock_engine = MockQuakeEngine::new();
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
            pyshinqlx_set_cvar_limit(
                py,
                "sv_maxclients",
                "64",
                "1",
                "64",
                Some(cvar_flags::CVAR_CHEAT as i32),
            )
        });
        assert!(result.is_ok());
    }
}
