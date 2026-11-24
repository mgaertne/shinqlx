#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]

use alloc::borrow::Cow;
use core::ffi::{CStr, c_char, c_int};

use log::{debug, warn};
use tap::TapOptional;

use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::{
        CurrentLevel, GameEntity, gclient_t, gentity_t, playerState_t, statIndex_t, trace_t,
        weapon_t,
    },
};

const KEEP_ARMOR: c_int = 1;
const KEEP_HEALTH: c_int = 2;
const KEEP_WEAPONS: c_int = 4;
const KEEP_POWERUPS: c_int = 8;
const KEEP_HOLDABLE: c_int = 16;
const REMOVE_MACHINEGUN: c_int = 32;

const START_HEALTH: c_int = 100;
const START_MACHINEGUN_AMMO: c_int = 100;

type TouchFunction = unsafe extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum RaceTouch {
    RacePoint,
    LegacyRacePoint,
    TargetInit,
}

impl RaceTouch {
    fn replacement(self) -> TouchFunction {
        match self {
            Self::RacePoint => race_point_touch,
            Self::LegacyRacePoint => legacy_race_point_touch,
            Self::TargetInit => target_init_touch,
        }
    }
}

fn classify(
    classname: &str,
    message: Option<&str>,
    has_target: bool,
    has_targetname: bool,
) -> Option<RaceTouch> {
    if classname.starts_with("trigger_multiple") {
        let message = message?;
        if message.starts_with("race_point") && (has_target || has_targetname) {
            return Some(RaceTouch::RacePoint);
        }
        return message
            .starts_with("target_init")
            .then_some(RaceTouch::TargetInit);
    }
    classname
        .starts_with("race_point")
        .then_some(RaceTouch::LegacyRacePoint)
}

fn text_of<'a>(text: *const c_char) -> Option<Cow<'a, str>> {
    if text.is_null() {
        return None;
    }
    // SAFETY: the game stores NUL-terminated strings in these fields and the entity outlives the call.
    Some(unsafe { CStr::from_ptr(text) }.to_string_lossy())
}

/// Replaces the touch function of the race entities of the loaded map, to be called after `G_InitGame`.
#[cfg_attr(
    test,
    allow(
        clippy::unnecessary_fallible_conversions,
        irrefutable_let_patterns,
        reason = "the mock entity converts infallibly"
    )
)]
pub(crate) fn replace_race_touches() {
    let Ok(level) = CurrentLevel::try_get() else {
        return;
    };
    let mut replaced = 0;
    (1..level.get_num_entities()).for_each(|entity_id| {
        let Ok(mut entity) = GameEntity::try_from(entity_id) else {
            return;
        };
        let gentity = entity.as_mut();
        if !bool::from(gentity.inuse) {
            return;
        }
        let Some(classname) = text_of(gentity.classname) else {
            return;
        };
        let message = text_of(gentity.message);
        classify(
            &classname,
            message.as_deref(),
            !gentity.target.is_null(),
            !gentity.targetname.is_null(),
        )
        .tap_some(|race_touch| {
            gentity.touch = Some(race_touch.replacement());
            replaced += 1;
        });
    });
    debug!(target: "shinqlx", func = "replace_race_touches"; "Replaced the touch function of {replaced} of {} entities", level.get_num_entities());
}

fn client_of<'a>(activator: *mut gentity_t) -> Option<&'a mut gclient_t> {
    // SAFETY: the game passes the activator of a touch as a valid entity, whose client is null or valid.
    unsafe { activator.as_ref() }.and_then(|activator| unsafe { activator.client.as_mut() })
}

fn keep_run_time(client: &mut gclient_t) {
    let Ok(level) = CurrentLevel::try_get() else {
        return;
    };
    client.pers.teamState.flagruntime = level.get_leveltime() - client.race.startTime;
}

fn call_original(ent: *mut gentity_t, activator: *mut gentity_t, trace: *mut trace_t) {
    let Some(main_engine) = MAIN_ENGINE
        .load()
        .as_ref()
        .map(|engine| engine.race_point_touch_orig())
    else {
        return;
    };
    match main_engine {
        Ok(original) => original(ent, activator, trace),
        Err(err) => warn!(target: "shinqlx", func = "call_original"; "{err:?}"),
    }
}

fn is_repeated_start(ent: &gentity_t, client: &gclient_t, notr_only: bool) -> bool {
    let wants_no_reset = if notr_only {
        text_of(ent.message).is_some_and(|message| message.starts_with("notr"))
    } else {
        ent.spawnflags != 0
    };
    wants_no_reset && client.race.startTime != 0 && ent.targetname.is_null()
}

fn race_touch(
    ent: *mut gentity_t,
    activator: *mut gentity_t,
    trace: *mut trace_t,
    notr_only: bool,
) {
    if let Some(client) = client_of(activator) {
        // SAFETY: the game passes the touched entity as a valid entity.
        if unsafe { ent.as_ref() }.is_some_and(|ent| is_repeated_start(ent, client, notr_only)) {
            return;
        }
        keep_run_time(client);
    }
    call_original(ent, activator, trace);
}

unsafe extern "C" fn race_point_touch(
    ent: *mut gentity_t,
    activator: *mut gentity_t,
    trace: *mut trace_t,
) {
    race_touch(ent, activator, trace, false);
}

unsafe extern "C" fn legacy_race_point_touch(
    ent: *mut gentity_t,
    activator: *mut gentity_t,
    trace: *mut trace_t,
) {
    race_touch(ent, activator, trace, true);
}

const MACHINEGUN_BIT: c_int = 1 << (weapon_t::WP_MACHINEGUN as c_int);

fn reset_player(spawnflags: c_int, health: &mut c_int, player_state: &mut playerState_t) {
    if spawnflags & KEEP_ARMOR == 0 {
        player_state.stats[statIndex_t::STAT_ARMOR as usize] = 0;
    }
    if spawnflags & KEEP_HEALTH == 0 {
        *health = START_HEALTH;
    }
    if spawnflags & KEEP_WEAPONS == 0 {
        player_state.stats[statIndex_t::STAT_WEAPONS as usize] = MACHINEGUN_BIT;
        player_state.ammo.fill(0);
        player_state.ammo[weapon_t::WP_MACHINEGUN as usize] = START_MACHINEGUN_AMMO;
        player_state.weapon = weapon_t::WP_MACHINEGUN as c_int;
    }
    if spawnflags & KEEP_POWERUPS == 0 {
        player_state.powerups.fill(0);
    }
    if spawnflags & KEEP_HOLDABLE == 0 {
        player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize] = 0;
    }
    if spawnflags & REMOVE_MACHINEGUN != 0 {
        player_state.stats[statIndex_t::STAT_WEAPONS as usize] &= !MACHINEGUN_BIT;
    }
}

unsafe extern "C" fn target_init_touch(
    ent: *mut gentity_t,
    activator: *mut gentity_t,
    _trace: *mut trace_t,
) {
    // SAFETY: the game passes the touched entity as a valid entity.
    let Some(spawnflags) = unsafe { ent.as_ref() }.map(|ent| ent.spawnflags) else {
        return;
    };
    // SAFETY: the game passes the activator of a touch as a valid entity.
    let Some(activator) = (unsafe { activator.as_mut() }) else {
        return;
    };
    // SAFETY: the client of the activator is null or valid.
    if let Some(client) = unsafe { activator.client.as_mut() } {
        reset_player(spawnflags, &mut activator.health, &mut client.ps);
    }
}

#[cfg(test)]
mod race_touch_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{MACHINEGUN_BIT, RaceTouch, classify, is_repeated_start, reset_player, text_of};
    use crate::ffi::c::prelude::*;

    #[rstest]
    #[case(
        "trigger_multiple",
        Some("race_point"),
        true,
        false,
        Some(RaceTouch::RacePoint)
    )]
    #[case(
        "trigger_multiple",
        Some("race_point"),
        false,
        true,
        Some(RaceTouch::RacePoint)
    )]
    #[case("trigger_multiple", Some("race_point"), false, false, None)]
    #[case(
        "trigger_multiple",
        Some("target_init"),
        false,
        false,
        Some(RaceTouch::TargetInit)
    )]
    #[case("trigger_multiple", Some("something"), true, true, None)]
    #[case("trigger_multiple", None, true, true, None)]
    #[case("race_point", None, false, false, Some(RaceTouch::LegacyRacePoint))]
    #[case(
        "race_point",
        Some("notr"),
        false,
        false,
        Some(RaceTouch::LegacyRacePoint)
    )]
    #[case("trigger_push", Some("race_point"), true, true, None)]
    fn entities_are_classified(
        #[case] classname: &str,
        #[case] message: Option<&str>,
        #[case] has_target: bool,
        #[case] has_targetname: bool,
        #[case] expected: Option<RaceTouch>,
    ) {
        assert_eq!(
            classify(classname, message, has_target, has_targetname),
            expected
        );
    }

    #[test]
    fn null_text_is_none() {
        assert_eq!(text_of(core::ptr::null()), None);
    }

    fn client_started_at(start_time: i32) -> gclient_t {
        GClientBuilder::default()
            .race(
                RaceInfoBuilder::default()
                    .startTime(start_time)
                    .build()
                    .expect("RaceInfoBuilder has defaults for all fields"),
            )
            .build()
            .expect("GClientBuilder has defaults for all fields")
    }

    #[rstest]
    #[case(0, 1000, false, true)]
    #[case(1, 1000, false, false)]
    #[case(1, 0, false, true)]
    fn the_run_time_is_kept_for_flagged_points(
        #[case] spawnflags: i32,
        #[case] start_time: i32,
        #[case] notr_only: bool,
        #[case] resets: bool,
    ) {
        let ent = GEntityBuilder::default()
            .spawnflags(spawnflags)
            .build()
            .expect("GEntityBuilder has defaults for all fields");
        let client = client_started_at(start_time);

        assert_eq!(!is_repeated_start(&ent, &client, notr_only), resets);
    }

    #[test]
    fn a_player_is_reset_without_keep_flags() {
        let mut health = 12;
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("PlayerStateBuilder has defaults for all fields");
        player_state.stats[statIndex_t::STAT_ARMOR as usize] = 50;
        player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize] = 3;
        player_state.powerups[2] = 99;
        player_state.ammo[5] = 7;
        player_state.weapon = 5;

        reset_player(0, &mut health, &mut player_state);

        assert_eq!(health, 100);
        assert_eq!(player_state.stats[statIndex_t::STAT_ARMOR as usize], 0);
        assert_eq!(
            player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize],
            0
        );
        assert_eq!(
            player_state.stats[statIndex_t::STAT_WEAPONS as usize],
            MACHINEGUN_BIT
        );
        assert_eq!(player_state.ammo[5], 0);
        assert_eq!(player_state.ammo[weapon_t::WP_MACHINEGUN as usize], 100);
        assert_eq!(player_state.weapon, weapon_t::WP_MACHINEGUN as i32);
        assert_eq!(player_state.powerups[2], 0);
    }

    #[test]
    fn keep_flags_leave_the_player_alone() {
        let mut health = 12;
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("PlayerStateBuilder has defaults for all fields");
        player_state.stats[statIndex_t::STAT_ARMOR as usize] = 50;
        player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize] = 3;
        player_state.stats[statIndex_t::STAT_WEAPONS as usize] = 0b1100;
        player_state.powerups[2] = 99;
        player_state.ammo[5] = 7;
        player_state.weapon = 5;

        reset_player(1 | 2 | 4 | 8 | 16, &mut health, &mut player_state);

        assert_eq!(health, 12);
        assert_eq!(player_state.stats[statIndex_t::STAT_ARMOR as usize], 50);
        assert_eq!(
            player_state.stats[statIndex_t::STAT_HOLDABLE_ITEM as usize],
            3
        );
        assert_eq!(
            player_state.stats[statIndex_t::STAT_WEAPONS as usize],
            0b1100
        );
        assert_eq!(player_state.powerups[2], 99);
        assert_eq!(player_state.ammo[5], 7);
        assert_eq!(player_state.weapon, 5);
    }

    #[test]
    fn the_machinegun_can_be_removed() {
        let mut health = 100;
        let mut player_state = PlayerStateBuilder::default()
            .build()
            .expect("PlayerStateBuilder has defaults for all fields");

        reset_player(32, &mut health, &mut player_state);

        assert_eq!(player_state.stats[statIndex_t::STAT_WEAPONS as usize], 0);
    }

    #[test]
    fn the_entity_layout_matches_the_game_module() {
        assert_eq!(core::mem::offset_of!(gentity_t, client), 0x240);
        assert_eq!(core::mem::offset_of!(gentity_t, target), 0x310);
        assert_eq!(core::mem::offset_of!(gentity_t, targetname), 0x318);
    }
}
