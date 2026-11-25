//! Map names: which ones are accepted and how they are compared.
//!
//! The engine matches map names ignoring case, so lookups use the lowercase key.

/// Whether `name` consists only of `[A-Za-z0-9_.+-]` and is not empty.
pub(crate) fn is_valid_map_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-'))
}

/// The key under which a map is looked up.
pub(crate) fn map_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// The map name of an archive entry `maps/<name>.bsp`, or `None` for anything else or an invalid name.
pub(crate) fn map_name_from_bsp_path(path: &str) -> Option<&str> {
    let file = path
        .get(..5)
        .filter(|dir| dir.eq_ignore_ascii_case("maps/"))
        .map(|_| &path[5..])?;
    let stem_len = file.len().checked_sub(4)?;
    let (stem, extension) = (file.get(..stem_len)?, file.get(stem_len..)?);
    (extension.eq_ignore_ascii_case(".bsp") && is_valid_map_name(stem)).then_some(stem)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("campgrounds", true)]
    #[case("Geit3DM4", true)]
    #[case("map-charon3dm12_v1.2+", true)]
    #[case("", false)]
    #[case("two words", false)]
    #[case("a/b", false)]
    #[case("ümlaut", false)]
    #[case("a\u{7}b", false)]
    #[case("a;quit", false)]
    fn accepts_only_the_safe_character_set(#[case] name: &str, #[case] valid: bool) {
        assert_eq!(is_valid_map_name(name), valid);
    }

    #[test]
    fn key_ignores_case() {
        assert_eq!(map_key("CAMPgrounds"), map_key("campgrounds"));
    }

    #[rstest]
    #[case("maps/campgrounds.bsp", Some("campgrounds"))]
    #[case("MAPS/Geit3DM4.BSP", Some("Geit3DM4"))]
    #[case("maps/.bsp", None)]
    #[case("maps/sub/x.bsp", None)]
    #[case("maps/x.aas", None)]
    #[case("maps/x", None)]
    #[case("scripts/x.bsp", None)]
    #[case("maps/ä.bsp", None)]
    #[case("map", None)]
    fn extracts_the_name_from_a_bsp_path(#[case] path: &str, #[case] expected: Option<&str>) {
        assert_eq!(map_name_from_bsp_path(path), expected);
    }
}
