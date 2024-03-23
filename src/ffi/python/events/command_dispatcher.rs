use super::prelude::*;

/// Event that goes off when a command is executed. This can be used
/// to for instance keep a log of all the commands admins have used.
#[pyclass(module = "_events", name = "CommandDispatcher", extends = EventDispatcher, frozen)]
pub(crate) struct CommandDispatcher {}

#[pymethods]
impl CommandDispatcher {
    #[classattr]
    #[allow(non_upper_case_globals)]
    const name: &'static str = "command";

    #[new]
    fn py_new(_py: Python<'_>) -> (Self, EventDispatcher) {
        let super_class = EventDispatcher {
            name: Self::name.into(),
            ..EventDispatcher::default()
        };
        (Self {}, super_class)
    }

    fn dispatch(
        slf: PyRef<'_, Self>,
        py: Python<'_>,
        caller: PyObject,
        command: PyObject,
        args: PyObject,
    ) {
        let super_class = slf.into_super();
        for i in 0..5 {
            for (_, handlers) in &super_class.plugins {
                for handler in &handlers[i] {
                    match handler.call1(py, (&caller, &command, &args)) {
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
