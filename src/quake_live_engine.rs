use crate::hooks::{shinqlx_set_configstring, ShiNQlx_SV_SetConfigstring};
use crate::quake_types::clientConnected_t::CON_DISCONNECTED;
use crate::quake_types::entityType_t::ET_ITEM;
use crate::quake_types::entity_event_t::EV_ITEM_RESPAWN;
use crate::quake_types::itemType_t::IT_WEAPON;
use crate::quake_types::meansOfDeath_t::MOD_KAMIKAZE;
use crate::quake_types::persistantFields_t::PERS_ROUND_SCORE;
use crate::quake_types::pmtype_t::PM_NORMAL;
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
use crate::quake_types::{
    cbufExec_t, client_t, cvar_t, entity_event_t, gclient_t, gentity_t, gitem_t, level_locals_t,
    privileges_t, qboolean, serverStatic_t, trace_t, usercmd_t, vec3_t, CS_ITEMS, CS_VOTE_NO,
    CS_VOTE_STRING, CS_VOTE_TIME, CS_VOTE_YES, DAMAGE_NO_PROTECTION, EF_KAMIKAZE, EF_TALK,
    FL_DROPPED_ITEM,
};
use crate::SV_MAXCLIENTS;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::ops::{BitAnd, BitAndAssign, BitOrAssign, Not};

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

pub(crate) struct GameClient {
    game_client: &'static mut gclient_t,
}

impl TryFrom<*mut gclient_t> for GameClient {
    type Error = &'static str;

    fn try_from(game_client: *mut gclient_t) -> Result<Self, Self::Error> {
        unsafe {
            game_client
                .as_mut()
                .map(|gclient_t| Self {
                    game_client: gclient_t,
                })
                .ok_or("null pointer passed")
        }
    }
}

lazy_static! {
    static ref POWERUP_INDEX_LOOKUP: HashMap<i32, usize> = HashMap::from([
        (0, PW_QUAD as usize),
        (1, PW_BATTLESUIT as usize),
        (2, PW_HASTE as usize),
        (3, PW_INVIS as usize),
        (4, PW_REGEN as usize),
        (5, PW_INVULNERABILITY as usize),
    ]);
}

impl GameClient {
    pub(crate) fn get_client_num(&self) -> i32 {
        self.game_client.ps.clientNum
    }

    pub(crate) fn remove_kamikaze_flag(&mut self) {
        self.game_client
            .ps
            .eFlags
            .bitand_assign(!EF_KAMIKAZE as i32);
    }

    pub(crate) fn set_privileges(&mut self, privileges: i32) {
        self.game_client.sess.privileges = privileges_t::from(privileges);
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.game_client.ps.pm_type == 0
    }

    pub(crate) fn get_position(&self) -> (f32, f32, f32) {
        (
            self.game_client.ps.origin[0],
            self.game_client.ps.origin[1],
            self.game_client.ps.origin[2],
        )
    }

    pub(crate) fn set_position(&mut self, position: (f32, f32, f32)) {
        self.game_client.ps.origin[0] = position.0;
        self.game_client.ps.origin[1] = position.1;
        self.game_client.ps.origin[2] = position.2;
    }

    pub(crate) fn get_velocity(&self) -> (f32, f32, f32) {
        (
            self.game_client.ps.velocity[0],
            self.game_client.ps.velocity[1],
            self.game_client.ps.velocity[2],
        )
    }

    pub(crate) fn set_velocity(&mut self, velocity: (f32, f32, f32)) {
        self.game_client.ps.velocity[0] = velocity.0 as c_float;
        self.game_client.ps.velocity[1] = velocity.1 as c_float;
        self.game_client.ps.velocity[2] = velocity.2 as c_float;
    }

    pub(crate) fn get_armor(&self) -> i32 {
        self.game_client.ps.stats[STAT_ARMOR as usize]
    }

    pub(crate) fn set_armor(&mut self, armor: i32) {
        self.game_client.ps.stats[STAT_ARMOR as usize] = armor;
    }

    pub(crate) fn get_noclip(&self) -> bool {
        self.game_client.noclip.into()
    }

    pub(crate) fn set_noclip(&mut self, activate: bool) {
        self.game_client.noclip = activate.into();
    }

    pub(crate) fn get_weapon(&self) -> i32 {
        self.game_client.ps.weapon
    }

    pub(crate) fn set_weapon(&mut self, weapon: i32) {
        self.game_client.ps.weapon = weapon;
    }

    pub(crate) fn get_weapons(&self) -> [i32; 15] {
        let mut returned = [0; 15];
        let weapon_stats = self.game_client.ps.stats[STAT_WEAPONS as usize];
        for (i, item) in returned.iter_mut().enumerate() {
            *item = match weapon_stats.bitand(1 << (i + 1)) != 0 {
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
            weapon_flags.bitor_assign(modifier);
        }

        self.game_client.ps.stats[STAT_WEAPONS as usize] = weapon_flags;
    }

    pub(crate) fn get_ammo(&self) -> [i32; 15] {
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
            let powerup_index = *POWERUP_INDEX_LOOKUP.get(&(powerup as i32)).unwrap();
            *item = self.game_client.ps.powerups[powerup_index];
            if *item != 0 {
                *item -= current_level.get_leveltime();
            }
        }
        returned
    }

    pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]) {
        let current_level = CurrentLevel::default();
        for (powerup, &item) in powerups.iter().enumerate() {
            let powerup_index = *POWERUP_INDEX_LOOKUP.get(&(powerup as i32)).unwrap();
            if item == 0 {
                self.game_client.ps.powerups[powerup_index] = 0;
            } else {
                let level_time = current_level.get_leveltime();
                self.game_client.ps.powerups[powerup_index] =
                    level_time - (level_time % 1000) + item;
            }
        }
    }

    pub(crate) fn get_holdable(&self) -> i32 {
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize]
    }

    pub(crate) fn set_holdable(&mut self, holdable: i32) {
        // 37 - kamikaze
        if holdable == 37 {
            self.game_client.ps.eFlags.bitor_assign(EF_KAMIKAZE as i32);
        } else {
            self.remove_kamikaze_flag();
        }
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize] = holdable;
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

    pub(crate) fn set_flight(&mut self, flight_params: (i32, i32, i32, i32)) {
        self.game_client.ps.stats[STAT_CUR_FLIGHT_FUEL as usize] = flight_params.0;
        self.game_client.ps.stats[STAT_MAX_FLIGHT_FUEL as usize] = flight_params.1;
        self.game_client.ps.stats[STAT_FLIGHT_THRUST as usize] = flight_params.2;
        self.game_client.ps.stats[STAT_FLIGHT_REFUEL as usize] = flight_params.3;
    }

    pub(crate) fn set_invulnerability(&mut self, time: i32) {
        self.game_client.invulnerabilityTime = CurrentLevel::default().get_leveltime() + time;
    }

    pub(crate) fn is_chatting(&self) -> bool {
        self.game_client.ps.eFlags.bitand(EF_TALK as c_int) != 0
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.game_client.ps.pm_type == 4
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
        self.game_client.ps.pm_type = PM_NORMAL as c_int;
    }
}

pub(crate) struct GameEntity {
    gentity_t: &'static mut gentity_t,
}

impl TryFrom<*mut gentity_t> for GameEntity {
    type Error = &'static str;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or("null pointer passed")
        }
    }
}

impl TryFrom<i32> for GameEntity {
    type Error = &'static str;

    fn try_from(entity_id: i32) -> Result<Self, Self::Error> {
        extern "C" {
            static g_entities: *mut gentity_t;
        }

        if entity_id < 0 {
            return Err("invalid entity_id");
        }
        unsafe {
            g_entities
                .offset(entity_id as isize)
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or("entity not found")
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

        unsafe { (self.gentity_t as *const gentity_t).offset_from(g_entities) as i32 }
    }

    pub(crate) fn start_kamikaze(&self) {
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

    pub(crate) fn get_team(&self) -> i32 {
        if self.gentity_t.client.is_null() {
            return TEAM_SPECTATOR as i32;
        }
        if unsafe { self.gentity_t.client.as_ref().unwrap().pers.connected } == CON_DISCONNECTED {
            return TEAM_SPECTATOR as i32;
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.sessionTeam as i32 }
    }

    pub(crate) fn get_privileges(&self) -> i32 {
        if self.gentity_t.client.is_null() {
            return -1;
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.privileges as i32 }
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

    pub(crate) fn slay_with_mod(&mut self, mean_of_death: i32) {
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
            + if mean_of_death == MOD_KAMIKAZE as i32 {
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
                mean_of_death,
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
        self.gentity_t.flags.bitand(FL_DROPPED_ITEM as i32) != 0
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
            ShiNQlx_SV_SetConfigstring(CS_ITEMS as c_int, csbuffer.as_ptr());
        } else {
            unsafe { G_FreeEntity(self.gentity_t) };
        }
    }
}

pub(crate) struct Activator {
    activator: &'static gentity_t,
}

impl TryFrom<*mut gentity_t> for Activator {
    type Error = &'static str;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_ref()
                .map(|gentity| Self { activator: gentity })
                .ok_or("null pointer passed")
        }
    }
}

impl Activator {
    pub(crate) fn get_owner_num(&self) -> i32 {
        self.activator.r.ownerNum
    }
}

pub(crate) struct CVar {
    cvar: &'static cvar_t,
}

impl TryFrom<*const cvar_t> for CVar {
    type Error = &'static str;

    fn try_from(cvar: *const cvar_t) -> Result<Self, Self::Error> {
        unsafe {
            cvar.as_ref()
                .map(|cvar| Self { cvar })
                .ok_or("null pointer passed")
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

pub(crate) struct Client {
    client_t: &'static client_t,
}

impl TryFrom<*const client_t> for Client {
    type Error = &'static str;

    fn try_from(client: *const client_t) -> Result<Self, Self::Error> {
        unsafe {
            client
                .as_ref()
                .map(|client_t| Self { client_t })
                .ok_or("null pointer passed")
        }
    }
}

impl TryFrom<i32> for Client {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        if client_id < 0 {
            return Err("invalid client_id");
        }
        unsafe {
            svs.as_ref()
                .unwrap()
                .clients
                .offset(client_id as isize)
                .as_ref()
                .map(|client| Self { client_t: client })
                .ok_or("client not found")
        }
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        unsafe {
            (self.client_t as *const client_t).offset_from(svs.as_ref().unwrap().clients) as i32
        }
    }

    pub(crate) fn get_state(&self) -> i32 {
        self.client_t.state as i32
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect(&self, reason: &str) {
        extern "C" {
            static SV_DropClient: extern "C" fn(*const client_t, *const c_char);
        }

        let c_reason = CString::new(reason).unwrap_or(CString::new("").unwrap());
        unsafe {
            SV_DropClient(self.client_t, c_reason.into_raw());
        }
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

        shinqlx_set_configstring(CS_VOTE_STRING as i32, vote_disp);
        shinqlx_set_configstring(
            CS_VOTE_TIME as i32,
            format!("{}", self.level.voteTime).as_str(),
        );
        shinqlx_set_configstring(CS_VOTE_YES as i32, "0");
        shinqlx_set_configstring(CS_VOTE_NO as i32, "0");
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

        let c_name = CString::new(name).unwrap();
        unsafe { CVar::try_from(Cvar_FindVar(c_name.into_raw())).ok() }
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

        let c_tags = CString::new(new_tags).unwrap();
        unsafe { Cbuf_ExecuteText(exec_t, c_tags.into_raw()) }
    }
}

pub(crate) trait AddCommand {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
}

impl AddCommand for QuakeLiveEngine {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn()) {
        extern "C" {
            static Cmd_AddCommand: extern "C" fn(*const c_char, *const c_void);
        }

        let c_cmd = CString::new(cmd).unwrap();
        unsafe { Cmd_AddCommand(c_cmd.into_raw(), func as *const c_void) }
    }
}

pub(crate) trait SetModuleOffset {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
}

impl SetModuleOffset for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn()) {
        extern "C" {
            static Sys_SetModuleOffset: extern "C" fn(*const c_char, *const c_void);
        }

        let c_module_name = CString::new(module_name).unwrap();
        unsafe { Sys_SetModuleOffset(c_module_name.into_raw(), offset as *const c_void) }
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
        extern "C" {
            static SV_ExecuteClientCommand: extern "C" fn(*const client_t, *const c_char, qboolean);
        }

        let command_native = CString::new(cmd).unwrap();
        match client {
            Some(safe_client) => unsafe {
                SV_ExecuteClientCommand(
                    safe_client.client_t,
                    command_native.into_raw(),
                    client_ok.into(),
                )
            },
            None => unsafe {
                SV_ExecuteClientCommand(std::ptr::null(), command_native.as_ptr(), client_ok.into())
            },
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

        let command_native = CString::new(command).unwrap();
        match client {
            Some(safe_client) => unsafe {
                SV_SendServerCommand(safe_client.client_t, command_native.into_raw())
            },
            None => unsafe { SV_SendServerCommand(std::ptr::null(), command_native.as_ptr()) },
        }
    }
}

pub(crate) trait ClientEnterWorld {
    fn client_enter_world(&self, client: &Client, cmd: *const usercmd_t);
}

impl ClientEnterWorld for QuakeLiveEngine {
    fn client_enter_world(&self, client: &Client, cmd: *const usercmd_t) {
        extern "C" {
            static SV_ClientEnterWorld: extern "C" fn(*const client_t, *const usercmd_t);
        }

        unsafe { SV_ClientEnterWorld(client.client_t, cmd) }
    }
}

pub(crate) trait SetConfigstring {
    fn set_configstring(&self, index: &i32, value: &str);
}

impl SetConfigstring for QuakeLiveEngine {
    fn set_configstring(&self, index: &i32, value: &str) {
        extern "C" {
            static SV_SetConfigstring: extern "C" fn(c_int, *const c_char);
        }

        if let Ok(c_value) = CString::new(value) {
            unsafe { SV_SetConfigstring(index.to_owned(), c_value.into_raw()) }
        }
    }
}

pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        extern "C" {
            static Com_Printf: extern "C" fn(*const c_char);
        }

        let c_msg = CString::new(msg).unwrap();
        unsafe { Com_Printf(c_msg.into_raw()) }
    }
}

pub(crate) trait SpawnServer {
    fn spawn_server(&self, server: &str, kill_bots: bool);
}

impl SpawnServer for QuakeLiveEngine {
    fn spawn_server(&self, server: &str, kill_bots: bool) {
        extern "C" {
            static SV_SpawnServer: extern "C" fn(*const c_char, qboolean);
        }

        let c_server = CString::new(server).unwrap();
        unsafe { SV_SpawnServer(c_server.into_raw(), kill_bots.into()) }
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
                return None;
            }
            Some(CStr::from_ptr(c_return).to_string_lossy().into())
        }
    }
}

pub(crate) trait ClientSpawn {
    fn client_spawn(&self, ent: &GameEntity);
}

impl ClientSpawn for QuakeLiveEngine {
    fn client_spawn(&self, ent: &GameEntity) {
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

        let c_cmd = CString::new(cmd).unwrap();
        unsafe { Cmd_ExecuteString(c_cmd.into_raw()) }
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
        unsafe { CVar::try_from(Cvar_Get(c_name.into_raw(), c_value.into_raw(), flags_value)).ok() }
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
            CVar::try_from(Cvar_Set2(
                c_name.into_raw(),
                c_value.into_raw(),
                forced.into(),
            ))
            .ok()
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
            CVar::try_from(Cvar_GetLimit(
                c_name.into_raw(),
                c_value.into_raw(),
                c_min.into_raw(),
                c_max.into_raw(),
                flags_value,
            ))
            .ok()
        }
    }
}

pub(crate) trait GetConfigstring {
    fn get_configstring(&self, index: i32) -> String;
}

impl GetConfigstring for QuakeLiveEngine {
    fn get_configstring(&self, index: i32) -> String {
        extern "C" {
            static SV_GetConfigstring: extern "C" fn(c_int, *mut c_char, c_int);
        }

        let mut buffer: [u8; 4096] = [0; 4096];
        unsafe {
            SV_GetConfigstring(
                index,
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
