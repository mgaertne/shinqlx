#![allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    reason = "the variants mirror the names of the C constants of the engine"
)]

use derive_more::Display;
use pyo3::{IntoPyObjectExt, class::basic::CompareOp, types::PyInt};

use super::{PythonReturnCodes, commands::CommandPriorities, prelude::*};
use crate::ffi::c::prelude::*;

/// Compares a member that behaves like an integer with another member of the same enumeration or
/// with an integer, but never with the members of other enumerations.
pub(super) fn richcmp_with_index(
    py: Python<'_>,
    this: i32,
    same: Option<i32>,
    other: &Bound<'_, PyAny>,
    op: CompareOp,
) -> PyResult<Py<PyAny>> {
    let equal = same.map(|value| value == this).or_else(|| {
        other
            .is_exact_instance_of::<PyInt>()
            .then(|| other.extract::<i32>().ok().map(|value| value == this))
            .flatten()
    });
    richcmp_result(py, equal, op)
}

/// Returns the name of a team or privilege level, given as enum member or as string.
pub(crate) fn name_of(value: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(team) = value.cast::<Team>() {
        return Ok(team.get().to_string());
    }
    if let Ok(privileges) = value.cast::<Privileges>() {
        return Ok(privileges.get().to_string());
    }
    value.extract::<String>()
}

fn compare_with_name_or_index(value: i32, name: &str, other: &Bound<'_, PyAny>) -> Option<bool> {
    if let Ok(other_name) = other.extract::<&str>() {
        return Some(other_name == name);
    }
    if other.is_exact_instance_of::<PyInt>() {
        return other.extract::<i32>().ok().map(|index| index == value);
    }
    None
}

fn richcmp_result(py: Python<'_>, equal: Option<bool>, op: CompareOp) -> PyResult<Py<PyAny>> {
    match (op, equal) {
        (CompareOp::Eq, Some(equal)) => equal.into_py_any(py),
        (CompareOp::Ne, Some(equal)) => (!equal).into_py_any(py),
        _ => Ok(py.NotImplemented()),
    }
}

/// The teams of Quake Live.
///
/// The string of a member is the name that :attr:`shinqlx.Player.team` returns. The old
/// constants ``shinqlx.TEAM_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "Teams", frozen, str)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Display)]
#[repr(i32)]
pub(crate) enum Team {
    #[display("free")]
    FREE = team_t::TEAM_FREE as i32,
    #[display("red")]
    RED = team_t::TEAM_RED as i32,
    #[display("blue")]
    BLUE = team_t::TEAM_BLUE as i32,
    #[display("spectator")]
    SPECTATOR = team_t::TEAM_SPECTATOR as i32,
}

#[pymethods]
impl Team {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let this = slf.get();
        let equal = other.cast::<Self>().map_or_else(
            |_| compare_with_name_or_index(*this as i32, &this.to_string(), other),
            |same| Some(same.get() == this),
        );
        richcmp_result(slf.py(), equal, op)
    }
}

/// The privilege levels of a player.
///
/// The string of a member is the name that :attr:`shinqlx.Player.privileges` returns. The old
/// constants ``shinqlx.PRIV_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "Privileges", frozen, str)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Display)]
#[repr(i32)]
pub(crate) enum Privileges {
    #[display("none")]
    NONE = privileges_t::PRIV_NONE as i32,
    #[display("mod")]
    MOD = privileges_t::PRIV_MOD as i32,
    #[display("admin")]
    ADMIN = privileges_t::PRIV_ADMIN as i32,
    #[display("root")]
    ROOT = privileges_t::PRIV_ROOT as i32,
    #[display("banned")]
    BANNED = privileges_t::PRIV_BANNED as i32,
}

#[pymethods]
impl Privileges {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let this = slf.get();
        let equal = other.cast::<Self>().map_or_else(
            |_| compare_with_name_or_index(*this as i32, &this.to_string(), other),
            |same| Some(same.get() == this),
        );
        richcmp_result(slf.py(), equal, op)
    }
}

/// The connection states of a client.
///
/// The string of a member is the name that :attr:`shinqlx.Player.state` returns. The old
/// constants ``shinqlx.CS_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "ConnectionStates", frozen, str)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Display)]
#[repr(i32)]
pub(crate) enum ConnectionState {
    #[display("free")]
    FREE = clientState_t::CS_FREE as i32,
    #[display("zombie")]
    ZOMBIE = clientState_t::CS_ZOMBIE as i32,
    #[display("connected")]
    CONNECTED = clientState_t::CS_CONNECTED as i32,
    #[display("primed")]
    PRIMED = clientState_t::CS_PRIMED as i32,
    #[display("active")]
    ACTIVE = clientState_t::CS_ACTIVE as i32,
}

#[pymethods]
impl ConnectionState {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let this = slf.get();
        let equal = other.cast::<Self>().map_or_else(
            |_| compare_with_name_or_index(*this as i32, &this.to_string(), other),
            |same| Some(same.get() == this),
        );
        richcmp_result(slf.py(), equal, op)
    }
}

/// The priorities of hooks and commands, the highest priority is called first.
///
/// The old constants ``shinqlx.PRI_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "Priorities", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum Priority {
    HIGHEST = CommandPriorities::PRI_HIGHEST as i32,
    HIGH = CommandPriorities::PRI_HIGH as i32,
    NORMAL = CommandPriorities::PRI_NORMAL as i32,
    LOW = CommandPriorities::PRI_LOW as i32,
    LOWEST = CommandPriorities::PRI_LOWEST as i32,
}

#[pymethods]
impl Priority {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let same = other.cast::<Self>().ok().map(|same| *same.get() as i32);
        richcmp_with_index(slf.py(), *slf.get() as i32, same, other, op)
    }
}

/// The values that handlers of hooks and commands can return.
///
/// The old constants ``shinqlx.RET_*`` have the values of the members.
#[pyclass(module = "_shinqlx", name = "ReturnCodes", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum ReturnCode {
    /// Continue execution normally.
    NONE = PythonReturnCodes::RET_NONE as i32,
    /// Stop execution of event handlers within Python.
    STOP = PythonReturnCodes::RET_STOP as i32,
    /// Only stop the event, but let other handlers process it.
    STOP_EVENT = PythonReturnCodes::RET_STOP_EVENT as i32,
    /// Stop execution at an engine level. SCARY STUFF!
    STOP_ALL = PythonReturnCodes::RET_STOP_ALL as i32,
    /// Used for commands. Replies to the channel with a command's usage.
    USAGE = PythonReturnCodes::RET_USAGE as i32,
}

#[pymethods]
impl ReturnCode {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __bool__(&self) -> bool {
        *self as i32 != 0
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let this = *slf.get() as i32;
        let equal = if other.is_none() {
            Some(*slf.get() == ReturnCode::NONE)
        } else {
            other.cast::<Self>().map_or_else(
                |_| {
                    other
                        .is_exact_instance_of::<PyInt>()
                        .then(|| other.extract::<i32>().ok().map(|value| value == this))
                        .flatten()
                },
                |same| Some(same.get() == slf.get()),
            )
        };
        richcmp_result(slf.py(), equal, op)
    }
}

/// The weapons of Quake Live.
///
/// The string of a member is the short name that :meth:`shinqlx.Player.weapon` accepts and that
/// the fields of :class:`shinqlx.Weapons` use.
#[pyclass(module = "_shinqlx", name = "Weapon", frozen, str)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Display)]
#[repr(i32)]
pub(crate) enum Weapon {
    #[display("none")]
    NONE = weapon_t::WP_NONE as i32,
    #[display("g")]
    GAUNTLET = weapon_t::WP_GAUNTLET as i32,
    #[display("mg")]
    MACHINEGUN = weapon_t::WP_MACHINEGUN as i32,
    #[display("sg")]
    SHOTGUN = weapon_t::WP_SHOTGUN as i32,
    #[display("gl")]
    GRENADE_LAUNCHER = weapon_t::WP_GRENADE_LAUNCHER as i32,
    #[display("rl")]
    ROCKET_LAUNCHER = weapon_t::WP_ROCKET_LAUNCHER as i32,
    #[display("lg")]
    LIGHTNING = weapon_t::WP_LIGHTNING as i32,
    #[display("rg")]
    RAILGUN = weapon_t::WP_RAILGUN as i32,
    #[display("pg")]
    PLASMAGUN = weapon_t::WP_PLASMAGUN as i32,
    #[display("bfg")]
    BFG = weapon_t::WP_BFG as i32,
    #[display("gh")]
    GRAPPLING_HOOK = weapon_t::WP_GRAPPLING_HOOK as i32,
    #[display("ng")]
    NAILGUN = weapon_t::WP_NAILGUN as i32,
    #[display("pl")]
    PROX_LAUNCHER = weapon_t::WP_PROX_LAUNCHER as i32,
    #[display("cg")]
    CHAINGUN = weapon_t::WP_CHAINGUN as i32,
    #[display("hmg")]
    HMG = weapon_t::WP_HMG as i32,
    #[display("hands")]
    HANDS = weapon_t::WP_HANDS as i32,
}

#[pymethods]
impl Weapon {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let this = slf.get();
        let equal = other.cast::<Self>().map_or_else(
            |_| compare_with_name_or_index(*this as i32, &this.to_string(), other),
            |same| Some(same.get() == this),
        );
        richcmp_result(slf.py(), equal, op)
    }
}

/// The states of a vote on the server.
#[pyclass(module = "_shinqlx", name = "VoteStates", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum VoteState {
    NONE = voteState_t::VOTE_NONE as i32,
    PENDING = voteState_t::VOTE_PENDING as i32,
    YES = voteState_t::VOTE_YES as i32,
    NO = voteState_t::VOTE_NO as i32,
    FORCE_PASS = voteState_t::VOTE_FORCE_PASS as i32,
    FORCE_FAIL = voteState_t::VOTE_FORCE_FAIL as i32,
    EXPIRED = voteState_t::VOTE_EXPIRED as i32,
}

#[pymethods]
impl VoteState {
    fn __index__(&self) -> i32 {
        *self as i32
    }

    fn __hash__(&self) -> isize {
        *self as isize
    }

    fn __richcmp__(
        slf: &Bound<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let same = other.cast::<Self>().ok().map(|same| *same.get() as i32);
        richcmp_with_index(slf.py(), *slf.get() as i32, same, other, op)
    }
}

#[cfg(test)]
mod game_enums_tests {
    use pyo3::IntoPyObjectExt;
    use rstest::rstest;

    use super::{
        ConnectionState, Priority, Privileges, PythonReturnCodes, ReturnCode, Team, VoteState,
        name_of,
    };
    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[case(Team::FREE, team_t::TEAM_FREE as i32, "free")]
    #[case(Team::RED, team_t::TEAM_RED as i32, "red")]
    #[case(Team::BLUE, team_t::TEAM_BLUE as i32, "blue")]
    #[case(Team::SPECTATOR, team_t::TEAM_SPECTATOR as i32, "spectator")]
    fn team_has_the_index_and_name_of_the_engine(
        #[case] team: Team,
        #[case] index: i32,
        #[case] name: &str,
    ) {
        assert_eq!(team.__index__(), index);
        assert_eq!(team.to_string(), name);
    }

    #[rstest]
    #[case(Privileges::NONE, 0, "none")]
    #[case(Privileges::MOD, 1, "mod")]
    #[case(Privileges::ADMIN, 2, "admin")]
    #[case(Privileges::ROOT, 3, "root")]
    #[case(Privileges::BANNED, -1, "banned")]
    fn privileges_have_the_index_and_name_of_the_engine(
        #[case] privileges: Privileges,
        #[case] index: i32,
        #[case] name: &str,
    ) {
        assert_eq!(privileges.__index__(), index);
        assert_eq!(privileges.to_string(), name);
    }

    #[rstest]
    #[case(ConnectionState::FREE, 0, "free")]
    #[case(ConnectionState::ZOMBIE, 1, "zombie")]
    #[case(ConnectionState::CONNECTED, 2, "connected")]
    #[case(ConnectionState::PRIMED, 3, "primed")]
    #[case(ConnectionState::ACTIVE, 4, "active")]
    fn connection_state_has_the_index_and_name_of_the_engine(
        #[case] state: ConnectionState,
        #[case] index: i32,
        #[case] name: &str,
    ) {
        assert_eq!(state.__index__(), index);
        assert_eq!(state.to_string(), name);
    }

    #[rstest]
    #[case(Priority::HIGHEST, 0)]
    #[case(Priority::HIGH, 1)]
    #[case(Priority::NORMAL, 2)]
    #[case(Priority::LOW, 3)]
    #[case(Priority::LOWEST, 4)]
    fn priority_is_the_priority_of_hooks_and_commands(
        #[case] priority: Priority,
        #[case] index: i32,
    ) {
        assert_eq!(priority.__index__(), index);
    }

    #[rstest]
    #[case(ReturnCode::NONE, 0)]
    #[case(ReturnCode::STOP, 1)]
    #[case(ReturnCode::STOP_EVENT, 2)]
    #[case(ReturnCode::STOP_ALL, 3)]
    #[case(ReturnCode::USAGE, 4)]
    fn return_code_is_the_return_value_of_handlers(#[case] code: ReturnCode, #[case] index: i32) {
        assert_eq!(code.__index__(), index);
    }

    #[rstest]
    #[case(VoteState::NONE, 0)]
    #[case(VoteState::PENDING, 1)]
    #[case(VoteState::YES, 2)]
    #[case(VoteState::NO, 3)]
    #[case(VoteState::FORCE_PASS, 4)]
    #[case(VoteState::FORCE_FAIL, 5)]
    #[case(VoteState::EXPIRED, 6)]
    fn vote_state_is_the_state_of_the_engine(#[case] state: VoteState, #[case] index: i32) {
        assert_eq!(state.__index__(), index);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn priority_and_return_code_are_usable_as_integers_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let priority = Priority::LOW
                .into_py_any(py)
                .and_then(|member| member.extract::<i32>(py));
            let return_code = ReturnCode::STOP_EVENT
                .into_py_any(py)
                .and_then(|member| member.extract::<PythonReturnCodes>(py));

            assert_eq!(priority.expect("this should not happen"), 3);
            assert!(matches!(
                return_code.expect("this should not happen"),
                PythonReturnCodes::RET_STOP_EVENT
            ));
        });
    }

    #[rstest]
    #[case(Weapon::NONE, 0, "none")]
    #[case(Weapon::GAUNTLET, 1, "g")]
    #[case(Weapon::MACHINEGUN, 2, "mg")]
    #[case(Weapon::SHOTGUN, 3, "sg")]
    #[case(Weapon::GRENADE_LAUNCHER, 4, "gl")]
    #[case(Weapon::ROCKET_LAUNCHER, 5, "rl")]
    #[case(Weapon::LIGHTNING, 6, "lg")]
    #[case(Weapon::RAILGUN, 7, "rg")]
    #[case(Weapon::PLASMAGUN, 8, "pg")]
    #[case(Weapon::BFG, 9, "bfg")]
    #[case(Weapon::GRAPPLING_HOOK, 10, "gh")]
    #[case(Weapon::NAILGUN, 11, "ng")]
    #[case(Weapon::PROX_LAUNCHER, 12, "pl")]
    #[case(Weapon::CHAINGUN, 13, "cg")]
    #[case(Weapon::HMG, 14, "hmg")]
    #[case(Weapon::HANDS, 15, "hands")]
    fn weapon_has_the_index_and_short_name_of_the_engine(
        #[case] weapon: Weapon,
        #[case] index: i32,
        #[case] name: &str,
    ) {
        assert_eq!(weapon.__index__(), index);
        assert_eq!(weapon.to_string(), name);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn name_of_accepts_members_and_strings(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let team = Team::RED
                .into_bound_py_any(py)
                .expect("this should not happen");
            let privileges = Privileges::ADMIN
                .into_bound_py_any(py)
                .expect("this should not happen");
            let text = pyo3::types::PyString::new(py, "blue");
            let number = pyo3::types::PyInt::new(py, 1i32);

            assert_eq!(name_of(&team).expect("this should not happen"), "red");
            assert_eq!(
                name_of(&privileges).expect("this should not happen"),
                "admin"
            );
            assert_eq!(
                name_of(text.as_any()).expect("this should not happen"),
                "blue"
            );
            assert!(name_of(number.as_any()).is_err());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_enums_are_registered_in_the_module(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

assert shinqlx.Teams.RED == 1
assert shinqlx.ReturnCodes.NONE == None
assert None == shinqlx.ReturnCodes.NONE
assert shinqlx.ReturnCodes.STOP != None
assert not shinqlx.ReturnCodes.NONE
assert shinqlx.ReturnCodes.STOP
assert shinqlx.ReturnCodes.NONE == shinqlx.RET_NONE
assert shinqlx.ReturnCodes.STOP_ALL == 3
assert shinqlx.ReturnCodes.USAGE != shinqlx.Priorities.LOWEST
assert hash(shinqlx.ReturnCodes.STOP_EVENT) == hash(2)
assert shinqlx.Teams.RED == shinqlx.Teams.RED
assert shinqlx.Teams.RED != shinqlx.Teams.BLUE
assert str(shinqlx.Teams.SPECTATOR) == "spectator"
assert str(shinqlx.Privileges.ADMIN) == "admin"
assert shinqlx.Privileges.BANNED == -1
assert str(shinqlx.ConnectionStates.ACTIVE) == "active"
assert shinqlx.Priorities.HIGHEST == shinqlx.PRI_HIGHEST
assert shinqlx.ReturnCodes.STOP_ALL == shinqlx.RET_STOP_ALL
assert shinqlx.Teams.BLUE == shinqlx.TEAM_BLUE
assert shinqlx.Privileges.ROOT == shinqlx.PRIV_ROOT
assert shinqlx.ConnectionStates.PRIMED == shinqlx.CS_PRIMED
assert [10, 20, 30, 40][shinqlx.Teams.BLUE] == 30
assert shinqlx.VoteStates.FORCE_FAIL == 5
assert shinqlx.Weapon.ROCKET_LAUNCHER == 5
assert shinqlx.Weapon.ROCKET_LAUNCHER == "rl"
assert str(shinqlx.Weapon.RAILGUN) == "rg"
assert shinqlx.Weapon.RAILGUN != shinqlx.Teams.RED
assert shinqlx.WEAPONS[shinqlx.Weapon.PLASMAGUN] == "pg"

assert shinqlx.Teams.RED == "red"
assert "red" == shinqlx.Teams.RED
assert shinqlx.Teams.RED != "blue"
assert shinqlx.Teams.RED != shinqlx.Privileges.MOD
assert shinqlx.Privileges.ADMIN == "admin"
assert shinqlx.ConnectionStates.ACTIVE == "active"
assert shinqlx.Teams.RED != 2
assert hash(shinqlx.Teams.BLUE) == hash(2)
assert {shinqlx.Teams.RED: 1}[shinqlx.Teams.RED] == 1
            "#,
                None,
                None,
            )
            .expect("this should not happen");
        });
    }
}
