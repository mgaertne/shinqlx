use super::prelude::*;
use crate::prelude::*;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Activator {
    activator: &'static gentity_t,
}

impl TryFrom<*mut gentity_t> for Activator {
    type Error = QuakeLiveEngineError;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe { game_entity.as_ref() }
            .map(|gentity| Self { activator: gentity })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl Activator {
    pub(crate) fn get_owner_num(&self) -> i32 {
        self.activator.r.ownerNum
    }
}

#[cfg(test)]
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
    use super::Activator;
    use crate::ffi::c::prelude::*;
    use crate::prelude::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn activator_try_from_null_results_in_error() {
        assert_eq!(
            Activator::try_from(ptr::null_mut() as *mut gentity_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn activator_try_from_valid_entity() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            Activator::try_from(&mut gentity as *mut gentity_t).is_ok(),
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
        let activator =
            Activator::try_from(&mut gentity as *mut gentity_t).expect("this should not happen");
        assert_eq!(activator.get_owner_num(), 42);
    }
}
