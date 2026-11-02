use core::hint::cold_path;

use pyo3::{
    exceptions::{PyEnvironmentError, PyValueError},
    intern,
};
use tap::Conv;

use super::{Teams, prelude::*};
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_set_configstring;
#[cfg(not(test))]
use crate::hooks::shinqlx_set_configstring;
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::*,
    quake_live_engine::{ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SetCVarForced},
};

pub(crate) fn client_id(
    py: Python<'_>,
    name: &Bound<'_, PyAny>,
    player_list: Option<Vec<Player>>,
) -> Option<i32> {
    if name.is_none() {
        return None;
    }

    match name.to_string().parse::<i32>() {
        Ok(value) if (0..64).contains(&value) => Some(value),
        _ => match name.cast::<Player>() {
            Ok(player) => Some(player.get().id),
            _ => {
                let all_players = player_list.unwrap_or_else(|| {
                    Player::all_players(&py.get_type::<Player>()).unwrap_or_default()
                });

                match name.to_string().parse::<i64>() {
                    Ok(steam_id) => all_players
                        .iter()
                        .find(|&player| player.steam_id == steam_id)
                        .map(|player| player.id),
                    _ => match name.extract::<String>() {
                        Ok(player_name) => {
                            let clean_name = clean_text(&player_name).to_lowercase();
                            all_players
                                .iter()
                                .find(|&player| {
                                    let player_name = &*player.name.read();
                                    clean_text(player_name).to_lowercase() == clean_name
                                })
                                .map(|player| player.id)
                        }
                        _ => None,
                    },
                }
            }
        },
    }
}

pub(crate) fn get_cvar(cvar: &str) -> PyResult<Option<String>> {
    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            Ok(main_engine
                .find_cvar(cvar)
                .map(|cvar_result| cvar_result.get_string()))
        },
    )
}

pub(crate) fn set_cvar(cvar: &str, value: &str, flags: Option<i32>) -> PyResult<bool> {
    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            main_engine.find_cvar(cvar).as_ref().map_or_else(
                || {
                    main_engine.get_cvar(cvar, value, flags);
                    Ok(true)
                },
                |_| {
                    main_engine.set_cvar_forced(
                        cvar,
                        value,
                        flags.is_some_and(|unwrapped_flags| unwrapped_flags == -1),
                    );
                    Ok(false)
                },
            )
        },
    )
}

pub(crate) fn console_command(cmd: &str) -> PyResult<()> {
    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| {
            main_engine.execute_console_command(cmd);
            Ok(())
        },
    )
}

pub(crate) fn get_configstring(index: u16) -> PyResult<String> {
    if !(0..MAX_CONFIGSTRINGS as u16).contains(&index) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    MAIN_ENGINE.load().as_ref().map_or_else(
        || {
            cold_path();
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ))
        },
        |main_engine| Ok(main_engine.get_configstring(index)),
    )
}

pub(crate) fn set_configstring(index: u16, value: &str) -> PyResult<()> {
    if !(0..MAX_CONFIGSTRINGS as u16).contains(&index) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "index needs to be a number from 0 to {}.",
            MAX_CONFIGSTRINGS - 1
        )));
    }

    shinqlx_set_configstring(index.conv::<u32>(), value);

    Ok(())
}

pub(crate) fn set_teamsize(value: i32) -> PyResult<()> {
    let value_str = format!("{value}");
    set_cvar("teamsize", &value_str, None).map(|_| ())
}

pub(crate) fn lock(team: Option<&str>) -> PyResult<()> {
    team.map_or_else(
        || console_command("lock"),
        |team_name| {
            let team_name_lower = team_name.to_lowercase();
            match Teams::from(team_name_lower.as_str()) {
                Teams::Invalid => {
                    cold_path();
                    Err(PyValueError::new_err("Invalid team."))
                }
                team => {
                    let lock_cmd = format!("lock {team}");
                    console_command(&lock_cmd)
                }
            }
        },
    )
}

pub(crate) fn unlock(team: Option<&str>) -> PyResult<()> {
    team.map_or_else(
        || console_command("unlock"),
        |team_name| {
            let team_name_lower = team_name.to_lowercase();
            match Teams::from(team_name_lower.as_str()) {
                Teams::Invalid => {
                    cold_path();
                    Err(PyValueError::new_err("Invalid team."))
                }
                team => {
                    let unlock_cmd = format!("unlock {team}");
                    console_command(&unlock_cmd)
                }
            }
        },
    )
}

pub(crate) fn put(py: Python<'_>, player: &Bound<'_, PyAny>, team: &str) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let team_lower = team.to_lowercase();
                match Teams::from(team_lower.as_str()) {
                    Teams::Invalid => {
                        cold_path();
                        Err(PyValueError::new_err("Invalid team."))
                    }
                    team => {
                        let team_change_cmd = format!("put {player_id} {team}");
                        console_command(&team_change_cmd)
                    }
                }
            })
        },
    )
}

pub(crate) fn mute(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let mute_cmd = format!("mute {player_id}");
                console_command(&mute_cmd)
            })
        },
    )
}

pub(crate) fn unmute(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let unmute_cmd = format!("unmute {player_id}");
                console_command(&unmute_cmd)
            })
        },
    )
}

pub(crate) fn tempban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let tempban_cmd = format!("tempban {player_id}");
                console_command(&tempban_cmd)
            })
        },
    )
}

pub(crate) fn ban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let ban_cmd = format!("ban {player_id}");
                console_command(&ban_cmd)
            })
        },
    )
}

pub(crate) fn unban(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let unban_cmd = format!("unban {player_id}");
                console_command(&unban_cmd)
            })
        },
    )
}

pub(crate) fn opsay(msg: &str) -> PyResult<()> {
    let opsay_cmd = format!("opsay {msg}");
    console_command(&opsay_cmd)
}

pub(crate) fn addadmin(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let addadmin_cmd = format!("addadmin {player_id}");
                console_command(&addadmin_cmd)
            })
        },
    )
}

pub(crate) fn addmod(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let addmod_cmd = format!("addmod {player_id}");
                console_command(&addmod_cmd)
            })
        },
    )
}

pub(crate) fn demote(py: Python<'_>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let demote_cmd = format!("demote {player_id}");
                console_command(&demote_cmd)
            })
        },
    )
}

pub(crate) fn addscore(py: Python<'_>, player: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
    client_id(py, player, None).map_or_else(
        || {
            cold_path();
            Err(PyValueError::new_err("Invalid player."))
        },
        |player_id| {
            py.detach(|| {
                let addscore_cmd = format!("addscore {player_id} {score}");
                console_command(&addscore_cmd)
            })
        },
    )
}

pub(crate) fn addteamscore(team: &str, score: i32) -> PyResult<()> {
    let team_lower = team.to_lowercase();
    match Teams::from(team_lower.as_str()) {
        Teams::Invalid => {
            cold_path();
            Err(PyValueError::new_err("Invalid team."))
        }
        team => {
            let addteamscore_cmd = format!("addteamscore {team} {score}");
            console_command(&addteamscore_cmd)
        }
    }
}

pub(crate) fn is_vote_active() -> bool {
    get_configstring(CS_VOTE_STRING as u16).is_ok_and(|vote_string| !vote_string.is_empty())
}

#[pyfunction]
#[pyo3(pass_module)]
pub(crate) fn set_map_subtitles(module: &Bound<'_, PyModule>) -> PyResult<()> {
    get_configstring(CS_MESSAGE as u16)
        .and_then(|map_title| module.setattr(intern!(module.py(), "_map_title"), map_title))?;

    get_configstring(CS_AUTHOR as u16).and_then(|mut map_subtitle1| {
        module.setattr(intern!(module.py(), "_map_subtitle1"), &map_subtitle1)?;

        if !map_subtitle1.is_empty() {
            map_subtitle1.push_str(" - ");
        }

        map_subtitle1.push_str("Running shinqlx ^6v");
        map_subtitle1.push_str(env!("SHINQLX_VERSION"));
        map_subtitle1.push_str("^7 with plugins ^6");

        let plugins_version = module
            .getattr(intern!(module.py(), "__plugins_version__"))
            .and_then(|value| value.extract::<String>())
            .unwrap_or_else(|_| "NOT_SET".to_string());

        map_subtitle1.push_str(&plugins_version);
        map_subtitle1.push_str("^7.");

        set_configstring(CS_AUTHOR as u16, &map_subtitle1)
    })?;

    get_configstring(CS_AUTHOR2 as u16).and_then(|mut map_subtitle2| {
        module.setattr(intern!(module.py(), "_map_subtitle2"), &map_subtitle2)?;

        module.py().detach(|| {
            if !map_subtitle2.is_empty() {
                map_subtitle2.push_str(" - ");
            }
            map_subtitle2
                .push_str("Check ^6https://github.com/mgaertne/shinqlx^7 for more details.");
            set_configstring(CS_AUTHOR2 as u16, &map_subtitle2)
        })
    })
}

#[cfg(test)]
mod set_map_subtitles_tests {
    use mockall::predicate;
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::set_map_subtitles;
    use crate::{
        ffi::{
            c::prelude::{CS_AUTHOR, CS_AUTHOR2, CS_MESSAGE},
            python::pyshinqlx_setup_fixture::*,
        },
        hooks::mock_hooks::shinqlx_set_configstring_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_subtitles_sets_map_title_attribute(_pyshinqlx_setup: ()) {
        let map_subtitle1 = format!(
            "Till 'Firestarter' Merker - Running shinqlx ^6v{}^7 with plugins ^61.3.3.7^7.",
            env!("SHINQLX_VERSION")
        );
        let map_subtitle2 = "Second author would go here - Check ^6https://github.com/mgaertne/shinqlx^7 for more details.";
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR), predicate::eq(map_subtitle1))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR2), predicate::eq(map_subtitle2))
            .times(1);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "thunderstruck", 1)
            .with_get_configstring(CS_AUTHOR as u16, "Till 'Firestarter' Merker", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "Second author would go here", 1)
            .run(|| {
                Python::attach(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    shinqlx_module
                        .setattr(intern!(py, "__plugins_version__"), "1.3.3.7")
                        .expect("this should not happen");
                    let result = set_map_subtitles(&shinqlx_module);
                    assert!(result.is_ok());
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_title"))
                            .is_ok_and(|value| value.to_string() == "thunderstruck")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle1"))
                            .is_ok_and(|value| value.to_string() == "Till 'Firestarter' Merker")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle2"))
                            .is_ok_and(|value| value.to_string() == "Second author would go here")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_subtitles_with_empty_authors(_pyshinqlx_setup: ()) {
        let map_subtitle1 = format!(
            "Running shinqlx ^6v{}^7 with plugins ^6NOT_SET^7.",
            env!("SHINQLX_VERSION")
        );
        let map_subtitle2 = "Check ^6https://github.com/mgaertne/shinqlx^7 for more details.";
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR), predicate::eq(map_subtitle1))
            .times(1);
        set_configstring_ctx
            .expect()
            .with(predicate::eq(CS_AUTHOR2), predicate::eq(map_subtitle2))
            .times(1);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "thunderstruck", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    shinqlx_module
                        .delattr(intern!(py, "__plugins_version__"))
                        .expect("this should not happen");
                    let result = set_map_subtitles(&shinqlx_module);
                    assert!(result.is_ok());
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_title"))
                            .is_ok_and(|value| value.to_string() == "thunderstruck")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle1"))
                            .is_ok_and(|value| value.to_string() == "")
                    );
                    assert!(
                        shinqlx_module
                            .getattr(intern!(py, "_map_subtitle2"))
                            .is_ok_and(|value| value.to_string() == "")
                    );
                });
            });
    }
}
