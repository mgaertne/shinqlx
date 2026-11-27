//! Who is in a recording: a client number is taken by one player after another.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

const CS_PLAYERS: u16 = 529;

/// One presence of a client, from the first frame it plays in until it leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stay {
    /// The client number.
    pub client: u8,
    /// Which presence of this client number it is, starting with 1.
    pub number: u32,
    /// The name of the player when arriving, without color codes.
    pub name: Option<String>,
    /// The SteamID of the player; the game tells none for bots.
    pub steam_id: Option<u64>,
}

/// Follows who is there while a recording is read.
#[derive(Debug, Default)]
pub(super) struct Presence {
    present: BTreeSet<u8>,
    arrivals: BTreeMap<u8, u32>,
}

impl Presence {
    /// To be told about every client in a frame; returns the stay that begins with it, if one does.
    pub(super) fn arrive(
        &mut self,
        client: u8,
        configstrings: &BTreeMap<u16, Vec<u8>>,
    ) -> Option<Stay> {
        if !self.present.insert(client) {
            return None;
        }
        let number = self.arrivals.entry(client).or_default();
        *number += 1;
        let info = configstrings.get(&(CS_PLAYERS + u16::from(client)));
        let value = |key| info.and_then(|info| info_value(info, key));
        Some(Stay {
            client,
            number: *number,
            name: value("n").map(|name| without_colors(&name)),
            steam_id: value("st").and_then(|id| id.parse().ok()),
        })
    }

    pub(super) fn leave(&mut self, client: u8) {
        self.present.remove(&client);
    }
}

/// The value of a key in a player configstring like `n\Name\t\1`.
fn info_value(info: &[u8], wanted: &str) -> Option<String> {
    let info = String::from_utf8_lossy(info);
    let mut parts = info.trim_start_matches('\\').split('\\');
    core::iter::from_fn(|| parts.next().zip(parts.next()))
        .find_map(|(key, value)| (key == wanted).then(|| value.into()))
}

/// A name without its color codes like `^1`.
pub fn without_colors(name: &str) -> String {
    let mut characters = name.chars().peekable();
    core::iter::from_fn(|| {
        loop {
            let character = characters.next()?;
            if character == '^' && characters.next_if(char::is_ascii_digit).is_some() {
                continue;
            }
            return Some(character);
        }
    })
    .collect()
}

#[cfg(test)]
mod players_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn name_and_steam_id_are_taken_from_the_player_configstring() {
        let configstrings = BTreeMap::from([(
            529,
            b"n\\^1Red^7 Player\\t\\1\\st\\76561198000000001\\c\\de".to_vec(),
        )]);

        let stay = Presence::default().arrive(0, &configstrings);

        assert_eq!(
            stay.map(|stay| (stay.name, stay.steam_id)),
            Some((Some("Red Player".into()), Some(76_561_198_000_000_001)))
        );
    }

    #[test]
    fn a_stay_begins_again_only_after_leaving() {
        let mut presence = Presence::default();
        let configstrings = BTreeMap::from([(531, b"n\\Someone".to_vec())]);

        let first = presence.arrive(2, &configstrings);
        let still_there = presence.arrive(2, &configstrings);
        presence.leave(2);
        let second = presence.arrive(2, &BTreeMap::new());

        assert_eq!(
            first,
            Some(Stay {
                client: 2,
                number: 1,
                name: Some("Someone".into()),
                steam_id: None,
            })
        );
        assert_eq!(still_there, None);
        assert_eq!(second.map(|stay| (stay.number, stay.name)), Some((2, None)));
    }
}
