use crate::ffi::python::{prelude::*, set_cvar};

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar")]
#[pyo3(signature = (cvar, value, flags=None), text_signature = "(cvar, value, flags=None)")]
pub(crate) fn pyshinqlx_set_cvar(
    py: Python<'_>,
    cvar: &str,
    value: &str,
    flags: Option<i32>,
) -> PyResult<bool> {
    py.detach(|| set_cvar(cvar, value, flags))
}

#[cfg(test)]
mod set_cvar_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
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
                let result = Python::attach(|py| {
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
                let result = Python::attach(|py| {
                    pyshinqlx_set_cvar(py, "sv_maxclients", "64", Some(cvar_flags::CVAR_ROM as i32))
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }
}
