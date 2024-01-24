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
    pub(crate) use super::activator::MockActivator as Activator;
    #[cfg(test)]
    pub(crate) use super::activator::MockActivator;

    #[cfg(not(test))]
    pub(crate) use super::client::Client;
    #[cfg(test)]
    pub(crate) use super::client::MockClient as Client;
    #[cfg(test)]
    pub(crate) use super::client::MockClient;

    #[cfg(not(test))]
    pub(crate) use super::current_level::CurrentLevel;
    #[cfg(test)]
    pub(crate) use super::current_level::MockTestCurrentLevel as CurrentLevel;
    #[cfg(test)]
    pub(crate) use super::current_level::MockTestCurrentLevel;

    pub(crate) use super::cvar::CVar;

    #[cfg(not(test))]
    pub(crate) use super::game_client::GameClient;
    #[cfg(test)]
    pub(crate) use super::game_client::MockGameClient as GameClient;
    #[cfg(test)]
    pub(crate) use super::game_client::MockGameClient;

    #[cfg(not(test))]
    pub(crate) use super::game_entity::GameEntity;
    #[cfg(test)]
    pub(crate) use super::game_entity::MockGameEntity as GameEntity;
    #[cfg(test)]
    pub(crate) use super::game_entity::MockGameEntity;

    #[cfg(not(test))]
    pub(crate) use super::game_item::GameItem;
    #[cfg(test)]
    pub(crate) use super::game_item::MockGameItem as GameItem;
    #[cfg(test)]
    pub(crate) use super::game_item::MockGameItem;

    pub(crate) use super::quake_types::*;

    #[cfg(test)]
    pub(crate) use super::server_static::MockTestServerStatic as ServerStatic;
    #[cfg(test)]
    pub(crate) use super::server_static::MockTestServerStatic;
    #[cfg(not(test))]
    pub(crate) use super::server_static::ServerStatic;
}
