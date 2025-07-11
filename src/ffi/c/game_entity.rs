use alloc::{borrow::Cow, vec};
use core::{
    borrow::BorrowMut,
    f32::consts::PI,
    ffi::{CStr, c_char, c_float, c_int},
    hint::cold_path,
};

use arrayvec::ArrayVec;
use tap::{TapFallible, TapOptional, TryConv};

use super::prelude::*;
use crate::{
    MAIN_ENGINE,
    hooks::shinqlx_set_configstring,
    prelude::*,
    quake_live_engine::{
        ComPrintf, FreeEntity, GetConfigstring, RegisterDamage, StartKamikaze, TryLaunchItem,
    },
};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameEntity {
    gentity_t: &'static mut gentity_t,
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
                "null pointer passed".to_string(),
            ))
    }
}

impl TryFrom<i32> for GameEntity {
    type Error = QuakeLiveEngineError;

    fn try_from(entity_id: i32) -> Result<Self, Self::Error> {
        match MAX_GENTITIES.try_conv::<i32>() {
            Ok(max_gentities) if !(0..max_gentities).contains(&entity_id) => {
                cold_path();
                Err(QuakeLiveEngineError::InvalidId(entity_id))
            }
            _ => {
                let g_entities = GameEntity::get_entities_list();
                if g_entities.is_null() {
                    cold_path();
                    Err(QuakeLiveEngineError::EntityNotFound(
                        "g_entities not initialized".to_string(),
                    ))
                } else {
                    Self::try_from(unsafe { g_entities.offset(entity_id as isize) }).map_err(|_| {
                        cold_path();
                        QuakeLiveEngineError::EntityNotFound("entity not found".to_string())
                    })
                }
            }
        }
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
            cold_path();
            return Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".to_string(),
            ));
        }

        Self::try_from(unsafe { g_entities.offset(entity_id as isize) }).map_err(|_| {
            cold_path();
            QuakeLiveEngineError::EntityNotFound("entity not found".to_string())
        })
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn ShiNQlx_Touch_Item(
    ent: *mut gentity_t,
    other: *mut gentity_t,
    trace: *mut trace_t,
) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        let _ = main_engine.touch_item_orig().tap_ok(|original_func| {
            (unsafe { ent.as_ref() }).tap_some(|&entity| {
                if !ptr::eq(entity.parent, other) {
                    original_func(ent, other, trace);
                }
            });
        });
    });
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn ShiNQlx_Switch_Touch_Item(ent: *mut gentity_t) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        if let (Ok(touch_item_func), Ok(free_entity_func)) = (
            main_engine.touch_item_orig(),
            main_engine.g_free_entity_orig(),
        ) {
            (unsafe { ent.as_mut() }).tap_some_mut(|mut_ent| {
                let level_time = CurrentLevel::try_get()
                    .map(|current_level| current_level.get_leveltime())
                    .unwrap_or_default();

                mut_ent.touch = Some(touch_item_func);
                mut_ent.think = Some(free_entity_func);
                mut_ent.nextthink = level_time + 29000;
            });
        }
    });
}

const OFFSET_G_ENTITIES: usize = 0x11B;

impl GameEntity {
    fn get_entities_list() -> *mut gentity_t {
        #[cfg(test)]
        if cfg!(test) {
            cold_path();
            return MockGameEntity::get_entities_list();
        }

        Self::get_entities_list_real()
    }

    fn get_entities_list_real() -> *mut gentity_t {
        MAIN_ENGINE
            .load()
            .as_ref()
            .map_or(ptr::null_mut(), |main_engine| {
                main_engine
                    .g_run_frame_orig()
                    .map_or(ptr::null_mut(), |func_pointer| {
                        let base_address = unsafe {
                            ptr::read_unaligned(
                                (func_pointer as usize + OFFSET_G_ENTITIES) as *const i32,
                            )
                        };
                        let gentities_ptr =
                            base_address as usize + func_pointer as usize + OFFSET_G_ENTITIES + 4;
                        gentities_ptr as *mut gentity_t
                    })
            })
    }

    pub(crate) fn get_entity_id(&self) -> i32 {
        let g_entities = Self::get_entities_list();
        if g_entities.is_null() {
            return -1;
        }
        i32::try_from(unsafe { (self.gentity_t as *const gentity_t).offset_from(g_entities) })
            .unwrap_or(-1)
    }

    pub(crate) fn start_kamikaze(&mut self) {
        MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
            main_engine.start_kamikaze(self);
        });
    }

    pub(crate) fn get_player_name(&self) -> String {
        match self.get_game_client() {
            Ok(game_client)
                if game_client.get_connection_state() != clientConnected_t::CON_DISCONNECTED =>
            {
                game_client.get_player_name().into()
            }
            _ => "".into(),
        }
    }

    pub(crate) fn get_team(&self) -> team_t {
        match self.get_game_client() {
            Ok(game_client)
                if game_client.get_connection_state() != clientConnected_t::CON_DISCONNECTED =>
            {
                game_client.get_team()
            }
            _ => team_t::TEAM_SPECTATOR,
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

    #[allow(mismatched_lifetime_syntaxes)]
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
        let damage = self.get_health()
            + if mean_of_death == meansOfDeath_t::MOD_KAMIKAZE {
                100000
            } else {
                0
            };

        let _ = self.get_game_client().tap_ok_mut(|game_client| {
            game_client.set_armor(0);
        });

        // self damage = half damage, so multiplaying by 2
        MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
            main_engine.register_damage(
                self.gentity_t,
                self.gentity_t,
                self.gentity_t,
                &mut [0.0, 0.0, 0.0],
                &mut [0.0, 0.0, 0.0],
                damage * 2,
                DAMAGE_NO_PROTECTION as c_int,
                mean_of_death as c_int,
            );
        });
    }

    pub(crate) fn in_use(&self) -> bool {
        self.gentity_t.inuse.into()
    }

    pub(crate) fn get_classname(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(self.gentity_t.classname) }.to_string_lossy()
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
        if MAIN_ENGINE.load().is_none() {
            return;
        }

        let level_time = CurrentLevel::try_get()
            .map(|current_level| current_level.get_leveltime())
            .unwrap_or_default();

        let Ok(mut game_client) = self.get_game_client() else {
            cold_path();
            return;
        };
        let Ok(mut gitem) =
            crate::ffi::c::game_item::GameItem::try_from(game_client.get_holdable())
        else {
            cold_path();
            return;
        };

        let angle = self.gentity_t.s.apos.trBase[1] * (PI * 2.0 / 360.0);
        let mut velocity = [150.0 * angle.cos(), 150.0 * angle.sin(), 250.0];
        MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
            let mut entity = main_engine
                .try_launch_item(
                    gitem.borrow_mut(),
                    self.gentity_t.s.pos.trBase.borrow_mut(),
                    velocity.borrow_mut(),
                )
                .unwrap();
            entity.set_touch(Some(ShiNQlx_Touch_Item));
            entity.set_parent(self.gentity_t);
            entity.set_think(Some(ShiNQlx_Switch_Touch_Item));
            entity.set_next_think(level_time + 1000);
            entity.set_position_trace_time(level_time - 500);
        });
        game_client.set_holdable(0);
    }

    pub(crate) fn is_kamikaze_timer(&self) -> bool {
        self.get_classname() == "kamikaze timer"
    }

    pub(crate) fn free_entity(&mut self) {
        MAIN_ENGINE
            .load()
            .as_ref()
            .tap_some(|main_engine| main_engine.free_entity(self));
    }

    pub(crate) fn replace_item(&mut self, item_id: i32) {
        MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
            let class_name = unsafe { CStr::from_ptr(self.gentity_t.classname) };
            main_engine.com_printf(&class_name.to_string_lossy());
            if item_id != 0 {
                #[cfg_attr(test, allow(irrefutable_let_patterns))]
                let Ok(gitem) = item_id.try_conv::<GameItem>() else {
                    cold_path();
                    return;
                };
                self.gentity_t.s.modelindex = item_id;
                self.gentity_t.classname = gitem.get_classname().as_ptr() as *const c_char;
                self.gentity_t.item = gitem.as_ref();

                // this forces client to load new item
                let mut items = main_engine.get_configstring(CS_ITEMS as u16).to_string();
                items.replace_range(item_id as usize..=item_id as usize, "1");
                shinqlx_set_configstring(item_id as u32, &items);
            } else {
                self.free_entity();
            }
        });
    }

    pub(crate) fn get_targetting_entity_ids(&self) -> Vec<u32> {
        if self.gentity_t.targetname.is_null() {
            return vec![];
        }

        let my_targetname = unsafe { CStr::from_ptr(self.gentity_t.targetname) }.to_string_lossy();

        (1..MAX_GENTITIES)
            .filter(|entity_id| {
                GameEntity::try_from(*entity_id).is_ok_and(|other_ent| {
                    !other_ent.gentity_t.target.is_null()
                        && my_targetname
                            == unsafe { CStr::from_ptr(other_ent.gentity_t.target) }
                                .to_string_lossy()
                })
            })
            .collect::<ArrayVec<u32, { MAX_GENTITIES as usize }>>()
            .to_vec()
    }

    pub(crate) fn set_next_think(&mut self, next_think: i32) {
        self.gentity_t.nextthink = next_think;
    }

    pub(crate) fn set_think(&mut self, think: Option<unsafe extern "C" fn(*mut gentity_t)>) {
        self.gentity_t.think = think;
    }

    pub(crate) fn set_touch(
        &mut self,
        touch: Option<unsafe extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>,
    ) {
        self.gentity_t.touch = touch;
    }

    pub(crate) fn set_parent(&mut self, parent: &mut gentity_t) {
        self.gentity_t.parent = parent;
    }

    pub(crate) fn set_position_trace_time(&mut self, trace_time: i32) {
        self.gentity_t.s.pos.trTime = trace_time;
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
mockall::mock! {
    pub(crate) GameEntity {
        pub(crate) fn get_entities_list() -> *mut gentity_t;
        pub(crate) fn get_entity_id(&self) -> i32;
        pub(crate) fn start_kamikaze(&mut self);
        pub(crate) fn get_player_name(&self) -> String;
        pub(crate) fn get_team(&self) -> team_t;
        pub(crate) fn get_privileges(&self) -> privileges_t;
        pub(crate) fn get_game_client(&self) -> Result<GameClient, QuakeLiveEngineError>;
        pub(crate) fn get_activator(&self) -> Result<Activator, QuakeLiveEngineError>;
        pub(crate) fn get_health(&self) -> i32;
        pub(crate) fn set_health(&mut self, new_health: i32);
        pub(crate) fn slay_with_mod(&mut self, mean_of_death: meansOfDeath_t);
        pub(crate) fn in_use(&self) -> bool;
        pub(crate) fn get_classname(&self) -> Cow<'_, str>;
        pub(crate) fn is_game_item(&self, item_type: entityType_t) -> bool;
        pub(crate) fn is_respawning_weapon(&self) -> bool;
        pub(crate) fn set_respawn_time(&mut self, respawn_time: i32);
        pub(crate) fn has_flags(&self) -> bool;
        pub(crate) fn is_dropped_item(&self) -> bool;
        pub(crate) fn get_client_number(&self) -> i32;
        pub(crate) fn drop_holdable(&mut self);
        pub(crate) fn is_kamikaze_timer(&self) -> bool;
        pub(crate) fn free_entity(&mut self);
        pub(crate) fn replace_item(&mut self, item_id: i32);
        pub(crate) fn get_targetting_entity_ids(&self) -> Vec<u32>;
        pub(crate) fn set_next_think(&mut self, next_think: i32);
        pub(crate) fn set_think(&mut self, think: Option<unsafe extern "C" fn(*mut gentity_t)>);
        pub(crate) fn set_touch(
            &mut self,
            touch: Option<unsafe extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>,
        );
        pub(crate) fn set_parent(&mut self, parent: &mut gentity_t);
        pub(crate) fn set_position_trace_time(&mut self, trace_time: i32);
    }

    impl AsMut<gentity_t> for GameEntity {
        fn as_mut(&mut self) -> &mut gentity_t;
    }

    impl From<i32> for GameEntity {
        fn from(entity_id: i32) -> Self;
    }

    impl TryFrom<*mut gentity_t> for GameEntity {
        type Error = QuakeLiveEngineError;
        fn try_from(gentity: *mut gentity_t) -> Result<Self, QuakeLiveEngineError>;
    }
}

#[cfg(test)]
pub(crate) struct MockGameEntityBuilder(Option<MockGameEntity>);

#[cfg(test)]
impl Default for MockGameEntityBuilder {
    fn default() -> Self {
        Self(Some(MockGameEntity::new()))
    }
}

#[cfg(test)]
impl MockGameEntityBuilder {
    pub(crate) fn with_player_name<F, G>(mut self, player_name: F, times: G) -> Self
    where
        F: FnMut() -> String + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_player_name()
                .returning_st(player_name)
                .times(times)
        });
        self
    }

    pub(crate) fn with_team<F, G>(mut self, team: F, times: G) -> Self
    where
        F: FnMut() -> team_t + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_team()
                .returning_st(team)
                .times(times)
        });
        self
    }

    pub(crate) fn with_privileges<F, G>(mut self, privileges: F, times: G) -> Self
    where
        F: FnMut() -> privileges_t + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_privileges()
                .returning_st(privileges)
                .times(times)
        });
        self
    }

    pub(crate) fn with_health<F>(mut self, health: i32, times: F) -> Self
    where
        F: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_health()
                .return_const_st(health)
                .times(times)
        });
        self
    }

    pub(crate) fn with_set_health<F, G>(mut self, health: F, times: G) -> Self
    where
        F: mockall::Predicate<i32> + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_set_health()
                .with(health)
                .times(times)
        });
        self
    }

    pub(crate) fn with_game_client<F>(mut self, returned: F) -> Self
    where
        F: FnMut() -> Result<MockGameClient, QuakeLiveEngineError> + 'static,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_game_client()
                .returning_st(returned)
        });
        self
    }

    pub(crate) fn with_targetting_entity_ids<F, G>(mut self, entities: F, times: G) -> Self
    where
        F: FnMut() -> Vec<u32> + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_targetting_entity_ids()
                .returning_st(entities)
                .times(times)
        });
        self
    }

    pub(crate) fn with_slay_with_mod<F, G>(mut self, means_of_death: F, times: G) -> Self
    where
        F: mockall::Predicate<meansOfDeath_t> + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_slay_with_mod()
                .with(means_of_death)
                .times(times)
        });
        self
    }

    pub(crate) fn with_in_use<F>(mut self, in_use: bool, times: F) -> Self
    where
        F: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_in_use()
                .return_const(in_use)
                .times(times)
        });
        self
    }

    pub(crate) fn with_client_number<F>(mut self, client_number: i32, times: F) -> Self
    where
        F: Into<mockall::TimesRange>,
    {
        self.0.as_mut().map(|mock_game_entity| {
            mock_game_entity
                .expect_get_client_number()
                .return_const(client_number)
                .times(times)
        });
        self
    }

    fn build(&mut self) -> MockGameEntity {
        self.0.take().unwrap_or_default()
    }

    pub(crate) fn run<F, G>(mut self, predicate: F, execute: G)
    where
        F: mockall::Predicate<i32> + Send + 'static,
        G: FnOnce(),
    {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate)
            .returning_st(move |_| self.build());
        execute();
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
mockall::mock! {
    StaticFunc {
        pub(crate) extern "C" fn touch_item(entity: *mut gentity_t, other: *mut gentity_t, trace: *mut trace_t);
        pub(crate) extern "C" fn g_free_entity(entity: *mut gentity_t);
    }
}

#[cfg(test)]
mod game_entity_tests {
    use core::{borrow::BorrowMut, ffi::c_int};

    use mockall::predicate;
    use pretty_assertions::assert_eq;

    use super::{GameEntity, MockStaticFunc, ShiNQlx_Switch_Touch_Item, ShiNQlx_Touch_Item};
    use crate::{ffi::c::prelude::*, prelude::*};

    #[test]
    #[serial]
    fn shinqlx_touch_item_with_no_main_engine() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut other_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut trace = TraceBuilder::default()
            .build()
            .expect("this should not happen");

        ShiNQlx_Touch_Item(
            entity.borrow_mut(),
            other_entity.borrow_mut(),
            trace.borrow_mut(),
        );
    }

    #[test]
    #[serial]
    fn shinqlx_touch_item_with_unintialized_main_engine() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut other_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut trace = TraceBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            })
            .run(|| {
                ShiNQlx_Touch_Item(
                    entity.borrow_mut(),
                    other_entity.borrow_mut(),
                    trace.borrow_mut(),
                );
            });
    }

    #[test]
    #[serial]
    fn shinqlx_touch_item_with_null_entity() {
        let mut other_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut trace = TraceBuilder::default()
            .build()
            .expect("this should not happen");

        let touch_item_ctx = MockStaticFunc::touch_item_context();
        touch_item_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MockStaticFunc::touch_item));
            })
            .run(|| {
                ShiNQlx_Touch_Item(
                    ptr::null_mut() as *mut gentity_t,
                    other_entity.borrow_mut(),
                    trace.borrow_mut(),
                );
            });
    }

    #[test]
    #[serial]
    fn shinqlx_touch_item_with_entity_touched_by_parent() {
        let mut other_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut entity = GEntityBuilder::default()
            .parent(other_entity.borrow_mut())
            .build()
            .expect("this should not happen");
        let mut trace = TraceBuilder::default()
            .build()
            .expect("this should not happen");

        let touch_item_ctx = MockStaticFunc::touch_item_context();
        touch_item_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MockStaticFunc::touch_item));
            })
            .run(|| {
                ShiNQlx_Touch_Item(
                    entity.borrow_mut(),
                    other_entity.borrow_mut(),
                    trace.borrow_mut(),
                );
            });
    }

    #[test]
    #[serial]
    fn shinqlx_touch_item_with_entity_touched_by_non_parent() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut other_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut trace = TraceBuilder::default()
            .build()
            .expect("this should not happen");

        let touch_item_ctx = MockStaticFunc::touch_item_context();
        touch_item_ctx.expect().times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MockStaticFunc::touch_item));
            })
            .run(|| {
                ShiNQlx_Touch_Item(
                    entity.borrow_mut(),
                    other_entity.borrow_mut(),
                    trace.borrow_mut(),
                );
            });
    }

    #[test]
    #[serial]
    fn shinqlx_switch_touch_item_with_no_main_engine() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        ShiNQlx_Switch_Touch_Item(entity.borrow_mut());
    }

    #[test]
    #[serial]
    fn shinqlx_switch_touch_item_with_unintialized_main_engine() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_engine
                    .expect_g_free_entity_orig()
                    .returning(|| Ok(MOCK_FREE_ENTITY_FN));
            })
            .run(|| {
                ShiNQlx_Switch_Touch_Item(entity.borrow_mut());
            });
    }

    static MOCK_TOUCH_ITEM_FN: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t) =
        MockStaticFunc::touch_item;
    static MOCK_FREE_ENTITY_FN: extern "C" fn(*mut gentity_t) = MockStaticFunc::g_free_entity;

    #[test]
    #[serial]
    fn shinqlx_switch_touch_item_with_partially_intialized_main_engine() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MOCK_TOUCH_ITEM_FN));
                mock_engine
                    .expect_g_free_entity_orig()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            })
            .run(|| {
                ShiNQlx_Switch_Touch_Item(entity.borrow_mut());
            });
    }

    #[test]
    #[serial]
    fn shinqlx_switch_touch_item_with_null_entity() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MOCK_TOUCH_ITEM_FN));
                mock_engine
                    .expect_g_free_entity_orig()
                    .returning(|| Ok(MOCK_FREE_ENTITY_FN));
            })
            .run(|| {
                ShiNQlx_Switch_Touch_Item(ptr::null_mut() as *mut gentity_t);
            });
    }

    #[test]
    #[serial]
    fn shinqlx_switch_touch_item_with_sets_properties() {
        let mut entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        let current_level_ctx = MockCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_touch_item_orig()
                    .returning(|| Ok(MOCK_TOUCH_ITEM_FN));
                mock_engine
                    .expect_g_free_entity_orig()
                    .returning(|| Ok(MOCK_FREE_ENTITY_FN));
            })
            .run(|| {
                ShiNQlx_Switch_Touch_Item(entity.borrow_mut());

                assert!(
                    entity
                        .touch
                        .is_some_and(|func| func as usize == MOCK_TOUCH_ITEM_FN as usize)
                );
                assert!(
                    entity
                        .think
                        .is_some_and(|func| func as usize == MOCK_FREE_ENTITY_FN as usize)
                );
                assert_eq!(entity.nextthink, 30234);
            });
    }

    #[test]
    fn game_entity_try_from_null_results_in_error() {
        assert_eq!(
            GameEntity::try_from(ptr::null_mut() as *mut gentity_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string()
            ))
        );
    }

    #[test]
    fn game_entity_try_from_valid_gentity() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t).is_ok(),
            true
        );
    }

    #[test]
    #[serial]
    fn game_entity_try_from_negative_entity_id() {
        assert_eq!(
            GameEntity::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    #[serial]
    fn game_entity_try_from_too_large_i32_entity_id() {
        assert_eq!(
            GameEntity::try_from(65536i32),
            Err(QuakeLiveEngineError::InvalidId(65536))
        );
    }

    #[test]
    #[serial]
    fn game_entity_try_from_valid_i32_gentities_not_initialized() {
        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning(|| ptr::null_mut() as *mut gentity_t);

        assert_eq!(
            GameEntity::try_from(42i32),
            Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".to_string()
            ))
        );
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn game_entity_try_from_valid_i32_gentities_initialized() {
        let mut gentities = vec![
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
        ];
        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning_st(move || gentities[0].borrow_mut() as *mut gentity_t);

        assert!(GameEntity::try_from(2i32).is_ok());
    }

    #[test]
    #[serial]
    fn game_entity_try_from_too_large_u32_entity_id() {
        assert_eq!(
            GameEntity::try_from(65536u32),
            Err(QuakeLiveEngineError::InvalidId(65536))
        );
    }

    #[test]
    #[serial]
    fn game_entity_try_from_valid_u32_gentities_not_initialized() {
        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning(|| ptr::null_mut() as *mut gentity_t);

        assert_eq!(
            GameEntity::try_from(42u32),
            Err(QuakeLiveEngineError::EntityNotFound(
                "g_entities not initialized".to_string()
            ))
        );
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn game_entity_try_from_valid_u32_gentities_initialized() {
        let mut gentities = vec![
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
        ];
        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning_st(move || gentities[0].borrow_mut() as *mut gentity_t);

        assert!(GameEntity::try_from(2u32).is_ok());
    }

    #[test]
    #[serial]
    fn game_entity_get_entities_list_with_no_main_engine() {
        assert!(GameEntity::get_entities_list_real().is_null());
    }

    #[test]
    #[serial]
    fn game_entity_get_entities_list_with_no_g_run_frame_orig() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_g_run_frame_orig()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            })
            .run(|| {
                assert!(GameEntity::get_entities_list_real().is_null());
            });
    }

    #[test]
    #[serial]
    fn game_entity_get_entity_id_with_no_entities_list() {
        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning(|| ptr::null_mut() as *mut gentity_t);

        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_entity_id(), -1);
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn game_entity_get_entity_id_gets_offset() {
        let mut gentities = vec![
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
            GEntityBuilder::default()
                .build()
                .expect("this should not happen"),
        ];

        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning_st(move || gentities[0].borrow_mut() as *mut gentity_t);

        let game_entity = GameEntity::try_from(3).expect("this should not happen");
        assert_eq!(game_entity.get_entity_id(), 3);
    }

    #[test]
    #[serial]
    fn game_entity_start_kamikaze_with_no_main_engine() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        game_entity.start_kamikaze();
    }

    #[test]
    #[serial]
    fn game_entity_start_kamikaze() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_start_kamikaze().times(1);
            })
            .run(|| {
                game_entity.start_kamikaze();
            });
    }

    #[test]
    #[serial]
    fn game_entity_get_player_name_from_null_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_player_name(), "");
    }

    #[test]
    #[serial]
    fn game_entity_get_player_name_from_disconnected_game_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_connection_state()
                .return_const(clientConnected_t::CON_DISCONNECTED);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_player_name(), "");
    }

    #[test]
    #[serial]
    fn game_entity_get_player_name_from_connected_game_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_connection_state()
                .return_const(clientConnected_t::CON_CONNECTED);
            mock_game_client
                .expect_get_player_name()
                .return_const("UnknownPlayer");
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_player_name(), "UnknownPlayer");
    }

    #[test]
    #[serial]
    fn game_entity_get_team_from_null_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_team(), team_t::TEAM_SPECTATOR);
    }

    #[test]
    #[serial]
    fn game_entity_get_team_from_disconnected_game_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_connection_state()
                .return_const(clientConnected_t::CON_DISCONNECTED);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_team(), team_t::TEAM_SPECTATOR);
    }

    #[test]
    #[serial]
    fn game_entity_get_team_from_connected_game_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_connection_state()
                .return_const(clientConnected_t::CON_CONNECTED);
            mock_game_client
                .expect_get_team()
                .return_const(team_t::TEAM_RED);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_team(), team_t::TEAM_RED);
    }

    #[test]
    #[serial]
    fn game_entity_get_privileges_from_null_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_privileges(), privileges_t::PRIV_BANNED);
    }

    #[test]
    #[serial]
    fn game_entity_get_privileges_from_connected_game_client() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_privileges()
                .return_const(privileges_t::PRIV_ROOT);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_privileges(), privileges_t::PRIV_ROOT);
    }

    #[test]
    #[serial]
    fn game_entity_get_game_client_when_none_is_set() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_game_client().is_err(), true);
    }

    #[test]
    #[serial]
    fn game_entity_get_game_client_with_valid_gclient() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .returning(|_| Ok(MockGameClient::new()));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_game_client().is_ok(), true);
    }

    #[test]
    #[serial]
    fn game_entity_get_activator_when_none_is_set() {
        let activator_try_from_ctx = MockActivator::try_from_context();
        activator_try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_activator().is_err(), true);
    }

    #[test]
    #[serial]
    fn game_entity_get_activator_with_valid_activator() {
        let activator_try_from_ctx = MockActivator::try_from_context();
        activator_try_from_ctx
            .expect()
            .returning(|_| Ok(MockActivator::new()));
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_activator().is_ok(), true);
    }

    #[test]
    fn game_entity_set_health() {
        let mut gclient = GClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .client(gclient.borrow_mut() as *mut gclient_t)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_health(666);
        assert_eq!(game_entity.get_health(), 666);
    }

    #[test]
    #[serial]
    fn game_entity_slay_with_mod_with_no_main_engine() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_armor()
                .with(predicate::eq(0))
                .times(1);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .health(42)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        game_entity.slay_with_mod(meansOfDeath_t::MOD_CRUSH);
    }

    #[test]
    #[serial]
    fn game_entity_slay_with_mod() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_armor()
                .with(predicate::eq(0))
                .times(1);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .health(42)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_register_damage().withf(
                    |_target,
                     _inflictor,
                     _attacker,
                     _dir,
                     _pos,
                     damage,
                     dmg_flags,
                     mean_of_death| {
                        *damage == 84
                            && *dmg_flags == DAMAGE_NO_PROTECTION as c_int
                            && *mean_of_death == meansOfDeath_t::MOD_CRUSH as c_int
                    },
                );
            })
            .run(|| {
                game_entity.slay_with_mod(meansOfDeath_t::MOD_CRUSH);
            });
    }

    #[test]
    #[serial]
    fn game_entity_slay_with_kamikaze() {
        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_armor()
                .with(predicate::eq(0))
                .times(1);
            Ok(mock_game_client)
        });
        let mut gentity = GEntityBuilder::default()
            .health(123)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_register_damage().withf(
                    |_target,
                     _inflictor,
                     _attacker,
                     _dir,
                     _pos,
                     damage,
                     dmg_flags,
                     mean_of_death| {
                        *damage == 200246
                            && *dmg_flags == DAMAGE_NO_PROTECTION as c_int
                            && *mean_of_death == meansOfDeath_t::MOD_KAMIKAZE as c_int
                    },
                );
            })
            .run(|| {
                game_entity.slay_with_mod(meansOfDeath_t::MOD_KAMIKAZE);
            });
    }

    #[test]
    fn game_entity_in_use() {
        let mut gentity = GEntityBuilder::default()
            .inuse(qboolean::qtrue)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.in_use(), true);
    }

    #[test]
    fn game_entity_get_classname() {
        let classname = c"entity classname";
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_classname(), "entity classname");
    }

    #[test]
    fn game_entity_is_game_item() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_game_item(entityType_t::ET_ITEM), true);
        assert_eq!(game_entity.is_game_item(entityType_t::ET_PLAYER), false);
    }

    #[test]
    fn game_entity_is_respawning_weapon_for_player_entity() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_PLAYER as i32)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    fn game_entity_is_respawning_weapon_for_null_item() {
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_PLAYER as i32)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(ptr::null() as *const gitem_t)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    fn game_entity_is_respawning_weapon_for_non_weapon() {
        let gitem = GItemBuilder::default()
            .giType(itemType_t::IT_AMMO)
            .build()
            .expect("this should not happen");
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(&gitem as *const gitem_t)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_respawning_weapon(), false);
    }

    #[test]
    fn game_entity_is_respawning_weapon_for_an_actual_weapon() {
        let gitem = GItemBuilder::default()
            .giType(itemType_t::IT_WEAPON)
            .build()
            .expect("this should not happen");
        let entity_state = EntityStateBuilder::default()
            .eType(entityType_t::ET_ITEM as i32)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .item(&gitem as *const gitem_t)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_respawning_weapon(), true);
    }

    #[test]
    fn game_entity_set_respawn_time() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_respawn_time(42);
        assert_eq!(gentity.wait, 42.0);
    }

    #[test]
    fn game_entity_has_flags_with_no_flags() {
        let mut gentity = GEntityBuilder::default()
            .flags(0)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.has_flags(), false);
    }

    #[test]
    fn game_entity_has_flags_with_flags_set() {
        let mut gentity = GEntityBuilder::default()
            .flags(42)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.has_flags(), true);
    }

    #[test]
    fn game_entity_is_dropped_item_for_non_dropped_item() {
        let mut gentity = GEntityBuilder::default()
            .flags(FL_FORCE_GESTURE as i32)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_dropped_item(), false);
    }

    #[test]
    fn game_entity_is_dropped_item_for_dropped_item() {
        let mut gentity = GEntityBuilder::default()
            .flags(FL_DROPPED_ITEM as i32)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_dropped_item(), true);
    }

    #[test]
    fn game_entity_get_client_number() {
        let entity_state = EntityStateBuilder::default()
            .clientNum(42)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .s(entity_state)
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.get_client_number(), 42);
    }

    #[test]
    #[serial]
    fn game_entity_drop_holdable_with_no_main_engine() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        game_entity.drop_holdable()
    }

    #[test]
    #[serial]
    fn game_entity_drop_holdable_with_no_game_client() {
        let current_level_ctx = MockCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx
            .expect()
            .returning_st(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default().run(|| game_entity.drop_holdable());
    }

    #[test]
    #[serial]
    fn game_entity_drop_holdable_with_item_on_game_client() {
        let current_level_ctx = MockCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut current_level = MockCurrentLevel::new();
            current_level.expect_get_leveltime().return_const(1234);
            Ok(current_level)
        });

        let game_client_try_from_ctx = MockGameClient::try_from_context();
        game_client_try_from_ctx.expect().returning_st(|_| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_holdable().returning_st(|| -1);
            Ok(mock_game_client)
        });

        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default().run(|| game_entity.drop_holdable());
    }

    #[test]
    fn game_entity_is_kamikaze_timer_for_non_kamikaze_timer() {
        let classname = c"no kamikaze timer";
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_kamikaze_timer(), false);
    }

    #[test]
    fn game_entity_is_kamikaze_timer_for_kamikaze_timer() {
        let classname = c"kamikaze timer";
        let mut gentity = GEntityBuilder::default()
            .classname(classname.as_ptr())
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(game_entity.is_kamikaze_timer(), true);
    }

    #[test]
    #[serial]
    fn game_entity_free_entity_with_no_main_engine() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        game_entity.free_entity();
    }

    #[test]
    #[serial]
    fn game_entity_free_entity() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_free_entity();
            })
            .run(|| {
                game_entity.free_entity();
            });
    }

    #[test]
    #[serial]
    fn game_entity_replace_item_with_no_main_engine() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        game_entity.replace_item(42);
    }

    #[test]
    #[serial]
    fn game_entity_replace_item_with_no_replacement() {
        let class_name = c"class_name";
        let mut gentity = GEntityBuilder::default()
            .classname(class_name.as_ptr())
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("class_name"), 0..)
            .configure(|mock_engine| {
                mock_engine.expect_free_entity();
            })
            .run(|| {
                game_entity.replace_item(0);
            });
    }

    #[test]
    fn game_entity_get_targetting_entity_ids_for_no_targetname() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        assert_eq!(game_entity.get_targetting_entity_ids(), Vec::<u32>::new());
    }

    #[test]
    fn game_entity_set_next_think() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_next_think(1337);

        assert_eq!(gentity.nextthink, 1337);
    }

    static SWITCH_TOUCH_ITEM_FN: unsafe extern "C" fn(*mut gentity_t) = ShiNQlx_Switch_Touch_Item;

    #[test]
    fn game_entity_set_think_to_none() {
        let mut gentity = GEntityBuilder::default()
            .think(Some(SWITCH_TOUCH_ITEM_FN))
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_think(None);

        assert!(
            #[allow(unpredictable_function_pointer_comparisons)]
            gentity.think.is_none()
        );
    }

    #[test]
    fn game_entity_set_think_to_some_value() {
        let mut gentity = GEntityBuilder::default()
            .think(None)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_think(Some(SWITCH_TOUCH_ITEM_FN));

        assert!(
            gentity
                .think
                .is_some_and(|func| func as usize == SWITCH_TOUCH_ITEM_FN as usize)
        );
    }

    static TOUCH_ITEM_FN: unsafe extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t) =
        ShiNQlx_Touch_Item;

    #[test]
    fn game_entity_set_touch_to_none() {
        let mut gentity = GEntityBuilder::default()
            .touch(Some(TOUCH_ITEM_FN))
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_touch(None);

        assert!(
            #[allow(unpredictable_function_pointer_comparisons)]
            gentity.touch.is_none()
        );
    }

    #[test]
    fn game_entity_set_touch_to_some_value() {
        let mut gentity = GEntityBuilder::default()
            .touch(None)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_touch(Some(TOUCH_ITEM_FN));

        assert!(
            gentity
                .touch
                .is_some_and(|func| func as usize == TOUCH_ITEM_FN as usize)
        );
    }

    #[test]
    fn game_entity_set_parent_to_some_value() {
        let mut gentity = GEntityBuilder::default()
            .parent(ptr::null_mut() as *mut gentity_t)
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");

        let mut parent_entity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        game_entity.set_parent(parent_entity.borrow_mut());

        assert_eq!(gentity.parent, parent_entity.borrow_mut() as *mut gentity_t);
    }

    #[test]
    fn game_entity_set_position_trace_time() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_entity = GameEntity::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        game_entity.set_position_trace_time(1337);

        assert_eq!(gentity.s.pos.trTime, 1337);
    }
}
