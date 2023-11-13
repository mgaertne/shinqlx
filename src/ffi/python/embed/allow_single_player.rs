use crate::ffi::c::CurrentLevel;

use pyo3::{pyfunction, Python};

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
pub(crate) fn pyshinqlx_allow_single_player(py: Python<'_>, allow: bool) {
    py.allow_threads(|| {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.set_training_map(allow))
    });
}

#[cfg(test)]
#[cfg(not(miri))]
mod allow_single_player_tests {
    use super::pyshinqlx_allow_single_player;
    use crate::ffi::c::current_level::MockTestCurrentLevel;
    use crate::prelude::*;
    use mockall::predicate;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn allow_single_player_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| pyshinqlx_allow_single_player(py, true));
    }

    #[test]
    #[serial]
    fn allow_single_player_sets_training_map() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_set_training_map()
                .with(predicate::eq(true))
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| pyshinqlx_allow_single_player(py, true));
    }
}
