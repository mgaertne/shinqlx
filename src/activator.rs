use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::gentity_t;

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
            .ok_or(NullPointerPassed("null pointer passed".into()))
    }
}

impl Activator {
    pub(crate) fn get_owner_num(&self) -> i32 {
        self.activator.r.ownerNum
    }
}

#[cfg(test)]
pub(crate) mod activator_tests {
    use crate::activator::Activator;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::{gentity_t, EntitySharedBuilder, GEntityBuilder};
    use pretty_assertions::assert_eq;

    #[test]
    pub(crate) fn activator_try_from_null_results_in_error() {
        assert_eq!(
            Activator::try_from(std::ptr::null_mut() as *mut gentity_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn activator_try_from_valid_entity() {
        let mut gentity = GEntityBuilder::default().build().unwrap();
        assert_eq!(
            Activator::try_from(&mut gentity as *mut gentity_t).is_ok(),
            true
        );
    }

    #[test]
    pub(crate) fn activator_get_owner_num() {
        let entity_shared = EntitySharedBuilder::default().ownerNum(42).build().unwrap();
        let mut gentity = GEntityBuilder::default().r(entity_shared).build().unwrap();
        let activator = Activator::try_from(&mut gentity as *mut gentity_t).unwrap();
        assert_eq!(activator.get_owner_num(), 42);
    }
}
