mod chat_log;
mod motd;
mod permission;
mod workshop;

use pyo3::{intern, prelude::*};

use super::{
    commands::CommandPriorities,
    plugin::{Plugin, PluginMethods},
};

type Constructor = for<'py> fn(Python<'py>) -> PyResult<Bound<'py, PyAny>>;

static NATIVE_PLUGINS: [(&str, Constructor); 4] = [
    ("log", chat_log::create),
    ("motd", motd::create),
    ("permission", permission::create),
    ("workshop", workshop::create),
];

/// Removes the color reset that `Player.name` appends, so that a reply which ends the name with a
/// color reset of its own does not repeat it.
fn without_trailing_color_reset(name: &str) -> &str {
    name.strip_suffix("^7").unwrap_or(name)
}

fn register_hook(
    plugin: &Bound<'_, Plugin>,
    event: &str,
    handler: &str,
    priority: CommandPriorities,
) -> PyResult<()> {
    plugin.add_hook(event, &plugin.getattr(handler)?, priority as i32)
}

fn register_command(
    plugin: &Bound<'_, Plugin>,
    names: &[&str],
    handler: &str,
    permission: i32,
    usage: &str,
    channels: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let py = plugin.py();
    let name = match names {
        [single] => pyo3::types::PyString::new(py, single).into_any(),
        several => pyo3::types::PyTuple::new(py, several)?.into_any(),
    };
    plugin.add_command(
        &name,
        &plugin.getattr(handler)?,
        permission,
        channels,
        None,
        CommandPriorities::PRI_NORMAL as u32,
        false,
        5,
        true,
        usage,
    )
}

pub(super) fn has_native_plugin(name: &str) -> bool {
    NATIVE_PLUGINS.iter().any(|(plugin, _)| *plugin == name)
}

pub(super) fn create_native_plugin<'py>(
    py: Python<'py>,
    name: &str,
) -> Option<PyResult<Bound<'py, PyAny>>> {
    NATIVE_PLUGINS
        .iter()
        .find(|(plugin, _)| *plugin == name)
        .map(|(_, constructor)| constructor(py))
}

pub(super) fn is_native_plugin(plugin: &Bound<'_, PyAny>) -> bool {
    plugin
        .get_type()
        .getattr(intern!(plugin.py(), "__module__"))
        .and_then(|module| module.extract::<String>())
        .is_ok_and(|module| module == "shinqlx")
}

#[cfg(test)]
mod native_plugins_tests {
    use pyo3::prelude::*;
    use rstest::rstest;

    use super::{create_native_plugin, has_native_plugin, without_trailing_color_reset};
    use crate::ffi::python::pyshinqlx_setup_fixture::pyshinqlx_setup;

    #[rstest]
    #[case("Ada^7", "Ada")]
    #[case("^1Ada^7", "^1Ada")]
    #[case("Ada", "Ada")]
    #[case("Ada^2", "Ada^2")]
    #[case("^7", "")]
    #[case("", "")]
    fn the_trailing_color_reset_of_a_name_is_removed(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(without_trailing_color_reset(name), expected);
    }

    #[rstest]
    #[case("workshop", true)]
    #[case("log", true)]
    #[case("motd", true)]
    #[case("permission", true)]
    #[case("balance", false)]
    #[case("r!workshop", false)]
    #[case("", false)]
    fn has_native_plugin_knows_the_builtin_plugins(#[case] name: &str, #[case] expected: bool) {
        assert_eq!(has_native_plugin(name), expected);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn create_native_plugin_of_unknown_name_is_none(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            assert!(create_native_plugin(py, "balance").is_none());
        });
    }
}
