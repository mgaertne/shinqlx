#![allow(non_camel_case_types, clippy::upper_case_acronyms)]

use super::prelude::*;
use crate::ffi::c::prelude::*;

/// The indices of the configstrings of Quake Live.
///
/// Members can be used wherever a configstring index is expected, e.g. in
/// :func:`shinqlx.get_configstring`.
#[pyclass(module = "_shinqlx", name = "ConfigString", frozen, eq, eq_int, hash)]
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(i32)]
pub(crate) enum ConfigString {
    SERVERINFO = CS_SERVERINFO as i32,
    SYSTEMINFO = CS_SYSTEMINFO as i32,
    MUSIC = CS_MUSIC as i32,
    MESSAGE = CS_MESSAGE as i32,
    MOTD = CS_MOTD as i32,
    WARMUP = CS_WARMUP as i32,
    SCORES1 = CS_SCORES1 as i32,
    SCORES2 = CS_SCORES2 as i32,
    VOTE_TIME = CS_VOTE_TIME as i32,
    VOTE_STRING = CS_VOTE_STRING as i32,
    VOTE_YES = CS_VOTE_YES as i32,
    VOTE_NO = CS_VOTE_NO as i32,
    GAME_VERSION = CS_GAME_VERSION as i32,
    LEVEL_START_TIME = CS_LEVEL_START_TIME as i32,
    INTERMISSION = CS_INTERMISSION as i32,
    ITEMS = CS_ITEMS as i32,
    MODELS = CS_MODELS as i32,
    SOUNDS = CS_SOUNDS as i32,
    PLAYERS = CS_PLAYERS as i32,
    LOCATIONS = CS_LOCATIONS as i32,
    PARTICLES = CS_PARTICLES as i32,
    FLAGSTATUS = CS_FLAGSTATUS as i32,
    FIRSTPLACE = CS_FIRSTPLACE as i32,
    SECONDPLACE = CS_SECONDPLACE as i32,
    ROUND_STATUS = CS_ROUND_STATUS as i32,
    ROUND_TIME = CS_ROUND_TIME as i32,
    RED_PLAYERS_LEFT = CS_RED_PLAYERS_LEFT as i32,
    BLUE_PLAYERS_LEFT = CS_BLUE_PLAYERS_LEFT as i32,
    SHADERSTATE = CS_SHADERSTATE as i32,
    NEXTMAP = CS_NEXTMAP as i32,
    PRACTICE = CS_PRACTICE as i32,
    FREECAM = CS_FREECAM as i32,
    PAUSE_START_TIME = CS_PAUSE_START_TIME as i32,
    PAUSE_END_TIME = CS_PAUSE_END_TIME as i32,
    TIMEOUTS_RED = CS_TIMEOUTS_RED as i32,
    TIMEOUTS_BLUE = CS_TIMEOUTS_BLUE as i32,
    MODEL_OVERRIDE = CS_MODEL_OVERRIDE as i32,
    PLAYER_CYLINDERS = CS_PLAYER_CYLINDERS as i32,
    DEBUGFLAGS = CS_DEBUGFLAGS as i32,
    ENABLEBREATH = CS_ENABLEBREATH as i32,
    DMGTHROUGHDEPTH = CS_DMGTHROUGHDEPTH as i32,
    AUTHOR = CS_AUTHOR as i32,
    AUTHOR2 = CS_AUTHOR2 as i32,
    ADVERT_DELAY = CS_ADVERT_DELAY as i32,
    PMOVEINFO = CS_PMOVEINFO as i32,
    ARMORINFO = CS_ARMORINFO as i32,
    WEAPONINFO = CS_WEAPONINFO as i32,
    PLAYERINFO = CS_PLAYERINFO as i32,
    SCORE1STPLAYER = CS_SCORE1STPLAYER as i32,
    SCORE2NDPLAYER = CS_SCORE2NDPLAYER as i32,
    CLIENTNUM1STPLAYER = CS_CLIENTNUM1STPLAYER as i32,
    CLIENTNUM2NDPLAYER = CS_CLIENTNUM2NDPLAYER as i32,
    NAME1STPLAYER = CS_NAME1STPLAYER as i32,
    NAME2NDPLAYER = CS_NAME2NDPLAYER as i32,
    ATMOSEFFECT = CS_ATMOSEFFECT as i32,
    MOST_DAMAGEDEALT_PLYR = CS_MOST_DAMAGEDEALT_PLYR as i32,
    MOST_ACCURATE_PLYR = CS_MOST_ACCURATE_PLYR as i32,
    REDTEAMBASE = CS_REDTEAMBASE as i32,
    BLUETEAMBASE = CS_BLUETEAMBASE as i32,
    BEST_ITEMCONTROL_PLYR = CS_BEST_ITEMCONTROL_PLYR as i32,
    MOST_VALUABLE_OFFENSIVE_PLYR = CS_MOST_VALUABLE_OFFENSIVE_PLYR as i32,
    MOST_VALUABLE_DEFENSIVE_PLYR = CS_MOST_VALUABLE_DEFENSIVE_PLYR as i32,
    MOST_VALUABLE_PLYR = CS_MOST_VALUABLE_PLYR as i32,
    GENERIC_COUNT_RED = CS_GENERIC_COUNT_RED as i32,
    GENERIC_COUNT_BLUE = CS_GENERIC_COUNT_BLUE as i32,
    AD_SCORES = CS_AD_SCORES as i32,
    ROUND_WINNER = CS_ROUND_WINNER as i32,
    CUSTOM_SETTINGS = CS_CUSTOM_SETTINGS as i32,
    ROTATIONMAPS = CS_ROTATIONMAPS as i32,
    ROTATIONVOTES = CS_ROTATIONVOTES as i32,
    DISABLE_VOTE_UI = CS_DISABLE_VOTE_UI as i32,
    ALLREADY_TIME = CS_ALLREADY_TIME as i32,
    INFECTED_SURVIVOR_MINSPEED = CS_INFECTED_SURVIVOR_MINSPEED as i32,
    RACE_POINTS = CS_RACE_POINTS as i32,
    DISABLE_LOADOUT = CS_DISABLE_LOADOUT as i32,
    MATCH_GUID = CS_MATCH_GUID as i32,
    STARTING_WEAPONS = CS_STARTING_WEAPONS as i32,
    STEAM_ID = CS_STEAM_ID as i32,
    STEAM_WORKSHOP_IDS = CS_STEAM_WORKSHOP_IDS as i32,
}

#[pymethods]
impl ConfigString {
    fn __index__(&self) -> i32 {
        *self as i32
    }
}

#[cfg(test)]
mod config_string_tests {
    use pyo3::IntoPyObjectExt;
    use rstest::rstest;

    use super::ConfigString;
    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[rstest]
    #[case(ConfigString::WARMUP, CS_WARMUP)]
    #[case(ConfigString::PLAYERS, CS_PLAYERS)]
    #[case(ConfigString::ROUND_STATUS, CS_ROUND_STATUS)]
    #[case(ConfigString::BLUE_PLAYERS_LEFT, CS_BLUE_PLAYERS_LEFT)]
    #[case(ConfigString::PLAYERINFO, 684)]
    #[case(ConfigString::CLIENTNUM2NDPLAYER, 688)]
    #[case(ConfigString::MOST_DAMAGEDEALT_PLYR, 692)]
    #[case(ConfigString::ROTATIONMAPS, 705)]
    #[case(ConfigString::DISABLE_LOADOUT, 711)]
    #[case(ConfigString::STEAM_WORKSHOP_IDS, CS_STEAM_WORKSHOP_IDS)]
    fn index_is_the_configstring_index(#[case] config_string: ConfigString, #[case] index: u32) {
        assert_eq!(config_string.__index__() as u32, index);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn config_string_is_usable_as_an_index_in_python(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let index = ConfigString::NEXTMAP
                .into_py_any(py)
                .and_then(|member| member.extract::<u16>(py));

            assert_eq!(index.expect("this should not happen"), CS_NEXTMAP as u16);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn config_string_is_registered_in_the_module(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            py.run(
                cr#"
import shinqlx

assert shinqlx.ConfigString.WARMUP == 5
assert shinqlx.ConfigString.WARMUP == shinqlx.ConfigString.WARMUP
assert shinqlx.ConfigString.WARMUP != shinqlx.ConfigString.MOTD
assert int(shinqlx.ConfigString.PLAYERS) == shinqlx.ConfigString.PLAYERS.__index__()
assert [0, 1, 2, 3, 4, 5, 6][shinqlx.ConfigString.WARMUP] == 5
            "#,
                None,
                None,
            )
            .expect("this should not happen");
        });
    }
}
