use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyTuple},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Deferral {
    NextFrame,
    Delay(f64),
    Thread,
}

/// A function that is not run when it is called, but deferred as configured. It is a descriptor,
/// so it binds `self` when it decorates a method, and it looks like the function it wraps.
#[pyclass(module = "shinqlx", name = "DeferredCall", frozen, dict)]
pub(super) struct DeferredCall {
    func: Py<PyAny>,
    deferral: Deferral,
}

impl DeferredCall {
    pub(super) fn wrap<'py>(
        func: &Bound<'py, PyAny>,
        deferral: Deferral,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = func.py();
        let deferred = Bound::new(
            py,
            Self {
                func: func.clone().unbind(),
                deferral,
            },
        )?
        .into_any();
        py.import(intern!(py, "functools"))?
            .call_method1(intern!(py, "update_wrapper"), (&deferred, func))?;
        Ok(deferred)
    }

    fn start_thread<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        kwargs: &Bound<'py, PyDict>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        let shinqlx = py.import(intern!(py, "shinqlx"))?;
        let threading = py.import(intern!(py, "threading"))?;
        let thread_name = shinqlx
            .getattr(intern!(py, "_thread_name"))?
            .extract::<String>()?;
        let current_thread_name = threading
            .call_method0(intern!(py, "current_thread"))?
            .getattr(intern!(py, "name"))?
            .extract::<String>()?;
        if current_thread_name.ends_with(&thread_name) {
            self.func.bind(py).call(args, Some(kwargs))?;
            return Ok(None);
        }

        let thread_count = shinqlx
            .getattr(intern!(py, "_thread_count"))?
            .extract::<usize>()?;
        let func_name = self
            .func
            .bind(py)
            .getattr(intern!(py, "__name__"))
            .map_or_else(|_| "thread".to_string(), |name| name.to_string());
        let thread_options = PyDict::new(py);
        thread_options.set_item(intern!(py, "target"), self.func.bind(py))?;
        thread_options.set_item(
            intern!(py, "name"),
            format!("{func_name}-{thread_count}-{thread_name}"),
        )?;
        thread_options.set_item(intern!(py, "args"), args)?;
        thread_options.set_item(intern!(py, "kwargs"), kwargs)?;
        thread_options.set_item(intern!(py, "daemon"), true)?;
        let thread = threading
            .getattr(intern!(py, "Thread"))?
            .call((), Some(&thread_options))?;
        thread.call_method0(intern!(py, "start"))?;
        shinqlx.setattr(intern!(py, "_thread_count"), thread_count + 1)?;
        Ok(Some(thread))
    }
}

#[pymethods]
impl DeferredCall {
    #[pyo3(signature = (*args, **kwargs))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        let kwargs = kwargs.map_or_else(|| PyDict::new(py), |kwargs| kwargs.clone());
        let func = self.func.bind(py);

        match self.deferral {
            Deferral::NextFrame => {
                py.import(intern!(py, "shinqlx"))?
                    .getattr(intern!(py, "next_frame_tasks"))?
                    .call_method1(intern!(py, "put_nowait"), ((func, args, kwargs),))?;
                Ok(None)
            }
            Deferral::Delay(time) => {
                py.import(intern!(py, "shinqlx"))?
                    .getattr(intern!(py, "frame_tasks"))?
                    .call_method1(intern!(py, "enter"), (time, 1, func, args, kwargs))?;
                Ok(None)
            }
            Deferral::Thread => self.start_thread(py, args, &kwargs),
        }
    }

    fn __get__<'py>(
        slf: &Bound<'py, Self>,
        instance: Option<&Bound<'py, PyAny>>,
        _owner: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        match instance {
            None => Ok(slf.clone().into_any()),
            Some(instance) => py
                .import(intern!(py, "types"))?
                .getattr(intern!(py, "MethodType"))?
                .call1((slf, instance)),
        }
    }
}

#[cfg(test)]
mod deferred_call_tests {
    use std::ffi::CString;

    use pyo3::{exceptions::PyTypeError, intern, prelude::*, types::IntoPyDict};
    use rstest::rstest;

    use crate::{
        ffi::python::{
            pyshinqlx_setup_fixture::pyshinqlx_setup, pyshinqlx_test_support::run_all_frame_tasks,
        },
        prelude::*,
    };

    fn plugin_module<'py>(py: Python<'py>, decorator: &str) -> Bound<'py, PyModule> {
        let code = CString::new(format!(
            r#"
import shinqlx


class SamplePlugin:
    def __init__(self):
        self.calls = []

    {decorator}
    def remember(self, value, factor=1):
        """Remembers the value."""
        self.calls.append(value * factor)


plugin = SamplePlugin()
"#
        ))
        .expect("this should not happen");
        PyModule::from_code(py, &code, c"", c"").expect("could not create the sample plugin")
    }

    fn calls(module: &Bound<'_, PyModule>) -> Vec<i32> {
        module
            .getattr(intern!(module.py(), "plugin"))
            .and_then(|plugin| plugin.getattr(intern!(module.py(), "calls")))
            .and_then(|calls| calls.extract())
            .expect("could not read the calls of the plugin")
    }

    fn remember<'py>(module: &Bound<'py, PyModule>, value: i32) -> PyResult<Bound<'py, PyAny>> {
        module
            .getattr(intern!(module.py(), "plugin"))?
            .call_method1(intern!(module.py(), "remember"), (value,))
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_runs_a_plugin_method_with_its_instance_in_the_next_frame(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.next_frame");

            assert!(remember(&module, 3).is_ok_and(|result| result.is_none()));
            assert!(calls(&module).is_empty());

            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(calls(&module), vec![3]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_runs_a_plugin_method_with_its_instance_after_the_delay(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.delay(0.0001)");

            assert!(remember(&module, 4).is_ok_and(|result| result.is_none()));
            assert!(calls(&module).is_empty());

            let frame_tasks = py
                .import(intern!(py, "shinqlx"))
                .and_then(|shinqlx| shinqlx.getattr(intern!(py, "frame_tasks")))
                .expect("could not get the frame tasks");
            assert!(frame_tasks.call_method0(intern!(py, "run")).is_ok());
            assert_eq!(calls(&module), vec![4]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_runs_a_plugin_method_with_its_instance_in_a_thread(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.thread");

            let thread = remember(&module, 5).expect("could not start the thread");
            assert!(thread.call_method0(intern!(py, "join")).is_ok());

            assert_eq!(calls(&module), vec![5]);
            assert!(
                thread
                    .getattr(intern!(py, "name"))
                    .is_ok_and(|name| name.to_string().starts_with("remember-"))
            );
            assert!(
                thread
                    .getattr(intern!(py, "daemon"))
                    .is_ok_and(|daemon| daemon.is_truthy().unwrap_or(false))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_threaded_plugin_method_called_from_a_thread_runs_inline(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                cr#"
import shinqlx


class SamplePlugin:
    def __init__(self):
        self.calls = []
        self.inner_result = "not run"

    @shinqlx.thread
    def inner(self, value):
        self.calls.append(value)

    @shinqlx.thread
    def outer(self, value):
        self.inner_result = self.inner(value)


plugin = SamplePlugin()
"#,
                c"",
                c"",
            )
            .expect("could not create the sample plugin");
            let plugin = module
                .getattr(intern!(py, "plugin"))
                .expect("could not get the plugin");

            let thread = plugin
                .call_method1(intern!(py, "outer"), (8,))
                .expect("could not start the thread");
            assert!(thread.call_method0(intern!(py, "join")).is_ok());

            assert_eq!(calls(&module), vec![8]);
            assert!(
                plugin
                    .getattr(intern!(py, "inner_result"))
                    .is_ok_and(|result| result.is_none())
            );
        });
    }

    #[rstest]
    #[case("@shinqlx.next_frame")]
    #[case("@shinqlx.delay(0.0001)")]
    #[case("@shinqlx.thread")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_deferred_method_looks_like_the_method_it_wraps(
        #[case] decorator: &str,
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let module = plugin_module(py, decorator);
            let remember = module
                .getattr(intern!(py, "SamplePlugin"))
                .and_then(|class| class.getattr(intern!(py, "remember")))
                .expect("could not get the method");

            let attribute = |name: &str| {
                remember
                    .getattr(name)
                    .expect("missing attribute")
                    .to_string()
            };
            assert_eq!(attribute("__name__"), "remember");
            assert_eq!(attribute("__doc__"), "Remembers the value.");
            assert!(attribute("__qualname__").ends_with("SamplePlugin.remember"));
            assert!(
                remember
                    .hasattr(intern!(py, "__wrapped__"))
                    .unwrap_or(false)
            );

            let signature = py
                .import(intern!(py, "inspect"))
                .and_then(|inspect| inspect.call_method1(intern!(py, "signature"), (&remember,)))
                .expect("could not get the signature");
            assert_eq!(signature.to_string(), "(self, value, factor=1)");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_deferred_method_binds_to_the_instance_it_is_accessed_on(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.next_frame");
            let plugin = module
                .getattr(intern!(py, "plugin"))
                .expect("could not get the plugin");

            let bound = plugin
                .getattr(intern!(py, "remember"))
                .expect("could not get the bound method");

            assert!(
                bound
                    .getattr(intern!(py, "__self__"))
                    .is_ok_and(|bound_to| bound_to.is(&plugin))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_deferred_method_can_be_called_through_the_class(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.next_frame");
            let plugin = module
                .getattr(intern!(py, "plugin"))
                .expect("could not get the plugin");

            let result = module
                .getattr(intern!(py, "SamplePlugin"))
                .and_then(|class| class.call_method1(intern!(py, "remember"), (&plugin, 7)));
            assert!(result.is_ok());

            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(calls(&module), vec![7]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn keyword_arguments_are_passed_on_to_the_deferred_method(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = plugin_module(py, "@shinqlx.next_frame");

            let result = module.getattr(intern!(py, "plugin")).and_then(|plugin| {
                plugin.call_method(
                    intern!(py, "remember"),
                    (2,),
                    Some(&[("factor", 5)].into_py_dict(py).expect("no dict")),
                )
            });
            assert!(result.is_ok());

            assert!(run_all_frame_tasks(py).is_ok());
            assert_eq!(calls(&module), vec![10]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_hook_on_a_deferred_method_checks_the_signature_of_the_wrapped_method(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let result = PyModule::from_code(
                py,
                cr#"
import shinqlx


class SamplePlugin:
    @shinqlx.hook("frame")
    @shinqlx.next_frame
    def on_frame(self, player):
        pass
"#,
                c"",
                c"",
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }
}
