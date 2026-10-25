//! Entry points for the criterion benchmarks. Not part of the stable API of this crate.

use pyo3::{
    append_to_inittab,
    ffi::Py_IsInitialized,
    intern,
    prelude::*,
    types::{PyDict, PyTuple},
};

use super::{
    EVENT_DISPATCHERS, clean_text as clean_text_impl,
    events::{EventDispatcherManager, EventDispatcherManagerMethods, FrameEventDispatcher},
    handlers::handle_frame as handle_frame_impl,
    pyshinqlx_module,
};

/// Starts the embedded interpreter with the shinqlx module and a frame event dispatcher.
pub fn prepare_python() {
    if unsafe { Py_IsInitialized() } == 0 {
        append_to_inittab!(pyshinqlx_module);
        Python::initialize();
    }

    Python::attach(|py| {
        let dispatchers = Bound::new(py, EventDispatcherManager::default())
            .expect("could not create event dispatcher manager");
        dispatchers
            .add_dispatcher(&py.get_type::<FrameEventDispatcher>())
            .expect("could not add frame dispatcher");
        EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));

        py.import(intern!(py, "shinqlx"))
            .expect("could not import shinqlx module");
    });
}

/// Removes the color tags from a text.
pub fn clean_text(text: &str) -> String {
    clean_text_impl(&text)
}

/// Runs the handler that is called by the server every frame.
pub fn handle_frame() {
    Python::attach(|py| {
        handle_frame_impl(py);
    });
}

/// Queues `count` tasks for the next frame the same way `shinqlx.next_frame` does.
pub fn queue_next_frame_tasks(count: usize) {
    Python::attach(|py| {
        let next_frame_tasks = py
            .import(intern!(py, "shinqlx"))
            .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "next_frame_tasks")))
            .expect("could not get next frame tasks");
        let no_op = py
            .eval(c"lambda: None", None, None)
            .expect("could not create no-op task");
        for _ in 0..count {
            next_frame_tasks
                .call_method1(
                    intern!(py, "put_nowait"),
                    ((&no_op, PyTuple::empty(py), PyDict::new(py)),),
                )
                .expect("could not queue task");
        }
    });
}
