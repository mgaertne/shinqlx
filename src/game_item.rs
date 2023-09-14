use crate::prelude::*;
use crate::quake_live_engine::{GameAddEvent, TryLaunchItem};
use crate::MAIN_ENGINE;
use alloc::string::String;
use core::ffi::{c_float, CStr};
use core::ops::Deref;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameItem {
    pub(crate) gitem_t: &'static mut gitem_t,
}

impl AsMut<gitem_t> for GameItem {
    fn as_mut(&mut self) -> &mut gitem_t {
        self.gitem_t
    }
}

impl TryFrom<*mut gitem_t> for GameItem {
    type Error = QuakeLiveEngineError;

    fn try_from(game_item: *mut gitem_t) -> Result<Self, Self::Error> {
        unsafe { game_item.as_mut() }
            .map(|gitem| Self { gitem_t: gitem })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl TryFrom<i32> for GameItem {
    type Error = QuakeLiveEngineError;

    fn try_from(item_id: i32) -> Result<Self, Self::Error> {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            return Err(QuakeLiveEngineError::InvalidId(item_id));
        }
        let bg_itemlist = GameItem::get_item_list();
        Self::try_from(unsafe { bg_itemlist.offset(item_id as isize) as *mut gitem_t })
            .map_err(|_| QuakeLiveEngineError::EntityNotFound("entity not found".into()))
    }
}

const OFFSET_BG_ITEMLIST: usize = 0x2A;

impl GameItem {
    pub(crate) fn get_num_items() -> i32 {
        let bg_itemlist = Self::get_item_list();
        if bg_itemlist.is_null() {
            return 0;
        }

        (1..=4096)
            .filter(|index| {
                let Some(item) = (unsafe { bg_itemlist.offset((*index) as isize).as_ref() }) else {
                    return false;
                };
                !item.classname.is_null()
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    fn get_item_list() -> *mut gitem_t {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return ptr::null_mut();
        };

        let Ok(launch_item_orig) = main_engine.launch_item_orig() else {
            return ptr::null_mut();
        };

        let base_address = unsafe {
            ptr::read_unaligned((launch_item_orig as usize + OFFSET_BG_ITEMLIST) as *const i32)
        };

        let bg_itemlist_ptr_ptr =
            base_address as usize + launch_item_orig as usize + OFFSET_BG_ITEMLIST + 4;

        let bg_itemlist_ptr = unsafe { ptr::read(bg_itemlist_ptr_ptr as *const u64) };
        bg_itemlist_ptr as *mut gitem_t
    }

    #[allow(unused)]
    pub(crate) fn get_item_id(&self) -> i32 {
        let bg_itemlist = Self::get_item_list();
        if bg_itemlist.is_null() {
            return -1;
        }
        self.get_item_id_intern(bg_itemlist)
    }

    #[cfg_attr(not(test), inline)]
    fn get_item_id_intern(&self, bg_itemlist: *mut gitem_t) -> i32 {
        i32::try_from(unsafe { (self.gitem_t as *const gitem_t).offset_from(bg_itemlist) })
            .unwrap_or(-1)
    }

    pub(crate) fn get_classname(&self) -> String {
        unsafe { CStr::from_ptr(self.gitem_t.classname) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn spawn(&mut self, origin: (i32, i32, i32)) {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return;
        };

        self.spawn_intern(origin, main_engine.deref());
    }

    #[cfg_attr(not(test), inline)]
    fn spawn_intern<'a, T>(&'a mut self, origin: (i32, i32, i32), quake_live_engine: &T)
    where
        T: TryLaunchItem<&'a mut GameItem> + for<'b> GameAddEvent<&'b mut GameEntity, i32>,
    {
        let mut origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let mut velocity = [0.0, 0.0, 0.9];

        let Ok(mut gentity) =
            quake_live_engine.try_launch_item(self, &mut origin_vec, &mut velocity)
        else {
            return;
        };

        gentity.set_next_think(0);
        gentity.set_think(None);
        // make item be scaled up
        quake_live_engine.game_add_event(&mut gentity, entity_event_t::EV_ITEM_RESPAWN, 0);
    }
}

#[cfg(test)]
mod game_item_tests {
    use super::GameItem;
    use crate::game_entity::MockGameEntity;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use crate::MAIN_ENGINE;
    use alloc::ffi::CString;
    use core::ffi::c_char;
    use pretty_assertions::assert_eq;

    #[test]
    fn game_item_from_null_pointer() {
        assert_eq!(
            GameItem::try_from(ptr::null_mut() as *mut gitem_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn game_item_from_valid_item() {
        let mut gitem = GItemBuilder::default().build().unwrap();
        let game_item = GameItem::try_from(&mut gitem as *mut gitem_t);
        assert!(game_item.is_ok());
    }

    #[test]
    fn game_item_try_get_from_negative_item_id() {
        assert_eq!(
            GameItem::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    fn game_item_try_get_with_no_items_available() {
        assert_eq!(
            GameItem::try_from(42),
            Err(QuakeLiveEngineError::InvalidId(42))
        );
    }

    #[test]
    fn get_num_items_from_non_existing_item_list() {
        assert_eq!(GameItem::get_num_items(), 0);
    }

    #[test]
    fn get_item_list_with_no_main_engine() {
        assert!(GameItem::get_item_list().is_null());
    }

    #[test]
    #[serial]
    fn get_item_list_with_offset_function_not_defined_in_main_engine() {
        {
            MAIN_ENGINE.store(Some(QuakeLiveEngine::new().into()));
        }

        let result = GameItem::get_item_list();

        {
            MAIN_ENGINE.store(None);
        }

        assert!(result.is_null());
    }

    #[test]
    fn game_item_get_item_id_with_no_itemlist() {
        let mut gitem = GItemBuilder::default().build().unwrap();
        let game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        assert_eq!(game_item.get_item_id(), -1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn game_item_get_item_id_internal_gets_offset() {
        let mut itemlist = vec![
            GItemBuilder::default().build().unwrap(),
            GItemBuilder::default().build().unwrap(),
            GItemBuilder::default().build().unwrap(),
            GItemBuilder::default().build().unwrap(),
        ];
        let game_item = GameItem::try_from(&mut itemlist[1] as *mut gitem_t).unwrap();
        assert_eq!(game_item.get_item_id_intern(&mut itemlist[0]), 1);
    }

    #[test]
    fn game_item_get_classname() {
        let classname = CString::new("item classname").unwrap();
        let mut gitem = GItemBuilder::default()
            .classname(classname.as_ptr() as *mut c_char)
            .build()
            .unwrap();
        let game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        assert_eq!(game_item.get_classname(), "item classname")
    }

    #[test]
    fn game_item_spawn() {
        let mut mock_engine = MockQuakeEngine::new();
        let mut gitem = GItemBuilder::default().build().unwrap();
        let mut game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        mock_engine
            .expect_try_launch_item()
            .withf(|_, origin, velocity| origin == &[1.0, 2.0, 3.0] && velocity == &[0.0, 0.0, 0.9])
            .return_once(|_, _, _| {
                let mut game_entity = MockGameEntity::new();
                game_entity
                    .expect_set_next_think()
                    .withf(|&next_think| next_think == 0);
                game_entity
                    .expect_set_think()
                    .withf(|&think| think.is_none());
                Ok(game_entity)
            });
        mock_engine
            .expect_game_add_event()
            .withf(|_, event, param| event == &entity_event_t::EV_ITEM_RESPAWN && param == &0);
        game_item.spawn_intern((1, 2, 3), &mock_engine);
    }
}
