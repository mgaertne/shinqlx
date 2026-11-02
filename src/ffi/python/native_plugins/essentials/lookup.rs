use pyo3::prelude::*;

use super::super::{
    target::{Resolution, Target, reply, resolve_text, stop_all, usage},
    tell,
};

pub(super) enum Lookup<'py> {
    Found(Bound<'py, PyAny>),
    Finished(Option<i32>),
}

pub(super) fn lookup_player<'py>(
    caller: &Bound<'py, PyAny>,
    channel: &Bound<'py, PyAny>,
    text: &str,
    invalid: &str,
    tell_caller: bool,
) -> PyResult<Lookup<'py>> {
    match resolve_text(caller.py(), caller, text)? {
        Resolution::Target(Target::Player(_, player)) => Ok(Lookup::Found(player)),
        Resolution::Reported => stop_all().map(Lookup::Finished),
        Resolution::Target(Target::Id(_)) | Resolution::UnknownClientId if tell_caller => {
            tell(caller, invalid)?;
            stop_all().map(Lookup::Finished)
        }
        Resolution::Target(Target::Id(_)) | Resolution::UnknownClientId => {
            reply(channel, invalid).map(Lookup::Finished)
        }
    }
}

pub(super) fn act_on_player<'py>(
    caller: &Bound<'py, PyAny>,
    msg: &[String],
    channel: &Bound<'py, PyAny>,
    action: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<()>,
) -> PyResult<Option<i32>> {
    let Some(ident) = msg.get(1) else {
        return usage();
    };
    match lookup_player(caller, channel, ident, "Invalid ID.", false)? {
        Lookup::Found(player) => action(&player).map(|()| None),
        Lookup::Finished(code) => Ok(code),
    }
}
