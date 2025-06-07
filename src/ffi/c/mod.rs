mod activator;
mod client;
mod current_level;
mod cvar;
mod game_client;
pub(crate) mod game_entity;
pub(crate) mod game_item;
mod quake_types;
mod server_static;

pub(crate) mod prelude {
    #[cfg(not(test))]
    pub(crate) use super::activator::Activator;
    #[cfg(test)]
    pub(crate) use super::activator::{MockActivator as Activator, MockActivator};
    #[cfg(not(test))]
    pub(crate) use super::client::Client;
    #[cfg(test)]
    pub(crate) use super::client::{MockClient as Client, MockClient};
    #[cfg(not(test))]
    pub(crate) use super::current_level::CurrentLevel;
    #[cfg(test)]
    pub(crate) use super::current_level::{MockCurrentLevel as CurrentLevel, MockCurrentLevel};
    #[cfg(not(test))]
    pub(crate) use super::game_client::GameClient;
    #[cfg(test)]
    pub(crate) use super::game_client::{MockGameClient as GameClient, MockGameClient};
    #[cfg(not(test))]
    pub(crate) use super::game_entity::GameEntity;
    #[cfg(test)]
    pub(crate) use super::game_entity::{
        MockGameEntity as GameEntity, MockGameEntity, MockGameEntityBuilder,
    };
    #[cfg(not(test))]
    pub(crate) use super::game_item::GameItem;
    #[cfg(test)]
    pub(crate) use super::game_item::{MockGameItem as GameItem, MockGameItem};
    #[cfg(not(test))]
    pub(crate) use super::server_static::ServerStatic;
    #[cfg(test)]
    pub(crate) use super::server_static::{MockServerStatic as ServerStatic, MockServerStatic};
    pub(crate) use super::{cvar::CVar, quake_types::*};
}
