use pyo3::{intern, prelude::*, sync::PyOnceLock};

static DECORATORS: PyOnceLock<Py<PyModule>> = PyOnceLock::new();

fn decorators_module(py: Python<'_>) -> PyResult<&Bound<'_, PyModule>> {
    DECORATORS
        .get_or_try_init(py, || {
            PyModule::from_code(
                py,
                cr#"
def hook(event, priority=None):
    """Hooks the decorated method of a plugin to an event. The hook is registered
    automatically when the plugin is loaded, so the plugin does not need to call
    :meth:`Plugin.add_hook` itself.

    :param event: The name of the event to hook the method to.
    :param priority: The priority of the hook, one of the ``PRI_*`` constants.
        Defaults to :data:`PRI_NORMAL`.
    """

    def decorator(handler):
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
def handler(): pass
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
