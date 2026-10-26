//! Entry points for the criterion benchmarks. Not part of the stable API of this crate.

use pyo3::{
    append_to_inittab,
    ffi::Py_IsInitialized,
    intern,
    prelude::*,
    types::{PyDict, PyString, PyTuple},
};

use super::{
    EVENT_DISPATCHERS, clean_text as clean_text_impl,
    events::{
        ClientCommandDispatcher, DamageDispatcher, DeathDispatcher, EventDispatcherManager,
        EventDispatcherManagerMethods, FrameEventDispatcher, KillDispatcher,
        ServerCommandDispatcher, SetConfigstringDispatcher, StatsDispatcher,
        dispatcher_debug_log as dispatcher_debug_log_impl,
    },
    handlers::{
        handle_client_command as handle_client_command_impl, handle_damage as handle_damage_impl,
        handle_frame as handle_frame_impl, handle_server_command as handle_server_command_impl,
        handle_set_configstring as handle_set_configstring_impl,
    },
    pyshinqlx_module,
    stats_listener::handle_zmq_msg,
};

/// Starts the embedded interpreter with the shinqlx module and some event dispatchers.
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
        dispatchers
            .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
            .expect("could not add set configstring dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
            .expect("could not add client command dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
            .expect("could not add server command dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<DamageDispatcher>())
            .expect("could not add damage dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<StatsDispatcher>())
            .expect("could not add stats dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<DeathDispatcher>())
            .expect("could not add death dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<KillDispatcher>())
            .expect("could not add kill dispatcher");
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

/// Runs the handler that is called by the server whenever a configstring is set.
pub fn handle_set_configstring(index: u32, value: &str) {
    Python::attach(|py| {
        handle_set_configstring_impl(py, index, value);
    });
}

/// Writes a debug message to the log the way the event dispatchers do.
pub fn dispatcher_debug_log(message: &str) {
    Python::attach(|py| dispatcher_debug_log_impl(py, message));
}

/// Formats the arguments of a client command event the way the event dispatchers do for logging.
pub fn format_client_command_arguments() -> String {
    Python::attach(|py| {
        let player = Bound::new(py, super::player::Player::from_client_id(3, None))
            .expect("could not create player object");
        let args = PyTuple::new(
            py,
            [player.into_any(), PyString::new(py, "scores").into_any()],
        )
        .expect("could not create arguments");
        format!("client_command{args}")
    })
}

/// Handles a message of the ZMQ stats protocol the way the stats listener does.
pub fn handle_stats_message(message: &str) {
    Python::attach(|py| {
        handle_zmq_msg(py, message);
    });
}

/// Runs the handler that is called by the server for every command of a client.
pub fn handle_client_command(client_id: i32, command: &str) {
    Python::attach(|py| {
        handle_client_command_impl(py, client_id, command);
    });
}

/// Runs the handler that is called by the server for every command sent to a client.
pub fn handle_server_command(client_id: i32, command: &str) {
    Python::attach(|py| {
        handle_server_command_impl(py, client_id, command);
    });
}

/// Runs the handler that is called by the server whenever damage is dealt.
pub fn handle_damage(target_id: i32, attacker_id: Option<i32>, damage: i32) {
    Python::attach(|py| {
        handle_damage_impl(py, target_id, attacker_id, damage, 0, 5);
    });
}

/// Creates a `shinqlx.Powerups` object from six integers the way a plugin does.
pub fn create_powerups() {
    Python::attach(|py| {
        py.import(intern!(py, "shinqlx"))
            .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "Powerups")))
            .and_then(|powerups_class| powerups_class.call1(((0, 0, 0, 0, 0, 0),)))
            .expect("could not create powerups");
    });
}

/// Creates a `shinqlx.Weapons` object from fifteen booleans the way a plugin does.
pub fn create_weapons() {
    Python::attach(|py| {
        py.import(intern!(py, "shinqlx"))
            .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "Weapons")))
            .and_then(|weapons_class| {
                let values = PyTuple::new(py, [false; 15])?;
                weapons_class.call1((values,))
            })
            .expect("could not create weapons");
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
