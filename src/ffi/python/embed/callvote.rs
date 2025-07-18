use tap::TapFallible;

use crate::ffi::{c::prelude::*, python::prelude::*};

/// Calls a vote as if started by the server and not a player.
#[pyfunction(name = "callvote", signature = (vote, vote_disp, vote_time=None), text_signature = "(vote, vote_disp, vote_time=None)")]
pub(crate) fn pyshinqlx_callvote(
    py: Python<'_>,
    vote: &str,
    vote_disp: &str,
    vote_time: Option<i32>,
) {
    py.allow_threads(|| {
        let _ = CurrentLevel::try_get().tap_ok_mut(|current_level| {
            current_level.callvote(vote, vote_disp, vote_time);
        });
    })
}

#[cfg(test)]
mod callvote_tests {
    use mockall::predicate;
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_with_no_current_level(_pyshinqlx_setup: ()) {
        let level_ctx = MockCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| {
            pyshinqlx_callvote(py, "map thunderstruck", "map thunderstruck", None)
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_with_current_level_calls_vote(_pyshinqlx_setup: ()) {
        let level_ctx = MockCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockCurrentLevel::new();
            mock_level
                .expect_callvote()
                .with(
                    predicate::eq("map theatreofpain"),
                    predicate::eq("map Theatre of Pain"),
                    predicate::eq(Some(10)),
                )
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| {
            pyshinqlx_callvote(py, "map theatreofpain", "map Theatre of Pain", Some(10))
        });
    }
}
