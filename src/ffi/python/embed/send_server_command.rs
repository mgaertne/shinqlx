use alloc::borrow::Cow;

use tap::TryConv;

use super::validate_client_id;
use crate::ffi::{c::prelude::*, python::prelude::*};
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_send_server_command;
#[cfg(not(test))]
use crate::hooks::shinqlx_send_server_command;

const FULL_WIDTH_PERCENT: char = '\u{ff05}';
const DISPLAYED_COMMANDS: [&str; 4] = ["print", "chat", "tchat", "cp"];

/// The server treats a `%` in the text of a command as the start of a printf placeholder and does not
/// display the text. Commands that show text therefore get the full width percent sign instead.
fn without_percent_signs(cmd: &str) -> Cow<'_, str> {
    let shows_text = cmd
        .split_whitespace()
        .next()
        .is_some_and(|command| DISPLAYED_COMMANDS.contains(&command));
    if shows_text && cmd.contains('%') {
        Cow::Owned(cmd.replace('%', &FULL_WIDTH_PERCENT.to_string()))
    } else {
        Cow::Borrowed(cmd)
    }
}

/// Sends a server command to either one specific client or all the clients.
///
/// A ``%`` in commands that show text, such as ``print`` and ``cp``, is replaced by the full width percent sign
/// ``％`` (U+FF05), because the server would otherwise take it for a placeholder and not show the text.
#[pyfunction]
#[pyo3(name = "send_server_command")]
#[pyo3(signature = (client_id, cmd))]
pub(crate) fn pyshinqlx_send_server_command(
    py: Python<'_>,
    client_id: Option<i32>,
    cmd: &str,
) -> PyResult<bool> {
    let cmd = without_percent_signs(cmd);
    py.detach(|| match client_id {
        None => {
            shinqlx_send_server_command(None, cmd.as_ref());
            Ok(true)
        }
        Some(actual_client_id) => {
            validate_client_id(actual_client_id)?;

            let opt_client = actual_client_id
                .try_conv::<Client>()
                .ok()
                .filter(|client| client.get_state() == clientState_t::CS_ACTIVE);
            let returned = opt_client.is_some();
            if returned {
                shinqlx_send_server_command(opt_client, cmd.as_ref());
            }
            Ok(returned)
        }
    })
}

#[cfg(test)]
mod percent_sign_tests {
    use rstest::rstest;

    use super::without_percent_signs;

    #[rstest]
    #[case("print \"100% sure\n\"", "print \"100％ sure\n\"")]
    #[case("chat \"%s %d\"", "chat \"％s ％d\"")]
    #[case("tchat \"50%\"", "tchat \"50％\"")]
    #[case("cp \"%n\"", "cp \"％n\"")]
    #[case("print \"no placeholder\n\"", "print \"no placeholder\n\"")]
    #[case("cs 0 \"a%b\"", "cs 0 \"a%b\"")]
    #[case("clearSounds", "clearSounds")]
    #[case("", "")]
    fn percent_signs_are_replaced_in_commands_that_show_text(
        #[case] cmd: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(without_percent_signs(cmd), expected);
    }
}

#[cfg(test)]
mod send_server_command_tests {
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        hooks::mock_hooks::shinqlx_send_server_command_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_replaces_percent_signs_in_displayed_text(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "print \"100\u{ff05} sure\n\"")
            .times(1);
        let result =
            Python::attach(|py| pyshinqlx_send_server_command(py, None, "print \"100% sure\n\""));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_no_client_id(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "asdf")
            .times(1);
        let result = Python::attach(|py| pyshinqlx_send_server_command(py, None, "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        Python::attach(|py| {
            let result = pyshinqlx_send_server_command(py, Some(0), "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_userinfo_for_client_id_below_zero(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_send_server_command(py, Some(-1), "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_client_id_above_max_clients(_pyshinqlx_setup: ()) {
        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::attach(|py| {
                let result = pyshinqlx_send_server_command(py, Some(42), "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_active_client(_pyshinqlx_setup: ()) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "asdf")
            .times(1);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::attach(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
            assert_eq!(result.expect("result was not OK"), true);
        });
    }

    #[rstest]
    #[case(clientState_t::CS_FREE)]
    #[case(clientState_t::CS_CONNECTED)]
    #[case(clientState_t::CS_PRIMED)]
    #[case(clientState_t::CS_ZOMBIE)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_for_non_active_free_client(
        #[case] clientstate: clientState_t,
        _pyshinqlx_setup: (),
    ) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx.expect().returning(move |_client_id| {
            let mut mock_client = MockClient::new();
            mock_client.expect_get_state().return_const(clientstate);
            mock_client
        });

        let hook_ctx = shinqlx_send_server_command_context();
        hook_ctx.expect().times(0);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::attach(|py| pyshinqlx_send_server_command(py, Some(2), "asdf"));
            assert_eq!(result.expect("result was not OK"), false);
        });
    }
}
