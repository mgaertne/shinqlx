use core::{hint::cold_path, sync::atomic::Ordering};

use pyo3::exceptions::PyValueError;
use tap::TryConv;

use crate::{
    ffi::{c::prelude::*, python::prelude::*},
    prelude::*,
};

/// Returns a dictionary with information about a plapub(crate) yer by ID.
#[pyfunction(name = "player_info")]
pub(crate) fn pyshinqlx_player_info(
    py: Python<'_>,
    client_id: i32,
) -> PyResult<Option<PlayerInfo>> {
    py.allow_threads(|| {
        if !(0..MAX_CLIENTS as i32).contains(&client_id) {
            cold_path();
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}, or None.",
                MAX_CLIENTS - 1
            )));
        }

        let allowed_free_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);

        if client_id
            .try_conv::<Client>()
            .ok()
            .filter(|client| {
                client.get_state() == clientState_t::CS_FREE
                    && allowed_free_clients & (1 << client_id as u64) == 0
            })
            .is_some()
        {
            warn!(
                target: "shinqlx",
                "WARNING: get_player_info called for CS_FREE client {client_id}."
            );
            return Ok(None);
        };

        Ok(Some(PlayerInfo::from(client_id)))
    })
}

#[cfg(test)]
mod get_player_info_tests {
    use core::sync::atomic::Ordering;

    use mockall::predicate;
    use pyo3::exceptions::PyValueError;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_player_info_for_client_id_below_zero(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_info(py, -1);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_player_info_for_client_id_above_max_clients(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_player_info(py, 65);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_player_info_for_existing_client(_pyshinqlx_setup: ()) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
                    assert_eq!(
                        player_info.expect("result was not OK"),
                        Some(PlayerInfo {
                            client_id: 2,
                            name: "Mocked Player".to_string(),
                            connection_state: clientState_t::CS_ACTIVE as i32,
                            userinfo: "asdf".to_string(),
                            steam_id: 1234,
                            team: team_t::TEAM_RED as i32,
                            privileges: privileges_t::PRIV_NONE as i32
                        })
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_player_info_for_non_allowed_free_client(_pyshinqlx_setup: ()) {
        ALLOW_FREE_CLIENT.store(0, Ordering::Release);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
            assert_eq!(player_info.expect("result was not OK"), None);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_player_info_for_allowed_free_client(_pyshinqlx_setup: ()) {
        ALLOW_FREE_CLIENT.store(1 << 2, Ordering::Release);

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let player_info = Python::with_gil(|py| pyshinqlx_player_info(py, 2));
                    assert_eq!(
                        player_info.expect("result was not OK"),
                        Some(PlayerInfo {
                            client_id: 2,
                            name: "Mocked Player".to_string(),
                            connection_state: clientState_t::CS_FREE as i32,
                            userinfo: "asdf".to_string(),
                            steam_id: 1234,
                            team: team_t::TEAM_RED as i32,
                            privileges: privileges_t::PRIV_NONE as i32
                        })
                    );
                });
            });
    }
}
