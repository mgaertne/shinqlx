//! Installing, listing and removing workshop items without the Python layer.

pub(crate) mod item;
pub(crate) mod list;
pub(crate) mod manifest;
pub(crate) mod steam;

#[cfg(test)]
mod tests {
    #[test]
    fn the_workshop_core_knows_nothing_about_python_or_the_ffi_layer() {
        let forbidden = ["pyo3", "crate::ffi", "super::super::ffi"];
        for (file, source) in [
            ("item.rs", include_str!("item.rs")),
            ("list.rs", include_str!("list.rs")),
            ("manifest.rs", include_str!("manifest.rs")),
            ("steam.rs", include_str!("steam.rs")),
        ] {
            let code = source.split("#[cfg(test)]").next().unwrap_or_default();
            for word in forbidden {
                assert!(!code.contains(word), "{file} mentions {word}");
            }
        }
    }
}
