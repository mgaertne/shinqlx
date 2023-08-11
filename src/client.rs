use crate::prelude::*;
use crate::server_static::ServerStatic;
use crate::MAIN_ENGINE;
use alloc::ffi::CString;
use alloc::string::String;
use core::ffi::{c_char, CStr};

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
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl TryFrom<i32> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        if let Ok(max_clients) = i32::try_from(MAX_CLIENTS) {
            if client_id >= max_clients {
                return Err(QuakeLiveEngineError::InvalidId(client_id));
            }
        }

        if client_id < 0 {
            return Err(QuakeLiveEngineError::InvalidId(client_id));
        }

        let server_static = ServerStatic::try_get()?;
        Self::try_from(unsafe {
            server_static
                .serverStatic_t
                .clients
                .offset(client_id as isize)
        } as *mut client_t)
        .map_err(|_| QuakeLiveEngineError::ClientNotFound("client not found".into()))
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        let Ok(server_static) = ServerStatic::try_get() else {
            return -1;
        };
        self._get_client_id_internal(server_static)
    }

    fn _get_client_id_internal(&self, server_static: ServerStatic) -> i32 {
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

    pub(crate) fn disconnect<T>(&mut self, reason: T)
    where
        T: AsRef<str>,
    {
        let c_reason = CString::new(reason.as_ref()).unwrap_or(CString::new("").unwrap());

        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
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
        unsafe { CStr::from_ptr(&self.client_t.name as *const c_char) }
            .to_string_lossy()
            .into()
    }

    pub(crate) fn get_user_info(&self) -> String {
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
    use crate::prelude::*;
    use crate::server_static::ServerStatic;
    use crate::MAIN_ENGINE;
    use core::ffi::c_char;
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    #[test]
    pub(crate) fn client_try_from_null_results_in_error() {
        assert_eq!(
            Client::try_from(core::ptr::null() as *const client_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    pub(crate) fn client_try_from_valid_client() {
        let mut client = ClientBuilder::default().build().unwrap();
        assert_eq!(Client::try_from(&mut client as *mut client_t).is_ok(), true);
    }

    #[test]
    pub(crate) fn client_try_from_negative_client_id() {
        assert_eq!(
            Client::try_from(-1),
            Err(QuakeLiveEngineError::InvalidId(-1))
        );
    }

    #[test]
    pub(crate) fn client_try_from_too_large_client_id() {
        assert_eq!(
            Client::try_from(32384),
            Err(QuakeLiveEngineError::InvalidId(32384))
        );
    }

    #[test]
    pub(crate) fn client_get_client_id_when_no_serverstatic_found() {
        let mut client = ClientBuilder::default().build().unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_client_id(), -1);
    }

    #[test]
    pub(crate) fn client_get_client_id_interal_from_server_static() {
        let mut client = ClientBuilder::default().build().unwrap();
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut client as *mut client_t)
            .build()
            .unwrap();
        let rust_server_static =
            ServerStatic::try_from(&mut server_static as *mut serverStatic_t).unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client._get_client_id_internal(rust_server_static), 0);
    }

    #[test]
    pub(crate) fn client_get_client_id_interal_from_server_static_not_first_position() {
        let mut clients = vec![
            ClientBuilder::default().build().unwrap(),
            ClientBuilder::default().build().unwrap(),
            ClientBuilder::default().build().unwrap(),
        ];
        let mut server_static = ServerStaticBuilder::default()
            .clients(&mut clients[0])
            .build()
            .unwrap();
        let rust_server_static =
            ServerStatic::try_from(&mut server_static as *mut serverStatic_t).unwrap();
        let rust_client = Client::try_from(&mut clients[2] as *mut client_t).unwrap();
        assert_eq!(rust_client._get_client_id_internal(rust_server_static), 2);
    }

    #[test]
    pub(crate) fn client_get_state() {
        let mut client = ClientBuilder::default()
            .state(clientState_t::CS_ZOMBIE)
            .build()
            .unwrap();
        let rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        assert_eq!(rust_client.get_state(), clientState_t::CS_ZOMBIE);
    }

    #[test]
    pub(crate) fn client_has_gentity_with_no_shared_entity() {
        let mut client = ClientBuilder::default()
            .gentity(core::ptr::null_mut())
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

    #[test]
    #[serial]
    pub(crate) fn client_disconnect_with_no_main_engine() {
        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }

        let mut client = ClientBuilder::default().build().unwrap();
        let mut rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        rust_client.disconnect("disconnected");
    }

    #[test]
    #[serial]
    pub(crate) fn client_disconnect_with_no_detour_setup() {
        {
            let mut guard = MAIN_ENGINE.write();
            *guard = Some(QuakeLiveEngine::new());
        }

        let mut client = ClientBuilder::default().build().unwrap();
        let mut rust_client = Client::try_from(&mut client as *mut client_t).unwrap();
        rust_client.disconnect("disconnected");

        {
            let mut guard = MAIN_ENGINE.write();
            *guard = None;
        }
    }

    #[test]
    pub(crate) fn client_get_name_from_empty_name() {
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
