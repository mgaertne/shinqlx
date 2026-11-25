//! Parser for the `{ key "value" }` blocks of `.arena` files and `arenas.txt`.
//!
//! It follows the engine's tokenizer: tokens are separated by whitespace, a quoted token runs to the next quote,
//! `//` and `/* */` comments are skipped, and values are taken literally.

use super::strip_control;

/// The pairs of one block in file order; keys keep their spelling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) entries: Vec<(String, String)>,
}

impl Block {
    /// The value of `key`, compared ignoring ASCII case; the last of several equal keys wins, like in an info string.
    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

/// Why the rest of a file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockError {
    /// A token other than `{` where a block should start.
    MissingOpeningBrace,
    /// The file ended inside a block; that block is dropped.
    UnexpectedEnd,
}

/// The complete blocks of a file, and the error that stopped parsing, if any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ParsedBlocks {
    pub(crate) blocks: Vec<Block>,
    pub(crate) error: Option<BlockError>,
}

/// Parses all blocks of `text`. Control characters (below 0x20) are removed from keys and values.
pub(crate) fn parse_blocks(text: &str) -> ParsedBlocks {
    let mut tokens = Tokens::new(text.strip_prefix('\u{feff}').unwrap_or(text));
    let mut parsed = ParsedBlocks::default();

    while let Some(opening) = tokens.next_token() {
        if opening != "{" {
            parsed.error = Some(BlockError::MissingOpeningBrace);
            break;
        }
        match read_block(&mut tokens) {
            Some(block) => parsed.blocks.push(block),
            None => {
                parsed.error = Some(BlockError::UnexpectedEnd);
                break;
            }
        }
    }
    parsed
}

/// Parses only the first block of `text`, `None` when there is none or it is incomplete.
pub(crate) fn parse_first_block(text: &str) -> Option<Block> {
    let mut tokens = Tokens::new(text.strip_prefix('\u{feff}').unwrap_or(text));
    (tokens.next_token()? == "{").then(|| read_block(&mut tokens))?
}

fn read_block(tokens: &mut Tokens<'_>) -> Option<Block> {
    let mut block = Block::default();
    loop {
        let key = tokens.next_token()?;
        if key == "}" {
            return Some(block);
        }
        let value = tokens.next_token()?;
        block
            .entries
            .push((strip_control(key), strip_control(value)));
    }
}

struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self { rest: text }
    }

    fn skip_blanks_and_comments(&mut self) {
        loop {
            self.rest = self.rest.trim_start_matches(|c: char| c <= ' ');
            if let Some(after) = self.rest.strip_prefix("//") {
                self.rest = after.find('\n').map_or("", |end| &after[end..]);
            } else if let Some(after) = self.rest.strip_prefix("/*") {
                self.rest = after.find("*/").map_or("", |end| &after[end + 2..]);
            } else {
                return;
            }
        }
    }

    fn next_token(&mut self) -> Option<&'a str> {
        self.skip_blanks_and_comments();
        if self.rest.is_empty() {
            return None;
        }
        if let Some(quoted) = self.rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            self.rest = quoted.get(end + 1..).unwrap_or("");
            return Some(&quoted[..end]);
        }
        let end = self
            .rest
            .find(|c: char| c <= ' ')
            .unwrap_or(self.rest.len());
        let (token, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    fn pairs(block: &Block) -> Vec<(&str, &str)> {
        block
            .entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }

    #[test]
    fn parses_a_quoted_block() {
        let parsed =
            parse_blocks("{\nmap \"campgrounds\"\nlongname \"Campgrounds\"\ntype \"ffa ca\"\n}\n");

        assert_eq!(parsed.error, None);
        assert_eq!(
            pairs(&parsed.blocks[0]),
            vec![
                ("map", "campgrounds"),
                ("longname", "Campgrounds"),
                ("type", "ffa ca")
            ]
        );
    }

    #[test]
    fn parses_several_blocks_unquoted_values_and_comments() {
        let text = "// header\n{ map a fraglimit 40 } /* in between\n} */ { map \"b\" // trailing\n type \"ca\" }";

        let parsed = parse_blocks(text);

        assert_eq!(parsed.error, None);
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(
            pairs(&parsed.blocks[0]),
            vec![("map", "a"), ("fraglimit", "40")]
        );
        assert_eq!(pairs(&parsed.blocks[1]), vec![("map", "b"), ("type", "ca")]);
    }

    #[test]
    fn handles_crlf_bom_and_multiline_values() {
        let parsed = parse_blocks("\u{feff}{\r\nmap \"a\"\r\nlongname \"two\r\nlines\"\r\n}\r\n");

        assert_eq!(parsed.error, None);
        assert_eq!(
            pairs(&parsed.blocks[0]),
            vec![("map", "a"), ("longname", "twolines")]
        );
    }

    #[test]
    fn removes_control_characters_but_keeps_everything_else() {
        let parsed = parse_blocks("{ longname \"^1Red\u{7}\tArena\u{7f} ü\" }");

        assert_eq!(
            pairs(&parsed.blocks[0]),
            vec![("longname", "^1RedArena\u{7f} ü")]
        );
    }

    #[test]
    fn empty_value_and_empty_block_are_kept() {
        let parsed = parse_blocks("{ type \"\" } { }");

        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(parsed.blocks[0].get("type"), Some(""));
        assert!(parsed.blocks[1].entries.is_empty());
    }

    #[test]
    fn get_ignores_case_and_prefers_the_last_duplicate() {
        let parsed = parse_blocks("{ Map a map b }");

        assert_eq!(parsed.blocks[0].get("MAP"), Some("b"));
        assert_eq!(parsed.blocks[0].get("type"), None);
    }

    #[rstest]
    #[case("", 0, None)]
    #[case("   \n// only a comment", 0, None)]
    #[case("{ map a } stray { map b }", 1, Some(BlockError::MissingOpeningBrace))]
    #[case("{ map a } { map b", 1, Some(BlockError::UnexpectedEnd))]
    #[case("{ map", 0, Some(BlockError::UnexpectedEnd))]
    #[case("{ map \"unterminated", 0, Some(BlockError::UnexpectedEnd))]
    fn keeps_complete_blocks_before_an_error(
        #[case] text: &str,
        #[case] blocks: usize,
        #[case] error: Option<BlockError>,
    ) {
        let parsed = parse_blocks(text);

        assert_eq!(parsed.blocks.len(), blocks);
        assert_eq!(parsed.error, error);
    }

    #[test]
    fn braces_inside_a_token_are_not_separate_tokens() {
        let parsed = parse_blocks("{map a}");

        assert_eq!(parsed.blocks.len(), 0);
        assert_eq!(parsed.error, Some(BlockError::MissingOpeningBrace));
    }
}
