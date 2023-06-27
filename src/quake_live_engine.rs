use crate::client::Client;
use crate::cvar::CVar;
use crate::game_entity::GameEntity;
use crate::game_item::GameItem;
use crate::hooks::{
    CLIENT_CONNECT_TRAMPOLINE, CLIENT_SPAWN_TRAMPOLINE, CMD_ADDCOMMAND_DETOUR,
    COM_PRINTF_TRAMPOLINE, G_DAMAGE_TRAMPOLINE, G_START_KAMIKAZE_TRAMPOLINE,
    SV_CLIENTENTERWORLD_DETOUR, SV_EXECUTECLIENTCOMMAND_DETOUR, SV_SENDSERVERCOMMAND_TRAMPOLINE,
    SV_SETCONFGISTRING_DETOUR, SV_SPAWNSERVER_DETOUR, SYS_SETMODULEOFFSET_DETOUR,
};
use crate::quake_types::meansOfDeath_t::{
    MOD_BFG, MOD_BFG_SPLASH, MOD_CHAINGUN, MOD_CRUSH, MOD_FALLING, MOD_GAUNTLET, MOD_GRAPPLE,
    MOD_GRENADE, MOD_GRENADE_SPLASH, MOD_HMG, MOD_JUICED, MOD_KAMIKAZE, MOD_LAVA, MOD_LIGHTNING,
    MOD_LIGHTNING_DISCHARGE, MOD_MACHINEGUN, MOD_NAIL, MOD_PLASMA, MOD_PLASMA_SPLASH,
    MOD_PROXIMITY_MINE, MOD_RAILGUN, MOD_RAILGUN_HEADSHOT, MOD_ROCKET, MOD_ROCKET_SPLASH,
    MOD_SHOTGUN, MOD_SLIME, MOD_SUICIDE, MOD_SWITCH_TEAMS, MOD_TARGET_LASER, MOD_TELEFRAG,
    MOD_THAW, MOD_TRIGGER_HURT, MOD_UNKNOWN, MOD_WATER,
};
use crate::quake_types::powerup_t::{
    PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
};
use crate::quake_types::privileges_t::{PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT};
use crate::quake_types::weapon_t::{
    WP_BFG, WP_CHAINGUN, WP_GAUNTLET, WP_GRAPPLING_HOOK, WP_GRENADE_LAUNCHER, WP_HANDS, WP_HMG,
    WP_LIGHTNING, WP_MACHINEGUN, WP_NAILGUN, WP_NONE, WP_NUM_WEAPONS, WP_PLASMAGUN,
    WP_PROX_LAUNCHER, WP_RAILGUN, WP_ROCKET_LAUNCHER, WP_SHOTGUN,
};
use crate::quake_types::{
    cbufExec_t, cvar_t, entity_event_t, gentity_t, gitem_t, meansOfDeath_t, powerup_t,
    privileges_t, qboolean, usercmd_t, vec3_t, weapon_t, MAX_STRING_CHARS,
};
use crate::{QuakeLiveFunction, STATIC_FUNCTION_MAP};
#[cfg(test)]
use mockall::*;
use std::ffi::{c_char, c_float, c_int, CStr, CString};
use std::ops::Not;

impl From<qboolean> for c_int {
    fn from(value: qboolean) -> Self {
        match value {
            qboolean::qtrue => 1,
            _ => 0,
        }
    }
}

impl From<qboolean> for bool {
    fn from(value: qboolean) -> Self {
        matches!(value, qboolean::qtrue)
    }
}

impl From<bool> for qboolean {
    fn from(value: bool) -> Self {
        match value {
            true => qboolean::qtrue,
            _ => qboolean::qfalse,
        }
    }
}

impl Not for qboolean {
    type Output = qboolean;

    fn not(self) -> Self::Output {
        match self {
            qboolean::qtrue => qboolean::qfalse,
            _ => qboolean::qtrue,
        }
    }
}

#[cfg(test)]
pub(crate) mod qboolean_tests {
    use crate::quake_types::qboolean;
    use pretty_assertions::assert_eq;
    use std::ffi::c_int;

    #[test]
    pub(crate) fn qboolean_as_c_int() {
        assert_eq!(c_int::from(qboolean::qtrue), 1);
        assert_eq!(c_int::from(qboolean::qfalse), 0);
    }

    #[test]
    pub(crate) fn qboolean_as_bool() {
        assert_eq!(bool::from(qboolean::qtrue), true);
        assert_eq!(bool::from(qboolean::qfalse), false);
    }

    #[test]
    pub(crate) fn qboolean_from_bool() {
        assert_eq!(qboolean::from(true), qboolean::qtrue);
        assert_eq!(qboolean::from(false), qboolean::qfalse);
    }

    #[test]
    pub(crate) fn qboolean_negation() {
        assert_eq!(!qboolean::qtrue, qboolean::qfalse);
        assert_eq!(!qboolean::qfalse, qboolean::qtrue);
    }
}

impl From<i32> for privileges_t {
    fn from(value: i32) -> Self {
        match value {
            -1 => PRIV_BANNED,
            0x1 => PRIV_MOD,
            0x2 => PRIV_ADMIN,
            0x3 => PRIV_ROOT,
            _ => PRIV_NONE,
        }
    }
}

#[cfg(test)]
pub(crate) mod privileges_tests {
    use crate::quake_types::privileges_t;
    use crate::quake_types::privileges_t::{
        PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT,
    };
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn privileges_from_integer() {
        assert_eq!(privileges_t::from(-1), PRIV_BANNED);
        assert_eq!(privileges_t::from(1), PRIV_MOD);
        assert_eq!(privileges_t::from(2), PRIV_ADMIN);
        assert_eq!(privileges_t::from(3), PRIV_ROOT);
        assert_eq!(privileges_t::from(0), PRIV_NONE);
        assert_eq!(privileges_t::from(666), PRIV_NONE);
    }
}

impl TryFrom<usize> for powerup_t {
    type Error = String;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PW_QUAD),
            1 => Ok(PW_BATTLESUIT),
            2 => Ok(PW_HASTE),
            3 => Ok(PW_INVIS),
            4 => Ok(PW_REGEN),
            5 => Ok(PW_INVULNERABILITY),
            _ => Err("invalid power up".into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod powerup_t_tests {
    use crate::quake_types::powerup_t;
    use crate::quake_types::powerup_t::{
        PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
    };
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn powerup_t_from_integer() {
        assert_eq!(powerup_t::try_from(0), Ok(PW_QUAD));
        assert_eq!(powerup_t::try_from(1), Ok(PW_BATTLESUIT));
        assert_eq!(powerup_t::try_from(2), Ok(PW_HASTE));
        assert_eq!(powerup_t::try_from(3), Ok(PW_INVIS));
        assert_eq!(powerup_t::try_from(4), Ok(PW_REGEN));
        assert_eq!(powerup_t::try_from(5), Ok(PW_INVULNERABILITY));
        assert_eq!(
            powerup_t::try_from(666),
            Err("invalid power up".to_string())
        );
    }

    #[test]
    pub(crate) fn powerup_t_from_usize() {
        assert_eq!(powerup_t::try_from(0usize), Ok(PW_QUAD));
        assert_eq!(powerup_t::try_from(1usize), Ok(PW_BATTLESUIT));
        assert_eq!(powerup_t::try_from(2usize), Ok(PW_HASTE));
        assert_eq!(powerup_t::try_from(3usize), Ok(PW_INVIS));
        assert_eq!(powerup_t::try_from(4usize), Ok(PW_REGEN));
        assert_eq!(powerup_t::try_from(5usize), Ok(PW_INVULNERABILITY));
        assert_eq!(
            powerup_t::try_from(666usize),
            Err("invalid power up".to_string())
        );
    }
}

impl From<weapon_t> for i32 {
    fn from(value: weapon_t) -> Self {
        match value {
            WP_NUM_WEAPONS => 0,
            _ => value as i32,
        }
    }
}

impl TryFrom<i32> for weapon_t {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WP_NONE),
            1 => Ok(WP_GAUNTLET),
            2 => Ok(WP_MACHINEGUN),
            3 => Ok(WP_SHOTGUN),
            4 => Ok(WP_GRENADE_LAUNCHER),
            5 => Ok(WP_ROCKET_LAUNCHER),
            6 => Ok(WP_LIGHTNING),
            7 => Ok(WP_RAILGUN),
            8 => Ok(WP_PLASMAGUN),
            9 => Ok(WP_BFG),
            10 => Ok(WP_GRAPPLING_HOOK),
            11 => Ok(WP_NAILGUN),
            12 => Ok(WP_PROX_LAUNCHER),
            13 => Ok(WP_CHAINGUN),
            14 => Ok(WP_HMG),
            15 => Ok(WP_HANDS),
            _ => Err("invalid weapon".into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod weapon_t_tests {
    use crate::quake_types::weapon_t;
    use crate::quake_types::weapon_t::{
        WP_BFG, WP_CHAINGUN, WP_GAUNTLET, WP_GRAPPLING_HOOK, WP_GRENADE_LAUNCHER, WP_HANDS, WP_HMG,
        WP_LIGHTNING, WP_MACHINEGUN, WP_NAILGUN, WP_NONE, WP_NUM_WEAPONS, WP_PLASMAGUN,
        WP_PROX_LAUNCHER, WP_RAILGUN, WP_ROCKET_LAUNCHER, WP_SHOTGUN,
    };
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn integer_from_weapon_t() {
        assert_eq!(i32::from(WP_NONE), 0);
        assert_eq!(i32::from(WP_GAUNTLET), 1);
        assert_eq!(i32::from(WP_MACHINEGUN), 2);
        assert_eq!(i32::from(WP_SHOTGUN), 3);
        assert_eq!(i32::from(WP_GRENADE_LAUNCHER), 4);
        assert_eq!(i32::from(WP_ROCKET_LAUNCHER), 5);
        assert_eq!(i32::from(WP_LIGHTNING), 6);
        assert_eq!(i32::from(WP_RAILGUN), 7);
        assert_eq!(i32::from(WP_PLASMAGUN), 8);
        assert_eq!(i32::from(WP_BFG), 9);
        assert_eq!(i32::from(WP_GRAPPLING_HOOK), 10);
        assert_eq!(i32::from(WP_NAILGUN), 11);
        assert_eq!(i32::from(WP_PROX_LAUNCHER), 12);
        assert_eq!(i32::from(WP_CHAINGUN), 13);
        assert_eq!(i32::from(WP_HMG), 14);
        assert_eq!(i32::from(WP_HANDS), 15);
        assert_eq!(i32::from(WP_NUM_WEAPONS), 0);
    }

    #[test]
    pub(crate) fn weapon_t_from_integer() {
        assert_eq!(weapon_t::try_from(0), Ok(WP_NONE));
        assert_eq!(weapon_t::try_from(1), Ok(WP_GAUNTLET));
        assert_eq!(weapon_t::try_from(2), Ok(WP_MACHINEGUN));
        assert_eq!(weapon_t::try_from(3), Ok(WP_SHOTGUN));
        assert_eq!(weapon_t::try_from(4), Ok(WP_GRENADE_LAUNCHER));
        assert_eq!(weapon_t::try_from(5), Ok(WP_ROCKET_LAUNCHER));
        assert_eq!(weapon_t::try_from(6), Ok(WP_LIGHTNING));
        assert_eq!(weapon_t::try_from(7), Ok(WP_RAILGUN));
        assert_eq!(weapon_t::try_from(8), Ok(WP_PLASMAGUN));
        assert_eq!(weapon_t::try_from(9), Ok(WP_BFG));
        assert_eq!(weapon_t::try_from(10), Ok(WP_GRAPPLING_HOOK));
        assert_eq!(weapon_t::try_from(11), Ok(WP_NAILGUN));
        assert_eq!(weapon_t::try_from(12), Ok(WP_PROX_LAUNCHER));
        assert_eq!(weapon_t::try_from(13), Ok(WP_CHAINGUN));
        assert_eq!(weapon_t::try_from(14), Ok(WP_HMG));
        assert_eq!(weapon_t::try_from(15), Ok(WP_HANDS));
        assert_eq!(weapon_t::try_from(16), Err("invalid weapon".to_string()));
        assert_eq!(weapon_t::try_from(-1), Err("invalid weapon".to_string()));
        assert_eq!(weapon_t::try_from(666), Err("invalid weapon".to_string()));
    }
}

impl TryFrom<i32> for meansOfDeath_t {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MOD_UNKNOWN),
            1 => Ok(MOD_SHOTGUN),
            2 => Ok(MOD_GAUNTLET),
            3 => Ok(MOD_MACHINEGUN),
            4 => Ok(MOD_GRENADE),
            5 => Ok(MOD_GRENADE_SPLASH),
            6 => Ok(MOD_ROCKET),
            7 => Ok(MOD_ROCKET_SPLASH),
            8 => Ok(MOD_PLASMA),
            9 => Ok(MOD_PLASMA_SPLASH),
            10 => Ok(MOD_RAILGUN),
            11 => Ok(MOD_LIGHTNING),
            12 => Ok(MOD_BFG),
            13 => Ok(MOD_BFG_SPLASH),
            14 => Ok(MOD_WATER),
            15 => Ok(MOD_SLIME),
            16 => Ok(MOD_LAVA),
            17 => Ok(MOD_CRUSH),
            18 => Ok(MOD_TELEFRAG),
            19 => Ok(MOD_FALLING),
            20 => Ok(MOD_SUICIDE),
            21 => Ok(MOD_TARGET_LASER),
            22 => Ok(MOD_TRIGGER_HURT),
            23 => Ok(MOD_NAIL),
            24 => Ok(MOD_CHAINGUN),
            25 => Ok(MOD_PROXIMITY_MINE),
            26 => Ok(MOD_KAMIKAZE),
            27 => Ok(MOD_JUICED),
            28 => Ok(MOD_GRAPPLE),
            29 => Ok(MOD_SWITCH_TEAMS),
            30 => Ok(MOD_THAW),
            31 => Ok(MOD_LIGHTNING_DISCHARGE),
            32 => Ok(MOD_HMG),
            33 => Ok(MOD_RAILGUN_HEADSHOT),
            _ => Err("invalid means of death".into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod meansofdeath_t_tests {
    use crate::quake_types::meansOfDeath_t;
    use crate::quake_types::meansOfDeath_t::{
        MOD_BFG, MOD_BFG_SPLASH, MOD_CHAINGUN, MOD_CRUSH, MOD_FALLING, MOD_GAUNTLET, MOD_GRAPPLE,
        MOD_GRENADE, MOD_GRENADE_SPLASH, MOD_HMG, MOD_JUICED, MOD_KAMIKAZE, MOD_LAVA,
        MOD_LIGHTNING, MOD_LIGHTNING_DISCHARGE, MOD_MACHINEGUN, MOD_NAIL, MOD_PLASMA,
        MOD_PLASMA_SPLASH, MOD_PROXIMITY_MINE, MOD_RAILGUN, MOD_RAILGUN_HEADSHOT, MOD_ROCKET,
        MOD_ROCKET_SPLASH, MOD_SHOTGUN, MOD_SLIME, MOD_SUICIDE, MOD_SWITCH_TEAMS, MOD_TARGET_LASER,
        MOD_TELEFRAG, MOD_THAW, MOD_TRIGGER_HURT, MOD_UNKNOWN, MOD_WATER,
    };
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn meansofdeath_t_from_integer() {
        assert_eq!(meansOfDeath_t::try_from(0), Ok(MOD_UNKNOWN));
        assert_eq!(meansOfDeath_t::try_from(1), Ok(MOD_SHOTGUN));
        assert_eq!(meansOfDeath_t::try_from(2), Ok(MOD_GAUNTLET));
        assert_eq!(meansOfDeath_t::try_from(3), Ok(MOD_MACHINEGUN));
        assert_eq!(meansOfDeath_t::try_from(4), Ok(MOD_GRENADE));
        assert_eq!(meansOfDeath_t::try_from(5), Ok(MOD_GRENADE_SPLASH));
        assert_eq!(meansOfDeath_t::try_from(6), Ok(MOD_ROCKET));
        assert_eq!(meansOfDeath_t::try_from(7), Ok(MOD_ROCKET_SPLASH));
        assert_eq!(meansOfDeath_t::try_from(8), Ok(MOD_PLASMA));
        assert_eq!(meansOfDeath_t::try_from(9), Ok(MOD_PLASMA_SPLASH));
        assert_eq!(meansOfDeath_t::try_from(10), Ok(MOD_RAILGUN));
        assert_eq!(meansOfDeath_t::try_from(11), Ok(MOD_LIGHTNING));
        assert_eq!(meansOfDeath_t::try_from(12), Ok(MOD_BFG));
        assert_eq!(meansOfDeath_t::try_from(13), Ok(MOD_BFG_SPLASH));
        assert_eq!(meansOfDeath_t::try_from(14), Ok(MOD_WATER));
        assert_eq!(meansOfDeath_t::try_from(15), Ok(MOD_SLIME));
        assert_eq!(meansOfDeath_t::try_from(16), Ok(MOD_LAVA));
        assert_eq!(meansOfDeath_t::try_from(17), Ok(MOD_CRUSH));
        assert_eq!(meansOfDeath_t::try_from(18), Ok(MOD_TELEFRAG));
        assert_eq!(meansOfDeath_t::try_from(19), Ok(MOD_FALLING));
        assert_eq!(meansOfDeath_t::try_from(20), Ok(MOD_SUICIDE));
        assert_eq!(meansOfDeath_t::try_from(21), Ok(MOD_TARGET_LASER));
        assert_eq!(meansOfDeath_t::try_from(22), Ok(MOD_TRIGGER_HURT));
        assert_eq!(meansOfDeath_t::try_from(23), Ok(MOD_NAIL));
        assert_eq!(meansOfDeath_t::try_from(24), Ok(MOD_CHAINGUN));
        assert_eq!(meansOfDeath_t::try_from(25), Ok(MOD_PROXIMITY_MINE));
        assert_eq!(meansOfDeath_t::try_from(26), Ok(MOD_KAMIKAZE));
        assert_eq!(meansOfDeath_t::try_from(27), Ok(MOD_JUICED));
        assert_eq!(meansOfDeath_t::try_from(28), Ok(MOD_GRAPPLE));
        assert_eq!(meansOfDeath_t::try_from(29), Ok(MOD_SWITCH_TEAMS));
        assert_eq!(meansOfDeath_t::try_from(30), Ok(MOD_THAW));
        assert_eq!(meansOfDeath_t::try_from(31), Ok(MOD_LIGHTNING_DISCHARGE));
        assert_eq!(meansOfDeath_t::try_from(32), Ok(MOD_HMG));
        assert_eq!(meansOfDeath_t::try_from(33), Ok(MOD_RAILGUN_HEADSHOT));
        assert_eq!(
            meansOfDeath_t::try_from(-1),
            Err("invalid means of death".to_string())
        );
        assert_eq!(
            meansOfDeath_t::try_from(666),
            Err("invalid means of death".to_string())
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuakeLiveEngineError {
    NullPointerPassed(String),
    EntityNotFound(String),
    InvalidId(i32),
    ClientNotFound(String),
}

#[derive(Default)]
pub(crate) struct QuakeLiveEngine {}

#[cfg_attr(test, automock)]
pub(crate) trait FindCVar {
    fn find_cvar(&self, name: &str) -> Option<CVar>;
}

impl FindCVar for QuakeLiveEngine {
    fn find_cvar(&self, name: &str) -> Option<CVar> {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cvar_FindVar) }) else
        {
            return None;
        };
        let original_func: extern "C" fn(*const c_char) -> *const cvar_t =
            unsafe { std::mem::transmute(*func_pointer) };
        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let cvar = original_func(c_name.as_ptr());
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CbufExecuteText {
    fn cbuf_execute_text(&self, exec_t: cbufExec_t, new_tags: &str);
}

impl CbufExecuteText for QuakeLiveEngine {
    fn cbuf_execute_text(&self, exec_t: cbufExec_t, new_tags: &str) {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cbuf_ExecuteText) }) else
        {
            return;
        };
        let original_func: extern "C" fn(cbufExec_t, *const c_char) =
            unsafe { std::mem::transmute(*func_pointer) };
        let Ok(c_tags) = CString::new(new_tags) else {
            return;
        };
        original_func(exec_t, c_tags.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait AddCommand {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
}

impl AddCommand for QuakeLiveEngine {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn()) {
        let Ok(c_cmd) = CString::new(cmd) else {
            return;
        };
        unsafe { CMD_ADDCOMMAND_DETOUR.call(c_cmd.as_ptr(), func) };
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetModuleOffset {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
}

impl SetModuleOffset for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn()) {
        let Ok(c_module_name) = CString::new(module_name) else {
            return;
        };
        unsafe { SYS_SETMODULEOFFSET_DETOUR.call(c_module_name.as_ptr(), offset) };
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait InitGame {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32);
}

impl InitGame for QuakeLiveEngine {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32) {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::G_InitGame) }) else {
            return;
        };

        let original_func: extern "C" fn(c_int, c_int, c_int) =
            unsafe { std::mem::transmute(*func_pointer) };

        original_func(level_time, random_seed, restart);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ExecuteClientCommand {
    #[allow(clippy::needless_lifetimes)]
    fn execute_client_command<'a>(
        &self,
        client: Option<&'a mut Client>,
        cmd: &str,
        client_ok: bool,
    );
}

impl ExecuteClientCommand for QuakeLiveEngine {
    fn execute_client_command(&self, client: Option<&mut Client>, cmd: &str, client_ok: bool) {
        let Ok(c_command) = CString::new(cmd) else {
            return;
        };
        match client {
            Some(safe_client) => unsafe {
                SV_EXECUTECLIENTCOMMAND_DETOUR.call(
                    safe_client.client_t,
                    c_command.as_ptr(),
                    client_ok.into(),
                )
            },
            None => unsafe {
                SV_EXECUTECLIENTCOMMAND_DETOUR.call(
                    std::ptr::null_mut(),
                    c_command.as_ptr(),
                    client_ok.into(),
                )
            },
        }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SendServerCommand {
    fn send_server_command(&self, client: Option<Client>, command: &str);
}

impl SendServerCommand for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<Client>, command: &str) {
        let Some(trampoline_func) = SV_SENDSERVERCOMMAND_TRAMPOLINE.get() else {
            return;
        };

        let Ok(c_command) = CString::new(command) else {
            return;
        };
        match client {
            Some(safe_client) => trampoline_func(safe_client.client_t, c_command.as_ptr()),
            None => trampoline_func(std::ptr::null(), c_command.as_ptr()),
        }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientEnterWorld {
    fn client_enter_world(&self, client: &mut Client, cmd: *const usercmd_t);
}

impl ClientEnterWorld for QuakeLiveEngine {
    fn client_enter_world(&self, client: &mut Client, cmd: *const usercmd_t) {
        unsafe { SV_CLIENTENTERWORLD_DETOUR.call(client.client_t, cmd) };
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetConfigstring {
    fn set_configstring(&self, index: &u32, value: &str);
}

impl SetConfigstring for QuakeLiveEngine {
    fn set_configstring(&self, index: &u32, value: &str) {
        let Ok(c_value) = CString::new(value) else {
            return;
        };
        let Ok(c_index) = c_int::try_from(index.to_owned()) else {
            return;
        };
        unsafe { SV_SETCONFGISTRING_DETOUR.call(c_index, c_value.as_ptr()) };
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        let Some(trampoline_func) = COM_PRINTF_TRAMPOLINE.get() else {
            return;
        };
        let Ok(c_msg) = CString::new(msg) else {
            return;
        };
        trampoline_func(c_msg.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SpawnServer {
    fn spawn_server(&self, server: &str, kill_bots: bool);
}

impl SpawnServer for QuakeLiveEngine {
    fn spawn_server(&self, server: &str, kill_bots: bool) {
        let Ok(c_server) = CString::new(server) else {
            return;
        };
        unsafe { SV_SPAWNSERVER_DETOUR.call(c_server.as_ptr(), kill_bots.into()) };
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait RunFrame {
    fn run_frame(&self, time: i32);
}

impl RunFrame for QuakeLiveEngine {
    fn run_frame(&self, time: i32) {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::G_RunFrame) }) else {
            return;
        };

        let original_func: extern "C" fn(c_int) = unsafe { std::mem::transmute(*func_pointer) };

        original_func(time);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientConnect {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> *const c_char;
}

impl ClientConnect for QuakeLiveEngine {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> *const c_char {
        let Some(trampoline_func) = (unsafe { CLIENT_CONNECT_TRAMPOLINE.as_ref() }) else {
            return std::ptr::null_mut();
        };
        trampoline_func(client_num, first_time.into(), is_bot.into())
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientSpawn {
    fn client_spawn(&self, ent: &mut GameEntity);
}

impl ClientSpawn for QuakeLiveEngine {
    fn client_spawn(&self, ent: &mut GameEntity) {
        let Some(trampoline_func) = (unsafe { CLIENT_SPAWN_TRAMPOLINE.as_ref() }) else {
            return;
        };
        trampoline_func(ent.gentity_t);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cmd_Args) }) else {
            return None;
        };
        let original_func: extern "C" fn() -> *const c_char =
            unsafe { std::mem::transmute(*func_pointer) };

        let cmd_args = original_func();
        if cmd_args.is_null() {
            return None;
        }
        let cmd_args = unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy();
        Some(cmd_args.to_string())
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cmd_Argc) }) else
        {
            return 0;
        };
        let original_func: extern "C" fn() -> c_int = unsafe { std::mem::transmute(*func_pointer) };
        original_func()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgv {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
}

impl CmdArgv for QuakeLiveEngine {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str> {
        if argno < 0 {
            return None;
        }
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cmd_Argc) }) else {
            return None;
        };
        let original_func: extern "C" fn(c_int) -> *const c_char =
            unsafe { std::mem::transmute(*func_pointer) };

        let cmd_argv = original_func(argno);
        if cmd_argv.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(cmd_argv).to_str().ok() }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GameAddEvent {
    fn game_add_event(&self, game_entity: &GameEntity, event: entity_event_t, event_param: i32);
}

impl GameAddEvent for QuakeLiveEngine {
    fn game_add_event(&self, game_entity: &GameEntity, event: entity_event_t, event_param: i32) {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::G_RunFrame) }) else {
            return;
        };

        let original_func: extern "C" fn(*const gentity_t, entity_event_t, c_int) =
            unsafe { std::mem::transmute(*func_pointer) };

        original_func(
            game_entity.gentity_t as *const gentity_t,
            event,
            event_param,
        );
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ConsoleCommand {
    fn execute_console_command(&self, cmd: &str);
}

impl ConsoleCommand for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: &str) {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cmd_ExecuteString) }) else {
            return;
        };
        let original_func: extern "C" fn(*const c_char) =
            unsafe { std::mem::transmute(*func_pointer) };

        let Ok(c_cmd) = CString::new(cmd) else {
            return;
        };
        original_func(c_cmd.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GetCVar {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
}

impl GetCVar for QuakeLiveEngine {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar> {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cvar_Get) }) else {
            return None;
        };
        let original_func: extern "C" fn(*const c_char, *const c_char, c_int) -> *const cvar_t =
            unsafe { std::mem::transmute(*func_pointer) };

        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let flags_value = flags.unwrap_or_default();
        let cvar = original_func(c_name.as_ptr(), c_value.as_ptr(), flags_value);
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetCVarForced {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar>;
}

impl SetCVarForced for QuakeLiveEngine {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar> {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cvar_Set2) }) else
        {
            return None;
        };
        let original_func: extern "C" fn(*const c_char, *const c_char, qboolean) -> *const cvar_t =
            unsafe { std::mem::transmute(*func_pointer) };
        let Ok(c_name) = CString::new(name) else {
                return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let cvar = original_func(c_name.as_ptr(), c_value.as_ptr(), forced.into());
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetCVarLimit {
    fn set_cvar_limit(
        &self,
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar>;
}

impl SetCVarLimit for QuakeLiveEngine {
    fn set_cvar_limit(
        &self,
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar> {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::Cvar_GetLimit) }) else {
            return None;
        };
        let original_func: extern "C" fn(
            *const c_char,
            *const c_char,
            *const c_char,
            *const c_char,
            c_int,
        ) -> *const cvar_t = unsafe { std::mem::transmute(*func_pointer) };

        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let Ok(c_min) = CString::new(min) else {
            return None;
        };
        let Ok(c_max) = CString::new(max) else {
            return None;
        };
        let flags_value = flags.unwrap_or_default();
        let cvar = original_func(
            c_name.as_ptr(),
            c_value.as_ptr(),
            c_min.as_ptr(),
            c_max.as_ptr(),
            flags_value,
        );
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GetConfigstring {
    fn get_configstring(&self, index: u32) -> String;
}

impl GetConfigstring for QuakeLiveEngine {
    fn get_configstring(&self, index: u32) -> String {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::SV_GetConfigstring) }) else
        {
            return "".into();
        };
        let original_func: extern "C" fn(c_int, *mut c_char, c_int) =
            unsafe { std::mem::transmute(*func_pointer) };

        let mut buffer: [u8; MAX_STRING_CHARS as usize] = [0; MAX_STRING_CHARS as usize];
        original_func(
            index as c_int,
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len() as c_int,
        );
        let Ok(result) = CStr::from_bytes_until_nul(&buffer) else {
            return "".into();
        };
        result.to_string_lossy().into()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait RegisterDamage {
    #[allow(clippy::too_many_arguments)]
    fn register_damage(
        &self,
        target: *const gentity_t,
        inflictor: *const gentity_t,
        attacker: *const gentity_t,
        dir: *const c_float,
        pos: *const c_float,
        damage: c_int,
        dflags: c_int,
        means_of_death: c_int,
    );
}

impl RegisterDamage for QuakeLiveEngine {
    fn register_damage(
        &self,
        target: *const gentity_t,
        inflictor: *const gentity_t,
        attacker: *const gentity_t,
        dir: *const c_float,
        pos: *const c_float,
        damage: c_int,
        dflags: c_int,
        means_of_death: c_int,
    ) {
        let Some(trampoline_func) = (unsafe { G_DAMAGE_TRAMPOLINE.as_ref() }) else {
            return;
        };
        trampoline_func(
            target,
            inflictor,
            attacker,
            dir,
            pos,
            damage,
            dflags,
            means_of_death,
        );
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait FreeEntity {
    fn free_entity(&self, gentity: *mut gentity_t);
}

impl FreeEntity for QuakeLiveEngine {
    fn free_entity(&self, gentity: *mut gentity_t) {
        let Some(func_pointer) = (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::G_FreeEntity) }) else {
            return;
        };

        let original_func: extern "C" fn(*mut gentity_t) =
            unsafe { std::mem::transmute(*func_pointer) };

        original_func(gentity);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait LaunchItem {
    fn launch_item(&self, gitem: &GameItem, origin: vec3_t, velocity: vec3_t) -> GameEntity;
}

impl LaunchItem for QuakeLiveEngine {
    fn launch_item(&self, gitem: &GameItem, origin: vec3_t, velocity: vec3_t) -> GameEntity {
        let Some(func_pointer) =
            (unsafe { STATIC_FUNCTION_MAP.get(&QuakeLiveFunction::LaunchItem) }) else {
            panic!("LaunchItem not found!");   
        };

        let original_func: extern "C" fn(*const gitem_t, vec3_t, vec3_t) -> *mut gentity_t =
            unsafe { std::mem::transmute(*func_pointer) };

        GameEntity::try_from(original_func(gitem.gitem_t, origin, velocity)).unwrap()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait StartKamikaze {
    fn start_kamikaze(&self, gentity: &GameEntity);
}

impl StartKamikaze for QuakeLiveEngine {
    fn start_kamikaze(&self, gentity: &GameEntity) {
        let Some(trampoline_func) = (unsafe { G_START_KAMIKAZE_TRAMPOLINE.as_ref() }) else {
            return;
        };
        trampoline_func(gentity.gentity_t as *const gentity_t);
    }
}
