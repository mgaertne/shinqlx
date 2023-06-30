use crate::quake_live_engine::QuakeLiveEngineError::{
    EntityNotFound, InvalidId, NullPointerPassed,
};
use crate::quake_live_engine::{GameAddEvent, LaunchItem, QuakeLiveEngine, QuakeLiveEngineError};
use crate::quake_types::entity_event_t::EV_ITEM_RESPAWN;
use crate::quake_types::gitem_t;
use crate::LAUNCH_ITEM_ORIG_PTR;
use std::ffi::{c_float, CStr};
use std::sync::atomic::Ordering;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameItem {
    pub(crate) gitem_t: &'static mut gitem_t,
}

impl TryFrom<*mut gitem_t> for GameItem {
    type Error = QuakeLiveEngineError;

    fn try_from(game_item: *mut gitem_t) -> Result<Self, Self::Error> {
        unsafe {
            game_item
                .as_mut()
                .map(|gitem| Self { gitem_t: gitem })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl TryFrom<i32> for GameItem {
    type Error = QuakeLiveEngineError;

    fn try_from(item_id: i32) -> Result<Self, Self::Error> {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            return Err(InvalidId(item_id));
        }
        let bg_itemlist = GameItem::get_item_list();
        Self::try_from(unsafe { bg_itemlist.offset(item_id as isize) as *mut gitem_t })
            .map_err(|_| EntityNotFound("entity not found".into()))
    }
}

impl GameItem {
    pub(crate) fn get_num_items() -> i32 {
        let bg_itemlist = Self::get_item_list();
        if bg_itemlist.is_null() {
            return 0;
        }

        for i in 1..=4096 {
            let item = unsafe { bg_itemlist.offset(i) };
            if item.is_null() || unsafe { item.as_ref() }.unwrap().classname.is_null() {
                return i as i32;
            }
        }
        0
    }

    fn get_item_list() -> *mut gitem_t {
        let func_pointer = LAUNCH_ITEM_ORIG_PTR.load(Ordering::Relaxed);
        if func_pointer == 0 {
            return std::ptr::null_mut();
        };
        let base_address = unsafe { std::ptr::read_unaligned((func_pointer + 0x2A) as *const i32) };
        let bg_itemlist_ptr_ptr = base_address as u64 + func_pointer + 0x2A + 4;
        let bg_itemlist_ptr = unsafe { std::ptr::read(bg_itemlist_ptr_ptr as *const u64) };
        bg_itemlist_ptr as *mut gitem_t
    }

    #[allow(unused)]
    pub(crate) fn get_item_id(&mut self) -> i32 {
        let bg_itemlist = Self::get_item_list();
        i32::try_from(unsafe { (self.gitem_t as *mut gitem_t).offset_from(bg_itemlist) })
            .unwrap_or(-1)
    }

    pub(crate) fn get_classname(&self) -> String {
        unsafe {
            CStr::from_ptr(self.gitem_t.classname)
                .to_string_lossy()
                .into()
        }
    }

    pub(crate) fn spawn(&mut self, origin: (i32, i32, i32)) {
        self.spawn_internal(origin, &QuakeLiveEngine::default());
    }

    pub(crate) fn spawn_internal(
        &mut self,
        origin: (i32, i32, i32),
        quake_live_engine: &(impl LaunchItem + GameAddEvent),
    ) {
        let mut origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let mut velocity = [0.0, 0.0, 0.9];

        let mut gentity = quake_live_engine.launch_item(self, &mut origin_vec, &mut velocity);
        gentity.gentity_t.nextthink = 0;
        gentity.gentity_t.think = None;
        // make item be scaled up
        quake_live_engine.game_add_event(&mut gentity, EV_ITEM_RESPAWN, 0);
    }
}

#[cfg(test)]
pub(crate) mod game_item_tests {
    use crate::game_entity::GameEntity;
    use crate::game_item::GameItem;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_live_engine::{GameAddEvent, LaunchItem};
    use crate::quake_types::entity_event_t::EV_ITEM_RESPAWN;
    use crate::quake_types::{
        entity_event_t, gentity_t, gitem_t, vec3_t, GEntityBuilder, GItemBuilder,
    };
    use mockall::predicate::*;
    use mockall::*;
    use pretty_assertions::assert_eq;
    use std::ffi::{c_char, CString};

    #[test]
    pub(crate) fn game_item_from_null_pointer() {
        assert_eq!(
            GameItem::try_from(std::ptr::null_mut() as *mut gitem_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn game_item_from_valid_item() {
        let mut gitem = GItemBuilder::default().build().unwrap();
        let game_item = GameItem::try_from(&mut gitem as *mut gitem_t);
        assert!(game_item.is_ok());
    }

    #[test]
    pub(crate) fn game_item_get_classname() {
        let classname = CString::new("item classname").unwrap();
        let mut gitem = GItemBuilder::default()
            .classname(classname.as_ptr() as *mut c_char)
            .build()
            .unwrap();
        let game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        assert_eq!(game_item.get_classname(), "item classname")
    }

    #[test]
    pub(crate) fn game_item_spawn() {
        mock! {
            QuakeEngine {}
            impl LaunchItem for QuakeEngine {
                fn launch_item(&self, gitem: &mut GameItem, origin: &mut vec3_t, velocity: &mut vec3_t) -> GameEntity;
            }

            impl GameAddEvent for QuakeEngine {
                fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
            }
        }

        let mut mock = MockQuakeEngine::new();
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        let mut gitem = GItemBuilder::default().build().unwrap();
        let mut game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        mock.expect_launch_item()
            .withf_st(|_, origin, velocity| {
                origin == &[1.0, 2.0, 3.0] && velocity == &[0.0, 0.0, 0.9]
            })
            .return_once_st(|_, _, _| game_entity);
        mock.expect_game_add_event()
            .withf_st(|entity, event, param| {
                entity.gentity_t.nextthink == 0
                    && entity.gentity_t.think.is_none()
                    && event == &EV_ITEM_RESPAWN
                    && param == &0
            })
            .return_const(());
        game_item.spawn_internal((1, 2, 3), &mock);
    }
}
