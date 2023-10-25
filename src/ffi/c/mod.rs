pub(crate) mod activator;
pub(crate) mod client;
pub(crate) mod current_level;
pub(crate) mod cvar;
pub(crate) mod game_client;
pub(crate) mod game_entity;
pub(crate) mod game_item;
pub(crate) mod quake_types;
pub(crate) mod server_static;

#[cfg(not(test))]
pub(crate) use activator::Activator;
#[cfg(test)]
pub(crate) use activator::MockActivator as Activator;
#[cfg(not(test))]
pub(crate) use client::Client;
#[cfg(test)]
pub(crate) use client::MockClient as Client;
#[cfg(not(test))]
pub(crate) use current_level::CurrentLevel;
#[cfg(test)]
pub(crate) use current_level::MockTestCurrentLevel as CurrentLevel;
pub(crate) use cvar::CVar;
#[cfg(not(test))]
pub(crate) use game_client::GameClient;
#[cfg(test)]
pub(crate) use game_client::MockGameClient as GameClient;
#[cfg(not(test))]
pub(crate) use game_entity::GameEntity;
#[cfg(test)]
pub(crate) use game_entity::MockGameEntity as GameEntity;
#[cfg(not(test))]
pub(crate) use game_item::GameItem;
#[cfg(test)]
pub(crate) use game_item::MockGameItem as GameItem;
#[cfg(test)]
pub(crate) use server_static::MockTestServerStatic as ServerStatic;
#[cfg(not(test))]
pub(crate) use server_static::ServerStatic;
