use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
pub(crate) fn pyshinqlx_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
    py.allow_threads(|| {
        let maxclients = MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| Ok(main_engine.get_max_clients()),
        )?;

        let result: Vec<Option<PlayerInfo>> = (0..maxclients)
            .filter_map(|client_id| {
                #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
                Client::try_from(client_id).map_or_else(
                    |_| None,
                    |client| match client.get_state() {
                        clientState_t::CS_FREE => None,
                        _ => Some(Some(PlayerInfo::from(client_id))),
                    },
                )
            })
            .collect();

        Ok(result)
    })
}

#[cfg(test)]
mod get_players_info_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_players_info_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = pyshinqlx_players_info(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_players_info_for_existing_clients(_pyshinqlx_setup: ()) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        client_try_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_FREE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_player_name()
                .returning(|| "Mocked Player".to_string());
            mock_game_entity
                .expect_get_team()
                .returning(|| team_t::TEAM_RED);
            mock_game_entity
                .expect_get_privileges()
                .returning(|| privileges_t::PRIV_NONE);
            mock_game_entity
        });

        with_mocked_engine(|mock_engine| {
            mock_engine.expect_get_max_clients().returning(|| 3);
        })
        .run(|| {
            let players_info = Python::with_gil(pyshinqlx_players_info);
            assert_eq!(
                players_info.expect("result was not OK"),
                vec![
                    Some(PlayerInfo {
                        client_id: 0,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: privileges_t::PRIV_NONE as i32
                    }),
                    Some(PlayerInfo {
                        client_id: 2,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: privileges_t::PRIV_NONE as i32
                    })
                ]
            );
        });
    }
}
