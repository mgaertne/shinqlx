use crate::hooks::SV_DROPCLIENT_DETOUR;
use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::{
    ClientNotFound, InvalidId, NullPointerPassed,
};
use crate::quake_types::{clientState_t, client_t, serverStatic_t, MAX_CLIENTS};
use std::ffi::{c_char, CStr, CString};

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct Client {
    pub(crate) client_t: &'static client_t,
}

impl TryFrom<*const client_t> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client: *const client_t) -> Result<Self, Self::Error> {
        unsafe {
            client
                .as_ref()
                .map(|client_t| Self { client_t })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl TryFrom<i32> for Client {
    type Error = QuakeLiveEngineError;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        if client_id < 0 || client_id >= i32::try_from(MAX_CLIENTS).unwrap() {
            return Err(InvalidId(client_id));
        }
        unsafe {
            Self::try_from(
                svs.as_ref().unwrap().clients.offset(client_id as isize) as *const client_t
            )
            .map_err(|_| ClientNotFound("client not found".into()))
        }
    }
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        extern "C" {
            static svs: *mut serverStatic_t;
        }

        unsafe {
            i32::try_from(
                (self.client_t as *const client_t).offset_from(svs.as_ref().unwrap().clients),
            )
            .unwrap()
        }
    }

    pub(crate) fn get_state(&self) -> clientState_t {
        self.client_t.state
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect(&self, reason: &str) {
        let c_reason = CString::new(reason).unwrap_or(CString::new("").unwrap());
        unsafe { SV_DROPCLIENT_DETOUR.call(self.client_t, c_reason.as_ptr()) };
    }

    pub(crate) fn get_name(&self) -> String {
        if self.client_t.name.as_ptr().is_null() {
            "".into()
        } else {
            unsafe {
                CStr::from_ptr(&self.client_t.name as *const c_char)
                    .to_string_lossy()
                    .into()
            }
        }
    }

    pub(crate) fn get_user_info(&self) -> String {
        if self.client_t.userinfo.as_ptr().is_null() {
            "".into()
        } else {
            unsafe {
                CStr::from_ptr(self.client_t.userinfo.as_ptr())
                    .to_string_lossy()
                    .into()
            }
        }
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }
}

#[cfg(test)]
pub(crate) mod client_tests {
    use crate::client::Client;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::clientState_t::CS_ZOMBIE;
    use crate::quake_types::{
        client_t, sharedEntity_t, ClientBuilder, SharedEntityBuilder, MAX_INFO_STRING,
        MAX_NAME_LENGTH,
    };
    use pretty_assertions::assert_eq;
    use std::ffi::c_char;

    #[test]
    pub(crate) fn client_try_from_null_results_in_error() {
        assert_eq!(
            Client::try_from(std::ptr::null_mut() as *const client_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn client_try_from_valid_client() {
        let client = ClientBuilder::default().build().unwrap();
        assert_eq!(Client::try_from(&client as *const client_t).is_ok(), true);
    }

    #[test]
    pub(crate) fn client_get_state() {
        let client = ClientBuilder::default().state(CS_ZOMBIE).build().unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_state(), CS_ZOMBIE);
    }

    #[test]
    pub(crate) fn client_has_gentity_with_no_shared_entity() {
        let client = ClientBuilder::default()
            .gentity(std::ptr::null_mut() as *mut sharedEntity_t)
            .build()
            .unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.has_gentity(), false);
    }

    #[test]
    pub(crate) fn client_has_gentity_with_valid_shared_entity() {
        let mut shared_entity = SharedEntityBuilder::default().build().unwrap();
        let client = ClientBuilder::default()
            .gentity(&mut shared_entity as *mut sharedEntity_t)
            .build()
            .unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.has_gentity(), true);
    }

    #[test]
    pub(crate) fn client_get_name_from_null() {
        let client = ClientBuilder::default()
            .name([0; MAX_NAME_LENGTH as usize])
            .build()
            .unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_name(), "");
    }

    #[test]
    pub(crate) fn client_get_name_from_valid_name() {
        let mut player_name: [c_char; MAX_NAME_LENGTH as usize] = [0; MAX_NAME_LENGTH as usize];
        for (index, char) in "UnknownPlayer".chars().enumerate() {
            player_name[index] = char.to_owned() as c_char;
        }
        let client = ClientBuilder::default().name(player_name).build().unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_name(), "UnknownPlayer");
    }

    #[test]
    pub(crate) fn client_get_userinfo_from_null() {
        let client = ClientBuilder::default()
            .userinfo([0; MAX_INFO_STRING as usize])
            .build()
            .unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_user_info(), "");
    }

    #[test]
    pub(crate) fn client_get_userinfo_from_valid_userinfo() {
        let mut userinfo: [c_char; MAX_INFO_STRING as usize] = [0; MAX_INFO_STRING as usize];
        for (index, char) in "some user info".chars().enumerate() {
            userinfo[index] = char.to_owned() as c_char;
        }
        let client = ClientBuilder::default().userinfo(userinfo).build().unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_user_info(), "some user info");
    }

    #[test]
    pub(crate) fn client_get_steam_id() {
        let client = ClientBuilder::default().steam_id(1234).build().unwrap();
        let rust_client = Client::try_from(&client as *const client_t).unwrap();
        assert_eq!(rust_client.get_steam_id(), 1234);
    }
}