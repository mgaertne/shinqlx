use alloc::borrow::Cow;
use core::{
    borrow::BorrowMut,
    ffi::{CStr, c_float},
    hint::cold_path,
};

use tap::{TapFallible, TapOptional};

use super::prelude::*;
use crate::{
    MAIN_ENGINE,
    prelude::*,
    quake_live_engine::{GameAddEvent, TryLaunchItem},
};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct GameItem {
    gitem_t: &'static mut gitem_t,
}

impl AsRef<gitem_t> for GameItem {
    fn as_ref(&self) -> &gitem_t {
        self.gitem_t
    }
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
                "null pointer passed".to_string(),
            ))
    }
}

impl TryFrom<i32> for GameItem {
    type Error = QuakeLiveEngineError;

    fn try_from(item_id: i32) -> Result<Self, Self::Error> {
        if item_id < 0 || item_id >= GameItem::get_num_items() {
            cold_path();
            return Err(QuakeLiveEngineError::InvalidId(item_id));
        }
        let bg_itemlist = GameItem::get_item_list();
        Self::try_from(unsafe { bg_itemlist.offset(item_id as isize) as *mut gitem_t }).map_err(
            |_| {
                cold_path();
                QuakeLiveEngineError::EntityNotFound("entity not found".to_string())
            },
        )
    }
}

const OFFSET_BG_ITEMLIST: usize = 0x2A;

impl GameItem {
    pub(crate) fn get_num_items() -> i32 {
        let bg_itemlist = Self::get_item_list();
        if bg_itemlist.is_null() {
            cold_path();
            return 0;
        }

        (1..=4096)
            .filter(|index| {
                (unsafe { bg_itemlist.offset((*index) as isize).as_ref() })
                    .is_some_and(|item| !item.classname.is_null())
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    fn get_item_list() -> *mut gitem_t {
        #[cfg(test)]
        if cfg!(test) {
            cold_path();
            return MockGameItem::get_mocked_item_list();
        }

        Self::get_item_list_real()
    }

    fn get_item_list_real() -> *mut gitem_t {
        MAIN_ENGINE
            .load()
            .as_ref()
            .map_or(ptr::null_mut(), |main_engine| {
                main_engine
                    .launch_item_orig()
                    .map_or(ptr::null_mut(), |launch_item_orig| {
                        let base_address = unsafe {
                            ptr::read_unaligned(
                                (launch_item_orig as usize + OFFSET_BG_ITEMLIST) as *const i32,
                            )
                        };

                        let bg_itemlist_ptr_ptr = base_address as usize
                            + launch_item_orig as usize
                            + OFFSET_BG_ITEMLIST
                            + 4;

                        let bg_itemlist_ptr =
                            unsafe { ptr::read(bg_itemlist_ptr_ptr as *const u64) };
                        bg_itemlist_ptr as *mut gitem_t
                    })
            })
    }

    pub(crate) fn get_classname(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(self.gitem_t.classname) }.to_string_lossy()
    }

    pub(crate) fn spawn(&mut self, origin: (i32, i32, i32)) {
        let mut origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let mut velocity = [0.0, 0.0, 0.9];

        MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
            let _ = main_engine
                .try_launch_item(self, origin_vec.borrow_mut(), velocity.borrow_mut())
                .tap_ok_mut(|gentity| {
                    gentity.set_next_think(0);
                    gentity.set_think(None);
                    // make item be scaled up
                    main_engine.game_add_event(gentity, entity_event_t::EV_ITEM_RESPAWN, 0);
                });
        });
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
mockall::mock! {
    pub(crate) GameItem {
        pub(crate) fn get_mocked_item_list() -> *mut gitem_t;
        pub(crate) fn get_num_items() -> i32;
        pub(crate) fn get_classname(&self) -> Cow<'_, str>;
        pub(crate) fn spawn(&mut self, _origin: (i32, i32, i32));
    }

    impl TryFrom<*mut gitem_t> for GameItem {
        type Error = QuakeLiveEngineError;
        fn try_from(game_item: *mut gitem_t) -> Result<Self, QuakeLiveEngineError> {}
    }

    impl From<i32> for GameItem {
        fn from(_item_id: i32) -> Self {}
    }

    impl AsMut<gitem_t> for GameItem {
        fn as_mut(&mut self) -> &mut gitem_t {}
    }

    impl AsRef<gitem_t> for GameItem {
        fn as_ref(&self) -> &gitem_t {}
    }
}

#[cfg(test)]
mod game_item_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;

    use super::GameItem;
    use crate::{ffi::c::prelude::*, prelude::*};

    #[test]
    fn game_item_from_null_pointer() {
        assert_eq!(
            GameItem::try_from(ptr::null_mut() as *mut gitem_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string()
            ))
        );
    }

    #[test]
    fn game_item_from_valid_item() {
        let mut gitem = GItemBuilder::default()
            .build()
            .expect("this should not happen");
        let game_item = GameItem::try_from(gitem.borrow_mut() as *mut gitem_t);
        assert!(game_item.is_ok());
    }

    #[test]
    #[serial]
    fn game_item_try_from_with_negative_item_id() {
        assert_eq!(
            GameItem::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    #[serial]
    fn game_entity_try_from_valid_i32_item_id_out_of_range() {
        let get_item_ctx = MockGameItem::get_mocked_item_list_context();
        get_item_ctx
            .expect()
            .returning_st(|| ptr::null_mut() as *mut gitem_t);

        assert_eq!(
            GameItem::try_from(42),
            Err(QuakeLiveEngineError::InvalidId(42))
        );
    }

    #[test]
    #[serial]
    fn game_item_try_get_with_no_items_available() {
        let get_item_ctx = MockGameItem::get_mocked_item_list_context();
        get_item_ctx
            .expect()
            .returning_st(|| ptr::null_mut() as *mut gitem_t);

        assert_eq!(
            GameItem::try_from(42),
            Err(QuakeLiveEngineError::InvalidId(42))
        );
    }

    #[test]
    #[serial]
    fn get_num_items_from_non_existing_item_list() {
        let get_item_ctx = MockGameItem::get_mocked_item_list_context();
        get_item_ctx
            .expect()
            .returning_st(|| ptr::null_mut() as *mut gitem_t);

        assert_eq!(GameItem::get_num_items(), 0);
    }

    #[test]
    #[serial]
    fn get_item_list_with_no_main_engine() {
        assert!(GameItem::get_item_list_real().is_null());
    }

    #[test]
    #[serial]
    fn get_item_list_with_offset_function_not_defined_in_main_engine() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_launch_item_orig()
                    .return_once(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            })
            .run(|| {
                let result = GameItem::get_item_list_real();

                assert!(result.is_null());
            });
    }

    #[test]
    #[serial]
    fn game_item_get_classname() {
        let classname = c"item classname";
        let mut gitem = GItemBuilder::default()
            .classname(classname.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let game_item =
            GameItem::try_from(gitem.borrow_mut() as *mut gitem_t).expect("this should not happen");
        assert_eq!(game_item.get_classname(), "item classname")
    }

    #[test]
    #[serial]
    fn game_item_spawn() {
        let mut gitem = GItemBuilder::default()
            .build()
            .expect("this should not happen");
        let mut game_item =
            GameItem::try_from(gitem.borrow_mut() as *mut gitem_t).expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_try_launch_item()
                    .withf(|_item, origin, velocity| {
                        origin == &[1.0, 2.0, 3.0] && velocity == &[0.0, 0.0, 0.9]
                    })
                    .return_once(|_item, _origin, _velocity| {
                        let mut game_entity = MockGameEntity::new();
                        game_entity.expect_set_next_think().with(predicate::eq(0));
                        game_entity.expect_set_think().with(predicate::eq(None));
                        Ok(game_entity)
                    });
                mock_engine
                    .expect_game_add_event()
                    .withf(|_entity, event, param| {
                        event == &entity_event_t::EV_ITEM_RESPAWN && param == &0
                    });
            })
            .run(|| {
                game_item.spawn((1, 2, 3));
            });
    }
}
