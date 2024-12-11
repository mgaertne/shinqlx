use crate::MAIN_ENGINE;
use crate::ffi::python::prelude::*;
use crate::quake_live_engine::FindCVar;

use pyo3::exceptions::PyEnvironmentError;

/// Gets a cvar.
#[pyfunction]
#[pyo3(name = "get_cvar")]
pub(crate) fn pyshinqlx_get_cvar(py: Python<'_>, cvar: &str) -> PyResult<Option<String>> {
    py.allow_threads(|| {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| {
                Ok(main_engine
                    .find_cvar(cvar)
                    .map(|cvar_result| cvar_result.get_string()))
            },
        )
    })
}

#[cfg(test)]
mod get_cvar_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::*;

    use pyo3::exceptions::PyEnvironmentError;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_get_cvar(py, "sv_maxclients");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_not_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "asdf", |_| None, 1)
            .run(|| {
                let result = Python::with_gil(|py| pyshinqlx_get_cvar(py, "asdf"))
                    .expect("result waa not OK");
                assert!(result.is_none());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_is_found(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                let result = Python::with_gil(|py| pyshinqlx_get_cvar(py, "sv_maxclients"))
                    .expect("result was not OK");
                assert!(result.as_ref().is_some_and(|cvar| cvar == "16"));
            });
    }
}
