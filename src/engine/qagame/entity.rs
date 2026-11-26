use core::{
    ffi::{c_char, c_int},
    hint::cold_path,
    sync::atomic::{AtomicUsize, Ordering},
};

use arc_swap::ArcSwapOption;
use detour::TypedDetour;
use tap::{Conv, TapFallible, TapOptional};

#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{
        QuakeLiveEngine, QuakeLiveEngineError,
        callbacks::EntityCallbacks,
        patterns::QuakeLiveFunction,
        prelude::*,
        qagame::{spawn_vars, stats_feed},
        symbols::function_at,
    },
    prelude::error,
};

pub(crate) trait GameAddEvent<T: AsMut<gentity_t>, U: Into<c_int>> {
    fn game_add_event(&self, game_entity: T, event: entity_event_t, event_param: U);
}

impl<T: AsMut<gentity_t>, U: Into<c_int>> GameAddEvent<T, U> for QuakeLiveEngine {
    fn game_add_event(&self, mut game_entity: T, event: entity_event_t, event_param: U) {
        let event_param_param = event_param.conv::<c_int>();
        let _ = self
            .g_addevent_orig()
            .tap_ok(|original_func| original_func(game_entity.as_mut(), event, event_param_param));
    }
}

#[cfg(test)]
mod game_add_event_quake_live_engine_tests {
    use core::sync::atomic::Ordering;

    use super::{
        GameAddEvent, QuakeLiveEngine,
        mock_quake_functions::{G_AddEvent, G_AddEvent_context},
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{GEntityBuilder, MockGameEntity, entity_event_t},
        prelude::serial,
    };

    #[test]
    fn game_add_event_with_no_original_function_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        quake_engine.game_add_event(mock_gentity, entity_event_t::EV_LIGHTNING_DISCHARGE, 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_add_event_with_valid_original_function() {
        let mut mock_game_entity = MockGameEntity::default();
        mock_game_entity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_add_event_ctx = G_AddEvent_context();
        g_add_event_ctx
            .expect()
            .withf(|&ent, &event, &parm| {
                !ent.is_null() && event == entity_event_t::EV_KAMIKAZE && parm == 42
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .entity
            .g_addevent_orig
            .store(G_AddEvent as *const () as usize, Ordering::Release);

        quake_engine.game_add_event(mock_game_entity, entity_event_t::EV_KAMIKAZE, 42);
    }
}

pub(crate) trait FireWeaponOriginal {
    /// Lets the game module fire the weapon of the entity with the original function.
    fn fire_weapon(&self, ent: *mut gentity_t);
}

impl FireWeaponOriginal for QuakeLiveEngine {
    fn fire_weapon(&self, ent: *mut gentity_t) {
        self.vm_functions
            .entity
            .fire_weapon_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(ent);
            });
    }
}

pub(crate) trait StatsMedalOriginal {
    /// Lets the game module build the `PLAYER_MEDAL` stats event with the original function.
    fn stats_medal(&self, ent: *mut gentity_t, medal: *const c_char, total: c_int);
}

impl StatsMedalOriginal for QuakeLiveEngine {
    fn stats_medal(&self, ent: *mut gentity_t, medal: *const c_char, total: c_int) {
        self.vm_functions
            .entity
            .stats_medal_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(ent, medal, total);
            });
    }
}

pub(crate) trait StatsFlagStatusOriginal {
    /// Lets the game module build the `FLAG_STATUS` stats event with the original function.
    fn stats_flag_status(&self, team: c_int, status: c_int, carrier: *mut gentity_t);
}

impl StatsFlagStatusOriginal for QuakeLiveEngine {
    fn stats_flag_status(&self, team: c_int, status: c_int, carrier: *mut gentity_t) {
        self.vm_functions
            .entity
            .stats_flag_status_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(team, status, carrier);
            });
    }
}

pub(crate) trait TouchItem {
    /// Lets the game module process the touch of the item with the original function.
    fn touch_item(&self, item: *mut gentity_t, toucher: *mut gentity_t, trace: *mut trace_t);
}

impl TouchItem for QuakeLiveEngine {
    fn touch_item(&self, item: *mut gentity_t, toucher: *mut gentity_t, trace: *mut trace_t) {
        self.vm_functions
            .item
            .touch_item_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(item, toucher, trace);
            });
    }
}

pub(crate) trait RegisterDamage<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> {
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the signature of the engine function"
    )]
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: T,
        dflags: U,
        means_of_death: V,
    );
}

impl<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> RegisterDamage<T, U, V> for QuakeLiveEngine {
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: T,
        dflags: U,
        means_of_death: V,
    ) {
        let damage_param = damage.into();
        let dflags_param = dflags.into();
        let means_of_death_param = means_of_death.into();
        self.vm_functions
            .entity
            .g_damage_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(
                    target,
                    inflictor,
                    attacker,
                    dir,
                    pos,
                    damage_param,
                    dflags_param,
                    means_of_death_param,
                );
            });
    }
}

#[cfg(test)]
mod register_damage_quake_live_engine_tests {
    use core::{ffi::c_int, ptr};

    use detour::TypedDetour;

    use super::{
        QuakeLiveEngine, RegisterDamage,
        mock_quake_functions::{G_Damage, G_Damage_context, detoured_G_Damage},
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{
            DAMAGE_NO_PROTECTION, DAMAGE_NO_TEAM_PROTECTION, gentity_t, meansOfDeath_t::*, vec3_t,
        },
        prelude::serial,
    };

    #[test]
    fn register_damage_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.register_damage(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            42,
            DAMAGE_NO_PROTECTION as c_int,
            MOD_LIGHTNING_DISCHARGE as c_int,
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_damage_with_valid_detour_function() {
        let g_damage_ctx = G_Damage_context();
        g_damage_ctx
            .expect()
            .withf(
                |&target, &inflictor, &attacker, &dir, &pos, &dmg, &dflags, &means_of_death| {
                    target.is_null()
                        && inflictor.is_null()
                        && attacker.is_null()
                        && dir.is_null()
                        && pos.is_null()
                        && dmg == 42
                        && dflags == DAMAGE_NO_TEAM_PROTECTION as c_int
                        && means_of_death == MOD_BFG as c_int
                },
            )
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine.vm_functions.entity.g_damage_detour.store(Some(
            // SAFETY: the mock original and the replacement have the same signature.
            unsafe {
                TypedDetour::new(
                    G_Damage
                        as extern "C" fn(
                            *mut gentity_t,
                            *mut gentity_t,
                            *mut gentity_t,
                            *mut vec3_t,
                            *mut vec3_t,
                            c_int,
                            c_int,
                            c_int,
                        ),
                    detoured_G_Damage,
                )
            }
            .expect("this should not happen")
            .into(),
        ));

        quake_engine.register_damage(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            42,
            DAMAGE_NO_TEAM_PROTECTION as c_int,
            MOD_BFG as c_int,
        );
    }
}

pub(crate) trait FreeEntity<T: AsMut<gentity_t>> {
    fn free_entity(&self, gentity: T);
}

impl<T: AsMut<gentity_t>> FreeEntity<T> for QuakeLiveEngine {
    fn free_entity(&self, mut gentity: T) {
        let _ = self.g_free_entity_orig().tap_ok(|original_func| {
            original_func(gentity.as_mut());
        });
    }
}

#[cfg(test)]
mod free_entity_quake_live_engine_tests {
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use super::{
        FreeEntity, QuakeLiveEngine,
        mock_quake_functions::{G_FreeEntity, G_FreeEntity_context},
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{GEntityBuilder, MockGameEntity},
        prelude::serial,
    };

    #[test]
    fn free_entity_with_no_original_function_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        quake_engine.free_entity(mock_gentity);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn free_entity_with_valid_original_function() {
        let mut mock_gentity = MockGameEntity::new();
        mock_gentity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_free_entity_ctx = G_FreeEntity_context();
        g_free_entity_ctx
            .expect()
            .withf(|&ent| !ent.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .entity
            .g_free_entity_orig
            .store(G_FreeEntity as *const () as usize, Ordering::Release);

        quake_engine.free_entity(mock_gentity.borrow_mut());
    }
}

/// `G_SpawnGEntityFromSpawnVars` loads the pointer to the level data right after it has called `G_Spawn`.
const SPAWN_LEVEL_LOAD_OFFSET: usize = 0x13;
const SPAWN_LEVEL_LOAD: [u8; 3] = [0x48, 0x8b, 0x1d];

pub(crate) trait SpawnEntityFromVars {
    /// Creates the entity that the key/value pairs describe, like the entity string of a map does; the game module
    /// frees it again when it has no spawn function. Returns whether the game module was asked to spawn it.
    fn spawn_entity_from_vars(&self, vars: &[(String, String)]) -> bool;
}

impl SpawnEntityFromVars for QuakeLiveEngine {
    #[cfg(target_os = "linux")]
    fn spawn_entity_from_vars(&self, vars: &[(String, String)]) -> bool {
        let Ok(spawn) = self.spawn_from_vars_orig() else {
            return false;
        };
        // SAFETY: the address is that of the function in the loaded game module, whose code is readable.
        let Some(level) = (unsafe {
            stats_feed::locate_global_at(spawn as usize + SPAWN_LEVEL_LOAD_OFFSET, SPAWN_LEVEL_LOAD)
        }) else {
            return false;
        };
        // SAFETY: the global is the level data of the game module, which has the table of spawn variables.
        if unsafe { spawn_vars::write_spawn_vars(level, vars) }.is_err() {
            return false;
        }
        spawn();
        // SAFETY: as above.
        unsafe { spawn_vars::clear_spawn_vars(level) };
        true
    }

    #[cfg(not(target_os = "linux"))]
    fn spawn_entity_from_vars(&self, _vars: &[(String, String)]) -> bool {
        false
    }
}

#[cfg(test)]
mod spawn_entity_quake_live_engine_tests {
    use super::{SpawnEntityFromVars, test_helpers::*};

    #[test]
    fn spawn_entity_from_vars_with_no_function_set() {
        let quake_engine = default_quake_engine();

        assert!(!quake_engine.spawn_entity_from_vars(&[("classname".into(), "info_null".into())]));
    }
}

/// Slot of the import table of the game module that holds the engine function `trap_LinkEntity`.
const LINK_ENTITY_SLOT: usize = 0x138;
/// `G_FreeEntity` starts with the load of the pointer to the import table, which it uses for `trap_UnlinkEntity`.
const FREE_ENTITY_TABLE_LOAD: [u8; 3] = [0x48, 0x8b, 0x05];

pub(crate) trait LinkEntity<T: AsMut<gentity_t>> {
    /// Tells the engine about the new position and size of the entity; returns whether that was possible.
    fn link_entity(&self, gentity: T) -> bool;
}

impl<T: AsMut<gentity_t>> LinkEntity<T> for QuakeLiveEngine {
    #[cfg(target_os = "linux")]
    fn link_entity(&self, mut gentity: T) -> bool {
        let Ok(free_entity) = self.g_free_entity_orig() else {
            return false;
        };
        // SAFETY: the address is that of G_FreeEntity in the loaded game module, whose code is readable.
        let Some(link_entity) = (unsafe {
            stats_feed::locate_import_at(
                free_entity as usize,
                FREE_ENTITY_TABLE_LOAD,
                LINK_ENTITY_SLOT,
            )
        }) else {
            return false;
        };
        // SAFETY: the slot holds trap_LinkEntity, which takes the entity.
        let link_entity = unsafe { function_at::<extern "C" fn(*mut gentity_t)>(link_entity) };
        link_entity(gentity.as_mut());
        true
    }

    #[cfg(not(target_os = "linux"))]
    fn link_entity(&self, _gentity: T) -> bool {
        false
    }
}

#[cfg(test)]
mod link_entity_quake_live_engine_tests {
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use super::{
        LinkEntity, QuakeLiveEngine,
        mock_quake_functions::{G_FreeEntity, G_FreeEntity_context},
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{GEntityBuilder, MockGameEntity},
        prelude::serial,
    };

    #[test]
    fn link_entity_with_no_original_function_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        assert!(!quake_engine.link_entity(mock_gentity));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn link_entity_without_the_table_load_in_the_function() {
        let mut mock_gentity = MockGameEntity::new();
        mock_gentity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_free_entity_ctx = G_FreeEntity_context();
        g_free_entity_ctx.expect().never();

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .entity
            .g_free_entity_orig
            .store(G_FreeEntity as *const () as usize, Ordering::Release);

        assert!(!quake_engine.link_entity(mock_gentity.borrow_mut()));
    }
}

pub(crate) trait StartKamikaze<T: AsMut<gentity_t> + ?Sized> {
    fn start_kamikaze(&self, gentity: T);
}

impl<T: AsMut<gentity_t>> StartKamikaze<T> for QuakeLiveEngine {
    fn start_kamikaze(&self, mut gentity: T) {
        self.vm_functions
            .entity
            .g_start_kamikaze_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(gentity.as_mut());
            });
    }
}

#[cfg(test)]
mod start_kamikaze_quake_live_engine_tests {
    use detour::TypedDetour;

    use super::{
        QuakeLiveEngine, StartKamikaze,
        mock_quake_functions::{
            G_StartKamikaze, G_StartKamikaze_context, detoured_G_StartKamikaze,
        },
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{GEntityBuilder, MockGameEntity, gentity_t},
        prelude::serial,
    };

    #[test]
    fn register_damage_with_no_detour_set() {
        let mock_gentity = MockGameEntity::default();
        let quake_engine = default_quake_engine();

        quake_engine.start_kamikaze(mock_gentity);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn start_kamikaze_with_valid_detour_function() {
        let mut mock_gentity = MockGameEntity::default();
        mock_gentity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_start_kamikaze_ctx = G_StartKamikaze_context();
        g_start_kamikaze_ctx
            .expect()
            .withf(|&ent| !ent.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .entity
            .g_start_kamikaze_detour
            .store(Some(
                // SAFETY: the mock original and the replacement have the same signature.
                unsafe {
                    TypedDetour::new(
                        G_StartKamikaze as extern "C" fn(*mut gentity_t),
                        detoured_G_StartKamikaze,
                    )
                }
                .expect("this should not happen")
                .into(),
            ));

        quake_engine.start_kamikaze(mock_gentity);
    }
}

/// The functions and hooks of the game module that act on entities.
#[derive(Default)]
pub(in crate::engine) struct EntityFunctions {
    pub(in crate::engine) g_addevent_orig: AtomicUsize,
    pub(in crate::engine) g_damage_orig: AtomicUsize,
    pub(in crate::engine) fire_weapon_orig: AtomicUsize,
    pub(in crate::engine) stats_medal_orig: AtomicUsize,
    pub(in crate::engine) stats_flag_status_orig: AtomicUsize,
    pub(in crate::engine) g_start_kamikaze_orig: AtomicUsize,
    pub(in crate::engine) g_free_entity_orig: AtomicUsize,
    pub(in crate::engine) spawn_from_vars_orig: AtomicUsize,
    #[cfg(feature = "race-hooks")]
    pub(in crate::engine) race_point_touch_orig: AtomicUsize,

    pub(in crate::engine) g_start_kamikaze_detour: ArcSwapOption<GStartKamikazeDetourType>,
    pub(in crate::engine) g_damage_detour: ArcSwapOption<GDamageDetourType>,
    pub(in crate::engine) fire_weapon_detour: ArcSwapOption<FireWeaponDetourType>,
    pub(in crate::engine) stats_medal_detour: ArcSwapOption<StatsMedalDetourType>,
    pub(in crate::engine) stats_flag_status_detour: ArcSwapOption<StatsFlagStatusDetourType>,
}

impl EntityFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn symbols(&self) -> Vec<(QuakeLiveFunction, &AtomicUsize)> {
        vec![
            (QuakeLiveFunction::G_AddEvent, &self.g_addevent_orig),
            (QuakeLiveFunction::G_Damage, &self.g_damage_orig),
            (QuakeLiveFunction::FireWeapon, &self.fire_weapon_orig),
            (QuakeLiveFunction::G_StatsMedal, &self.stats_medal_orig),
            (
                QuakeLiveFunction::G_StatsFlagStatus,
                &self.stats_flag_status_orig,
            ),
            (
                QuakeLiveFunction::G_StartKamikaze,
                &self.g_start_kamikaze_orig,
            ),
            (QuakeLiveFunction::G_FreeEntity, &self.g_free_entity_orig),
            (
                QuakeLiveFunction::G_SpawnGEntityFromSpawnVars,
                &self.spawn_from_vars_orig,
            ),
            #[cfg(feature = "race-hooks")]
            (
                QuakeLiveFunction::RacePointTouch,
                &self.race_point_touch_orig,
            ),
        ]
    }

    pub(in crate::engine) fn hook(
        &self,
        callbacks: &EntityCallbacks,
    ) -> Result<(), QuakeLiveEngineError> {
        let g_start_kamikaze_orig = self.g_start_kamikaze_orig.load(Ordering::Acquire);
        let g_start_kamikaze_func =
            // SAFETY: the address was found in the engine and has the signature of the function type.
            unsafe { function_at::<extern "C" fn(*mut gentity_s)>(g_start_kamikaze_orig) };
        // SAFETY: the original is a function of the engine and the replacement has the same signature.
        let g_start_kamikaze_detour = unsafe {
            GStartKamikazeDetourType::new(g_start_kamikaze_func, callbacks.g_startkamikaze)
        }
        .map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_StartKamikaze)
        })?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { g_start_kamikaze_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_StartKamikaze)
        })?;

        self.g_start_kamikaze_detour
            .swap(Some(g_start_kamikaze_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling start_kamikaze detour: {e}");
                });
            });

        let g_damage_orig = self.g_damage_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let g_damage_func = unsafe {
            function_at::<
                extern "C" fn(
                    *mut gentity_t,
                    *mut gentity_t,
                    *mut gentity_t,
                    *mut vec3_t,
                    *mut vec3_t,
                    c_int,
                    c_int,
                    c_int,
                ),
            >(g_damage_orig)
        };
        // SAFETY: the original and the replacement are functions with the same signature.
        let g_damage_detour = unsafe { GDamageDetourType::new(g_damage_func, callbacks.g_damage) }
            .map_err(|_| {
                cold_path();
                QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_Damage)
            })?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { g_damage_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_Damage)
        })?;

        self.g_damage_detour
            .swap(Some(g_damage_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling damage detour: {e}");
                });
            });

        let fire_weapon_orig = self.fire_weapon_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let fire_weapon_func =
            unsafe { function_at::<extern "C" fn(*mut gentity_t)>(fire_weapon_orig) };
        // SAFETY: the original and the replacement are functions with the same signature.
        let fire_weapon_detour =
            unsafe { FireWeaponDetourType::new(fire_weapon_func, callbacks.fire_weapon) }.map_err(
                |_| {
                    cold_path();
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::FireWeapon)
                },
            )?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { fire_weapon_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::FireWeapon)
        })?;

        self.fire_weapon_detour
            .swap(Some(fire_weapon_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling fire weapon detour: {e}");
                });
            });

        let stats_medal_orig = self.stats_medal_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let stats_medal_func = unsafe {
            function_at::<extern "C" fn(*mut gentity_t, *const c_char, c_int)>(stats_medal_orig)
        };
        // SAFETY: the original and the replacement are functions with the same signature.
        let stats_medal_detour =
            unsafe { StatsMedalDetourType::new(stats_medal_func, callbacks.stats_medal) }.map_err(
                |_| {
                    cold_path();
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_StatsMedal)
                },
            )?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { stats_medal_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_StatsMedal)
        })?;

        self.stats_medal_detour
            .swap(Some(stats_medal_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling stats medal detour: {e}");
                });
            });

        let stats_flag_status_orig = self.stats_flag_status_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let stats_flag_status_func = unsafe {
            function_at::<extern "C" fn(c_int, c_int, *mut gentity_t)>(stats_flag_status_orig)
        };
        // SAFETY: the original and the replacement are functions with the same signature.
        let stats_flag_status_detour = unsafe {
            StatsFlagStatusDetourType::new(stats_flag_status_func, callbacks.stats_flag_status)
        }
        .map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_StatsFlagStatus)
        })?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { stats_flag_status_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_StatsFlagStatus)
        })?;

        self.stats_flag_status_detour
            .swap(Some(stats_flag_status_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling stats flag status detour: {e}");
                });
            });

        Ok(())
    }

    pub(in crate::engine) fn unhook(&self) {
        [
            &self.g_addevent_orig,
            &self.g_damage_orig,
            &self.fire_weapon_orig,
            &self.stats_medal_orig,
            &self.stats_flag_status_orig,
            &self.g_start_kamikaze_orig,
            &self.g_free_entity_orig,
            &self.spawn_from_vars_orig,
            #[cfg(feature = "race-hooks")]
            &self.race_point_touch_orig,
        ]
        .iter()
        .for_each(|field| {
            field.store(0, Ordering::Release);
        });

        self.g_start_kamikaze_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling start_kamikaze detour: {e}");
                });
            });

        self.g_damage_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling damage detour: {e}");
                });
            });

        self.fire_weapon_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling fire weapon detour: {e}");
                });
            });

        self.stats_medal_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling stats medal detour: {e}");
                });
            });

        self.stats_flag_status_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling stats flag status detour: {e}");
                });
            });
    }
}

impl QuakeLiveEngine {
    pub(in crate::engine) fn g_addevent_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, entity_event_t, c_int), QuakeLiveEngineError> {
        match self
            .vm_functions
            .entity
            .g_addevent_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_AddEvent,
                ))
            }
            g_addevent_orig => {
                // SAFETY: the address was found in the engine and has the signature of the function type.
                let g_addevent_func = unsafe {
                    function_at::<extern "C" fn(*mut gentity_t, entity_event_t, c_int)>(
                        g_addevent_orig,
                    )
                };
                Ok(g_addevent_func)
            }
        }
    }

    pub(crate) fn g_free_entity_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t), QuakeLiveEngineError> {
        match self
            .vm_functions
            .entity
            .g_free_entity_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_FreeEntity,
                ))
            }
            g_free_entity_orig => {
                let g_free_entity_func =
                    // SAFETY: the address was found in the engine and has the signature of the function type.
                    unsafe { function_at::<extern "C" fn(*mut gentity_t)>(g_free_entity_orig) };
                Ok(g_free_entity_func)
            }
        }
    }

    pub(crate) fn spawn_from_vars_orig(&self) -> Result<extern "C" fn(), QuakeLiveEngineError> {
        match self
            .vm_functions
            .entity
            .spawn_from_vars_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_SpawnGEntityFromSpawnVars,
                ))
            }
            spawn_from_vars_orig => {
                // SAFETY: the address was found in the engine and has the signature of the function type.
                Ok(unsafe { function_at::<extern "C" fn()>(spawn_from_vars_orig) })
            }
        }
    }

    #[cfg(feature = "race-hooks")]
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn race_point_touch_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>
    {
        match self
            .vm_functions
            .entity
            .race_point_touch_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::RacePointTouch,
                ))
            }
            race_point_touch_orig => {
                // SAFETY: the address was found in the engine and has the signature of the function type.
                let race_point_touch_func = unsafe {
                    function_at::<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>(
                        race_point_touch_orig,
                    )
                };
                Ok(race_point_touch_func)
            }
        }
    }
}

#[cfg(test)]
mod vm_accessor_tests {
    use core::sync::atomic::Ordering;

    use crate::{
        engine::{mock_quake_functions::*, patterns::QuakeLiveFunction, test_helpers::*},
        prelude::QuakeLiveEngineError,
    };

    #[test]
    fn g_addevent_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_addevent_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_AddEvent,))
        );
    }

    #[test]
    fn g_addevent_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .entity
            .g_addevent_orig
            .store(G_AddEvent as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .entity
            .g_addevent_orig
            .load(Ordering::Acquire);
        let result = quake_engine.g_addevent_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }

    #[test]
    fn g_free_entity_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_free_entity_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_FreeEntity,))
        );
    }

    #[test]
    fn g_free_entity_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .entity
            .g_free_entity_orig
            .store(G_FreeEntity as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .entity
            .g_free_entity_orig
            .load(Ordering::Acquire);
        let result = quake_engine.g_free_entity_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }
}

pub(in crate::engine) type GStartKamikazeDetourType = TypedDetour<extern "C" fn(*mut gentity_t)>;
pub(in crate::engine) type GDamageDetourType = TypedDetour<
    extern "C" fn(
        *mut gentity_t,
        *mut gentity_t,
        *mut gentity_t,
        *mut vec3_t,
        *mut vec3_t,
        c_int,
        c_int,
        c_int,
    ),
>;
pub(in crate::engine) type FireWeaponDetourType = TypedDetour<extern "C" fn(*mut gentity_t)>;
pub(in crate::engine) type StatsMedalDetourType =
    TypedDetour<extern "C" fn(*mut gentity_t, *const c_char, c_int)>;
pub(in crate::engine) type StatsFlagStatusDetourType =
    TypedDetour<extern "C" fn(c_int, c_int, *mut gentity_t)>;
