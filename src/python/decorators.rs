use core::hint::cold_path;

use pyo3::{
    PyTraverseError,
    exceptions::{PyAttributeError, PyTypeError, PyValueError},
    gc::PyVisit,
    intern,
    prelude::*,
    types::{
        PyBool, PyCFunction, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple, PyType,
    },
};

use super::{
    PythonReturnCodes,
    commands::CommandPriorities,
    deferred::{Deferral, DeferredCall},
    game::{Game, GameMethods},
    game_enums::GameState,
    player::Player,
    plugin::{CvarType, Plugin, PluginMethods, read_cvar_as},
};

const HOOKS_MARKER: &str = "_shinqlx_hooks";
const COMMANDS_MARKER: &str = "_shinqlx_commands";
const GAME_STATES: [&str; 3] = ["warmup", "countdown", "in_progress"];
const ARGS_OPTION: &str = "args";
const MIN_ARGS_OPTION: &str = "min_args";
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
            inspect.call_method1(
                intern!(py, "signature"),
                (dispatcher.getattr(intern!(py, "dispatch"))?,),
            )
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
///     ``prefix`` and ``usage``, and the following two.
/// :param args: The types of the arguments of the command, a sequence of ``int``, ``float``, ``str``
///     and :class:`Player`. A player is found by name or id. The handler then takes the parsed
///     arguments after ``channel``. If there are fewer arguments than ``min_args``, or an argument
///     cannot be converted or no player is found, the handler is not called and the command returns
///     :data:`RET_USAGE`. Further arguments are ignored and remain in ``msg``. Without ``usage``,
///     the usage is made from the names of the parameters of the handler.
/// :param min_args: The number of arguments that have to be given. Defaults to all of ``args``.
#[pyfunction]
#[pyo3(
    signature = (name, **options),
    text_signature = "(name, *, permission = 0, channels = None, exclude_channels = None, priority = PRI_NORMAL, client_cmd_pass = False, client_cmd_perm = 0, prefix = True, usage = \"\", args = None, min_args = None)"
)]
fn command<'py>(
    py: Python<'py>,
    name: Bound<'py, PyAny>,
    options: Option<Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyCFunction>> {
    let name = name.unbind();
    let options = options.map_or_else(|| Ok(PyDict::new(py)), |options| options.copy())?;
    let command_arguments = CommandArguments::from_options(&options)?;
    let options = options.unbind();

    PyCFunction::new_closure(
        py,
        Some(c"command_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let handler = only_handler(arguments)?;
            let py = handler.py();
            match &command_arguments {
                Some(command_arguments) => command_arguments.check_accepted_by(&handler, 1)?,
                None => check_accepts(&handler, &COMMAND_PARAMETERS, "a command handler", 1)?,
            }

            let entry = (name.bind(py), options.bind(py)).into_pyobject(py)?;
            mark(&handler, COMMANDS_MARKER, entry.into_any())?;
            Ok::<_, PyErr>(handler.unbind())
        },
    )
}

/// Calls the decorated method only while a game is running, and optionally only in the given
/// states. Otherwise the call is skipped and returns ``None``, so a skipped hook never stops an
/// event for other plugins. A skipped command replies with ``message`` if there is one. It works
/// with :func:`hook` and :func:`command` in any order, and just as well with a method that the
/// plugin registers itself with :meth:`Plugin.add_hook` or :meth:`Plugin.add_command`.
///
/// :param state: The game state, a member of :class:`GameStates` or one of the strings ``"warmup"``,
///     ``"countdown"`` and ``"in_progress"``, or several of them in a list, tuple or set. Without a
///     state, any running game is enough.
/// :param message: The reply to a skipped command. It is only sent for commands, whose handler is
///     called with the message as a list. Hooks never reply.
#[pyfunction]
#[pyo3(
    signature = (state = None, message = None),
    text_signature = "(state=None, message=None)"
)]
fn game<'py>(
    py: Python<'py>,
    state: Option<Bound<'py, PyAny>>,
    message: Option<String>,
) -> PyResult<Bound<'py, PyCFunction>> {
    let guard = GameGuard {
        states: state.as_ref().map(game_state_names).transpose()?,
        message,
    };

    PyCFunction::new_closure(
        py,
        Some(c"game_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let handler = only_handler(arguments)?;
            let py = handler.py();
            if handler.is_instance_of::<GameGuardedMethod>() {
                cold_path();
                return Err(PyTypeError::new_err(
                    "a handler can only be decorated with game once",
                ));
            }

            let guarded = Bound::new(
                py,
                GameGuardedMethod {
                    function: handler.clone().unbind(),
                    guard: guard.clone(),
                },
            )?
            .into_any();
            py.import(intern!(py, "functools"))?
                .call_method1(intern!(py, "update_wrapper"), (&guarded, &handler))?;
            Ok::<_, PyErr>(guarded.unbind())
        },
    )
}

fn game_state_names(state: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let is_single = |item: &Bound<'_, PyAny>| {
        item.is_instance_of::<PyString>() || item.is_instance_of::<GameState>()
    };

    let items = if is_single(state) {
        vec![state.clone()]
    } else {
        state.try_iter()?.collect::<PyResult<Vec<_>>>()?
    };
    if items.is_empty() {
        cold_path();
        return Err(PyValueError::new_err("at least one game state is needed"));
    }

    items
        .iter()
        .map(|item| {
            if !is_single(item) {
                cold_path();
                return Err(PyTypeError::new_err(
                    "game states have to be GameStates members or strings",
                ));
            }

            let name = item.str()?.to_string();
            if GAME_STATES.contains(&name.as_str()) {
                return Ok(name);
            }
            cold_path();
            Err(PyValueError::new_err(format!(
                "unknown game state '{name}', it has to be one of {}",
                GAME_STATES.join(", ")
            )))
        })
        .collect()
}

/// The check of the ``game`` decorator.
#[derive(Clone)]
struct GameGuard {
    states: Option<Vec<String>>,
    message: Option<String>,
}

impl GameGuard {
    fn permits(&self, py: Python<'_>) -> bool {
        let Ok(game) = Game::py_new(py, true).and_then(|game| Bound::new(py, game)) else {
            return false;
        };

        self.states
            .as_ref()
            .is_none_or(|states| game.get_state().is_ok_and(|state| states.contains(&state)))
    }

    /// Replies to a command with the message. The handler of a command gets the message as a list,
    /// unlike the handlers of hooks, so only commands are recognized.
    fn reject(&self, arguments: &Bound<'_, PyTuple>) -> PyResult<()> {
        let Some(message) = &self.message else {
            return Ok(());
        };
        if arguments.len() < 3 || !arguments.get_item(1)?.is_instance_of::<PyList>() {
            return Ok(());
        }

        arguments
            .get_item(2)?
            .call_method1(intern!(arguments.py(), "reply"), (message,))
            .map(|_| ())
    }
}

/// A method that ``game`` guards. It binds to the plugin like a function does, and the bound method
/// checks the game before it calls the method.
#[pyclass(module = "_shinqlx", name = "GameGuardedMethod", dict, frozen)]
struct GameGuardedMethod {
    function: Py<PyAny>,
    guard: GameGuard,
}

#[pymethods]
impl GameGuardedMethod {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.function)
    }

    fn __get__<'py>(
        slf: &Bound<'py, Self>,
        instance: &Bound<'py, PyAny>,
        owner: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if instance.is_none() {
            return Ok(slf.clone().into_any());
        }

        let method = slf
            .get()
            .function
            .bind(slf.py())
            .call_method1(intern!(slf.py(), "__get__"), (instance, owner))?;
        DecoratedHandler::wrap(&method, Some(slf.get().guard.clone()), None)
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        if !self.guard.permits(py) {
            self.guard.reject(args)?;
            return Ok(py.None());
        }

        self.function.bind(py).call(args, kwargs).map(Bound::unbind)
    }
}

#[derive(Clone, Copy)]
enum ArgumentType {
    Int,
    Float,
    Str,
    Player,
}

impl ArgumentType {
    fn from_type(declared: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = declared.py();
        [
            (py.get_type::<PyInt>(), Self::Int),
            (py.get_type::<PyFloat>(), Self::Float),
            (py.get_type::<PyString>(), Self::Str),
            (py.get_type::<Player>(), Self::Player),
        ]
        .into_iter()
        .find(|(known, _)| declared.is(known))
        .map(|(_, argument_type)| argument_type)
        .ok_or_else(|| {
            cold_path();
            PyValueError::new_err("the type of an argument has to be int, float, str or Player")
        })
    }

    fn parse<'py>(self, py: Python<'py>, word: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(match self {
            Self::Int => word
                .parse::<i64>()
                .ok()
                .map(|value| PyInt::new(py, value).into_any()),
            Self::Float => word
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| PyFloat::new(py, value).into_any()),
            Self::Str => Some(PyString::new(py, word).into_any()),
            Self::Player => Some(
                py.get_type::<Plugin>()
                    .call_method1(intern!(py, "player"), (word,))?,
            )
            .filter(|player| !player.is_none()),
        })
    }
}

fn positional_parameter_names(handler: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let py = handler.py();
    let mut names = vec![];
    for parameter in py
        .import(intern!(py, "inspect"))?
        .call_method1(intern!(py, "signature"), (handler,))?
        .getattr(intern!(py, "parameters"))?
        .call_method0(intern!(py, "values"))?
        .try_iter()?
    {
        let parameter = parameter?;
        let kind = parameter
            .getattr(intern!(py, "kind"))?
            .getattr(intern!(py, "name"))?
            .to_string();
        if kind == "POSITIONAL_ONLY" || kind == "POSITIONAL_OR_KEYWORD" {
            names.push(parameter.getattr(intern!(py, "name"))?.to_string());
        }
    }
    Ok(names)
}

/// The ``args`` and ``min_args`` options of the ``command`` decorator.
struct CommandArguments {
    types: Vec<ArgumentType>,
    min_args: usize,
}

impl CommandArguments {
    fn from_options(options: &Bound<'_, PyDict>) -> PyResult<Option<Self>> {
        let min_args = options.get_item(MIN_ARGS_OPTION)?;
        let Some(declared) = options.get_item(ARGS_OPTION)? else {
            if min_args.is_some() {
                cold_path();
                return Err(PyTypeError::new_err("min_args needs args"));
            }
            return Ok(None);
        };
        if declared.is_instance_of::<PyString>() {
            cold_path();
            return Err(PyTypeError::new_err(
                "args has to be a sequence of types, not a string",
            ));
        }

        let types = declared
            .try_iter()?
            .map(|declared_type| declared_type.and_then(|value| ArgumentType::from_type(&value)))
            .collect::<PyResult<Vec<_>>>()?;
        let min_args = min_args.map_or(Ok(types.len()), |value| value.extract::<usize>())?;
        if min_args > types.len() {
            cold_path();
            return Err(PyValueError::new_err(
                "min_args cannot be larger than the number of args",
            ));
        }

        Ok(Some(Self { types, min_args }))
    }

    fn check_accepted_by(&self, handler: &Bound<'_, PyAny>, receivers: usize) -> PyResult<()> {
        let accepted = |argument_count: usize| {
            let names = COMMAND_PARAMETERS
                .iter()
                .map(ToString::to_string)
                .chain((1..=argument_count).map(|index| format!("arg{index}")))
                .collect::<Vec<_>>();
            check_accepts(handler, &names, "a command handler", receivers)
        };

        accepted(self.min_args)?;
        if self.types.len() != self.min_args {
            accepted(self.types.len())?;
        }
        Ok(())
    }

    /// The usage from the names of the parameters that take the arguments: required ones in angle
    /// brackets, optional ones in square brackets.
    fn usage(&self, handler: &Bound<'_, PyAny>) -> String {
        let names = positional_parameter_names(handler).unwrap_or_default();

        (0..self.types.len())
            .map(|index| {
                let name = names
                    .get(COMMAND_PARAMETERS.len() + index)
                    .map_or_else(|| format!("arg{}", index + 1), Clone::clone);
                if index < self.min_args {
                    format!("<{name}>")
                } else {
                    format!("[{name}]")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse<'py>(
        &self,
        py: Python<'py>,
        message: &Bound<'py, PyAny>,
    ) -> PyResult<Option<Vec<Bound<'py, PyAny>>>> {
        let words = message.extract::<Vec<String>>()?;
        let words = words.get(1..).unwrap_or_default();
        if words.len() < self.min_args {
            return Ok(None);
        }

        let mut parsed = Vec::with_capacity(self.types.len());
        for (argument_type, word) in self.types.iter().zip(words) {
            let Some(value) = argument_type.parse(py, word)? else {
                return Ok(None);
            };
            parsed.push(value);
        }
        Ok(Some(parsed))
    }

    fn call(
        &self,
        handler: &Bound<'_, PyAny>,
        arguments: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let py = handler.py();
        let Ok(message) = arguments.get_item(1) else {
            return handler.call1(arguments).map(Bound::unbind);
        };

        match self.parse(py, &message)? {
            None => Ok(PyInt::new(py, PythonReturnCodes::RET_USAGE as i32)
                .into_any()
                .unbind()),
            Some(parsed) => {
                let all_arguments = arguments.iter().chain(parsed).collect::<Vec<_>>();
                handler
                    .call1(PyTuple::new(py, all_arguments)?)
                    .map(Bound::unbind)
            }
        }
    }
}

/// Wraps a handler of a plugin with the game check and the parsing of the arguments, so the
/// handler itself does not need to do either. Keeps the name and the documentation of the handler.
#[pyclass(module = "_shinqlx", name = "DecoratedHandler", dict, frozen)]
struct DecoratedHandler {
    handler: Py<PyAny>,
    guard: Option<GameGuard>,
    arguments: Option<CommandArguments>,
}

impl DecoratedHandler {
    fn wrap<'py>(
        handler: &Bound<'py, PyAny>,
        guard: Option<GameGuard>,
        arguments: Option<CommandArguments>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = handler.py();
        let wrapper = Bound::new(
            py,
            Self {
                handler: handler.clone().unbind(),
                guard,
                arguments,
            },
        )?
        .into_any();
        py.import(intern!(py, "functools"))?
            .call_method1(intern!(py, "update_wrapper"), (&wrapper, handler))?;
        Ok(wrapper)
    }
}

#[pymethods]
impl DecoratedHandler {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.handler)
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        if let Some(guard) = &self.guard
            && !guard.permits(py)
        {
            guard.reject(args)?;
            return Ok(py.None());
        }

        let handler = self.handler.bind(py);
        let Some(arguments) = &self.arguments else {
            return handler.call(args, kwargs).map(Bound::unbind);
        };

        // the game is checked before the arguments, so a command in the wrong state is not parsed
        if let Ok(guarded) = handler.cast::<Self>()
            && let Some(guard) = &guarded.get().guard
            && !guard.permits(py)
        {
            guard.reject(args)?;
            return Ok(py.None());
        }
        arguments.call(handler, args)
    }
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
    value_type: CvarType,
    flags: i32,
    minimum: Option<Py<PyAny>>,
    maximum: Option<Py<PyAny>>,
}

#[pymethods]
impl Cvar {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.minimum)?;
        visit.call(&self.maximum)
    }

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

        let value_type = cvar_value_type(py, r#type, default.as_ref())?;
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
        read_cvar_as(slf.py(), &declaration.name, declaration.value_type)
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
) -> PyResult<CvarType> {
    if let Some(declared) = declared {
        let declared_type = declared
            .is_instance_of::<PyType>()
            .then(|| {
                declared
                    .getattr(intern!(py, "__name__"))
                    .and_then(|name| name.extract::<String>())
                    .ok()
            })
            .flatten()
            .and_then(|name| CvarType::from_name(&name));
        return declared_type.ok_or_else(|| {
            cold_path();
            PyValueError::new_err(format!("type has to be one of {}", CVAR_TYPES.join(", ")))
        });
    }

    Ok(match default {
        Some(value) if value.is_instance_of::<PyBool>() => CvarType::Bool,
        Some(value) if value.is_instance_of::<PyInt>() => CvarType::Int,
        Some(value) if value.is_instance_of::<PyFloat>() => CvarType::Float,
        Some(value) if value.is_instance_of::<PyList>() => CvarType::List,
        Some(value) if value.is_instance_of::<PyTuple>() => CvarType::Tuple,
        Some(value) if value.is_instance_of::<PySet>() => CvarType::Set,
        _ => CvarType::Str,
    })
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
    m.add_function(wrap_pyfunction!(game, m)?)?;
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
        for (_, options) in &commands {
            match CommandArguments::from_options(options)? {
                Some(arguments) => arguments.check_accepted_by(&handler, 0)?,
                None => check_accepts(&handler, &COMMAND_PARAMETERS, "a command handler", 0)?,
            }
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
            let options = options.copy()?;
            let arguments = CommandArguments::from_options(&options)?;
            for option in [ARGS_OPTION, MIN_ARGS_OPTION] {
                if options.contains(option)? {
                    options.del_item(option)?;
                }
            }
            if let Some(arguments) = &arguments
                && !options.contains(intern!(py, "usage"))?
            {
                options.set_item(intern!(py, "usage"), arguments.usage(&handler))?;
            }

            let command_handler = if arguments.is_some() {
                DecoratedHandler::wrap(&handler, None, arguments)?
            } else {
                handler.clone()
            };
            plugin.call_method(
                intern!(py, "add_command"),
                (command_name, &command_handler),
                Some(&options),
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

/// Defer a function call by a number of seconds or by a number of game frames.
///
/// Exactly one of *seconds* and *frames* has to be given. One frame is the same as
/// :func:`next_frame`, and the seconds are the same as in :func:`delay`.
#[pyfunction]
#[pyo3(signature = (*, seconds=None, frames=None))]
pub(super) fn later<'py>(
    py: Python<'py>,
    seconds: Option<f64>,
    frames: Option<i64>,
) -> PyResult<Bound<'py, PyAny>> {
    let deferral = match (seconds, frames) {
        (Some(_), Some(_)) => {
            cold_path();
            return Err(PyTypeError::new_err(
                "later() takes either seconds or frames, not both",
            ));
        }
        (None, None) => {
            cold_path();
            return Err(PyTypeError::new_err(
                "later() needs either seconds or frames",
            ));
        }
        (Some(seconds), None) if seconds.is_finite() && seconds >= 0.0 => Deferral::Delay(seconds),
        (Some(_), None) => {
            cold_path();
            return Err(PyValueError::new_err(
                "seconds has to be a finite number that is not negative",
            ));
        }
        (None, Some(1)) => Deferral::NextFrame,
        (None, Some(frames)) => Deferral::Frames(
            u32::try_from(frames)
                .ok()
                .filter(|&frames| frames >= 1)
                .ok_or_else(|| {
                    cold_path();
                    PyValueError::new_err("frames has to be a positive integer")
                })?,
        ),
    };

    PyCFunction::new_closure(
        py,
        Some(c"later_decorator"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let func = only_handler(arguments)?;
            if !func.is_callable() {
                cold_path();
                return Err(PyValueError::new_err("'func' must be callable"));
            }

            DeferredCall::wrap(&func, deferral).map(Bound::unbind)
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

    use pyo3::{
        intern,
        prelude::*,
        types::{PyDict, PyList},
    };
    use rstest::rstest;

    use super::register_decorated_handlers;
    use crate::{
        engine::prelude::{CS_SERVERINFO, CVar, CVarBuilder, cvar_t},
        prelude::*,
        python::{
            COMMANDS, EVENT_DISPATCHERS,
            commands::CommandInvoker,
            events::{
                EventDispatcherManager, EventDispatcherManagerMethods, TeamSwitchAttemptDispatcher,
            },
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::recording_channel,
        },
    };

    fn decorated_plugin<'py>(py: Python<'py>, class_body: &str) -> Bound<'py, PyAny> {
        let code = alloc::ffi::CString::new(format!(
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
        with_game_state(None, test);
    }

    fn with_game_state<F>(game_state: Option<&str>, test: F)
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
        if let Some(game_state) = game_state {
            let serverinfo = if game_state.is_empty() {
                String::new()
            } else {
                format!(r"\g_gameState\{game_state}")
            };
            engine = engine.with_get_configstring(CS_SERVERINFO as u16, serverinfo, 0..);
        }

        engine.run(|| {
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

    const COMMAND_WITH_ARGUMENTS: &str = "    def __init__(self):\n        super().__init__()\n        \
         self.calls = []\n\n    \
         @shinqlx.command('foo', args=(int, str, float), min_args=1)\n    \
         def cmd_foo(self, player, msg, channel, n, name=None, ratio=None):\n        \
         self.calls.append((n, name, ratio, msg))";

    fn run_checks(py: Python<'_>, plugin: &Bound<'_, PyAny>, checks: &str) {
        let locals = PyDict::new(py);
        locals
            .set_item("plugin", plugin)
            .expect("could not set plugin");
        locals
            .set_item("shinqlx", py.import("shinqlx").expect("no shinqlx"))
            .expect("could not set shinqlx");
        locals
            .set_item("channel", recording_channel(py))
            .expect("could not set channel");
        let code = alloc::ffi::CString::new(checks).expect("this should not happen");
        py.run(&code, Some(&locals), Some(&locals))
            .expect("checks failed");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn command_arguments_are_parsed_and_passed_after_the_channel(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(py, COMMAND_WITH_ARGUMENTS);
            register_decorated_handlers(&plugin).expect("could not register");

            run_checks(
                py,
                &plugin,
                r#"
command = plugin.commands[0]
assert command.handler.__name__ == "cmd_foo"
msg = ["!foo", "5", "bob", "2.5", "extra"]
assert command.handler(None, msg, None) is None
assert plugin.calls == [(5, "bob", 2.5, msg)]
assert command.handler(None, ["!foo", "7"], None) is None
assert plugin.calls[-1][:3] == (7, None, None)
"#,
            );
        });
    }

    #[rstest]
    #[case("[\"!foo\"]")]
    #[case("[\"!foo\", \"seven\"]")]
    #[case("[\"!foo\", \"7\", \"bob\", \"nan\"]")]
    #[case("[\"!foo\", \"7\", \"bob\", \"inf\"]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn invalid_command_arguments_return_usage_without_calling_the_handler(
        _pyshinqlx_setup: (),
        #[case] msg: &str,
    ) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(py, COMMAND_WITH_ARGUMENTS);
            register_decorated_handlers(&plugin).expect("could not register");

            run_checks(
                py,
                &plugin,
                &format!(
                    "assert plugin.commands[0].handler(None, {msg}, None) == shinqlx.RET_USAGE\n\
                     assert plugin.calls == []"
                ),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn usage_is_made_from_the_parameter_names_unless_given(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.command('foo', args=(int, str), min_args=1)\n    \
                 def cmd_foo(self, player, msg, channel, count, *rest): pass\n\n    \
                 @shinqlx.command('bar', args=(int,), usage='<how many>')\n    \
                 def cmd_bar(self, player, msg, channel, count): pass\n\n    \
                 @shinqlx.command('baz')\n    \
                 def cmd_baz(self, player, msg, channel): pass",
            );
            register_decorated_handlers(&plugin).expect("could not register");

            run_checks(
                py,
                &plugin,
                r#"
usages = {command.name[0]: command.usage for command in plugin.commands}
assert usages == {"foo": "<count> [arg2]", "bar": "<how many>", "baz": ""}, usages
"#,
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn command_options_that_are_not_add_command_arguments_are_not_passed_on(_pyshinqlx_setup: ()) {
        with_dispatchers_and_commands(|py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.command('foo', args=(int,), min_args=0, permission=2)\n    \
                 def cmd_foo(self, player, msg, channel, count=0): pass",
            );

            assert!(register_decorated_handlers(&plugin).is_ok());
            run_checks(py, &plugin, "assert plugin.commands[0].permission == 2");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn invalid_declarations_are_rejected_when_decorating(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                c"
import shinqlx


def rejected(error, declare):
    try:
        declare()
    except error:
        return True
    return False


def handler(self, player, msg, channel): pass


results = [
    rejected(ValueError, lambda: shinqlx.command('x', args=(dict,))),
    rejected(TypeError, lambda: shinqlx.command('x', args='int')),
    rejected(TypeError, lambda: shinqlx.command('x', min_args=1)),
    rejected(ValueError, lambda: shinqlx.command('x', args=(int,), min_args=2)),
    rejected(TypeError, lambda: shinqlx.command('x', args=(int,))(handler)),
    rejected(ValueError, lambda: shinqlx.game(state='no_such_state')),
    rejected(ValueError, lambda: shinqlx.game(state=())),
    rejected(TypeError, lambda: shinqlx.game(state=3)),
    rejected(TypeError, lambda: shinqlx.game()(shinqlx.game()(handler))),
]
",
                c"",
                c"",
            )
            .expect("could not run the declarations");

            let results = module
                .getattr(intern!(py, "results"))
                .and_then(|results| results.extract::<Vec<bool>>())
                .expect("could not get results");
            assert!(results.iter().all(|rejected| *rejected), "{results:?}");
        });
    }

    #[rstest]
    #[case("IN_PROGRESS", "'hooked'")]
    #[case("PRE_GAME", "None")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_guards_methods_that_the_plugin_registers_itself(
        _pyshinqlx_setup: (),
        #[case] game_state: &str,
        #[case] expected: &str,
    ) {
        with_game_state(Some(game_state), |py| {
            let plugin = decorated_plugin(
                py,
                "    def __init__(self):\n        super().__init__()\n        \
                 self.add_hook('team_switch_attempt', self.on_switch)\n\n    \
                 @shinqlx.game(state='in_progress')\n    \
                 def on_switch(self, player, old_team, new_team):\n        \
                 return 'hooked'",
            );

            run_checks(
                py,
                &plugin,
                &format!(
                    "assert len(plugin.hooks) == 1\n\
                     assert plugin.hooks[0][1](None, 'a', 'b') == {expected}\n\
                     assert plugin.on_switch(None, 'a', 'b') == {expected}\n\
                     assert type(plugin).on_switch(plugin, None, 'a', 'b') == {expected}\n\
                     assert plugin.on_switch.__name__ == 'on_switch'"
                ),
            );
        });
    }

    #[rstest]
    #[case("IN_PROGRESS", "['ran']", "[]")]
    #[case("PRE_GAME", "[]", "['not now']")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_guards_commands_that_the_plugin_registers_itself(
        _pyshinqlx_setup: (),
        #[case] game_state: &str,
        #[case] expected_calls: &str,
        #[case] expected_replies: &str,
    ) {
        with_game_state(Some(game_state), |py| {
            let plugin = decorated_plugin(
                py,
                "    def __init__(self):\n        super().__init__()\n        \
                 self.calls = []\n        \
                 self.add_command('foo', self.cmd_foo)\n\n    \
                 @shinqlx.game(state='in_progress', message='not now')\n    \
                 def cmd_foo(self, player, msg, channel):\n        \
                 self.calls.append('ran')",
            );

            run_checks(
                py,
                &plugin,
                &format!(
                    "assert len(plugin.commands) == 1\n\
                     assert plugin.commands[0].handler(None, ['!foo'], channel) is None\n\
                     assert plugin.calls == {expected_calls}, plugin.calls\n\
                     assert channel.replies == {expected_replies}, channel.replies"
                ),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_without_hook_or_command_is_a_plain_guard(_pyshinqlx_setup: ()) {
        with_game_state(Some(""), |py| {
            let plugin = decorated_plugin(
                py,
                "    @shinqlx.game()\n    def handler(self): return 'ran'",
            );

            assert!(register_decorated_handlers(&plugin).is_ok());
            assert_eq!(registered(&plugin, "hooks"), 0);
            run_checks(py, &plugin, "assert plugin.handler() is None");
        });
    }

    const GUARDED_HANDLERS: &str = "    def __init__(self):\n        super().__init__()\n        \
         self.calls = []\n\n    \
         @shinqlx.hook('team_switch_attempt')\n    \
         @shinqlx.game(state='in_progress', message='ignored for hooks')\n    \
         def on_team_switch_attempt(self, player, old_team, new_team):\n        \
         self.calls.append('hook')\n        return 'hooked'\n\n    \
         @shinqlx.command('foo', args=(int,))\n    \
         @shinqlx.game(state=(shinqlx.GameStates.WARMUP, 'countdown'), message='not now')\n    \
         def cmd_foo(self, player, msg, channel, count):\n        \
         self.calls.append(count)\n\n    \
         @shinqlx.command('bar')\n    \
         @shinqlx.game()\n    \
         def cmd_bar(self, player, msg, channel):\n        \
         self.calls.append('bar')";

    const GUARD_CHECKS: &str = r#"
hook = plugin.hooks[0][1]
handlers = {command.name[0]: command.handler for command in plugin.commands}
foo, bar = handlers["foo"], handlers["bar"]
assert hook.__name__ == "on_team_switch_attempt"
"#;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn skipped_handlers_return_none_and_commands_reply_when_no_game_is_running(
        _pyshinqlx_setup: (),
    ) {
        with_game_state(Some(""), |py| {
            let plugin = decorated_plugin(py, GUARDED_HANDLERS);
            register_decorated_handlers(&plugin).expect("could not register");

            run_checks(
                py,
                &plugin,
                &format!(
                    "{GUARD_CHECKS}\n\
                     assert hook(None, 'red', 'blue') is None\n\
                     assert foo(None, ['!foo', '3'], channel) is None\n\
                     assert bar(None, ['!bar'], channel) is None\n\
                     assert plugin.calls == []\n\
                     assert channel.replies == ['not now']"
                ),
            );
        });
    }

    #[rstest]
    #[case("IN_PROGRESS", true, false)]
    #[case("PRE_GAME", false, true)]
    #[case("COUNT_DOWN", false, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handlers_run_only_in_the_declared_game_states(
        _pyshinqlx_setup: (),
        #[case] game_state: &str,
        #[case] hook_runs: bool,
        #[case] command_runs: bool,
    ) {
        with_game_state(Some(game_state), |py| {
            let plugin = decorated_plugin(py, GUARDED_HANDLERS);
            register_decorated_handlers(&plugin).expect("could not register");

            run_checks(
                py,
                &plugin,
                &format!(
                    "{GUARD_CHECKS}\n\
                     assert hook(None, 'red', 'blue') == ('hooked' if {} else None)\n\
                     foo(None, ['!foo', '3'], channel)\n\
                     assert bar(None, ['!bar'], channel) is None\n\
                     assert plugin.calls == ({}) + ['bar'], plugin.calls\n\
                     assert channel.replies == ({}), channel.replies",
                    if hook_runs { "True" } else { "False" },
                    match (hook_runs, command_runs) {
                        (true, _) => "['hook']",
                        (_, true) => "[3]",
                        _ => "[]",
                    },
                    if command_runs { "[]" } else { "['not now']" },
                ),
            );
        });
    }
}

#[cfg(test)]
mod decorator_signature_tests {
    use alloc::ffi::CString;
    use core::borrow::BorrowMut;

    use pyo3::{exceptions::PyTypeError, intern, prelude::*, types::PyDict};
    use rstest::rstest;

    use super::register_decorated_handlers;
    use crate::{
        engine::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
        python::{
            COMMANDS, commands::CommandInvoker, pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::RestoredAttribute,
        },
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
        prelude::*,
        python::{pyshinqlx_setup_fixture::*, pyshinqlx_test_support::run_all_frame_tasks},
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
mod later_tests {
    use alloc::ffi::CString;

    use pyo3::prelude::*;
    use rstest::rstest;

    use crate::prelude::*;

    const RUN_FRAME: &str = r#"
def run_frame():
    pending = []
    while not shinqlx.next_frame_tasks.empty():
        pending.append(shinqlx.next_frame_tasks.get(block=False))
    for func, args, kwargs in pending:
        func(*args, **kwargs)
"#;

    fn run(py: Python<'_>, code: &str) -> PyResult<()> {
        let code = CString::new(format!("import shinqlx\n{RUN_FRAME}\n{code}"))
            .expect("this should not happen");
        py.run(&code, None, None)
    }

    #[rstest]
    #[case::both("seconds=1, frames=1", "TypeError")]
    #[case::neither("", "TypeError")]
    #[case::positional("5", "TypeError")]
    #[case::negative_seconds("seconds=-1", "ValueError")]
    #[case::nan_seconds("seconds=float('nan')", "ValueError")]
    #[case::infinite_seconds("seconds=float('inf')", "ValueError")]
    #[case::zero_frames("frames=0", "ValueError")]
    #[case::negative_frames("frames=-3", "ValueError")]
    #[case::too_many_frames("frames=2**40", "ValueError")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_rejects_invalid_arguments(
        #[case] arguments: &str,
        #[case] exception: &str,
        #[from(crate::python::pyshinqlx_setup_fixture::pyshinqlx_setup)] _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                &format!(
                    "try:\n    shinqlx.later({arguments})\nexcept {exception}:\n    pass\nelse:\n    raise AssertionError('no {exception}')"
                ),
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_rejects_a_function_that_is_not_callable(
        #[from(crate::python::pyshinqlx_setup_fixture::pyshinqlx_setup)] _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                "try:\n    shinqlx.later(frames=2)(5)\nexcept ValueError:\n    pass\nelse:\n    raise AssertionError('no ValueError')",
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_with_one_frame_runs_in_the_next_frame(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                r#"
calls = []

@shinqlx.later(frames=1)
def remember(value, factor=1):
    calls.append(value * factor)

assert remember(3, factor=2) is None
assert calls == []
assert shinqlx.next_frame_tasks.qsize() == 1
run_frame()
assert calls == [6]
assert shinqlx.next_frame_tasks.qsize() == 0
"#,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_with_several_frames_runs_in_the_last_of_them(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                r#"
calls = []

@shinqlx.later(frames=3)
def remember(value, factor=1):
    calls.append(value * factor)

assert remember(3, factor=2) is None
seen = []
for _ in range(5):
    run_frame()
    seen.append(list(calls))

assert seen == [[], [], [6], [6], [6]], seen
assert shinqlx.next_frame_tasks.qsize() == 0
"#,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_with_several_frames_counts_every_call_on_its_own(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                r#"
calls = []

@shinqlx.later(frames=2)
def remember(value):
    calls.append(value)

remember(1)
run_frame()
remember(2)
seen = []
for _ in range(3):
    run_frame()
    seen.append(list(calls))

assert seen == [[1], [1, 2], [1, 2]], seen
"#,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_with_several_frames_survives_a_failing_function(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                r#"
@shinqlx.later(frames=2)
def failing():
    raise RuntimeError("expected")

failing()
run_frame()
try:
    run_frame()
except RuntimeError:
    pass
else:
    raise AssertionError("no RuntimeError")
assert shinqlx.next_frame_tasks.qsize() == 0
"#,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[case::fraction("0.0001")]
    #[case::zero("0")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_with_seconds_enters_the_function_into_the_frame_tasks(
        #[case] seconds: &str,
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                &format!(
                    r#"
calls = []

@shinqlx.later(seconds={seconds})
def remember(value):
    calls.append(value)

remember(4)
assert calls == []
assert len(shinqlx.frame_tasks.queue) == 1
assert shinqlx.next_frame_tasks.qsize() == 0
shinqlx.frame_tasks.run()
assert calls == [4]
"#
                ),
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn later_decorates_a_method_and_binds_its_instance(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        Python::attach(|py| {
            let result = run(
                py,
                r#"
class Sample:
    def __init__(self):
        self.calls = []

    @shinqlx.later(frames=2)
    def remember(self, value):
        """Remembers the value."""
        self.calls.append(value)

sample = Sample()
sample.remember(8)
run_frame()
assert sample.calls == []
run_frame()
assert sample.calls == [8]
assert Sample.remember.__name__ == "remember"
assert Sample.remember.__doc__ == "Remembers the value."
"#,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }
}

#[cfg(test)]
mod delay_tests {
    use pretty_assertions::assert_eq;
    use pyo3::{intern, prelude::*, types::PyBool};
    use rstest::rstest;

    use crate::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn delay_enqueues_function_call(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
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
    fn delay_with_float_delay(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
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
    fn delay_enqueues_function_call_for_class_method(
        #[from(crate::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
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

    use crate::{prelude::serial, python::pyshinqlx_setup_fixture::*};

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
        engine::prelude::{CVar, CVarBuilder, cvar_t},
        prelude::*,
        python::pyshinqlx_setup_fixture::pyshinqlx_setup,
    };

    fn plugin_module<'py>(py: Python<'py>, source: &str) -> PyResult<Bound<'py, PyModule>> {
        let code = alloc::ffi::CString::new(format!("import shinqlx\n\n{source}\n"))
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
        let value = alloc::ffi::CString::new(cvar_value).expect("this should not happen");
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
                            &alloc::ffi::CString::new(expected).expect("this should not happen"),
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
