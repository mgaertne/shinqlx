use crate::hooks::{shinqlx_set_configstring, shinqlx_sv_setconfigstring};
use crate::hooks::{
    CMD_ADDCOMMAND_DETOUR, SV_CLIENTENTERWORLD_DETOUR, SV_DROPCLIENT_DETOUR,
    SV_EXECUTECLIENTCOMMAND_DETOUR, SV_SETCONFGISTRING_DETOUR, SV_SPAWNSERVER_DETOUR,
    SYS_SETMODULEOFFSET_DETOUR,
};
use crate::quake_live_engine::QuakeLiveEngineError::{
    ClientNotFound, EntityNotFound, InvalidId, NullPointerPassed,
};
use crate::quake_types::clientConnected_t::CON_DISCONNECTED;
use crate::quake_types::entityType_t::ET_ITEM;
use crate::quake_types::entity_event_t::EV_ITEM_RESPAWN;
use crate::quake_types::itemType_t::IT_WEAPON;
use crate::quake_types::meansOfDeath_t::{
    MOD_BFG, MOD_BFG_SPLASH, MOD_CHAINGUN, MOD_CRUSH, MOD_FALLING, MOD_GAUNTLET, MOD_GRAPPLE,
    MOD_GRENADE, MOD_GRENADE_SPLASH, MOD_HMG, MOD_JUICED, MOD_KAMIKAZE, MOD_LAVA, MOD_LIGHTNING,
    MOD_LIGHTNING_DISCHARGE, MOD_MACHINEGUN, MOD_NAIL, MOD_PLASMA, MOD_PLASMA_SPLASH,
    MOD_PROXIMITY_MINE, MOD_RAILGUN, MOD_RAILGUN_HEADSHOT, MOD_ROCKET, MOD_ROCKET_SPLASH,
    MOD_SHOTGUN, MOD_SLIME, MOD_SUICIDE, MOD_SWITCH_TEAMS, MOD_TARGET_LASER, MOD_TELEFRAG,
    MOD_THAW, MOD_TRIGGER_HURT, MOD_UNKNOWN, MOD_WATER,
};
use crate::quake_types::persistantFields_t::PERS_ROUND_SCORE;
use crate::quake_types::pmtype_t::{PM_FREEZE, PM_NORMAL};
use crate::quake_types::powerup_t::{
    PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
};
use crate::quake_types::privileges_t::{PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT};
use crate::quake_types::statIndex_t::{
    STAT_ARMOR, STAT_CUR_FLIGHT_FUEL, STAT_FLIGHT_REFUEL, STAT_FLIGHT_THRUST, STAT_HOLDABLE_ITEM,
    STAT_MAX_FLIGHT_FUEL, STAT_WEAPONS,
};
use crate::quake_types::team_t::TEAM_SPECTATOR;
use crate::quake_types::voteState_t::{VOTE_NO, VOTE_PENDING, VOTE_YES};
use crate::quake_types::weapon_t::{
    WP_BFG, WP_CHAINGUN, WP_GAUNTLET, WP_GRAPPLING_HOOK, WP_GRENADE_LAUNCHER, WP_HANDS, WP_HMG,
    WP_LIGHTNING, WP_MACHINEGUN, WP_NAILGUN, WP_NONE, WP_NUM_WEAPONS, WP_PLASMAGUN,
    WP_PROX_LAUNCHER, WP_RAILGUN, WP_ROCKET_LAUNCHER, WP_SHOTGUN,
};
use crate::quake_types::{
    cbufExec_t, clientState_t, client_t, cvar_t, entity_event_t, gclient_t, gentity_t, gitem_t,
    level_locals_t, meansOfDeath_t, pmtype_t, powerup_t, privileges_t, qboolean, serverStatic_t,
    team_t, trace_t, usercmd_t, vec3_t, weapon_t, CS_ITEMS, CS_VOTE_NO, CS_VOTE_STRING,
    CS_VOTE_TIME, CS_VOTE_YES, DAMAGE_NO_PROTECTION, EF_KAMIKAZE, EF_TALK, FL_DROPPED_ITEM,
    MAX_CLIENTS, MAX_GENTITIES, MODELINDEX_KAMIKAZE,
};
use crate::SV_MAXCLIENTS;
use std::f32::consts::PI;
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
        assert!(bool::from(qboolean::qtrue));
        assert!(!bool::from(qboolean::qfalse));
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

impl From<privileges_t> for i32 {
    fn from(value: privileges_t) -> Self {
        value as i32
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

impl From<pmtype_t> for i32 {
    fn from(value: pmtype_t) -> Self {
        value as i32
    }
}

impl From<clientState_t> for i32 {
    fn from(value: clientState_t) -> Self {
        value as i32
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

impl From<team_t> for i32 {
    fn from(value: team_t) -> Self {
        value as i32
    }
}

impl From<meansOfDeath_t> for i32 {
    fn from(value: meansOfDeath_t) -> Self {
        value as i32
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

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameClient {
    game_client: &'static mut gclient_t,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuakeLiveEngineError {
    NullPointerPassed(String),
    EntityNotFound(String),
    InvalidId(i32),
    ClientNotFound(String),
}

impl TryFrom<*mut gclient_t> for GameClient {
    type Error = QuakeLiveEngineError;

    fn try_from(game_client: *mut gclient_t) -> Result<Self, Self::Error> {
        unsafe {
            game_client
                .as_mut()
                .map(|gclient_t| Self {
                    game_client: gclient_t,
                })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl GameClient {
    pub(crate) fn get_client_num(&self) -> i32 {
        self.game_client.ps.clientNum
    }

    pub(crate) fn remove_kamikaze_flag(&mut self) {
        self.game_client.ps.eFlags &= !i32::try_from(EF_KAMIKAZE).unwrap();
    }

    pub(crate) fn set_privileges<T>(&mut self, privileges: T)
    where
        T: Into<privileges_t>,
    {
        self.game_client.sess.privileges = privileges.into();
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.game_client.ps.pm_type == 0
    }

    pub(crate) fn get_position(&self) -> (f32, f32, f32) {
        self.game_client.ps.origin.into()
    }

    pub(crate) fn set_position<T>(&mut self, position: T)
    where
        T: Into<[f32; 3]>,
    {
        self.game_client.ps.origin = position.into();
    }

    pub(crate) fn get_velocity(&self) -> (f32, f32, f32) {
        self.game_client.ps.velocity.into()
    }

    pub(crate) fn set_velocity<T>(&mut self, velocity: T)
    where
        T: Into<[f32; 3]>,
    {
        self.game_client.ps.velocity = velocity.into();
    }

    pub(crate) fn get_armor(&self) -> i32 {
        self.game_client.ps.stats[STAT_ARMOR as usize]
    }

    pub(crate) fn set_armor<T>(&mut self, armor: T)
    where
        T: Into<i32>,
    {
        self.game_client.ps.stats[STAT_ARMOR as usize] = armor.into();
    }

    pub(crate) fn get_noclip(&self) -> bool {
        self.game_client.noclip.into()
    }

    pub(crate) fn set_noclip<T>(&mut self, activate: T)
    where
        T: Into<qboolean>,
    {
        self.game_client.noclip = activate.into();
    }

    pub(crate) fn get_weapon(&self) -> weapon_t {
        self.game_client.ps.weapon.try_into().unwrap()
    }

    pub(crate) fn set_weapon<T>(&mut self, weapon: T)
    where
        T: Into<c_int>,
    {
        self.game_client.ps.weapon = weapon.into();
    }

    pub(crate) fn get_weapons(&self) -> [i32; 15] {
        let mut returned = [0; 15];
        let weapon_stats = self.game_client.ps.stats[STAT_WEAPONS as usize];
        for (i, item) in returned.iter_mut().enumerate() {
            *item = match weapon_stats & (1 << (i + 1)) != 0 {
                true => 1,
                false => 0,
            };
        }
        returned
    }

    pub(crate) fn set_weapons(&mut self, weapons: [i32; 15]) {
        let mut weapon_flags = 0;
        for (i, &item) in weapons.iter().enumerate() {
            let modifier = if item > 0 { 1 << (i + 1) } else { 0 };
            weapon_flags |= modifier;
        }

        self.game_client.ps.stats[STAT_WEAPONS as usize] = weapon_flags;
    }

    pub(crate) fn get_ammos(&self) -> [i32; 15] {
        let mut returned = [0; 15];
        let ammos = self.game_client.ps.ammo;
        for (i, item) in returned.iter_mut().enumerate() {
            *item = ammos[i + 1];
        }
        returned
    }

    pub(crate) fn set_ammos(&mut self, ammos: [i32; 15]) {
        for (i, &item) in ammos.iter().enumerate() {
            self.game_client.ps.ammo[i + 1] = item;
        }
    }

    pub(crate) fn get_powerups(&self) -> [i32; 6] {
        let mut returned = [0; 6];
        let current_level = CurrentLevel::default();
        for (powerup, item) in returned.iter_mut().enumerate() {
            let powerup_index = powerup_t::try_from(powerup).unwrap();
            *item = self.game_client.ps.powerups[powerup_index as usize];
            if *item != 0 {
                *item -= current_level.get_leveltime();
            }
        }
        returned
    }

    pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]) {
        let current_level = CurrentLevel::default();
        for (powerup, &item) in powerups.iter().enumerate() {
            let powerup_index = powerup_t::try_from(powerup).unwrap();
            if item == 0 {
                self.game_client.ps.powerups[powerup_index as usize] = 0;
            } else {
                let level_time = current_level.get_leveltime();
                self.game_client.ps.powerups[powerup_index as usize] =
                    level_time - (level_time % 1000) + item;
            }
        }
    }

    pub(crate) fn get_holdable(&self) -> i32 {
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize]
    }

    pub(crate) fn set_holdable<T>(&mut self, holdable: T)
    where
        T: Into<i32>,
    {
        let holdable_index: i32 = holdable.into();
        if holdable_index == MODELINDEX_KAMIKAZE as i32 {
            self.game_client.ps.eFlags |= i32::try_from(EF_KAMIKAZE).unwrap();
        } else {
            self.remove_kamikaze_flag();
        }
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize] = holdable_index;
    }

    pub(crate) fn get_current_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_CUR_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_max_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_MAX_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_flight_thrust(&self) -> i32 {
        self.game_client.ps.stats[STAT_FLIGHT_THRUST as usize]
    }

    pub(crate) fn get_flight_refuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_FLIGHT_REFUEL as usize]
    }

    pub(crate) fn set_flight<T>(&mut self, flight_params: T)
    where
        T: Into<[i32; 4]>,
    {
        let flight_params_array: [i32; 4] = flight_params.into();
        self.game_client.ps.stats[STAT_CUR_FLIGHT_FUEL as usize] = flight_params_array[0];
        self.game_client.ps.stats[STAT_MAX_FLIGHT_FUEL as usize] = flight_params_array[1];
        self.game_client.ps.stats[STAT_FLIGHT_THRUST as usize] = flight_params_array[2];
        self.game_client.ps.stats[STAT_FLIGHT_REFUEL as usize] = flight_params_array[3];
    }

    pub(crate) fn set_invulnerability(&mut self, time: i32) {
        self.game_client.invulnerabilityTime = CurrentLevel::default().get_leveltime() + time;
    }

    pub(crate) fn is_chatting(&self) -> bool {
        self.game_client.ps.eFlags & (EF_TALK as c_int) != 0
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.game_client.ps.pm_type == PM_FREEZE.into()
    }

    pub(crate) fn get_score(&self) -> i32 {
        if self.game_client.sess.sessionTeam == TEAM_SPECTATOR {
            0
        } else {
            self.game_client.ps.persistant[PERS_ROUND_SCORE as usize]
        }
    }

    pub(crate) fn set_score(&mut self, score: i32) {
        self.game_client.ps.persistant[PERS_ROUND_SCORE as usize] = score;
    }

    pub(crate) fn get_kills(&self) -> i32 {
        self.game_client.expandedStats.numKills
    }

    pub(crate) fn get_deaths(&self) -> i32 {
        self.game_client.expandedStats.numDeaths
    }

    pub(crate) fn get_damage_dealt(&self) -> i32 {
        self.game_client.expandedStats.totalDamageDealt
    }

    pub(crate) fn get_damage_taken(&self) -> i32 {
        self.game_client.expandedStats.totalDamageTaken
    }

    pub(crate) fn get_time_on_team(&self) -> i32 {
        CurrentLevel::default().level.time - self.game_client.pers.enterTime
    }

    pub(crate) fn get_ping(&self) -> i32 {
        self.game_client.ps.ping
    }

    pub(crate) fn set_vote_pending(&mut self) {
        self.game_client.pers.voteState = VOTE_PENDING;
    }

    pub(crate) fn spawn(&mut self) {
        self.game_client.ps.pm_type = PM_NORMAL.into();
    }
}

#[cfg(test)]
pub(crate) mod quake_live_fixtures {
    use crate::quake_types::{
        clientPersistant_t, clientSession_t, expandedStatObj_t, gclient_t, playerState_t,
        playerTeamState_t, raceInfo_t, usercmd_t, ClientPersistantBuilder, ClientSessionBuilder,
        ExpandedStatsBuilder, GClientBuilder, PlayerStateBuilder, PlayerTeamStateBuilder,
        RaceInfoBuilder, UserCmdBuilder,
    };
    use rstest::*;

    #[fixture]
    pub(crate) fn player_state() -> playerState_t {
        PlayerStateBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn user_cmd() -> usercmd_t {
        UserCmdBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn player_team_state() -> playerTeamState_t {
        PlayerTeamStateBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn client_persistant(
        user_cmd: usercmd_t,
        player_team_state: playerTeamState_t,
    ) -> clientPersistant_t {
        ClientPersistantBuilder::default()
            .cmd(user_cmd)
            .teamState(player_team_state)
            .build()
            .unwrap()
    }

    #[fixture]
    pub(crate) fn client_session() -> clientSession_t {
        ClientSessionBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn expanded_stats() -> expandedStatObj_t {
        ExpandedStatsBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn race_info() -> raceInfo_t {
        RaceInfoBuilder::default().build().unwrap()
    }

    #[fixture]
    pub(crate) fn gclient(
        player_state: playerState_t,
        client_persistant: clientPersistant_t,
        client_session: clientSession_t,
        expanded_stats: expandedStatObj_t,
        race_info: raceInfo_t,
    ) -> gclient_t {
        GClientBuilder::default()
            .ps(player_state)
            .pers(client_persistant)
            .sess(client_session)
            .expandedStats(expanded_stats)
            .race(race_info)
            .build()
            .unwrap()
    }
}

#[cfg(test)]
pub(crate) mod game_client_tests {
    use crate::quake_live_engine::quake_live_fixtures::*;
    use crate::quake_live_engine::GameClient;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::persistantFields_t::PERS_ROUND_SCORE;
    use crate::quake_types::pmtype_t::{PM_DEAD, PM_FREEZE, PM_NORMAL};
    use crate::quake_types::privileges_t::{
        PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT,
    };
    use crate::quake_types::statIndex_t::{STAT_ARMOR, STAT_HOLDABLE_ITEM};
    use crate::quake_types::team_t::{TEAM_BLUE, TEAM_RED, TEAM_SPECTATOR};
    use crate::quake_types::voteState_t::{VOTE_PENDING, VOTE_YES};
    use crate::quake_types::weapon_t::{
        WP_BFG, WP_CHAINGUN, WP_GAUNTLET, WP_GRAPPLING_HOOK, WP_GRENADE_LAUNCHER, WP_HANDS, WP_HMG,
        WP_LIGHTNING, WP_MACHINEGUN, WP_NAILGUN, WP_NONE, WP_PLASMAGUN, WP_PROX_LAUNCHER,
        WP_RAILGUN, WP_ROCKET_LAUNCHER, WP_SHOTGUN,
    };
    use crate::quake_types::{
        gclient_t, privileges_t, qboolean, weapon_t, EF_TALK, MODELINDEX_KAMIKAZE,
        MODELINDEX_TELEPORTER,
    };
    use pretty_assertions::assert_eq;
    use rstest::*;

    #[test]
    pub(crate) fn game_client_try_from_null_results_in_error() {
        assert_eq!(
            GameClient::try_from(std::ptr::null_mut() as *mut gclient_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[rstest]
    pub(crate) fn game_client_try_from_valid_value_result(gclient: gclient_t) {
        let mut mut_gclient = gclient;
        let game_client = GameClient::try_from(&mut mut_gclient as *mut gclient_t);

        assert!(game_client.is_ok());
    }

    #[rstest]
    pub(crate) fn game_client_get_client_num(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.clientNum = 42;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_client_num(), 42);
    }

    #[rstest]
    pub(crate) fn game_client_remove_kamikaze_flag_with_no_flag_set(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.eFlags = 0;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.remove_kamikaze_flag();
        assert_eq!(raw_client.ps.eFlags, 0);
    }

    #[rstest]
    pub(crate) fn game_client_remove_kamikaze_flag_removes_kamikaze_flag(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.eFlags = 513;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.remove_kamikaze_flag();
        assert_eq!(raw_client.ps.eFlags, 1);
    }

    #[rstest]
    #[case(PRIV_NONE)]
    #[case(PRIV_ADMIN)]
    #[case(PRIV_ROOT)]
    #[case(PRIV_MOD)]
    #[case(PRIV_BANNED)]
    pub(crate) fn game_client_set_privileges(#[case] privilege: privileges_t, gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_privileges(privilege);
        assert_eq!(raw_client.sess.privileges, privilege);
    }

    #[rstest]
    pub(crate) fn game_client_is_alive(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.pm_type = 0;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert!(game_client.is_alive());
    }

    #[rstest]
    pub(crate) fn game_client_is_dead(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.pm_type = 1;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert!(!game_client.is_alive());
    }

    #[rstest]
    pub(crate) fn game_client_get_position(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.origin = [21.0, 42.0, 11.0];
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[rstest]
    pub(crate) fn game_client_set_position(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_position((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[rstest]
    pub(crate) fn game_client_get_velocity(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.velocity = [21.0, 42.0, 11.0];
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[rstest]
    pub(crate) fn game_client_set_velocity(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_velocity((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[rstest]
    pub(crate) fn game_client_get_armor(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.stats[STAT_ARMOR as usize] = 69;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_armor(), 69);
    }

    #[rstest]
    pub(crate) fn game_client_set_armor(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_armor(42);
        assert_eq!(game_client.get_armor(), 42);
    }

    #[rstest]
    pub(crate) fn game_client_get_noclip(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.noclip = qboolean::qfalse;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_noclip(), false);
    }

    #[rstest]
    pub(crate) fn game_client_set_noclip(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_noclip(true);
        assert_eq!(game_client.get_noclip(), true);
    }

    #[rstest]
    #[case(WP_NONE)]
    #[case(WP_GAUNTLET)]
    #[case(WP_MACHINEGUN)]
    #[case(WP_SHOTGUN)]
    #[case(WP_GRENADE_LAUNCHER)]
    #[case(WP_ROCKET_LAUNCHER)]
    #[case(WP_PLASMAGUN)]
    #[case(WP_RAILGUN)]
    #[case(WP_LIGHTNING)]
    #[case(WP_BFG)]
    #[case(WP_GRAPPLING_HOOK)]
    #[case(WP_CHAINGUN)]
    #[case(WP_NAILGUN)]
    #[case(WP_PROX_LAUNCHER)]
    #[case(WP_HMG)]
    #[case(WP_HANDS)]
    pub(crate) fn game_client_set_weapon(#[case] weapon: weapon_t, gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_weapon(weapon);
        assert_eq!(game_client.get_weapon(), weapon);
    }

    #[rstest]
    pub(crate) fn game_client_set_weapons(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_weapons([0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]);
        assert_eq!(
            game_client.get_weapons(),
            [0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]
        );
    }

    #[rstest]
    pub(crate) fn game_client_set_ammos(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_ammos([10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]);
        assert_eq!(
            game_client.get_ammos(),
            [10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]
        );
    }

    #[rstest]
    pub(crate) fn game_client_get_holdable(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.stats[STAT_HOLDABLE_ITEM as usize] = MODELINDEX_KAMIKAZE as i32;
        let game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
    }

    #[rstest]
    pub(crate) fn game_client_set_holdable(gclient: gclient_t) {
        let mut raw_client = gclient;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_holdable(MODELINDEX_KAMIKAZE as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
        assert_eq!(raw_client.ps.eFlags, 512);
    }

    #[rstest]
    pub(crate) fn game_client_set_holdable_removes_kamikaze_flag(gclient: gclient_t) {
        let mut raw_client = gclient;
        raw_client.ps.eFlags = 513;
        let mut game_client = GameClient::try_from(&mut raw_client as *mut gclient_t).unwrap();
        game_client.set_holdable(MODELINDEX_TELEPORTER as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_TELEPORTER as i32);
        assert_eq!(raw_client.ps.eFlags, 1);
    }

    #[rstest]
    pub(crate) fn game_client_set_flight_values(gclient: gclient_t) {
        let mut mut_client = gclient;
        let mut game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        game_client.set_flight((1, 2, 3, 4));
        assert_eq!(game_client.get_current_flight_fuel(), 1);
        assert_eq!(game_client.get_max_flight_fuel(), 2);
        assert_eq!(game_client.get_flight_thrust(), 3);
        assert_eq!(game_client.get_flight_refuel(), 4);
    }

    #[rstest]
    pub(crate) fn game_client_is_chatting(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.eFlags = EF_TALK as i32;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert!(game_client.is_chatting());
    }

    #[rstest]
    pub(crate) fn game_client_is_not_chatting(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.eFlags = 0;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert!(!game_client.is_chatting());
    }

    #[rstest]
    pub(crate) fn game_client_is_frozen(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.pm_type = PM_FREEZE as i32;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert!(game_client.is_frozen());
    }

    #[rstest]
    pub(crate) fn game_client_is_not_frozen(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.pm_type = PM_NORMAL as i32;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert!(!game_client.is_frozen());
    }

    #[rstest]
    pub(crate) fn game_client_get_score(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.sess.sessionTeam = TEAM_RED;
        mut_client.ps.persistant[PERS_ROUND_SCORE as usize] = 42;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_score(), 42);
    }

    #[rstest]
    pub(crate) fn game_client_get_score_of_spectator(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.sess.sessionTeam = TEAM_SPECTATOR;
        mut_client.ps.persistant[PERS_ROUND_SCORE as usize] = 42;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_score(), 0);
    }

    #[rstest]
    pub(crate) fn game_client_set_score(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.sess.sessionTeam = TEAM_BLUE;
        let mut game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        game_client.set_score(21);
        assert_eq!(game_client.get_score(), 21);
    }

    #[rstest]
    pub(crate) fn game_client_get_kills(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.expandedStats.numKills = 5;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_kills(), 5);
    }

    #[rstest]
    pub(crate) fn game_client_get_deaths(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.expandedStats.numDeaths = 69;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_deaths(), 69);
    }

    #[rstest]
    pub(crate) fn game_client_get_damage_dealt(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.expandedStats.totalDamageDealt = 666;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_damage_dealt(), 666);
    }

    #[rstest]
    pub(crate) fn game_client_get_damage_taken(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.expandedStats.totalDamageTaken = 1234;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_damage_taken(), 1234);
    }

    #[rstest]
    pub(crate) fn game_client_get_ping(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.ping = 1;
        let game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_ping(), 1);
    }

    #[rstest]
    pub(crate) fn game_client_set_vote_pending(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.pers.voteState = VOTE_YES;
        let mut game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        game_client.set_vote_pending();
        assert_eq!(mut_client.pers.voteState, VOTE_PENDING);
    }

    #[rstest]
    pub(crate) fn game_client_spawn(gclient: gclient_t) {
        let mut mut_client = gclient;
        mut_client.ps.pm_type = PM_DEAD.into();
        let mut game_client = GameClient::try_from(&mut mut_client as *mut gclient_t).unwrap();
        game_client.spawn();
        assert_eq!(mut_client.ps.pm_type, PM_NORMAL.into());
    }
}

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameEntity {
    gentity_t: &'static mut gentity_t,
}

impl TryFrom<*mut gentity_t> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl TryFrom<i32> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(entity_id: i32) -> Result<Self, Self::Error> {
        extern "C" {
            static g_entities: *mut gentity_t;
        }

        if entity_id < 0 || entity_id >= i32::try_from(MAX_GENTITIES).unwrap() {
            return Err(InvalidId(entity_id));
        }
        unsafe {
            g_entities
                .offset(entity_id as isize)
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or(EntityNotFound("entity not found".into()))
        }
    }
}

impl TryFrom<u32> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(entity_id: u32) -> Result<Self, Self::Error> {
        extern "C" {
            static g_entities: *mut gentity_t;
        }

        if entity_id >= MAX_GENTITIES {
            return Err(InvalidId(entity_id as i32));
        }
        unsafe {
            g_entities
                .offset(entity_id as isize)
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or(EntityNotFound("entity not found".into()))
        }
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub(crate) extern "C" fn ShiNQlx_Touch_Item(
    ent: *mut gentity_t,
    other: *mut gentity_t,
    trace: *mut trace_t,
) {
    extern "C" {
        static Touch_Item: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t);
    }

    unsafe {
        if ent.as_ref().unwrap().parent == other {
            return;
        }
        Touch_Item(ent, other, trace);
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub(crate) extern "C" fn ShiNQlx_Switch_Touch_Item(ent: *mut gentity_t) {
    extern "C" {
        static Touch_Item: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t);
        static G_FreeEntity: extern "C" fn(*mut gentity_t);
    }

    unsafe {
        let ref_mut_ent = ent.as_mut().unwrap();
        ref_mut_ent.touch = Some(Touch_Item);
        ref_mut_ent.think = Some(G_FreeEntity);
        ref_mut_ent.nextthink = CurrentLevel::default().get_leveltime() + 29000;
    }
}

impl GameEntity {
    pub(crate) fn get_client_id(&self) -> i32 {
        extern "C" {
            static g_entities: *mut gentity_t;
        }

        unsafe {
            i32::try_from((self.gentity_t as *const gentity_t).offset_from(g_entities)).unwrap()
        }
    }

    pub(crate) fn start_kamikaze(&mut self) {
        extern "C" {
            static G_StartKamikaze: extern "C" fn(*const gentity_t);
        }

        unsafe { G_StartKamikaze(self.gentity_t as *const gentity_t) }
    }

    pub(crate) fn get_player_name(&self) -> String {
        if self.gentity_t.client.is_null() {
            return "".into();
        }
        if unsafe { self.gentity_t.client.as_ref().unwrap().pers.connected } == CON_DISCONNECTED {
            return "".into();
        }

        unsafe {
            CStr::from_ptr(
                self.gentity_t
                    .client
                    .as_ref()
                    .unwrap()
                    .pers
                    .netname
                    .as_ptr(),
            )
            .to_string_lossy()
            .into()
        }
    }

    pub(crate) fn get_team(&self) -> team_t {
        if self.gentity_t.client.is_null() {
            return TEAM_SPECTATOR;
        }
        if unsafe { self.gentity_t.client.as_ref().unwrap().pers.connected } == CON_DISCONNECTED {
            return TEAM_SPECTATOR;
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.sessionTeam }
    }

    pub(crate) fn get_privileges(&self) -> privileges_t {
        if self.gentity_t.client.is_null() {
            return privileges_t::from(-1);
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.privileges }
    }

    pub(crate) fn get_game_client(&self) -> Option<GameClient> {
        GameClient::try_from(self.gentity_t.client).ok()
    }

    pub(crate) fn get_activator(&self) -> Option<Activator> {
        self.gentity_t.activator.try_into().ok()
    }

    pub(crate) fn get_health(&self) -> i32 {
        self.gentity_t.health
    }

    pub(crate) fn set_health(&mut self, new_health: i32) {
        self.gentity_t.health = new_health as c_int;
    }

    pub(crate) fn slay_with_mod(&mut self, mean_of_death: meansOfDeath_t) {
        extern "C" {
            static G_Damage: extern "C" fn(
                *const gentity_t,
                *const gentity_t,
                *const gentity_t,
                *const c_float, // oritinal: vec3_t
                *const c_float, // original: vec3_t
                c_int,
                c_int,
                c_int,
            );
        }

        let damage = self.get_health()
            + if mean_of_death == MOD_KAMIKAZE {
                100000
            } else {
                0
            };

        self.get_game_client().unwrap().set_armor(0);
        // self damage = half damage, so multiplaying by 2
        unsafe {
            G_Damage(
                self.gentity_t,
                self.gentity_t,
                self.gentity_t,
                std::ptr::null(),
                std::ptr::null(),
                damage * 2,
                DAMAGE_NO_PROTECTION as c_int,
                mean_of_death.into(),
            );
        }
    }

    pub(crate) fn in_use(&self) -> bool {
        self.gentity_t.inuse.into()
    }

    pub(crate) fn get_classname(&self) -> String {
        unsafe {
            CStr::from_ptr(self.gentity_t.classname)
                .to_string_lossy()
                .into()
        }
    }

    pub(crate) fn is_game_item(&self, item_type: i32) -> bool {
        self.gentity_t.s.eType == item_type
    }

    pub(crate) fn is_respawning_weapon(&self) -> bool {
        unsafe {
            self.is_game_item(ET_ITEM as i32)
                && !self.gentity_t.item.is_null()
                && self.gentity_t.item.as_ref().unwrap().giType == IT_WEAPON
        }
    }

    pub(crate) fn set_respawn_time(&mut self, respawn_time: i32) {
        self.gentity_t.wait = respawn_time as c_float;
    }

    pub(crate) fn has_flags(&self) -> bool {
        self.gentity_t.flags != 0
    }

    pub(crate) fn is_dropped_item(&self) -> bool {
        self.gentity_t.flags & (FL_DROPPED_ITEM as i32) != 0
    }

    pub(crate) fn get_client_number(&self) -> i32 {
        self.gentity_t.s.clientNum
    }

    pub(crate) fn drop_holdable(&mut self) {
        extern "C" {
            static bg_itemlist: *const gitem_t;
            static LaunchItem: extern "C" fn(*const gitem_t, vec3_t, vec3_t) -> *const gentity_t;
        }

        let angle = self.gentity_t.s.apos.trBase[1] * (PI * 2.0 / 360.0);
        let velocity = [150.0 * angle.cos(), 150.0 * angle.sin(), 250.0];
        unsafe {
            let entity = LaunchItem(
                bg_itemlist.offset(
                    self.gentity_t.client.as_ref().unwrap().ps.stats[STAT_HOLDABLE_ITEM as usize]
                        as isize,
                ),
                self.gentity_t.s.pos.trBase,
                velocity,
            )
            .cast_mut();
            let mut_ref_entity = entity.as_mut().unwrap();
            mut_ref_entity.touch = Some(ShiNQlx_Touch_Item);
            mut_ref_entity.parent = self.gentity_t;
            mut_ref_entity.think = Some(ShiNQlx_Switch_Touch_Item);
            let current_level = CurrentLevel::default();
            mut_ref_entity.nextthink = current_level.get_leveltime() + 1000;
            mut_ref_entity.s.pos.trTime = current_level.get_leveltime() - 500;

            self.gentity_t.client.as_mut().unwrap().ps.stats[STAT_HOLDABLE_ITEM as usize] = 0;
        }
    }

    pub(crate) fn is_kamikaze_timer(&self) -> bool {
        unsafe { CStr::from_ptr(self.gentity_t.classname).to_string_lossy() == "kamikaze timer" }
    }

    pub(crate) fn free_entity(&mut self) {
        extern "C" {
            static G_FreeEntity: extern "C" fn(*mut gentity_t);
        }

        unsafe { G_FreeEntity(self.gentity_t) };
    }

    pub(crate) fn spawn_item(item_id: i32, origin: (i32, i32, i32)) {
        extern "C" {
            static bg_itemlist: *const gitem_t;
            static LaunchItem: extern "C" fn(*const gitem_t, vec3_t, vec3_t) -> *const gentity_t;
            static G_AddEvent: extern "C" fn(*const gentity_t, entity_event_t, c_int);
        }

        let origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let velocity = [0.0, 0.0, 0.9];

        unsafe {
            let ent = LaunchItem(bg_itemlist.offset(item_id as isize), origin_vec, velocity)
                as *mut gentity_t;
            let mut_ref_ent = ent.as_mut().unwrap();
            mut_ref_ent.nextthink = 0;
            mut_ref_ent.think = None;
            G_AddEvent(ent, EV_ITEM_RESPAWN, 0); // make item be scaled up
        }
    }

    pub(crate) fn replace_item(&mut self, item_id: i32) {
        extern "C" {
            static Com_Printf: extern "C" fn(*const c_char);
            static bg_itemlist: *const gitem_t;
            static SV_GetConfigstring: extern "C" fn(c_int, *mut c_char, c_int);
            static G_FreeEntity: extern "C" fn(*mut gentity_t);
        }

        unsafe { Com_Printf(self.gentity_t.classname) };
        if item_id != 0 {
            let item = unsafe { bg_itemlist.offset(item_id as isize).as_ref().unwrap() };
            self.gentity_t.s.modelindex = item_id;
            self.gentity_t.classname = item.classname;
            self.gentity_t.item = item;

            // this forces client to load new item
            let mut csbuffer: [c_char; 4096] = [0; 4096];
            unsafe {
                SV_GetConfigstring(
                    CS_ITEMS as c_int,
                    csbuffer.as_mut_ptr(),
                    csbuffer.len() as c_int,
                );
            }
            csbuffer[item_id as usize] = '1' as c_char;
            shinqlx_sv_setconfigstring(CS_ITEMS as c_int, csbuffer.as_ptr());
        } else {
            unsafe { G_FreeEntity(self.gentity_t) };
        }
    }
}

#[cfg(test)]
pub(crate) mod game_entity_tests {
    use crate::quake_live_engine::GameEntity;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::gentity_t;
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn game_entity_try_from_null_results_in_error() {
        assert_eq!(
            GameEntity::try_from(std::ptr::null_mut() as *mut gentity_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }
}

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Activator {
    activator: &'static gentity_t,
}

impl TryFrom<*mut gentity_t> for Activator {
    type Error = QuakeLiveEngineError;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_ref()
                .map(|gentity| Self { activator: gentity })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl Activator {
    pub(crate) fn get_owner_num(&self) -> i32 {
        self.activator.r.ownerNum
    }
}

#[cfg(test)]
pub(crate) mod activator_tests {
    use crate::quake_live_engine::Activator;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::gentity_t;
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn activator_try_from_null_results_in_error() {
        assert_eq!(
            Activator::try_from(std::ptr::null_mut() as *mut gentity_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }
}

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CVar {
    cvar: &'static cvar_t,
}

impl TryFrom<*const cvar_t> for CVar {
    type Error = QuakeLiveEngineError;

    fn try_from(cvar: *const cvar_t) -> Result<Self, Self::Error> {
        unsafe {
            cvar.as_ref()
                .map(|cvar| Self { cvar })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl CVar {
    pub(crate) fn get_string(&self) -> String {
        unsafe { CStr::from_ptr(self.cvar.string).to_string_lossy().into() }
    }

    pub(crate) fn get_integer(&self) -> i32 {
        self.cvar.integer
    }
}

#[cfg(test)]
pub(crate) mod cvar_tests {
    use crate::quake_live_engine::CVar;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::cvar_t;
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn cvar_try_from_null_results_in_error() {
        assert_eq!(
            CVar::try_from(std::ptr::null_mut() as *const cvar_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }
}

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Client {
    client_t: &'static client_t,
}

impl TryFrom<*const client_t> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client: *const client_t) -> Result<Self, Self::Error> {
        unsafe {
            client
                .as_ref()
                .map(|client_t| Self { client_t })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl TryFrom<i32> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        if client_id < 0 || client_id >= i32::try_from(MAX_CLIENTS).unwrap() {
            return Err(InvalidId(client_id));
        }
        unsafe {
            svs.as_ref()
                .unwrap()
                .clients
                .offset(client_id as isize)
                .as_ref()
                .map(|client| Self { client_t: client })
                .ok_or(ClientNotFound("client not found".into()))
        }
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        unsafe {
            i32::try_from(
                (self.client_t as *const client_t).offset_from(svs.as_ref().unwrap().clients),
            )
            .unwrap()
        }
    }

    pub(crate) fn get_state(&self) -> clientState_t {
        self.client_t.state
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect(&self, reason: &str) {
        let c_reason = CString::new(reason).unwrap_or(CString::new("").unwrap());
        unsafe { SV_DROPCLIENT_DETOUR.call(self.client_t, c_reason.as_ptr()) };
    }

    pub(crate) fn get_name(&self) -> String {
        if self.client_t.name.as_ptr().is_null() {
            "".into()
        } else {
            unsafe {
                CStr::from_ptr(&self.client_t.name as *const c_char)
                    .to_string_lossy()
                    .into()
            }
        }
    }

    pub(crate) fn get_user_info(&self) -> String {
        if self.client_t.userinfo.as_ptr().is_null() {
            "".into()
        } else {
            unsafe {
                CStr::from_ptr(self.client_t.userinfo.as_ptr())
                    .to_string_lossy()
                    .into()
            }
        }
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }

    pub(crate) fn set_vote(&self, yes_or_no: bool) {
        if let Ok(game_entity) = GameEntity::try_from(self.get_client_id()) {
            unsafe {
                game_entity
                    .gentity_t
                    .client
                    .as_mut()
                    .unwrap()
                    .pers
                    .voteState = if yes_or_no { VOTE_YES } else { VOTE_NO };
            }
        };
    }
}

#[cfg(test)]
pub(crate) mod client_tests {
    use crate::quake_live_engine::Client;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::client_t;
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn client_try_from_null_results_in_error() {
        assert_eq!(
            Client::try_from(std::ptr::null_mut() as *const client_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }
}

#[repr(transparent)]
pub(crate) struct CurrentLevel {
    level: &'static mut level_locals_t,
}

impl Default for CurrentLevel {
    fn default() -> Self {
        extern "C" {
            static level: *mut level_locals_t;
        }

        unsafe {
            Self {
                level: level.as_mut().unwrap(),
            }
        }
    }
}

impl CurrentLevel {
    pub(crate) fn get_vote_time(&self) -> Option<i32> {
        if self.level.voteTime <= 0 {
            None
        } else {
            Some(self.level.voteTime)
        }
    }

    pub(crate) fn get_leveltime(&self) -> i32 {
        self.level.time
    }

    pub(crate) fn callvote(&mut self, vote: &str, vote_disp: &str, vote_time: Option<i32>) {
        let actual_vote_time = vote_time.unwrap_or(30);
        for (dest, src) in self
            .level
            .voteString
            .iter_mut()
            .zip(CString::new(vote).unwrap().as_bytes_with_nul().iter())
        {
            *dest = *src as _;
        }
        for (dest, src) in self
            .level
            .voteDisplayString
            .iter_mut()
            .zip(CString::new(vote_disp).unwrap().as_bytes_with_nul().iter())
        {
            *dest = *src as _;
        }
        self.level.voteTime = self.level.time - 30000 + actual_vote_time * 1000;
        self.level.voteYes = 0;
        self.level.voteNo = 0;

        let maxclients = unsafe { SV_MAXCLIENTS };
        for client_id in 0..maxclients {
            if let Ok(game_entity) = GameEntity::try_from(client_id) {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_vote_pending();
            }
        }

        shinqlx_set_configstring(CS_VOTE_STRING, vote_disp);
        shinqlx_set_configstring(CS_VOTE_TIME, format!("{}", self.level.voteTime).as_str());
        shinqlx_set_configstring(CS_VOTE_YES, "0");
        shinqlx_set_configstring(CS_VOTE_NO, "0");
    }

    pub(crate) fn set_training_map(&mut self, is_training_map: bool) {
        self.level.mapIsTrainingMap = is_training_map.into();
    }
}

#[derive(Default)]
pub(crate) struct QuakeLiveEngine {}

pub(crate) trait FindCVar {
    fn find_cvar(&self, name: &str) -> Option<CVar>;
}

impl FindCVar for QuakeLiveEngine {
    fn find_cvar(&self, name: &str) -> Option<CVar> {
        extern "C" {
            static Cvar_FindVar: extern "C" fn(*const c_char) -> *const cvar_t;
        }

        if let Ok(c_name) = CString::new(name) {
            unsafe {
                let cvar = Cvar_FindVar(c_name.as_ptr());
                CVar::try_from(cvar).ok()
            }
        } else {
            None
        }
    }
}

pub(crate) trait CbufExecuteText {
    fn cbuf_execute_text(&self, exec_t: cbufExec_t, new_tags: &str);
}

impl CbufExecuteText for QuakeLiveEngine {
    fn cbuf_execute_text(&self, exec_t: cbufExec_t, new_tags: &str) {
        extern "C" {
            static Cbuf_ExecuteText: extern "C" fn(cbufExec_t, *const c_char);
        }

        if let Ok(c_tags) = CString::new(new_tags) {
            unsafe {
                Cbuf_ExecuteText(exec_t, c_tags.as_ptr());
            }
        }
    }
}

pub(crate) trait AddCommand {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
}

impl AddCommand for QuakeLiveEngine {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn()) {
        if let Ok(c_cmd) = CString::new(cmd) {
            unsafe { CMD_ADDCOMMAND_DETOUR.call(c_cmd.as_ptr(), func) };
        }
    }
}

pub(crate) trait SetModuleOffset {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
}

impl SetModuleOffset for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn()) {
        if let Ok(c_module_name) = CString::new(module_name) {
            unsafe { SYS_SETMODULEOFFSET_DETOUR.call(c_module_name.as_ptr(), offset) };
        }
    }
}

pub(crate) trait InitGame {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32);
}

impl InitGame for QuakeLiveEngine {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32) {
        extern "C" {
            static G_InitGame: extern "C" fn(c_int, c_int, c_int);
        }

        unsafe { G_InitGame(level_time, random_seed, restart) }
    }
}

pub(crate) trait ExecuteClientCommand {
    fn execute_client_command(&self, client: Option<&Client>, cmd: &str, client_ok: bool);
}

impl ExecuteClientCommand for QuakeLiveEngine {
    fn execute_client_command(&self, client: Option<&Client>, cmd: &str, client_ok: bool) {
        if let Ok(c_command) = CString::new(cmd) {
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
                        std::ptr::null(),
                        c_command.as_ptr(),
                        client_ok.into(),
                    )
                },
            }
        }
    }
}

pub(crate) trait SendServerCommand {
    fn send_server_command(&self, client: Option<Client>, command: &str);
}

impl SendServerCommand for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<Client>, command: &str) {
        extern "C" {
            static SV_SendServerCommand: extern "C" fn(*const client_t, *const c_char, ...);
        }

        if let Ok(c_command) = CString::new(command) {
            match client {
                Some(safe_client) => unsafe {
                    SV_SendServerCommand(safe_client.client_t, c_command.as_ptr());
                },
                None => unsafe {
                    SV_SendServerCommand(std::ptr::null(), c_command.as_ptr());
                },
            }
        }
    }
}

pub(crate) trait ClientEnterWorld {
    fn client_enter_world(&self, client: &Client, cmd: *const usercmd_t);
}

impl ClientEnterWorld for QuakeLiveEngine {
    fn client_enter_world(&self, client: &Client, cmd: *const usercmd_t) {
        unsafe { SV_CLIENTENTERWORLD_DETOUR.call(client.client_t, cmd) };
    }
}

pub(crate) trait SetConfigstring {
    fn set_configstring(&self, index: &u32, value: &str);
}

impl SetConfigstring for QuakeLiveEngine {
    fn set_configstring(&self, index: &u32, value: &str) {
        if let Ok(c_value) = CString::new(value) {
            unsafe {
                SV_SETCONFGISTRING_DETOUR
                    .call(index.to_owned().try_into().unwrap(), c_value.as_ptr())
            };
        }
    }
}

pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        extern "C" {
            static Com_Printf: extern "C" fn(*const c_char, ...);
        }

        if let Ok(c_msg) = CString::new(msg) {
            unsafe {
                Com_Printf(c_msg.as_ptr());
            }
        }
    }
}

pub(crate) trait SpawnServer {
    fn spawn_server(&self, server: &str, kill_bots: bool);
}

impl SpawnServer for QuakeLiveEngine {
    fn spawn_server(&self, server: &str, kill_bots: bool) {
        if let Ok(c_server) = CString::new(server) {
            unsafe { SV_SPAWNSERVER_DETOUR.call(c_server.as_ptr(), kill_bots.into()) };
        }
    }
}

pub(crate) trait RunFrame {
    fn run_frame(&self, time: i32);
}

impl RunFrame for QuakeLiveEngine {
    fn run_frame(&self, time: i32) {
        extern "C" {
            static G_RunFrame: extern "C" fn(c_int);
        }

        unsafe { G_RunFrame(time) };
    }
}

pub(crate) trait ClientConnect {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> Option<String>;
}

impl ClientConnect for QuakeLiveEngine {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> Option<String> {
        extern "C" {
            static ClientConnect: extern "C" fn(c_int, qboolean, qboolean) -> *const c_char;
        }

        unsafe {
            let c_return = ClientConnect(client_num, first_time.into(), is_bot.into());
            if c_return.is_null() {
                None
            } else {
                Some(CStr::from_ptr(c_return).to_string_lossy().into())
            }
        }
    }
}

pub(crate) trait ClientSpawn {
    fn client_spawn(&self, ent: &mut GameEntity);
}

impl ClientSpawn for QuakeLiveEngine {
    fn client_spawn(&self, ent: &mut GameEntity) {
        extern "C" {
            static ClientSpawn: extern "C" fn(*const gentity_t);
        }

        unsafe { ClientSpawn(ent.gentity_t) };
    }
}

pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        extern "C" {
            static Cmd_Args: extern "C" fn() -> *const c_char;
        }

        let cmd_args = unsafe { Cmd_Args() };
        if cmd_args.is_null() {
            None
        } else {
            let cmd_args = unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy();
            Some(cmd_args.to_string())
        }
    }
}

pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        extern "C" {
            static Cmd_Argc: extern "C" fn() -> c_int;
        }

        unsafe { Cmd_Argc() }
    }
}

pub(crate) trait CmdArgv {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
}

impl CmdArgv for QuakeLiveEngine {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str> {
        extern "C" {
            static Cmd_Argv: extern "C" fn(c_int) -> *const c_char;
        }

        if argno < 0 {
            None
        } else {
            let cmd_argv = unsafe { Cmd_Argv(argno) };
            if cmd_argv.is_null() {
                None
            } else {
                unsafe { CStr::from_ptr(cmd_argv).to_str().ok() }
            }
        }
    }
}

pub(crate) trait GameAddEvent {
    fn game_add_event(&self, game_entity: &GameEntity, event: entity_event_t, event_param: i32);
}

impl GameAddEvent for QuakeLiveEngine {
    fn game_add_event(&self, game_entity: &GameEntity, event: entity_event_t, event_param: i32) {
        extern "C" {
            static G_AddEvent: extern "C" fn(*const gentity_t, entity_event_t, c_int);
        }

        unsafe {
            G_AddEvent(
                game_entity.gentity_t as *const gentity_t,
                event,
                event_param,
            )
        }
    }
}

pub(crate) trait ConsoleCommand {
    fn execute_console_command(&self, cmd: &str);
}

impl ConsoleCommand for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: &str) {
        extern "C" {
            static Cmd_ExecuteString: extern "C" fn(*const c_char);
        }

        if let Ok(c_cmd) = CString::new(cmd) {
            unsafe {
                Cmd_ExecuteString(c_cmd.as_ptr());
            }
        }
    }
}

pub(crate) trait GetCVar {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
}

impl GetCVar for QuakeLiveEngine {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar> {
        extern "C" {
            static Cvar_Get: extern "C" fn(*const c_char, *const c_char, c_int) -> *const cvar_t;
        }

        let c_name = CString::new(name).unwrap();
        let c_value = CString::new(value).unwrap();
        let flags_value = flags.unwrap_or_default();
        unsafe {
            let cvar = Cvar_Get(c_name.as_ptr(), c_value.as_ptr(), flags_value);
            CVar::try_from(cvar).ok()
        }
    }
}

pub(crate) trait SetCVarForced {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar>;
}

impl SetCVarForced for QuakeLiveEngine {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar> {
        extern "C" {
            static Cvar_Set2:
                extern "C" fn(*const c_char, *const c_char, qboolean) -> *const cvar_t;
        }

        let c_name = CString::new(name).unwrap();
        let c_value = CString::new(value).unwrap();
        unsafe {
            let cvar = Cvar_Set2(c_name.as_ptr(), c_value.as_ptr(), forced.into());
            CVar::try_from(cvar).ok()
        }
    }
}

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
        extern "C" {
            static Cvar_GetLimit: extern "C" fn(
                *const c_char,
                *const c_char,
                *const c_char,
                *const c_char,
                c_int,
            ) -> *const cvar_t;
        }

        let c_name = CString::new(name).unwrap();
        let c_value = CString::new(value).unwrap();
        let c_min = CString::new(min).unwrap();
        let c_max = CString::new(max).unwrap();
        let flags_value = flags.unwrap_or_default();
        unsafe {
            let cvar = Cvar_GetLimit(
                c_name.as_ptr(),
                c_value.as_ptr(),
                c_min.as_ptr(),
                c_max.as_ptr(),
                flags_value,
            );
            CVar::try_from(cvar).ok()
        }
    }
}

pub(crate) trait GetConfigstring {
    fn get_configstring(&self, index: u32) -> String;
}

impl GetConfigstring for QuakeLiveEngine {
    fn get_configstring(&self, index: u32) -> String {
        extern "C" {
            static SV_GetConfigstring: extern "C" fn(c_int, *mut c_char, c_int);
        }

        let mut buffer: [u8; 4096] = [0; 4096];
        unsafe {
            SV_GetConfigstring(
                index as c_int,
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len() as c_int,
            );
        };
        CStr::from_bytes_until_nul(&buffer)
            .unwrap()
            .to_string_lossy()
            .into()
    }
}

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
        extern "C" {
            static G_Damage: extern "C" fn(
                *const gentity_t,
                *const gentity_t,
                *const gentity_t,
                *const c_float, // oritinal: vec3_t
                *const c_float, // original: vec3_t
                c_int,
                c_int,
                c_int,
            );
        }

        unsafe {
            G_Damage(
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
}
