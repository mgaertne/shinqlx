use crate::game_entity::GameEntity;
use crate::hooks::shinqlx_set_configstring;
use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::{level_locals_t, CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_TIME, CS_VOTE_YES};
use crate::MAIN_ENGINE;
use std::ffi::c_char;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CurrentLevel {
    level: &'static mut level_locals_t,
}

impl TryFrom<*mut level_locals_t> for CurrentLevel {
    type Error = QuakeLiveEngineError;

    fn try_from(level_locals: *mut level_locals_t) -> Result<Self, Self::Error> {
        unsafe { level_locals.as_mut() }
            .map(|level| Self { level })
            .ok_or(NullPointerPassed("null pointer passed".into()))
    }
}

const OFFSET_LEVEL: usize = 0x4A1;

impl Default for CurrentLevel {
    fn default() -> Self {
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            debug_println!("main quake live engine not readable.");
            panic!("main quake live engine not readable.");
        };

        let Some(ref main_engine) = *main_engine_guard else {
            debug_println!("main quake live engine not initialized.");
            panic!("main quake live engine not initialized.");
        };

        let Ok(func_pointer) = main_engine.g_init_game_orig() else {
            debug_println!("G_InitGame not initialized.");
            panic!("G_InitGame not initialized.");
        };
        let base_address = unsafe {
            std::ptr::read_unaligned((func_pointer as usize + OFFSET_LEVEL) as *const i32)
        };
        let level_ptr = base_address as usize + func_pointer as usize + OFFSET_LEVEL + 4;
        Self::try_from(level_ptr as *mut level_locals_t).unwrap()
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
        let Ok(main_engine_guard) = MAIN_ENGINE.read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let actual_vote_time = vote_time.unwrap_or(30);

        let mut vote_bytes_iter = vote.bytes();
        self.level.voteString[0..vote.len()]
            .fill_with(|| vote_bytes_iter.next().unwrap() as c_char);
        self.level.voteString[vote.len()..].fill(0 as c_char);

        let mut vote_disp_bytes_iter = vote_disp.bytes();
        self.level.voteDisplayString[0..vote_disp.len()]
            .fill_with(|| vote_disp_bytes_iter.next().unwrap() as c_char);
        self.level.voteDisplayString[vote_disp.len()..].fill(0 as c_char);

        self.level.voteTime = self.level.time - 30000 + actual_vote_time * 1000;
        self.level.voteYes = 0;
        self.level.voteNo = 0;

        let maxclients = main_engine.get_max_clients();

        (0..maxclients)
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_pending());

        shinqlx_set_configstring(CS_VOTE_STRING, vote_disp);
        shinqlx_set_configstring(CS_VOTE_TIME, format!("{}", self.level.voteTime).as_str());
        shinqlx_set_configstring(CS_VOTE_YES, "0");
        shinqlx_set_configstring(CS_VOTE_NO, "0");
    }

    pub(crate) fn set_training_map(&mut self, is_training_map: bool) {
        self.level.mapIsTrainingMap = is_training_map.into();
    }
}

#[cfg(test)]
pub(crate) mod current_level_tests {
    use crate::current_level::CurrentLevel;
    #[cfg(not(miri))]
    use crate::quake_live_engine::QuakeLiveEngine;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::{level_locals_t, qboolean, LevelLocalsBuilder};
    #[cfg(not(miri))]
    use crate::MAIN_ENGINE;
    use pretty_assertions::assert_eq;
    #[cfg(not(miri))]
    use std::ffi::CStr;
    #[cfg(not(miri))]
    use std::sync::atomic::Ordering;
    #[cfg(not(miri))]
    use test_context::{test_context, TestContext};

    #[cfg(not(miri))]
    struct QuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for QuakeLiveEngineContext {
        fn setup() -> Self {
            let main_engine = QuakeLiveEngine::new();
            main_engine.sv_maxclients.store(8, Ordering::Relaxed);

            let Ok(mut guard) = MAIN_ENGINE.write() else {
                assert!(false, "could not write MAIN_ENGINE");
                panic!("could not write MAIN_ENGINE");
            };
            *guard = Some(main_engine);

            Self {}
        }

        fn teardown(self) {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                assert!(false, "could not write MAIN_ENGINE");
                return;
            };
            *guard = None;
        }
    }

    #[cfg(not(miri))]
    struct NoQuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for NoQuakeLiveEngineContext {
        fn setup() -> Self {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                assert!(false, "could not write MAIN_ENGINE");
                panic!("could not write MAIN_ENGINE");
            };
            *guard = None;

            Self {}
        }
    }

    #[cfg(not(miri))]
    #[test_context(NoQuakeLiveEngineContext)]
    #[test]
    #[should_panic(expected = "main quake live engine not initialized")]
    pub(crate) fn current_level_default_panics_when_no_main_engine_found(
        _ctx: &mut NoQuakeLiveEngineContext,
    ) {
        CurrentLevel::default();
    }

    #[cfg(not(miri))]
    #[test_context(QuakeLiveEngineContext)]
    #[test]
    #[should_panic(expected = "G_InitGame not initialized.")]
    pub(crate) fn current_level_default_panics_when_g_init_game_not_set(
        _ctx: &mut QuakeLiveEngineContext,
    ) {
        CurrentLevel::default();
    }

    #[test]
    pub(crate) fn current_level_from_null() {
        assert_eq!(
            CurrentLevel::try_from(std::ptr::null_mut() as *mut level_locals_t),
            Err(NullPointerPassed("null pointer passed".into())),
        );
    }

    #[test]
    pub(crate) fn current_level_from_valid_level_locals() {
        let mut level = LevelLocalsBuilder::default().build().unwrap();
        assert!(CurrentLevel::try_from(&mut level as *mut level_locals_t).is_ok())
    }

    #[test]
    pub(crate) fn current_level_get_vote_time_no_vote_running() {
        let mut level = LevelLocalsBuilder::default().voteTime(0).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert!(current_level.get_vote_time().is_none());
    }

    #[test]
    pub(crate) fn current_level_get_vote_time_vote_running() {
        let mut level = LevelLocalsBuilder::default().voteTime(60).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert_eq!(current_level.get_vote_time(), Some(60));
    }

    #[test]
    pub(crate) fn current_level_get_time() {
        let mut level = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert_eq!(current_level.get_leveltime(), 1234);
    }

    #[test]
    pub(crate) fn current_level_set_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qfalse)
            .build()
            .unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.set_training_map(true);
        assert_eq!(level.mapIsTrainingMap, qboolean::qtrue);
    }

    #[test]
    pub(crate) fn current_level_unset_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qtrue)
            .build()
            .unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.set_training_map(false);
        assert_eq!(level.mapIsTrainingMap, qboolean::qfalse);
    }

    #[cfg(not(miri))]
    #[test_context(NoQuakeLiveEngineContext)]
    #[test]
    pub(crate) fn current_level_callvote_with_no_main_engine(_ctx: &mut NoQuakeLiveEngineContext) {
        let mut level = LevelLocalsBuilder::default().build().unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.callvote("map thunderstruck", "map thunderstruck", None);
        assert!(level.voteString.iter().all(|c| *c == 0));
    }

    #[cfg(not(miri))]
    #[test_context(QuakeLiveEngineContext)]
    #[test]
    pub(crate) fn current_level_callvote_with_main_engine_set(_ctx: &mut QuakeLiveEngineContext) {
        let mut level = LevelLocalsBuilder::default().build().unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.callvote("map thunderstruck", "map thunderstruck", None);
        assert_eq!(
            CStr::from_bytes_until_nul(
                &level
                    .voteString
                    .iter()
                    .map(|c| *c as u8)
                    .collect::<Vec<u8>>()
            )
            .unwrap()
            .to_string_lossy(),
            "map thunderstruck"
        );
    }
}
