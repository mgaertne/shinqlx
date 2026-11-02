use pyo3::{intern, prelude::*, types::IntoPyDict};

use super::{super::tell, command_prefix};

pub(super) const SOUNDS_FLAG: &str = "essentials:sounds_enabled";

pub(super) fn sounds_enabled(
    database: &Bound<'_, PyAny>,
    player: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let py = database.py();
    database
        .call_method(
            intern!(py, "get_flag"),
            (player, SOUNDS_FLAG),
            Some(&[("default", true)].into_py_dict(py)?),
        )?
        .extract()
}

pub(super) fn tell_sounds_disabled(player: &Bound<'_, PyAny>) -> PyResult<()> {
    tell(
        player,
        &format!(
            "Sounds are disabled. Use ^6{}sounds^7 to enable them again.",
            command_prefix()?
        ),
    )
}
