use core::hint::cold_path;

use pyo3::{exceptions::PyValueError, types::PyDict};
use tap::TryConv;

use crate::ffi::{
    c::{game_entity::entity_id_limit, prelude::*},
    python::{entity_classes, prelude::*},
};

/// Snapshots of the entities of the game.
///
/// Only entities in use are listed unless `in_use` is false; `etype` limits the list to one type of
/// `EntityTypes`.
#[pyfunction]
#[pyo3(name = "entities", signature = (in_use = true, etype = None), text_signature = "(in_use=True, etype=None)")]
pub(crate) fn pyshinqlx_entities(
    py: Python<'_>,
    in_use: bool,
    etype: Option<&str>,
) -> Vec<EntityInfo> {
    py.detach(|| {
        (0..entity_id_limit())
            .filter_map(|entity_id| (entity_id as i32).try_conv::<GameEntity>().ok())
            .filter(|game_entity| !in_use || game_entity.in_use())
            .map(|game_entity| EntityInfo::from(&game_entity))
            .filter(|info| etype.is_none_or(|etype| info.etype == etype))
            .collect()
    })
}

/// Frees an entity of the game, e.g. an item or a trigger; the slots of the players cannot be freed.
///
/// Returns whether an entity in use was freed.
#[pyfunction]
#[pyo3(name = "remove_entity")]
pub(crate) fn pyshinqlx_remove_entity(py: Python<'_>, entity_id: i32) -> PyResult<bool> {
    if !(MAX_CLIENTS as i32..MAX_GENTITIES as i32).contains(&entity_id) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "entity_id need to be between {} and {}.",
            MAX_CLIENTS,
            MAX_GENTITIES - 1
        )));
    }

    py.detach(|| {
        Ok(entity_id
            .try_conv::<GameEntity>()
            .ok()
            .filter(|game_entity| game_entity.in_use())
            .is_some_and(|mut game_entity| {
                game_entity.free_entity();
                true
            }))
    })
}

/// Puts an entity of the game at a standstill at a new position, e.g. an item or a trigger.
///
/// Returns whether an entity in use was moved and the engine was told about it.
#[pyfunction]
#[pyo3(name = "move_entity")]
pub(crate) fn pyshinqlx_move_entity(
    py: Python<'_>,
    entity_id: i32,
    x: f32,
    y: f32,
    z: f32,
) -> PyResult<bool> {
    if !(MAX_CLIENTS as i32..MAX_GENTITIES as i32).contains(&entity_id) {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "entity_id need to be between {} and {}.",
            MAX_CLIENTS,
            MAX_GENTITIES - 1
        )));
    }
    if ![x, y, z].iter().all(|coordinate| coordinate.is_finite()) {
        cold_path();
        return Err(PyValueError::new_err("the position needs finite numbers."));
    }

    py.detach(|| {
        Ok(entity_id
            .try_conv::<GameEntity>()
            .ok()
            .filter(|game_entity| game_entity.in_use())
            .is_some_and(|mut game_entity| {
                game_entity.set_origin([x, y, z]);
                game_entity.link()
            }))
    })
}

fn entity_ids_in_use() -> Vec<i32> {
    (MAX_CLIENTS as i32..MAX_GENTITIES as i32)
        .filter(|&entity_id| {
            entity_id
                .try_conv::<GameEntity>()
                .is_ok_and(|game_entity| game_entity.in_use())
        })
        .collect()
}

/// Spawns an entity of the game like the entity string of a map does.
///
/// ``keys`` are the keys of the entity as a map has them, e.g. ``{"origin": "0 0 100", "target": "door"}``; the
/// values are turned into strings. Returns the snapshot of the new entity, or ``None`` if the game did not
/// create one, e.g. because it has no spawn function for ``classname``.
///
/// For a classname of the map editor, ``strict`` (the default) refuses a key that the class does not read, e.g.
/// a typo. A classname that is not known takes any keys.
///
/// Call it from the events of the game, not from another thread.
#[pyfunction]
#[pyo3(
    name = "spawn_entity",
    signature = (classname, keys = None, *, strict = true),
    text_signature = "(classname, keys=None, *, strict=True)"
)]
pub(crate) fn pyshinqlx_spawn_entity(
    py: Python<'_>,
    classname: &str,
    keys: Option<&Bound<'_, PyDict>>,
    strict: bool,
) -> PyResult<Option<EntityInfo>> {
    if classname.is_empty() {
        cold_path();
        return Err(PyValueError::new_err("classname must not be empty."));
    }

    let class = entity_classes::find(classname).filter(|_| strict);
    let mut vars = vec![("classname".to_string(), classname.to_string())];
    for (key, value) in keys.into_iter().flat_map(|keys| keys.iter()) {
        let key = key.extract::<String>()?;
        if key == "classname" {
            cold_path();
            return Err(PyValueError::new_err(
                "the classname is the first argument, not a key.",
            ));
        }
        if class.is_some_and(|class| !entity_classes::knows_key(class, &key)) {
            cold_path();
            return Err(PyValueError::new_err(format!(
                "{classname} does not read the key {key}; strict=False passes it anyway."
            )));
        }
        vars.push((key, value.str()?.to_string()));
    }
    if vars
        .iter()
        .any(|(key, value)| key.contains('\0') || value.contains('\0'))
    {
        cold_path();
        return Err(PyValueError::new_err(
            "keys and values must not contain NUL.",
        ));
    }

    py.detach(|| {
        let before = entity_ids_in_use();
        if !GameEntity::spawn_from_vars(&vars) {
            return Ok(None);
        }

        Ok(entity_ids_in_use()
            .into_iter()
            .filter(|entity_id| !before.contains(entity_id))
            .filter_map(|entity_id| entity_id.try_conv::<GameEntity>().ok())
            .map(|game_entity| EntityInfo::from(&game_entity))
            .find(|info| info.classname == classname))
    })
}

#[cfg(test)]
mod entities_tests {
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{exceptions::PyValueError, types::PyDict};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    fn mock_entity(entity_id: i32, in_use: bool, type_id: i32) -> MockGameEntity {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity.expect_in_use().returning(move || in_use);
        mock_game_entity
            .expect_get_entity_id()
            .returning(move || entity_id);
        mock_game_entity
            .expect_get_entity_type()
            .returning(move || type_id);
        mock_game_entity
            .expect_get_origin()
            .returning(|| [1.0, 2.0, 3.0]);
        mock_game_entity
            .expect_get_classname()
            .returning(move || format!("entity {entity_id}").into());
        mock_game_entity
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn entities_lists_the_entities_in_use_with_their_type(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .returning(|entity_id| mock_entity(entity_id, entity_id == 2, 2));

        let result = Python::attach(|py| pyshinqlx_entities(py, true, None));

        assert_eq!(
            result,
            vec![EntityInfo {
                id: 2,
                classname: "entity 2".to_string(),
                etype: "item".to_string(),
                type_id: 2,
                position: Vector3(1, 2, 3),
                in_use: true,
            }]
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn entities_can_include_unused_slots_and_filter_by_type(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|entity_id| {
            mock_entity(entity_id, false, if entity_id == 7 { 4 } else { 0 })
        });

        let result = Python::attach(|py| pyshinqlx_entities(py, false, Some("mover")));

        assert_eq!(result.iter().map(|info| info.id).collect::<Vec<_>>(), [7]);
        assert!(!result[0].in_use);
    }

    #[rstest]
    #[case(-1)]
    #[case(0)]
    #[case(63)]
    #[case(MAX_GENTITIES as i32)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_entity_rejects_player_slots_and_invalid_ids(
        _pyshinqlx_setup: (),
        #[case] entity_id: i32,
    ) {
        Python::attach(|py| {
            let result = pyshinqlx_remove_entity(py, entity_id);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_entity_frees_an_entity_in_use(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(100))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity.expect_free_entity().times(1);
                mock_game_entity
            });

        let result = Python::attach(|py| pyshinqlx_remove_entity(py, 100));

        assert!(result.is_ok_and(|freed| freed));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_entity_leaves_an_unused_slot_alone(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(100))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity.expect_free_entity().never();
                mock_game_entity
            });

        let result = Python::attach(|py| pyshinqlx_remove_entity(py, 100));

        assert!(result.is_ok_and(|freed| !freed));
    }

    #[rstest]
    #[case(-1, 0.0)]
    #[case(63, 0.0)]
    #[case(MAX_GENTITIES as i32, 0.0)]
    #[case(100, f32::NAN)]
    #[case(100, f32::INFINITY)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn move_entity_rejects_player_slots_invalid_ids_and_positions(
        _pyshinqlx_setup: (),
        #[case] entity_id: i32,
        #[case] x: f32,
    ) {
        Python::attach(|py| {
            let result = pyshinqlx_move_entity(py, entity_id, x, 0.0, 0.0);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn move_entity_sets_the_origin_and_links_an_entity_in_use(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(100))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| true);
                mock_game_entity
                    .expect_set_origin()
                    .with(predicate::eq([1.0, 2.5, -3.0]))
                    .times(1)
                    .return_const(());
                mock_game_entity.expect_link().times(1).returning(|| true);
                mock_game_entity
            });

        let result = Python::attach(|py| pyshinqlx_move_entity(py, 100, 1.0, 2.5, -3.0));

        assert!(result.is_ok_and(|moved| moved));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_entity_returns_the_new_entity(_pyshinqlx_setup: ()) {
        let spawned = alloc::sync::Arc::new(core::sync::atomic::AtomicBool::new(false));

        let game_entity_from_ctx = MockGameEntity::from_context();
        let in_use_after_spawn = spawned.clone();
        game_entity_from_ctx.expect().returning(move |entity_id| {
            let spawned = in_use_after_spawn.load(core::sync::atomic::Ordering::SeqCst);
            mock_entity(entity_id, spawned && entity_id == 200, 4)
        });
        let spawn_ctx = MockGameEntity::spawn_from_vars_context();
        spawn_ctx
            .expect()
            .withf(|vars| {
                vars == [
                    ("classname".to_string(), "entity 200".to_string()),
                    ("wait".to_string(), "2".to_string()),
                ]
            })
            .times(1)
            .returning(move |_| {
                spawned.store(true, core::sync::atomic::Ordering::SeqCst);
                true
            });

        let result = Python::attach(|py| {
            let keys = PyDict::new(py);
            keys.set_item("wait", 2).expect("could not set item");
            pyshinqlx_spawn_entity(py, "entity 200", Some(&keys), true)
        });

        assert!(result.is_ok_and(|info| {
            info.is_some_and(|info| info.id == 200 && info.etype == "mover")
        }));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_entity_without_a_new_entity_gives_none(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .returning(|entity_id| mock_entity(entity_id, false, 0));
        let spawn_ctx = MockGameEntity::spawn_from_vars_context();
        spawn_ctx.expect().times(1).returning(|_| true);

        let result = Python::attach(|py| pyshinqlx_spawn_entity(py, "no_such_class", None, true));

        assert!(result.is_ok_and(|info| info.is_none()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_entity_without_game_module_gives_none(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .returning(|entity_id| mock_entity(entity_id, false, 0));
        let spawn_ctx = MockGameEntity::spawn_from_vars_context();
        spawn_ctx.expect().times(1).returning(|_| false);

        let result = Python::attach(|py| pyshinqlx_spawn_entity(py, "info_null", None, true));

        assert!(result.is_ok_and(|info| info.is_none()));
    }

    #[rstest]
    #[case("", "wait", "1")]
    #[case("func_door", "classname", "other")]
    #[case("func_door", "wa\0it", "1")]
    #[case("func_door", "wait", "1\0")]
    #[case("func_door", "no_such_key", "1")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spawn_entity_rejects_invalid_input(
        _pyshinqlx_setup: (),
        #[case] classname: &str,
        #[case] key: &str,
        #[case] value: &str,
    ) {
        Python::attach(|py| {
            let keys = PyDict::new(py);
            keys.set_item(key, value).expect("could not set item");
            let result = pyshinqlx_spawn_entity(py, classname, Some(&keys), true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn move_entity_leaves_an_unused_slot_alone(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(100))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_in_use().returning(|| false);
                mock_game_entity.expect_set_origin().never();
                mock_game_entity.expect_link().never();
                mock_game_entity
            });

        let result = Python::attach(|py| pyshinqlx_move_entity(py, 100, 1.0, 2.0, 3.0));

        assert!(result.is_ok_and(|moved| !moved));
    }
}
