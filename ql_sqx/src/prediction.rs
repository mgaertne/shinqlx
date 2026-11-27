//! The game derives the entity of a player from the player state, so recordings only carry where the
//! entity differs from that derivation.

use super::{
    fields::{entity_field, player_field},
    state::{EntityState, PlayerState, float_bits},
};

/// Float fields the game truncates to whole numbers, as entity and player field.
const TRUNCATED: [(usize, usize); 8] = [
    both("pos.trBase[0]", "origin[0]"),
    both("pos.trBase[1]", "origin[1]"),
    both("pos.trBase[2]", "origin[2]"),
    both("pos.trDelta[0]", "velocity[0]"),
    both("pos.trDelta[1]", "velocity[1]"),
    both("pos.trDelta[2]", "velocity[2]"),
    both("apos.trBase[0]", "viewangles[0]"),
    both("apos.trBase[1]", "viewangles[1]"),
];
const COPIED: [(usize, usize); 6] = [
    both("torsoAnim", "torsoAnim"),
    both("legsAnim", "legsAnim"),
    both("groundEntityNum", "groundEntityNum"),
    both("eFlags", "eFlags"),
    both("weapon", "weapon"),
    both("loopSound", "loopSound"),
];
const MOVEMENT_DIRECTION: (usize, usize) = both("angles2[1]", "movementDir");
const HEALTH: (usize, usize) = (entity_field("health"), 0);
const ARMOR: (usize, usize) = (entity_field("armor"), 4);

const fn both(entity: &str, player: &str) -> (usize, usize) {
    (entity_field(entity), player_field(player))
}

/// What the entity of `player` is expected to be, given what it was before.
pub(super) fn predicted_entity(mut entity: EntityState, player: &PlayerState) -> EntityState {
    for (to, from) in TRUNCATED {
        entity.fields[to] = float_bits(f32::from_bits(player.fields[from]).trunc());
    }
    for (to, from) in COPIED {
        entity.fields[to] = player.fields[from];
    }
    let (to, from) = MOVEMENT_DIRECTION;
    entity.fields[to] = float_bits(player.fields[from] as f32);
    for (to, stat) in [HEALTH, ARMOR] {
        entity.fields[to] = u32::from(player.stats[stat].cast_unsigned());
    }
    entity
}

#[cfg(test)]
mod prediction_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_entity_of_a_player_follows_the_state() {
        let mut player = PlayerState::default();
        player.fields[player_field("origin[0]")] = (-12.75f32).to_bits();
        player.fields[player_field("velocity[2]")] = (-0.5f32).to_bits();
        player.fields[player_field("movementDir")] = 3;
        player.fields[player_field("weapon")] = 5;
        player.stats[0] = -2;
        player.stats[4] = 50;
        let mut before = EntityState {
            number: 7,
            ..EntityState::default()
        };
        before.fields[entity_field("eType")] = 1;

        let entity = predicted_entity(before, &player);

        let field = |name| entity.fields[entity_field(name)];
        assert_eq!(entity.number, 7);
        assert_eq!(field("eType"), 1);
        assert_eq!(field("pos.trBase[0]"), (-12.0f32).to_bits());
        assert_eq!(field("pos.trDelta[2]"), 0);
        assert_eq!(field("angles2[1]"), 3.0f32.to_bits());
        assert_eq!(field("weapon"), 5);
        assert_eq!(field("health"), 65534);
        assert_eq!(field("armor"), 50);
    }
}
