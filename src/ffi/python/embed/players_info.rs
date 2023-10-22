#[cfg(test)]
use crate::ffi::python::DUMMY_MAIN_ENGINE as MAIN_ENGINE;
use crate::prelude::*;
#[cfg(not(test))]
use crate::MAIN_ENGINE;
use pyo3::exceptions::PyEnvironmentError;

use crate::ffi::python::PlayerInfo;
use pyo3::prelude::*;

/// Returns a list with dictionaries with information about all the players on the server.
#[pyfunction(name = "players_info")]
pub(crate) fn minqlx_players_info(py: Python<'_>) -> PyResult<Vec<Option<PlayerInfo>>> {
    let maxclients = py.allow_threads(|| {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        Ok(main_engine.get_max_clients())
    })?;

    py.allow_threads(move || {
        let result: Vec<Option<PlayerInfo>> = (0..maxclients)
            .filter_map(|client_id| {
                Client::try_from(client_id).map_or_else(
                    |_| None,
                    |client| match client.get_state() {
                        clientState_t::CS_FREE => None,
                        _ => Some(Some(PlayerInfo::from(client_id))),
                    },
                )
            })
            .collect();

        Ok(result)
    })
}

#[cfg(test)]
#[cfg(not(miri))]
mod get_players_info_tests {
    use super::minqlx_players_info;
    use super::MAIN_ENGINE;
    use crate::prelude::*;
    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[serial]
    fn get_players_info_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = minqlx_players_info(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }
}
