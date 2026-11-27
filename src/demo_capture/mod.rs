//! Records the matches of the running server and tells about the recordings.

#[cfg_attr(
    test,
    allow(dead_code, reason = "only the running game process fills the catalog")
)]
pub(crate) mod catalog;
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only the running game process reads the engine structures"
    )
)]
mod convert;
#[cfg(not(test))]
mod disk;
#[cfg(not(test))]
mod engine_state;
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "some constructors only serve the tests")
)]
mod lifecycle;
#[cfg(not(test))]
mod live;
#[cfg(not(test))]
mod session;
mod settings;
pub(crate) mod sidecar;
mod storage;

#[cfg(not(test))]
pub(crate) use live::{
    client_dropped, client_thought, configstring, damage_begins, damage_done, frame, is_recording,
    map_ended, map_loaded, map_restarted, server_command, stats, weapon_fired,
};
