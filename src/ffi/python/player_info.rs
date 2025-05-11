use core::fmt::{Display, Formatter};

use super::prelude::*;
use crate::ffi::c::prelude::*;

/// Information about a player, such as Steam ID, name, client ID, and whatnot.
#[pyclass(
    module = "_shinqlx",
    name = "PlayerInfo",
    frozen,
    get_all,
    sequence,
    str
)]
#[derive(Debug, PartialEq, Clone)]
#[allow(unused)]
pub(crate) struct PlayerInfo {
    /// The player's client ID.
    pub(crate) client_id: i32,
    /// The player's name.
    pub(crate) name: String,
    /// The player's connection state.
    pub(crate) connection_state: i32,
    /// The player's userinfo.
    pub(crate) userinfo: String,
    /// The player's 64-bit representation of the Steam ID.
    pub(crate) steam_id: i64,
    /// The player's team.
    pub(crate) team: i32,
    /// The player's privileges.
    pub(crate) privileges: i32,
}

impl Display for PlayerInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
            self.client_id,
            self.name,
            self.connection_state,
            self.userinfo,
            self.steam_id,
            self.team,
            self.privileges
        )
    }
}

#[pymethods]
impl PlayerInfo {
    #[new]
    fn py_new(tuple: (i32, String, i32, String, i64, i32, i32)) -> Self {
        Self {
            client_id: tuple.0,
            name: tuple.1,
            connection_state: tuple.2,
            userinfo: tuple.3,
            steam_id: tuple.4,
            team: tuple.5,
            privileges: tuple.6,
        }
    }

    fn __repr__(&self) -> String {
        format!("{self}")
    }
}

impl From<i32> for PlayerInfo {
    fn from(client_id: i32) -> Self {
        let mut returned = PlayerInfo {
            client_id,
            name: Default::default(),
            connection_state: clientState_t::CS_FREE as i32,
            userinfo: Default::default(),
            steam_id: 0,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: -1,
        };

        #[cfg_attr(
            test,
            allow(clippy::unnecessary_fallible_conversions, irrefutable_let_patterns)
        )]
        if let Ok(game_entity) = GameEntity::try_from(client_id) {
            returned.name = game_entity.get_player_name();
            returned.team = game_entity.get_team() as i32;
            returned.privileges = game_entity.get_privileges() as i32;
        }

        #[cfg_attr(
            test,
            allow(clippy::unnecessary_fallible_conversions, irrefutable_let_patterns)
        )]
        if let Ok(client) = Client::try_from(client_id) {
            returned.connection_state = client.get_state() as i32;
            returned.userinfo = client.get_user_info().into();
            returned.steam_id = client.get_steam_id() as i64;
        }

        returned
    }
}

#[cfg(test)]
mod player_info_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use rstest::*;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_info_can_be_constructed_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player_info_constructor = py.run(
                cr#"
import shinqlx
_DUMMY_USERINFO = (
    r"ui_singlePlayerActive\0\cg_autoAction\1\cg_autoHop\0"
    r"\cg_predictItems\1\model\bitterman/sport_blue\headmodel\crash/red"
    r"\handicap\100\cl_anonymous\0\color1\4\color2\23\sex\male"
    r"\teamtask\0\rate\25000\country\NO"
)
player_info = shinqlx.PlayerInfo(
            (
                -1,
                "asdf",
                shinqlx.CS_CONNECTED,
                _DUMMY_USERINFO,
                -1,
                shinqlx.TEAM_SPECTATOR,
                shinqlx.PRIV_NONE,
            )
        )
            "#,
                None,
                None,
            );
            assert!(player_info_constructor.is_ok());
        });
    }

    fn default_player_info() -> PlayerInfo {
        PlayerInfo {
            client_id: 2,
            name: "UnknownPlayer".to_string(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".to_string(),
            steam_id: 42,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        }
    }

    #[test]
    fn player_info_python_string() {
        assert_eq!(
            format!("{}", default_player_info()),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    fn player_info_python_repr() {
        assert_eq!(
            default_player_info().__repr__(),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    #[serial]
    fn player_info_from_existing_game_entity_and_client() {
        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 42);
            mock_client
        });

        MockGameEntityBuilder::default()
            .with_player_name(|| "UnknownPlayer".to_string(), 1..)
            .with_team(|| team_t::TEAM_SPECTATOR, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::always(), || {
                assert_eq!(PlayerInfo::from(2), default_player_info());
            });
    }
}
