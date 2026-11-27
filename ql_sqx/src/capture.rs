use alloc::vec::Vec;

use super::state::{EntityState, PlayerState};

/// Whom the game shows an entity to, apart from the visibility in the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Every client.
    Everyone,
    /// Only the client with this number.
    Only(i32),
    /// Every client but the one with this number.
    AllBut(i32),
}

impl Visibility {
    fn includes(self, viewer: i32) -> bool {
        match self {
            Self::Everyone => true,
            Self::Only(only) => only == viewer,
            Self::AllBut(excluded) => excluded != viewer,
        }
    }
}

/// An entity and whom it is shown to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapturedEntity {
    /// The networked fields.
    pub state: EntityState,
    /// Whom the game shows it to.
    pub visibility: Visibility,
}

/// Everything the server knows at the end of one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// The time of the level in milliseconds.
    pub server_time: i32,
    /// In strictly increasing order of their numbers.
    pub entities: Vec<CapturedEntity>,
    /// The player states by client number.
    pub players: Vec<(u8, PlayerState)>,
}

impl Frame {
    pub(super) fn player(&self, client: u8) -> Option<&PlayerState> {
        self.players
            .iter()
            .find_map(|(number, player)| (*number == client).then_some(player))
    }

    /// The entities a client gets whose view is that of `viewer`, without the entity of the viewer.
    pub(super) fn entities_for(&self, viewer: i32) -> Vec<EntityState> {
        self.entities
            .iter()
            .filter(|entity| {
                i32::from(entity.state.number) != viewer && entity.visibility.includes(viewer)
            })
            .map(|entity| entity.state)
            .collect()
    }
}
