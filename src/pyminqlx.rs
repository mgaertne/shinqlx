use crate::quake_common::clientState_t::CS_FREE;
use crate::quake_common::team_t::TEAM_SPECTATOR;
use crate::quake_common::Client;
use crate::SV_MAXCLIENTS;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::borrow::Cow;

#[pyclass]
#[pyo3(name = "player_info")]
#[allow(unused)]
struct PlayerInfo {
    client_id: i32,
    name: Cow<'static, str>,
    connection_state: i32,
    userinfo: Cow<'static, str>,
    steam_id: u64,
    team: i32,
    privileges: i32,
}

#[pyfunction]
#[pyo3(name = "player_info")]
fn get_player_info(client_id: i32) -> PyResult<Option<PlayerInfo>> {
    println!(
        "get_player_info called although not yet configured properly! {}",
        client_id
    );
    if client_id < 0 || client_id > unsafe { SV_MAXCLIENTS } {
        return Err(PyValueError::new_err(format!(
            "client_id needs to be a number from 0 to {}.",
            unsafe { SV_MAXCLIENTS }
        )));
    }
    let client_result = Client::try_from(client_id);
    match client_result {
        Err(_) => Ok(Some(PlayerInfo {
            client_id,
            name: Default::default(),
            connection_state: 0,
            userinfo: Default::default(),
            steam_id: 0,
            team: TEAM_SPECTATOR as i32,
            privileges: -1,
        })),
        Ok(client) => {
            if client.get_state() == CS_FREE as i32 {
                #[cfg(debug_assertions)]
                println!(
                    "WARNING: get_player_info called for CS_FREE client {}.",
                    client_id
                );
                Ok(None)
            } else {
                Ok(Some(PlayerInfo {
                    client_id,
                    name: client.get_player_name(),
                    connection_state: client.get_state(),
                    userinfo: client.get_user_info(),
                    steam_id: client.get_steam_id(),
                    team: client.get_team(),
                    privileges: client.get_privileges(),
                }))
            }
        }
    }
}

#[pymodule]
#[pyo3(name = "_minqlx")]
fn pyminqlx_init_module(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_player_info, m)?)?;
    Ok(())
}
