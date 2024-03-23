use super::prelude::*;

/// Event that triggers whenever a plugin is unloaded. Cannot be cancelled.
#[pyclass(module = "_events", name = "UnloadDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct UnloadDispatcher {}

#[pymethods]
impl UnloadDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "unload";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(slf: PyRef<'_, Self>, py: Python<'_>, plugin: PyObject) {
        let super_class = slf.into_super();
        if let Ok(plugin_str) = plugin.call_method0(py, intern!(py, "__repr__")) {
            let dbgstr = format!("{}({})", super_class.name, plugin_str);
            dispatcher_debug_log(py, dbgstr);
        }

        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&plugin,)) {
                        Err(e) => {
                            log_exception(py, &e);
                            continue;
                        }
                        Ok(res) => {
                            let res_i32 = res.extract::<PythonReturnCodes>(py);
                            if res_i32.as_ref().is_ok_and(|value| {
                                [PythonReturnCodes::RET_STOP, PythonReturnCodes::RET_STOP_ALL]
                                    .contains(value)
                            }) {
                                return;
                            }
                            if !res_i32.as_ref().is_ok_and(|value| {
                                [
                                    PythonReturnCodes::RET_NONE,
                                    PythonReturnCodes::RET_STOP_EVENT,
                                ]
                                .contains(value)
                            }) {
                                log_unexpected_return_value(py, Self::name, &res, handler);
                            }
                        }
                    }
                }
            }
        }
    }
}
