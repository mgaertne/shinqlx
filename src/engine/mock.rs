use core::ffi::{c_char, c_int};

use detour::TypedDetour;
use mockall::predicate;

use crate::{
    composition::ENGINE_CALLBACKS,
    engine::{callbacks::EngineCallbacks, prelude::*, *},
};

#[cfg(not(tarpaulin_include))]
mockall::mock! {
    pub(crate) QuakeEngine{
        pub(crate) fn with_callbacks(callbacks: &'static EngineCallbacks) -> Self;
        pub(crate) fn callbacks(&self) -> &'static EngineCallbacks;
        pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn hook_static(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn is_common_initialized(&self) -> bool;
        pub(crate) fn get_max_clients(&self) -> i32;
        pub(crate) fn initialize_static(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn set_tag(&self);
        pub(crate) fn initialize_cvars(&self);
        pub(crate) fn unhook_vm(&self, restart: bool);
        pub(crate) fn g_init_game_orig(
            &self,
        ) -> Result<extern "C" fn(c_int, c_int, c_int), QuakeLiveEngineError>;
        pub(crate) fn touch_item_orig(
            &self,
        ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>;
        #[cfg(feature = "race-hooks")]
        pub(crate) fn race_point_touch_orig(
            &self,
        ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>;
        pub(crate) fn g_free_entity_orig(
            &self,
        ) -> Result<extern "C" fn(*mut gentity_t), QuakeLiveEngineError>;
        pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError>;
        pub(crate) fn launch_item_orig(
            &self,
        ) -> Result<
            extern "C" fn(*mut gitem_t, &mut vec3_t, &mut vec3_t) -> *mut gentity_t,
            QuakeLiveEngineError,
        >;
        #[allow(clippy::type_complexity, reason = "spells out the detour type of the engine function")]
        pub(crate) fn sv_dropclient_detour(&self) -> Result<&'static TypedDetour<fn(*mut client_t, *const c_char)>, QuakeLiveEngineError>;
        pub(crate) fn sv_shutdown_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError>;
    }
    impl AddCommand<&str> for QuakeEngine {
        fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
    }
    impl SetModuleOffset<&str> for QuakeEngine {
        fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
    }
    impl InitGame<c_int, c_int, c_int> for QuakeEngine {
        fn init_game(&self, level_time: c_int, random_seed: c_int, restart: c_int);
    }
    impl ShutdownGame<c_int> for QuakeEngine {
        fn shutdown_game(&self, restart: c_int);
    }
    impl ExecuteClientCommand<Client, &str, qboolean> for QuakeEngine {
        fn execute_client_command(&self, client: Option<Client>, cmd: &str, client_ok: qboolean);
    }
    impl SendServerCommand<Client> for QuakeEngine {
        fn send_server_command(&self, client: Option <Client>, cmd: &str);
    }
    impl ClientEnterWorld<&mut Client> for QuakeEngine {
        fn client_enter_world(&self, client: &mut Client, cmd: * mut usercmd_t);
    }
    impl HandleSteamPacket for QuakeEngine {
        fn handle_steam_packet(&self, from: *const netadr_t, msg: *const msg_t) -> bool;
    }
    impl SetConfigstring<c_int> for QuakeEngine {
        fn set_configstring(&self, index: c_int, value: &str);
    }
    impl ComPrintf for QuakeEngine {
        fn com_printf(&self, msg: &str);
    }
    impl SpawnServer<&str, bool> for QuakeEngine {
        fn spawn_server(&self, server_str: &str, kill_bots: bool);
    }
    impl RunFrame<c_int> for QuakeEngine {
        fn run_frame(&self, time: c_int);
    }
    impl ClientConnect<c_int, bool, bool> for QuakeEngine {
        fn client_connect(&self, client_num: c_int, first_time: bool, is_bot: bool) -> *const c_char;
    }
    impl ClientSpawn<&mut GameEntity> for QuakeEngine {
        fn client_spawn(&self, ent: &mut GameEntity);
    }
    impl SetCvarUndetected for QuakeEngine {
        fn set_cvar_undetected(&self, name: *const c_char, value: *const c_char, force: qboolean) -> *mut cvar_t;
    }
    impl FireWeaponOriginal for QuakeEngine {
        fn fire_weapon(&self, ent: *mut gentity_t);
    }
    impl StatsMedalOriginal for QuakeEngine {
        fn stats_medal(&self, ent: *mut gentity_t, medal: *const c_char, total: c_int);
    }
    impl StatsFlagStatusOriginal for QuakeEngine {
        fn stats_flag_status(&self, team: c_int, status: c_int, carrier: *mut gentity_t);
    }
    impl TouchItem for QuakeEngine {
        fn touch_item(&self, item: *mut gentity_t, toucher: *mut gentity_t, trace: *mut trace_t);
    }
    impl RegisterDamage<c_int, c_int, c_int> for QuakeEngine {
        #[allow(clippy::too_many_arguments, reason = "mirrors the signature of the engine function")]
        fn register_damage(&self, target: *mut gentity_t, inflictor: *mut gentity_t, attacker: *mut gentity_t, dir: *mut vec3_t, pos: *mut vec3_t, damage: c_int, dflags: c_int, means_of_death: c_int);
    }
    impl TryLaunchItem<&mut qagame::game_item::GameItem> for QuakeEngine {
        #[allow(clippy::extra_unused_lifetimes, reason = "the mock repeats the lifetime of the trait signature")]
        fn try_launch_item<'a>(&self, gitem: &'a mut qagame::game_item::GameItem, origin: &mut vec3_t, velocity: &mut vec3_t) -> Result<GameEntity, QuakeLiveEngineError>;
    }
    impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
        fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
    }
    impl CmdArgs for QuakeEngine {
        fn cmd_args(&self) -> Option<String>;
    }
    impl CmdArgc for QuakeEngine {
        fn cmd_argc(&self) -> i32;
    }
    impl CmdArgv<i32> for QuakeEngine {
        fn cmd_argv(&self, argno: i32) -> Option<String>;
    }
    impl StartKamikaze<&mut qagame::game_entity::GameEntity> for QuakeEngine {
        fn start_kamikaze(&self, mut gentity: &mut qagame::game_entity::GameEntity);
    }
    impl FreeEntity<&mut qagame::game_entity::GameEntity> for QuakeEngine {
        fn free_entity(&self, mut gentity: &mut qagame::game_entity::GameEntity);
    }
    impl LinkEntity<&mut qagame::game_entity::GameEntity> for QuakeEngine {
        fn link_entity(&self, mut gentity: &mut qagame::game_entity::GameEntity) -> bool;
    }
    impl SpawnEntityFromVars for QuakeEngine {
        fn spawn_entity_from_vars(&self, vars: &[(String, String)]) -> bool;
    }
    impl GetConfigstring<u16> for QuakeEngine {
        fn get_configstring(&self, index: u16) -> String;
    }
    impl ConsoleCommand<&str> for QuakeEngine {
        fn execute_console_command(&self, cmd: &str);
    }
    impl FindCVar<&str> for QuakeEngine {
        fn find_cvar(&self, name: &str) -> Option<CVar>;
    }
    impl GetCVar<&str, &str, i32> for QuakeEngine {
        fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
    }
    impl SetCVarForced<&str, &str, bool> for QuakeEngine {
        fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar>;
    }
    impl SetCVarLimit<&str, &str, &str, &str, i32> for QuakeEngine {
        fn set_cvar_limit(&self, name: &str, value: &str, min: &str, max: &str, flags: Option<i32>) -> Option<CVar>;
    }
}

pub(crate) struct MockEngineBuilder {
    mock_engine: Option<MockQuakeEngine>,
}

impl MockEngineBuilder {
    pub(crate) fn configure<F>(mut self, setup: F) -> Self
    where
        F: FnOnce(&mut MockQuakeEngine),
    {
        self.mock_engine.as_mut().map(setup);
        self
    }

    pub(crate) fn with_max_clients(self, max_clients: i32) -> Self {
        self.configure(|mock_engine| {
            mock_engine
                .expect_get_max_clients()
                .return_const(max_clients);
        })
    }

    pub(crate) fn with_com_printf<F, G>(self, predicate: F, times: G) -> Self
    where
        F: mockall::Predicate<str> + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine.expect_com_printf().with(predicate).times(times);
        })
    }

    pub(crate) fn with_send_server_command<F, G>(self, matcher: F, times: G) -> Self
    where
        F: Fn(&Option<MockClient>, &str) -> bool + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_send_server_command()
                .withf(matcher)
                .times(times);
        })
    }

    pub(crate) fn with_execute_client_command<F, G>(self, matcher: F, times: G) -> Self
    where
        F: Fn(&Option<MockClient>, &str, &qboolean) -> bool + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_execute_client_command()
                .withf(matcher)
                .times(times);
        })
    }

    pub(crate) fn with_execute_console_command<F, G>(self, expected_cmd: F, times: G) -> Self
    where
        F: ToString + Send + Sync + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_execute_console_command()
                .withf(move |cmd| cmd == expected_cmd.to_string())
                .times(times);
        })
    }

    pub(crate) fn with_argc(self, argc: i32) -> Self {
        self.configure(|mock_engine| {
            mock_engine.expect_cmd_argc().return_const_st(argc);
        })
    }

    pub(crate) fn with_argv<F, G, H>(self, argv: F, opt_return: Option<G>, times: H) -> Self
    where
        F: mockall::Predicate<i32> + Send + 'static,
        G: ToString + Sync + Send + 'static,
        H: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_cmd_argv()
                .with(argv)
                .return_const_st(opt_return.map(move |return_str| return_str.to_string()))
                .times(times);
        })
    }

    pub(crate) fn with_args<F>(self, opt_return: Option<&'static str>, times: F) -> Self
    where
        F: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_cmd_args()
                .return_const_st(opt_return.map(move |return_str| return_str.to_string()))
                .times(times);
        })
    }

    pub(crate) fn with_find_cvar<F, G, H>(self, expect: F, returned: G, times: H) -> Self
    where
        F: Fn(&str) -> bool + Send + 'static,
        G: FnMut(&str) -> Option<CVar> + 'static,
        H: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .withf(expect)
                .returning_st(returned)
                .times(times);
        })
    }

    pub(crate) fn with_get_configstring<F, G>(self, matcher: u16, returned: F, times: G) -> Self
    where
        F: ToString + Send + Sync + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_get_configstring()
                .with(predicate::eq(matcher))
                .returning(move |_| returned.to_string())
                .times(times);
        })
    }

    pub(crate) fn run<F>(&mut self, execute: F)
    where
        F: FnOnce(),
    {
        let engine = self.mock_engine.take();
        crate::MAIN_ENGINE.store(engine.map(|mock_engine| mock_engine.into()));
        let _reset = MainEngineReset;
        execute();
    }
}

/// Removes the mocked engine on the thread that created it, also when the test panics.
struct MainEngineReset;

impl Drop for MainEngineReset {
    fn drop(&mut self) {
        crate::MAIN_ENGINE.store(None);
    }
}

impl Default for MockEngineBuilder {
    fn default() -> Self {
        let mut mock_engine = MockQuakeEngine::default();
        mock_engine
            .expect_callbacks()
            .return_const(&ENGINE_CALLBACKS);
        MockEngineBuilder {
            mock_engine: Some(mock_engine),
        }
    }
}

#[mockall::automock]
#[allow(
    dead_code,
    reason = "stand-ins for engine symbols that not every test calls"
)]
pub(crate) mod quake_functions {
    use core::{
        ffi::{c_char, c_float, c_int},
        ptr,
    };

    use crate::engine::prelude::{
        cbufExec_t, client_t, cvar_t, entity_event_t, gentity_t, gitem_t, msg_t, netadr_t,
        qboolean, trace_t, usercmd_t, vec3_t,
    };

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_HandleSteamPacket(
        _from: *const netadr_t,
        _msg: *const msg_t,
    ) -> c_int {
        0
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_HandleSteamPacket(
        _from: *const netadr_t,
        _msg: *const msg_t,
    ) -> c_int {
        0
    }

    #[allow(
        unused_attributes,
        clippy::just_underscores_and_digits,
        non_snake_case,
        reason = "mock stand-in for the variadic engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn Com_Printf(_fmt: *const c_char, _: ...) {}

    #[allow(
        unused_attributes,
        clippy::just_underscores_and_digits,
        non_snake_case,
        reason = "mock stand-in for the variadic engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn detoured_Com_Printf(_fmt: *const c_char, _: ...) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_AddCommand(_cmd: *const c_char, _func: unsafe extern "C" fn()) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_Cmd_AddCommand(
        _cmd: *const c_char,
        _func: unsafe extern "C" fn(),
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Args() -> *const c_char {
        ptr::null()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Argv(_arg: c_int) -> *const c_char {
        ptr::null()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Tokenizestring(_text_in: *const c_char) -> *const c_char {
        ptr::null()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cbuf_ExecuteText(_exec_when: cbufExec_t, _text: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_FindVar(_var_name: *const c_char) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_Get(
        _var_name: *const c_char,
        _var_value: *const c_char,
        _flags: c_int,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_GetLimit(
        _var_name: *const c_char,
        _var_value: *const c_char,
        _min: *const c_char,
        _max: *const c_char,
        _flags: c_int,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_Set2(
        _var_name: *const c_char,
        _value: *const c_char,
        _force: qboolean,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        clippy::just_underscores_and_digits,
        non_snake_case,
        reason = "mock stand-in for the variadic engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn SV_SendServerCommand(
        _cl: *mut client_t,
        _fmt: *const c_char,
        _: ...
    ) {
    }

    #[allow(
        unused_attributes,
        clippy::just_underscores_and_digits,
        non_snake_case,
        reason = "mock stand-in for the variadic engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn detoured_SV_SendServerCommand(
        _cl: *mut client_t,
        _fmt: *const c_char,
        _: ...
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_ExecuteClientCommand(
        _cl: *mut client_t,
        _s: *const c_char,
        _clientOK: qboolean,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_ExecuteClientCommand(
        _cl: *mut client_t,
        _s: *const c_char,
        _clientOK: qboolean,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_Shutdown(_finalmsg: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_Map_f() {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_ClientEnterWorld(_client: *mut client_t, _cmd: *mut usercmd_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_ClientEnterWorld(
        _client: *mut client_t,
        _cmd: *mut usercmd_t,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_SetConfigstring(_index: c_int, _value: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_SetConfigstring(_index: c_int, _value: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_GetConfigstring(
        _index: c_int,
        _buffer: *mut c_char,
        _bufferSize: c_int,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_DropClient(_drop: *mut client_t, _reason: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_DropClient(_drop: *mut client_t, _reason: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Sys_SetModuleOffset(
        _moduleName: *mut c_char,
        _offset: unsafe extern "C" fn(),
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_Sys_SetModuleOffset(
        _moduleName: *mut c_char,
        _offset: unsafe extern "C" fn(),
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_SpawnServer(_server: *mut c_char, _killBots: qboolean) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_Cvar_Set2(
        _var_name: *const c_char,
        _value: *const c_char,
        _force: qboolean,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_SpawnServer(_server: *mut c_char, _killBots: qboolean) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_ExecuteString(_text: *const c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Argc() -> c_int {
        0
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_InitGame(_level_time: c_int, _random_see: c_int, _restart: c_int) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_ShutdownGame(_restart: c_int) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_RunFrame(_time: c_int) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn ClientConnect(
        _client_num: c_int,
        _first_time: qboolean,
        _is_bot: qboolean,
    ) -> *const c_char {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_ClientConnect(
        _client_num: c_int,
        _first_time: qboolean,
        _is_bot: qboolean,
    ) -> *const c_char {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn ClientSpawn(_client: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_ClientSpawn(_client: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_AddEvent(
        _ent: *mut gentity_t,
        _event: entity_event_t,
        _eventParm: c_int,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_Damage(
        _target: *mut gentity_t,
        _inflictor: *mut gentity_t,
        _attacker: *mut gentity_t,
        _dir: *mut vec3_t,
        _pos: *mut vec3_t,
        _damage: c_int,
        _dflags: c_int,
        _means_of_death: c_int,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_G_Damage(
        _target: *mut gentity_t,
        _inflictor: *mut gentity_t,
        _attacker: *mut gentity_t,
        _dir: *mut vec3_t,
        _pos: *mut vec3_t,
        _damage: c_int,
        _dflags: c_int,
        _means_of_death: c_int,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_FreeEntity(_ent: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn LaunchItem(
        _item: *mut gitem_t,
        _origin: *mut vec3_t,
        _velocity: *mut vec3_t,
    ) -> *mut gentity_t {
        ptr::null_mut()
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_StartKamikaze(_ent: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_G_StartKamikaze(_ent: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn CheckPrivileges(_ent: *mut gentity_t, _cmd: *mut c_char) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn FireWeapon(_ent: *mut gentity_t) {}

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_StatsMedal(
        _ent: *mut gentity_t,
        _medal: *const c_char,
        _total: c_int,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_StatsFlagStatus(
        _team: c_int,
        _status: c_int,
        _carrier: *mut gentity_t,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Touch_Item(
        _ent: *mut gentity_t,
        _other: *mut gentity_t,
        _trace: *mut trace_t,
    ) {
    }

    #[allow(
        unused_attributes,
        non_snake_case,
        reason = "mock stand-in for the engine function of the same C name"
    )]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Drop_Item(
        _ent: *mut gentity_t,
        _item: *mut gitem_t,
        _angle: c_float,
    ) -> *mut gentity_t {
        ptr::null_mut()
    }
}
