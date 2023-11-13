use crate::prelude::*;
use pyo3::prelude::*;

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
    pub(crate) steam_id: u64,
    /// The player's team.
    pub(crate) team: i32,
    /// The player's privileges.
    pub(crate) privileges: i32,
}

#[pymethods]
impl PlayerInfo {
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
            returned.steam_id = client.get_steam_id();
        });

        returned
    }
}

#[cfg(test)]
mod player_info_tests {
    use super::PlayerInfo;
    use crate::ffi::c::client::MockClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::prelude::*;
    use pretty_assertions::assert_eq;

    #[test]
    #[serial]
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
    #[serial]
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
