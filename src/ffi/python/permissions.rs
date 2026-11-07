#![allow(
    non_camel_case_types,
    clippy::upper_case_acronyms,
    reason = "the variants are written like the other enumerations of the module"
)]

use pyo3::{IntoPyObjectExt, class::basic::CompareOp, types::PyInt};

use super::prelude::*;

/// The permission levels of the commands of the plugins.
///
/// The level of a player is stored in the database and compared with the ``permission`` of a
/// command. Any integer is a valid level, the members name the levels that the built-in plugins
/// use. Members compare to each other and to integers, and work as integers in the API.
#[pyclass(module = "_shinqlx", name = "Permissions", frozen)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum Permission {
    /// Information and the settings of oneself, e.g. ``!seen`` or ``!clan``.
    PLAYER = 0,
    /// Directing the running game, e.g. ``!shuffle``, ``!pause`` or ``!mute``.
    MODERATOR = 1,
    /// Enforcing the rules of a game, e.g. ``!kick``, ``!ban`` or ``!map``.
    GAME_ADMIN = 2,
    /// Managing data of the community, e.g. ratings. Not used by the built-in plugins.
    COMMUNITY_ADMIN = 3,
    /// Managing what is shown on the server, e.g. the message of the day.
    SERVER_ADMIN = 4,
    /// Controlling the bot itself, e.g. ``!setperm``, ``!rcon`` or ``!load``.
    GOD = 5,
}

#[pymethods]
impl Permission {
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
        let this = *slf.get() as i32;
        let other_level = other.cast::<Self>().map_or_else(
            |_| {
                other
                    .is_exact_instance_of::<PyInt>()
                    .then(|| other.extract::<i32>().ok())
                    .flatten()
            },
            |same| Some(*same.get() as i32),
        );

        match other_level {
            Some(level) => op.matches(this.cmp(&level)).into_py_any(slf.py()),
            None => Ok(slf.py().NotImplemented()),
        }
    }
}

#[cfg(test)]
mod permissions_tests {
    use rstest::rstest;

    use super::Permission;
    use crate::{ffi::python::prelude::*, prelude::*};

    #[rstest]
    #[case(Permission::PLAYER, 0)]
    #[case(Permission::MODERATOR, 1)]
    #[case(Permission::GAME_ADMIN, 2)]
    #[case(Permission::COMMUNITY_ADMIN, 3)]
    #[case(Permission::SERVER_ADMIN, 4)]
    #[case(Permission::GOD, 5)]
    fn permission_is_the_level_of_the_commands(#[case] permission: Permission, #[case] level: i32) {
        assert_eq!(permission.__index__(), level);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn permissions_are_registered_and_comparable(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

assert shinqlx.Permissions.GOD == 5
assert shinqlx.Permissions.PLAYER == 0
assert 2 == shinqlx.Permissions.GAME_ADMIN
assert shinqlx.Permissions.MODERATOR != 2
assert shinqlx.Permissions.MODERATOR != "1"
assert shinqlx.Permissions.SERVER_ADMIN >= shinqlx.Permissions.GAME_ADMIN
assert shinqlx.Permissions.SERVER_ADMIN > 3
assert 3 < shinqlx.Permissions.SERVER_ADMIN
assert 6 > shinqlx.Permissions.GOD
assert shinqlx.Permissions.PLAYER < shinqlx.Permissions.GOD
assert shinqlx.Permissions.GOD != shinqlx.Teams.RED
assert sorted([shinqlx.Permissions.GOD, shinqlx.Permissions.PLAYER, shinqlx.Permissions.MODERATOR]) == [
    shinqlx.Permissions.PLAYER,
    shinqlx.Permissions.MODERATOR,
    shinqlx.Permissions.GOD,
]
assert [10, 20, 30, 40, 50, 60][shinqlx.Permissions.COMMUNITY_ADMIN] == 40
assert hash(shinqlx.Permissions.GAME_ADMIN) == hash(2)
            "#,
                None,
                None,
            )
            .expect("this should not happen");
        });
    }
}
