use tap::TapFallible;

use crate::ffi::{c::prelude::*, python::prelude::*};

/// Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.
#[pyfunction]
#[pyo3(name = "allow_single_player")]
pub(crate) fn pyshinqlx_allow_single_player(py: Python<'_>, allow: bool) {
    py.detach(|| {
        let _ = CurrentLevel::try_get().tap_ok_mut(|current_level| {
            current_level.set_training_map(allow);
        });
    });
}

#[cfg(test)]
mod allow_single_player_tests {
    use mockall::predicate;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allow_single_player_with_no_current_level(_pyshinqlx_setup: ()) {
        let level_ctx = MockCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::attach(|py| pyshinqlx_allow_single_player(py, true));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allow_single_player_sets_training_map(_pyshinqlx_setup: ()) {
        let level_ctx = MockCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockCurrentLevel::new();
            mock_level
                .expect_set_training_map()
                .with(predicate::eq(true))
                .times(1);
            Ok(mock_level)
        });

        Python::attach(|py| pyshinqlx_allow_single_player(py, true));
    }
}
