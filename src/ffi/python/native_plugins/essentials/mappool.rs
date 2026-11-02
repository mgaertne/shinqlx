use std::{collections::BTreeMap, path::Path};

use log::warn;
use pyo3::prelude::*;

use super::{
    super::{target::stop_all, tell},
    command_prefix,
};
use crate::ffi::python::get_cvar;

pub(super) type Mappool = BTreeMap<String, Vec<String>>;

pub(super) fn parse_mappool(contents: &str) -> Mappool {
    let mut mappool = Mappool::new();
    for line in contents.lines() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        let Some((map, factory)) = line.split_once('|') else {
            continue;
        };
        mappool
            .entry(map.trim().to_lowercase())
            .or_default()
            .push(factory.trim().to_owned());
    }
    mappool
}

fn read_mappool(path: &Path) -> Option<Mappool> {
    std::fs::read_to_string(path)
        .map_err(|err| warn!(target: "shinqlx", "could not read the map pool {path:?}: {err}"))
        .ok()
        .map(|contents| parse_mappool(&contents))
}

pub(super) fn load_mappool() -> PyResult<Option<Mappool>> {
    let Some(mappool_file) = get_cvar("sv_mappoolfile")? else {
        return Ok(None);
    };
    for cvar in ["fs_homepath", "fs_basepath"] {
        let Some(base) = get_cvar(cvar)? else {
            continue;
        };
        let path = Path::new(&base).join("baseq3").join(&mappool_file);
        if path.is_file() {
            return Ok(read_mappool(&path));
        }
    }
    Ok(None)
}

pub(super) fn mappool_listing(mappool: &Mappool, indent: usize) -> String {
    mappool
        .iter()
        .map(|(map, factories)| {
            format!(
                "{}{map:25} Factories: {}",
                " ".repeat(indent),
                factories.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn mappool_error(caller: &Bound<'_, PyAny>, message: &str) -> PyResult<Option<i32>> {
    tell(
        caller,
        &format!(
            "{message} Use {}mappool to see available options.",
            command_prefix()?
        ),
    )?;
    stop_all()
}
