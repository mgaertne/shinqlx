use core::ptr::NonNull;

use crate::{engine::prelude::*, prelude::*};

/// Handle to the entity that activated another one.
#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Activator {
    activator: NonNull<gentity_t>,
}

impl TryFrom<*mut gentity_t> for Activator {
    type Error = QuakeLiveEngineError;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        NonNull::new(game_entity)
            .map(|activator| Self { activator })
            .ok_or(QuakeLiveEngineError::NullPointerPassed)
    }
}

impl Activator {
    pub(crate) fn get_owner_num(&self) -> i32 {
        // SAFETY: the pointer was non-null on creation and points to an entity of the loaded game module.
        unsafe { self.activator.as_ref() }.r.ownerNum
    }
}

#[cfg(test)]
#[cfg(not(tarpaulin_include))]
mockall::mock! {
    pub(crate) Activator {
        pub(crate) fn get_owner_num(&self) -> i32;
    }

    impl TryFrom<*mut gentity_t> for Activator {
        type Error = QuakeLiveEngineError;
        fn try_from(game_entity: *mut gentity_t) -> Result<Self, QuakeLiveEngineError>;
    }
}

#[cfg(test)]
mod activator_tests {
    use core::borrow::BorrowMut;

    use pretty_assertions::assert_eq;

    use super::Activator;
    use crate::{engine::prelude::*, prelude::*};

    #[test]
    fn activator_try_from_null_results_in_error() {
        assert_eq!(
            Activator::try_from(ptr::null_mut() as *mut gentity_t),
            Err(QuakeLiveEngineError::NullPointerPassed)
        );
    }

    #[test]
    fn activator_try_from_valid_entity() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            Activator::try_from(gentity.borrow_mut() as *mut gentity_t).is_ok(),
            true
        );
    }

    #[test]
    fn activator_get_owner_num() {
        let entity_shared = EntitySharedBuilder::default()
            .ownerNum(42)
            .build()
            .expect("this should not happen");
        let mut gentity = GEntityBuilder::default()
            .r(entity_shared)
            .build()
            .expect("this should not happen");
        let activator = Activator::try_from(gentity.borrow_mut() as *mut gentity_t)
            .expect("this should not happen");
        assert_eq!(activator.get_owner_num(), 42);
    }
}
