//! Property tests for the parsers: whatever a map or a file contains, they must not panic, and what they return
//! must keep the promises of their documentation.

use proptest::prelude::*;

use super::{
    block::{parse_blocks, parse_first_block},
    bsp::{mini_bsp, parse_worldspawn},
    factory::parse_factories,
    gametype::parse_type,
    name::{is_valid_map_name, map_key, map_name_from_bsp_path},
};

fn has_control_character(text: &str) -> bool {
    text.chars().any(|character| (character as u32) < 0x20)
}

proptest! {
    #[test]
    fn blocks_of_any_text_have_no_control_characters(text in any::<String>()) {
        let parsed = parse_blocks(&text);

        for block in &parsed.blocks {
            for (key, value) in &block.entries {
                prop_assert!(!has_control_character(key));
                prop_assert!(!has_control_character(value));
            }
        }
    }

    #[test]
    fn well_formed_blocks_are_read_back(
        blocks in prop::collection::vec(
            prop::collection::vec(("[a-z][a-z0-9_]{0,8}", "[A-Za-z0-9 ._+-]{0,16}"), 0..6),
            0..5,
        )
    ) {
        let text = blocks
            .iter()
            .map(|entries| {
                let body = entries
                    .iter()
                    .map(|(key, value)| format!("{key} \"{value}\""))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{{\n{body}\n}}")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let parsed = parse_blocks(&text);

        prop_assert_eq!(parsed.error, None);
        prop_assert_eq!(parsed.blocks.len(), blocks.len());
        for (block, entries) in parsed.blocks.iter().zip(&blocks) {
            prop_assert_eq!(&block.entries, entries);
        }
    }

    #[test]
    fn the_first_block_is_the_first_of_all_blocks(text in any::<String>()) {
        let first = parse_first_block(&text);
        let all = parse_blocks(&text);

        prop_assert_eq!(first.as_ref(), all.blocks.first());
    }

    #[test]
    fn a_bsp_of_any_bytes_does_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = parse_worldspawn(&bytes);
    }

    #[test]
    fn a_cut_bsp_does_not_panic(text in any::<String>(), cut in 0_usize..400) {
        let bsp = mini_bsp(&text);
        let _ = parse_worldspawn(&bsp[..cut.min(bsp.len())]);
    }

    #[test]
    fn the_worldspawn_texts_are_what_the_entity_text_says(
        message in "[A-Za-z0-9 ^._-]{1,20}",
        author in "[A-Za-z0-9 ._-]{1,20}",
    ) {
        let bsp = mini_bsp(&format!(
            "{{ \"classname\" \"worldspawn\" \"message\" \"{message}\" \"author\" \"{author}\" }}"
        ));

        let worldspawn = parse_worldspawn(&bsp).expect("a worldspawn");

        prop_assert_eq!(worldspawn.message, Some(message));
        prop_assert_eq!(worldspawn.author, Some(author));
        prop_assert_eq!(worldspawn.author2, None);
    }

    #[test]
    fn factories_of_any_text_do_not_panic_and_have_an_id(text in any::<String>()) {
        if let Ok(parsed) = parse_factories(&text) {
            prop_assert!(parsed.factories.iter().all(|factory| !factory.id.is_empty()));
        }
    }

    #[test]
    fn every_entry_of_a_factory_file_is_used_or_counted(
        ids in prop::collection::vec(prop::option::of("[a-z_]{1,8}"), 0..8),
    ) {
        let entries = ids
            .iter()
            .map(|id| match id {
                Some(id) => format!("{{\"id\": \"{id}\"}}"),
                None => "{\"title\": \"no id\"}".to_owned(),
            })
            .collect::<Vec<_>>()
            .join(",");

        let parsed = parse_factories(&format!("[{entries}]")).expect("an array");

        prop_assert_eq!(parsed.factories.len() + parsed.skipped, ids.len());
        prop_assert_eq!(parsed.skipped, ids.iter().filter(|id| id.is_none()).count());
    }

    #[test]
    fn a_map_name_from_a_path_is_valid_and_part_of_the_path(path in any::<String>()) {
        if let Some(name) = map_name_from_bsp_path(&path) {
            prop_assert!(is_valid_map_name(name));
            let expected = format!("maps/{}.bsp", name.to_ascii_lowercase());
            prop_assert_eq!(path.to_ascii_lowercase(), expected);
        }
    }

    #[test]
    fn a_valid_name_is_found_in_its_bsp_path(
        name in "[A-Za-z0-9_.+-]{1,24}",
        extension in prop::sample::select(vec![".bsp", ".BSP", ".Bsp"]),
    ) {
        let path = format!("maps/{name}{extension}");

        prop_assert_eq!(map_name_from_bsp_path(&path), Some(name.as_str()));
    }

    #[test]
    fn a_valid_name_has_only_the_allowed_characters(name in any::<String>()) {
        let allowed = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'));

        prop_assert_eq!(is_valid_map_name(&name), allowed);
    }

    #[test]
    fn the_key_of_a_name_ignores_case(name in "[A-Za-z0-9_.+-]{1,24}") {
        prop_assert_eq!(map_key(&name), map_key(&name.to_ascii_uppercase()));
    }

    #[test]
    fn game_types_of_a_type_value_are_known_and_without_repeats(value in "[a-zA-Z0-9 ]{0,40}") {
        let block = parse_first_block(&format!("{{ map a type \"{value}\" }}")).expect("a block");

        let (gametypes, tokens) = parse_type(&block);

        prop_assert_eq!(tokens.len(), value.split_whitespace().count());
        for (index, gametype) in gametypes.iter().enumerate() {
            prop_assert!(!gametypes[..index].contains(gametype));
        }
        prop_assert!(gametypes.len() <= tokens.len());
    }
}
