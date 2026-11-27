//! Copies the networked fields of the engine structures into the states of a demo.

use core::ffi::{c_char, c_int};

use ql_sqx::{EntityState, PlayerState, Visibility, float_bits};

use crate::engine::types::{entityState_t, playerState_t};

const SVF_NOCLIENT: c_int = 0x1;
const SVF_SINGLECLIENT: c_int = 0x100;
const SVF_NOTSINGLECLIENT: c_int = 0x800;

/// Whom an entity with these server flags is sent to, `None` if it is sent to nobody.
pub(super) fn visibility(sv_flags: c_int, single_client: c_int) -> Option<Visibility> {
    if sv_flags & SVF_NOCLIENT != 0 {
        None
    } else if sv_flags & SVF_SINGLECLIENT != 0 {
        Some(Visibility::Only(single_client))
    } else if sv_flags & SVF_NOTSINGLECLIENT != 0 {
        Some(Visibility::AllBut(single_client))
    } else {
        Some(Visibility::Everyone)
    }
}

fn int(value: c_int) -> u32 {
    value.cast_unsigned()
}

fn byte(value: c_char) -> u32 {
    u32::from(value.to_ne_bytes()[0])
}

/// The fields in the order of the netfield table of the engine.
pub(super) fn entity_state(number: u16, state: &entityState_t) -> EntityState {
    let (pos, apos) = (&state.pos, &state.apos);
    EntityState {
        number,
        fields: [
            int(pos.trTime),
            float_bits(pos.trBase[0]),
            float_bits(pos.trBase[1]),
            float_bits(pos.trDelta[0]),
            float_bits(pos.trDelta[1]),
            float_bits(pos.trBase[2]),
            float_bits(apos.trBase[1]),
            float_bits(pos.trDelta[2]),
            float_bits(apos.trBase[0]),
            float_bits(pos.gravity),
            int(state.event),
            float_bits(state.angles2[1]),
            int(state.eType),
            int(state.torsoAnim),
            int(state.eventParm),
            int(state.legsAnim),
            int(state.groundEntityNum),
            pos.trType as u32,
            int(state.eFlags),
            int(state.otherEntityNum),
            int(state.weapon),
            int(state.clientNum),
            float_bits(state.angles[1]),
            int(pos.trDuration),
            apos.trType as u32,
            float_bits(state.origin[0]),
            float_bits(state.origin[1]),
            float_bits(state.origin[2]),
            int(state.solid),
            int(state.powerups),
            int(state.modelindex),
            int(state.otherEntityNum2),
            int(state.loopSound),
            int(state.generic1),
            float_bits(state.origin2[2]),
            float_bits(state.origin2[0]),
            float_bits(state.origin2[1]),
            int(state.modelindex2),
            float_bits(state.angles[0]),
            int(state.time),
            int(apos.trTime),
            int(apos.trDuration),
            float_bits(apos.trBase[2]),
            float_bits(apos.trDelta[0]),
            float_bits(apos.trDelta[1]),
            float_bits(apos.trDelta[2]),
            float_bits(apos.gravity),
            int(state.time2),
            float_bits(state.angles[2]),
            float_bits(state.angles2[0]),
            float_bits(state.angles2[2]),
            int(state.constantLight),
            int(state.frame),
            int(state.jumpTime),
            int(state.doubleJumped),
            int(state.health),
            int(state.armor),
            int(state.location),
        ],
    }
}

/// The fields in the order of the netfield table of the engine.
pub(super) fn player_state(state: &playerState_t) -> PlayerState {
    let short = |values: &[c_int; 16]| values.map(|value| value as i16);
    PlayerState {
        fields: [
            int(state.commandTime),
            float_bits(state.origin[0]),
            float_bits(state.origin[1]),
            int(state.bobCycle),
            float_bits(state.velocity[0]),
            float_bits(state.velocity[1]),
            float_bits(state.viewangles[1]),
            float_bits(state.viewangles[0]),
            int(state.weaponTime),
            float_bits(state.origin[2]),
            float_bits(state.velocity[2]),
            int(state.legsTimer),
            int(state.pm_time),
            int(state.eventSequence),
            int(state.torsoAnim),
            int(state.movementDir),
            int(state.events[0]),
            int(state.legsAnim),
            int(state.events[1]),
            int(state.pm_flags),
            int(state.groundEntityNum),
            int(state.weaponstate),
            int(state.eFlags),
            int(state.externalEvent),
            int(state.gravity),
            int(state.speed),
            int(state.delta_angles[1]),
            int(state.externalEventParm),
            int(state.viewheight),
            int(state.damageEvent),
            int(state.damageYaw),
            int(state.damagePitch),
            int(state.damageCount),
            int(state.generic1),
            state.pm_type as u32,
            int(state.delta_angles[0]),
            int(state.delta_angles[2]),
            int(state.torsoTimer),
            int(state.eventParms[0]),
            int(state.eventParms[1]),
            int(state.clientNum),
            int(state.weapon),
            int(state.weaponPrimary),
            float_bits(state.viewangles[2]),
            float_bits(state.grapplePoint[0]),
            float_bits(state.grapplePoint[1]),
            float_bits(state.grapplePoint[2]),
            int(state.jumppad_ent),
            int(state.loopSound),
            int(state.jumpTime),
            int(state.doubleJumped),
            int(state.crouchTime),
            int(state.crouchSlideTime),
            int(state.location),
            int(state.fov),
            byte(state.forwardmove),
            byte(state.rightmove),
            byte(state.upmove),
        ],
        stats: short(&state.stats),
        persistant: short(&state.persistant),
        ammo: short(&state.ammo),
        powerups: state.powerups,
    }
}

#[cfg(test)]
mod convert_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::engine::types::{EntityStateBuilder, PlayerStateBuilder};

    #[test]
    fn entities_are_hidden_from_clients_like_the_game_asks() {
        assert_eq!(visibility(0, 3), Some(Visibility::Everyone));
        assert_eq!(visibility(SVF_NOCLIENT, 3), None);
        assert_eq!(visibility(SVF_SINGLECLIENT, 3), Some(Visibility::Only(3)));
        assert_eq!(
            visibility(SVF_NOTSINGLECLIENT, 3),
            Some(Visibility::AllBut(3))
        );
    }

    #[test]
    fn entity_fields_follow_the_netfield_table() {
        let state = EntityStateBuilder::default()
            .eType(3)
            .health(125)
            .location(7)
            .build()
            .expect("has defaults");

        let converted = entity_state(5, &state);

        assert_eq!(converted.fields[12], 3);
        assert_eq!(converted.fields[55], 125);
        assert_eq!(converted.fields[57], 7);
    }

    #[test]
    fn player_fields_follow_the_netfield_table() {
        let state = PlayerStateBuilder::default()
            .commandTime(1000)
            .weaponTime(-5)
            .clientNum(4)
            .forwardmove(-127i8 as c_char)
            .stats([-1; 16])
            .build()
            .expect("has defaults");

        let converted = player_state(&state);

        assert_eq!(converted.fields[0], 1000);
        assert_eq!(converted.fields[8].cast_signed(), -5);
        assert_eq!(converted.fields[40], 4);
        assert_eq!(converted.fields[55], 129);
        assert_eq!(converted.stats, [-1; 16]);
    }
}
