//! Guards against the way the engine handles dead ZMQ RCON clients.
//!
//! The engine sends every console line to all RCON clients through a ROUTER socket with
//! `ZMQ_ROUTER_MANDATORY`. A client whose send fails is erased from the client set, the engine
//! remembers the next entry, and reports the deletion with `Com_Printf`.
//!
//! * That call enters the same loop again, which erases the remembered entry as soon as it is dead as
//!   well. Two or more dead clients at the same time leave the outer loop with a freed entry and crash
//!   the server. Dropping the report before it reaches `Com_Printf` avoids the recursion.
//! * The send blocks without a limit while the pipe to a dead client does not fail, and the server
//!   stops in `poll` of the ZMQ library. A send timeout on the socket turns the wait into a failure,
//!   which the engine treats like any other dead client.

use core::{
    ffi::{c_int, c_void},
    fmt, ptr,
    sync::atomic::{AtomicI32, Ordering},
};
use std::sync::OnceLock;

use log::{debug, info, warn};
#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;

use crate::engine::FindCVar;
#[cfg(target_os = "linux")]
use crate::engine::QuakeLiveEngineError;
#[cfg(target_os = "linux")]
use crate::engine::patterns::{QuakeLiveFunction, pattern_search_module};

const DISCONNECT_NOTICE_PREFIX: &str = "zmq RCON client disconnected: ";

/// Name of the cvar that sets the send timeout of the RCON socket in milliseconds.
pub(crate) const SEND_TIMEOUT_CVAR: &str = "qlx_zmqRconSendTimeout";
/// The timeout used when the cvar is not set: about one server frame at `sv_fps 40`.
pub(crate) const DEFAULT_SEND_TIMEOUT_MS: c_int = 25;

const ZMQ_SNDTIMEO: c_int = 28;
const WAIT_WITHOUT_LIMIT: c_int = -1;

#[cfg(any(target_os = "linux", test))]
const SOCKET_SLOT_DISPLACEMENT_OFFSET: usize = 27;
#[cfg(any(target_os = "linux", test))]
const SOCKET_SLOT_INSTRUCTION_END: usize = 31;
#[cfg(any(target_os = "linux", test))]
const SET_OPTION_DISPLACEMENT_OFFSET: usize = 58;
#[cfg(any(target_os = "linux", test))]
const SET_OPTION_INSTRUCTION_END: usize = 62;

type SetSocketOption = unsafe extern "C" fn(*mut c_void, c_int, *const c_void, usize) -> c_int;

/// Whether the console line is the engine's report about an erased RCON client.
pub(crate) fn is_disconnect_notice(msg: &str) -> bool {
    msg.starts_with(DISCONNECT_NOTICE_PREFIX)
}

/// The send timeout for the configured cvar value: a negative value keeps the engine's unlimited wait,
/// `0` makes the sends of the socket non-blocking.
pub(crate) fn effective_send_timeout(configured: Option<c_int>) -> c_int {
    match configured {
        None => DEFAULT_SEND_TIMEOUT_MS,
        Some(timeout) if timeout < 0 => WAIT_WITHOUT_LIMIT,
        Some(timeout) => timeout,
    }
}

/// Why the send timeout could not be set.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SendTimeoutError {
    /// The engine has not opened its RCON socket, for example because `zmq_rcon_enable` is `0`.
    SocketNotOpen,
    /// The ZMQ library refused the option.
    Rejected,
}

impl fmt::Display for SendTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SocketNotOpen => write!(f, "the RCON socket of the engine is not open"),
            Self::Rejected => write!(f, "the ZMQ library rejected the send timeout"),
        }
    }
}

/// The RCON socket of the engine, reachable through the engine's own ZMQ library.
///
/// The library is linked statically into the engine, so its functions and the pointer to the
/// socket are found in the code of the engine instead of in a shared library.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RconSocket {
    set_option: SetSocketOption,
    handle_slot: *const *mut c_void,
}

// SAFETY: the slot is a static of the engine that is only read, and the function pointer is code.
unsafe impl Send for RconSocket {}
// SAFETY: see `Send`; sharing the reference gives no access beyond reading the slot.
unsafe impl Sync for RconSocket {}

/// The address an instruction with a 32-bit displacement relative to its end refers to.
///
/// # Safety
///
/// `code + disp_offset` must be readable for four bytes.
#[cfg(any(target_os = "linux", test))]
unsafe fn relative_target(code: usize, disp_offset: usize, instruction_end: usize) -> usize {
    // SAFETY: guaranteed by the caller.
    let displacement = unsafe { ptr::read_unaligned((code + disp_offset) as *const i32) };
    let displacement = isize::try_from(displacement).unwrap_or_default();
    (code + instruction_end).wrapping_add_signed(displacement)
}

impl RconSocket {
    /// Finds the socket pointer and the option setter of the ZMQ library in the engine.
    #[cfg(target_os = "linux")]
    pub(crate) fn locate(maps: &[&MemoryMap]) -> Result<Self, QuakeLiveEngineError> {
        let socket_init = pattern_search_module(maps, QuakeLiveFunction::Rcon_SocketInit).ok_or(
            QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Rcon_SocketInit),
        )?;
        let set_mandatory =
            pattern_search_module(maps, QuakeLiveFunction::Zsock_SetRouterMandatory).ok_or(
                QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Zsock_SetRouterMandatory,
                ),
            )?;

        // SAFETY: both addresses start a pattern match in the readable code of the engine, and the
        // displacements lie inside the matched bytes.
        let (handle_slot, set_option) = unsafe {
            (
                relative_target(
                    socket_init,
                    SOCKET_SLOT_DISPLACEMENT_OFFSET,
                    SOCKET_SLOT_INSTRUCTION_END,
                ),
                relative_target(
                    set_mandatory,
                    SET_OPTION_DISPLACEMENT_OFFSET,
                    SET_OPTION_INSTRUCTION_END,
                ),
            )
        };
        Ok(Self {
            // SAFETY: the target of the call is the option setter of the ZMQ library.
            set_option: unsafe { core::mem::transmute::<usize, SetSocketOption>(set_option) },
            handle_slot: handle_slot as *const *mut c_void,
        })
    }

    /// Limits how long a send to the RCON clients may block, in milliseconds. `-1` waits without limit.
    pub(crate) fn set_send_timeout(&self, timeout_ms: c_int) -> Result<(), SendTimeoutError> {
        // SAFETY: the slot is a pointer-sized static of the engine that holds the socket or null.
        let handle = unsafe { ptr::read_volatile(self.handle_slot) };
        if handle.is_null() {
            return Err(SendTimeoutError::SocketNotOpen);
        }

        // SAFETY: the handle is the open socket of the engine, and the option takes an `int`.
        let result = unsafe {
            (self.set_option)(
                handle,
                ZMQ_SNDTIMEO,
                ptr::from_ref(&timeout_ms).cast(),
                size_of::<c_int>(),
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(SendTimeoutError::Rejected)
        }
    }
}

static RCON_SOCKET: OnceLock<RconSocket> = OnceLock::new();

/// Remembers the socket found in the engine for the later calls of [`configured_socket`].
#[cfg(target_os = "linux")]
pub(crate) fn register_socket(socket: RconSocket) {
    let _ = RCON_SOCKET.set(socket);
}

/// The socket found in the engine, if the search was successful.
pub(crate) fn configured_socket() -> Option<&'static RconSocket> {
    RCON_SOCKET.get()
}

static APPLIED_SEND_TIMEOUT: AtomicI32 = AtomicI32::new(i32::MIN);

/// Sets the send timeout of the engine's RCON socket from `qlx_zmqRconSendTimeout`.
///
/// The cvar is read at every map load, so a change takes effect with the next map.
pub(crate) fn apply_send_timeout<T>(main_engine: &T)
where
    T: FindCVar<&'static str>,
{
    let Some(socket) = configured_socket() else {
        return;
    };
    let timeout = effective_send_timeout(
        main_engine
            .find_cvar(SEND_TIMEOUT_CVAR)
            .map(|cvar| cvar.get_integer()),
    );
    match socket.set_send_timeout(timeout) {
        Ok(()) => {
            if APPLIED_SEND_TIMEOUT.swap(timeout, Ordering::AcqRel) != timeout {
                info!(target: "shinqlx", func = "apply_send_timeout"; "ZMQ RCON send timeout: {timeout} ms");
            }
        }
        Err(SendTimeoutError::SocketNotOpen) => {
            debug!(target: "shinqlx", func = "apply_send_timeout"; "ZMQ RCON is not enabled, no send timeout set");
        }
        Err(err) => {
            warn!(target: "shinqlx", func = "apply_send_timeout"; "Could not set the ZMQ RCON send timeout: {err}");
        }
    }
}

#[cfg(test)]
mod zmq_rcon_tests {
    use core::{
        ffi::{c_int, c_void},
        ptr,
        sync::atomic::{AtomicI32, AtomicUsize, Ordering},
    };

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{
        DEFAULT_SEND_TIMEOUT_MS, RconSocket, SendTimeoutError, effective_send_timeout,
        is_disconnect_notice, relative_target,
    };

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

    #[rstest]
    #[case(None, DEFAULT_SEND_TIMEOUT_MS)]
    #[case(Some(25), 25)]
    #[case(Some(100), 100)]
    #[case(Some(1), 1)]
    #[case(Some(0), 0)]
    #[case(Some(-1), -1)]
    #[case(Some(-500), -1)]
    fn send_timeout_follows_the_configured_value(
        #[case] configured: Option<c_int>,
        #[case] expected: c_int,
    ) {
        assert_eq!(effective_send_timeout(configured), expected);
    }

    #[test]
    fn default_send_timeout_is_25_milliseconds() {
        assert_eq!(DEFAULT_SEND_TIMEOUT_MS, 25);
    }

    #[test]
    fn relative_target_follows_a_forward_displacement() {
        let mut code = [0u8; 32];
        code[2..6].copy_from_slice(&20i32.to_le_bytes());
        let start = code.as_ptr() as usize;

        // SAFETY: the displacement lies inside the array.
        let target = unsafe { relative_target(start, 2, 6) };

        assert_eq!(target, start + 26);
    }

    #[test]
    fn relative_target_follows_a_backward_displacement() {
        let mut code = [0u8; 32];
        code[20..24].copy_from_slice(&(-20i32).to_le_bytes());
        let start = code.as_ptr() as usize;

        // SAFETY: the displacement lies inside the array.
        let target = unsafe { relative_target(start, 20, 24) };

        assert_eq!(target, start + 4);
    }

    static ACCEPTED_OPTION: AtomicI32 = AtomicI32::new(0);
    static ACCEPTED_VALUE: AtomicI32 = AtomicI32::new(0);
    static ACCEPTED_LENGTH: AtomicUsize = AtomicUsize::new(0);
    static ACCEPTED_HANDLE: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn accepting_setter(
        socket: *mut c_void,
        option: c_int,
        value: *const c_void,
        length: usize,
    ) -> c_int {
        ACCEPTED_HANDLE.store(socket as usize, Ordering::SeqCst);
        ACCEPTED_OPTION.store(option, Ordering::SeqCst);
        // SAFETY: the caller passes a pointer to an `int`.
        ACCEPTED_VALUE.store(unsafe { *value.cast::<c_int>() }, Ordering::SeqCst);
        ACCEPTED_LENGTH.store(length, Ordering::SeqCst);
        0
    }

    unsafe extern "C" fn refusing_setter(
        _socket: *mut c_void,
        _option: c_int,
        _value: *const c_void,
        _length: usize,
    ) -> c_int {
        -1
    }

    #[test]
    fn send_timeout_is_set_as_zmq_sndtimeo_on_the_socket_of_the_slot() {
        let socket_marker = 0x1234usize as *mut c_void;
        let slot: *mut c_void = socket_marker;
        let rcon_socket = RconSocket {
            set_option: accepting_setter,
            handle_slot: ptr::from_ref(&slot),
        };

        assert_eq!(rcon_socket.set_send_timeout(25), Ok(()));

        assert_eq!(ACCEPTED_HANDLE.load(Ordering::SeqCst), 0x1234);
        assert_eq!(ACCEPTED_OPTION.load(Ordering::SeqCst), 28);
        assert_eq!(ACCEPTED_VALUE.load(Ordering::SeqCst), 25);
        assert_eq!(ACCEPTED_LENGTH.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn send_timeout_needs_an_open_socket() {
        let slot: *mut c_void = ptr::null_mut();
        let rcon_socket = RconSocket {
            set_option: refusing_setter,
            handle_slot: ptr::from_ref(&slot),
        };

        assert_eq!(
            rcon_socket.set_send_timeout(25),
            Err(SendTimeoutError::SocketNotOpen)
        );
    }

    #[test]
    fn refused_option_is_reported() {
        let slot = ptr::dangling_mut::<c_void>();
        let rcon_socket = RconSocket {
            set_option: refusing_setter,
            handle_slot: ptr::from_ref(&slot),
        };

        assert_eq!(
            rcon_socket.set_send_timeout(25),
            Err(SendTimeoutError::Rejected)
        );
    }

    #[cfg(target_os = "linux")]
    #[repr(align(8))]
    struct Region([u8; 256]);

    #[cfg(target_os = "linux")]
    fn memory_map_of(region: &Region) -> procfs::process::MemoryMap {
        use procfs::process::{MMPermissions, MMapExtension, MMapPath, MemoryMap};

        let start = region.0.as_ptr() as u64;
        MemoryMap {
            address: (start, start + region.0.len() as u64),
            perms: MMPermissions::READ | MMPermissions::EXECUTE,
            offset: 0,
            dev: (0, 0),
            inode: 0,
            pathname: MMapPath::Anonymous,
            extension: MMapExtension::default(),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[cfg_attr(miri, ignore)]
    fn socket_and_option_setter_are_found_through_the_displacements_of_the_engine_code() {
        use crate::engine::patterns::QuakeLiveFunction;

        let mut region = Region([0x90; 256]);
        let socket_init = QuakeLiveFunction::Rcon_SocketInit.pattern();
        region.0[..socket_init.len()].copy_from_slice(socket_init);
        // The slot lies 64 bytes into the region; the displacement counts from the end of the instruction.
        region.0[27..31].copy_from_slice(&(64i32 - 31).to_le_bytes());
        let set_mandatory = QuakeLiveFunction::Zsock_SetRouterMandatory.pattern();
        region.0[96..96 + set_mandatory.len()].copy_from_slice(set_mandatory);
        region.0[96 + 58..96 + 62].copy_from_slice(&(200i32 - (96 + 62)).to_le_bytes());
        let base = region.0.as_ptr() as usize;
        let map = memory_map_of(&region);

        let socket = RconSocket::locate(&[&map]).expect("both patterns are in the region");

        assert_eq!(socket.handle_slot as usize, base + 64);
        assert_eq!(socket.set_option as usize, base + 200);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[cfg_attr(miri, ignore)]
    fn missing_pattern_is_reported_as_not_found_function() {
        use crate::engine::{QuakeLiveEngineError, patterns::QuakeLiveFunction};

        let region = Region([0x90; 256]);
        let map = memory_map_of(&region);

        assert!(matches!(
            RconSocket::locate(&[&map]),
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Rcon_SocketInit
            ))
        ));
    }

    #[test]
    fn errors_are_displayed_for_the_log() {
        assert_eq!(
            SendTimeoutError::SocketNotOpen.to_string(),
            "the RCON socket of the engine is not open"
        );
        assert_eq!(
            SendTimeoutError::Rejected.to_string(),
            "the ZMQ library rejected the send timeout"
        );
    }
}
