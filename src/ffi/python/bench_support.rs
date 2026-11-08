//! Entry points for the criterion benchmarks. Not part of the stable API of this crate.

use pyo3::{
    append_to_inittab,
    ffi::Py_IsInitialized,
    intern,
    prelude::*,
    types::{PyDict, PyInt, PyString, PyTuple},
};

use super::{
    EVENT_DISPATCHERS, PythonReturnCodes, clean_text as clean_text_impl,
    decorators::register_declared_cvars,
    events::{
        ChatEventDispatcher, ClientCommandDispatcher, CommandDispatcher, ConsolePrintDispatcher,
        DamageDispatcher, DeathDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
        EventDispatcherMethods, FrameEventDispatcher, KillDispatcher, RoundEndDispatcher,
        ServerCommandDispatcher, SetConfigstringDispatcher, StatsDispatcher, VoteCalledDispatcher,
        VoteDispatcher, VoteStartedDispatcher, dispatcher_debug_log as dispatcher_debug_log_impl,
    },
    handlers::{
        handle_client_command as handle_client_command_impl,
        handle_console_print as handle_console_print_impl, handle_damage as handle_damage_impl,
        handle_frame as handle_frame_impl, handle_server_command as handle_server_command_impl,
        handle_set_configstring as handle_set_configstring_impl,
    },
    name_of, pyshinqlx_module,
    stats_listener::handle_zmq_msg,
};

/// Starts the embedded interpreter with the shinqlx module and some event dispatchers.
pub fn prepare_python() {
    // SAFETY: Py_IsInitialized may be called before the interpreter exists.
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
        dispatchers
            .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
            .expect("could not add chat dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<CommandDispatcher>())
            .expect("could not add command dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<RoundEndDispatcher>())
            .expect("could not add round end dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<VoteCalledDispatcher>())
            .expect("could not add vote called dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<VoteDispatcher>())
            .expect("could not add vote dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
            .expect("could not add vote started dispatcher");
        dispatchers
            .add_dispatcher(&py.get_type::<ConsolePrintDispatcher>())
            .expect("could not add console print dispatcher");
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

/// A Python handler that does as little as the set_configstring hook of the clan plugin does for
/// an index that does not belong to a player. It is called the way the event dispatchers call the
/// handlers that are hooked to them.
pub struct ClanLikeHandler(Py<PyAny>);

impl Default for ClanLikeHandler {
    fn default() -> Self {
        Python::attach(|py| {
            PyModule::from_code(
                py,
                c"
def handler(index, value):
    if not value:
        return
    elif 529 <= index < 529 + 64:
        return value
",
                c"",
                c"",
            )
            .and_then(|module| module.getattr(intern!(py, "handler")))
            .map(Bound::unbind)
            .map(Self)
            .expect("could not create handler")
        })
    }
}

impl ClanLikeHandler {
    /// Calls the handler with the arguments of a set_configstring event.
    pub fn call(&self, index: u32, value: &str) {
        Python::attach(|py| {
            let args = PyTuple::new(
                py,
                [
                    PyInt::new(py, index).into_any(),
                    PyString::new(py, value).into_any(),
                ],
            )
            .expect("could not create arguments");
            let result = self.0.call1(py, &args).expect("could not call handler");
            let _ = result.extract::<PythonReturnCodes>(py);
        });
    }
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

/// Runs the handler that is called by the server for every line printed to the console.
pub fn handle_console_print(text: &str) {
    Python::attach(|py| {
        handle_console_print_impl(py, text);
    });
}

/// Handles a message of the ZMQ stats protocol and empties the queue of the tasks that it has
/// deferred to the next frame, so that the queue does not grow during a benchmark. Emptying the
/// queue is part of the measured time.
pub fn handle_stats_message_draining(message: &str) {
    Python::attach(|py| {
        handle_zmq_msg(py, message);
        let next_frame_tasks = py
            .import(intern!(py, "shinqlx"))
            .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "next_frame_tasks")))
            .expect("could not get next frame tasks");
        while next_frame_tasks
            .call_method0(intern!(py, "empty"))
            .and_then(|empty| empty.is_truthy())
            .map(|empty| !empty)
            .expect("could not check the queue")
        {
            next_frame_tasks
                .call_method0(intern!(py, "get_nowait"))
                .expect("could not take a task");
        }
    });
}

/// A kill dispatcher with Python handlers hooked to it that is dispatched with a fixed number of
/// arguments, the way the dispatchers of most events call their handlers.
pub struct DispatchCase {
    dispatcher: Py<PyAny>,
    arguments: Py<PyTuple>,
}

impl DispatchCase {
    /// Hooks `handlers` handlers that do nothing and prepares `arguments` arguments.
    pub fn new(handlers: usize, arguments: usize) -> Self {
        Python::attach(|py| {
            let noop = PyModule::from_code(py, c"def noop(*args):\n    return None\n", c"", c"")
                .and_then(|module| module.getattr(intern!(py, "noop")))
                .expect("could not create handler");
            let dispatcher = py
                .get_type::<KillDispatcher>()
                .call0()
                .expect("could not create dispatcher");
            for index in 0..handlers {
                dispatcher
                    .cast::<KillDispatcher>()
                    .expect("this should not happen")
                    .as_super()
                    .get()
                    .hook_without_checks(&format!("bench{index}"), noop.clone().unbind());
            }
            let arguments = PyTuple::new(py, (0..arguments).map(|index| PyInt::new(py, index)))
                .expect("could not create arguments");
            Self {
                dispatcher: dispatcher.unbind(),
                arguments: arguments.unbind(),
            }
        })
    }

    /// Dispatches the event once.
    pub fn dispatch(&self) {
        Python::attach(|py| {
            let _ = self
                .dispatcher
                .bind(py)
                .cast::<KillDispatcher>()
                .expect("this should not happen")
                .as_super()
                .dispatch(self.arguments.bind(py));
        });
    }
}

/// A Python snippet that is run in a loop inside the interpreter, so that operations that take only
/// nanoseconds are not dominated by the cost of entering and leaving the interpreter.
///
/// The `setup` runs once at module level after `import shinqlx`, the `body` is the statement that
/// is repeated.
pub struct PythonCase(Py<PyAny>);

impl PythonCase {
    /// Compiles the snippet.
    pub fn new(setup: &str, body: &str) -> Self {
        let indented_body = body
            .lines()
            .map(|line| format!("        {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let source = std::ffi::CString::new(format!(
            "import shinqlx\n{setup}\n\ndef run(count):\n    for _ in range(count):\n{indented_body}\n"
        ))
        .expect("the snippet must not contain a nul byte");

        Python::attach(|py| {
            PyModule::from_code(py, &source, c"", c"")
                .and_then(|module| module.getattr(intern!(py, "run")))
                .map(Bound::unbind)
                .map(Self)
                .expect("could not compile the snippet")
        })
    }

    /// Runs the body `iterations` times.
    pub fn run(&self, iterations: usize) {
        Python::attach(|py| {
            self.0
                .call1(py, (iterations,))
                .expect("could not run the snippet");
        });
    }
}

/// Converts the value of a Python expression to the name that the setters of teams and privileges
/// use, the way `Player.team` and friends do.
pub struct NameOfCase(Py<PyAny>);

impl NameOfCase {
    /// Evaluates the expression, e.g. `shinqlx.Teams.RED` or `"red"`.
    pub fn new(expression: &str) -> Self {
        let code =
            std::ffi::CString::new(expression).expect("the expression must not contain a nul byte");
        Python::attach(|py| {
            let globals = PyDict::new(py);
            globals
                .set_item(
                    "shinqlx",
                    py.import(intern!(py, "shinqlx")).expect("no shinqlx"),
                )
                .expect("could not set shinqlx");
            py.eval(&code, Some(&globals), None)
                .map(Bound::unbind)
                .map(Self)
                .expect("could not evaluate the expression")
        })
    }

    /// Converts the value `iterations` times.
    pub fn run(&self, iterations: usize) {
        Python::attach(|py| {
            let value = self.0.bind(py);
            for _ in 0..iterations {
                name_of(value).expect("could not determine the name");
            }
        });
    }
}

/// Registers the `Cvar` declarations of a plugin class that has many methods, the way the plugin
/// loader does before the plugin is instantiated. The cvars have no default, so no engine is needed.
pub struct DeclaredCvarsCase(Py<PyAny>);

impl DeclaredCvarsCase {
    /// Creates a plugin class with `cvars` declarations and `methods` methods.
    pub fn new(cvars: usize, methods: usize) -> Self {
        let cvar_lines = (0..cvars)
            .map(|index| format!("    cvar_{index} = shinqlx.Cvar('qlx_bench{index}')"))
            .collect::<Vec<_>>()
            .join("\n");
        let method_lines = (0..methods)
            .map(|index| format!("    def method_{index}(self):\n        return {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let source = std::ffi::CString::new(format!(
            "import shinqlx\n\nclass bench_plugin(shinqlx.Plugin):\n{cvar_lines}\n{method_lines}\n    pass\n"
        ))
        .expect("the class must not contain a nul byte");

        Python::attach(|py| {
            PyModule::from_code(py, &source, c"", c"")
                .and_then(|module| module.getattr(intern!(py, "bench_plugin")))
                .map(Bound::unbind)
                .map(Self)
                .expect("could not create the plugin class")
        })
    }

    /// Registers the declared cvars once.
    pub fn register(&self) {
        Python::attach(|py| {
            register_declared_cvars(self.0.bind(py)).expect("could not register the cvars");
        });
    }
}

/// The limiter that drops the Steam queries of a source that sends too many.
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
pub struct QueryLimiterCase(Box<crate::query_rate_limiter::QueryRateLimiter>);

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
impl QueryLimiterCase {
    /// A limiter with the default limits of the server.
    pub fn with_defaults() -> Self {
        Self::with_limits(1.0, 10.0, 10.0)
    }

    /// A limiter with the given per source rate, burst and global rate. `0` disables a limit.
    pub fn with_limits(per_source_rate: f64, burst: f64, global_rate: f64) -> Self {
        Self(Box::new(crate::query_rate_limiter::QueryRateLimiter::new(
            crate::query_rate_limiter::QueryLimits {
                per_source_rate,
                burst,
                global_rate,
            },
        )))
    }

    /// Asks whether the packet of `source` may be handed to Steam at `now_ms`.
    pub fn allow(&mut self, source: [u8; 4], now_ms: u64) -> bool {
        self.0.allow(source, now_ms)
    }
}
