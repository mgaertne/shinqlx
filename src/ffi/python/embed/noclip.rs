use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets noclip for a player.
#[pyfunction]
#[pyo3(name = "noclip")]
pub(crate) fn pyshinqlx_noclip(py: Python<'_>, client_id: i32, activate: bool) -> PyResult<bool> {
    py.detach(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .filter(|game_client| game_client.get_noclip() != activate)
            .tap_some_mut(|game_client| {
                game_client.set_noclip(activate);
            })
            .is_some())
    })
}

#[cfg(test)]
mod noclip_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = pyshinqlx_noclip(py, 21, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_for_client_id_too_small(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_noclip(py, -1, false);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_for_client_id_too_large(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_noclip(py, 666, true);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_noclip(py, 2, true));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_for_entity_with_noclip_already_set_properly(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client.expect_set_noclip::<bool>().times(0);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_noclip(py, 2, true));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn noclip_for_entity_with_change_applied(_pyshinqlx_setup: ()) {
        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_noclip().returning(|| true);
                mock_game_client
                    .expect_set_noclip::<bool>()
                    .with(predicate::eq(false))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_noclip(py, 2, false));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }
}
