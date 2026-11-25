use alloc::borrow::Cow;
use core::{hint::cold_path, sync::atomic::Ordering};

use pyo3::types::{PyBool, PyString};

use super::{
    events::event_wanted, events_enum::EventName, prelude::*, stats_listener::handle_zmq_msg,
};

pub(crate) fn client_command_dispatcher(client_id: i32, cmd: &str) -> Option<Cow<'_, str>> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return Some(Cow::Borrowed(cmd));
    }

    Python::attach(|py| handle_client_command_outcome(py, client_id, cmd).into_text(cmd))
}

pub(crate) fn server_command_dispatcher(client_id: Option<i32>, cmd: &str) -> Option<Cow<'_, str>> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return Some(Cow::Borrowed(cmd));
    }

    Python::attach(|py| {
        handle_server_command_outcome(py, client_id.unwrap_or(-1), cmd).into_text(cmd)
    })
}

pub(crate) fn frame_dispatcher() {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    let _ = Python::attach(handle_frame);
}

pub(crate) fn client_connect_dispatcher(client_id: i32, is_bot: bool) -> Option<String> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return None;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::Release);
    }

    let returned: Option<String> = Python::attach(|py| {
        let result = handle_player_connect(py, client_id, is_bot).into_bound(py);

        match result.cast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => {
                Some("You are banned from this server.".to_string())
            }
            _ => result
                .cast::<PyString>()
                .ok()
                .map(|py_string| py_string.to_string_lossy().into_owned()),
        }
    });

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(
            allowed_clients & !(1 << client_id as u64),
            Ordering::Release,
        );
    }

    returned
}

pub(crate) fn client_disconnect_dispatcher<T>(client_id: i32, reason: T)
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(allowed_clients | (1 << client_id as u64), Ordering::Release);
    }

    Python::attach(|py| handle_player_disconnect(py, client_id, Some(reason.as_ref().to_string())));

    {
        let allowed_clients = ALLOW_FREE_CLIENT.load(Ordering::Acquire);
        ALLOW_FREE_CLIENT.store(
            allowed_clients & !(1 << client_id as u64),
            Ordering::Release,
        );
    }
}

pub(crate) fn client_loaded_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| handle_player_loaded(py, client_id));
}

pub(crate) fn new_game_dispatcher(restart: bool) {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    let _ = Python::attach(|py| handle_new_game(py, restart));
}

pub(crate) fn set_configstring_dispatcher<T>(index: T, value: &str) -> Option<Cow<'_, str>>
where
    T: Into<u32>,
{
    if !pyshinqlx_is_initialized() {
        cold_path();
        return Some(Cow::Borrowed(value));
    }

    Python::attach(|py| handle_set_configstring_outcome(py, index.into(), value).into_text(value))
}

pub(crate) fn rcon_dispatcher<T>(cmd: T)
where
    T: AsRef<str>,
{
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| handle_rcon(py, cmd.as_ref()));
}

pub(crate) fn console_print_dispatcher(text: &str) -> Option<Cow<'_, str>> {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return Some(Cow::Borrowed(text));
    }

    Python::attach(|py| handle_console_print_outcome(py, text).into_text(text))
}

pub(crate) fn client_spawn_dispatcher(client_id: i32) {
    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| handle_player_spawn(py, client_id));
}

pub(crate) fn kamikaze_use_dispatcher(client_id: i32) {
    if !event_wanted(EventName::KamikazeUse) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| handle_kamikaze_use(py, client_id));
}

/// Hands the stats messages that the game module produced since the last frame to the plugins.
pub(crate) fn stats_feed_dispatcher() {
    let messages = crate::stats_feed::take_pending();
    if messages.is_empty() {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        messages
            .iter()
            .for_each(|message| handle_zmq_msg(py, message));
    });
}

pub(crate) fn weapon_fired_dispatcher(client_id: i32, weapon: i32) {
    if !event_wanted(EventName::WeaponFired) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_weapon_fired(py, client_id, weapon);
    });
}

pub(crate) fn medal_dispatcher(client_id: i32, medal: &str, total: i32) {
    if !event_wanted(EventName::Medal) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_medal(py, client_id, medal, total);
    });
}

pub(crate) fn flag_status_dispatcher(team: i32, status: i32, carrier_id: Option<i32>) {
    if !event_wanted(EventName::FlagStatus) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_flag_status(py, team, status, carrier_id);
    });
}

pub(crate) fn objective_dispatcher(client_id: i32, kind: &str, count: i32) {
    if !event_wanted(EventName::Objective) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_objective(py, client_id, kind, count);
    });
}

pub(crate) fn cvar_changed_dispatcher(cvar_name: &str, old_value: &str, new_value: &str) {
    if !event_wanted(EventName::CvarChanged) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_cvar_changed(py, cvar_name, old_value, new_value);
    });
}

pub(crate) fn item_pickup_dispatcher(client_id: i32, entity_id: i32, classname: &str) {
    if !event_wanted(EventName::ItemPickup) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        let _ = handle_item_pickup(py, client_id, entity_id, classname);
    });
}

/// Whether the player may go on touching the item, `false` if a handler stopped the event.
pub(crate) fn item_touch_dispatcher(client_id: i32, entity_id: i32, classname: &str) -> bool {
    if !event_wanted(EventName::ItemTouch) {
        return true;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return true;
    }

    Python::attach(|py| {
        let result = handle_item_touch(py, client_id, entity_id, classname).into_bound(py);
        !result
            .cast::<PyBool>()
            .is_ok_and(|py_bool| !py_bool.is_true())
    })
}

pub(crate) fn kamikaze_explode_dispatcher(client_id: i32, is_used_on_demand: bool) {
    if !event_wanted(EventName::KamikazeExplode) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| handle_kamikaze_explode(py, client_id, is_used_on_demand));
}

pub(crate) fn damage_dispatcher(
    target_client_id: i32,
    attacker_client_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) {
    if !event_wanted(EventName::Damage) {
        return;
    }

    if !pyshinqlx_is_initialized() {
        cold_path();
        return;
    }

    Python::attach(|py| {
        handle_damage(
            py,
            target_client_id,
            attacker_client_id,
            damage,
            dflags,
            means_of_death,
        )
    });
}

#[cfg(test)]
mod pyshinqlx_dispatcher_tests {
    use alloc::borrow::Cow;

    use pretty_assertions::assert_eq;
    use pyo3::{
        IntoPyObjectExt,
        exceptions::PyException,
        types::{PyBool, PyString, PyTuple},
    };
    use rstest::*;

    use super::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        cvar_changed_dispatcher, damage_dispatcher, flag_status_dispatcher, frame_dispatcher,
        item_pickup_dispatcher, item_touch_dispatcher, kamikaze_explode_dispatcher,
        kamikaze_use_dispatcher, medal_dispatcher, new_game_dispatcher, objective_dispatcher,
        rcon_dispatcher, server_command_dispatcher, set_configstring_dispatcher,
        stats_feed_dispatcher, weapon_fired_dispatcher,
    };
    use crate::{
        ffi::{c::prelude::*, python::prelude::*},
        prelude::*,
    };

    #[test]
    #[serial]
    fn stats_feed_dispatcher_without_messages_does_not_touch_python() {
        let _ = crate::stats_feed::take_pending();
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().times(0);

        stats_feed_dispatcher();
    }

    #[test]
    #[serial]
    fn stats_feed_dispatcher_drops_messages_when_python_not_initiailized() {
        let _ = crate::stats_feed::take_pending();
        crate::stats_feed::queue_message("{}".to_string());
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        stats_feed_dispatcher();

        assert!(crate::stats_feed::take_pending().is_empty());
    }

    #[test]
    #[serial]
    fn weapon_fired_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_weapon_fired_ctx = handle_weapon_fired_context();
        handle_weapon_fired_ctx.expect().times(0);

        weapon_fired_dispatcher(123, 5);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn weapon_fired_dispatcher_hands_the_weapon_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_weapon_fired_ctx = handle_weapon_fired_context();
        handle_weapon_fired_ctx
            .expect()
            .withf(|_, client_id, weapon| *client_id == 123 && *weapon == 5)
            .returning(|py, _, _| py.None())
            .times(1);

        weapon_fired_dispatcher(123, 5);
    }

    #[test]
    #[serial]
    fn item_pickup_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_item_pickup_ctx = handle_item_pickup_context();
        handle_item_pickup_ctx.expect().times(0);

        item_pickup_dispatcher(123, 42, "item_armor_body");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn item_pickup_dispatcher_hands_the_item_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_item_pickup_ctx = handle_item_pickup_context();
        handle_item_pickup_ctx
            .expect()
            .withf(|_, client_id, entity_id, classname| {
                *client_id == 123 && *entity_id == 42 && classname == "item_armor_body"
            })
            .returning(|py, _, _, _| py.None())
            .times(1);

        item_pickup_dispatcher(123, 42, "item_armor_body");
    }

    #[test]
    #[serial]
    fn medal_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_medal_ctx = handle_medal_context();
        handle_medal_ctx.expect().times(0);

        medal_dispatcher(123, "IMPRESSIVE", 2);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn medal_dispatcher_hands_medal_and_total_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_medal_ctx = handle_medal_context();
        handle_medal_ctx
            .expect()
            .withf(|_, client_id, medal, total| {
                *client_id == 123 && medal == "EXCELLENT" && *total == 2
            })
            .returning(|py, _, _, _| py.None())
            .times(1);

        medal_dispatcher(123, "EXCELLENT", 2);
    }

    #[test]
    #[serial]
    fn flag_status_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_flag_status_ctx = handle_flag_status_context();
        handle_flag_status_ctx.expect().times(0);

        flag_status_dispatcher(1, 1, Some(123));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flag_status_dispatcher_hands_flag_and_carrier_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_flag_status_ctx = handle_flag_status_context();
        handle_flag_status_ctx
            .expect()
            .withf(|_, team, status, carrier_id| *team == 2 && *status == 0 && carrier_id.is_none())
            .returning(|py, _, _, _| py.None())
            .times(1);

        flag_status_dispatcher(2, 0, None);
    }

    #[test]
    #[serial]
    fn objective_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_objective_ctx = handle_objective_context();
        handle_objective_ctx.expect().times(0);

        objective_dispatcher(123, "capture", 2);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn objective_dispatcher_hands_kind_and_count_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_objective_ctx = handle_objective_context();
        handle_objective_ctx
            .expect()
            .withf(|_, client_id, kind, count| {
                *client_id == 123 && kind == "frag_carrier" && *count == 2
            })
            .returning(|py, _, _, _| py.None())
            .times(1);

        objective_dispatcher(123, "frag_carrier", 2);
    }

    #[test]
    #[serial]
    fn cvar_changed_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_cvar_changed_ctx = handle_cvar_changed_context();
        handle_cvar_changed_ctx.expect().times(0);

        cvar_changed_dispatcher("sv_hostname", "old", "new");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvar_changed_dispatcher_hands_the_values_to_the_handler(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_cvar_changed_ctx = handle_cvar_changed_context();
        handle_cvar_changed_ctx
            .expect()
            .withf(|_, name, old, new| name == "sv_hostname" && old == "old" && new == "new")
            .returning(|py, _, _, _| py.None())
            .times(1);

        cvar_changed_dispatcher("sv_hostname", "old", "new");
    }

    #[test]
    #[serial]
    fn item_touch_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_item_touch_ctx = handle_item_touch_context();
        handle_item_touch_ctx.expect().times(0);

        assert!(item_touch_dispatcher(123, 42, "item_armor_body"));
    }

    #[rstest]
    #[case(true, true)]
    #[case(false, false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn item_touch_dispatcher_follows_the_handler(
        _pyshinqlx_setup: (),
        #[case] handler_result: bool,
        #[case] expected: bool,
    ) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_item_touch_ctx = handle_item_touch_context();
        handle_item_touch_ctx
            .expect()
            .returning(move |py, _, _, _| {
                PyBool::new(py, handler_result)
                    .to_owned()
                    .into_any()
                    .unbind()
            });

        assert_eq!(item_touch_dispatcher(123, 42, "item_armor_body"), expected);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn item_touch_dispatcher_ignores_other_return_values(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_item_touch_ctx = handle_item_touch_context();
        handle_item_touch_ctx
            .expect()
            .returning(|py, _, _, _| py.None());

        assert!(item_touch_dispatcher(123, 42, "item_armor_body"));
    }

    #[test]
    #[serial]
    fn client_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx.expect().times(0);

        let result = client_command_dispatcher(123, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Unchanged);

        let result = client_command_dispatcher(123, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Replaced("qwertz".to_owned()));

        let result = client_command_dispatcher(123, "asdf");
        assert!(matches!(result, Some(Cow::Owned(ref text)) if text == "qwertz"));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Accepted);

        let result = client_command_dispatcher(123, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Blocked);

        let result = client_command_dispatcher(123, "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_client_command_ctx = handle_client_command_outcome_context();
        handle_client_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Ignored);

        let result = client_command_dispatcher(123, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[test]
    #[serial]
    fn server_command_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx.expect().times(0);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Unchanged);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Replaced("qwertz".to_owned()));

        let result = server_command_dispatcher(Some(123), "asdf");
        assert!(matches!(result, Some(Cow::Owned(ref text)) if text == "qwertz"));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Accepted);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Blocked);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn server_command_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_server_command_ctx = handle_server_command_outcome_context();
        handle_server_command_ctx
            .expect()
            .returning(|_, _, _| Outcome::Ignored);

        let result = server_command_dispatcher(Some(123), "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[test]
    #[serial]
    fn frame_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_frame_ctx = handle_frame_context();
        handle_frame_ctx.expect().times(0);

        frame_dispatcher();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn frame_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_frame_ctx = handle_frame_context();
        handle_frame_ctx.expect().returning(|_| None);

        frame_dispatcher();
    }

    #[test]
    #[serial]
    fn client_connect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().times(0);

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_connection_status(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyString::intern(py, "qwertz").into_any().unbind());

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, Some("qwertz".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, true).to_owned().into_any().unbind());

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx
            .expect()
            .returning(|py, _, _| PyBool::new(py, false).to_owned().into_any().unbind());

        let result = client_connect_dispatcher(42, true);
        assert_eq!(result, Some("You are banned from this server.".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().returning(|py, _, _| {
            PyException::new_err("asdf")
                .into_py_any(py)
                .expect("this should not happen")
        });

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let player_connect_handler_ctx = handle_player_connect_context();
        player_connect_handler_ctx.expect().returning(|py, _, _| {
            PyTuple::new(py, [1, 2, 3])
                .expect("this should not happen")
                .into_any()
                .unbind()
        });

        let result = client_connect_dispatcher(42, false);
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn client_disconnect_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx.expect().times(0);

        client_disconnect_dispatcher(42, "asdf");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx
            .expect()
            .returning(|py, _, _| py.None());

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_disconnect_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_disconnect_ctx = handle_player_disconnect_context();
        handle_player_disconnect_ctx.expect().returning(|py, _, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

        client_disconnect_dispatcher(42, "ragequit");
    }

    #[test]
    #[serial]
    fn client_loaded_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx.expect().times(0);

        client_loaded_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx
            .expect()
            .returning(|py, _| py.None());

        client_loaded_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_loaded_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_loaded_ctx = handle_player_loaded_context();
        handle_player_loaded_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

        client_loaded_dispatcher(123);
    }

    #[test]
    #[serial]
    fn new_game_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_new_game_ctx = handle_new_game_context();
        handle_new_game_ctx.expect().times(0);

        new_game_dispatcher(false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn new_game_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_new_game_ctx = handle_new_game_context();
        handle_new_game_ctx.expect().returning(|_, _| None);

        new_game_dispatcher(false);
    }

    #[test]
    #[serial]
    fn set_configstring_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx.expect().times(0);

        let result = set_configstring_dispatcher(666u32, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|_, _, _| Outcome::Unchanged);

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|_, _, _| Outcome::Replaced("qwertz".to_owned()));

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert!(matches!(result, Some(Cow::Owned(ref text)) if text == "qwertz"));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|_, _, _| Outcome::Accepted);

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|_, _, _| Outcome::Blocked);

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_set_configstring_ctx = handle_set_configstring_outcome_context();
        handle_set_configstring_ctx
            .expect()
            .returning(|_, _, _| Outcome::Ignored);

        let result = set_configstring_dispatcher(123u32, "asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[test]
    #[serial]
    fn rcon_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_rcon_ctx = handle_rcon_context();
        handle_rcon_ctx.expect().times(0);

        rcon_dispatcher("asdf");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_rcon_ctx = handle_rcon_context();
        handle_rcon_ctx.expect();

        rcon_dispatcher("asdf");
    }

    #[test]
    #[serial]
    fn console_print_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx.expect().times(0);

        let result = console_print_dispatcher("asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_original_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx
            .expect()
            .returning(|_, _| Outcome::Unchanged);

        let result = console_print_dispatcher("asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_another_cmd(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx
            .expect()
            .returning(|_, _| Outcome::Replaced("qwertz".to_owned()));

        let result = console_print_dispatcher("asdf");
        assert!(matches!(result, Some(Cow::Owned(ref text)) if text == "qwertz"));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_boolean_true(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx
            .expect()
            .returning(|_, _| Outcome::Accepted);

        let result = console_print_dispatcher("asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx
            .expect()
            .returning(|_, _| Outcome::Blocked);

        let result = console_print_dispatcher("asdf");
        assert_eq!(result, None);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_print_dispatcher_dispatcher_returns_not_supported_value(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_console_print_ctx = handle_console_print_outcome_context();
        handle_console_print_ctx
            .expect()
            .returning(|_, _| Outcome::Ignored);

        let result = console_print_dispatcher("asdf");
        assert!(matches!(result, Some(Cow::Borrowed("asdf"))));
    }

    #[test]
    #[serial]
    fn client_spawn_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx.expect().times(0);

        client_spawn_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx
            .expect()
            .returning(|py, _| py.None());

        client_spawn_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_player_spawn_ctx = handle_player_spawn_context();
        handle_player_spawn_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

        client_spawn_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_use_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();
        handle_kamikaze_use_ctx.expect().times(0);

        kamikaze_use_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();
        handle_kamikaze_use_ctx
            .expect()
            .returning(|py, _| py.None());

        kamikaze_use_dispatcher(123);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_use_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_use_ctx = handle_kamikaze_use_context();

        handle_kamikaze_use_ctx.expect().returning(|py, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });
        kamikaze_use_dispatcher(123);
    }

    #[test]
    #[serial]
    fn kamikaze_explode_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx.expect().times(0);

        kamikaze_explode_dispatcher(123, false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx
            .expect()
            .returning(|py, _, _| py.None());

        kamikaze_explode_dispatcher(123, false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kamikaze_explode_dispatcher_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_kamikaze_explode_ctx = handle_kamikaze_explode_context();
        handle_kamikaze_explode_ctx.expect().returning(|py, _, _| {
            PyException::new_err("")
                .into_py_any(py)
                .expect("this should not happen")
        });

        kamikaze_explode_dispatcher(123, true);
    }

    #[test]
    #[serial]
    fn damage_dispatcher_when_python_not_initiailized() {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| false);

        let handle_damage_ctx = handle_damage_context();
        handle_damage_ctx.expect().times(0);

        damage_dispatcher(
            123,
            None,
            666,
            DAMAGE_NO_PROTECTION as i32,
            meansOfDeath_t::MOD_TRIGGER_HURT as i32,
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn damage_dispatcher_dispatcher_works_properly(_pyshinqlx_setup: ()) {
        let is_initialized_context = pyshinqlx_is_initialized_context();
        is_initialized_context.expect().returning(|| true);

        let handle_damage_ctx = handle_damage_context();
        handle_damage_ctx
            .expect()
            .returning(|_, _, _, _, _, _| None);

        damage_dispatcher(
            123,
            Some(456),
            100,
            DAMAGE_NO_TEAM_PROTECTION as i32,
            meansOfDeath_t::MOD_ROCKET as i32,
        );
    }
}
