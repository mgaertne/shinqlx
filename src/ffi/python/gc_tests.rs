use pyo3::{intern, prelude::*};
use rstest::rstest;

use crate::{
    ffi::python::{
        prelude::*, pyshinqlx_setup_fixture::pyshinqlx_setup,
        pyshinqlx_test_support::RestoredAttribute,
    },
    prelude::*,
};

fn gc_checks(py: Python<'_>) -> Bound<'_, PyModule> {
    PyModule::from_code(
        py,
        cr#"
import gc
import os

import shinqlx
from shinqlx.database import AbstractDatabase


class GcProbeDatabase(AbstractDatabase):
    pass


class GcProbePlugin(shinqlx.Plugin):
    pass


def survivors(type_name):
    gc.collect()
    return [o for o in gc.get_objects() if type(o).__name__ == type_name]


def surviving_classes(class_name):
    gc.collect()
    return [o for o in gc.get_objects() if isinstance(o, type) and o.__name__ == class_name]


def plugin_with_database_is_collected():
    plugin = GcProbePlugin()
    plugin.db
    del plugin
    return not survivors("GcProbePlugin") and not survivors("GcProbeDatabase")


def class_with_deferred_method_is_collected():
    def make():
        class DeferredProbe:
            @shinqlx.next_frame
            def method(self):
                return __class__

    make()
    return not surviving_classes("DeferredProbe")


def deferred_call_exposes_its_function():
    def function():
        pass

    return any(referent is function for referent in gc.get_referents(shinqlx.next_frame(function)))


def resident_kib():
    with open("/proc/self/statm") as statm:
        return int(statm.read().split()[1]) * os.sysconf("SC_PAGE_SIZE") // 1024


def churn(iterations):
    for _ in range(iterations):
        plugin = GcProbePlugin()
        plugin.db

        class Deferred:
            @shinqlx.next_frame
            def method(self):
                return __class__

        shinqlx.Cvar("sample", 5.5, type=float, minimum=1.5, maximum=9.5)


def memory_growth(iterations):
    churn(100)
    gc.collect()
    objects_before, resident_before = len(gc.get_objects()), resident_kib()
    churn(iterations)
    gc.collect()
    return len(gc.get_objects()) - objects_before, resident_kib() - resident_before


def cvar_exposes_its_limits():
    minimum, maximum = 1.5, 9.5
    cvar = shinqlx.Cvar("sample", 5.5, type=float, minimum=minimum, maximum=maximum)
    referents = gc.get_referents(cvar)
    return any(r is minimum for r in referents) and any(r is maximum for r in referents)
"#,
        c"",
        c"",
    )
    .expect("could not create the gc checks module")
}

fn check(py: Python<'_>, name: &str) -> bool {
    gc_checks(py)
        .call_method0(name)
        .and_then(|result| result.extract::<bool>())
        .unwrap_or_else(|err| panic!("check {name} failed: {err}"))
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn plugin_with_database_instance_is_garbage_collected(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_database =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "database");
        let module = gc_checks(py);
        py.get_type::<Plugin>()
            .setattr(
                intern!(py, "database"),
                module
                    .getattr(intern!(py, "GcProbeDatabase"))
                    .expect("could not get the sample database"),
            )
            .expect("could not set the database type");

        assert!(
            module
                .call_method0(intern!(py, "plugin_with_database_is_collected"))
                .and_then(|result| result.extract::<bool>())
                .expect("check failed")
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn class_with_deferred_method_is_garbage_collected(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        assert!(check(py, "class_with_deferred_method_is_collected"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn deferred_call_exposes_its_function_to_the_garbage_collector(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        assert!(check(py, "deferred_call_exposes_its_function"));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn cvar_exposes_its_limits_to_the_garbage_collector(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        assert!(check(py, "cvar_exposes_its_limits"));
    });
}

const CHURN_ITERATIONS: u32 = 10_000;
const MAX_OBJECT_GROWTH: i64 = 500;
const MAX_RESIDENT_GROWTH_KIB: i64 = 2 * 1024;

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn recreating_plugins_does_not_grow_the_memory(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let _restore_database =
            RestoredAttribute::capture(py.get_type::<Plugin>().as_any(), "database");
        let module = gc_checks(py);
        py.get_type::<Plugin>()
            .setattr(
                intern!(py, "database"),
                module
                    .getattr(intern!(py, "GcProbeDatabase"))
                    .expect("could not get the sample database"),
            )
            .expect("could not set the database type");

        let (object_growth, resident_growth_kib) = module
            .call_method1(intern!(py, "memory_growth"), (CHURN_ITERATIONS,))
            .and_then(|result| result.extract::<(i64, i64)>())
            .expect("could not measure the memory growth");
        println!("objects: {object_growth}, resident: {resident_growth_kib} KiB");

        assert!(
            object_growth <= MAX_OBJECT_GROWTH,
            "{object_growth} more objects tracked by the garbage collector"
        );
        assert!(
            resident_growth_kib <= MAX_RESIDENT_GROWTH_KIB,
            "{resident_growth_kib} KiB more resident memory"
        );
    });
}
