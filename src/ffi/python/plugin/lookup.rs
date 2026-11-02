use super::{super::clean_text, Player};

pub(super) fn players_matching_name(players: Vec<Player>, name: &str) -> Vec<Player> {
    if name.is_empty() {
        return players;
    }

    let cleaned_text = clean_text(&name).to_lowercase();
    players
        .iter()
        .filter(|player| {
            let player_name = &*player.name.read();
            clean_text(player_name)
                .to_lowercase()
                .contains(&cleaned_text)
        })
        .cloned()
        .collect()
}

pub(super) enum TargetLookup {
    Found(Player),
    NotFound,
    Ambiguous(Vec<Player>),
}

fn client_id_of_target(target: &str) -> Option<i32> {
    let digits = target
        .strip_prefix("id:")
        .or_else(|| target.strip_prefix("id="))
        .unwrap_or(target);
    if !(1..=2).contains(&digits.len()) || !digits.chars().all(|digit| digit.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|id| (0..64).contains(id))
}

pub(super) fn lookup_target(players: Vec<Player>, target: &str) -> TargetLookup {
    if let Some(found) =
        client_id_of_target(target).and_then(|id| players.iter().find(|player| player.id == id))
    {
        return TargetLookup::Found(found.clone());
    }

    let mut matches = players_matching_name(players, target);
    match matches.len() {
        0 => TargetLookup::NotFound,
        1 => matches
            .pop()
            .map_or(TargetLookup::NotFound, TargetLookup::Found),
        _ => TargetLookup::Ambiguous(matches),
    }
}
