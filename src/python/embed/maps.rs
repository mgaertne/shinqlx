use pyo3::{exceptions::PyValueError, types::PyTuple};

use crate::{
    gametypes::GameTypes,
    maps::{loader, name::map_key, store},
    python::prelude::*,
};

/// The installed maps, one `MapInfo` per map sorted by name.
///
/// `gametype` limits the list to the maps that run in that game type, a member of `GameTypes` or its short name;
/// maps without an arena block run in every game type. Until the first scan has finished the result is empty, so a
/// plugin that needs it while it loads should ask later, e.g. with `shinqlx.delay`.
#[pyfunction]
#[pyo3(name = "maps", signature = (gametype = None), text_signature = "(gametype=None)")]
pub(crate) fn pyshinqlx_maps<'py>(
    py: Python<'py>,
    gametype: Option<&str>,
) -> PyResult<Bound<'py, PyTuple>> {
    if let Some(gametype) = gametype
        && !GameTypes::ALL
            .iter()
            .any(|known| known.type_short() == gametype)
    {
        return Err(PyValueError::new_err(format!(
            "unknown game type '{gametype}', use a short name of GameTypes."
        )));
    }

    let infos = store::current().map_or_else(Vec::new, |snapshot| {
        snapshot
            .maps
            .values()
            .filter(|map| {
                gametype.is_none_or(|gametype| {
                    map.gametypes()
                        .iter()
                        .any(|known| known.type_short() == gametype)
                })
            })
            .cloned()
            .map(MapInfo)
            .collect()
    });
    let infos = infos
        .into_iter()
        .map(|info| Bound::new(py, info))
        .collect::<PyResult<Vec<_>>>()?;
    PyTuple::new(py, infos)
}

/// The information about the map `name`, ignoring case, or `None` if no such map is installed.
///
/// Until the first scan has finished the answer is empty, so a plugin that needs it while it loads should ask
/// later, e.g. with `shinqlx.delay`.
#[pyfunction]
#[pyo3(name = "map_info")]
pub(crate) fn pyshinqlx_map_info(name: &str) -> Option<MapInfo> {
    store::current()?
        .maps
        .get(&map_key(name))
        .cloned()
        .map(MapInfo)
}

/// The factories of the installed `.factories` files, the most important first.
///
/// Until the first scan has finished the answer is empty, so a plugin that needs it while it loads should ask
/// later, e.g. with `shinqlx.delay`.
#[pyfunction]
#[pyo3(name = "factories")]
pub(crate) fn pyshinqlx_factories(py: Python<'_>) -> PyResult<Bound<'_, PyTuple>> {
    let factories = store::current().map_or_else(Vec::new, |snapshot| {
        snapshot
            .factories
            .iter()
            .cloned()
            .map(FactoryInfo)
            .collect()
    });
    let factories = factories
        .into_iter()
        .map(|factory| Bound::new(py, factory))
        .collect::<PyResult<Vec<_>>>()?;
    PyTuple::new(py, factories)
}

/// The factory with the id `factory_id`, ignoring case, or `None`; if several files define it, the most important.
///
/// Until the first scan has finished the answer is empty, so a plugin that needs it while it loads should ask
/// later, e.g. with `shinqlx.delay`.
#[pyfunction]
#[pyo3(name = "factory_info")]
pub(crate) fn pyshinqlx_factory_info(factory_id: &str) -> Option<FactoryInfo> {
    store::current()?
        .factories
        .iter()
        .find(|info| info.factory.id.eq_ignore_ascii_case(factory_id))
        .cloned()
        .map(FactoryInfo)
}

/// Scans the installed maps again in the background and returns at once.
///
/// The search paths of the game are read anew with the next frame, so workshop items that were installed since the
/// last scan are included. The information of the last scan stays valid until the new one is complete. Returns
/// whether the scan was requested; it is not when the search paths of the game are not known yet.
#[pyfunction]
#[pyo3(name = "reload_maps")]
pub(crate) fn pyshinqlx_reload_maps() -> bool {
    loader::request_refresh()
}

#[cfg(test)]
mod maps_tests {
    use pretty_assertions::assert_eq;
    use pyo3::{intern, prelude::*, types::PyTuple};
    use rstest::rstest;
    use serial_test::serial;

    use super::*;
    use crate::{
        maps::{
            block::parse_blocks,
            factory::Factory,
            model::{ArenaInfo as CoreArena, FactoryInfo as CoreFactory, MapInfo as CoreMap},
            scan::Snapshot,
        },
        python::pyshinqlx_setup_fixture::*,
    };

    fn map(name: &str, tokens: Option<&str>) -> CoreMap {
        let arenas = tokens
            .into_iter()
            .filter_map(|tokens| {
                CoreArena::from_block(
                    &parse_blocks(&format!(r#"{{ map "{name}" type "{tokens}" }}"#)).blocks[0],
                    None,
                )
            })
            .collect();
        CoreMap::assemble(name, None, arenas, Vec::new())
    }

    fn factory(id: &str, title: &str) -> CoreFactory {
        CoreFactory {
            factory: Factory {
                id: id.to_owned(),
                title: Some(title.to_owned()),
                author: None,
                description: None,
                basegt: None,
                cvars: Default::default(),
            },
            source: None,
        }
    }

    fn publish_sample() {
        let mut snapshot = Snapshot::default();
        for info in [
            map("Campgrounds", Some("ffa ca")),
            map("aerowalk", Some("duel")),
            map("bare", None),
        ] {
            snapshot.maps.insert(info.key(), info);
        }
        snapshot.factories = vec![factory("ca2", "first"), factory("CA2", "second")];
        store::publish(snapshot);
    }

    fn names(py: Python<'_>, result: &Bound<'_, PyTuple>) -> Vec<String> {
        result
            .iter()
            .map(|info| {
                info.getattr(intern!(py, "name"))
                    .and_then(|name| name.extract::<String>())
                    .expect("a name")
            })
            .collect()
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn maps_lists_every_map_sorted_by_name_and_filters_by_game_type(_pyshinqlx_setup: ()) {
        publish_sample();

        Python::attach(|py| {
            let all = pyshinqlx_maps(py, None).expect("maps");
            assert_eq!(names(py, &all), vec!["aerowalk", "bare", "Campgrounds"]);

            let ca = pyshinqlx_maps(py, Some("ca")).expect("maps");
            assert_eq!(names(py, &ca), vec!["bare", "Campgrounds"]);

            let duel = pyshinqlx_maps(py, Some("duel")).expect("maps");
            assert_eq!(names(py, &duel), vec!["aerowalk", "bare"]);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn maps_takes_a_member_of_game_types(_pyshinqlx_setup: ()) {
        publish_sample();

        Python::attach(|py| {
            let module = py.import("shinqlx").expect("the module");
            let member = module
                .getattr("GameTypes")
                .and_then(|class| class.getattr("CLAN_ARENA"))
                .expect("a member");

            let ca = module
                .getattr("maps")
                .and_then(|maps| maps.call1((member,)))
                .and_then(|result| result.cast_into::<PyTuple>().map_err(PyErr::from))
                .expect("maps");

            assert_eq!(names(py, &ca), vec!["bare", "Campgrounds"]);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn reload_maps_requests_a_scan_only_when_the_search_paths_are_known(_pyshinqlx_setup: ()) {
        loader::clear_refresh_request();
        let configured = loader::is_configured();

        let requested = pyshinqlx_reload_maps();

        assert_eq!(requested, configured);
        assert_eq!(loader::refresh_requested(), configured);
        loader::clear_refresh_request();
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn maps_refuses_an_unknown_game_type(_pyshinqlx_setup: ()) {
        publish_sample();

        Python::attach(|py| {
            let result = pyshinqlx_maps(py, Some("banana"));

            assert!(result.is_err_and(|error| error.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn map_info_finds_a_map_ignoring_case(_pyshinqlx_setup: ()) {
        publish_sample();

        Python::attach(|_py| {
            let found = pyshinqlx_map_info("CAMPGROUNDS").expect("the map is installed");
            assert_eq!(found.0.name, "Campgrounds");
            assert!(pyshinqlx_map_info("nowhere").is_none());
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn factory_info_takes_the_first_factory_with_the_id_ignoring_case(_pyshinqlx_setup: ()) {
        publish_sample();

        Python::attach(|py| {
            let found = pyshinqlx_factory_info("Ca2").expect("a factory");
            assert_eq!(found.0.factory.title.as_deref(), Some("first"));
            assert!(pyshinqlx_factory_info("none").is_none());
            assert_eq!(pyshinqlx_factories(py).expect("factories").len(), 2);
        });
    }
}
