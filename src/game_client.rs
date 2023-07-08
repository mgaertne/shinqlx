use crate::current_level::CurrentLevel;
use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::persistantFields_t::PERS_ROUND_SCORE;
use crate::quake_types::pmtype_t::{PM_FREEZE, PM_NORMAL};
use crate::quake_types::powerup_t::PW_NONE;
use crate::quake_types::statIndex_t::{
    STAT_ARMOR, STAT_CUR_FLIGHT_FUEL, STAT_FLIGHT_REFUEL, STAT_FLIGHT_THRUST, STAT_HOLDABLE_ITEM,
    STAT_MAX_FLIGHT_FUEL, STAT_WEAPONS,
};
use crate::quake_types::team_t::TEAM_SPECTATOR;
use crate::quake_types::voteState_t::{VOTE_NO, VOTE_PENDING, VOTE_YES};
use crate::quake_types::weapon_t::WP_NONE;
use crate::quake_types::{
    clientConnected_t, gclient_t, powerup_t, privileges_t, qboolean, team_t, weapon_t, EF_KAMIKAZE,
    EF_TALK, MODELINDEX_KAMIKAZE,
};
use std::ffi::{c_int, CStr};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameClient {
    game_client: &'static mut gclient_t,
}

impl TryFrom<*mut gclient_t> for GameClient {
    type Error = QuakeLiveEngineError;

    fn try_from(game_client: *mut gclient_t) -> Result<Self, Self::Error> {
        unsafe { game_client.as_mut() }
            .map(|gclient_t| Self {
                game_client: gclient_t,
            })
            .ok_or(NullPointerPassed("null pointer passed".into()))
    }
}

impl GameClient {
    pub(crate) fn get_client_num(&self) -> i32 {
        self.game_client.ps.clientNum
    }

    pub(crate) fn get_connection_state(&self) -> clientConnected_t {
        self.game_client.pers.connected
    }

    pub(crate) fn get_player_name(&self) -> String {
        unsafe { CStr::from_ptr(self.game_client.pers.netname.as_ptr()) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn get_team(&self) -> team_t {
        self.game_client.sess.sessionTeam
    }

    pub(crate) fn get_privileges(&self) -> privileges_t {
        self.game_client.sess.privileges
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
        self.game_client.ps.pm_type == PM_NORMAL
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
        self.game_client.ps.weapon.try_into().unwrap_or(WP_NONE)
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
        self.get_powerups_internal(&CurrentLevel::default())
    }

    pub(crate) fn get_powerups_internal(&self, current_level: &CurrentLevel) -> [i32; 6] {
        let mut returned = [0; 6];
        for (powerup, item) in returned.iter_mut().enumerate() {
            let powerup_index = powerup_t::try_from(powerup).unwrap_or(PW_NONE);
            *item = self.game_client.ps.powerups[powerup_index as usize];
            if *item != 0 {
                *item -= current_level.get_leveltime();
            }
        }
        returned
    }

    pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]) {
        self.set_powerups_internal(powerups, &CurrentLevel::default());
    }

    pub(crate) fn set_powerups_internal(
        &mut self,
        powerups: [i32; 6],
        current_level: &CurrentLevel,
    ) {
        for (powerup, &item) in powerups.iter().enumerate() {
            let powerup_index = powerup_t::try_from(powerup).unwrap_or(PW_NONE);
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
        self.set_invulnerability_internal(time, &CurrentLevel::default());
    }

    pub(crate) fn set_invulnerability_internal(&mut self, time: i32, current_level: &CurrentLevel) {
        self.game_client.invulnerabilityTime = current_level.get_leveltime() + time;
    }

    pub(crate) fn is_chatting(&self) -> bool {
        self.game_client.ps.eFlags & (EF_TALK as c_int) != 0
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.game_client.ps.pm_type == PM_FREEZE
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
        self.get_time_on_team_internal(&CurrentLevel::default())
    }

    pub(crate) fn get_time_on_team_internal(&self, current_level: &CurrentLevel) -> i32 {
        current_level.get_leveltime() - self.game_client.pers.enterTime
    }

    pub(crate) fn get_ping(&self) -> i32 {
        self.game_client.ps.ping
    }

    pub(crate) fn set_vote_pending(&mut self) {
        self.game_client.pers.voteState = VOTE_PENDING;
    }

    pub(crate) fn set_vote_state(&mut self, yes_or_no: bool) {
        self.game_client.pers.voteState = if yes_or_no { VOTE_YES } else { VOTE_NO };
    }

    pub(crate) fn spawn(&mut self) {
        self.game_client.ps.pm_type = PM_NORMAL;
    }
}

#[cfg(test)]
pub(crate) mod game_client_tests {
    use crate::current_level::CurrentLevel;
    use crate::game_client::GameClient;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::clientConnected_t::CON_CONNECTING;
    use crate::quake_types::persistantFields_t::PERS_ROUND_SCORE;
    use crate::quake_types::pmtype_t::{PM_DEAD, PM_FREEZE, PM_NORMAL};
    use crate::quake_types::powerup_t::{
        PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
    };
    use crate::quake_types::privileges_t::{
        PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT,
    };
    use crate::quake_types::statIndex_t::{STAT_ARMOR, STAT_HOLDABLE_ITEM};
    use crate::quake_types::team_t::{TEAM_BLUE, TEAM_RED, TEAM_SPECTATOR};
    use crate::quake_types::voteState_t::{VOTE_NO, VOTE_PENDING, VOTE_YES};
    use crate::quake_types::weapon_t::{
        WP_BFG, WP_CHAINGUN, WP_GAUNTLET, WP_GRAPPLING_HOOK, WP_GRENADE_LAUNCHER, WP_HANDS, WP_HMG,
        WP_LIGHTNING, WP_MACHINEGUN, WP_NAILGUN, WP_NONE, WP_PLASMAGUN, WP_PROX_LAUNCHER,
        WP_RAILGUN, WP_ROCKET_LAUNCHER, WP_SHOTGUN,
    };
    use crate::quake_types::{
        gclient_t, level_locals_t, privileges_t, qboolean, weapon_t, ClientPersistantBuilder,
        ClientSessionBuilder, ExpandedStatsBuilder, GClientBuilder, LevelLocalsBuilder,
        PlayerStateBuilder, EF_DEAD, EF_KAMIKAZE, EF_TALK, MODELINDEX_KAMIKAZE,
        MODELINDEX_TELEPORTER,
    };
    use pretty_assertions::assert_eq;
    use rstest::*;
    use std::ffi::c_char;

    #[test]
    pub(crate) fn game_client_try_from_null_results_in_error() {
        assert_eq!(
            GameClient::try_from(std::ptr::null_mut() as *mut gclient_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn game_client_try_from_valid_value_result() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t);

        assert_eq!(game_client.is_ok(), true);
    }

    #[test]
    pub(crate) fn game_client_get_client_num() {
        let player_state = PlayerStateBuilder::default().clientNum(42).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_client_num(), 42);
    }

    #[test]
    pub(crate) fn game_client_get_connection_state() {
        let client_persistant = ClientPersistantBuilder::default()
            .connected(CON_CONNECTING)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_connection_state(), CON_CONNECTING);
    }

    #[test]
    pub(crate) fn game_client_get_player_name() {
        let mut player_name: [c_char; 40] = [0; 40];
        for (index, char) in "awesome player".chars().enumerate() {
            player_name[index] = char.to_owned() as c_char;
        }
        let client_persistant = ClientPersistantBuilder::default()
            .netname(player_name)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_player_name(), "awesome player");
    }

    #[test]
    pub(crate) fn game_client_get_team() {
        let client_sessions = ClientSessionBuilder::default()
            .sessionTeam(TEAM_BLUE)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .sess(client_sessions)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_team(), TEAM_BLUE);
    }

    #[test]
    pub(crate) fn game_client_get_privileges() {
        let client_sessions = ClientSessionBuilder::default()
            .privileges(PRIV_MOD)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .sess(client_sessions)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_privileges(), PRIV_MOD);
    }

    #[test]
    pub(crate) fn game_client_remove_kamikaze_flag_with_no_flag_set() {
        let player_state = PlayerStateBuilder::default().eFlags(0).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.remove_kamikaze_flag();
        assert_eq!(gclient.ps.eFlags, 0);
    }

    #[test]
    pub(crate) fn game_client_remove_kamikaze_flag_removes_kamikaze_flag() {
        let player_state = PlayerStateBuilder::default().eFlags(513).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.remove_kamikaze_flag();
        assert_eq!(gclient.ps.eFlags, 1);
    }

    #[rstest]
    #[case(PRIV_NONE)]
    #[case(PRIV_ADMIN)]
    #[case(PRIV_ROOT)]
    #[case(PRIV_MOD)]
    #[case(PRIV_BANNED)]
    pub(crate) fn game_client_set_privileges(#[case] privilege: privileges_t) {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_privileges(privilege);
        assert_eq!(game_client.get_privileges(), privilege);
    }

    #[test]
    pub(crate) fn game_client_is_alive() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(PM_NORMAL)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_alive(), true);
    }

    #[test]
    pub(crate) fn game_client_is_dead() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(PM_DEAD)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_alive(), false);
    }

    #[test]
    pub(crate) fn game_client_get_position() {
        let player_state = PlayerStateBuilder::default()
            .origin([21.0, 42.0, 11.0])
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[test]
    pub(crate) fn game_client_set_position() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_position((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[test]
    pub(crate) fn game_client_get_velocity() {
        let player_state = PlayerStateBuilder::default()
            .velocity([21.0, 42.0, 11.0])
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[test]
    pub(crate) fn game_client_set_velocity() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_velocity((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[test]
    pub(crate) fn game_client_get_armor() {
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.stats[STAT_ARMOR as usize] = 69;
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_armor(), 69);
    }

    #[test]
    pub(crate) fn game_client_set_armor() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_armor(42);
        assert_eq!(game_client.get_armor(), 42);
    }

    #[test]
    pub(crate) fn game_client_get_noclip() {
        let mut gclient = GClientBuilder::default()
            .noclip(qboolean::qfalse)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_noclip(), false);
    }

    #[test]
    pub(crate) fn game_client_set_noclip() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_noclip(true);
        assert_eq!(game_client.get_noclip(), true);
    }

    #[test]
    pub(crate) fn game_client_disable_noclip() {
        let mut gclient = GClientBuilder::default()
            .noclip(qboolean::qtrue)
            .build()
            .unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_noclip(false);
        assert_eq!(game_client.get_noclip(), false);
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
    pub(crate) fn game_client_set_weapon(#[case] weapon: weapon_t) {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_weapon(weapon);
        assert_eq!(game_client.get_weapon(), weapon);
    }

    #[test]
    pub(crate) fn game_client_set_weapons() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_weapons([0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]);
        assert_eq!(
            game_client.get_weapons(),
            [0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]
        );
    }

    #[test]
    pub(crate) fn game_client_set_ammos() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_ammos([10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]);
        assert_eq!(
            game_client.get_ammos(),
            [10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]
        );
    }

    #[test]
    pub(crate) fn game_client_get_powerups_with_no_powerups() {
        let mut level_locals = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let mut gclient = GClientBuilder::default().build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_powerups_internal(&current_level), [0; 6]);
    }

    #[test]
    pub(crate) fn game_client_get_powerups_with_all_powerups_set() {
        let mut level_locals = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.powerups[PW_QUAD as usize] = 1235;
        player_state.powerups[PW_BATTLESUIT as usize] = 1236;
        player_state.powerups[PW_HASTE as usize] = 1237;
        player_state.powerups[PW_INVIS as usize] = 1238;
        player_state.powerups[PW_REGEN as usize] = 1239;
        player_state.powerups[PW_INVULNERABILITY as usize] = 1240;
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(
            game_client.get_powerups_internal(&current_level),
            [1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    pub(crate) fn game_client_set_powerups() {
        let mut level_locals = LevelLocalsBuilder::default().time(1000).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_powerups_internal([11, 12, 13, 14, 15, 16], &current_level);
        assert_eq!(
            game_client.get_powerups_internal(&current_level),
            [11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    pub(crate) fn game_client_set_powerups_deleting_all_powerups() {
        let mut level_locals = LevelLocalsBuilder::default().time(1000).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.powerups[PW_QUAD as usize] = 1235;
        player_state.powerups[PW_BATTLESUIT as usize] = 1236;
        player_state.powerups[PW_HASTE as usize] = 1237;
        player_state.powerups[PW_INVIS as usize] = 1238;
        player_state.powerups[PW_REGEN as usize] = 1239;
        player_state.powerups[PW_INVULNERABILITY as usize] = 1240;
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_powerups_internal([0, 0, 0, 0, 0, 0], &current_level);
        assert_eq!(
            game_client.get_powerups_internal(&current_level),
            [0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    pub(crate) fn game_client_get_holdable() {
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.stats[STAT_HOLDABLE_ITEM as usize] = MODELINDEX_KAMIKAZE as i32;
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
    }

    #[test]
    pub(crate) fn game_client_set_holdable() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_holdable(MODELINDEX_KAMIKAZE as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
        assert_eq!(gclient.ps.eFlags, EF_KAMIKAZE as i32);
    }

    #[test]
    pub(crate) fn game_client_set_holdable_removes_kamikaze_flag() {
        let player_state = PlayerStateBuilder::default().eFlags(513).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_holdable(MODELINDEX_TELEPORTER as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_TELEPORTER as i32);
        assert_eq!(gclient.ps.eFlags, EF_DEAD as i32);
    }

    #[test]
    pub(crate) fn game_client_set_flight_values() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_flight((1, 2, 3, 4));
        assert_eq!(game_client.get_current_flight_fuel(), 1);
        assert_eq!(game_client.get_max_flight_fuel(), 2);
        assert_eq!(game_client.get_flight_thrust(), 3);
        assert_eq!(game_client.get_flight_refuel(), 4);
    }

    #[test]
    pub(crate) fn game_client_set_invulnerability() {
        let mut level_locals = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_invulnerability_internal(10, &current_level);
        assert_eq!(gclient.invulnerabilityTime, 1244);
    }

    #[test]
    pub(crate) fn game_client_is_chatting() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(EF_TALK as i32)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_chatting(), true);
    }

    #[test]
    pub(crate) fn game_client_is_not_chatting() {
        let player_state = PlayerStateBuilder::default().eFlags(0).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_chatting(), false);
    }

    #[test]
    pub(crate) fn game_client_is_frozen() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(PM_FREEZE)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_frozen(), true);
    }

    #[test]
    pub(crate) fn game_client_is_not_frozen() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(PM_NORMAL)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.is_frozen(), false);
    }

    #[test]
    pub(crate) fn game_client_get_score() {
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.persistant[PERS_ROUND_SCORE as usize] = 42;
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(TEAM_RED)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .sess(client_session)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_score(), 42);
    }

    #[test]
    pub(crate) fn game_client_get_score_of_spectator() {
        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.persistant[PERS_ROUND_SCORE as usize] = 42;
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(TEAM_SPECTATOR)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .sess(client_session)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_score(), 0);
    }

    #[test]
    pub(crate) fn game_client_set_score() {
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(TEAM_BLUE)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .sess(client_session)
            .build()
            .unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_score(21);
        assert_eq!(game_client.get_score(), 21);
    }

    #[test]
    pub(crate) fn game_client_get_kills() {
        let expanded_stats = ExpandedStatsBuilder::default().numKills(5).build().unwrap();
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_kills(), 5);
    }

    #[test]
    pub(crate) fn game_client_get_deaths() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .numDeaths(69)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_deaths(), 69);
    }

    #[test]
    pub(crate) fn game_client_get_damage_dealt() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .totalDamageDealt(666)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_damage_dealt(), 666);
    }

    #[test]
    pub(crate) fn game_client_get_damage_taken() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .totalDamageTaken(1234)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_damage_taken(), 1234);
    }

    #[test]
    pub(crate) fn game_client_get_time_on_team() {
        let mut level_locals = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level =
            CurrentLevel::try_from(&mut level_locals as *mut level_locals_t).unwrap();
        let client_persistant = ClientPersistantBuilder::default()
            .enterTime(1192)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_time_on_team_internal(&current_level), 42);
    }

    #[test]
    pub(crate) fn game_client_get_ping() {
        let player_state = PlayerStateBuilder::default().ping(1).build().unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        assert_eq!(game_client.get_ping(), 1);
    }

    #[rstest]
    pub(crate) fn game_client_set_vote_pending() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(VOTE_YES)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_vote_pending();
        assert_eq!(gclient.pers.voteState, VOTE_PENDING);
    }

    #[rstest]
    pub(crate) fn game_client_set_vote_state_to_no() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(VOTE_PENDING)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_vote_state(false);
        assert_eq!(gclient.pers.voteState, VOTE_NO);
    }

    #[rstest]
    pub(crate) fn game_client_set_vote_state_to_yes() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(VOTE_PENDING)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.set_vote_state(true);
        assert_eq!(gclient.pers.voteState, VOTE_YES);
    }

    #[test]
    pub(crate) fn game_client_spawn() {
        let player_state = PlayerStateBuilder::default()
            .ping(1)
            .pm_type(PM_DEAD)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut game_client = GameClient::try_from(&mut gclient as *mut gclient_t).unwrap();
        game_client.spawn();
        assert_eq!(gclient.ps.pm_type, PM_NORMAL);
    }
}
