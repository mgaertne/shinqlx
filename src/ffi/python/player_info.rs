use super::prelude::*;
use crate::ffi::c::prelude::*;

/// Information about a player, such as Steam ID, name, client ID, and whatnot.
#[pyclass(frozen)]
#[pyo3(module = "shinqlx", name = "PlayerInfo", get_all)]
#[derive(Debug, PartialEq)]
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

    fn __str__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
    }

    fn __repr__(&self) -> String {
        format!("PlayerInfo(client_id={}, name={}, connection_state={}, userinfo={}, steam_id={}, team={}, privileges={})",
                self.client_id,
                self.name,
                self.connection_state,
                self.userinfo,
                self.steam_id,
                self.team,
                self.privileges)
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

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        GameEntity::try_from(client_id)
            .ok()
            .iter()
            .for_each(|game_entity| {
                returned.name = game_entity.get_player_name();
                returned.team = game_entity.get_team() as i32;
                returned.privileges = game_entity.get_privileges() as i32;
            });

        #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
        Client::try_from(client_id).ok().iter().for_each(|client| {
            returned.connection_state = client.get_state() as i32;
            returned.userinfo = client.get_user_info();
            returned.steam_id = client.get_steam_id() as i64;
        });

        returned
    }
}

#[cfg(test)]
mod player_info_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_info_can_be_constructed_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player_info_constructor = py.run_bound(
                r#"
import _shinqlx
_DUMMY_USERINFO = (
    "ui_singlePlayerActive\\0\\cg_autoAction\\1\\cg_autoHop\\0"
    "\\cg_predictItems\\1\\model\\bitterman/sport_blue\\headmodel\\crash/red"
    "\\handicap\\100\\cl_anonymous\\0\\color1\\4\\color2\\23\\sex\\male"
    "\\teamtask\\0\\rate\\25000\\country\\NO"
)
player_info = _shinqlx.PlayerInfo(
            (
                -1,
                "asdf",
                _shinqlx.CS_CONNECTED,
                _DUMMY_USERINFO,
                -1,
                _shinqlx.TEAM_SPECTATOR,
                _shinqlx.PRIV_NONE,
            )
        )
            "#,
                None,
                None,
            );
            assert!(
                player_info_constructor.is_ok(),
                "{}",
                player_info_constructor.expect_err("this should not happen")
            );
        });
    }

    #[test]
    fn player_info_python_string() {
        let player_info = PlayerInfo {
            client_id: 2,
            name: "UnknownPlayer".into(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".into(),
            steam_id: 42,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };

        assert_eq!(
            player_info.__str__(),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    fn player_info_python_repr() {
        let player_info = PlayerInfo {
            client_id: 2,
            name: "UnknownPlayer".into(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".into(),
            steam_id: 42,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };

        assert_eq!(
            player_info.__repr__(),
            "PlayerInfo(client_id=2, name=UnknownPlayer, connection_state=4, userinfo=asdf, \
            steam_id=42, team=3, privileges=0)"
        );
    }

    #[test]
    #[serial]
    fn player_info_from_existing_game_entity_and_client() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "UnknownPlayer".into());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_SPECTATOR);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });
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

        assert_eq!(
            PlayerInfo::from(2),
            PlayerInfo {
                client_id: 2,
                name: "UnknownPlayer".into(),
                connection_state: clientState_t::CS_ACTIVE as i32,
                userinfo: "asdf".into(),
                steam_id: 42,
                team: team_t::TEAM_SPECTATOR as i32,
                privileges: privileges_t::PRIV_NONE as i32
            }
        );
    }
}
