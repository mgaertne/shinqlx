//! Guard against the recursive cleanup of disconnected ZMQ RCON clients in the engine.
//!
//! The engine sends every console line to all RCON clients. A client whose send fails is erased from
//! the client set, the engine remembers the next entry, and reports the deletion with `Com_Printf`.
//! That call enters the same loop again, which erases the remembered entry as soon as it is dead as
//! well. Two or more dead clients at the same time leave the outer loop with a freed entry and crash
//! the server. Dropping the report before it reaches `Com_Printf` avoids the recursion.

const DISCONNECT_NOTICE_PREFIX: &str = "zmq RCON client disconnected: ";

/// Whether the console line is the engine's report about an erased RCON client.
pub(crate) fn is_disconnect_notice(msg: &str) -> bool {
    msg.starts_with(DISCONNECT_NOTICE_PREFIX)
}

#[cfg(test)]
mod rcon_disconnect_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::is_disconnect_notice;

    #[rstest]
    #[case("zmq RCON client disconnected: 4a3fc7789df54d46b162bef0b2182d0e", true)]
    #[case("zmq RCON client disconnected: ", true)]
    #[case("zmq RCON client connected: 4a3fc7789df54d46b162bef0b2182d0e", false)]
    #[case("zmq RCON command from 4a3f: map campgrounds", false)]
    #[case("player zmq RCON client disconnected: x", false)]
    #[case("", false)]
    fn recognises_only_the_disconnect_report(#[case] msg: &str, #[case] expected: bool) {
        assert_eq!(is_disconnect_notice(msg), expected);
    }
}
