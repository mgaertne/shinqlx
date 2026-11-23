//! The color codes `^0` to `^7` of Quake Live in player names and server messages.

fn is_color_code(bytes: &[u8], at: usize) -> bool {
    bytes.get(at) == Some(&b'^') && matches!(bytes.get(at + 1), Some(b'0'..=b'7'))
}

/// Splits `input` at its color codes into `(code, text)` pairs; text before the first code has the code `"reset"`.
pub fn split_ql_colors(input: &str) -> Vec<(&str, &str)> {
    let bytes = input.as_bytes();
    let first_code = (0..bytes.len())
        .find(|&at| is_color_code(bytes, at))
        .unwrap_or(bytes.len());

    let mut result = vec![("reset", &input[..first_code])];
    let mut at = first_code;
    while at < bytes.len() {
        if !is_color_code(bytes, at) {
            at += 1;
            continue;
        }
        let text_start = at + 2;
        let text_end = input[text_start..]
            .find('^')
            .map_or(input.len(), |offset| text_start + offset);
        if text_end > text_start {
            result.push((&input[at..text_start], &input[text_start..text_end]));
            at = text_end;
        } else {
            at += 1;
        }
    }

    result.retain(|(_, text)| !text.is_empty());
    result
}

/// The RGB values of `^1` to `^7`, from the 8 color palette in the guide at <https://www.euere.eu/ql/>.
/// `^0` is `None`: it stays the black of the terminal theme, so that it remains readable.
pub fn ql_rgb(code: &str) -> Option<(u8, u8, u8)> {
    match code {
        "^0" => None,
        "^1" => Some((0xFF, 0x00, 0x00)),
        "^2" => Some((0x00, 0xFF, 0x00)),
        "^3" => Some((0xFF, 0xFF, 0x00)),
        "^4" => Some((0x00, 0x00, 0xFF)),
        "^5" => Some((0x00, 0xFF, 0xFF)),
        "^6" => Some((0xFF, 0x00, 0xFF)),
        _ => Some((0xFF, 0xFF, 0xFF)),
    }
}

#[cfg(test)]
mod quake_colors_tests {
    use super::{ql_rgb, split_ql_colors};

    #[test]
    fn split_ql_colors_returns_unformatted_text_as_reset() {
        assert_eq!(split_ql_colors("hello"), vec![("reset", "hello")]);
    }

    #[test]
    fn split_ql_colors_splits_at_every_color_code() {
        assert_eq!(
            split_ql_colors("pre^1red^2green"),
            vec![("reset", "pre"), ("^1", "red"), ("^2", "green")]
        );
    }

    #[test]
    fn split_ql_colors_skips_the_empty_leading_part() {
        assert_eq!(split_ql_colors("^4blue"), vec![("^4", "blue")]);
    }

    #[test]
    fn split_ql_colors_of_empty_text_is_empty() {
        assert!(split_ql_colors("").is_empty());
    }

    #[test]
    fn split_ql_colors_ignores_consecutive_color_codes_without_text() {
        assert_eq!(split_ql_colors("^1^2green"), vec![("^2", "green")]);
    }

    #[test]
    fn split_ql_colors_drops_text_after_an_unknown_color_code() {
        assert_eq!(split_ql_colors("^1red^8lost"), vec![("^1", "red")]);
    }

    #[test]
    fn split_ql_colors_keeps_non_ascii_text() {
        assert_eq!(split_ql_colors("ä^3ö"), vec![("reset", "ä"), ("^3", "ö")]);
    }

    #[test]
    fn the_black_code_has_no_rgb_value() {
        assert_eq!(ql_rgb("^0"), None);
        assert_eq!(ql_rgb("^1"), Some((0xFF, 0, 0)));
        assert_eq!(ql_rgb("^7"), Some((0xFF, 0xFF, 0xFF)));
    }
}
