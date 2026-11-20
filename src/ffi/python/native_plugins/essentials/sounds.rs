use pyo3::{intern, prelude::*, types::IntoPyDict};

use super::{super::tell, command_prefix};
use crate::ffi::python::Player;

pub(super) const SOUNDS_FLAG: &str = "essentials:sounds_enabled";

pub(super) fn sounds_enabled(
    database: &Bound<'_, PyAny>,
    player: &Bound<'_, Player>,
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

pub(super) fn tell_sounds_disabled(player: &Bound<'_, Player>) -> PyResult<()> {
    tell(
        player,
        &format!(
            "Sounds are disabled. Use ^6{}sounds^7 to enable them again.",
            command_prefix()?
        ),
    )
}
