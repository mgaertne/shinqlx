//! Reads the maps, arena blocks and factories that a Quake Live server has installed.
//!
//! The module is pure Rust and knows nothing about Python; the Python layer builds on top of it.

pub(crate) mod archive;
pub(crate) mod block;
pub(crate) mod bsp;
pub(crate) mod factory;
pub(crate) mod filesystem;
pub(crate) mod gametype;
pub(crate) mod loader;
pub(crate) mod model;
pub(crate) mod name;
#[cfg(all(test, not(miri)))]
mod properties;
pub(crate) mod scan;
pub(crate) mod sources;
pub(crate) mod store;

/// Removes the control characters (below 0x20) from text taken from a file; everything else stays verbatim.
fn strip_control(text: &str) -> String {
    text.chars().filter(|c| *c >= ' ').collect()
}

#[cfg(test)]
mod boundary {
    #[test]
    fn module_does_not_depend_on_python_or_the_ffi_layer() {
        let sources = [
            include_str!("archive.rs"),
            include_str!("block.rs"),
            include_str!("bsp.rs"),
            include_str!("factory.rs"),
            include_str!("filesystem.rs"),
            include_str!("gametype.rs"),
            include_str!("loader.rs"),
            include_str!("model.rs"),
            include_str!("name.rs"),
            include_str!("scan.rs"),
            include_str!("sources.rs"),
            include_str!("store.rs"),
        ];
        let forbidden = ["pyo3", "crate::ffi", "super::super::ffi"];

        for (index, source) in sources.iter().enumerate() {
            let code = source.split("#[cfg(test)]").next().unwrap_or_default();
            for word in forbidden {
                assert!(!code.contains(word), "source {index} uses {word}");
            }
        }
    }
}
