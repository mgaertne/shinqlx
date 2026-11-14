use crate::{
    ffi::python::{prelude::*, with_main_engine},
    quake_live_engine::{FindCVar, SetCVarLimit},
};

/// Sets a non-string cvar with a minimum and maximum value.
#[pyfunction]
#[pyo3(name = "set_cvar_limit_once")]
#[pyo3(
    signature = (name, value, minimum, maximum, flags=0),
    text_signature = "(name, value, minimum, maximum, flags=0)"
)]
pub(crate) fn pyshinqlx_set_cvar_limit_once(
    py: Python<'_>,
    name: &str,
    value: &str,
    minimum: &str,
    maximum: &str,
    flags: i32,
) -> PyResult<bool> {
    py.detach(|| {
        with_main_engine(|main_engine| {
            if main_engine.find_cvar(name).is_some() {
                return Ok(false);
            }
            main_engine.set_cvar_limit(name, value, minimum, maximum, Some(flags));

            Ok(true)
        })
    })
}

#[cfg(test)]
mod set_cvar_limit_once_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{exceptions::PyEnvironmentError, types::PyDict, wrap_pyfunction};
    use rstest::*;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
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
                let result = Python::attach(|py| {
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
    fn set_cvar_limit_once_takes_the_parameter_names_of_minqlx(_pyshinqlx_setup: ()) {
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
                        predicate::eq(Some(0)),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::attach(|py| -> PyResult<bool> {
                    let function = wrap_pyfunction!(pyshinqlx_set_cvar_limit_once, py)?;
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("name", "sv_maxclients")?;
                    kwargs.set_item("value", "64")?;
                    kwargs.set_item("minimum", "1")?;
                    kwargs.set_item("maximum", "64")?;
                    function.call((), Some(&kwargs))?.extract()
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
                let result = Python::attach(|py| {
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
