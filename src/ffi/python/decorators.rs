use core::hint::cold_path;

use pyo3::{
    exceptions::{PyAttributeError, PyTypeError, PyValueError},
    intern,
    prelude::*,
    types::{PyBool, PyCFunction, PyDict, PyFloat, PyInt, PyList, PySet, PyTuple, PyType},
};

use super::{
    commands::CommandPriorities,
    deferred::{Deferral, DeferredCall},
    plugin::{Plugin, PluginMethods},
};

const HOOKS_MARKER: &str = "_shinqlx_hooks";
const COMMANDS_MARKER: &str = "_shinqlx_commands";
const CVAR_TYPES: [&str; 7] = ["str", "int", "float", "bool", "list", "set", "tuple"];
const COMMAND_PARAMETERS: [&str; 3] = ["player", "msg", "channel"];

pub(super) fn only_handler<'py>(arguments: &Bound<'py, PyTuple>) -> PyResult<Bound<'py, PyAny>> {
    match arguments.len() {
        1 => arguments.get_item(0),
        count => Err(PyTypeError::new_err(format!(
            "a decorator takes exactly one handler, but {count} were given"
        ))),
    }
}

// dispatch() of these takes fewer arguments than their hooks receive
const HOOK_PARAMETERS: [(&str, &[&str]); 2] = [
    ("vote_started", &["player", "vote", "args"]),
    ("vote_ended", &["votes", "vote", "args", "passed"]),
];

fn dispatcher_parameters(py: Python<'_>, event: &str) -> Option<Vec<String>> {
    if let Some((_, names)) = HOOK_PARAMETERS.iter().find(|(name, _)| *name == event) {
        return Some(names.iter().map(ToString::to_string).collect());
    }

    let dispatcher = py
        .import(intern!(py, "shinqlx"))
        .and_then(|shinqlx| shinqlx.getattr(intern!(py, "EVENT_DISPATCHERS")))
        .and_then(|dispatchers| dispatchers.get_item(event))
        .ok()?;
    let signature = py
        .import(intern!(py, "inspect"))
        .and_then(|inspect| {
            inspect.call_method1(intern!(py, "signature"), (dispatcher.getattr("dispatch")?,))
        })
        .ok()?;

    let mut names = vec![];
    for parameter in signature
        .getattr(intern!(py, "parameters"))
        .and_then(|parameters| parameters.call_method0(intern!(py, "values")))
        .ok()?
        .try_iter()
        .ok()?
    {
        let parameter = parameter.ok()?;
        let kind = parameter
            .getattr(intern!(py, "kind"))
            .and_then(|kind| kind.getattr(intern!(py, "name")))
            .ok()?
            .to_string();
        match kind.as_str() {
            "VAR_POSITIONAL" => return None,
            "POSITIONAL_ONLY" | "POSITIONAL_OR_KEYWORD" => {
                names.push(parameter.getattr(intern!(py, "name")).ok()?.to_string());
            }
            _ => (),
        }
    }
    Some(names)
}

fn check_accepts<S: AsRef<str>>(
    handler: &Bound<'_, PyAny>,
    names: &[S],
    purpose: &str,
    receivers: usize,
) -> PyResult<()> {
    let py = handler.py();
    let Some(signature) = py
        .import(intern!(py, "inspect"))
        .and_then(|inspect| inspect.call_method1(intern!(py, "signature"), (handler,)))
        .ok()
    else {
        return Ok(());
    };

    let placeholders = PyTuple::new(py, (0..names.len() + receivers).map(|_| py.None()))?;
    signature
        .call_method1(intern!(py, "bind"), placeholders)
        .map(|_| ())
        .map_err(|error| {
            if !error.is_instance_of::<PyTypeError>(py) {
                return error;
            }
            let handler_name = handler
                .getattr(intern!(py, "__qualname__"))
                .map_or_else(|_| format!("{handler:?}"), |name| name.to_string());
            let expected = names
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", ");
            PyTypeError::new_err(format!(
                "{handler_name} is {purpose} and has to accept ({expected}): {}",
                error.value(py)
            ))
        })
}

fn mark(handler: &Bound<'_, PyAny>, marker: &str, entry: Bound<'_, PyAny>) -> PyResult<()> {
    let py = handler.py();
    let mut entries = match handler.getattr(marker) {
        Ok(existing) => existing.try_iter()?.collect::<PyResult<Vec<_>>>()?,
        Err(error) if error.is_instance_of::<PyAttributeError>(py) => vec![],
        Err(error) => return Err(error),
    };
    entries.push(entry);

    handler.setattr(marker, PyTuple::new(py, entries)?)
}

fn markers<'py>(declared: &Bound<'py, PyAny>, marker: &str) -> Vec<Bound<'py, PyAny>> {
    declared
        .getattr(marker)
        .and_then(|entries| entries.try_iter())
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default()
}

/// Hooks the decorated method of a plugin to an event. The hook is registered automatically when
/// the plugin is loaded, so the plugin does not need to call :meth:`Plugin.add_hook` itself.
///
/// :param event: The name of the event to hook the method to.
/// :param priority: The priority of the hook, one of the ``PRI_*`` constants. Defaults to
///     :data:`PRI_NORMAL`.
#[pyfunction]
#[pyo3(signature = (event, priority = None), text_signature = "(event, priority=None)")]
fn hook(py: Python<'_>, event: String, priority: Option<i32>) -> PyResult<Bound<'_, PyCFunction>> {
    PyCFunction::new_closure(
        py,
        Some(c"hook_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let handler = only_handler(arguments)?;
            if let Some(names) = dispatcher_parameters(handler.py(), &event) {
                check_accepts(&handler, &names, &format!("hooked to '{event}'"), 1)?;
            }

            let entry = (event.as_str(), priority).into_pyobject(handler.py())?;
            mark(&handler, HOOKS_MARKER, entry.into_any())?;
            Ok::<_, PyErr>(handler.unbind())
        },
    )
}

/// Makes the decorated method of a plugin a command. The command is registered automatically when
/// the plugin is loaded, so the plugin does not need to call :meth:`Plugin.add_command` itself.
///
/// :param name: The name of the command, or a list of names.
/// :param options: The optional arguments of :meth:`Plugin.add_command`, such as ``permission``,
///     ``channels``, ``exclude_channels``, ``priority``, ``client_cmd_pass``, ``client_cmd_perm``,
///     ``prefix`` and ``usage``.
#[pyfunction]
#[pyo3(
    signature = (name, **options),
    text_signature = "(name, *, permission = 0, channels = None, exclude_channels = None, priority = PRI_NORMAL, client_cmd_pass = False, client_cmd_perm = 0, prefix = True, usage = \"\")"
)]
fn command<'py>(
    py: Python<'py>,
    name: Bound<'py, PyAny>,
    options: Option<Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyCFunction>> {
    let name = name.unbind();
    let options = options.map_or_else(|| Ok(PyDict::new(py)), |options| options.copy())?;
    let options = options.unbind();

    PyCFunction::new_closure(
        py,
        Some(c"command_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let handler = only_handler(arguments)?;
            let py = handler.py();
            check_accepts(&handler, &COMMAND_PARAMETERS, "a command handler", 1)?;

            let entry = (name.bind(py), options.bind(py)).into_pyobject(py)?;
            mark(&handler, COMMANDS_MARKER, entry.into_any())?;
            Ok::<_, PyErr>(handler.unbind())
        },
    )
}

/// The declaration of a cvar of a plugin.
///
/// Assigned to an attribute of a plugin class, it creates the cvar with the default value when the
/// plugin is loaded, before the plugin is instantiated, unless the cvar exists already. The
/// attribute is read-only and returns the current value of the cvar in the declared type on every
/// access, so type checkers know the type of the attribute.
///
/// :param name: The name of the cvar.
/// :param default: The value to create the cvar with. Without a default, the cvar is not created.
///     ``bool`` values are written as ``1`` or ``0``, lists, tuples and sets as comma separated values.
/// :param type: The type that the attribute returns, one of ``str``, ``int``, ``float``, ``bool``,
///     ``list``, ``set`` and ``tuple``. Defaults to the type of the default value, or ``str``.
/// :param flags: The cvar flags to create the cvar with, e.g. a :class:`CvarFlags` member.
/// :param minimum: The lower limit of the value of the cvar. Needs ``maximum`` as well.
/// :param maximum: The upper limit of the value of the cvar. Needs ``minimum`` as well.
#[pyclass(module = "_shinqlx", name = "Cvar", frozen)]
struct Cvar {
    name: String,
    default: Option<String>,
    value_type: Py<PyAny>,
    flags: i32,
    minimum: Option<Py<PyAny>>,
    maximum: Option<Py<PyAny>>,
}

#[pymethods]
impl Cvar {
    #[new]
    #[pyo3(
        signature = (name, default = None, *, r#type = None, flags = 0, minimum = None, maximum = None),
        text_signature = "(name, default=None, *, type=None, flags=0, minimum=None, maximum=None)"
    )]
    fn new<'py>(
        py: Python<'py>,
        name: String,
        default: Option<Bound<'py, PyAny>>,
        r#type: Option<Bound<'py, PyAny>>,
        flags: i32,
        minimum: Option<Bound<'py, PyAny>>,
        maximum: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Self> {
        if name.is_empty() {
            cold_path();
            return Err(PyValueError::new_err(
                "the name of a cvar must not be empty",
            ));
        }
        if minimum.is_some() != maximum.is_some() {
            cold_path();
            return Err(PyValueError::new_err(
                "minimum and maximum have to be given together",
            ));
        }

        let value_type = cvar_value_type(py, r#type, default.as_ref())?.unbind();
        let default = default.as_ref().map(cvar_default_string).transpose()?;

        Ok(Self {
            name,
            default,
            value_type,
            flags,
            minimum: minimum.map(Bound::unbind),
            maximum: maximum.map(Bound::unbind),
        })
    }

    fn __get__<'py>(
        slf: &Bound<'py, Self>,
        instance: &Bound<'py, PyAny>,
        _owner: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if instance.is_none() {
            return Ok(slf.clone().into_any());
        }

        let declaration = slf.get();
        instance.call_method1(
            intern!(slf.py(), "get_cvar"),
            (
                declaration.name.as_str(),
                declaration.value_type.bind(slf.py()),
            ),
        )
    }

    fn __set__(&self, _instance: &Bound<'_, PyAny>, _value: &Bound<'_, PyAny>) -> PyResult<()> {
        Err(PyAttributeError::new_err(format!(
            "the cvar '{}' is read-only, use set_cvar to change it",
            self.name
        )))
    }
}

impl Cvar {
    fn create_unless_existing(&self, plugin_class: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = plugin_class.py();
        let Some(default) = &self.default else {
            return Ok(());
        };

        match (&self.minimum, &self.maximum) {
            (Some(minimum), Some(maximum)) => plugin_class.call_method1(
                intern!(py, "set_cvar_limit_once"),
                (
                    self.name.as_str(),
                    default,
                    minimum.bind(py),
                    maximum.bind(py),
                    self.flags,
                ),
            )?,
            _ => plugin_class.call_method1(
                intern!(py, "set_cvar_once"),
                (self.name.as_str(), default, self.flags),
            )?,
        };

        Ok(())
    }
}

fn cvar_value_type<'py>(
    py: Python<'py>,
    declared: Option<Bound<'py, PyAny>>,
    default: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    if let Some(declared) = declared {
        let declared_name = declared
            .getattr(intern!(py, "__name__"))
            .and_then(|name| name.extract::<String>())
            .unwrap_or_default();
        if !declared.is_instance_of::<PyType>() || !CVAR_TYPES.contains(&declared_name.as_str()) {
            cold_path();
            return Err(PyValueError::new_err(format!(
                "type has to be one of {}",
                CVAR_TYPES.join(", ")
            )));
        }
        return Ok(declared);
    }

    let inferred = match default {
        Some(value) if value.is_instance_of::<PyBool>() => "bool",
        Some(value) if value.is_instance_of::<PyInt>() => "int",
        Some(value) if value.is_instance_of::<PyFloat>() => "float",
        Some(value) if value.is_instance_of::<PyList>() => "list",
        Some(value) if value.is_instance_of::<PyTuple>() => "tuple",
        Some(value) if value.is_instance_of::<PySet>() => "set",
        _ => "str",
    };
    py.import(intern!(py, "builtins"))?.getattr(inferred)
}

fn cvar_default_string(default: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(flag) = default.cast::<PyBool>() {
        return Ok(if flag.is_true() { "1" } else { "0" }.to_string());
    }
    if default.is_instance_of::<PyList>()
        || default.is_instance_of::<PyTuple>()
        || default.is_instance_of::<PySet>()
    {
        return default
            .try_iter()?
            .map(|item| {
                item.and_then(|item| item.str())
                    .map(|item| item.to_string())
            })
            .collect::<PyResult<Vec<_>>>()
            .map(|items| items.join(","));
    }

    default.str().map(|value| value.to_string())
}

/// Adds the `hook` and `command` decorators and the `Cvar` declaration to the shinqlx module.
pub(crate) fn register_decorators(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hook, m)?)?;
    m.add_function(wrap_pyfunction!(command, m)?)?;
    m.add_class::<Cvar>()
}

/// Creates the cvars that were declared as :class:`Cvar` attributes of the plugin class, unless they
/// exist already.
pub(crate) fn register_declared_cvars(plugin_class: &Bound<'_, PyAny>) -> PyResult<()> {
    let declarations = plugin_class
        .dir()?
        .iter()
        .filter_map(|attribute_name| attribute_name.extract::<String>().ok())
        .filter_map(|attribute_name| plugin_class.getattr(attribute_name.as_str()).ok());

    for declaration in declarations {
        if let Ok(declaration) = declaration.cast::<Cvar>() {
            declaration.get().create_unless_existing(plugin_class)?;
        }
    }

    Ok(())
}

/// Registers the hooks and commands that were declared by decorating methods of the plugin.
pub(crate) fn register_decorated_handlers(plugin: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = plugin.py();
    let plugin_class = plugin.get_type();

    for attribute_name in plugin_class.dir()?.iter() {
        let attribute_name = attribute_name.extract::<String>()?;
        let Ok(declared) = plugin_class.getattr(attribute_name.as_str()) else {
            continue;
        };
        let hooks = markers(&declared, HOOKS_MARKER)
            .iter()
            .map(|entry| entry.extract::<(String, Option<i32>)>())
            .collect::<PyResult<Vec<_>>>()?;
        let commands = markers(&declared, COMMANDS_MARKER)
            .iter()
            .map(|entry| entry.extract::<(Bound<'_, PyAny>, Bound<'_, PyDict>)>())
            .collect::<PyResult<Vec<_>>>()?;
        if hooks.is_empty() && commands.is_empty() {
            continue;
        }

        let handler = plugin.getattr(attribute_name.as_str())?;
        for (event, _) in &hooks {
            if let Some(names) = dispatcher_parameters(py, event) {
                check_accepts(&handler, &names, &format!("hooked to '{event}'"), 0)?;
            }
        }
        if !commands.is_empty() {
            check_accepts(&handler, &COMMAND_PARAMETERS, "a command handler", 0)?;
        }

        let plugin_methods = plugin.cast::<Plugin>()?;
        for (event, priority) in &hooks {
            plugin_methods.add_hook(
                event,
                &handler,
                priority.unwrap_or(CommandPriorities::PRI_NORMAL as i32),
            )?;
        }
        for (command_name, options) in &commands {
            plugin.call_method(
                intern!(py, "add_command"),
                (command_name, &handler),
                Some(options),
            )?;
        }
    }

    Ok(())
}

#[pyfunction]
pub(super) fn next_frame<'py>(func: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if !func.is_callable() {
        cold_path();
        return Err(PyValueError::new_err(
            "func has to be a callable Python function",
        ));
    }

    DeferredCall::wrap(func, Deferral::NextFrame)
}

/// Delay a function call a certain amount of time.
///
///     .. note::
///         It cannot guarantee you that it will be called right as the timer
///         expires, but unless some plugin is for some reason blocking, then
///         you can expect it to be called practically as soon as it expires.
#[pyfunction]
pub(super) fn delay<'py>(py: Python<'py>, time: f64) -> PyResult<Bound<'py, PyAny>> {
    PyCFunction::new_closure(
        py,
        Some(c"delay_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let func = only_handler(arguments)?;
            if !func.is_callable() {
                cold_path();
                return Err(PyValueError::new_err("'func' must be callable"));
            }

            DeferredCall::wrap(&func, Deferral::Delay(time)).map(Bound::unbind)
        },
    )
    .map(Bound::into_any)
}

/// Starts a thread with the function passed as its target. If a function decorated
/// with this is called within a function also decorated, it will **not** create a second
/// thread unless told to do so with the *force* keyword.
#[pyfunction]
pub(super) fn thread<'py>(func: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if !func.is_callable() {
        cold_path();
        return Err(PyValueError::new_err(
            "func has to be a callable Python function",
        ));
    }

    DeferredCall::wrap(func, Deferral::Thread)
}

#[cfg(test)]
mod decorators_tests {
    use core::borrow::BorrowMut;

    use pyo3::{intern, prelude::*, types::PyList};
    use rstest::rstest;

    use super::register_decorated_handlers;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                COMMANDS, EVENT_DISPATCHERS,
                commands::CommandInvoker,
                events::{
                    EventDispatcherManager, EventDispatcherManagerMethods,
                    TeamSwitchAttemptDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
            },
        },
        prelude::*,
    };

    fn decorated_plugin<'py>(py: Python<'py>, class_body: &str) -> Bound<'py, PyAny> {
        let code = std::ffi::CString::new(format!(
            "import shinqlx\n\nclass decorated_plugin(shinqlx.Plugin):\n{class_body}\n"
        ))
        .expect("this should not happen");
        PyModule::from_code(py, &code, c"", c"")
            .and_then(|module| module.getattr(intern!(py, "decorated_plugin")))
            .and_then(|plugin_class| plugin_class.call0())
            .expect("could not create plugin instance")
    }

    fn with_dispatchers_and_commands<F>(test: F)
    where
        F: FnOnce(Python<'_>),
    {
        let cvar_string = c"0";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatchers = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatchers
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add team switch attempt dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatchers.unbind().into()));
                    COMMANDS.store(Some(
                        Py::new(py, CommandInvoker::py_new())
                            .expect("could not create command invoker")
                            .into(),
                    ));

                    test(py);
                });
            });
    }

    fn registered(plugin: &Bound<'_, PyAny>, attribute: &str) -> usize {
        plugin
            .getattr(attribute)
            .expect("could not get attribute")
            .cast::<PyList>()
            .expect("could not downcast to list")
            .len()
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn decorators_only_mark_the_methods(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let marked = PyModule::from_code(
                py,
                c"
import shinqlx

@shinqlx.command('slap', permission=3)
@shinqlx.hook('chat', priority=shinqlx.PRI_HIGH)
@shinqlx.hook('team_switch')
def handler(self, *args): pass
",
                c"",
                c"",
            )
            .and_then(|module| module.getattr(intern!(py, "handler")))
            .expect("could not decorate handler");

            assert_eq!(
                marked
                    .getattr("_shinqlx_hooks")
                    .and_then(|hooks| hooks.len())
                    .expect("could not get hooks"),
                2
            );
            assert_eq!(
                marked
                    .getattr("_shinqlx_commands")
                    .and_then(|commands| commands.len())
                    .expect("could not get commands"),
                1
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn plugin_without_decorated_methods_registers_nothing(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(py, "    def handler(self): pass");

            assert!(register_decorated_handlers(&plugin).is_ok());
            assert_eq!(registered(&plugin, "hooks"), 0);
            assert_eq!(registered(&plugin, "commands"), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn decorated_methods_are_registered_as_hook_and_command(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.hook('team_switch_attempt', priority=shinqlx.PRI_HIGH)\n    \
                 def on_team_switch_attempt(self, player, old_team, new_team): pass\n\n    \
                 @shinqlx.command('slap', permission=1, usage='<name>')\n    \
                 def cmd_slap(self, player, msg, channel): pass",
            );

            assert!(register_decorated_handlers(&plugin).is_ok());
            assert_eq!(registered(&plugin, "hooks"), 1);
            assert_eq!(registered(&plugin, "commands"), 1);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stacked_decorators_register_every_hook(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.hook('team_switch_attempt')\n    \
                 @shinqlx.hook('team_switch_attempt', priority=shinqlx.PRI_LOW)\n    \
                 def on_team_switch_attempt(self, player, old_team, new_team): pass",
            );

            assert!(register_decorated_handlers(&plugin).is_ok());
            assert_eq!(registered(&plugin, "hooks"), 2);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn hooking_an_unknown_event_fails(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.hook('no_such_event')\n    def handler(self): pass",
            );

            assert!(register_decorated_handlers(&plugin).is_err());
        });
    }
}

#[cfg(test)]
mod decorator_signature_tests {
    use core::borrow::BorrowMut;
    use std::ffi::CString;

    use pyo3::{exceptions::PyTypeError, intern, prelude::*, types::PyDict};
    use rstest::rstest;

    use super::register_decorated_handlers;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                COMMANDS, commands::CommandInvoker, pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::RestoredAttribute,
            },
        },
        prelude::*,
    };

    fn plugin_class<'py>(py: Python<'py>, class_body: &str) -> PyResult<Bound<'py, PyAny>> {
        let code = CString::new(format!(
            "import shinqlx\n\nclass SamplePlugin(shinqlx.Plugin):\n{class_body}\n"
        ))
        .expect("this should not happen");
        PyModule::from_code(py, &code, c"", c"")
            .and_then(|module| module.getattr(intern!(py, "SamplePlugin")))
    }

    fn method(decorator: &str, signature: &str) -> String {
        format!("    {decorator}\n    def handler({signature}): pass")
    }

    fn with_engine<F>(test: F)
    where
        F: FnOnce(Python<'_>),
    {
        let cvar_string = c"0";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let mut engine = MockEngineBuilder::default().with_find_cvar(
            |cmd| cmd == "zmq_stats_enable",
            move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            0..,
        );
        engine.run(|| {
            Python::attach(|py| {
                COMMANDS.store(Some(
                    Py::new(py, CommandInvoker::py_new())
                        .expect("could not create command invoker")
                        .into(),
                ));
                test(py);
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_plugin_that_skips_the_base_constructor_gets_its_command_registered(_pyshinqlx_setup: ()) {
        with_engine(|py| {
            let plugin = plugin_class(
                py,
                "    def __init__(self):\n        pass\n\n    \
                 @shinqlx.command('slap')\n    \
                 def cmd_slap(self, *args, **kwargs):\n        pass",
            )
            .and_then(|plugin_class| plugin_class.call0())
            .expect("could not create the sample plugin");

            let result = register_decorated_handlers(&plugin);

            assert!(result.is_ok());
            let names = plugin
                .getattr(intern!(py, "commands"))
                .and_then(|commands| commands.get_item(0))
                .and_then(|command| command.getattr(intern!(py, "name")))
                .map(|names| names.to_string());
            assert_eq!(names.ok(), Some("['slap']".to_string()));
        });
    }

    #[rstest]
    #[case("self, player, msg, channel", true)]
    #[case("self, *args, **kwargs", true)]
    #[case("self, player, msg, channel, extra=None", true)]
    #[case("self, player, *rest", true)]
    #[case("self, player", false)]
    #[case("self, player, msg", false)]
    #[case("self", false)]
    #[case("self, player, msg, channel, extra", false)]
    #[case("player, msg, channel", false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn command_handlers_have_to_accept_player_msg_and_channel(
        _pyshinqlx_setup: (),
        #[case] signature: &str,
        #[case] accepted: bool,
    ) {
        Python::attach(|py| {
            let result = plugin_class(py, &method("@shinqlx.command('slap')", signature));

            match (accepted, result) {
                (true, result) => assert!(result.is_ok(), "{signature}: {result:?}"),
                (false, Err(err)) => {
                    assert!(
                        err.is_instance_of::<PyTypeError>(py),
                        "{signature}: {err:?}"
                    );
                    assert!(
                        err.value(py)
                            .to_string()
                            .contains("has to accept (player, msg, channel)")
                    );
                }
                (false, Ok(_)) => panic!("{signature} should have been rejected"),
            }
        });
    }

    #[rstest]
    #[case("frame", "self", true)]
    #[case("frame", "self, *args", true)]
    #[case("frame", "self, player", false)]
    #[case("player_connect", "self, player", true)]
    #[case("player_connect", "self", false)]
    #[case("player_connect", "self, player, extra", false)]
    #[case("team_switch_attempt", "self, player, old_team, new_team", true)]
    #[case("team_switch_attempt", "self, player, *teams", true)]
    #[case("team_switch_attempt", "self, player", false)]
    #[case("chat", "self, player, msg, channel", true)]
    #[case("chat", "self, player, msg", false)]
    #[case("vote_started", "self, player, vote, args", true)]
    #[case("vote_started", "self, vote, args", false)]
    #[case("vote_ended", "self, votes, vote, args, passed", true)]
    #[case("vote_ended", "self, passed", false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn hook_handlers_have_to_accept_the_arguments_of_their_event(
        _pyshinqlx_setup: (),
        #[case] event: &str,
        #[case] signature: &str,
        #[case] accepted: bool,
    ) {
        Python::attach(|py| {
            let result = plugin_class(py, &method(&format!("@shinqlx.hook('{event}')"), signature));

            match (accepted, result) {
                (true, result) => assert!(result.is_ok(), "{event} {signature}: {result:?}"),
                (false, Err(err)) => {
                    assert!(err.is_instance_of::<PyTypeError>(py), "{event}: {err:?}");
                    assert!(
                        err.value(py)
                            .to_string()
                            .contains(&format!("hooked to '{event}'"))
                    );
                }
                (false, Ok(_)) => panic!("{event} {signature} should have been rejected"),
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_signature_is_checked_again_when_the_dispatcher_was_unknown_while_decorating(
        _pyshinqlx_setup: (),
    ) {
        with_engine(|py| {
            let shinqlx = py
                .import(intern!(py, "shinqlx"))
                .expect("no shinqlx module");
            let plugin = {
                let _restore = RestoredAttribute::capture(shinqlx.as_any(), "EVENT_DISPATCHERS");
                shinqlx
                    .setattr(intern!(py, "EVENT_DISPATCHERS"), PyDict::new(py))
                    .expect("could not hide the dispatchers");
                plugin_class(py, &method("@shinqlx.hook('frame')", "self, player"))
                    .and_then(|plugin_class| plugin_class.call0())
                    .expect("the decorator should not know the event yet")
            };

            let result = register_decorated_handlers(&plugin);

            assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_unknown_event_is_reported_when_registering_not_when_decorating(_pyshinqlx_setup: ()) {
        with_engine(|py| {
            let plugin = plugin_class(py, &method("@shinqlx.hook('no_such_event')", "self"))
                .and_then(|plugin_class| plugin_class.call0())
                .expect("an unknown event must not fail while decorating");

            let result = register_decorated_handlers(&plugin);

            assert!(result.is_err());
        });
    }
}
#[cfg(test)]
mod next_frame_tests {
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use crate::{
        ffi::python::{pyshinqlx_setup_fixture::*, pyshinqlx_test_support::run_all_frame_tasks},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_enqueues_function_call(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.next_frame
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "get_nowait"))
                    .is_ok()
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn next_frame_enqueues_function_call_on_an_instance_method(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

class TestClass:
    @shinqlx.next_frame
    def test_func(self):
        pass

TestClass().test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let result2 = run_all_frame_tasks(py);
            assert!(result2.is_ok());
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }
}

#[cfg(test)]
mod delay_tests {
    use pretty_assertions::assert_eq;
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use crate::{ffi::python::pyshinqlx_setup_fixture::*, prelude::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_enqueues_function_call(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.delay(10)
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            let scheduler_queue = frame_tasks
                .getattr(intern!(py, "queue"))
                .expect("this should not happen");
            assert_eq!(scheduler_queue.len().expect("this should not happen"), 1);
            let queue_entry = scheduler_queue.get_item(0).expect("this should not happen");
            let _ = frame_tasks.call_method1(intern!(py, "cancel"), (queue_entry,));
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_with_float_delay(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

@shinqlx.delay(5.5)
def test_func():
    pass

test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| !bool_value.is_true()))
            );
            let scheduler_queue = frame_tasks
                .getattr(intern!(py, "queue"))
                .expect("this should not happen");
            assert_eq!(scheduler_queue.len().expect("this should not happen"), 1);
            let queue_entry = scheduler_queue.get_item(0).expect("this should not happen");
            let _ = frame_tasks.call_method1(intern!(py, "cancel"), (queue_entry,));
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_enqueues_function_call_for_class_method(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = py.run(
                cr#"
import shinqlx

class TestClass:
    @shinqlx.delay(0.0001)
    def test_func(self):
        pass

TestClass().test_func()
"#,
                None,
                None,
            );
            assert!(result.is_ok());

            let frame_tasks = py
                .import(intern!(py, "shinqlx"))
                .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "frame_tasks")))
                .expect("this should not happen");
            let sched_result = frame_tasks.call_method0(intern!(py, "run"));
            assert!(sched_result.is_ok());
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| value
                        .cast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true()))
            );
        });
    }
}

#[cfg(test)]
mod thread_tests {
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use crate::{ffi::python::pyshinqlx_setup_fixture::*, prelude::serial};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_on_callable_function(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let py_test_module = PyModule::from_code(
                py,
                cr#"
import shinqlx

called = False

@shinqlx.thread
def threaded_func():
    global called
    called = True
"#,
                c"",
                c"",
            )
            .expect("this should not happen");

            let result = py_test_module.call_method0(intern!(py, "threaded_func"));
            assert!(result.is_ok());

            assert!(
                py_test_module
                    .getattr(intern!(py, "called"))
                    .is_ok_and(|called| {
                        called.cast::<PyBool>().is_ok_and(|pybool| pybool.is_true())
                    })
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn thread_on_nested_thread(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let py_test_module = PyModule::from_code(
                py,
                cr#"
import shinqlx

called = False

@shinqlx.thread
def inner_thread():
    global called
    called = True

@shinqlx.thread
def threaded_func():
    inner_thread()
"#,
                c"",
                c"",
            )
            .expect("this should not happen");

            let result = py_test_module.call_method0(intern!(py, "threaded_func"));
            assert!(result.is_ok());

            assert!(
                py_test_module
                    .getattr(intern!(py, "called"))
                    .is_ok_and(|called| {
                        called.cast::<PyBool>().is_ok_and(|pybool| pybool.is_true())
                    })
            );
        });
    }
}

#[cfg(test)]
mod cvar_declaration_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{
        exceptions::{PyAttributeError, PyValueError},
        intern,
        prelude::*,
    };
    use rstest::rstest;

    use super::register_declared_cvars;
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::pyshinqlx_setup_fixture::pyshinqlx_setup,
        },
        prelude::*,
    };

    fn plugin_module<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyModule>> {
        let code = std::ffi::CString::new(format!("import shinqlx\n\n{source}\n"))
            .expect("this should not happen");
        PyModule::from_code(py, &code, c"", c"")
    }

    fn plugin_class<'py>(py: Python<'py>, source: &str) -> Bound<'py, PyAny> {
        plugin_module(py, source)
            .and_then(|module| module.getattr(intern!(py, "declared_plugin")))
            .expect("could not create plugin class")
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_with_limits_is_created_with_the_limits(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_delay", |_| None, 1..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_cvar_limit()
                    .with(
                        predicate::eq("qlx_delay"),
                        predicate::eq("5"),
                        predicate::eq("0"),
                        predicate::eq("60"),
                        predicate::eq(Some(1)),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class declared_plugin(shinqlx.Plugin):\n    unused = shinqlx.Cvar('qlx_delay', 5, minimum=0, maximum=60, flags=shinqlx.CvarFlags.ARCHIVE)",
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                });
            });
    }

    #[rstest]
    #[case("True", "1")]
    #[case("False", "0")]
    #[case("'text'", "text")]
    #[case("2.5", "2.5")]
    #[case("['a', 'b']", "a,b")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_default_is_written_as_string(
        _pyshinqlx_setup: (),
        #[case] default: &str,
        #[case] expected: &str,
    ) {
        let expected = expected.to_string();
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_value", |_| None, 1..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("qlx_value"),
                        predicate::eq(expected),
                        predicate::eq(Some(0)),
                    )
                    .times(1);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        &format!(
                            "class declared_plugin(shinqlx.Plugin):\n    unused = shinqlx.Cvar('qlx_value', {default})"
                        ),
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_without_default_is_not_created(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0)
            .configure(|mock_engine| {
                mock_engine.expect_get_cvar().times(0);
                mock_engine.expect_set_cvar_limit().times(0);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class declared_plugin(shinqlx.Plugin):\n    hostname = shinqlx.Cvar('sv_hostname')",
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn existing_cvar_keeps_its_value(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_value",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .configure(|mock_engine| {
                mock_engine.expect_get_cvar().times(0);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class declared_plugin(shinqlx.Plugin):\n    unused = shinqlx.Cvar('qlx_value', 'default')",
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                });
            });
    }

    #[rstest]
    #[case("int", "7", "7")]
    #[case("float", "2.5", "2.5")]
    #[case("str", "text", "'text'")]
    #[case("bool", "1", "True")]
    #[case("list", "a,b", "['a', 'b']")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn field_returns_the_current_value_in_the_declared_type(
        _pyshinqlx_setup: (),
        #[case] type_name: &str,
        #[case] cvar_value: &str,
        #[case] expected: &str,
    ) {
        let value = std::ffi::CString::new(cvar_value).expect("this should not happen");
        let mut raw_cvar = CVarBuilder::default()
            .string(value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_value",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        &format!(
                            "class declared_plugin(shinqlx.Plugin):\n    value = shinqlx.Cvar('qlx_value', type={type_name})"
                        ),
                    );

                    let instance = plugin_class.call0().expect("could not create plugin");
                    let read = instance
                        .getattr(intern!(py, "value"))
                        .expect("could not read the field");
                    let expected = py
                        .eval(
                            &std::ffi::CString::new(expected).expect("this should not happen"),
                            None,
                            None,
                        )
                        .expect("could not evaluate the expectation");
                    assert!(read.eq(expected).expect("could not compare"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn field_takes_its_type_from_the_default(_pyshinqlx_setup: ()) {
        let value = c"2.5";
        let mut raw_cvar = CVarBuilder::default()
            .string(value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_value",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class declared_plugin(shinqlx.Plugin):\n    value = shinqlx.Cvar('qlx_value', 1.5)",
                    );

                    let read = plugin_class
                        .call0()
                        .and_then(|instance| instance.getattr(intern!(py, "value")))
                        .and_then(|value| value.extract::<f64>());

                    assert!(read.is_ok_and(|value| (value - 2.5).abs() < f64::EPSILON));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn field_is_read_only(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let plugin_class = plugin_class(
                py,
                "class declared_plugin(shinqlx.Plugin):\n    value = shinqlx.Cvar('qlx_value', 1)",
            );
            let instance = plugin_class.call0().expect("could not create plugin");

            let result = instance.setattr(intern!(py, "value"), 3);

            assert!(result.is_err_and(|error| error.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn field_on_the_class_is_the_descriptor(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let plugin_class = plugin_class(
                py,
                "class declared_plugin(shinqlx.Plugin):\n    value = shinqlx.Cvar('qlx_value', 1)",
            );

            assert!(
                plugin_class
                    .getattr(intern!(py, "value"))
                    .is_ok_and(|field| field.get_type().name().is_ok_and(|name| name == "Cvar"))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn inherited_declarations_are_registered_for_the_subclass(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "qlx_base", |_| None, 1..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("qlx_base"),
                        predicate::eq("1"),
                        predicate::eq(Some(0)),
                    )
                    .times(1..);
            })
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class base_plugin(shinqlx.Plugin):\n    base = shinqlx.Cvar('qlx_base', 1)\n\nclass declared_plugin(base_plugin):\n    pass",
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                    assert!(plugin_class.hasattr(intern!(py, "base")).unwrap_or(false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_assigned_in_the_class_body_is_created_and_readable(_pyshinqlx_setup: ()) {
        let value = c"12";
        let mut raw_cvar = CVarBuilder::default()
            .string(value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_count",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let plugin_class = plugin_class(
                        py,
                        "class declared_plugin(shinqlx.Plugin):\n    count = shinqlx.Cvar('qlx_count', 3)",
                    );

                    assert!(register_declared_cvars(&plugin_class).is_ok());
                    let read = plugin_class
                        .call0()
                        .and_then(|instance| instance.getattr(intern!(py, "count")))
                        .and_then(|value| value.extract::<i32>());
                    assert_eq!(read.expect("could not read the cvar"), 12);
                });
            });
    }

    #[rstest]
    #[case("shinqlx.Cvar('')")]
    #[case("shinqlx.Cvar('qlx_x', 1, minimum=0)")]
    #[case("shinqlx.Cvar('qlx_x', 1, type=dict)")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn invalid_cvar_declarations_are_rejected(_pyshinqlx_setup: (), #[case] declaration: &str) {
        Python::attach(|py| {
            let result = plugin_module(
                py,
                &format!("class declared_plugin(shinqlx.Plugin):\n    value = {declaration}"),
            );

            assert!(result.is_err_and(|error| error.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_assigned_in_the_class_body_is_read_only(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let plugin_class = plugin_class(
                py,
                "class declared_plugin(shinqlx.Plugin):\n    value = shinqlx.Cvar('qlx_value', 1)",
            );
            let instance = plugin_class.call0().expect("could not create plugin");

            let result = instance.setattr(intern!(py, "value"), 3);

            assert!(result.is_err_and(|error| error.is_instance_of::<PyAttributeError>(py)));
        });
    }
}
