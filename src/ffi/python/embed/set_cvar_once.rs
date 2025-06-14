use core::hint::cold_path;

use pyo3::exceptions::PyEnvironmentError;

use crate::{
    MAIN_ENGINE,
    ffi::python::prelude::*,
    quake_live_engine::{FindCVar, GetCVar},
};

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar_once")]
#[pyo3(signature = (cvar, value, flags=0), text_signature = "(cvar, value, flags=0)")]
pub(crate) fn pyshinqlx_set_cvar_once(
    py: Python<'_>,
    cvar: &str,
    value: &Bound<'_, PyAny>,
    flags: i32,
) -> PyResult<bool> {
    let value_string = value.to_string();
    py.allow_threads(|| {
        MAIN_ENGINE.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ))
            },
            |main_engine| {
                if main_engine.find_cvar(cvar).is_some() {
                    return Ok(false);
                }

                main_engine.get_cvar(cvar, &value_string, Some(flags));
                Ok(true)
            },
        )
    })
}

#[cfg(test)]
mod set_cvar_once_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{
        exceptions::PyEnvironmentError,
        types::{PyInt, PyString},
    };
    use rstest::*;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar_once(
                py,
                "sv_maxclients",
                PyString::intern(py, "64").as_any(),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_not_existing_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("sv_maxclients"),
                        predicate::eq("64"),
                        predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    pyshinqlx_set_cvar_once(
                        py,
                        "sv_maxclients",
                        PyInt::new(py, 64i32).as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                })
                .unwrap();
                assert_eq!(result, true);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default().build().unwrap();
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine.expect_get_cvar().times(0);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    pyshinqlx_set_cvar_once(
                        py,
                        "sv_maxclients",
                        PyString::intern(py, "64").as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                })
                .unwrap();
                assert_eq!(result, false);
            });
    }
}
