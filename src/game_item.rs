use crate::game_entity::GameEntity;
use crate::prelude::*;
use crate::quake_live_engine::{GameAddEvent, QuakeLiveEngineError, TryLaunchItem};
use crate::MAIN_ENGINE;
use alloc::string::String;
use core::ffi::{c_float, CStr};

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
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return core::ptr::null_mut();
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return core::ptr::null_mut();
        };

        let Ok(launch_item_orig) = main_engine.launch_item_orig() else {
            return core::ptr::null_mut();
        };

        let base_address = unsafe {
            core::ptr::read_unaligned(
                (launch_item_orig as usize + OFFSET_BG_ITEMLIST) as *const i32,
            )
        };

        let bg_itemlist_ptr_ptr =
            base_address as usize + launch_item_orig as usize + OFFSET_BG_ITEMLIST + 4;

        let bg_itemlist_ptr = unsafe { core::ptr::read(bg_itemlist_ptr_ptr as *const u64) };
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
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        self.spawn_intern(origin, main_engine);
    }

    #[cfg_attr(not(test), inline)]
    fn spawn_intern<'a, T>(&'a mut self, origin: (i32, i32, i32), quake_live_engine: &'a T)
    where
        T: TryLaunchItem<&'a mut GameItem> + GameAddEvent<GameEntity, i32>,
    {
        let mut origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let mut velocity = [0.0, 0.0, 0.9];

        let Ok(gentity) = quake_live_engine.try_launch_item(self, &mut origin_vec, &mut velocity)
        else {
            return;
        };

        gentity.gentity_t.nextthink = 0;
        gentity.gentity_t.think = None;
        // make item be scaled up
        quake_live_engine.game_add_event(gentity, entity_event_t::EV_ITEM_RESPAWN, 0);
    }
}

#[cfg(test)]
mod game_item_tests {
    use crate::game_entity::GameEntity;
    use crate::game_item::GameItem;
    use crate::prelude::*;
    use crate::quake_live_engine::{GameAddEvent, TryLaunchItem};
    use crate::MAIN_ENGINE;
    use alloc::ffi::CString;
    use core::ffi::c_char;
    use mockall::*;
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    #[test]
    fn game_item_from_null_pointer() {
        assert_eq!(
            GameItem::try_from(core::ptr::null_mut() as *mut gitem_t),
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
            let mut guard = MAIN_ENGINE.write();
            *guard = Some(QuakeLiveEngine::new());
        }

        let result = GameItem::get_item_list();

        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
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
        mock! {
            QuakeEngine {}
            impl TryLaunchItem<&mut GameItem> for QuakeEngine {
                fn try_launch_item<'a>(&self, gitem: &'a mut GameItem, origin: &mut vec3_t, velocity: &mut vec3_t) -> Result<GameEntity, QuakeLiveEngineError>;
            }

            impl GameAddEvent<GameEntity, i32> for QuakeEngine {
                fn game_add_event(&self, game_entity: GameEntity, event: entity_event_t, event_param: i32);
            }
        }

        let mut mock = MockQuakeEngine::new();
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let game_entity = GameEntity::try_from(&mut gentity as *mut gentity_t).unwrap();
        let mut gitem = GItemBuilder::default().build().unwrap();
        let mut game_item = GameItem::try_from(&mut gitem as *mut gitem_t).unwrap();
        mock.expect_try_launch_item()
            .withf_st(|_, origin, velocity| {
                origin == &[1.0, 2.0, 3.0] && velocity == &[0.0, 0.0, 0.9]
            })
            .return_once_st(|_, _, _| Ok(game_entity));
        mock.expect_game_add_event()
            .withf_st(|entity, event, param| {
                entity.gentity_t.nextthink == 0
                    && entity.gentity_t.think.is_none()
                    && event == &entity_event_t::EV_ITEM_RESPAWN
                    && param == &0
            })
            .return_const(());
        game_item.spawn_intern((1, 2, 3), &mock);
    }
}
