#[cfg(not(test))]
use crate::client::Client;
#[cfg(test)]
use crate::commands::mock_python::{
    new_game_dispatcher, pyminqlx_initialize, pyminqlx_is_initialized, pyminqlx_reload,
    rcon_dispatcher,
};
#[cfg(test)]
use crate::commands::MockClient as Client;
#[cfg(test)]
use crate::commands::MockGameEntity as GameEntity;
#[cfg(not(test))]
use crate::game_entity::GameEntity;
use crate::prelude::*;
use crate::pyminqlx::CUSTOM_COMMAND_HANDLER;
#[cfg(not(test))]
use crate::pyminqlx::{
    new_game_dispatcher, pyminqlx_initialize, pyminqlx_is_initialized, pyminqlx_reload,
    rcon_dispatcher,
};
use crate::quake_live_engine::{
    CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand,
};
use crate::MAIN_ENGINE;
#[cfg(test)]
use mockall::{automock, mock};
use pyo3::{Py, PyAny, Python};
use rand::Rng;

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_send_server_command_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_send_server_command_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("{}\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_center_print_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_center_print_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("cp \"{}\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_regular_print_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_regular_print_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("print \"{}\n\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    cmd_slap_intern(maxclients, main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_slap_intern<T, U>(maxclients: T, main_engine: &U)
where
    T: Into<i32> + Copy,
    U: CmdArgc
        + CmdArgv<i32>
        + ComPrintf<String>
        + for<'a> GameAddEvent<&'a mut GameEntity, i32>
        + SendServerCommand<Client, String>,
{
    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    };

    if client_id < 0 || client_id >= maxclients.into() {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = main_engine.cmd_argv(2).unwrap_or("0");
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    let Ok(mut client_entity) = GameEntity::try_from(client_id) else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n".into());
        return;
    }

    main_engine.com_printf("Slapping...\n".into());

    let Ok(client) = Client::try_from(client_id) else {
        return;
    };
    let message = if dmg != 0 {
        format!(
            "print \"{}^7 was slapped for {} damage!\n\"\n",
            client.get_name(),
            dmg
        )
    } else {
        format!("print \"{}^7 was slapped\n\"\n", client.get_name())
    };

    main_engine.send_server_command(None::<Client>, message);

    let mut rng = rand::thread_rng();
    let Ok(mut game_client) = client_entity.get_game_client() else {
        return;
    };
    game_client.set_velocity((
        (rng.gen_range(-1.0..=1.0) * 200.0),
        (rng.gen_range(-1.0..=1.0) * 200.0),
        300.0,
    ));
    if dmg > 0 {
        let old_health = client_entity.get_health();
        client_entity.set_health(old_health - dmg);
        if old_health - dmg <= 0 {
            let client_number = client_entity.get_client_number();
            main_engine.game_add_event(
                &mut client_entity,
                entity_event_t::EV_DEATH1,
                client_number,
            );
            return;
        }
    }
    main_engine.game_add_event(&mut client_entity, entity_event_t::EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    cmd_slay_intern(maxclients, main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_slay_intern<T, U>(maxclients: T, main_engine: &U)
where
    T: Into<i32> + Copy,
    U: CmdArgc
        + CmdArgv<i32>
        + ComPrintf<String>
        + for<'a> GameAddEvent<&'a mut GameEntity, i32>
        + SendServerCommand<Client, String>,
{
    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    };

    if client_id < 0 || client_id >= maxclients.into() {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    }

    let Ok(mut client_entity) = GameEntity::try_from(client_id) else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n".into());
        return;
    }

    main_engine.com_printf("Slaying player...\n".into());

    let Ok(client) = Client::try_from(client_id) else {
        return;
    };

    main_engine.send_server_command(
        None::<Client>,
        format!("print \"{}^7 was slain!\n\"\n", client.get_name()),
    );

    client_entity.set_health(-40);
    let client_number = client_entity.get_client_number();
    main_engine.game_add_event(
        &mut client_entity,
        entity_event_t::EV_GIB_PLAYER,
        client_number,
    );
}

#[no_mangle]
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_py_rcon_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_py_rcon_intern<T>(main_engine: &T)
where
    T: CmdArgs,
{
    let Some(commands) = main_engine.cmd_args() else {
        return;
    };

    rcon_dispatcher(commands);
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    let Some(custom_command_lock) = CUSTOM_COMMAND_HANDLER.try_read() else {
        return;
    };

    let Some(ref custom_command_handler) = *custom_command_lock else {
        return;
    };

    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_py_command_intern(custom_command_handler, main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_py_command_intern<T>(custom_command_handler: &Py<PyAny>, main_engine: &T)
where
    T: CmdArgs + ComPrintf<String>,
{
    let cmd_args = main_engine.cmd_args();

    Python::with_gil(|py| {
        let result = match cmd_args {
            None => custom_command_handler.call0(py),
            Some(args) => custom_command_handler.call1(py, (args,)),
        };

        if result.is_err() || !result.unwrap().is_true(py).unwrap() {
            main_engine.com_printf(
                "The command failed to be executed. pyshinqlx found no handler.\n".into(),
            );
        }
    });
}

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_restart_python_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_restart_python_intern<T>(main_engine: &T)
where
    T: ComPrintf<String>,
{
    main_engine.com_printf("Restarting Python...\n".into());

    if pyminqlx_is_initialized() {
        if pyminqlx_reload().is_err() {
            return;
        };
        // minqlx initializes after the first new game starts, but since the game already
        // start, we manually trigger the event to make it initialize properly.
        new_game_dispatcher(false);
        return;
    }

    if pyminqlx_initialize().is_err() {
        return;
    };

    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    new_game_dispatcher(false);
}

#[cfg(test)]
#[cfg_attr(test, automock)]
#[cfg_attr(test, allow(dead_code))]
mod python {
    use crate::pyminqlx::PythonInitializationError;

    pub(crate) fn rcon_dispatcher<T>(_cmd: T)
    where
        T: AsRef<str> + 'static,
    {
    }

    pub(crate) fn new_game_dispatcher(_restart: bool) {}

    pub(crate) fn pyminqlx_is_initialized() -> bool {
        false
    }

    pub(crate) fn pyminqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }

    pub(crate) fn pyminqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}

#[cfg(test)]
mock! {
    QuakeEngine {}
    impl CmdArgs for QuakeEngine {
        fn cmd_args(&self) -> Option<String>;
    }
    impl CmdArgc for QuakeEngine {
        fn cmd_argc(&self) -> i32;
    }
    impl CmdArgv<i32> for QuakeEngine {
        fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
    }
    impl ComPrintf<String> for QuakeEngine {
        fn com_printf(&self, msg: String);
    }
    impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
        fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
    }
    impl SendServerCommand<Client, String> for QuakeEngine {
        fn send_server_command(&self, client: Option<Client>, command: String);
    }
}

#[cfg(test)]
mock! {
    pub(crate) GameClient {
        pub(crate) fn set_velocity<T>(&mut self, velocity: T)
        where
            T: Into<[f32; 3]> + 'static;
    }
}

#[cfg(test)]
mock! {
    pub(crate) GameEntity {
        pub(crate) fn get_game_client(&self) -> Result<MockGameClient, QuakeLiveEngineError>;
        pub(crate) fn in_use(&self) -> bool;
        pub(crate) fn get_health(&self) -> i32;
        pub(crate) fn set_health(&mut self, new_health: i32);
        pub(crate) fn get_client_number(&self) -> i32;
    }

    impl AsMut<gentity_t> for GameEntity {
        fn as_mut(&mut self) -> &mut gentity_t;
    }

    impl TryFrom<i32> for GameEntity {
        type Error = QuakeLiveEngineError;
        fn try_from(entity_id: i32) -> Result<Self, QuakeLiveEngineError>;
    }
}

#[cfg(test)]
mock! {
    pub(crate) Client {
        pub(crate) fn get_name(&self) -> String;
    }

    impl TryFrom<i32> for Client {
        type Error = QuakeLiveEngineError;
        fn try_from(entity_id: i32) -> Result<Self, QuakeLiveEngineError>;
    }

    impl AsRef<client_t> for Client {
        fn as_ref(&self) -> &client_t;
    }
}

#[cfg(test)]
pub(crate) mod commands_tests {
    use super::Client;
    use crate::commands::mock_python::{
        new_game_dispatcher_context, pyminqlx_initialize_context, pyminqlx_is_initialized_context,
        pyminqlx_reload_context, rcon_dispatcher_context,
    };
    use crate::commands::{
        cmd_center_print_intern, cmd_py_command_intern, cmd_py_rcon_intern,
        cmd_regular_print_intern, cmd_restart_python_intern, cmd_send_server_command_intern,
        cmd_slap_intern, cmd_slay_intern, MockClient, MockGameClient, MockGameEntity,
        MockQuakeEngine,
    };
    #[cfg(not(miri))]
    use crate::pyminqlx::pyminqlx_setup_fixture::*;
    use crate::pyminqlx::PythonInitializationError;
    use crate::quake_types::entity_event_t;
    use pyo3::types::PyModule;
    use pyo3::{IntoPy, Py, Python};
    #[cfg(not(miri))]
    use rstest::rstest;
    use serial_test::serial;

    #[test]
    fn cmd_send_server_command_with_no_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_send_server_command().times(0);

        cmd_send_server_command_intern(&mock);
    }

    #[test]
    fn cmd_send_server_command_with_server_command() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()))
            .times(1);
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "asdf\n")
            .return_once_st(|_, _| ())
            .times(1);

        cmd_send_server_command_intern(&mock);
    }

    #[test]
    fn cmd_center_print_with_no_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_send_server_command().times(0);

        cmd_center_print_intern(&mock);
    }

    #[test]
    fn cmd_center_print_with_server_command() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()))
            .times(1);
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "cp \"asdf\"\n")
            .return_once_st(|_, _| ())
            .times(1);

        cmd_center_print_intern(&mock);
    }

    #[test]
    fn cmd_regular_print_with_no_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_send_server_command().times(0);

        cmd_regular_print_intern(&mock);
    }

    #[test]
    fn cmd_regular_print_with_server_command() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()))
            .times(1);
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "print \"asdf\n\"\n")
            .return_once_st(|_, _| ())
            .times(1);

        cmd_regular_print_intern(&mock);
    }

    #[test]
    fn cmd_slap_with_too_few_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 1).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 0)
            .return_once_st(|_| Some("!slap"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "Usage: !slap <client_id> [damage]\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_unparseable_client_id() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2147483648"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_too_small_client_id() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("-1"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_too_large_client_id() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("42"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_not_in_use() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "The player is currently not active.\n")
            .return_once_st(|_| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| false)
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_no_health() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "The player is currently not active.\n")
            .return_once_st(|_| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| true)
                    .times(1);
                game_entity_mock
                    .expect_get_health()
                    .returning_st(|| 0)
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_no_damage_provided_slaps() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "Slapping...\n")
            .return_once_st(|_| ())
            .times(1);
        mock.expect_send_server_command()
            .withf_st(|client, cmd| {
                client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
            })
            .return_once_st(|_, _| ())
            .times(1);
        mock.expect_game_add_event()
            .withf_st(|_, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .return_once_st(|_, _, _| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| true)
                    .times(1);
                game_entity_mock
                    .expect_get_health()
                    .returning_st(|| 200)
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .returning_st(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .return_once_st(|_| ())
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        let client_try_from_ctx = Client::try_from_context();
        client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .returning_st(|| "Slapped Player".into())
                    .times(1);
                Ok(client_mock)
            })
            .times(1);
        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_slaps() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 3).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 2)
            .return_once_st(|_| Some("1"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "Slapping...\n")
            .return_once_st(|_| ())
            .times(1);
        mock.expect_send_server_command()
            .withf_st(|client, cmd| {
                client.is_none()
                    && cmd == "print \"Slapped Player^7 was slapped for 1 damage!\n\"\n"
            })
            .return_once_st(|_, _| ())
            .times(1);
        mock.expect_game_add_event()
            .withf_st(|_, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .return_once_st(|_, _, _| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| true)
                    .times(1);
                game_entity_mock
                    .expect_get_health()
                    .returning_st(|| 200)
                    .times(1..);
                game_entity_mock
                    .expect_set_health()
                    .withf_st(|&health| health == 199)
                    .return_once_st(|_| ())
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .returning_st(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .return_once_st(|_| ())
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        let client_try_from_ctx = Client::try_from_context();
        client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .returning_st(|| "Slapped Player".into())
                    .times(1);
                Ok(client_mock)
            })
            .times(1);
        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_provided_slaps_and_kills() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 3).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 2)
            .return_once_st(|_| Some("666"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "Slapping...\n")
            .return_once_st(|_| ())
            .times(1);
        mock.expect_send_server_command()
            .withf_st(|client, cmd| {
                client.is_none()
                    && cmd == "print \"Slapped Player^7 was slapped for 666 damage!\n\"\n"
            })
            .return_once_st(|_, _| ())
            .times(1);
        mock.expect_game_add_event()
            .withf_st(|_, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_DEATH1 && event_param == 42
            })
            .return_once_st(|_, _, _| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| true)
                    .times(1);
                game_entity_mock
                    .expect_get_health()
                    .returning_st(|| 200)
                    .times(1..);
                game_entity_mock
                    .expect_set_health()
                    .withf_st(|&health| health == -466)
                    .return_once_st(|_| ())
                    .times(1);
                game_entity_mock.expect_get_game_client().returning_st(|| {
                    let mut game_client_mock = MockGameClient::default();
                    game_client_mock
                        .expect_set_velocity::<(f32, f32, f32)>()
                        .return_once_st(|_| ())
                        .times(1);
                    Ok(game_client_mock)
                });
                game_entity_mock
                    .expect_get_client_number()
                    .return_once_st(|| 42)
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        let client_try_from_ctx = Client::try_from_context();
        client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .returning_st(|| "Slapped Player".into())
                    .times(1);
                Ok(client_mock)
            })
            .times(1);
        cmd_slap_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slap_with_unparseable_provided_damage_provided_slaps() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 3);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"));
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 2)
            .return_once_st(|_| Some("2147483648"));
        mock.expect_com_printf()
            .withf_st(|text| text == "Slapping...\n")
            .return_once_st(|_| ());
        mock.expect_send_server_command()
            .withf_st(|client, cmd| {
                client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
            })
            .return_once_st(|_, _| ());
        mock.expect_game_add_event()
            .withf_st(|_, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .return_once_st(|_, _, _| ());

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().returning_st(|| true);
                game_entity_mock.expect_get_health().returning_st(|| 200);
                game_entity_mock.expect_set_health().return_once_st(|_| ());
                game_entity_mock.expect_get_game_client().returning_st(|| {
                    let mut game_client_mock = MockGameClient::default();
                    game_client_mock
                        .expect_set_velocity::<(f32, f32, f32)>()
                        .return_once_st(|_| ());
                    Ok(game_client_mock)
                });
                game_entity_mock
                    .expect_get_client_number()
                    .return_once_st(|| 42);
                Ok(game_entity_mock)
            });
        let client_try_from_ctx = Client::try_from_context();
        client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .returning_st(|| "Slapped Player".into());
                Ok(client_mock)
            });
        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slay_with_too_few_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 1).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 0)
            .return_once_st(|_| Some("!slap"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "Usage: !slap <client_id> [damage]\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slay_intern(16, &mock);
    }

    #[test]
    fn cmd_slay_with_unparseable_client_id() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2147483648"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slay_intern(16, &mock);
    }

    #[test]
    fn cmd_slay_with_too_small_client_id() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("-1"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slay_intern(16, &mock);
    }

    #[test]
    fn cmd_slay_with_too_large_client_id() {
        let mut mock = MockQuakeEngine::new();

        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("42"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_once_st(|_| ())
            .times(1);

        cmd_slay_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_not_in_use() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2).times(1);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"))
            .times(1);
        mock.expect_com_printf()
            .withf_st(|text| text == "The player is currently not active.\n")
            .return_once_st(|_| ())
            .times(1);

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .returning_st(|| false)
                    .times(1);
                Ok(game_entity_mock)
            })
            .times(1);
        cmd_slay_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_no_health() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"));
        mock.expect_com_printf()
            .withf_st(|text| text == "The player is currently not active.\n")
            .return_once_st(|_| ());

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().returning_st(|| true);
                game_entity_mock.expect_get_health().returning_st(|| 0);
                Ok(game_entity_mock)
            });
        cmd_slay_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_slay_player_is_slain() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2);
        mock.expect_cmd_argv()
            .withf_st(|&argv| argv == 1)
            .return_once_st(|_| Some("2"));
        mock.expect_com_printf()
            .withf_st(|text| text == "Slaying player...\n")
            .return_once_st(|_| ());
        mock.expect_send_server_command()
            .withf_st(|client, cmd| {
                client.is_none() && cmd == "print \"Slain Player^7 was slain!\n\"\n"
            })
            .return_once_st(|_, _| ());
        mock.expect_game_add_event()
            .withf_st(|_, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_GIB_PLAYER && event_param == 42
            })
            .return_once_st(|_, _, _| ());

        let game_client_try_from_ctx = MockGameEntity::try_from_context();
        game_client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().returning_st(|| true);
                game_entity_mock.expect_get_health().returning_st(|| 200);
                game_entity_mock
                    .expect_set_health()
                    .withf(|&new_health| new_health < 0)
                    .return_once_st(|_| ());
                game_entity_mock
                    .expect_get_client_number()
                    .return_once_st(|| 42);
                Ok(game_entity_mock)
            });
        let client_try_from_ctx = Client::try_from_context();
        client_try_from_ctx
            .expect()
            .withf_st(|&client_id| client_id == 2)
            .returning_st(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .returning_st(|| "Slain Player".into());
                Ok(client_mock)
            });
        cmd_slay_intern(16, &mock);
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_with_no_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);

        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx.expect::<&str>().times(0);

        cmd_py_rcon_intern(&mock);
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_forwards_args() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("!version".into()))
            .times(1);

        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx
            .expect::<String>()
            .withf_st(|cmd| cmd == "!version")
            .return_once_st(|_| ())
            .times(1);

        cmd_py_rcon_intern(&mock);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn cmd_py_command_with_arguments(_pyminqlx_setup: ()) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("custom parameter".into()))
            .times(1);
        mock.expect_com_printf().times(0);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler(params):
    return (params == "custom parameter")
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let custom_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        cmd_py_command_intern(&custom_command_handler, &mock);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn cmd_py_command_with_no_args(_pyminqlx_setup: ()) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_com_printf().times(0);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    return True
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let custom_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        cmd_py_command_intern(&custom_command_handler, &mock);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn cmd_py_command_returns_error(_pyminqlx_setup: ()) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_com_printf()
            .withf_st(|text| {
                text == "The command failed to be executed. pyshinqlx found no handler.\n"
            })
            .return_once_st(|_| ())
            .times(1);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    raise Exception 
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let custom_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        cmd_py_command_intern(&custom_command_handler, &mock);
    }

    #[cfg_attr(not(miri), rstest)]
    #[serial]
    fn cmd_py_command_returns_false(_pyminqlx_setup: ()) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None).times(1);
        mock.expect_com_printf()
            .withf_st(|text| {
                text == "The command failed to be executed. pyshinqlx found no handler.\n"
            })
            .return_once_st(|_| ())
            .times(1);

        let pymodule: Py<PyModule> = Python::with_gil(|py| {
            PyModule::from_code(
                py,
                r#"
def handler():
    return False 
"#,
                "",
                "",
            )
            .unwrap()
            .into_py(py)
        });
        let custom_command_handler =
            Python::with_gil(|py| pymodule.getattr(py, "handler").unwrap().into_py(py));

        cmd_py_command_intern(&custom_command_handler, &mock);
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_com_printf()
            .withf_st(|text| text == "Restarting Python...\n")
            .return_once_st(|_| ())
            .times(1);

        let pyminqlx_is_initialized_ctx = pyminqlx_is_initialized_context();
        pyminqlx_is_initialized_ctx
            .expect()
            .times(1)
            .return_once_st(|| true)
            .times(1);
        let pyminqlx_reload_ctx = pyminqlx_reload_context();
        pyminqlx_reload_ctx
            .expect()
            .times(1)
            .return_once_st(|| Ok(()));
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .times(1)
            .withf_st(|&new_game| !new_game)
            .return_once_st(|_| ())
            .times(1);

        cmd_restart_python_intern(&mock);
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized_reload_fails() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_com_printf()
            .withf_st(|text| text == "Restarting Python...\n")
            .return_once_st(|_| ())
            .times(1);

        let pyminqlx_is_initialized_ctx = pyminqlx_is_initialized_context();
        pyminqlx_is_initialized_ctx
            .expect()
            .return_once_st(|| true)
            .times(1);
        let pyminqlx_reload_ctx = pyminqlx_reload_context();
        pyminqlx_reload_ctx
            .expect()
            .return_once_st(|| Err(PythonInitializationError::NotInitializedError))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        cmd_restart_python_intern(&mock);
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_com_printf()
            .withf_st(|text| text == "Restarting Python...\n")
            .return_once_st(|_| ())
            .times(1);

        let pyminqlx_is_initialized_ctx = pyminqlx_is_initialized_context();
        pyminqlx_is_initialized_ctx
            .expect()
            .return_once_st(|| false)
            .times(1);
        let pyminqlx_initialize_ctx = pyminqlx_initialize_context();
        pyminqlx_initialize_ctx
            .expect()
            .return_once_st(|| Ok(()))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .withf_st(|&value| !value)
            .return_once_st(|_| ())
            .times(1);

        cmd_restart_python_intern(&mock);
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized_initialize_fails() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_com_printf()
            .withf_st(|text| text == "Restarting Python...\n")
            .return_once_st(|_| ())
            .times(1);

        let pyminqlx_is_initialized_ctx = pyminqlx_is_initialized_context();
        pyminqlx_is_initialized_ctx
            .expect()
            .return_once_st(|| false)
            .times(1);
        let pyminqlx_initialize_ctx = pyminqlx_initialize_context();
        pyminqlx_initialize_ctx
            .expect()
            .return_once_st(|| Err(PythonInitializationError::MainScriptError))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        cmd_restart_python_intern(&mock);
    }
}
