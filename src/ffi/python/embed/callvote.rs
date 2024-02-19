use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;

/// Calls a vote as if started by the server and not a player.
#[pyfunction]
#[pyo3(name = "callvote")]
pub(crate) fn pyshinqlx_callvote(
    py: Python<'_>,
    vote: &str,
    vote_disp: &str,
    vote_time: Option<i32>,
) {
    py.allow_threads(|| {
        CurrentLevel::try_get()
            .ok()
            .iter_mut()
            .for_each(|current_level| current_level.callvote(vote, vote_disp, vote_time));
    })
}

#[cfg(test)]
mod callvote_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_with_no_current_level() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx
            .expect()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        Python::with_gil(|py| {
            pyshinqlx_callvote(py, "map thunderstruck", "map thunderstruck", None)
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_with_current_level_calls_vote() {
        let level_ctx = MockTestCurrentLevel::try_get_context();
        level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
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
