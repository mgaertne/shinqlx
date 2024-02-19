use super::prelude::*;
use crate::prelude::*;

use core::ffi::{c_int, CStr};

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
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
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
        self.game_client.ps.pm_type == pmtype_t::PM_NORMAL
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
        self.game_client.ps.stats[statIndex_t::STAT_ARMOR as usize]
    }

    pub(crate) fn set_armor<T>(&mut self, armor: T)
    where
        T: Into<i32>,
    {
        self.game_client.ps.stats[statIndex_t::STAT_ARMOR as usize] = armor.into();
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
        self.game_client
            .ps
            .weapon
            .try_into()
            .unwrap_or(weapon_t::WP_NONE)
    }

    pub(crate) fn set_weapon<T>(&mut self, weapon: T)
    where
        T: Into<c_int>,
    {
        self.game_client.ps.weapon = weapon.into();
    }

    pub(crate) fn get_weapons(&self) -> [i32; 15] {
        let weapon_stats = self.game_client.ps.stats[statIndex_t::STAT_WEAPONS as usize];
        (0..15)
            .map(|i| match weapon_stats & (1 << (i + 1)) {
                0 => 0,
                _ => 1,
            })
            .collect::<Vec<i32>>()
            .try_into()
            .unwrap()
    }

    pub(crate) fn set_weapons(&mut self, weapons: [i32; 15]) {
        let weapon_flags = weapons
            .iter()
            .enumerate()
            .filter(|(_, &item)| item > 0)
            .map(|(i, _)| 1 << (i + 1))
            .sum();
        self.game_client.ps.stats[statIndex_t::STAT_WEAPONS as usize] = weapon_flags;
    }

    pub(crate) fn get_ammos(&self) -> [i32; 15] {
        let ammos = self.game_client.ps.ammo;
        ammos
            .iter()
            .skip(1)
            .copied()
            .collect::<Vec<i32>>()
            .try_into()
            .unwrap()
    }

    pub(crate) fn set_ammos(&mut self, ammos: [i32; 15]) {
        ammos
            .iter()
            .enumerate()
            .for_each(|(i, &item)| self.game_client.ps.ammo[i + 1] = item);
    }

    pub(crate) fn get_powerups(&self) -> [i32; 6] {
        let level_time = CurrentLevel::try_get()
            .ok()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();

        (0..6)
            .map(|powerup| powerup_t::try_from(powerup).unwrap_or(powerup_t::PW_NONE))
            .map(|powerup_index| self.game_client.ps.powerups[powerup_index as usize])
            .map(|powerup_time| match powerup_time {
                0 => 0,
                _ => powerup_time - level_time,
            })
            .collect::<Vec<i32>>()
            .try_into()
            .unwrap()
    }

    pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]) {
        let level_time = CurrentLevel::try_get()
            .ok()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();

        powerups
            .iter()
            .enumerate()
            .map(|(powerup, &item)| {
                (
                    powerup_t::try_from(powerup).unwrap_or(powerup_t::PW_NONE),
                    item,
                )
            })
            .for_each(|(powerup_index, item)| {
                self.game_client.ps.powerups[powerup_index as usize] = if item == 0 {
                    0
                } else {
                    level_time - (level_time % 1000) + item
                }
            });
    }

    pub(crate) fn get_holdable(&self) -> i32 {
        self.game_client.ps.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize]
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
        self.game_client.ps.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize] = holdable_index;
    }

    pub(crate) fn get_current_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[statIndex_t::STAT_CUR_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_max_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[statIndex_t::STAT_MAX_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_flight_thrust(&self) -> i32 {
        self.game_client.ps.stats[statIndex_t::STAT_FLIGHT_THRUST as usize]
    }

    pub(crate) fn get_flight_refuel(&self) -> i32 {
        self.game_client.ps.stats[statIndex_t::STAT_FLIGHT_REFUEL as usize]
    }

    pub(crate) fn set_flight<T>(&mut self, flight_params: T)
    where
        T: Into<[i32; 4]>,
    {
        let flight_params_array: [i32; 4] = flight_params.into();
        self.game_client.ps.stats[statIndex_t::STAT_CUR_FLIGHT_FUEL as usize] =
            flight_params_array[0];
        self.game_client.ps.stats[statIndex_t::STAT_MAX_FLIGHT_FUEL as usize] =
            flight_params_array[1];
        self.game_client.ps.stats[statIndex_t::STAT_FLIGHT_THRUST as usize] =
            flight_params_array[2];
        self.game_client.ps.stats[statIndex_t::STAT_FLIGHT_REFUEL as usize] =
            flight_params_array[3];
    }

    pub(crate) fn set_invulnerability(&mut self, time: i32) {
        let level_time = CurrentLevel::try_get()
            .ok()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();
        self.game_client.invulnerabilityTime = level_time + time;
    }

    pub(crate) fn is_chatting(&self) -> bool {
        self.game_client.ps.eFlags & (EF_TALK as c_int) != 0
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.game_client.ps.pm_type == pmtype_t::PM_FREEZE
    }

    pub(crate) fn get_score(&self) -> i32 {
        if self.game_client.sess.sessionTeam == team_t::TEAM_SPECTATOR {
            0
        } else {
            self.game_client.ps.persistant[persistantFields_t::PERS_ROUND_SCORE as usize]
        }
    }

    pub(crate) fn set_score(&mut self, score: i32) {
        self.game_client.ps.persistant[persistantFields_t::PERS_ROUND_SCORE as usize] = score;
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
        let level_time = CurrentLevel::try_get()
            .ok()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();
        level_time - self.game_client.pers.enterTime
    }

    pub(crate) fn get_ping(&self) -> i32 {
        self.game_client.ps.ping
    }

    pub(crate) fn set_vote_pending(&mut self) {
        self.game_client.pers.voteState = voteState_t::VOTE_PENDING;
    }

    pub(crate) fn set_vote_state(&mut self, yes_or_no: bool) {
        self.game_client.pers.voteState = if yes_or_no {
            voteState_t::VOTE_YES
        } else {
            voteState_t::VOTE_NO
        };
    }

    pub(crate) fn spawn(&mut self) {
        self.game_client.ps.pm_type = pmtype_t::PM_NORMAL;
    }
}

#[cfg(test)]
mockall::mock! {
    pub(crate) GameClient {
        pub(crate) fn get_client_num(&self) -> i32;
        pub(crate) fn get_connection_state(&self) -> clientConnected_t;
        pub(crate) fn get_player_name(&self) -> String;
        pub(crate) fn get_team(&self) -> team_t;
        pub(crate) fn get_privileges(&self) -> privileges_t;
        pub(crate) fn remove_kamikaze_flag(&mut self);
        pub(crate) fn set_privileges<T>(&mut self, privileges: T)
        where
            T: Into<privileges_t> + 'static;
        pub(crate) fn is_alive(&self) -> bool;
        pub(crate) fn get_position(&self) -> (f32, f32, f32);
        pub(crate) fn set_position<T>(&mut self, position: T)
        where
            T: Into<[f32; 3]> + 'static;
        pub(crate) fn get_velocity(&self) -> (f32, f32, f32);
        pub(crate) fn set_velocity<T>(&mut self, velocity: T)
        where
            T: Into<[f32; 3]> + 'static;
        pub(crate) fn get_armor(&self) -> i32;
        pub(crate) fn set_armor<T>(&mut self, armor: T)
        where
            T: Into<i32> + 'static;
        pub(crate) fn get_noclip(&self) -> bool;
        pub(crate) fn set_noclip<T>(&mut self, activate: T)
        where
            T: Into<qboolean> + 'static;
        pub(crate) fn get_weapon(&self) -> weapon_t;
        pub(crate) fn set_weapon<T>(&mut self, weapon: T)
        where
            T: Into<c_int> + 'static;
        pub(crate) fn get_weapons(&self) -> [i32; 15];
        pub(crate) fn set_weapons(&mut self, weapons: [i32; 15]);
        pub(crate) fn get_ammos(&self) -> [i32; 15];
        pub(crate) fn set_ammos(&mut self, ammos: [i32; 15]);
        pub(crate) fn get_powerups(&self) -> [i32; 6];
        pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]);
        pub(crate) fn get_holdable(&self) -> i32;
        pub(crate) fn set_holdable<T>(&mut self, holdable: T)
        where
            T: Into<i32> + 'static;
        pub(crate) fn get_current_flight_fuel(&self) -> i32;
        pub(crate) fn get_max_flight_fuel(&self) -> i32;
        pub(crate) fn get_flight_thrust(&self) -> i32;
        pub(crate) fn get_flight_refuel(&self) -> i32;
        pub(crate) fn set_flight<T>(&mut self, flight_params: T)
        where
            T: Into<[i32; 4]> + 'static;
        pub(crate) fn set_invulnerability(&mut self, time: i32);
        pub(crate) fn is_chatting(&self) -> bool;
        pub(crate) fn is_frozen(&self) -> bool;
        pub(crate) fn get_score(&self) -> i32;
        pub(crate) fn set_score(&mut self, score: i32);
        pub(crate) fn get_kills(&self) -> i32;
        pub(crate) fn get_deaths(&self) -> i32;
        pub(crate) fn get_damage_dealt(&self) -> i32;
        pub(crate) fn get_damage_taken(&self) -> i32;
        pub(crate) fn get_time_on_team(&self) -> i32;
        pub(crate) fn get_ping(&self) -> i32;
        pub(crate) fn set_vote_pending(&mut self);
        pub(crate) fn set_vote_state(&mut self, yes_or_no: bool);
        pub(crate) fn spawn(&mut self);
    }

    impl TryFrom<*mut gclient_t> for GameClient {
        type Error = QuakeLiveEngineError;
        fn try_from(game_client: *mut gclient_t) -> Result<Self, QuakeLiveEngineError>;
    }
}

#[cfg(test)]
mod game_client_tests {
    use super::GameClient;
    use crate::ffi::c::prelude::*;
    use crate::prelude::*;

    use core::ffi::c_char;
    use pretty_assertions::assert_eq;
    use rstest::*;

    #[test]
    fn game_client_try_from_null_results_in_error() {
        assert_eq!(
            GameClient::try_from(ptr::null_mut() as *mut gclient_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn game_client_try_from_valid_value_result() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let game_client = GameClient::try_from(&mut gclient as *mut gclient_t);

        assert_eq!(game_client.is_ok(), true);
    }

    #[test]
    fn game_client_get_client_num() {
        let player_state = PlayerStateBuilder::default()
            .clientNum(42)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_client_num(), 42);
    }

    #[test]
    fn game_client_get_connection_state() {
        let client_persistant = ClientPersistantBuilder::default()
            .connected(clientConnected_t::CON_CONNECTING)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(
            game_client.get_connection_state(),
            clientConnected_t::CON_CONNECTING
        );
    }

    #[test]
    fn game_client_get_player_name() {
        let player_name_str = "awesome player";
        let mut bytes_iter = player_name_str.bytes();
        let mut player_name: [c_char; 40usize] = [0; 40usize];
        player_name[0..player_name_str.len()]
            .fill_with(|| bytes_iter.next().expect("this should not happen") as c_char);
        let client_persistant = ClientPersistantBuilder::default()
            .netname(player_name)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_player_name(), "awesome player");
    }

    #[test]
    fn game_client_get_team() {
        let client_sessions = ClientSessionBuilder::default()
            .sessionTeam(team_t::TEAM_BLUE)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .sess(client_sessions)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_team(), team_t::TEAM_BLUE);
    }

    #[test]
    fn game_client_get_privileges() {
        let client_sessions = ClientSessionBuilder::default()
            .privileges(privileges_t::PRIV_MOD)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .sess(client_sessions)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_privileges(), privileges_t::PRIV_MOD);
    }

    #[test]
    fn game_client_remove_kamikaze_flag_with_no_flag_set() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(0)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.remove_kamikaze_flag();
        assert_eq!(gclient.ps.eFlags, 0);
    }

    #[test]
    fn game_client_remove_kamikaze_flag_removes_kamikaze_flag() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(513)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.remove_kamikaze_flag();
        assert_eq!(gclient.ps.eFlags, 1);
    }

    #[rstest]
    #[case(privileges_t::PRIV_NONE)]
    #[case(privileges_t::PRIV_ADMIN)]
    #[case(privileges_t::PRIV_ROOT)]
    #[case(privileges_t::PRIV_MOD)]
    #[case(privileges_t::PRIV_BANNED)]
    fn game_client_set_privileges(#[case] privilege: privileges_t) {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_privileges(privilege);
        assert_eq!(game_client.get_privileges(), privilege);
    }

    #[test]
    fn game_client_is_alive() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(pmtype_t::PM_NORMAL)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_alive(), true);
    }

    #[test]
    fn game_client_is_dead() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(pmtype_t::PM_DEAD)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_alive(), false);
    }

    #[test]
    fn game_client_get_position() {
        let player_state = PlayerStateBuilder::default()
            .origin([21.0, 42.0, 11.0])
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[test]
    fn game_client_set_position() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_position((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_position(), (21.0, 42.0, 11.0));
    }

    #[test]
    fn game_client_get_velocity() {
        let player_state = PlayerStateBuilder::default()
            .velocity([21.0, 42.0, 11.0])
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[test]
    fn game_client_set_velocity() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_velocity((21.0, 42.0, 11.0));
        assert_eq!(game_client.get_velocity(), (21.0, 42.0, 11.0));
    }

    #[test]
    fn game_client_get_armor() {
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.stats[statIndex_t::STAT_ARMOR as usize] = 69;
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_armor(), 69);
    }

    #[test]
    fn game_client_set_armor() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_armor(42);
        assert_eq!(game_client.get_armor(), 42);
    }

    #[test]
    fn game_client_get_noclip() {
        let mut gclient = GClientBuilder::default()
            .noclip(qboolean::qfalse)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_noclip(), false);
    }

    #[test]
    fn game_client_set_noclip() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_noclip(true);
        assert_eq!(game_client.get_noclip(), true);
    }

    #[test]
    fn game_client_disable_noclip() {
        let mut gclient = GClientBuilder::default()
            .noclip(qboolean::qtrue)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_noclip(false);
        assert_eq!(game_client.get_noclip(), false);
    }

    #[rstest]
    #[case(weapon_t::WP_NONE)]
    #[case(weapon_t::WP_GAUNTLET)]
    #[case(weapon_t::WP_MACHINEGUN)]
    #[case(weapon_t::WP_SHOTGUN)]
    #[case(weapon_t::WP_GRENADE_LAUNCHER)]
    #[case(weapon_t::WP_ROCKET_LAUNCHER)]
    #[case(weapon_t::WP_PLASMAGUN)]
    #[case(weapon_t::WP_RAILGUN)]
    #[case(weapon_t::WP_LIGHTNING)]
    #[case(weapon_t::WP_BFG)]
    #[case(weapon_t::WP_GRAPPLING_HOOK)]
    #[case(weapon_t::WP_CHAINGUN)]
    #[case(weapon_t::WP_NAILGUN)]
    #[case(weapon_t::WP_PROX_LAUNCHER)]
    #[case(weapon_t::WP_HMG)]
    #[case(weapon_t::WP_HANDS)]
    fn game_client_set_weapon(#[case] weapon: weapon_t) {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_weapon(weapon);
        assert_eq!(game_client.get_weapon(), weapon);
    }

    #[test]
    fn game_client_set_weapons() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_weapons([0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]);
        assert_eq!(
            game_client.get_weapons(),
            [0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0]
        );
    }

    #[test]
    fn game_client_set_ammos() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_ammos([10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]);
        assert_eq!(
            game_client.get_ammos(),
            [10, 20, 31, 40, 51, 61, 70, 80, 90, 42, 69, -1, 1, 1, -1]
        );
    }

    #[test]
    #[serial]
    fn game_client_get_powerups_with_no_powerups() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_powerups(), [0; 6]);
    }

    #[test]
    #[serial]
    fn game_client_get_powerups_with_all_powerups_set() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.powerups[powerup_t::PW_QUAD as usize] = 1235;
        player_state.powerups[powerup_t::PW_BATTLESUIT as usize] = 1236;
        player_state.powerups[powerup_t::PW_HASTE as usize] = 1237;
        player_state.powerups[powerup_t::PW_INVIS as usize] = 1238;
        player_state.powerups[powerup_t::PW_REGEN as usize] = 1239;
        player_state.powerups[powerup_t::PW_INVULNERABILITY as usize] = 1240;
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_powerups(), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    #[serial]
    fn game_client_set_powerups() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1000);
            Ok(current_level)
        });

        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_powerups([11, 12, 13, 14, 15, 16]);
        assert_eq!(game_client.get_powerups(), [11, 12, 13, 14, 15, 16]);
    }

    #[test]
    #[serial]
    fn game_client_set_powerups_deleting_all_powerups() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1000);
            Ok(current_level)
        });

        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.powerups[powerup_t::PW_QUAD as usize] = 1235;
        player_state.powerups[powerup_t::PW_BATTLESUIT as usize] = 1236;
        player_state.powerups[powerup_t::PW_HASTE as usize] = 1237;
        player_state.powerups[powerup_t::PW_INVIS as usize] = 1238;
        player_state.powerups[powerup_t::PW_REGEN as usize] = 1239;
        player_state.powerups[powerup_t::PW_INVULNERABILITY as usize] = 1240;
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_powerups([0, 0, 0, 0, 0, 0]);
        assert_eq!(game_client.get_powerups(), [0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn game_client_get_holdable() {
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize] = MODELINDEX_KAMIKAZE as i32;
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
    }

    #[test]
    fn game_client_set_holdable() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_holdable(MODELINDEX_KAMIKAZE as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_KAMIKAZE as i32);
        assert_eq!(gclient.ps.eFlags, EF_KAMIKAZE as i32);
    }

    #[test]
    fn game_client_set_holdable_removes_kamikaze_flag() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(513)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_holdable(MODELINDEX_TELEPORTER as i32);
        assert_eq!(game_client.get_holdable(), MODELINDEX_TELEPORTER as i32);
        assert_eq!(gclient.ps.eFlags, EF_DEAD as i32);
    }

    #[test]
    fn game_client_set_flight_values() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_flight((1, 2, 3, 4));
        assert_eq!(game_client.get_current_flight_fuel(), 1);
        assert_eq!(game_client.get_max_flight_fuel(), 2);
        assert_eq!(game_client.get_flight_thrust(), 3);
        assert_eq!(game_client.get_flight_refuel(), 4);
    }

    #[test]
    #[serial]
    fn game_client_set_invulnerability() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_invulnerability(10);
        assert_eq!(gclient.invulnerabilityTime, 1244);
    }

    #[test]
    fn game_client_is_chatting() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(EF_TALK as i32)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_chatting(), true);
    }

    #[test]
    fn game_client_is_not_chatting() {
        let player_state = PlayerStateBuilder::default()
            .eFlags(0)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_chatting(), false);
    }

    #[test]
    fn game_client_is_frozen() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(pmtype_t::PM_FREEZE)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_frozen(), true);
    }

    #[test]
    fn game_client_is_not_frozen() {
        let player_state = PlayerStateBuilder::default()
            .pm_type(pmtype_t::PM_NORMAL)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.is_frozen(), false);
    }

    #[test]
    fn game_client_get_score() {
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.persistant[persistantFields_t::PERS_ROUND_SCORE as usize] = 42;
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(team_t::TEAM_RED)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .sess(client_session)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_score(), 42);
    }

    #[test]
    fn game_client_get_score_of_spectator() {
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("this should not happen");
        player_state.persistant[persistantFields_t::PERS_ROUND_SCORE as usize] = 42;
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(team_t::TEAM_SPECTATOR)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .sess(client_session)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_score(), 0);
    }

    #[test]
    fn game_client_set_score() {
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(team_t::TEAM_BLUE)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .sess(client_session)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_score(21);
        assert_eq!(game_client.get_score(), 21);
    }

    #[test]
    fn game_client_get_kills() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .numKills(5)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_kills(), 5);
    }

    #[test]
    fn game_client_get_deaths() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .numDeaths(69)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_deaths(), 69);
    }

    #[test]
    fn game_client_get_damage_dealt() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .totalDamageDealt(666)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_damage_dealt(), 666);
    }

    #[test]
    fn game_client_get_damage_taken() {
        let expanded_stats = ExpandedStatsBuilder::default()
            .totalDamageTaken(1234)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .expandedStats(expanded_stats)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_damage_taken(), 1234);
    }

    #[test]
    #[serial]
    fn game_client_get_time_on_team() {
        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockTestCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let client_persistant = ClientPersistantBuilder::default()
            .enterTime(1192)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_time_on_team(), 42);
    }

    #[test]
    fn game_client_get_ping() {
        let player_state = PlayerStateBuilder::default()
            .ping(1)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        assert_eq!(game_client.get_ping(), 1);
    }

    #[rstest]
    fn game_client_set_vote_pending() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(voteState_t::VOTE_YES)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_vote_pending();
        assert_eq!(gclient.pers.voteState, voteState_t::VOTE_PENDING);
    }

    #[rstest]
    fn game_client_set_vote_state_to_no() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(voteState_t::VOTE_PENDING)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_vote_state(false);
        assert_eq!(gclient.pers.voteState, voteState_t::VOTE_NO);
    }

    #[rstest]
    fn game_client_set_vote_state_to_yes() {
        let client_persistant = ClientPersistantBuilder::default()
            .voteState(voteState_t::VOTE_PENDING)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.set_vote_state(true);
        assert_eq!(gclient.pers.voteState, voteState_t::VOTE_YES);
    }

    #[test]
    fn game_client_spawn() {
        let player_state = PlayerStateBuilder::default()
            .ping(1)
            .pm_type(pmtype_t::PM_DEAD)
            .build()
            .expect("this should not happen");
        let mut gclient = GClientBuilder::default()
            .ps(player_state)
            .build()
            .expect("this should not happen");
        let mut game_client =
            GameClient::try_from(&mut gclient as *mut gclient_t).expect("this should not happen");
        game_client.spawn();
        assert_eq!(gclient.ps.pm_type, pmtype_t::PM_NORMAL);
    }
}
