use super::prelude::*;
use crate::prelude::*;
use crate::MAIN_ENGINE;

#[derive(Debug, PartialEq)]
#[allow(non_snake_case)]
#[repr(transparent)]
pub(crate) struct ServerStatic {
    serverStatic_t: &'static mut serverStatic_t,
}

impl TryFrom<*mut serverStatic_t> for ServerStatic {
    type Error = QuakeLiveEngineError;

    fn try_from(server_static: *mut serverStatic_t) -> Result<Self, Self::Error> {
        unsafe { server_static.as_mut() }
            .map(|svs| Self {
                serverStatic_t: svs,
            })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl ServerStatic {
    pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(QuakeLiveEngineError::MainEngineNotInitialized);
        };

        let func_pointer = main_engine.sv_shutdown_orig()?;

        let svs_ptr_ptr = func_pointer as usize + 0xAC;
        let svs_ptr: u32 = unsafe { ptr::read(svs_ptr_ptr as *const u32) };
        Self::try_from(svs_ptr as *mut serverStatic_t)
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) unsafe fn try_get_client_by_id(
        &self,
        client_id: i32,
    ) -> Result<*mut client_t, QuakeLiveEngineError> {
        let max_clients = i32::try_from(MAX_CLIENTS).unwrap();

        if !(0..max_clients).contains(&client_id) {
            return Err(QuakeLiveEngineError::InvalidId(client_id));
        }

        Ok(self.serverStatic_t.clients.offset(client_id as isize))
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) unsafe fn try_determine_client_id(
        &self,
        client_t: &client_t,
    ) -> Result<i32, QuakeLiveEngineError> {
        let offset = (client_t as *const client_t).offset_from(self.serverStatic_t.clients);

        if !(0..MAX_CLIENTS as isize).contains(&offset) {
            return Err(QuakeLiveEngineError::ClientNotFound(
                "client not found".into(),
            ));
        }

        offset
            .try_into()
            .map_err(|_| QuakeLiveEngineError::ClientNotFound("client not found".into()))
    }
}

#[cfg(test)]
mockall::mock! {
    pub(crate) TestServerStatic {
        pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError>;
        pub(crate) unsafe fn try_get_client_by_id(
            &self,
            client_id: i32,
        ) -> Result<*mut client_t, QuakeLiveEngineError>;
        pub(crate) unsafe fn try_determine_client_id(
            &self,
            client_t: *const client_t,
        ) -> Result<i32, QuakeLiveEngineError>;
    }
    impl TryFrom<*mut serverStatic_t> for TestServerStatic {
        type Error = QuakeLiveEngineError;
        fn try_from(server_static: *mut serverStatic_t) -> Result<Self, QuakeLiveEngineError>;
    }
}

#[cfg(test)]
mod server_static_tests {
    use super::ServerStatic;
    use super::MAIN_ENGINE;
    use crate::ffi::c::prelude::*;
    use crate::prelude::*;
    use crate::quake_live_functions::QuakeLiveFunction::SV_Shutdown;

    use pretty_assertions::assert_eq;

    #[test]
    fn server_static_try_from_null_results_in_error() {
        assert_eq!(
            ServerStatic::try_from(ptr::null_mut()),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn server_static_try_from_valid_server_static() {
        let mut server_static = ServerStaticBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            ServerStatic::try_from(&mut server_static as *mut serverStatic_t).is_ok(),
            true
        );
    }

    #[test]
    #[serial]
    fn server_static_default_panics_when_no_main_engine_found() {
        MAIN_ENGINE.store(None);

        let result = ServerStatic::try_get();

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("this should not happen"),
            QuakeLiveEngineError::MainEngineNotInitialized
        );
    }

    #[test]
    #[serial]
    fn server_static_default_panics_when_offset_function_not_initialized() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_sv_shutdown_orig()
            .return_once(|| Err(QuakeLiveEngineError::StaticFunctionNotFound(SV_Shutdown)));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = ServerStatic::try_get();

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("this should not happen"),
            QuakeLiveEngineError::StaticFunctionNotFound(SV_Shutdown)
        );
    }

    #[test]
    fn server_static_try_get_client_by_id_from_too_small_id() {
        let mut server_static = ServerStaticBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_get_client_by_id(-1) },
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    fn server_static_try_get_client_by_id_from_too_large_id() {
        let mut server_static = ServerStaticBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_get_client_by_id(65536) },
            Err(QuakeLiveEngineError::InvalidId(65536))
        );
    }

    #[test]
    fn server_static_try_get_client_by_id_from_valid_client() {
        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut client as *mut client_t)
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");

        assert_eq!(
            unsafe { rust_server_static.try_get_client_by_id(0) },
            Ok(&mut client as *mut client_t)
        );
    }

    //noinspection DuplicatedCode
    #[test]
    fn server_static_try_get_client_by_id_from_ok_client_not_first_position() {
        let mut clients = vec![
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
        ];
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut clients[0])
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_get_client_by_id(2) },
            Ok(&mut clients[2] as *mut client_t)
        );
    }
    #[test]
    #[cfg_attr(miri, ignore)]
    fn server_static_determine_client_id_from_invalid_client() {
        let client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut server_static = ServerStaticBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_determine_client_id(&client) },
            Err(QuakeLiveEngineError::ClientNotFound(
                "client not found".into()
            ))
        );
    }

    #[test]
    fn server_static_determine_client_id_from_ok_client() {
        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut client as *mut client_t)
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_determine_client_id(&client) },
            Ok(0)
        );
    }

    //noinspection DuplicatedCode
    #[test]
    fn server_static_determine_client_id_from_ok_client_not_first_position() {
        let mut clients = vec![
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
        ];
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut clients[0])
            .build()
            .expect("this should not happen");
        let rust_server_static = ServerStatic::try_from(&mut server_static as *mut serverStatic_t)
            .expect("this should not happen");
        assert_eq!(
            unsafe { rust_server_static.try_determine_client_id(&(clients[2])) },
            Ok(2)
        );
    }
}
