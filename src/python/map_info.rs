//! The Python classes for what the scan of the installed maps found.
//!
//! They only hold plain Rust data, so there are no references to Python objects to take care of for the garbage
//! collector; the tuples and mappings they return are made on access.

use pyo3::{
    PyClass, intern,
    pyclass_init::PyClassInitializer,
    types::{PyDict, PyString, PyTuple},
};

use super::{
    GameTypes,
    prelude::*,
    string_enums::{Factories, MapSourceKind},
};
use crate::maps::model::{self, SourceKind};

/// A value as Python writes it, `None` for a missing one.
fn repr_option<T: core::fmt::Debug>(value: Option<&T>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| format!("{value:?}"))
}

fn kind_name(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Base => MapSourceKind::Base,
        SourceKind::Home => MapSourceKind::Home,
        SourceKind::Workshop => MapSourceKind::Workshop,
    }
    .as_str()
}

/// The member of the Python enumeration `class` of the module `shinqlx` for `value`.
fn enum_member<'py>(py: Python<'py>, class: &str, value: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(intern!(py, "shinqlx"))?
        .getattr(class)?
        .call1((value,))
}

/// The member of `class` for `value` if `known`, otherwise `value` as the plain string the file has.
fn enum_or_text<'py>(
    py: Python<'py>,
    class: &str,
    known: bool,
    value: &str,
) -> PyResult<Bound<'py, PyAny>> {
    if known {
        enum_member(py, class, value)
    } else {
        Ok(PyString::new(py, value).into_any())
    }
}

fn gametype_members<'py>(
    py: Python<'py>,
    gametypes: &[GameTypes],
) -> PyResult<Bound<'py, PyTuple>> {
    let members = gametypes
        .iter()
        .map(|gametype| enum_member(py, "GameTypes", gametype.type_short()))
        .collect::<PyResult<Vec<_>>>()?;
    PyTuple::new(py, members)
}

fn read_only_mapping<'py, 'a>(
    py: Python<'py>,
    entries: impl IntoIterator<Item = (&'a String, &'a String)>,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    entries
        .into_iter()
        .try_for_each(|(key, value)| dict.set_item(key, value))?;
    py.import(intern!(py, "types"))?
        .getattr(intern!(py, "MappingProxyType"))?
        .call1((dict,))
}

fn tuple_of<'py, T: PyClass + Into<PyClassInitializer<T>>>(
    py: Python<'py>,
    items: impl IntoIterator<Item = T>,
) -> PyResult<Bound<'py, PyTuple>> {
    let items = items
        .into_iter()
        .map(|item| Bound::new(py, item))
        .collect::<PyResult<Vec<_>>>()?;
    PyTuple::new(py, items)
}

/// The archive or directory a file of the map information comes from; the full path is only in the log.
#[pyclass(module = "_shinqlx", name = "MapSource", frozen, eq)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapSource {
    pub(crate) kind: String,
    pub(crate) pk3_path: String,
    pub(crate) workshop_id: Option<u64>,
    pub(crate) has_aas: bool,
    pub(crate) has_levelshot: bool,
    pub(crate) has_preview: bool,
}

#[pymethods]
impl MapSource {
    /// Where it lies, a member of `MapSourceKind`.
    #[getter]
    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        enum_member(py, "MapSourceKind", &self.kind)
    }

    /// The archive as `"pak00.pk3"`, or the directory as `"baseq3"`; for a workshop item the id and the file
    /// name as `"123456789/map-charon3dm12.pk3"`. The directories above it are left out on purpose.
    #[getter]
    fn pk3_path(&self) -> &str {
        &self.pk3_path
    }

    /// The id of the workshop item for files that come from one.
    #[getter]
    fn workshop_id(&self) -> Option<u64> {
        self.workshop_id
    }

    /// Whether the bot file `maps/<name>.aas` the game uses comes from here.
    #[getter]
    fn has_aas(&self) -> bool {
        self.has_aas
    }

    /// Whether the levelshot picture of the map comes from here.
    #[getter]
    fn has_levelshot(&self) -> bool {
        self.has_levelshot
    }

    /// Whether the picture for the map-end vote comes from here.
    #[getter]
    fn has_preview(&self) -> bool {
        self.has_preview
    }

    fn __repr__(&self) -> String {
        format!(
            "MapSource(kind={:?}, pk3_path={:?}, workshop_id={})",
            self.kind,
            self.pk3_path,
            repr_option(self.workshop_id.as_ref())
        )
    }
}

impl From<&model::MapSource> for MapSource {
    fn from(source: &model::MapSource) -> Self {
        Self {
            kind: kind_name(source.kind).to_owned(),
            pk3_path: source.workshop_id.map_or_else(
                || source.file_name.clone(),
                |id| format!("{id}/{}", source.file_name),
            ),
            has_aas: source.has_aas,
            has_levelshot: source.has_levelshot,
            has_preview: source.has_preview,
            workshop_id: source.workshop_id,
        }
    }
}

/// One arena block of a map, as a `.arena` file or `arenas.txt` has it.
#[pyclass(module = "_shinqlx", name = "ArenaInfo", frozen, eq)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArenaInfo(pub(crate) model::ArenaInfo);

#[pymethods]
impl ArenaInfo {
    /// The short name of the map the block is for, as the block spells it.
    #[getter]
    fn map(&self) -> &str {
        &self.0.map
    }

    #[getter]
    fn longname(&self) -> Option<&str> {
        self.0.longname.as_deref()
    }

    #[getter]
    fn author(&self) -> Option<&str> {
        self.0.author.as_deref()
    }

    #[getter]
    fn author2(&self) -> Option<&str> {
        self.0.author2.as_deref()
    }

    /// The game types the block names that the game understands, as members of `GameTypes`.
    #[getter]
    fn gametypes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        gametype_members(py, &self.0.gametypes)
    }

    /// All tokens of the `type` value as written, including the ones the game ignores.
    #[getter]
    fn type_tokens<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, &self.0.type_tokens)
    }

    #[getter]
    fn fraglimit(&self) -> Option<i32> {
        self.0.fraglimit
    }

    #[getter]
    fn timelimit(&self) -> Option<i32> {
        self.0.timelimit
    }

    #[getter]
    fn capturelimit(&self) -> Option<i32> {
        self.0.capturelimit
    }

    /// Every key of the block as written, as a read-only mapping.
    #[getter]
    fn raw<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        read_only_mapping(py, &self.0.raw)
    }

    /// The file the block comes from, if known.
    #[getter]
    fn source(&self) -> Option<MapSource> {
        self.0.source.as_ref().map(MapSource::from)
    }

    fn __repr__(&self) -> String {
        format!(
            "ArenaInfo(map={:?}, longname={})",
            self.0.map,
            repr_option(self.0.longname.as_ref())
        )
    }
}

/// Everything known about an installed map.
#[pyclass(module = "_shinqlx", name = "MapInfo", frozen, eq)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapInfo(pub(crate) model::MapInfo);

#[pymethods]
impl MapInfo {
    /// The short name of the map, as the archive spells it; the game finds it ignoring case.
    #[getter]
    fn name(&self) -> &str {
        &self.0.name
    }

    /// The long name; the short name if nothing names the map otherwise. It can contain colour codes.
    #[getter]
    fn longname(&self) -> &str {
        &self.0.longname
    }

    #[getter]
    fn author(&self) -> Option<&str> {
        self.0.author.as_deref()
    }

    #[getter]
    fn author2(&self) -> Option<&str> {
        self.0.author2.as_deref()
    }

    /// The game types the map runs in, as members of `GameTypes`; every game type for a map without an
    /// arena block.
    #[getter]
    fn gametypes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        gametype_members(py, self.0.gametypes())
    }

    /// All tokens of the `type` value of the deciding arena block as written, including the ones the game ignores.
    #[getter]
    fn type_tokens<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.0.type_tokens())
    }

    /// All arena blocks for the map, the most important first; empty for a map without one, which runs in every
    /// game type.
    #[getter]
    fn arenas<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        tuple_of(py, self.0.arenas.iter().cloned().map(ArenaInfo))
    }

    /// The archives that contain the map, the most important first.
    #[getter]
    fn sources<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        tuple_of(py, self.0.sources.iter().map(MapSource::from))
    }

    /// Whether a levelshot picture exists for the map.
    #[getter]
    fn has_levelshot(&self) -> bool {
        self.0.has_levelshot
    }

    /// Whether a picture for the map-end vote exists for the map.
    #[getter]
    fn has_preview(&self) -> bool {
        self.0.has_preview
    }

    /// Whether the bot file `maps/<name>.aas` exists; without it the map has no bot support.
    #[getter]
    fn has_aas(&self) -> bool {
        self.0.has_aas
    }

    fn __repr__(&self) -> String {
        format!(
            "MapInfo(name={:?}, longname={:?})",
            self.0.name, self.0.longname
        )
    }
}

/// A factory of a `.factories` file: a game type with changed settings.
#[pyclass(module = "_shinqlx", name = "FactoryInfo", frozen, eq)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactoryInfo(pub(crate) model::FactoryInfo);

#[pymethods]
impl FactoryInfo {
    /// The id that `map <name> <id>` takes: a member of `Factories` for a factory of the base game, otherwise the
    /// text of the file.
    #[getter]
    fn id<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let id = self.0.factory.id.as_str();
        let known = Factories::MEMBERS.iter().any(|(_, value)| *value == id);
        enum_or_text(py, "Factories", known, id)
    }

    #[getter]
    fn title(&self) -> Option<&str> {
        self.0.factory.title.as_deref()
    }

    #[getter]
    fn author(&self) -> Option<&str> {
        self.0.factory.author.as_deref()
    }

    #[getter]
    fn description(&self) -> Option<&str> {
        self.0.factory.description.as_deref()
    }

    /// The game type the factory is based on: a member of `GameTypes` if the game knows it, otherwise the text of
    /// the file.
    #[getter]
    fn basegt<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.0
            .factory
            .basegt
            .as_deref()
            .map(|basegt| {
                let known = GameTypes::ALL
                    .iter()
                    .any(|gametype| gametype.type_short() == basegt);
                enum_or_text(py, "GameTypes", known, basegt)
            })
            .transpose()
    }

    /// The cvars the factory sets, as a read-only mapping of strings.
    #[getter]
    fn cvars<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        read_only_mapping(py, &self.0.factory.cvars)
    }

    /// The file the factory comes from, if known.
    #[getter]
    fn source(&self) -> Option<MapSource> {
        self.0.source.as_ref().map(MapSource::from)
    }

    fn __repr__(&self) -> String {
        format!(
            "FactoryInfo(id={:?}, title={})",
            self.0.factory.id,
            repr_option(self.0.factory.title.as_ref())
        )
    }
}

#[cfg(test)]
mod map_info_tests {
    use pretty_assertions::assert_eq;
    use pyo3::{intern, prelude::*, types::PyTuple};
    use rstest::rstest;
    use serial_test::serial;

    use super::*;
    use crate::{
        maps::{
            block::parse_blocks,
            factory::Factory,
            model::{ArenaInfo as CoreArena, MapInfo as CoreMap, WorldSpawn},
        },
        python::pyshinqlx_setup_fixture::*,
    };

    fn source() -> model::MapSource {
        model::MapSource {
            kind: SourceKind::Workshop,
            file_name: "map-x.pk3".to_owned(),
            has_aas: true,
            has_levelshot: false,
            has_preview: false,
            workshop_id: Some(42),
        }
    }

    fn map_info() -> MapInfo {
        let arena = CoreArena::from_block(
            &parse_blocks(r#"{ map x longname "X Arena" type "ca ffa" fraglimit 30 bots "a b" }"#)
                .blocks[0],
            Some(source()),
        )
        .expect("a block with a map");
        MapInfo(CoreMap::assemble(
            "X",
            Some(WorldSpawn {
                message: Some("^1X".to_owned()),
                author: None,
                author2: None,
            }),
            vec![arena],
            vec![source()],
        ))
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn every_getter_reports_what_the_core_holds(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let map = Bound::new(py, map_info()).expect("could not create the map info");
            let text = |object: &Bound<'_, PyAny>, name: &str| {
                object
                    .getattr(name)
                    .and_then(|value| value.extract::<Option<String>>())
                    .expect(name)
            };
            let flag = |object: &Bound<'_, PyAny>, name: &str| {
                object
                    .getattr(name)
                    .and_then(|value| value.extract::<bool>())
                    .expect(name)
            };

            assert_eq!(text(&map, "name").as_deref(), Some("X"));
            assert_eq!(text(&map, "author"), None);
            assert_eq!(text(&map, "author2"), None);
            assert!(!flag(&map, "has_levelshot"));
            assert!(!flag(&map, "has_preview"));
            assert!(!flag(&map, "has_aas"));
            assert_eq!(
                map.getattr("type_tokens")
                    .and_then(|tokens| tokens.extract::<Vec<String>>())
                    .expect("type_tokens"),
                vec!["ca", "ffa"]
            );

            let arena = map
                .getattr("arenas")
                .and_then(|arenas| arenas.get_item(0))
                .expect("an arena");
            assert_eq!(text(&arena, "map").as_deref(), Some("x"));
            assert_eq!(text(&arena, "longname").as_deref(), Some("X Arena"));
            assert_eq!(text(&arena, "author"), None);
            assert_eq!(text(&arena, "author2"), None);
            assert_eq!(
                arena
                    .getattr("fraglimit")
                    .and_then(|value| value.extract::<Option<i32>>())
                    .expect("fraglimit"),
                Some(30)
            );
            assert_eq!(
                arena
                    .getattr("timelimit")
                    .and_then(|value| value.extract::<Option<i32>>())
                    .expect("timelimit"),
                None
            );
            assert_eq!(
                arena
                    .getattr("capturelimit")
                    .and_then(|value| value.extract::<Option<i32>>())
                    .expect("capturelimit"),
                None
            );
            assert_eq!(
                arena
                    .getattr("raw")
                    .and_then(|raw| raw.get_item("bots"))
                    .and_then(|value| value.extract::<String>())
                    .expect("raw"),
                "a b"
            );
            assert!(
                arena
                    .getattr("type_tokens")
                    .and_then(|tokens| tokens.extract::<Vec<String>>())
                    .is_ok_and(|tokens| tokens == ["ca", "ffa"])
            );

            let arena_source = arena.getattr("source").expect("source");
            assert_eq!(
                text(&arena_source, "pk3_path").as_deref(),
                Some("42/map-x.pk3")
            );
            assert!(flag(&arena_source, "has_aas"));
            assert!(!flag(&arena_source, "has_levelshot"));
            assert!(!flag(&arena_source, "has_preview"));
            assert_eq!(
                arena_source
                    .getattr("workshop_id")
                    .and_then(|id| id.extract::<Option<u64>>())
                    .expect("workshop_id"),
                Some(42)
            );
            assert!(
                arena_source
                    .getattr("kind")
                    .and_then(|kind| kind.extract::<String>())
                    .is_ok_and(|kind| kind == "workshop")
            );

            let factory = Bound::new(
                py,
                FactoryInfo(model::FactoryInfo {
                    factory: Factory {
                        id: "ca".to_owned(),
                        title: Some("Clan Arena".to_owned()),
                        author: Some("a".to_owned()),
                        description: Some("d".to_owned()),
                        basegt: None,
                        cvars: Default::default(),
                    },
                    source: Some(source()),
                }),
            )
            .expect("could not create the factory info");
            assert_eq!(text(&factory, "title").as_deref(), Some("Clan Arena"));
            assert_eq!(text(&factory, "author").as_deref(), Some("a"));
            assert_eq!(text(&factory, "description").as_deref(), Some("d"));
            assert_eq!(text(&factory, "basegt"), None);
            assert_eq!(
                text(&factory.getattr("source").expect("source"), "pk3_path").as_deref(),
                Some("42/map-x.pk3")
            );
            assert!(format!("{:?}", factory.repr().expect("repr")).contains("FactoryInfo"));
            assert!(format!("{:?}", map.repr().expect("repr")).contains("MapInfo"));
            assert!(format!("{:?}", arena.repr().expect("repr")).contains("ArenaInfo"));
        });
    }

    #[test]
    fn source_reports_the_kind_as_the_string_of_a_member() {
        let source = MapSource::from(&source());

        assert_eq!(source.kind, "workshop");
        assert_eq!(source.workshop_id, Some(42));
        assert_eq!(
            source.__repr__(),
            r#"MapSource(kind="workshop", pk3_path="42/map-x.pk3", workshop_id=42)"#
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn map_info_returns_tuples_and_read_only_mappings(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let map = Bound::new(py, map_info()).expect("could not create the map info");

            let gametypes = map.getattr(intern!(py, "gametypes")).expect("gametypes");
            assert!(gametypes.cast::<PyTuple>().is_ok());
            assert_eq!(
                gametypes.extract::<Vec<String>>().expect("strings"),
                vec!["ca".to_owned(), "ffa".to_owned()]
            );
            let game_types_class = py
                .import("shinqlx")
                .and_then(|module| module.getattr("GameTypes"))
                .expect("GameTypes");
            assert!(gametypes.try_iter().expect("iterable").all(|member| {
                member
                    .and_then(|member| member.is_instance(&game_types_class))
                    .expect("isinstance")
            }));
            assert_eq!(
                map.getattr("longname")
                    .and_then(|v| v.extract::<String>())
                    .expect("longname"),
                "^1X"
            );

            let arena = map
                .getattr(intern!(py, "arenas"))
                .and_then(|arenas| arenas.get_item(0))
                .expect("an arena");
            let raw = arena.getattr(intern!(py, "raw")).expect("raw");
            assert_eq!(
                raw.get_item("bots")
                    .and_then(|v| v.extract::<String>())
                    .expect("bots"),
                "a b"
            );
            assert!(raw.set_item("x", "y").is_err());
            assert_eq!(
                arena
                    .getattr("fraglimit")
                    .and_then(|v| v.extract::<i32>())
                    .expect("fraglimit"),
                30
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn factory_info_keeps_the_cvars_read_only(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let factory = Bound::new(
                py,
                FactoryInfo(model::FactoryInfo {
                    factory: Factory {
                        id: "ca2".to_owned(),
                        title: Some("CA 2".to_owned()),
                        author: None,
                        description: None,
                        basegt: Some("ca".to_owned()),
                        cvars: [("g_roundlimit".to_owned(), "5".to_owned())].into(),
                    },
                    source: None,
                }),
            )
            .expect("could not create the factory info");

            let cvars = factory.getattr(intern!(py, "cvars")).expect("cvars");
            assert_eq!(
                cvars
                    .get_item("g_roundlimit")
                    .and_then(|v| v.extract::<String>())
                    .expect("a value"),
                "5"
            );
            assert!(cvars.set_item("a", "b").is_err());
            assert!(factory.setattr("id", "x").is_err());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn factory_ids_of_the_base_game_are_members_of_factories(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let info = |id: &str, basegt: &str| {
                Bound::new(
                    py,
                    FactoryInfo(model::FactoryInfo {
                        factory: Factory {
                            id: id.to_owned(),
                            title: None,
                            author: None,
                            description: None,
                            basegt: Some(basegt.to_owned()),
                            cvars: Default::default(),
                        },
                        source: None,
                    }),
                )
                .expect("could not create the factory info")
            };
            let module = py.import("shinqlx").expect("the module");
            let is_member = |value: Bound<'_, PyAny>, class: &str| {
                let class = module.getattr(class).expect("the class");
                value.is_instance(&class).expect("isinstance")
            };

            let ca = info("ca", "ca");
            assert!(is_member(ca.getattr("id").expect("id"), "Factories"));
            assert!(is_member(
                ca.getattr("basegt").expect("basegt"),
                "GameTypes"
            ));

            let own = info("my_ca", "my_gametype");
            let own_id = own.getattr("id").expect("id");
            assert!(!is_member(own_id.clone(), "Factories"));
            assert_eq!(own_id.extract::<String>().expect("a string"), "my_ca");
            let own_base = own.getattr("basegt").expect("basegt");
            assert!(!is_member(own_base, "GameTypes"));
        });
    }
}
