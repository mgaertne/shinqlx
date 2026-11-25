use derive_more::Display;

use super::{prelude::*, string_enums::EntityType};
use crate::ffi::c::prelude::*;

/// A snapshot of an entity of the game; it does not change when the entity does.
#[pyclass(
    module = "_shinqlx",
    name = "EntityInfo",
    frozen,
    get_all,
    str,
    from_py_object
)]
#[derive(Debug, PartialEq, Clone, Display)]
#[display(
    "EntityInfo(id={id}, classname={classname}, etype={etype}, type_id={type_id}, position={position}, in_use={in_use})"
)]
pub(crate) struct EntityInfo {
    /// The number of the entity.
    pub(crate) id: i32,
    /// The classname of the entity, e.g. `"item_armor_body"`.
    pub(crate) classname: String,
    /// The type of the entity, the string of a member of `EntityTypes`.
    pub(crate) etype: String,
    /// The number of the type in the engine, which for events is `ET_EVENTS` plus the number of the event.
    pub(crate) type_id: i32,
    /// The position of the entity.
    pub(crate) position: Vector3,
    /// Whether the entity is in use, unused slots of the entity list are free.
    pub(crate) in_use: bool,
}

#[pymethods]
impl EntityInfo {
    fn __repr__(&self) -> String {
        format!("{self}")
    }
}

impl From<&GameEntity> for EntityInfo {
    fn from(game_entity: &GameEntity) -> Self {
        let type_id = game_entity.get_entity_type();
        let [x, y, z] = game_entity.get_origin();
        Self {
            id: game_entity.get_entity_id(),
            classname: game_entity.get_classname().into_owned(),
            etype: EntityType::from(type_id).as_str().to_string(),
            type_id,
            position: Vector3::from((x, y, z)),
            in_use: game_entity.in_use(),
        }
    }
}
