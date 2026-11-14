use crate::{
    ffi::python::{prelude::*, with_main_engine},
    quake_live_engine::{FindCVar, GetCVar},
};

/// Sets a cvar.
#[pyfunction]
#[pyo3(name = "set_cvar_once")]
#[pyo3(signature = (name, value, flags=0), text_signature = "(name, value, flags=0)")]
pub(crate) fn pyshinqlx_set_cvar_once(
    py: Python<'_>,
    name: &str,
    value: &Bound<'_, PyAny>,
    flags: i32,
) -> PyResult<bool> {
    let value_string = value.to_string();
    py.detach(|| {
        with_main_engine(|main_engine| {
            if main_engine.find_cvar(name).is_some() {
                return Ok(false);
            }

            main_engine.get_cvar(name, &value_string, Some(flags));
            Ok(true)
        })
    })
}

#[cfg(test)]
mod set_cvar_once_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{
        exceptions::PyEnvironmentError,
        types::{PyDict, PyInt, PyString},
        wrap_pyfunction,
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
        Python::attach(|py| {
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
                let result = Python::attach(|py| {
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
    fn set_cvar_once_takes_the_parameter_names_of_minqlx(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("sv_maxclients"),
                        predicate::eq("64"),
                        predicate::eq(Some(0)),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::attach(|py| -> PyResult<bool> {
                    let function = wrap_pyfunction!(pyshinqlx_set_cvar_once, py)?;
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("name", "sv_maxclients")?;
                    kwargs.set_item("value", "64")?;
                    function.call((), Some(&kwargs))?.extract()
                });
                assert!(result.is_ok_and(|value| value));
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
                let result = Python::attach(|py| {
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
