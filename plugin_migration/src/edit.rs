use core::{fmt, ops::Range};

/// Replaces `range` of a source text by `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) range: Range<usize>,
    pub(crate) replacement: String,
}

impl Edit {
    pub(crate) fn replace(range: Range<usize>, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OverlappingEdits;

impl fmt::Display for OverlappingEdits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("edits overlap")
    }
}

impl core::error::Error for OverlappingEdits {}

/// Applies `edits` to `text`; fails if two edits touch the same bytes.
pub(crate) fn apply_edits(text: &str, mut edits: Vec<Edit>) -> Result<String, OverlappingEdits> {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

    let mut result = String::with_capacity(text.len());
    let mut position = 0;
    for edit in &edits {
        if edit.range.start < position {
            return Err(OverlappingEdits);
        }
        result.push_str(&text[position..edit.range.start]);
        result.push_str(&edit.replacement);
        position = edit.range.end;
    }
    result.push_str(&text[position..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{Edit, OverlappingEdits, apply_edits};

    #[test]
    fn applies_edits_independent_of_their_order() {
        let edits = vec![Edit::replace(6..11, "there"), Edit::replace(0..5, "hi")];
        assert_eq!(
            apply_edits("hello world", edits),
            Ok("hi there".to_string())
        );
    }

    #[test]
    fn inserts_at_a_position() {
        let edits = vec![Edit::replace(5..5, ",")];
        assert_eq!(
            apply_edits("hello world", edits),
            Ok("hello, world".to_string())
        );
    }

    #[test]
    fn rejects_overlapping_edits() {
        let edits = vec![Edit::replace(0..5, "a"), Edit::replace(3..8, "b")];
        assert_eq!(apply_edits("hello world", edits), Err(OverlappingEdits));
    }

    #[test]
    fn no_edits_return_the_text() {
        assert_eq!(apply_edits("same", vec![]), Ok("same".to_string()));
    }
}
