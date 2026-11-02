use alloc::ffi::CString;
use core::ffi::CStr;

use pyo3::{intern, prelude::*};
use rstest::rstest;

use super::{Essentials, create};
use crate::{
    ffi::python::{
        COMMANDS, EVENT_DISPATCHERS,
        commands::CommandInvoker,
        events::{
            CommandDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
            PlayerConnectDispatcher, PlayerDisconnectDispatcher, VoteCalledDispatcher,
        },
        pyshinqlx_setup_fixture::pyshinqlx_setup,
        pyshinqlx_test_support::engine_with_cvars,
    },
    prelude::*,
};

const KNOWN_CVARS: [(&str, &CStr); 6] = [
    ("zmq_stats_enable", c"0"),
    ("qlx_votepass", c"1"),
    ("qlx_votepassThreshold", c"0.33"),
    ("qlx_teamsizeMinimum", c"1"),
    ("qlx_teamsizeMaximum", c"8"),
    ("qlx_enforceMappool", c"0"),
];
const MAPPOOL_CVARS: [&str; 3] = ["sv_mappoolfile", "fs_homepath", "fs_basepath"];

fn leaked(text: &str) -> &'static CStr {
    Box::leak(
        CString::new(text)
            .expect("the cvar value contains no nul byte")
            .into_boxed_c_str(),
    )
}

fn with_created_plugin<F>(mappool_cvars: &[(&'static str, &'static CStr)], test: F)
where
    F: FnOnce(Python<'_>, Bound<'_, Essentials>),
{
    let cvars: Vec<_> = KNOWN_CVARS
        .iter()
        .chain(mappool_cvars.iter())
        .copied()
        .collect();
    let mut engine = MAPPOOL_CVARS
        .into_iter()
        .filter(|name| !mappool_cvars.iter().any(|(set, _)| set == name))
        .fold(engine_with_cvars(&cvars), |engine, missing| {
            engine.with_find_cvar(move |cmd| cmd == missing, |_| None, 0..)
        });
    engine.run(|| {
        Python::attach(|py| {
            let dispatchers = Bound::new(py, EventDispatcherManager::default())
                .expect("could not create the dispatcher manager");
            for dispatcher in [
                py.get_type::<PlayerConnectDispatcher>(),
                py.get_type::<PlayerDisconnectDispatcher>(),
                py.get_type::<VoteCalledDispatcher>(),
                py.get_type::<CommandDispatcher>(),
            ] {
                dispatchers
                    .add_dispatcher(&dispatcher)
                    .expect("could not add the dispatcher");
            }
            EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));
            COMMANDS.store(Some(
                Py::new(py, CommandInvoker::py_new())
                    .expect("could not create the command invoker")
                    .into(),
            ));

            let plugin = create(py)
                .expect("could not create the plugin")
                .cast_into::<Essentials>()
                .expect("the plugin is not an Essentials plugin");
            test(py, plugin);
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn creating_the_plugin_registers_the_hooks_and_the_commands(_pyshinqlx_setup: ()) {
    with_created_plugin(&[], |py, plugin| {
        let hooks = plugin
            .getattr(intern!(py, "hooks"))
            .and_then(|hooks| hooks.len())
            .expect("could not get the hooks");
        let commands = plugin
            .getattr(intern!(py, "commands"))
            .and_then(|commands| commands.len())
            .expect("could not get the commands");

        assert_eq!(hooks, 4);
        assert_eq!(commands, 41);
        assert!(plugin.get().mappool.is_none());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn creating_the_plugin_loads_the_mappool_from_the_homepath(_pyshinqlx_setup: ()) {
    let home = tempfile::tempdir().expect("could not create the temp dir");
    std::fs::create_dir(home.path().join("baseq3")).expect("could not create baseq3");
    std::fs::write(
        home.path().join("baseq3").join("mappool.txt"),
        "campgrounds|ca\ncampgrounds|ffa\n",
    )
    .expect("could not write the map pool");
    let home_path = leaked(&home.path().to_string_lossy());

    with_created_plugin(
        &[
            ("sv_mappoolfile", c"mappool.txt"),
            ("fs_homepath", home_path),
        ],
        |_, plugin| {
            let mappool = plugin.get().mappool.clone().expect("no map pool loaded");
            assert_eq!(mappool["campgrounds"], ["ca", "ffa"]);
        },
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn creating_the_plugin_falls_back_to_the_basepath_for_the_mappool(_pyshinqlx_setup: ()) {
    let base = tempfile::tempdir().expect("could not create the temp dir");
    std::fs::create_dir(base.path().join("baseq3")).expect("could not create baseq3");
    std::fs::write(base.path().join("baseq3").join("pool.txt"), "overkill|ca\n")
        .expect("could not write the map pool");
    let empty_home = tempfile::tempdir().expect("could not create the temp dir");
    let base_path = leaked(&base.path().to_string_lossy());
    let home_path = leaked(&empty_home.path().to_string_lossy());

    with_created_plugin(
        &[
            ("sv_mappoolfile", c"pool.txt"),
            ("fs_homepath", home_path),
            ("fs_basepath", base_path),
        ],
        |_, plugin| {
            let mappool = plugin.get().mappool.clone().expect("no map pool loaded");
            assert_eq!(mappool["overkill"], ["ca"]);
        },
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn creating_the_plugin_without_a_mappool_file_has_no_mappool(_pyshinqlx_setup: ()) {
    let base = tempfile::tempdir().expect("could not create the temp dir");
    let base_path = leaked(&base.path().to_string_lossy());

    with_created_plugin(
        &[
            ("sv_mappoolfile", c"missing.txt"),
            ("fs_basepath", base_path),
        ],
        |_, plugin| assert!(plugin.get().mappool.is_none()),
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn creating_the_plugin_ignores_an_unreadable_mappool_file(_pyshinqlx_setup: ()) {
    let home = tempfile::tempdir().expect("could not create the temp dir");
    std::fs::create_dir(home.path().join("baseq3")).expect("could not create baseq3");
    std::fs::write(
        home.path().join("baseq3").join("broken.txt"),
        [0xff, 0xfe, 0xfd],
    )
    .expect("could not write the map pool");
    let home_path = leaked(&home.path().to_string_lossy());

    with_created_plugin(
        &[
            ("sv_mappoolfile", c"broken.txt"),
            ("fs_homepath", home_path),
        ],
        |_, plugin| assert!(plugin.get().mappool.is_none()),
    );
}
