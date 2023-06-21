use crate::game_entity::GameEntity;
use crate::hooks::shinqlx_set_configstring;
use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::{level_locals_t, CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_TIME, CS_VOTE_YES};
use crate::SV_MAXCLIENTS;
use std::ffi::CString;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CurrentLevel {
    level: &'static mut level_locals_t,
}

impl TryFrom<*mut level_locals_t> for CurrentLevel {
    type Error = QuakeLiveEngineError;

    fn try_from(level_locals: *mut level_locals_t) -> Result<Self, Self::Error> {
        unsafe {
            level_locals
                .as_mut()
                .map(|level| Self { level })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl Default for CurrentLevel {
    fn default() -> Self {
        extern "C" {
            static level: *mut level_locals_t;
        }

        unsafe { Self::try_from(level).unwrap() }
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
        for (dest, src) in self.level.voteString.iter_mut().zip(
            CString::new(vote)
                .unwrap_or(CString::new("").unwrap())
                .as_bytes_with_nul()
                .iter(),
        ) {
            *dest = *src as _;
        }
        for (dest, src) in self.level.voteDisplayString.iter_mut().zip(
            CString::new(vote_disp)
                .unwrap_or(CString::new("").unwrap())
                .as_bytes_with_nul()
                .iter(),
        ) {
            *dest = *src as _;
        }
        self.level.voteTime = self.level.time - 30000 + actual_vote_time * 1000;
        self.level.voteYes = 0;
        self.level.voteNo = 0;

        let maxclients = unsafe { SV_MAXCLIENTS };
        for client_id in 0..maxclients {
            if let Ok(game_entity) = GameEntity::try_from(client_id) {
                if let Ok(mut game_client) = game_entity.get_game_client() {
                    game_client.set_vote_pending();
                }
            }
        }

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
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::{level_locals_t, qboolean, LevelLocalsBuilder};
    use pretty_assertions::assert_eq;

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
}
