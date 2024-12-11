use crate::MAIN_ENGINE;
use crate::ffi::python::prelude::*;
use crate::quake_live_engine::{FindCVar, GetCVar, SetCVarForced};

use pyo3::exceptions::PyEnvironmentError;

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None))]
pub(crate) fn pyshinqlx_set_cvar(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    flags: Option<i32>,
) -> PyResult<bool> {
    py.allow_threads(|| {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| match main_engine.find_cvar(cvar) {
                None => {
                    main_engine.get_cvar(cvar, value, flags);
                    Ok(true)
                }
                Some(_) => {
                    main_engine.set_cvar_forced(
                        cvar,
                        value,
                        flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
                    );
                    Ok(false)
                }
            },
        )
    })
}

#[cfg(test)]
mod set_cvar_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use pyo3::exceptions::PyEnvironmentError;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_set_cvar(py, "sv_maxclients", "64", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_not_existing_cvar(_pyshinqlx_setup: ()) {
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
                    pyshinqlx_set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
                });
                assert_eq!(result.expect("result was not OK"), true);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_cvar_forced()
                    .with(
                        predicate::eq("sv_maxclients"),
                        predicate::eq("64"),
                        predicate::eq(false),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    pyshinqlx_set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }
}
