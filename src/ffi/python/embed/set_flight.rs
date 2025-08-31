use tap::{TapOptional, TryConv};

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};

/// Sets a player's flight parameters, such as current fuel, max fuel and, so on.
#[pyfunction]
#[pyo3(name = "set_flight")]
pub(crate) fn pyshinqlx_set_flight(
    py: Python<'_>,
    client_id: i32,
    flight: &Flight,
) -> PyResult<bool> {
    py.detach(|| {
        validate_client_id(client_id)?;

        Ok(client_id
            .try_conv::<GameEntity>()
            .ok()
            .and_then(|game_entity| game_entity.get_game_client().ok())
            .tap_some_mut(|game_client| {
                game_client.set_flight::<[i32; 4]>((*flight).into());
            })
            .is_some())
    })
}

#[cfg(test)]
mod set_flight_tests {
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
    fn set_flight_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let flight = Flight(0, 0, 0, 0);

        Python::attach(|py| {
            let result = pyshinqlx_set_flight(py, 21, &flight);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_flight_for_client_id_too_small(_pyshinqlx_setup: ()) {
        let flight = Flight(0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_flight(py, -1, &flight);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_flight_for_client_id_too_large(_pyshinqlx_setup: ()) {
        let flight = Flight(0, 0, 0, 0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_set_flight(py, 666, &flight);
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_flight_for_existing_game_client(_pyshinqlx_setup: ()) {
        let flight = Flight(12, 34, 56, 78);

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_flight::<[i32; 4]>()
                    .with(predicate::eq([12, 34, 56, 78]))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_set_flight(py, 2, &flight));
                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_flight_for_entity_with_no_game_client(_pyshinqlx_setup: ()) {
        let flight = Flight(12, 34, 56, 78);

        MockGameEntityBuilder::default()
            .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .run(predicate::always(), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    let result = Python::attach(|py| pyshinqlx_set_flight(py, 2, &flight));
                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }
}
