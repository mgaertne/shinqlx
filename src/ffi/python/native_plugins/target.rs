use pyo3::{intern, prelude::*};

use super::without_trailing_color_reset;
use crate::ffi::python::{NonexistentPlayerError, Plugin, PythonReturnCodes};

pub(super) const INVALID_CLIENT_ID: &str =
    "Invalid client ID. Use either a client ID or a SteamID64.";

pub(super) enum Target<'py> {
    Id(i64),
    Player(i64, Bound<'py, PyAny>),
}

impl Target<'_> {
    pub(super) fn steam_id(&self) -> i64 {
        match self {
            Target::Id(steam_id) | Target::Player(steam_id, _) => *steam_id,
        }
    }

    pub(super) fn player(&self) -> Option<&Bound<'_, PyAny>> {
        match self {
            Target::Id(_) => None,
            Target::Player(_, player) => Some(player),
        }
    }
}

pub(super) fn reply(channel: &Bound<'_, PyAny>, message: &str) -> PyResult<Option<i32>> {
    channel
        .call_method1(intern!(channel.py(), "reply"), (message,))
        .map(|_| None)
}

pub(super) fn usage() -> PyResult<Option<i32>> {
    Ok(Some(PythonReturnCodes::RET_USAGE as i32))
}

pub(super) fn find_player<'py>(
    py: Python<'py>,
    client_id: i64,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    match py
        .get_type::<Plugin>()
        .call_method1(intern!(py, "player"), (client_id,))
    {
        Ok(player) if !player.is_none() => Ok(Some(player)),
        Ok(_) => Ok(None),
        Err(err) if err.is_instance_of::<NonexistentPlayerError>(py) => Ok(None),
        Err(err) => Err(err),
    }
}

pub(super) fn resolve_target<'py>(py: Python<'py>, ident: i64) -> PyResult<Option<Target<'py>>> {
    if !(0..64).contains(&ident) {
        return Ok(Some(Target::Id(ident)));
    }

    let Some(player) = find_player(py, ident)? else {
        return Ok(None);
    };
    let steam_id = player.getattr(intern!(py, "steam_id"))?.extract()?;
    Ok(Some(Target::Player(steam_id, player)))
}

pub(super) enum Resolution<'py> {
    Target(Target<'py>),
    UnknownClientId,
    Reported,
}

pub(super) fn resolve_text<'py>(
    py: Python<'py>,
    caller: &Bound<'py, PyAny>,
    text: &str,
) -> PyResult<Resolution<'py>> {
    let text = text.trim();
    if let Ok(ident) = text.parse::<i64>() {
        return Ok(
            resolve_target(py, ident)?.map_or(Resolution::UnknownClientId, Resolution::Target)
        );
    }

    let Some(found) = Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), caller, text, None)?
    else {
        return Ok(Resolution::Reported);
    };
    let steam_id = found.steam_id;
    Ok(Resolution::Target(Target::Player(
        steam_id,
        Bound::new(py, found)?.into_any(),
    )))
}

pub(super) fn display_name(target: &Target<'_>) -> PyResult<String> {
    match target {
        Target::Id(steam_id) => Ok(steam_id.to_string()),
        Target::Player(_, player) => player
            .getattr(intern!(player.py(), "name"))?
            .extract::<String>()
            .map(|name| without_trailing_color_reset(&name).to_owned()),
    }
}
