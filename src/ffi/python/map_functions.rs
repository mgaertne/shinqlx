use std::path::{Path, PathBuf};

use pyo3::exceptions::PyEnvironmentError;

use super::{engine_api::get_cvar, prelude::*};
use crate::map_catalog::{MapCatalog, MapRecord, recent_catalog, scan};

/// The name of the cvar that points to the workshop directory of the Steam installation.
pub(crate) const WORKSHOP_PATH_CVAR: &str = "qlx_workshopPath";

/// The Steam app id of the Quake Live game, the workshop keeps its items below it.
const WORKSHOP_APP_ID: &str = "282440";

/// A map that is installed on the server, with what the `.arena` files declare for it.
#[pyclass(
    module = "_shinqlx",
    name = "MapInfo",
    frozen,
    get_all,
    eq,
    from_py_object
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapInfo {
    /// The name of the map as the `map` command takes it.
    name: String,
    /// The full name of the map, ``None`` if no `.arena` file declares it.
    longname: Option<String>,
    /// The short names of the gametypes the map declares, ``None`` if it declares none.
    gametypes: Option<Vec<String>>,
    /// The `pk3` files that contain the map.
    sources: Vec<String>,
    /// The ids of the workshop items that contain the map.
    workshop_ids: Vec<u64>,
}

impl From<&MapRecord> for MapInfo {
    fn from(record: &MapRecord) -> Self {
        Self {
            name: record.name.clone(),
            longname: record.longname.clone(),
            gametypes: record.gametypes.clone(),
            sources: record
                .sources
                .iter()
                .map(|source| source.pk3.to_string_lossy().into_owned())
                .collect(),
            workshop_ids: record
                .sources
                .iter()
                .filter_map(|source| source.workshop_id)
                .collect(),
        }
    }
}

#[pymethods]
impl MapInfo {
    fn __repr__(&self) -> String {
        let longname = self
            .longname
            .as_ref()
            .map_or_else(|| "None".to_string(), |longname| format!("{longname:?}"));
        let gametypes = self
            .gametypes
            .as_ref()
            .map_or_else(|| "None".to_string(), |gametypes| format!("{gametypes:?}"));
        format!(
            "MapInfo(name={:?}, longname={longname}, gametypes={gametypes}, workshop_ids={:?})",
            self.name, self.workshop_ids
        )
    }
}

fn workshop_directory(base: &Path) -> PathBuf {
    base.join("..")
        .join("..")
        .join("workshop")
        .join("content")
        .join(WORKSHOP_APP_ID)
}

fn catalog(py: Python<'_>) -> PyResult<MapCatalog> {
    if let Some(catalog) = recent_catalog() {
        return Ok(catalog);
    }

    let path_of = |cvar: &str| {
        get_cvar(cvar).map(|value| value.filter(|value| !value.is_empty()).map(PathBuf::from))
    };
    let base = path_of("fs_basepath")?
        .ok_or_else(|| PyEnvironmentError::new_err("the cvar fs_basepath is not available"))?;
    let home = path_of("fs_homepath")?.unwrap_or_else(|| base.clone());
    let workshop = path_of(WORKSHOP_PATH_CVAR)?.unwrap_or_else(|| workshop_directory(&base));

    Ok(py.detach(|| scan(&base, Some(&workshop), &home)))
}

/// Returns the maps that are installed on the server, in alphabetical order.
///
/// The maps are read from the ``pk3`` files of the search paths, so the first call can take a moment.
/// The result is reused for 30 seconds.
#[pyfunction]
pub(crate) fn installed_maps(py: Python<'_>) -> PyResult<Vec<MapInfo>> {
    Ok(catalog(py)?.maps().map(MapInfo::from).collect())
}

/// Returns what is known about an installed map, ``None`` if the map is not installed.
#[pyfunction]
pub(crate) fn map_info(py: Python<'_>, mapname: &str) -> PyResult<Option<MapInfo>> {
    Ok(catalog(py)?.map(mapname).map(MapInfo::from))
}

/// Returns the ids of the factories without duplicates, in the order they are declared.
#[pyfunction]
pub(crate) fn factories(py: Python<'_>) -> PyResult<Vec<String>> {
    Ok(catalog(py)?.factories().to_vec())
}

/// Returns whether a map declares a gametype, given by its short name like ``"ca"``.
///
/// ``None`` means that the map declares no gametypes at all, or is not installed. It does not mean ``False``.
#[pyfunction]
pub(crate) fn map_supports(
    py: Python<'_>,
    mapname: &str,
    gametype: &str,
) -> PyResult<Option<bool>> {
    Ok(catalog(py)?.supports(mapname, gametype))
}

#[cfg(test)]
mod map_functions_tests {
    use core::borrow::BorrowMut;
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyEnvironmentError;
    use rstest::rstest;

    use super::{MapInfo, factories, installed_maps, map_info, map_supports, workshop_directory};
    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        map_catalog::{MapRecord, MapSource, forget_scan, write_pack},
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_functions_read_the_installed_maps_through_the_cvars(_pyshinqlx_setup: ()) {
        forget_scan();
        let base = tempfile::tempdir().expect("could not create directory");
        let home = tempfile::tempdir().expect("could not create directory");
        std::fs::create_dir_all(base.path().join("baseq3")).expect("could not create directory");
        std::fs::create_dir_all(home.path().join("baseq3")).expect("could not create directory");
        write_pack(
            &base.path().join("baseq3/pak00.pk3"),
            &[
                ("maps/aerowalk.bsp", ""),
                (
                    "scripts/arenas.txt",
                    "{\nmap \"aerowalk\"\nlongname \"Aerowalk\"\ntype \"ffa ca\"\n}",
                ),
                ("scripts/factories.txt", r#"[{"id": "ffa"}]"#),
            ],
        );
        let base_path = alloc::ffi::CString::new(base.path().to_string_lossy().as_bytes())
            .expect("could not create string");
        let home_path = alloc::ffi::CString::new(home.path().to_string_lossy().as_bytes())
            .expect("could not create string");
        let empty = c"";
        let mut raw_base = CVarBuilder::default()
            .string(base_path.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_home = CVarBuilder::default()
            .string(home_path.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_empty = CVarBuilder::default()
            .string(empty.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |_| true,
                move |name| match name {
                    "fs_basepath" => CVar::try_from(raw_base.borrow_mut() as *mut cvar_t).ok(),
                    "fs_homepath" => CVar::try_from(raw_home.borrow_mut() as *mut cvar_t).ok(),
                    _ => CVar::try_from(raw_empty.borrow_mut() as *mut cvar_t).ok(),
                },
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let maps = installed_maps(py).expect("could not list maps");
                    assert_eq!(maps.len(), 1);
                    assert_eq!(maps[0].longname.as_deref(), Some("Aerowalk"));

                    let info = map_info(py, "AeroWalk").expect("could not look up map");
                    assert!(info.is_some_and(|info| info.name == "aerowalk"));
                    assert!(
                        map_info(py, "missing")
                            .expect("could not look up map")
                            .is_none()
                    );
                    assert_eq!(
                        factories(py).expect("could not list factories"),
                        vec!["ffa".to_string()]
                    );
                    assert_eq!(
                        map_supports(py, "aerowalk", "ca").expect("could not look up map"),
                        Some(true)
                    );
                });
            });
        forget_scan();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn without_the_base_path_the_functions_report_an_error(_pyshinqlx_setup: ()) {
        forget_scan();

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 1..)
            .run(|| {
                Python::attach(|py| {
                    assert!(
                        installed_maps(py)
                            .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py))
                    );
                });
            });
    }

    #[test]
    fn workshop_directory_lies_next_to_the_steam_apps() {
        assert_eq!(
            workshop_directory(Path::new("/srv/steamapps/common/qlds")),
            Path::new("/srv/steamapps/common/qlds/../../workshop/content/282440")
        );
    }

    #[test]
    fn map_info_lists_pk3_files_and_workshop_ids() {
        let record = MapRecord {
            name: "custom".to_string(),
            longname: Some("Custom Map".to_string()),
            gametypes: Some(vec!["ca".to_string()]),
            sources: vec![
                MapSource {
                    pk3: "/srv/baseq3/a.pk3".into(),
                    workshop_id: None,
                },
                MapSource {
                    pk3: "/srv/workshop/1234/b.pk3".into(),
                    workshop_id: Some(1234),
                },
            ],
        };

        assert_eq!(
            MapInfo::from(&record),
            MapInfo {
                name: "custom".to_string(),
                longname: Some("Custom Map".to_string()),
                gametypes: Some(vec!["ca".to_string()]),
                sources: vec![
                    "/srv/baseq3/a.pk3".to_string(),
                    "/srv/workshop/1234/b.pk3".to_string()
                ],
                workshop_ids: vec![1234],
            }
        );
    }
}
