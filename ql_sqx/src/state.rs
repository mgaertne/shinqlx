use super::fields::{ENTITY_FIELD_COUNT, PLAYER_FIELD_COUNT};

pub(super) const ENTITY_NUMBER_BITS: u8 = 10;
/// Ends a list of entities and is therefore no number of an entity.
pub(super) const ENTITY_NUMBER_END: u16 = (1 << ENTITY_NUMBER_BITS) - 1;
/// The length of the arrays of a player state.
pub const ARRAY_LENGTH: usize = 16;

/// The bits of a float as a demo carries it, which knows no negative zero.
pub fn float_bits(value: f32) -> u32 {
    (value + 0.0).to_bits()
}

/// An entity as the raw bits of its networked fields, in the order of `ENTITY_FIELDS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityState {
    /// The entity number.
    pub number: u16,
    /// The raw bits of the fields.
    pub fields: [u32; ENTITY_FIELD_COUNT],
}

impl Default for EntityState {
    fn default() -> Self {
        Self {
            number: 0,
            fields: [0; ENTITY_FIELD_COUNT],
        }
    }
}

/// A player state as the raw bits of its networked fields, in the order of `PLAYER_FIELDS`, and its arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    /// The raw bits of the fields.
    pub fields: [u32; PLAYER_FIELD_COUNT],
    /// Values like health and armor.
    pub stats: [i16; ARRAY_LENGTH],
    /// Values that survive a respawn, like the score.
    pub persistant: [i16; ARRAY_LENGTH],
    /// Ammunition by weapon.
    pub ammo: [i16; ARRAY_LENGTH],
    /// The end times of the powerups.
    pub powerups: [i32; ARRAY_LENGTH],
}

impl PlayerState {
    const CLIENT_NUMBER_FIELD: usize = 40;

    /// The player whose view this is, who differs from the owner of the state while spectating.
    pub(super) fn client_number(&self) -> i32 {
        self.fields[Self::CLIENT_NUMBER_FIELD].cast_signed()
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            fields: [0; PLAYER_FIELD_COUNT],
            stats: [0; ARRAY_LENGTH],
            persistant: [0; ARRAY_LENGTH],
            ammo: [0; ARRAY_LENGTH],
            powerups: [0; ARRAY_LENGTH],
        }
    }
}
