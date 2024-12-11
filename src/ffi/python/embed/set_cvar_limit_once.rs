use crate::MAIN_ENGINE;
use crate::ffi::python::prelude::*;
use crate::quake_live_engine::{FindCVar, SetCVarLimit};

use pyo3::exceptions::PyEnvironmentError;

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit_once")]
#[pyo3(signature = (cvar, value, min, max, flags=0), text_signature = "(cvar, value, min, max, flags=0)")]
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
mod set_cvar_limit_once_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar_limit_once(py, "sv_maxclients", "64", "1", "64", 0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_when_no_previous_value_is_set(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 0..)
            .configure(|mock_engine| {
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
            })
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default().build().unwrap();
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine.expect_set_cvar_limit().times(0);
            })
            .run(|| {
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
            });
    }
}
