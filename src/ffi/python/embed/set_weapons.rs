use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's weapons.
#[pyfunction]
#[pyo3(name = "set_weapons")]
pub(crate) fn pyshinqlx_set_weapons(
    py: Python<'_>,
    client_id: i32,
    weapons: &Weapons,
) -> PyResult<bool> {
    py.allow_threads(|| {
        validate_client_id(client_id)?;

        #[cfg_attr(
            test,
            allow(clippy::unnecessary_fallible_conversions, irrefutable_let_patterns)
        )]
        let opt_game_client = GameEntity::try_from(client_id)
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok());
        let returned = opt_game_client.is_some();
        if let Some(mut game_client) = opt_game_client {
            game_client.set_weapons((*weapons).into());
        }
        Ok(returned)
    })
}

#[cfg(test)]
mod set_weapons_tests {
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
    fn set_weapons_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let weapons = Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

        Python::with_gil(|py| {
            let result = pyshinqlx_set_weapons(py, 21, &weapons);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapons_for_client_id_too_small(_pyshinqlx_setup: ()) {
        let weapons = Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_weapons(py, -1, &weapons);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapons_for_client_id_too_large(_pyshinqlx_setup: ()) {
        let weapons = Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = pyshinqlx_set_weapons(py, 666, &weapons);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapons_for_existing_game_client(_pyshinqlx_setup: ()) {
        let weapons = Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1);

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapons()
                    .with(predicate::eq([1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_set_weapons(py, 2, &weapons));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_weapons_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let weapons = Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);

        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::with_gil(|py| pyshinqlx_set_weapons(py, 2, &weapons));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
