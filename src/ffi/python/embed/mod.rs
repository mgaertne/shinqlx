mod add_console_command;
mod allow_single_player;
mod callvote;
mod client_command;
mod console_command;
mod console_print;
mod destroy_kamikaze_timers;
mod dev_print_items;
mod drop_holdable;
mod force_vote;
mod force_weapon_respawn_time;
mod get_configstring;
mod get_cvar;
mod get_targetting_entities;
mod get_userinfo;
mod kick;
mod noclip;
mod player_info;
mod player_spawn;
mod player_state;
mod player_stats;
mod players_info;
mod register_handler;
mod remove_dropped_items;
mod replace_items;
mod send_server_command;
mod set_ammo;
mod set_armor;
mod set_configstring;
mod set_cvar;
mod set_cvar_limit;
mod set_flight;
mod set_health;
mod set_holdable;
mod set_invulnerability;
mod set_position;
mod set_powerups;
mod set_privileges;
mod set_score;
mod set_velocity;
mod set_weapon;
mod set_weapons;
mod slay_with_mod;
mod spawn_item;

use crate::MAIN_ENGINE;

pub(crate) use add_console_command::pyshinqlx_add_console_command;
pub(crate) use allow_single_player::pyshinqlx_allow_single_player;
pub(crate) use callvote::pyshinqlx_callvote;
pub(crate) use client_command::pyshinqlx_client_command;
pub(crate) use console_command::pyshinqlx_console_command;
pub(crate) use console_print::pyshinqlx_console_print;
pub(crate) use destroy_kamikaze_timers::pyshinqlx_destroy_kamikaze_timers;
pub(crate) use dev_print_items::pyshinqlx_dev_print_items;
pub(crate) use drop_holdable::pyshinqlx_drop_holdable;
pub(crate) use force_vote::pyshinqlx_force_vote;
pub(crate) use force_weapon_respawn_time::pyshinqlx_force_weapon_respawn_time;
pub(crate) use get_configstring::pyshinqlx_get_configstring;
pub(crate) use get_cvar::pyshinqlx_get_cvar;
pub(crate) use get_targetting_entities::pyshinqlx_get_entity_targets;
pub(crate) use get_userinfo::pyshinqlx_get_userinfo;
pub(crate) use kick::pyshinqlx_kick;
pub(crate) use noclip::pyshinqlx_noclip;
pub(crate) use player_info::pyshinqlx_player_info;
pub(crate) use player_spawn::pyshinqlx_player_spawn;
pub(crate) use player_state::pyshinqlx_player_state;
pub(crate) use player_stats::pyshinqlx_player_stats;
pub(crate) use players_info::pyshinqlx_players_info;
pub(crate) use register_handler::pyshinqlx_register_handler;
pub(crate) use remove_dropped_items::pyshinqlx_remove_dropped_items;
pub(crate) use replace_items::pyshinqlx_replace_items;
pub(crate) use send_server_command::pyshinqlx_send_server_command;
pub(crate) use set_ammo::pyshinqlx_set_ammo;
pub(crate) use set_armor::pyshinqlx_set_armor;
pub(crate) use set_configstring::pyshinqlx_set_configstring;
pub(crate) use set_cvar::pyshinqlx_set_cvar;
pub(crate) use set_cvar_limit::pyshinqlx_set_cvar_limit;
pub(crate) use set_flight::pyshinqlx_set_flight;
pub(crate) use set_health::pyshinqlx_set_health;
pub(crate) use set_holdable::pyshinqlx_set_holdable;
pub(crate) use set_invulnerability::pyshinqlx_set_invulnerability;
pub(crate) use set_position::pyshinqlx_set_position;
pub(crate) use set_powerups::pyshinqlx_set_powerups;
pub(crate) use set_privileges::pyshinqlx_set_privileges;
pub(crate) use set_score::pyshinqlx_set_score;
pub(crate) use set_velocity::pyshinqlx_set_velocity;
pub(crate) use set_weapon::pyshinqlx_set_weapon;
pub(crate) use set_weapons::pyshinqlx_set_weapons;
pub(crate) use slay_with_mod::pyshinqlx_slay_with_mod;
pub(crate) use spawn_item::pyshinqlx_spawn_item;

use pyo3::exceptions::{PyEnvironmentError, PyValueError};
use pyo3::{PyResult, Python};

fn validate_client_id(py: Python<'_>, client_id: i32) -> PyResult<()> {
    py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let maxclients = main_engine.get_max_clients();
        if !(0..maxclients).contains(&client_id) {
            return Err(PyValueError::new_err(format!(
                "client_id needs to be a number from 0 to {}, or None.",
                maxclients - 1
            )));
        }
        Ok(())
    })
}
