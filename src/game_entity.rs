use crate::activator::Activator;
use crate::current_level::CurrentLevel;
use crate::game_client::GameClient;
use crate::game_item::GameItem;
use crate::hooks::shinqlx_set_configstring;
use crate::prelude::*;
use crate::quake_live_engine::{
    ComPrintf, FreeEntity, GetConfigstring, QuakeLiveEngineError, RegisterDamage, StartKamikaze,
    TryLaunchItem,
};
use crate::MAIN_ENGINE;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::f32::consts::PI;
use core::ffi::{c_char, c_float, c_int, CStr};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameEntity {
    pub(crate) gentity_t: &'static mut gentity_t,
}

impl AsMut<gentity_t> for GameEntity {
    fn as_mut(&mut self) -> &mut gentity_t {
        self.gentity_t
    }
}

impl TryFrom<*mut gentity_t> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe { game_entity.as_mut() }
            .map(|gentity| Self { gentity_t: gentity })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl TryFrom<i32> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(entity_id: i32) -> Result<Self, Self::Error> {
        if let Ok(max_gentities) = i32::try_from(MAX_GENTITIES) {
            if entity_id >= max_gentities {
                return Err(QuakeLiveEngineError::InvalidId(entity_id));
            }
        }
        if entity_id < 0 {
            return Err(QuakeLiveEngineError::InvalidId(entity_id));
        }

        let g_entities = GameEntity::get_entities_list();
        if g_entities.is_null() {
            return Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".into(),
            ));
        }

        Self::try_from(unsafe { g_entities.offset(entity_id as isize) })
            .map_err(|_| QuakeLiveEngineError::EntityNotFound("entity not found".into()))
    }
}

impl TryFrom<u32> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(entity_id: u32) -> Result<Self, Self::Error> {
        if entity_id >= MAX_GENTITIES {
            return Err(QuakeLiveEngineError::InvalidId(entity_id as i32));
        }
        let g_entities = GameEntity::get_entities_list();
        if g_entities.is_null() {
            return Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".into(),
            ));
        }

        Self::try_from(unsafe { g_entities.offset(entity_id as isize) })
            .map_err(|_| QuakeLiveEngineError::EntityNotFound("entity not found".into()))
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub(crate) extern "C" fn ShiNQlx_Touch_Item(
    ent: *mut gentity_t,
    other: *mut gentity_t,
    trace: *mut trace_t,
) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let Ok(original_func) = main_engine.touch_item_orig() else {
        return;
    };

    let Some(entity) = (unsafe { ent.as_ref() }) else {
        return;
    };

    if entity.parent != other {
        original_func(ent, other, trace);
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub(crate) extern "C" fn ShiNQlx_Switch_Touch_Item(ent: *mut gentity_t) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let Ok(touch_item_func) = main_engine.touch_item_orig() else {
        return;
    };

    let Ok(free_entity_func) = main_engine.g_free_entity_orig() else {
        return;
    };

    let Some(mut_ent) = (unsafe { ent.as_mut() }) else {
        return;
    };

    let level_time = CurrentLevel::try_get()
        .ok()
        .map(|current_level| current_level.get_leveltime())
        .unwrap_or_default();

    mut_ent.touch = Some(touch_item_func);
    mut_ent.think = Some(free_entity_func);
    mut_ent.nextthink = level_time + 29000;
}

const OFFSET_G_ENTITIES: usize = 0x11B;

impl GameEntity {
    fn get_entities_list() -> *mut gentity_t {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return core::ptr::null_mut();
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return core::ptr::null_mut();
        };

        let Ok(func_pointer) = main_engine.g_run_frame_orig() else {
            return core::ptr::null_mut();
        };
        let base_address = unsafe {
            core::ptr::read_unaligned((func_pointer as usize + OFFSET_G_ENTITIES) as *const i32)
        };
        let gentities_ptr = base_address as usize + func_pointer as usize + OFFSET_G_ENTITIES + 4;
        gentities_ptr as *mut gentity_t
    }

    pub(crate) fn get_entity_id(&self) -> i32 {
        let g_entities = Self::get_entities_list();
        if g_entities.is_null() {
            return -1;
        }
        self.get_entity_id_inter(g_entities)
    }

    fn get_entity_id_inter(&self, g_entities: *mut gentity_t) -> i32 {
        i32::try_from(unsafe { (self.gentity_t as *const gentity_t).offset_from(g_entities) })
            .unwrap_or(-1)
    }

    pub(crate) fn start_kamikaze(&mut self) {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        self.start_kamikaze_intern(main_engine);
    }

    #[cfg_attr(not(test), inline)]
    fn start_kamikaze_intern<'a, T: 'static>(&'a mut self, kamikaze_starter: &T)
    where
        T: for<'b> StartKamikaze<&'b mut GameEntity>,
    {
        kamikaze_starter.start_kamikaze(self);
    }

    pub(crate) fn get_player_name(&self) -> String {
        match self.get_game_client() {
            Err(_) => "".into(),
            Ok(game_client) => {
                if game_client.get_connection_state() == clientConnected_t::CON_DISCONNECTED {
                    "".into()
                } else {
                    game_client.get_player_name()
                }
            }
        }
    }

    pub(crate) fn get_team(&self) -> team_t {
        match self.get_game_client() {
            Err(_) => team_t::TEAM_SPECTATOR,
            Ok(game_client) => {
                if game_client.get_connection_state() == clientConnected_t::CON_DISCONNECTED {
                    team_t::TEAM_SPECTATOR
                } else {
                    game_client.get_team()
                }
            }
        }
    }

    pub(crate) fn get_privileges(&self) -> privileges_t {
        match self.get_game_client() {
            Err(_) => privileges_t::from(-1),
            Ok(game_client) => game_client.get_privileges(),
        }
    }

    pub(crate) fn get_game_client(&self) -> Result<GameClient, QuakeLiveEngineError> {
        self.gentity_t.client.try_into()
    }

    pub(crate) fn get_activator(&self) -> Result<Activator, QuakeLiveEngineError> {
        self.gentity_t.activator.try_into()
    }

    pub(crate) fn get_health(&self) -> i32 {
        self.gentity_t.health
    }

    pub(crate) fn set_health(&mut self, new_health: i32) {
        self.gentity_t.health = new_health as c_int;
    }

    pub(crate) fn slay_with_mod(&mut self, mean_of_death: meansOfDeath_t) {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        self.slay_with_mod_intern(mean_of_death, main_engine);
    }

    #[cfg_attr(not(test), inline)]
    fn slay_with_mod_intern<T: 'static>(
        &mut self,
        mean_of_death: meansOfDeath_t,
        quake_live_engine: &T,
    ) where
        T: RegisterDamage<c_int, c_int, c_int>,
    {
        let damage = self.get_health()
            + if mean_of_death == meansOfDeath_t::MOD_KAMIKAZE {
                100000
            } else {
                0
            };

        let _ = self
            .get_game_client()
            .map(|mut game_client| game_client.set_armor(0));

        // self damage = half damage, so multiplaying by 2
        quake_live_engine.register_damage(
            self.gentity_t,
            self.gentity_t,
            self.gentity_t,
            &mut [0.0, 0.0, 0.0],
            &mut [0.0, 0.0, 0.0],
            damage * 2,
            DAMAGE_NO_PROTECTION as c_int,
            mean_of_death as c_int,
        );
    }

    pub(crate) fn in_use(&self) -> bool {
        self.gentity_t.inuse.into()
    }

    pub(crate) fn get_classname(&self) -> String {
        unsafe { CStr::from_ptr(self.gentity_t.classname) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn is_game_item(&self, item_type: entityType_t) -> bool {
        self.gentity_t.s.eType == item_type as i32
    }

    pub(crate) fn is_respawning_weapon(&self) -> bool {
        if !self.is_game_item(entityType_t::ET_ITEM) {
            return false;
        }

        if self.gentity_t.item.is_null() {
            return false;
        }

        let Some(item) = (unsafe { self.gentity_t.item.as_ref() }) else {
            return false;
        };

        item.giType == itemType_t::IT_WEAPON
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
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let level_time = CurrentLevel::try_get()
            .ok()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();
        self.drop_holdable_intern(level_time, main_engine);
    }

    #[cfg_attr(not(test), inline)]
    fn drop_holdable_intern<T: 'static>(&mut self, level_time: i32, quake_live_engine: &T)
    where
        T: TryLaunchItem<GameItem>,
    {
        let Ok(mut game_client) = self.get_game_client() else {
            return;
        };
        let Ok(gitem) = GameItem::try_from(game_client.get_holdable()) else {
            return;
        };

        let angle = self.gentity_t.s.apos.trBase[1] * (PI * 2.0 / 360.0);
        let mut velocity = [150.0 * angle.cos(), 150.0 * angle.sin(), 250.0];
        let entity = quake_live_engine
            .try_launch_item(gitem, &mut self.gentity_t.s.pos.trBase, &mut velocity)
            .unwrap();
        entity.gentity_t.touch = Some(ShiNQlx_Touch_Item);
        entity.gentity_t.parent = self.gentity_t;
        entity.gentity_t.think = Some(ShiNQlx_Switch_Touch_Item);
        entity.gentity_t.nextthink = level_time + 1000;
        entity.gentity_t.s.pos.trTime = level_time - 500;
        game_client.set_holdable(0);
    }

    pub(crate) fn is_kamikaze_timer(&self) -> bool {
        self.get_classname() == "kamikaze timer"
    }

    pub(crate) fn free_entity(&mut self) {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        self.free_entity_intern(main_engine);
    }

    #[cfg_attr(not(test), inline)]
    fn free_entity_intern<'a, T: 'static>(&'a mut self, quake_live_engine: &T)
    where
        T: for<'b> FreeEntity<&'b mut GameEntity>,
    {
        quake_live_engine.free_entity(self);
    }

    pub(crate) fn replace_item(&mut self, item_id: i32) {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let class_name = unsafe { CStr::from_ptr(self.gentity_t.classname) };
        main_engine.com_printf(class_name.to_string_lossy());
        if item_id != 0 {
            let Ok(gitem) = GameItem::try_from(item_id) else {
                return;
            };
            self.gentity_t.s.modelindex = item_id;
            self.gentity_t.classname = gitem.get_classname().as_ptr() as *const c_char;
            self.gentity_t.item = gitem.gitem_t;

            // this forces client to load new item
            let mut items = main_engine.get_configstring(CS_ITEMS as u16);
            items.replace_range(item_id as usize..=item_id as usize, "1");
            shinqlx_set_configstring(item_id as u32, items.as_str());
        } else {
            self.free_entity();
        }
    }

    pub(crate) fn get_targetting_entity_ids(&self) -> Vec<u32> {
        if self.gentity_t.targetname.is_null() {
            return vec![];
        }

        let my_targetname = unsafe { CStr::from_ptr(self.gentity_t.targetname) }.to_string_lossy();

        (1..MAX_GENTITIES)
            .filter(|entity_id| match GameEntity::try_from(*entity_id) {
                Ok(other_ent) => {
                    !other_ent.gentity_t.target.is_null()
                        && my_targetname
                            == unsafe { CStr::from_ptr(other_ent.gentity_t.target) }
                                .to_string_lossy()
                }
                Err(_) => false,
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod game_entity_tests {
    use crate::game_entity::GameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::{FreeEntity, RegisterDamage, StartKamikaze};
    use alloc::ffi::CString;
    use core::ffi::{c_char, c_int};
    use mockall::mock;
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn game_entity_try_from_null_results_in_error() {
        assert_eq!(
            GameEntity::try_from(core::ptr::null_mut() as *mut gentity_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_valid_gentity() {
        let mut gentity = GEntityBuilder::default().build().unwrap();
        assert_eq!(
            GameEntity::try_from(&mut gentity as *mut gentity_t).is_ok(),
            true
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_negative_entity_id() {
        assert_eq!(
            GameEntity::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_too_large_i32_entity_id() {
        assert_eq!(
            GameEntity::try_from(65536i32),
            Err(QuakeLiveEngineError::InvalidId(65536))
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_valid_i32_gentities_not_initialized() {
        assert_eq!(
            GameEntity::try_from(42i32),
            Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".into()
            ))
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_too_large_u32_entity_id() {
        assert_eq!(
            GameEntity::try_from(65536u32),
            Err(QuakeLiveEngineError::InvalidId(65536))
        );
    }

    #[test]
    pub(crate) fn game_entity_try_from_valid_u32_gentities_not_initialized() {
        assert_eq!(
            GameEntity::try_from(42u32),
            Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".into()
            ))
        );
    }

    #[test]
    pub(crate) fn game_entity_get_entities_list_with_no_main_engine() {
        assert!(GameEntity::get_entities_list().is_null());
    }

    #[test]
    pub(crate) fn game_entity_get_entity_with_no_entities_list() {
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_entity_id(), -1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    pub(crate) fn game_entity_get_entity_intern_gets_offset() {
        let mut gentities = vec![
            GEntityBuilder::default().build().unwrap(),
            GEntityBuilder::default().build().unwrap(),
            GEntityBuilder::default().build().unwrap(),
            GEntityBuilder::default().build().unwrap(),
            GEntityBuilder::default().build().unwrap(),
        ];
        let game_entity = GameEntity::try_from(&mut gentities[3] as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_entity_id_inter(&mut gentities[0]), 3);
    }

    #[test]
    pub(crate) fn game_entity_start_kamikaze() {
        mock! {
            QuakeEngine {}
            impl StartKamikaze<&mut GameEntity> for QuakeEngine {
                fn start_kamikaze(&self, mut gentity: &mut GameEntity);
            }
        }
        let mut mock = MockQuakeEngine::new();
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        mock.expect_start_kamikaze().return_const(());
        game_entity.start_kamikaze_intern(&mock);
    }

    #[test]
    pub(crate) fn game_entity_get_player_name_from_null_client() {
        let mut gentity = GEntityBuilder::default()
            .client(core::ptr::null_mut() as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_player_name(), "");
    }

    #[test]
    pub(crate) fn game_entity_get_player_name_from_disconnected_game_client() {
        let client_persistant = ClientPersistantBuilder::default()
            .connected(clientConnected_t::CON_DISCONNECTED)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_player_name(), "");
    }

    #[test]
    pub(crate) fn game_entity_get_player_name_from_connected_game_client() {
        let player_name_str = "UnknownPlayer";
        let mut bytes_iter = player_name_str.bytes();
        let mut player_name: [c_char; 40usize] = [0; 40usize];
        player_name[0..player_name_str.len()].fill_with(|| bytes_iter.next().unwrap() as c_char);
        let client_persistant = ClientPersistantBuilder::default()
            .connected(clientConnected_t::CON_CONNECTED)
            .netname(player_name)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_player_name(), "UnknownPlayer");
    }

    #[test]
    pub(crate) fn game_entity_get_team_from_null_client() {
        let mut gentity = GEntityBuilder::default()
            .client(core::ptr::null_mut() as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_team(), team_t::TEAM_SPECTATOR);
    }

    #[test]
    pub(crate) fn game_entity_get_team_from_disconnected_game_client() {
        let client_persistant = ClientPersistantBuilder::default()
            .connected(clientConnected_t::CON_DISCONNECTED)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_team(), team_t::TEAM_SPECTATOR);
    }

    #[test]
    pub(crate) fn game_entity_get_team_from_connected_game_client() {
        let client_session = ClientSessionBuilder::default()
            .sessionTeam(team_t::TEAM_RED)
            .build()
            .unwrap();
        let client_persistant = ClientPersistantBuilder::default()
            .connected(clientConnected_t::CON_CONNECTED)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .pers(client_persistant)
            .sess(client_session)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_team(), team_t::TEAM_RED);
    }

    #[test]
    pub(crate) fn game_entity_get_privileges_from_null_client() {
        let mut gentity = GEntityBuilder::default()
            .client(core::ptr::null_mut() as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_privileges(), privileges_t::PRIV_BANNED);
    }

    #[test]
    pub(crate) fn game_entity_get_privileges_from_connected_game_client() {
        let client_session = ClientSessionBuilder::default()
            .privileges(privileges_t::PRIV_ROOT)
            .build()
            .unwrap();
        let mut gclient = GClientBuilder::default()
            .sess(client_session)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_privileges(), privileges_t::PRIV_ROOT);
    }

    #[test]
    pub(crate) fn game_entity_get_game_client_when_none_is_set() {
        let mut gentity = GEntityBuilder::default()
            .client(core::ptr::null_mut() as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_game_client().is_err(), true);
    }

    #[test]
    pub(crate) fn game_entity_get_game_client_with_valid_gclient() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_game_client().is_ok(), true);
    }

    #[test]
    pub(crate) fn game_entity_get_activator_when_none_is_set() {
        let mut gentity = GEntityBuilder::default()
            .activator(core::ptr::null_mut() as *mut gentity_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_activator().is_err(), true);
    }

    #[test]
    pub(crate) fn game_entity_get_activator_with_valid_activator() {
        let mut activator = GEntityBuilder::default().build().unwrap();
        let mut gentity = GEntityBuilder::default()
            .activator(&mut activator as *mut gentity_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_activator().is_ok(), true);
    }

    #[test]
    pub(crate) fn game_entity_set_health() {
        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .build()
            .unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        game_entity.set_health(666);
        assert_eq!(game_entity.get_health(), 666);
    }

    #[test]
    pub(crate) fn game_entity_slay_with_mod() {
        mock! {
            QuakeEngine {}
            #[allow(clippy::too_many_arguments)]
            impl RegisterDamage<c_int, c_int, c_int> for QuakeEngine {
                fn register_damage<'a>(
                    &self,
                    target: *mut gentity_t,
                    inflictor: *mut gentity_t,
                    attacker: *mut gentity_t,
                    dir: *mut vec3_t,
                    pos: *mut vec3_t,
                    damage: c_int,
                    dflags: c_int,
                    means_of_death: c_int
                );
            }
        }

        let mut player_state = PlayerStateBuilder::default().build().unwrap();
        player_state.stats[statIndex_t::STAT_ARMOR as usize] = 69;
        let mut gclient = GClientBuilder::default().ps(player_state).build().unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .health(42)
            .build()
            .unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();

        let mut mock = MockQuakeEngine::new();
        mock.expect_register_damage()
            .withf_st(|_, _, _, _, _, damage, dmg_flags, mean_of_death| {
                *damage == 84
                    && *dmg_flags == DAMAGE_NO_PROTECTION as c_int
                    && *mean_of_death == meansOfDeath_t::MOD_CRUSH as c_int
            })
            .return_const(());
        game_entity.slay_with_mod_intern(meansOfDeath_t::MOD_CRUSH, &mock);
        assert_eq!(gclient.ps.stats[statIndex_t::STAT_ARMOR as usize], 0);
    }

    #[test]
    pub(crate) fn game_entity_slay_with_kamikaze() {
        mock! {
            QuakeEngine {}
            #[allow(clippy::too_many_arguments)]
            impl RegisterDamage<c_int, c_int, c_int> for QuakeEngine {
                fn register_damage<'a>(
                    &self,
                    target: *mut gentity_t,
                    inflictor: *mut gentity_t,
                    attacker: *mut gentity_t,
                    dir: *mut vec3_t,
                    pos: *mut vec3_t,
                    damage: c_int,
                    dflags: c_int,
                    means_of_death: c_int
                );
            }
        }

        let mut gclient = GClientBuilder::default().build().unwrap();
        let mut gentity = GEntityBuilder::default()
            .client(&mut gclient as *mut gclient_t)
            .health(123)
            .build()
            .unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();

        let mut mock = MockQuakeEngine::new();
        mock.expect_register_damage()
            .withf_st(|_, _, _, _, _, damage, dmg_flags, mean_of_death| {
                *damage == 200246
                    && *dmg_flags == DAMAGE_NO_PROTECTION as c_int
                    && *mean_of_death == meansOfDeath_t::MOD_KAMIKAZE as c_int
            })
            .return_const(());
        game_entity.slay_with_mod_intern(meansOfDeath_t::MOD_KAMIKAZE, &mock);
        assert_eq!(gclient.ps.stats[statIndex_t::STAT_ARMOR as usize], 0);
    }

    #[test]
    pub(crate) fn game_entity_in_use() {
        let mut gentity = GEntityBuilder::default()
            .inuse(qboolean::qtrue)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.in_use(), true);
    }

    #[test]
    pub(crate) fn game_entity_get_classname() {
        let classname = CString::new("entity classname").unwrap();
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_classname(), "entity classname");
    }

    #[test]
    pub(crate) fn game_entity_is_game_item() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default().s(entity_state).build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_game_item(entityType_t::ET_ITEM), true);
        assert_eq!(game_entity.is_game_item(entityType_t::ET_PLAYER), false);
    }

    #[test]
    pub(crate) fn game_entity_is_respawning_weapon_for_player_entity() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_PLAYER as i32)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default().s(entity_state).build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    pub(crate) fn game_entity_is_respawning_weapon_for_null_item() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_PLAYER as i32)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(core::ptr::null() as *const gitem_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    pub(crate) fn game_entity_is_respawning_weapon_for_non_weapon() {
        let gitem = GItemBuilder::default()
            .giType(itemType_t::IT_AMMO)
            .build()
            .unwrap();
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(&gitem as *const gitem_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    pub(crate) fn game_entity_is_respawning_weapon_for_an_actual_weapon() {
        let gitem = GItemBuilder::default()
            .giType(itemType_t::IT_WEAPON)
            .build()
            .unwrap();
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .unwrap();
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(&gitem as *const gitem_t)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_respawning_weapon(), true);
    }

    #[test]
    pub(crate) fn game_entity_set_respawn_time() {
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        game_entity.set_respawn_time(42);
        assert_eq!(gentity.wait, 42.0);
    }

    #[test]
    pub(crate) fn game_entity_has_flags_with_no_flags() {
        let mut gentity = GEntityBuilder::default().flags(0).build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.has_flags(), false);
    }

    #[test]
    pub(crate) fn game_entity_has_flags_with_flags_set() {
        let mut gentity = GEntityBuilder::default().flags(42).build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.has_flags(), true);
    }

    #[test]
    pub(crate) fn game_entity_is_dropped_item_for_non_dropped_item() {
        let mut gentity = GEntityBuilder::default()
            .flags(FL_FORCE_GESTURE as i32)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_dropped_item(), false);
    }

    #[test]
    pub(crate) fn game_entity_is_dropped_item_for_dropped_item() {
        let mut gentity = GEntityBuilder::default()
            .flags(FL_DROPPED_ITEM as i32)
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_dropped_item(), true);
    }

    #[test]
    pub(crate) fn game_entity_get_client_number() {
        let entity_state = EntityStateBuilder::default().clientNum(42).build().unwrap();
        let mut gentity = GEntityBuilder::default().s(entity_state).build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.get_client_number(), 42);
    }

    #[test]
    pub(crate) fn game_entity_is_kamikaze_timer_for_non_kamikaze_timer() {
        let classname = CString::new("no kamikaze timer").unwrap();
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_kamikaze_timer(), false);
    }

    #[test]
    pub(crate) fn game_entity_is_kamikaze_timer_for_kamikaze_timer() {
        let classname = CString::new("kamikaze timer").unwrap();
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(game_entity.is_kamikaze_timer(), true);
    }

    #[test]
    pub(crate) fn game_entity_free_entity() {
        mock! {
            QuakeEngine {}
            impl FreeEntity<&mut GameEntity> for QuakeEngine {
                fn free_entity(&self, mut gentity: &mut GameEntity);
            }
        }

        let mut gentity = GEntityBuilder::default().build().unwrap();
        let mut game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();

        let mut mock = MockQuakeEngine::new();
        mock.expect_free_entity().return_const(());

        game_entity.free_entity_intern(&mock);
    }
}
