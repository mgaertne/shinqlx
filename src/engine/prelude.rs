//! The handles and C types of the engine; in tests the handles are their mocks.

#[cfg(not(test))]
pub(crate) use crate::engine::qagame::activator::Activator;
#[cfg(test)]
pub(crate) use crate::engine::qagame::activator::{MockActivator as Activator, MockActivator};
#[cfg(not(test))]
pub(crate) use crate::engine::qagame::current_level::CurrentLevel;
#[cfg(test)]
pub(crate) use crate::engine::qagame::current_level::{
    MockCurrentLevel as CurrentLevel, MockCurrentLevel,
};
#[cfg(not(test))]
pub(crate) use crate::engine::qagame::game_client::GameClient;
#[cfg(test)]
pub(crate) use crate::engine::qagame::game_client::{MockGameClient as GameClient, MockGameClient};
#[cfg(not(test))]
pub(crate) use crate::engine::qagame::game_entity::GameEntity;
#[cfg(test)]
pub(crate) use crate::engine::qagame::game_entity::{
    MockGameEntity as GameEntity, MockGameEntity, MockGameEntityBuilder,
};
#[cfg(not(test))]
pub(crate) use crate::engine::qagame::game_item::GameItem;
#[cfg(test)]
pub(crate) use crate::engine::qagame::game_item::{MockGameItem as GameItem, MockGameItem};
#[cfg(not(test))]
pub(crate) use crate::engine::qzeroded::client::Client;
#[cfg(test)]
pub(crate) use crate::engine::qzeroded::client::{MockClient as Client, MockClient};
#[cfg(not(test))]
pub(crate) use crate::engine::qzeroded::server_static::ServerStatic;
#[cfg(test)]
pub(crate) use crate::engine::qzeroded::server_static::{
    MockServerStatic as ServerStatic, MockServerStatic,
};
pub(crate) use crate::engine::{qzeroded::cvar::CVar, types::*};
