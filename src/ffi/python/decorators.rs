use pyo3::{
    exceptions::{PyAttributeError, PyTypeError},
    intern,
    prelude::*,
    types::{PyCFunction, PyDict, PyTuple},
};

use super::{
    commands::CommandPriorities,
    plugin::{Plugin, PluginMethods},
};

const HOOKS_MARKER: &str = "_shinqlx_hooks";
const COMMANDS_MARKER: &str = "_shinqlx_commands";
const COMMAND_PARAMETERS: [&str; 3] = ["player", "msg", "channel"];

fn only_handler<'py>(arguments: &Bound<'py, PyTuple>) -> PyResult<Bound<'py, PyAny>> {
    match arguments.len() {
        1 => arguments.get_item(0),
        count => Err(PyTypeError::new_err(format!(
            "a decorator takes exactly one handler, but {count} were given"
        ))),
    }
}

fn dispatcher_parameters(py: Python<'_>, event: &str) -> Option<Vec<String>> {
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

/// Adds the `hook` and `command` decorators to the shinqlx module.
pub(crate) fn register_decorators(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(hook, m)?)?;
    m.add_function(wrap_pyfunction!(command, m)?)
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
