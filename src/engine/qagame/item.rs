use core::{
    hint::cold_path,
    sync::atomic::{AtomicUsize, Ordering},
};

use arc_swap::ArcSwapOption;
use detour::TypedDetour;
use tap::{TapFallible, TapOptional};

#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{
        QuakeLiveEngine, QuakeLiveEngineError, callbacks::ItemCallbacks,
        patterns::QuakeLiveFunction, prelude::*, symbols::function_at,
    },
    prelude::*,
};

pub(crate) trait TryLaunchItem<T: AsMut<gitem_t>> {
    fn try_launch_item(
        &self,
        gitem: T,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> Result<GameEntity, QuakeLiveEngineError>;
}

impl<T: AsMut<gitem_t>> TryLaunchItem<T> for QuakeLiveEngine {
    fn try_launch_item(
        &self,
        mut gitem: T,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> Result<GameEntity, QuakeLiveEngineError> {
        self.launch_item_orig()
            .map(|original_func| original_func(gitem.as_mut(), origin, velocity))
            .and_then(GameEntity::try_from)
    }
}

#[cfg(test)]
mod try_launch_item_quake_live_engine_tests {
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use super::{
        QuakeLiveEngine, TryLaunchItem,
        mock_quake_functions::{LaunchItem, LaunchItem_context},
        test_helpers::*,
    };
    use crate::{
        engine::{
            patterns::QuakeLiveFunction,
            prelude::{GEntityBuilder, GItemBuilder, MockGameEntity, MockGameItem, vec3_t},
        },
        prelude::{QuakeLiveEngineError, serial},
    };

    #[test]
    fn try_launch_item_with_no_original_function_set() {
        let mock_item = MockGameItem::default();
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let quake_engine = default_quake_engine();

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::LaunchItem,))
        )
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_launch_item_with_valid_original_function() {
        let mut mock_item = MockGameItem::default();
        mock_item.expect_as_mut().returning(|| {
            GItemBuilder::default()
                .build()
                .expect("this should not happen")
        });
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let launch_item_ctx = LaunchItem_context();
        launch_item_ctx
            .expect()
            .withf(|&item, &_pos, &_dir| !item.is_null())
            .returning(|_, _, _| {
                let mut returned = GEntityBuilder::default()
                    .build()
                    .expect("this should not happen");
                returned.borrow_mut()
            })
            .times(1);

        let gentity_try_from_ctx = MockGameEntity::try_from_context();
        gentity_try_from_ctx
            .expect()
            .returning(|_| Ok(MockGameEntity::default()))
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .item
            .launch_item_orig
            .store(LaunchItem as *const () as usize, Ordering::Release);

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_launch_item_with_valid_original_function_when_invalid_gentity_returned() {
        let mut mock_item = MockGameItem::default();
        mock_item.expect_as_mut().returning(|| {
            GItemBuilder::default()
                .build()
                .expect("this should not happen")
        });
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let launch_item_ctx = LaunchItem_context();
        launch_item_ctx
            .expect()
            .withf(|&item, &_pos, &_dir| !item.is_null())
            .returning(|_, _, _| {
                let mut returned = GEntityBuilder::default()
                    .build()
                    .expect("this should not happen");
                returned.borrow_mut()
            })
            .times(1);

        let gentity_try_from_ctx = MockGameEntity::try_from_context();
        gentity_try_from_ctx
            .expect()
            .returning(|_| {
                Err(QuakeLiveEngineError::NullPointerPassed(
                    "null pointer passed".to_string(),
                ))
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .item
            .launch_item_orig
            .store(LaunchItem as *const () as usize, Ordering::Release);

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::NullPointerPassed("null pointer passed".to_string(),)));
    }
}

/// The functions and hooks of the game module that handle items.
#[derive(Default)]
pub(in crate::engine) struct ItemFunctions {
    pub(in crate::engine) touch_item_orig: AtomicUsize,
    pub(in crate::engine) launch_item_orig: AtomicUsize,
    pub(in crate::engine) drop_item_orig: AtomicUsize,

    pub(in crate::engine) touch_item_detour: ArcSwapOption<TouchItemDetourType>,
}

impl ItemFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn symbols(&self) -> Vec<(QuakeLiveFunction, &AtomicUsize)> {
        vec![
            (QuakeLiveFunction::Touch_Item, &self.touch_item_orig),
            (QuakeLiveFunction::LaunchItem, &self.launch_item_orig),
            (QuakeLiveFunction::Drop_Item, &self.drop_item_orig),
        ]
    }

    pub(in crate::engine) fn hook(
        &self,
        callbacks: &ItemCallbacks,
    ) -> Result<(), QuakeLiveEngineError> {
        let touch_item_orig = self.touch_item_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let touch_item_func = unsafe {
            function_at::<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>(
                touch_item_orig,
            )
        };
        // SAFETY: the original and the replacement are functions with the same signature.
        let touch_item_detour =
            unsafe { TouchItemDetourType::new(touch_item_func, callbacks.touch_item) }.map_err(
                |_| {
                    cold_path();
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::Touch_Item)
                },
            )?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { touch_item_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::Touch_Item)
        })?;

        self.touch_item_detour
            .swap(Some(touch_item_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling touch item detour: {e}");
                });
            });

        Ok(())
    }

    pub(in crate::engine) fn unhook(&self) {
        [
            &self.touch_item_orig,
            &self.launch_item_orig,
            &self.drop_item_orig,
        ]
        .iter()
        .for_each(|field| {
            field.store(0, Ordering::Release);
        });

        self.touch_item_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling touch item detour: {e}");
                });
            });
    }
}

impl QuakeLiveEngine {
    pub(crate) fn launch_item_orig(
        &self,
    ) -> Result<
        extern "C" fn(*mut gitem_t, *mut vec3_t, *mut vec3_t) -> *mut gentity_t,
        QuakeLiveEngineError,
    > {
        match self
            .vm_functions
            .item
            .launch_item_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::LaunchItem,
                ))
            }
            launch_item_orig => {
                // SAFETY: the address was found in the engine and has the signature of the function type.
                let launch_item_func = unsafe {
                    function_at::<
                        extern "C" fn(*mut gitem_t, *mut vec3_t, *mut vec3_t) -> *mut gentity_t,
                    >(launch_item_orig)
                };
                Ok(launch_item_func)
            }
        }
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn touch_item_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>
    {
        match self
            .vm_functions
            .item
            .touch_item_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::Touch_Item,
                ))
            }
            touch_item_orig => {
                // SAFETY: the address was found in the engine and has the signature of the function type.
                let touch_item_func = unsafe {
                    function_at::<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>(
                        touch_item_orig,
                    )
                };
                Ok(touch_item_func)
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
    fn launch_item_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.launch_item_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::LaunchItem,))
        );
    }

    #[test]
    fn launch_item_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .item
            .launch_item_orig
            .store(LaunchItem as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .item
            .launch_item_orig
            .load(Ordering::Acquire);
        let result = quake_engine.launch_item_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }

    #[test]
    fn touch_item_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.touch_item_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::Touch_Item,))
        );
    }

    #[test]
    fn touch_item_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .item
            .touch_item_orig
            .store(Touch_Item as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .item
            .touch_item_orig
            .load(Ordering::Acquire);
        let result = quake_engine.touch_item_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }
}

pub(in crate::engine) type TouchItemDetourType =
    TypedDetour<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t)>;
