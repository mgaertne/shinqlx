//! Maps the `type` tokens of an arena block to game types, the way the engine reads them.
//!
//! The engine understands exactly the lowercase tokens of this table. Everything else (`CA`, `1f`, the old Quake 3
//! tokens `tourney` and `team`, unknown words) is ignored by it and is therefore not translated here.

use super::block::Block;
use crate::gametypes::GameTypes;

const TOKENS: [(&str, GameTypes); 12] = [
    ("ffa", GameTypes::FreeForAll),
    ("duel", GameTypes::Duel),
    ("race", GameTypes::Race),
    ("tdm", GameTypes::TeamDeathmatch),
    ("ca", GameTypes::ClanArena),
    ("ctf", GameTypes::CaptureTheFlag),
    ("oneflag", GameTypes::OneFlag),
    ("har", GameTypes::Harvester),
    ("ft", GameTypes::FreezeTag),
    ("dom", GameTypes::Domination),
    ("ad", GameTypes::AttackAndDefend),
    ("rr", GameTypes::RedRover),
];

/// The game type for an arena token, if the engine understands it.
pub(crate) fn gametype_from_token(token: &str) -> Option<GameTypes> {
    TOKENS
        .iter()
        .find_map(|(known, gametype)| (*known == token).then_some(*gametype))
}

/// The tokens of the `type` value of an arena block as written, including the ones the engine ignores, and the
/// game types they stand for, in the order of their first token and without repeats. A block without a `type` key,
/// or with an empty one, makes the map unplayable.
pub(crate) fn parse_type(block: &Block) -> (Vec<GameTypes>, Vec<String>) {
    let type_tokens = block
        .get("type")
        .map(|value| {
            value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut gametypes = Vec::new();
    for gametype in type_tokens
        .iter()
        .filter_map(|token| gametype_from_token(token))
    {
        if !gametypes.contains(&gametype) {
            gametypes.push(gametype);
        }
    }
    (gametypes, type_tokens)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;
    use crate::maps::block::parse_blocks;

    fn block(text: &str) -> Block {
        parse_blocks(text).blocks.remove(0)
    }

    #[rstest]
    #[case("ffa", Some(GameTypes::FreeForAll))]
    #[case("oneflag", Some(GameTypes::OneFlag))]
    #[case("rr", Some(GameTypes::RedRover))]
    #[case("1f", None)]
    #[case("CA", None)]
    #[case("tourney", None)]
    #[case("team", None)]
    #[case("banana", None)]
    #[case("", None)]
    fn token_table_follows_the_engine(#[case] token: &str, #[case] expected: Option<GameTypes>) {
        assert_eq!(gametype_from_token(token), expected);
    }

    #[test]
    fn every_game_type_has_exactly_one_token() {
        for gametype in GameTypes::ALL {
            let tokens = TOKENS
                .iter()
                .filter(|(_, known)| *known == gametype)
                .count();
            assert_eq!(tokens, 1, "{gametype:?}");
        }
    }

    #[test]
    fn block_keeps_all_tokens_and_the_understood_game_types_without_repeats() {
        let (gametypes, tokens) = parse_type(&block(r#"{ map a type "ca tourney CA ffa ca" }"#));

        assert_eq!(gametypes, vec![GameTypes::ClanArena, GameTypes::FreeForAll]);
        assert_eq!(tokens, vec!["ca", "tourney", "CA", "ffa", "ca"]);
    }

    #[rstest]
    #[case(r#"{ map a }"#)]
    #[case(r#"{ map a type "" }"#)]
    #[case(r#"{ map a type "tourney team" }"#)]
    fn block_without_understood_tokens_is_unplayable(#[case] text: &str) {
        assert!(parse_type(&block(text)).0.is_empty());
    }
}
