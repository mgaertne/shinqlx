//! Reads what recordings need from the running server.

use alloc::vec::Vec;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ql_sqx::{CapturedEntity, Frame, Input, Shot};

use super::{
    convert::{Vitals, entity_state, input, player_state, shot, visibility},
    lifecycle::{GameState, Observation},
    sidecar::PlayerEntry,
};
use crate::{
    engine::{FindCVar, prelude::*, qagame::game_entity::entity_id_limit},
    gametypes::GameTypes,
    prelude::*,
};

pub(super) fn cvar(main_engine: &QuakeLiveEngine, name: &str) -> Option<String> {
    main_engine.find_cvar(name).map(|cvar| cvar.get_string())
}

pub(super) fn observe(main_engine: &QuakeLiveEngine, has_room: bool) -> Observation {
    Observation {
        now: Instant::now(),
        game_state: cvar(main_engine, "g_gameState").and_then(|state| GameState::from_cvar(&state)),
        human_connected: (0..main_engine.get_max_clients()).any(is_connected_human),
        has_room,
    }
}

fn is_connected_human(client_id: i32) -> bool {
    Client::try_from(client_id).is_ok_and(|client| {
        matches!(
            client.get_state(),
            clientState_t::CS_CONNECTED | clientState_t::CS_PRIMED | clientState_t::CS_ACTIVE
        ) && client.as_ref().netchan.remoteAddress.type_ != netadrtype_t::NA_BOT
    })
}

/// The short name of the game type like `ca`.
pub(super) fn gametype(main_engine: &QuakeLiveEngine) -> String {
    cvar(main_engine, "g_gametype")
        .and_then(|number| number.parse::<i32>().ok())
        .map(|number| GameTypes::from(number).type_short().into())
        .unwrap_or_default()
}

pub(super) fn map_name(main_engine: &QuakeLiveEngine) -> String {
    cvar(main_engine, "mapname").unwrap_or_default()
}

pub(super) fn meta(main_engine: &QuakeLiveEngine, started: SystemTime) -> Vec<(String, String)> {
    let cvar = |name| cvar(main_engine, name).unwrap_or_default();
    let started = started
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    [
        ("map", map_name(main_engine)),
        ("factory", cvar("g_factory")),
        ("sv_fps", cvar("sv_fps")),
        ("shinqlx", env!("CARGO_PKG_VERSION").into()),
        ("started", started.to_string()),
    ]
    .map(|(key, value)| (key.into(), value))
    .into()
}

pub(super) fn configstrings(main_engine: &QuakeLiveEngine) -> Vec<(u16, Vec<u8>)> {
    (0..MAX_CONFIGSTRINGS as u16)
        .filter_map(|index| {
            let value = main_engine.get_configstring_bytes(index);
            (!value.is_empty()).then_some((index, value))
        })
        .collect()
}

/// Everyone who is in the game, spectators included.
pub(super) fn players(main_engine: &QuakeLiveEngine) -> Vec<PlayerEntry> {
    (0..main_engine.get_max_clients())
        .filter_map(|client_id| {
            let client = Client::try_from(client_id).ok()?;
            (client.get_state() == clientState_t::CS_ACTIVE).then(|| PlayerEntry {
                steam_id: client.get_steam_id(),
                name: client.get_name().into_owned(),
                client_id: client_id.try_into().unwrap_or(u8::MAX),
            })
        })
        .collect()
}

/// The state at the end of a game frame.
pub(super) fn capture(main_engine: &QuakeLiveEngine, server_time: i32) -> Frame {
    let entities = (0..entity_id_limit())
        .filter_map(|number| {
            let mut entity = GameEntity::try_from(number).ok()?;
            let entity = entity.as_mut();
            if !bool::from(entity.r.linked) {
                return None;
            }
            let visibility = visibility(entity.r.svFlags, entity.r.singleClient)?;
            Some(CapturedEntity {
                state: entity_state(u16::try_from(number).ok()?, &entity.s),
                visibility,
            })
        })
        .collect();
    let clients = u8::try_from(main_engine.get_max_clients()).unwrap_or_default();
    let players = (0..clients)
        .filter_map(|client| Some((client, player_state(&read_player(client)?))))
        .collect();
    Frame {
        server_time,
        entities,
        players,
    }
}

/// The command the game ran last for a client.
pub(super) fn last_command(client_num: i32) -> Option<Input> {
    let client = u8::try_from(client_num).ok()?;
    let mut entity = GameEntity::try_from(u32::from(client)).ok()?;
    // SAFETY: a non-null client pointer of an entity points to a client of the loaded game module.
    let game_client = unsafe { entity.as_mut().client.as_ref() }?;
    Some(input(client, &game_client.pers.cmd))
}

/// The shot the player of this entity is about to fire.
pub(super) fn shot_of(entity: *mut gentity_t) -> Option<Shot> {
    let client = u8::try_from(GameEntity::try_from(entity).ok()?.get_entity_id()).ok()?;
    Some(shot(client, &read_player(client)?))
}

/// The client number, health and armor of an entity that is a player.
pub(super) fn vitals(entity: &GameEntity) -> Option<(u8, Vitals)> {
    let client = u8::try_from(entity.get_entity_id()).ok()?;
    let armor = entity.get_game_client().ok()?.get_armor();
    let health = entity.get_health();
    Some((client, Vitals { health, armor }))
}

fn read_player(client: u8) -> Option<playerState_t> {
    let mut entity = GameEntity::try_from(u32::from(client)).ok()?;
    // SAFETY: a non-null client pointer of an entity points to a client of the loaded game module.
    let game_client = unsafe { entity.as_mut().client.as_ref() }?;
    (game_client.pers.connected == clientConnected_t::CON_CONNECTED).then(|| game_client.ps.clone())
}
