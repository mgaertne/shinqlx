use alloc::{borrow::Cow, ffi::CString};
use core::ffi::{CStr, c_char};

use tap::TapOptional;

use super::prelude::*;
use crate::{MAIN_ENGINE, prelude::*};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Client {
    client_t: &'static mut client_t,
}

impl TryFrom<*const client_t> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client: *const client_t) -> Result<Self, Self::Error> {
        Self::try_from(client.cast_mut())
    }
}

impl TryFrom<*mut client_t> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client: *mut client_t) -> Result<Self, Self::Error> {
        unsafe { client.as_mut() }
            .map(|client_t| Self { client_t })
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string(),
            ))
    }
}

impl TryFrom<i32> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        let server_static = ServerStatic::try_get()?;
        let client = server_static.try_get_client_by_id(client_id)?;
        Self::try_from(client)
            .map_err(|_| QuakeLiveEngineError::ClientNotFound("client not found".to_string()))
    }
}

impl AsMut<client_t> for Client {
    fn as_mut(&mut self) -> &mut client_t {
        self.client_t
    }
}

impl AsRef<client_t> for Client {
    fn as_ref(&self) -> &client_t {
        self.client_t
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        let Ok(server_static) = ServerStatic::try_get() else {
            return -1;
        };
        server_static
            .try_determine_client_id(self.client_t)
            .unwrap_or(-1)
    }

    pub(crate) fn get_state(&self) -> clientState_t {
        self.client_t.state
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect<T>(&mut self, reason: T)
    where
        T: AsRef<str>,
    {
        let c_reason = CString::new(reason.as_ref()).unwrap_or_else(|_| c"".into());

        MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
            if let Ok(detour) = main_engine.sv_dropclient_detour() {
                detour.call(self.client_t, c_reason.as_ptr());
            }
        });
    }

    pub(crate) fn get_name(&self) -> Cow<str> {
        unsafe { CStr::from_ptr(&self.client_t.name as *const c_char) }.to_string_lossy()
    }

    pub(crate) fn get_user_info(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(self.client_t.userinfo.as_ptr()) }.to_string_lossy()
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }
}

#[cfg(test)]
mockall::mock! {
    pub(crate) Client {
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn get_name(&self) -> Cow<'_, str>;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn has_gentity(&self) -> bool;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn get_client_id(&self) -> i32;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn get_state(&self) -> clientState_t;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn disconnect(&mut self, reason: &str);
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn get_user_info(&self) -> Cow<'_, str>;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        pub(crate) fn get_steam_id(&self) -> u64;
    }

    impl TryFrom<*mut client_t> for Client {
        type Error = QuakeLiveEngineError;
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        fn try_from(client: *mut client_t) -> Result<Self, QuakeLiveEngineError>;
    }

    impl From<i32> for Client {
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        fn from(entity_id: i32) -> Self;
    }

    impl AsRef<client_t> for Client {
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        fn as_ref(&self) -> &client_t;
    }

    impl AsMut<client_t> for Client {
        #[allow(unused_attributes)]
        #[cfg(not(tarpaulin_include))]
        fn as_mut(&mut self) -> &mut client_t;
    }

}

#[cfg(test)]
mod client_tests {
    use core::{
        borrow::BorrowMut,
        ffi::{CStr, c_char},
    };
    use std::sync::OnceLock;

    use pretty_assertions::assert_eq;
    use retour::GenericDetour;

    use super::Client;
    use crate::{ffi::c::prelude::*, prelude::*, quake_live_functions::QuakeLiveFunction};

    #[test]
    fn client_try_from_null_results_in_error() {
        assert_eq!(
            Client::try_from(ptr::null() as *const client_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".to_string()
            ))
        );
    }

    #[test]
    fn client_try_from_valid_client() {
        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            Client::try_from(client.borrow_mut() as *mut client_t).is_ok(),
            true
        );
    }

    #[test]
    #[serial]
    fn client_try_from_negative_client_id() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx.expect().return_once(|| {
            let mut server_static_mock = MockTestServerStatic::new();
            server_static_mock
                .expect_try_get_client_by_id()
                .returning(|id| Err(QuakeLiveEngineError::InvalidId(id)));
            Ok(server_static_mock)
        });

        assert_eq!(
            Client::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    #[serial]
    fn client_try_from_too_large_client_id() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx.expect().return_once(|| {
            let mut server_static_mock = MockTestServerStatic::new();
            server_static_mock
                .expect_try_get_client_by_id()
                .returning(|id| Err(QuakeLiveEngineError::InvalidId(id)));
            Ok(server_static_mock)
        });

        assert_eq!(
            Client::try_from(32384),
            Err(QuakeLiveEngineError::InvalidId(32384))
        );
    }

    #[test]
    #[serial]
    fn client_try_from_valid_client_id_but_null() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx.expect().return_once(|| {
            let mut server_static_mock = MockTestServerStatic::new();
            server_static_mock
                .expect_try_get_client_by_id()
                .returning(|_| Ok(ptr::null_mut() as *mut client_t));
            Ok(server_static_mock)
        });

        assert_eq!(
            Client::try_from(2),
            Err(QuakeLiveEngineError::ClientNotFound(
                "client not found".to_string()
            ))
        );
    }

    #[test]
    #[serial]
    fn client_get_client_id_when_no_serverstatic_found() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx
            .expect()
            .return_once(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_client_id(), -1);
    }

    #[test]
    #[serial]
    fn client_get_client_id_from_server_static() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx.expect().return_once(|| {
            let mut server_static_mock = MockTestServerStatic::new();
            server_static_mock
                .expect_try_determine_client_id()
                .return_once(|_| Ok(0));
            Ok(server_static_mock)
        });

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");

        assert_eq!(rust_client.get_client_id(), 0);
    }

    #[test]
    #[serial]
    fn client_get_client_id_from_server_static_not_first_position() {
        let server_static_try_get_ctx = MockTestServerStatic::try_get_context();
        server_static_try_get_ctx.expect().return_once(|| {
            let mut server_static_mock = MockTestServerStatic::new();
            server_static_mock
                .expect_try_determine_client_id()
                .return_once(|_| Ok(2));
            Ok(server_static_mock)
        });

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");

        assert_eq!(rust_client.get_client_id(), 2);
    }

    #[test]
    fn client_get_state() {
        let mut client = ClientBuilder::default()
            .state(clientState_t::CS_ZOMBIE)
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_state(), clientState_t::CS_ZOMBIE);
    }

    #[test]
    fn client_has_gentity_with_no_shared_entity() {
        let mut client = ClientBuilder::default()
            .gentity(ptr::null_mut())
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.has_gentity(), false);
    }

    #[test]
    fn client_has_gentity_with_valid_shared_entity() {
        let mut shared_entity = SharedEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let mut client = ClientBuilder::default()
            .gentity(shared_entity.borrow_mut() as *mut sharedEntity_t)
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.has_gentity(), true);
    }

    #[test]
    #[serial]
    fn client_disconnect_with_no_main_engine() {
        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");
        let mut rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        rust_client.disconnect("disconnected");
    }

    #[test]
    #[serial]
    fn client_disconnect_with_no_detour_setup() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_sv_dropclient_detour().return_once(|| {
                    Err(QuakeLiveEngineError::StaticDetourNotFound(
                        QuakeLiveFunction::SV_DropClient,
                    ))
                });
            })
            .run(|| {
                let mut client = ClientBuilder::default()
                    .build()
                    .expect("this should not happen");
                let mut rust_client = Client::try_from(client.borrow_mut() as *mut client_t)
                    .expect("this should not happen");
                rust_client.disconnect("disconnected");
            });
    }

    mockall::mock! {
       SV_DropcClient {
            #[allow(unused_attributes)]
            #[cfg(not(tarpaulin_include))]
            fn original_func(_client: *mut client_t, _reason: *const c_char);
            #[allow(unused_attributes)]
            #[cfg(not(tarpaulin_include))]
            fn replacement_func(_client: *mut client_t, _reason: *const c_char);
        }
    }

    #[cfg_attr(test, allow(clippy::type_complexity))]
    static SV_DROPCLIENT_DETOUR: OnceLock<GenericDetour<fn(*mut client_t, *const c_char)>> =
        OnceLock::new();

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_with_valid_detour() {
        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        let sv_dropclient_detour = unsafe {
            GenericDetour::<fn(*mut client_t, *const c_char)>::new(
                MockSV_DropcClient::original_func,
                MockSV_DropcClient::replacement_func,
            )
            .expect("this should not happen")
        };
        SV_DROPCLIENT_DETOUR
            .set(sv_dropclient_detour)
            .expect("this should not happen");

        let dropclient_original_ctx = MockSV_DropcClient::original_func_context();
        dropclient_original_ctx
            .expect()
            .withf(|_client, &reason| unsafe { CStr::from_ptr(reason) } == c"disconnected");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_sv_dropclient_detour().returning(|| {
                    let Some(detour) = SV_DROPCLIENT_DETOUR.get() else {
                        return Err(QuakeLiveEngineError::MainEngineNotInitialized);
                    };

                    Ok(detour)
                });
            })
            .run(|| {
                let mut rust_client = Client::try_from(client.borrow_mut() as *mut client_t)
                    .expect("this should not happen");
                rust_client.disconnect("disconnected");
            });
    }

    #[test]
    fn client_get_name_from_empty_name() {
        let mut client = ClientBuilder::default()
            .name([0; MAX_NAME_LENGTH as usize])
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_name(), "");
    }

    #[test]
    fn client_get_name_from_valid_name() {
        let player_name_str = "UnknownPlayer";
        let mut bytes = player_name_str.bytes();
        let mut player_name: [c_char; MAX_NAME_LENGTH as usize] = [0; MAX_NAME_LENGTH as usize];
        player_name[0..player_name_str.len()].fill_with(|| bytes.next().unwrap() as c_char);
        let mut client = ClientBuilder::default()
            .name(player_name)
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_name(), "UnknownPlayer");
    }

    #[test]
    fn client_get_userinfo_from_null() {
        let mut client = ClientBuilder::default()
            .userinfo([0; MAX_INFO_STRING as usize])
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_user_info(), "");
    }

    #[test]
    fn client_get_userinfo_from_valid_userinfo() {
        let user_info_str = "some user info";
        let mut bytes = user_info_str.bytes();
        let mut userinfo: [c_char; MAX_INFO_STRING as usize] = [0; MAX_INFO_STRING as usize];
        userinfo[0..user_info_str.len()]
            .fill_with(|| bytes.next().expect("this should not happen") as c_char);
        let mut client = ClientBuilder::default()
            .userinfo(userinfo)
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_user_info(), "some user info");
    }

    #[test]
    fn client_get_steam_id() {
        let mut client = ClientBuilder::default()
            .steam_id(1234)
            .build()
            .expect("this should not happen");
        let rust_client =
            Client::try_from(client.borrow_mut() as *mut client_t).expect("this should not happen");
        assert_eq!(rust_client.get_steam_id(), 1234);
    }
}
