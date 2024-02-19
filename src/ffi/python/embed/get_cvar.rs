use crate::ffi::python::prelude::*;
use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
pub(crate) fn pyshinqlx_get_cvar(py: Python<'_>, cvar: &str) -> PyResult<Option<String>> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        match main_engine.find_cvar(cvar) {
            None => Ok(None),
            Some(cvar_result) => Ok(Some(cvar_result.get_string())),
        }
    })
}

#[cfg(test)]
mod get_cvar_tests {
    use super::MAIN_ENGINE;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use alloc::ffi::CString;
    use core::ffi::c_char;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = pyshinqlx_get_cvar(py, "sv_maxclients");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_not_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("asdf"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| pyshinqlx_get_cvar(py, "asdf"));
        assert_eq!(result.expect("result was not OK"), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_is_found() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| pyshinqlx_get_cvar(py, "sv_maxclients"));
        assert_eq!(result.expect("result was not OK"), Some("16".into()));
    }
}
