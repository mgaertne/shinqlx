use super::prelude::*;
use crate::ffi::c::prelude::*;

/// Information about a player's state in the game.
#[pyclass(frozen)]
#[pyo3(module = "shinqlx", name = "PlayerState", get_all)]
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

#[cfg(test)]
mod player_state_tests {
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;

    #[test]
    #[serial]
    fn player_state_from_game_client() {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity.expect_get_game_client().returning(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_position()
                .returning(|| (1.0, 2.0, 3.0));
            mock_game_client
                .expect_get_velocity()
                .returning(|| (4.0, 5.0, 6.0));
            mock_game_client.expect_is_alive().returning(|| true);
            mock_game_client.expect_get_armor().returning(|| 456);
            mock_game_client.expect_get_noclip().returning(|| true);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_NAILGUN);
            mock_game_client
                .expect_get_weapons()
                .returning(|| [1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
            mock_game_client
                .expect_get_ammos()
                .returning(|| [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
            mock_game_client
                .expect_get_powerups()
                .returning(|| [12, 34, 56, 78, 90, 24]);
            mock_game_client
                .expect_get_holdable()
                .returning(|| Holdable::Kamikaze.into());
            mock_game_client
                .expect_get_current_flight_fuel()
                .returning(|| 12);
            mock_game_client
                .expect_get_max_flight_fuel()
                .returning(|| 34);
            mock_game_client.expect_get_flight_thrust().returning(|| 56);
            mock_game_client.expect_get_flight_refuel().returning(|| 78);
            mock_game_client.expect_is_chatting().returning(|| true);
            mock_game_client.expect_is_frozen().returning(|| true);
            Ok(mock_game_client)
        });
        mock_game_entity.expect_get_health().returning(|| 123);

        assert_eq!(
            PlayerState::from(mock_game_entity),
            PlayerState {
                is_alive: true,
                position: Vector3(1, 2, 3),
                velocity: Vector3(4, 5, 6),
                health: 123,
                armor: 456,
                noclip: true,
                weapon: weapon_t::WP_NAILGUN.into(),
                weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
                ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
                powerups: Powerups(12, 34, 56, 78, 90, 24),
                holdable: Some("kamikaze".into()),
                flight: Flight(12, 34, 56, 78),
                is_chatting: true,
                is_frozen: true,
            }
        );
    }

    #[test]
    fn player_state_to_str() {
        let player_state = PlayerState {
            is_alive: true,
            position: Vector3(1, 2, 3),
            velocity: Vector3(4, 5, 6),
            health: 123,
            armor: 456,
            noclip: true,
            weapon: weapon_t::WP_NAILGUN.into(),
            weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
            ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            powerups: Powerups(12, 34, 56, 78, 90, 24),
            holdable: Some("kamikaze".into()),
            flight: Flight(12, 34, 56, 78),
            is_chatting: true,
            is_frozen: true,
        };
        assert_eq!(
            player_state.__str__(),
            "PlayerState(\
            is_alive=true, \
            position=Vector3(x=1, y=2, z=3), \
            veclocity=Vector3(x=4, y=5, z=6), \
            health=123, \
            armor=456, \
            noclip=true, \
            weapon=11, \
            weapons=Weapons(g=1, mg=1, sg=1, gl=0, rl=0, lg=0, rg=0, pg=1, bfg=1, gh=0, ng=0, pl=0, cg=1, hmg=1, hands=1), \
            ammo=Weapons(g=1, mg=2, sg=3, gl=4, rl=5, lg=6, rg=6, pg=8, bfg=9, gh=10, ng=11, pl=12, cg=13, hmg=14, hands=15), \
            powerups=Powerups(quad=12, battlesuit=34, haste=56, invisibility=78, regeneration=90, invulnerability=24), \
            holdable=kamikaze, \
            flight=Flight(fuel=12, max_fuel=34, thrust=56, refuel=78), \
            is_chatting=true, \
            is_frozen=true)");
    }

    #[test]
    fn player_state_to_str_with_no_holdble() {
        let player_state = PlayerState {
            is_alive: true,
            position: Vector3(1, 2, 3),
            velocity: Vector3(4, 5, 6),
            health: 123,
            armor: 456,
            noclip: true,
            weapon: weapon_t::WP_NAILGUN.into(),
            weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
            ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            powerups: Powerups(12, 34, 56, 78, 90, 24),
            holdable: None,
            flight: Flight(12, 34, 56, 78),
            is_chatting: true,
            is_frozen: true,
        };

        assert_eq!(
                player_state.__str__(),
                "PlayerState(\
            is_alive=true, \
            position=Vector3(x=1, y=2, z=3), \
            veclocity=Vector3(x=4, y=5, z=6), \
            health=123, \
            armor=456, \
            noclip=true, \
            weapon=11, \
            weapons=Weapons(g=1, mg=1, sg=1, gl=0, rl=0, lg=0, rg=0, pg=1, bfg=1, gh=0, ng=0, pl=0, cg=1, hmg=1, hands=1), \
            ammo=Weapons(g=1, mg=2, sg=3, gl=4, rl=5, lg=6, rg=6, pg=8, bfg=9, gh=10, ng=11, pl=12, cg=13, hmg=14, hands=15), \
            powerups=Powerups(quad=12, battlesuit=34, haste=56, invisibility=78, regeneration=90, invulnerability=24), \
            holdable=None, \
            flight=Flight(fuel=12, max_fuel=34, thrust=56, refuel=78), \
            is_chatting=true, \
            is_frozen=true)");
    }

    #[test]
    fn player_state_repr() {
        let player_state = PlayerState {
            is_alive: true,
            position: Vector3(1, 2, 3),
            velocity: Vector3(4, 5, 6),
            health: 123,
            armor: 456,
            noclip: true,
            weapon: weapon_t::WP_NAILGUN.into(),
            weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
            ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            powerups: Powerups(12, 34, 56, 78, 90, 24),
            holdable: Some("kamikaze".into()),
            flight: Flight(12, 34, 56, 78),
            is_chatting: true,
            is_frozen: true,
        };

        assert_eq!(
                player_state.__repr__(),
                "PlayerState(\
            is_alive=true, \
            position=Vector3(x=1, y=2, z=3), \
            veclocity=Vector3(x=4, y=5, z=6), \
            health=123, \
            armor=456, \
            noclip=true, \
            weapon=11, \
            weapons=Weapons(g=1, mg=1, sg=1, gl=0, rl=0, lg=0, rg=0, pg=1, bfg=1, gh=0, ng=0, pl=0, cg=1, hmg=1, hands=1), \
            ammo=Weapons(g=1, mg=2, sg=3, gl=4, rl=5, lg=6, rg=6, pg=8, bfg=9, gh=10, ng=11, pl=12, cg=13, hmg=14, hands=15), \
            powerups=Powerups(quad=12, battlesuit=34, haste=56, invisibility=78, regeneration=90, invulnerability=24), \
            holdable=kamikaze, \
            flight=Flight(fuel=12, max_fuel=34, thrust=56, refuel=78), \
            is_chatting=true, \
            is_frozen=true)");
    }

    #[test]
    fn player_state_repr_with_no_holdable() {
        let player_state = PlayerState {
            is_alive: true,
            position: Vector3(1, 2, 3),
            velocity: Vector3(4, 5, 6),
            health: 123,
            armor: 456,
            noclip: true,
            weapon: weapon_t::WP_NAILGUN.into(),
            weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
            ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            powerups: Powerups(12, 34, 56, 78, 90, 24),
            holdable: None,
            flight: Flight(12, 34, 56, 78),
            is_chatting: true,
            is_frozen: true,
        };

        assert_eq!(
                player_state.__repr__(),
                "PlayerState(\
            is_alive=true, \
            position=Vector3(x=1, y=2, z=3), \
            veclocity=Vector3(x=4, y=5, z=6), \
            health=123, \
            armor=456, \
            noclip=true, \
            weapon=11, \
            weapons=Weapons(g=1, mg=1, sg=1, gl=0, rl=0, lg=0, rg=0, pg=1, bfg=1, gh=0, ng=0, pl=0, cg=1, hmg=1, hands=1), \
            ammo=Weapons(g=1, mg=2, sg=3, gl=4, rl=5, lg=6, rg=6, pg=8, bfg=9, gh=10, ng=11, pl=12, cg=13, hmg=14, hands=15), \
            powerups=Powerups(quad=12, battlesuit=34, haste=56, invisibility=78, regeneration=90, invulnerability=24), \
            holdable=None, \
            flight=Flight(fuel=12, max_fuel=34, thrust=56, refuel=78), \
            is_chatting=true, \
            is_frozen=true)");
    }
}
