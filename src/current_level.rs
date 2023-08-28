#[cfg(test)]
use crate::current_level::mock_hooks::shinqlx_set_configstring;
use crate::game_entity::GameEntity;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;
use crate::prelude::*;
use crate::MAIN_ENGINE;
use core::ffi::c_char;
#[cfg(test)]
use mockall::automock;

#[cfg(test)]
#[automock]
#[allow(dead_code)]
mod hooks {
    pub(crate) fn shinqlx_set_configstring(_index: u32, _value: String) {}
}

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
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

const OFFSET_LEVEL: usize = 0x4A1;

impl CurrentLevel {
    pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError> {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return Err(QuakeLiveEngineError::MainEngineUnreadable);
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return Err(QuakeLiveEngineError::MainEngineNotInitialized);
        };

        let func_pointer = main_engine.g_init_game_orig()?;
        let base_address =
            unsafe { ptr::read_unaligned((func_pointer as usize + OFFSET_LEVEL) as *const i32) };
        let level_ptr = base_address as usize + func_pointer as usize + OFFSET_LEVEL + 4;
        Self::try_from(level_ptr as *mut level_locals_t)
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

    pub(crate) fn callvote<T, U>(&mut self, vote: T, vote_disp: U, vote_time: Option<i32>)
    where
        T: AsRef<str>,
        U: AsRef<str>,
    {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let actual_vote_time = vote_time.unwrap_or(30);

        let mut vote_bytes_iter = vote.as_ref().bytes();
        self.level.voteString[0..vote.as_ref().len()]
            .fill_with(|| vote_bytes_iter.next().unwrap() as c_char);
        self.level.voteString[vote.as_ref().len()..].fill(0 as c_char);

        let mut vote_disp_bytes_iter = vote_disp.as_ref().bytes();
        self.level.voteDisplayString[0..vote_disp.as_ref().len()]
            .fill_with(|| vote_disp_bytes_iter.next().unwrap() as c_char);
        self.level.voteDisplayString[vote_disp.as_ref().len()..].fill(0 as c_char);

        self.level.voteTime = self.level.time - 30000 + actual_vote_time * 1000;
        self.level.voteYes = 0;
        self.level.voteNo = 0;

        let maxclients = main_engine.get_max_clients();

        (0..maxclients)
            .filter_map(|client_id| GameEntity::try_from(client_id).ok())
            .filter_map(|game_entity| game_entity.get_game_client().ok())
            .for_each(|mut game_client| game_client.set_vote_pending());

        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(CS_VOTE_STRING, vote_disp.as_ref().to_string());
        shinqlx_set_configstring(CS_VOTE_TIME, format!("{}", self.level.voteTime));
        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(CS_VOTE_YES, "0".to_string());
        #[allow(clippy::unnecessary_to_owned)]
        shinqlx_set_configstring(CS_VOTE_NO, "0".to_string());
    }

    pub(crate) fn set_training_map(&mut self, is_training_map: bool) {
        self.level.mapIsTrainingMap = is_training_map.into();
    }
}

#[cfg(test)]
mod current_level_tests {
    use crate::current_level::mock_hooks::*;
    use crate::current_level::CurrentLevel;
    use crate::prelude::*;
    use crate::quake_live_functions::QuakeLiveFunction::G_InitGame;
    use crate::MAIN_ENGINE;
    use alloc::vec::Vec;
    use core::ffi::CStr;
    use core::sync::atomic::Ordering;
    use pretty_assertions::assert_eq;
    use std::panic::catch_unwind;

    #[test]
    #[serial]
    fn current_level_default_panics_when_no_main_engine_found() {
        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }
        let result = CurrentLevel::try_get();

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            QuakeLiveEngineError::MainEngineNotInitialized
        );
    }

    #[test]
    #[serial]
    fn current_level_default_panics_when_g_init_game_not_set() {
        {
            let mut guard = MAIN_ENGINE.write();
            *guard = Some(QuakeLiveEngine::new());
        }

        let result = CurrentLevel::try_get();

        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            QuakeLiveEngineError::VmFunctionNotFound(G_InitGame)
        );
    }

    #[test]
    fn current_level_from_null() {
        assert_eq!(
            CurrentLevel::try_from(ptr::null_mut()),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            )),
        );
    }

    #[test]
    fn current_level_from_valid_level_locals() {
        let mut level = LevelLocalsBuilder::default().build().unwrap();
        assert!(CurrentLevel::try_from(&mut level as *mut level_locals_t).is_ok())
    }

    #[test]
    fn current_level_get_vote_time_no_vote_running() {
        let mut level = LevelLocalsBuilder::default().voteTime(0).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert!(current_level.get_vote_time().is_none());
    }

    #[test]
    fn current_level_get_vote_time_vote_running() {
        let mut level = LevelLocalsBuilder::default().voteTime(60).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert_eq!(current_level.get_vote_time(), Some(60));
    }

    #[test]
    fn current_level_get_time() {
        let mut level = LevelLocalsBuilder::default().time(1234).build().unwrap();
        let current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        assert_eq!(current_level.get_leveltime(), 1234);
    }

    #[test]
    fn current_level_set_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qfalse)
            .build()
            .unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.set_training_map(true);
        assert_eq!(level.mapIsTrainingMap, qboolean::qtrue);
    }

    #[test]
    fn current_level_unset_training_map() {
        let mut level = LevelLocalsBuilder::default()
            .mapIsTrainingMap(qboolean::qtrue)
            .build()
            .unwrap();
        let mut current_level = CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
        current_level.set_training_map(false);
        assert_eq!(level.mapIsTrainingMap, qboolean::qfalse);
    }

    #[test]
    #[serial]
    fn current_level_callvote_with_no_main_engine() {
        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }
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
            ""
        );
    }

    #[test]
    #[serial]
    fn current_level_callvote_with_main_engine_set() {
        let main_engine = QuakeLiveEngine::new();
        main_engine.sv_maxclients.store(8, Ordering::SeqCst);

        {
            let mut guard = MAIN_ENGINE.write();
            *guard = Some(main_engine);
        }

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx.expect().return_const_st(());

        let result = catch_unwind(|| {
            let mut level = LevelLocalsBuilder::default().build().unwrap();
            let mut current_level =
                CurrentLevel::try_from(&mut level as *mut level_locals_t).unwrap();
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
        });

        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }

        assert!(result.is_ok());
    }
}
