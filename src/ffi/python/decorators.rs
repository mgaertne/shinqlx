use pyo3::{intern, prelude::*, sync::PyOnceLock};

static DECORATORS: PyOnceLock<Py<PyModule>> = PyOnceLock::new();

fn decorators_module(py: Python<'_>) -> PyResult<&Bound<'_, PyModule>> {
    DECORATORS
        .get_or_try_init(py, || {
            PyModule::from_code(
                py,
                cr#"
import inspect

import shinqlx

COMMAND_PARAMETERS = ("player", "msg", "channel")
POSITIONAL = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)


def _hook_parameters(event):
    """Names of the arguments the dispatcher of an event passes to its hooks, or ``None`` if
    the event or its arguments are not known."""
    try:
        signature = inspect.signature(shinqlx.EVENT_DISPATCHERS[event].dispatch)
    except (AttributeError, KeyError, TypeError, ValueError):
        return None

    parameters = signature.parameters.values()
    if any(parameter.kind is inspect.Parameter.VAR_POSITIONAL for parameter in parameters):
        return None
    return [parameter.name for parameter in parameters if parameter.kind in POSITIONAL]


def _check_accepts(handler, names, purpose, receivers):
    try:
        signature = inspect.signature(handler)
    except (TypeError, ValueError):
        return

    try:
        signature.bind(*([None] * (len(names) + receivers)))
    except TypeError as error:
        name = getattr(handler, "__qualname__", repr(handler))
        raise TypeError(
            f"{name} is {purpose} and has to accept ({', '.join(names)}): {error}"
        ) from None


def hook(event, priority=None):
    """Hooks the decorated method of a plugin to an event. The hook is registered
    automatically when the plugin is loaded, so the plugin does not need to call
    :meth:`Plugin.add_hook` itself.

    :param event: The name of the event to hook the method to.
    :param priority: The priority of the hook, one of the ``PRI_*`` constants.
        Defaults to :data:`PRI_NORMAL`.
    """

    def decorator(handler):
        names = _hook_parameters(event)
        if names is not None:
            _check_accepts(handler, names, f"hooked to '{event}'", 1)

        handler._shinqlx_hooks = (
            *getattr(handler, "_shinqlx_hooks", ()),
            (event, priority),
        )
        return handler

    return decorator


def command(name, **options):
    """Makes the decorated method of a plugin a command. The command is registered
    automatically when the plugin is loaded, so the plugin does not need to call
    :meth:`Plugin.add_command` itself.

    :param name: The name of the command, or a list of names.
    :param options: The optional arguments of :meth:`Plugin.add_command`, such as
        ``permission``, ``channels``, ``exclude_channels``, ``priority``,
        ``client_cmd_pass``, ``client_cmd_perm``, ``prefix`` and ``usage``.
    """

    def decorator(handler):
        _check_accepts(handler, COMMAND_PARAMETERS, "a command handler", 1)

        handler._shinqlx_commands = (
            *getattr(handler, "_shinqlx_commands", ()),
            (name, options),
        )
        return handler

    return decorator


def register_decorated_handlers(plugin):
    plugin_class = type(plugin)
    for attribute_name in dir(plugin_class):
        declared = getattr(plugin_class, attribute_name, None)
        hooks = getattr(declared, "_shinqlx_hooks", ())
        commands = getattr(declared, "_shinqlx_commands", ())
        if not hooks and not commands:
            continue

        handler = getattr(plugin, attribute_name)
        for event, _ in hooks:
            names = _hook_parameters(event)
            if names is not None:
                _check_accepts(handler, names, f"hooked to '{event}'", 0)
        if commands:
            _check_accepts(handler, COMMAND_PARAMETERS, "a command handler", 0)

        for event, priority in hooks:
            if priority is None:
                plugin.add_hook(event, handler)
            else:
                plugin.add_hook(event, handler, priority)
        for command_name, options in commands:
            plugin.add_command(command_name, handler, **options)
"#,
                c"_decorators.py",
                c"_decorators",
            )
            .map(Bound::unbind)
        })
        .map(|decorators| decorators.bind(py))
}

/// Adds the `hook` and `command` decorators to the shinqlx module.
pub(crate) fn register_decorators(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let decorators = decorators_module(m.py())?;
    m.add("hook", decorators.getattr(intern!(m.py(), "hook"))?)?;
    m.add("command", decorators.getattr(intern!(m.py(), "command"))?)
}

/// Registers the hooks and commands that were declared by decorating methods of the plugin.
pub(crate) fn register_decorated_handlers(plugin: &Bound<'_, PyAny>) -> PyResult<()> {
    decorators_module(plugin.py())?
        .call_method1(
            intern!(plugin.py(), "register_decorated_handlers"),
            (plugin,),
        )
        .map(|_| ())
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
