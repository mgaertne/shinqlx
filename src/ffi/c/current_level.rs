use core::ffi::c_char;

use tap::{TapOptional, TryConv};

use super::prelude::*;
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;
use crate::{MAIN_ENGINE, prelude::*};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CurrentLevel<'a> {
    level: &'a mut level_locals_t,
}

impl TryFrom<*mut level_locals_t> for CurrentLevel<'_> {
    type Error = QuakeLiveEngineError;

    fn try_from(level_locals: *mut level_locals_t) -> Result<Self, Self::Error> {
        unsafe { level_locals.as_mut() }
            .map(|level| Self { level })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string(),
            ))
    }
}

const OFFSET_LEVEL: usize = 0x4A1;

impl CurrentLevel<'_> {
    pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError> {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(QuakeLiveEngineError::MainEngineNotInitialized),
            |main_engine| {
                let func_pointer = main_engine.g_init_game_orig()?;
                let base_address = unsafe {
                    ptr::read_unaligned((func_pointer as usize + OFFSET_LEVEL) as *const i32)
                };
                let level_ptr = base_address as usize + func_pointer as usize + OFFSET_LEVEL + 4;
                Self::try_from(level_ptr as *mut level_locals_t)
            },
        )
    }

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
        MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
            let actual_vote_time = vote_time.unwrap_or(30);

            let mut vote_bytes = vote.bytes();
            self.level.voteString[0..vote.len()].fill_with(|| vote_bytes.next().unwrap() as c_char);
            self.level.voteString[vote.len()..].fill(0 as c_char);

            let mut vote_disp_bytes = vote_disp.bytes();
            self.level.voteDisplayString[0..vote_disp.len()]
                .fill_with(|| vote_disp_bytes.next().unwrap() as c_char);
            self.level.voteDisplayString[vote_disp.len()..].fill(0 as c_char);

            self.level.voteTime = self.level.time - 30000 + actual_vote_time * 1000;
            self.level.voteYes = 0;
            self.level.voteNo = 0;

            let maxclients = main_engine.get_max_clients();

            (0..maxclients)
                .filter_map(|client_id| client_id.try_conv::<GameEntity>().ok())
                .filter_map(|game_entity| game_entity.get_game_client().ok())
                .for_each(|mut game_client| game_client.set_vote_pending());

            shinqlx_set_configstring(CS_VOTE_STRING, vote_disp);
            shinqlx_set_configstring(CS_VOTE_TIME, &format!("{}", self.level.voteTime));
            shinqlx_set_configstring(CS_VOTE_YES, "0");
            shinqlx_set_configstring(CS_VOTE_NO, "0");
        });
    }

    pub(crate) fn set_training_map(&mut self, is_training_map: bool) {
        self.level.mapIsTrainingMap = is_training_map.into();
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
mockall::mock! {
    pub(crate) CurrentLevel {
        pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError>;
        pub(crate) fn get_vote_time(&self) -> Option<i32>;
        pub(crate) fn get_leveltime(&self) -> i32;
        pub(crate) fn callvote(&mut self, vote: &str, vote_disp: &str, vote_time: Option<i32>);
        pub(crate) fn set_training_map(&mut self, is_training_map: bool);
    }
}

#[cfg(test)]
mod current_level_tests {
    use core::{borrow::BorrowMut, ffi::CStr};

    use mockall::predicate;
    use pretty_assertions::assert_eq;

    use super::CurrentLevel;
    use crate::{
        ffi::c::prelude::*, hooks::mock_hooks::shinqlx_set_configstring_context, prelude::*,
        quake_live_functions::QuakeLiveFunction::G_InitGame,
    };

    #[test]
    #[serial]
    fn current_level_default_panics_when_no_main_engine_found() {
        let result = CurrentLevel::try_get();

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("this should not happen"),
            QuakeLiveEngineError::MainEngineNotInitialized
        );
    }

    #[test]
    #[serial]
    fn current_level_default_panics_when_g_init_game_not_set() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_g_init_game_orig()
                    .return_once(|| Err(QuakeLiveEngineError::VmFunctionNotFound(G_InitGame)));
            })
            .run(|| {
                let result = CurrentLevel::try_get();

                assert!(result.is_err());
                assert_eq!(
                    result.expect_err("this should not happen"),
                    QuakeLiveEngineError::VmFunctionNotFound(G_InitGame)
                );
            });
    }

    #[test]
    fn current_level_from_null() {
        assert_eq!(
            CurrentLevel::try_from(ptr::null_mut()),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string()
            )),
        );
    }

    #[test]
    fn current_level_from_valid_level_locals() {
        let mut level = LevelLocalsBuilder::default()
            .build()
            .expect("this should not happen");
        assert!(CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t).is_ok())
    }

    #[test]
    fn current_level_get_vote_time_no_vote_running() {
        let mut level = LevelLocalsBuilder::default()
            .voteTime(0)
            .build()
            .expect("this should not happen");
        let current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        assert!(current_level.get_vote_time().is_none());
    }

    #[test]
    fn current_level_get_vote_time_vote_running() {
        let mut level = LevelLocalsBuilder::default()
            .voteTime(60)
            .build()
            .expect("this should not happen");
        let current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        assert_eq!(current_level.get_vote_time(), Some(60));
    }

    #[test]
    fn current_level_get_time() {
        let mut level = LevelLocalsBuilder::default()
            .time(1234)
            .build()
            .expect("this should not happen");
        let current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        assert_eq!(current_level.get_leveltime(), 1234);
    }

    #[test]
    fn current_level_set_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qfalse)
            .build()
            .expect("this should not happen");
        let mut current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        current_level.set_training_map(true);
        assert_eq!(level.mapIsTrainingMap, qboolean::qtrue);
    }

    #[test]
    fn current_level_unset_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qtrue)
            .build()
            .expect("this should not happen");
        let mut current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        current_level.set_training_map(false);
        assert_eq!(level.mapIsTrainingMap, qboolean::qfalse);
    }

    #[test]
    #[serial]
    fn current_level_callvote_with_no_main_engine() {
        let mut level = LevelLocalsBuilder::default()
            .build()
            .expect("this should not happen");
        let mut current_level = CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
            .expect("this should not happen");
        current_level.callvote("map thunderstruck", "map thunderstruck", None);
        assert_eq!(
            unsafe { CStr::from_ptr(level.voteString.as_ptr()) }.to_string_lossy(),
            ""
        );
    }

    #[test]
    #[serial]
    fn current_level_callvote_with_main_engine_set() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(
                predicate::eq(CS_VOTE_STRING),
                predicate::eq("map thunderstruck"),
            )
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_TIME), predicate::eq("42"))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_YES), predicate::eq("0"))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_NO), predicate::eq("0"))
            .times(1);

        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning(|| ptr::null_mut() as *mut gentity_t);
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_set_vote_pending().times(1);
                    Ok(mock_game_client)
                });
                mock_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_entity
        });

        MockEngineBuilder::default().with_max_clients(8).run(|| {
            let mut level = LevelLocalsBuilder::default()
                .time(42)
                .build()
                .expect("this should not happen");
            let mut current_level =
                CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
                    .expect("this should not happen");
            current_level.callvote("map thunderstruck", "map thunderstruck", None);
            assert_eq!(
                unsafe { CStr::from_ptr(level.voteString.as_ptr()) }.to_string_lossy(),
                "map thunderstruck"
            );
            assert_eq!(
                unsafe { CStr::from_ptr(level.voteDisplayString.as_ptr()) }.to_string_lossy(),
                "map thunderstruck"
            );
            assert_eq!(level.voteTime, 42);
            assert_eq!(level.voteYes, 0);
            assert_eq!(level.voteNo, 0);
        });
    }

    #[test]
    #[serial]
    fn current_level_callvote_with_vote_time_value() {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_STRING), predicate::eq("map asdf"))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_TIME), predicate::eq("12042"))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_YES), predicate::eq("0"))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_VOTE_NO), predicate::eq("0"))
            .times(1);

        let get_entities_list_ctx = MockGameEntity::get_entities_list_context();
        get_entities_list_ctx
            .expect()
            .returning(|| ptr::null_mut() as *mut gentity_t);
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_set_vote_pending().times(1);
                    Ok(mock_game_client)
                });
                mock_entity
            });
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_entity
        });

        MockEngineBuilder::default().with_max_clients(8).run(|| {
            let mut level = LevelLocalsBuilder::default()
                .time(42)
                .build()
                .expect("this should not happen");
            let mut current_level =
                CurrentLevel::try_from(level.borrow_mut() as *mut level_locals_t)
                    .expect("this should not happen");
            current_level.callvote("map campgrounds", "map asdf", Some(42));
            assert_eq!(
                unsafe { CStr::from_ptr(level.voteString.as_ptr()) }.to_string_lossy(),
                "map campgrounds"
            );
            assert_eq!(
                unsafe { CStr::from_ptr(level.voteDisplayString.as_ptr()) }.to_string_lossy(),
                "map asdf"
            );
            assert_eq!(level.voteTime, 12042);
            assert_eq!(level.voteYes, 0);
            assert_eq!(level.voteNo, 0);
        });
    }
}
