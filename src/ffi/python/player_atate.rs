use super::Flight;
use super::Holdable;
use super::Powerups;
use super::Vector3;
use super::Weapons;
use crate::prelude::*;
use pyo3::prelude::*;

/// Information about a player's state in the game.
#[pyclass]
#[pyo3(module = "minqlx", name = "PlayerState", get_all)]
#[derive(Debug, PartialEq)]
pub(crate) struct PlayerState {
    /// Whether the player's alive or not.
    pub(crate) is_alive: bool,
    /// The player's position.
    pub(crate) position: Vector3,
    /// The player's velocity.
    pub(crate) velocity: Vector3,
    /// The player's health.
    pub(crate) health: i32,
    /// The player's armor.
    pub(crate) armor: i32,
    /// Whether the player has noclip or not.
    pub(crate) noclip: bool,
    /// The weapon the player is currently using.
    pub(crate) weapon: i32,
    /// The player's weapons.
    pub(crate) weapons: Weapons,
    /// The player's weapon ammo.
    pub(crate) ammo: Weapons,
    ///The player's powerups.
    pub(crate) powerups: Powerups,
    /// The player's holdable item.
    pub(crate) holdable: Option<String>,
    /// A struct sequence with flight parameters.
    pub(crate) flight: Flight,
    /// Whether the player is currently chatting.
    pub(crate) is_chatting: bool,
    /// Whether the player is frozen(freezetag).
    pub(crate) is_frozen: bool,
}

#[pymethods]
impl PlayerState {
    fn __str__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
                self.is_alive,
                self.position.__str__(),
                self.velocity.__str__(),
                self.health,
                self.armor,
                self.noclip,
                self.weapon,
                self.weapons.__str__(),
                self.ammo.__str__(),
                self.powerups.__str__(),
                match self.holdable.as_ref() {
                    Some(value) => value,
                    None => "None",
                },
                self.flight.__str__(),
                self.is_chatting,
                self.is_frozen)
    }

    fn __repr__(&self) -> String {
        format!("PlayerState(is_alive={}, position={}, veclocity={}, health={}, armor={}, noclip={}, weapon={}, weapons={}, ammo={}, powerups={}, holdable={}, flight={}, is_chatting={}, is_frozen={})",
                self.is_alive,
                self.position.__str__(),
                self.velocity.__str__(),
                self.health,
                self.armor,
                self.noclip,
                self.weapon,
                self.weapons.__str__(),
                self.ammo.__str__(),
                self.powerups.__str__(),
                match self.holdable.as_ref() {
                    Some(value) => value,
                    None => "None",
                },
                self.flight.__str__(),
                self.is_chatting,
                self.is_frozen)
    }
}

impl From<GameEntity> for PlayerState {
    fn from(game_entity: GameEntity) -> Self {
        let game_client = game_entity.get_game_client().unwrap();
        let position = game_client.get_position();
        let velocity = game_client.get_velocity();
        Self {
            is_alive: game_client.is_alive(),
            position: Vector3::from(position),
            velocity: Vector3::from(velocity),
            health: game_entity.get_health(),
            armor: game_client.get_armor(),
            noclip: game_client.get_noclip(),
            weapon: game_client.get_weapon().into(),
            weapons: Weapons::from(game_client.get_weapons()),
            ammo: Weapons::from(game_client.get_ammos()),
            powerups: Powerups::from(game_client.get_powerups()),
            holdable: Holdable::from(game_client.get_holdable()).into(),
            flight: Flight(
                game_client.get_current_flight_fuel(),
                game_client.get_max_flight_fuel(),
                game_client.get_flight_thrust(),
                game_client.get_flight_refuel(),
            ),
            is_chatting: game_client.is_chatting(),
            is_frozen: game_client.is_frozen(),
        }
    }
}
