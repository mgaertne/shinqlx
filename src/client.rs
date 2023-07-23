use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::{
    ClientNotFound, InvalidId, NullPointerPassed,
};
use crate::quake_types::{clientState_t, client_t, MAX_CLIENTS};
use crate::server_static::ServerStatic;
use crate::MAIN_ENGINE;
use std::ffi::{c_char, CStr, CString};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Client {
    pub(crate) client_t: &'static mut client_t,
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
            .ok_or(NullPointerPassed("null pointer passed".into()))
    }
}

impl TryFrom<i32> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        if let Ok(max_clients) = i32::try_from(MAX_CLIENTS) {
            if client_id >= max_clients {
                return Err(InvalidId(client_id));
            }
        }

        if client_id < 0 {
            return Err(InvalidId(client_id));
        }

        let server_static = ServerStatic::default();
        Self::try_from(unsafe {
            server_static
                .serverStatic_t
                .clients
                .offset(client_id as isize)
        } as *mut client_t)
        .map_err(|_| ClientNotFound("client not found".into()))
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        let server_static = ServerStatic::default();
        unsafe {
            (self.client_t as *const client_t).offset_from(server_static.serverStatic_t.clients)
        }
        .try_into()
        .unwrap_or(-1)
    }

    pub(crate) fn get_state(&self) -> clientState_t {
        self.client_t.state
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect(&mut self, reason: &str) {
        let c_reason = CString::new(reason).unwrap_or(CString::new("").unwrap());

        let Ok(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let Ok(detour) = main_engine.sv_dropclient_detour() else {
            return;
        };

        detour.call(self.client_t, c_reason.as_ptr());
    }

    pub(crate) fn get_name(&self) -> String {
        if self.client_t.name.as_ptr().is_null() {
            return "".into();
        }
        unsafe { CStr::from_ptr(&self.client_t.name as *const c_char) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn get_user_info(&self) -> String {
        if self.client_t.userinfo.as_ptr().is_null() {
            return "".into();
        };
        unsafe { CStr::from_ptr(self.client_t.userinfo.as_ptr()) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }
}

#[cfg(test)]
pub(crate) mod client_tests {
    use crate::client::Client;
    #[cfg(not(miri))]
    use crate::quake_live_engine::QuakeLiveEngine;
    use crate::quake_live_engine::QuakeLiveEngineError::{InvalidId, NullPointerPassed};
    use crate::quake_types::clientState_t::CS_ZOMBIE;
    use crate::quake_types::{
        client_t, sharedEntity_t, ClientBuilder, SharedEntityBuilder, MAX_INFO_STRING,
        MAX_NAME_LENGTH,
    };
    #[cfg(not(miri))]
    use crate::MAIN_ENGINE;
    use pretty_assertions::assert_eq;
    use std::ffi::c_char;
    #[cfg(not(miri))]
    use test_context::{test_context, TestContext};

    #[cfg(not(miri))]
    struct QuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for QuakeLiveEngineContext {
        fn setup() -> Self {
            let main_engine = QuakeLiveEngine::new();

            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = Some(main_engine);

            Self {}
        }

        fn teardown(self) {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = None;
        }
    }

    #[cfg(not(miri))]
    struct NoQuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for NoQuakeLiveEngineContext {
        fn setup() -> Self {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = None;

            Self {}
        }
    }

    #[test]
    pub(crate) fn client_try_from_null_results_in_error() {
        assert_eq!(
            Client::try_from(std::ptr::null() as *const client_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn client_try_from_valid_client() {
        let mut client = ClientBuilder::default().build().unwrap();
        assert_eq!(Client::try_from(&mut client as *mut client_t).is_ok(), true);
    }

    #[test]
    pub(crate) fn client_try_from_negative_client_id() {
        assert_eq!(Client::try_from(-1), Err(InvalidId(-1)));
    }

    #[test]
    pub(crate) fn client_try_from_too_large_client_id() {
        assert_eq!(Client::try_from(32384), Err(InvalidId(32384)));
    }

    #[test]
    pub(crate) fn client_get_state() {
        let mut client = ClientBuilder::default().state(CS_ZOMBIE).build().unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_state(), CS_ZOMBIE);
    }

    #[test]
    pub(crate) fn client_has_gentity_with_no_shared_entity() {
        let mut client = ClientBuilder::default()
            .gentity(std::ptr::null_mut())
            .build()
            .unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.has_gentity(), false);
    }

    #[test]
    pub(crate) fn client_has_gentity_with_valid_shared_entity() {
        let mut shared_entity = SharedEntityBuilder::default().build().unwrap();
        let mut client = ClientBuilder::default()
            .gentity(&mut shared_entity as *mut sharedEntity_t)
            .build()
            .unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.has_gentity(), true);
    }

    #[cfg(not(miri))]
    #[test_context(NoQuakeLiveEngineContext)]
    #[test]
    pub(crate) fn client_disconnect_with_no_main_engine(_ctx: &mut NoQuakeLiveEngineContext) {
        let mut client = ClientBuilder::default().build().unwrap();
        let mut rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        rust_client.disconnect("disconnected");
    }

    #[cfg(not(miri))]
    #[test_context(QuakeLiveEngineContext)]
    #[test]
    pub(crate) fn client_disconnect_with_no_detour_setup(_ctx: &mut QuakeLiveEngineContext) {
        let mut client = ClientBuilder::default().build().unwrap();
        let mut rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        rust_client.disconnect("disconnected");
    }

    #[test]
    pub(crate) fn client_get_name_from_null() {
        let mut client = ClientBuilder::default()
            .name([0; MAX_NAME_LENGTH as usize])
            .build()
            .unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_name(), "");
    }

    #[test]
    pub(crate) fn client_get_name_from_valid_name() {
        let player_name_str = "UnknownPlayer";
        let mut bytes_iter = player_name_str.bytes();
        let mut player_name: [c_char; MAX_NAME_LENGTH as usize] = [0; MAX_NAME_LENGTH as usize];
        player_name[0..player_name_str.len()].fill_with(|| bytes_iter.next().unwrap() as c_char);
        let mut client = ClientBuilder::default().name(player_name).build().unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_name(), "UnknownPlayer");
    }

    #[test]
    pub(crate) fn client_get_userinfo_from_null() {
        let mut client = ClientBuilder::default()
            .userinfo([0; MAX_INFO_STRING as usize])
            .build()
            .unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_user_info(), "");
    }

    #[test]
    pub(crate) fn client_get_userinfo_from_valid_userinfo() {
        let user_info_str = "some user info";
        let mut bytes_iter = user_info_str.bytes();
        let mut userinfo: [c_char; MAX_INFO_STRING as usize] = [0; MAX_INFO_STRING as usize];
        userinfo[0..user_info_str.len()].fill_with(|| bytes_iter.next().unwrap() as c_char);
        let mut client = ClientBuilder::default().userinfo(userinfo).build().unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_user_info(), "some user info");
    }

    #[test]
    pub(crate) fn client_get_steam_id() {
        let mut client = ClientBuilder::default().steam_id(1234).build().unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_steam_id(), 1234);
    }
}
