mod permission;
mod workshop;

use pyo3::{intern, prelude::*};

use super::{
    commands::CommandPriorities,
    plugin::{Plugin, PluginMethods},
};

type Constructor = for<'py> fn(Python<'py>) -> PyResult<Bound<'py, PyAny>>;

static NATIVE_PLUGINS: [(&str, Constructor); 2] = [
    ("permission", permission::create),
    ("workshop", workshop::create),
];

fn register_command(
    plugin: &Bound<'_, Plugin>,
    name: &str,
    handler: &str,
    permission: i32,
    usage: &str,
    channels: Option<Bound<'_, PyAny>>,
) -> PyResult<()> {
    let py = plugin.py();
    plugin.add_command(
        &pyo3::types::PyString::new(py, name),
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

    use super::{create_native_plugin, has_native_plugin};
    use crate::ffi::python::pyshinqlx_setup_fixture::pyshinqlx_setup;

    #[rstest]
    #[case("workshop", true)]
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
