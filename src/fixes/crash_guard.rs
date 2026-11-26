//! Makes sure that a crashed or terminated server ends, so that a process monitor can restart it.
//!
//! The engine installs its own signal handlers during its start and replaces any earlier ones, so the guards have to be
//! registered afterwards. [`install`] is cheap and does nothing while the guards are in place, so the hooks that run
//! after the start of the engine call it.
//!
//! The guard of `SIGSEGV` arms an alarm and then calls the handler of the engine. If that one does not finish in
//! [`CRASH_GRACE_SECONDS`], for example because its cleanup waits for a thread, the alarm ends the process with the
//! signal and a core dump.
//!
//! The handler of the engine for `SIGTERM` and `SIGINT` only sets two flags, and with the ZMQ rcon enabled the server
//! does not end on them. The guard of these signals calls the handler of the engine, asks the frame hook for the `quit`
//! command through [`take_terminate_request`] and arms an alarm that ends the process after
//! [`TERMINATE_GRACE_SECONDS`] if the server is still there, for example while it loads a map.

use core::{
    ffi::{c_int, c_void},
    mem,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::io;

use signal_hook::{
    consts::{SIGALRM, SIGINT, SIGSEGV, SIGTERM},
    low_level::emulate_default_handler,
};
use signal_hook_registry::register_signal_unchecked;

/// How long the handler of the engine may take after a crash before the process is ended.
const CRASH_GRACE_SECONDS: libc::c_uint = 10;
/// How long the server may take to end after `SIGTERM` or `SIGINT` before the process is ended.
const TERMINATE_GRACE_SECONDS: libc::c_uint = 30;

static CRASHING: AtomicBool = AtomicBool::new(false);
static TERMINATING: AtomicBool = AtomicBool::new(false);
static TERMINATE_REQUESTED: AtomicBool = AtomicBool::new(false);
static ALARM_REGISTERED: AtomicBool = AtomicBool::new(false);

type SigInfoHandler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);
type PlainHandler = extern "C" fn(c_int);

/// The handler that the engine had registered for a signal before the guard replaced it.
struct EngineHandler {
    address: AtomicUsize,
    flags: AtomicUsize,
}

impl EngineHandler {
    const fn new() -> Self {
        Self {
            address: AtomicUsize::new(libc::SIG_DFL),
            flags: AtomicUsize::new(0),
        }
    }

    fn remember(&self, action: &libc::sigaction) {
        self.flags
            .store(action.sa_flags as usize, Ordering::Release);
        self.address.store(action.sa_sigaction, Ordering::Release);
    }

    fn call(&self, signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
        let address = self.address.load(Ordering::Acquire);
        let flags = self.flags.load(Ordering::Acquire);
        if address == libc::SIG_DFL || address == libc::SIG_IGN {
            let _ = emulate_default_handler(signal);
        } else if flags & libc::SA_SIGINFO as usize != 0 {
            // SAFETY: the engine registered this address as a handler with three arguments.
            unsafe { mem::transmute::<usize, SigInfoHandler>(address)(signal, info, context) };
        } else {
            // SAFETY: the engine registered this address as a handler with one argument.
            unsafe { mem::transmute::<usize, PlainHandler>(address)(signal) };
        }
    }
}

static SEGV_ENGINE: EngineHandler = EngineHandler::new();
static TERM_ENGINE: EngineHandler = EngineHandler::new();
static INT_ENGINE: EngineHandler = EngineHandler::new();

fn engine_handler_for(signal: c_int) -> &'static EngineHandler {
    match signal {
        SIGSEGV => &SEGV_ENGINE,
        SIGTERM => &TERM_ENGINE,
        _ => &INT_ENGINE,
    }
}

extern "C" fn on_segv(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    CRASHING.store(true, Ordering::Release);
    // SAFETY: `alarm` is async-signal-safe.
    unsafe { libc::alarm(CRASH_GRACE_SECONDS) };

    engine_handler_for(signal).call(signal, info, context);
}

extern "C" fn on_terminate(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    TERMINATING.store(true, Ordering::Release);
    TERMINATE_REQUESTED.store(true, Ordering::Release);
    if !CRASHING.load(Ordering::Acquire) {
        // SAFETY: `alarm` is async-signal-safe.
        unsafe { libc::alarm(TERMINATE_GRACE_SECONDS) };
    }

    engine_handler_for(signal).call(signal, info, context);
}

fn guard(signal: c_int, handler: SigInfoHandler) -> io::Result<()> {
    // SAFETY: all-zero is a valid `sigaction`, and `sigaction` is called with valid pointers.
    unsafe {
        let mut current: libc::sigaction = mem::zeroed();
        if libc::sigaction(signal, core::ptr::null(), &raw mut current) != 0 {
            return Err(io::Error::last_os_error());
        }
        if current.sa_sigaction == handler as usize {
            return Ok(());
        }
        engine_handler_for(signal).remember(&current);

        let mut guard: libc::sigaction = mem::zeroed();
        guard.sa_sigaction = handler as usize;
        guard.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        libc::sigemptyset(&raw mut guard.sa_mask);
        if libc::sigaction(signal, &raw const guard, core::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Puts the guards in front of the current handlers of `SIGSEGV`, `SIGTERM` and `SIGINT`, unless they already are.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn install() -> io::Result<()> {
    guard(SIGSEGV, on_segv)?;
    guard(SIGTERM, on_terminate)?;
    guard(SIGINT, on_terminate)?;
    if !ALARM_REGISTERED.swap(true, Ordering::AcqRel) {
        // SAFETY: the action only uses atomics and `emulate_default_handler`, which are async-signal-safe.
        unsafe {
            register_signal_unchecked(SIGALRM, || {
                if CRASHING.load(Ordering::Acquire) {
                    let _ = emulate_default_handler(SIGSEGV);
                } else if TERMINATING.load(Ordering::Acquire) {
                    let _ = emulate_default_handler(SIGTERM);
                }
            })?;
        }
    }

    Ok(())
}

/// Whether the server was asked to end by a signal since the last call. It is meant for the frame hook, which then
/// executes the `quit` command.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn take_terminate_request() -> bool {
    TERMINATE_REQUESTED.swap(false, Ordering::AcqRel)
}

#[cfg(test)]
mod crash_guard_tests {
    use core::{
        ffi::{c_int, c_void},
        mem,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use serial_test::serial;

    use super::{EngineHandler, TERMINATE_REQUESTED, guard, take_terminate_request};

    static PLAIN_CALLS: AtomicUsize = AtomicUsize::new(0);
    static INFO_CALLS: AtomicUsize = AtomicUsize::new(0);
    static GUARD_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn plain_handler(_signal: c_int) {
        PLAIN_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    extern "C" fn info_handler(_signal: c_int, _info: *mut libc::siginfo_t, _context: *mut c_void) {
        INFO_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    extern "C" fn guard_handler(
        _signal: c_int,
        _info: *mut libc::siginfo_t,
        _context: *mut c_void,
    ) {
        GUARD_CALLS.fetch_add(1, Ordering::AcqRel);
    }

    fn registered(address: usize, flags: c_int) -> libc::sigaction {
        // SAFETY: all-zero is a valid `sigaction`.
        let mut action: libc::sigaction = unsafe { mem::zeroed() };
        action.sa_sigaction = address;
        action.sa_flags = flags;
        action
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn the_handler_of_the_engine_is_called_with_the_arguments_it_registered() {
        let plain = EngineHandler::new();
        plain.remember(&registered(plain_handler as *const () as usize, 0));
        let with_info = EngineHandler::new();
        with_info.remember(&registered(
            info_handler as *const () as usize,
            libc::SA_SIGINFO,
        ));

        plain.call(libc::SIGUSR1, core::ptr::null_mut(), core::ptr::null_mut());
        with_info.call(libc::SIGUSR1, core::ptr::null_mut(), core::ptr::null_mut());

        assert_eq!(PLAIN_CALLS.load(Ordering::Acquire), 1);
        assert_eq!(INFO_CALLS.load(Ordering::Acquire), 1);
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn a_guard_replaces_the_handler_once_and_keeps_the_engine_one() {
        // SAFETY: the signal is only used by this test and put back to the default at its end.
        unsafe { libc::signal(libc::SIGUSR2, libc::SIG_DFL) };

        guard(libc::SIGUSR2, guard_handler).expect("the guard can be installed");
        guard(libc::SIGUSR2, guard_handler).expect("a second guard changes nothing");
        // SAFETY: the guard handler is installed and only counts the call.
        unsafe { libc::raise(libc::SIGUSR2) };

        assert_eq!(GUARD_CALLS.load(Ordering::Acquire), 1);
        // SAFETY: restores the default action of the signal used here.
        unsafe { libc::signal(libc::SIGUSR2, libc::SIG_DFL) };
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn a_guard_for_an_impossible_signal_is_an_error() {
        assert!(guard(-1, guard_handler).is_err());
    }

    #[test]
    #[serial]
    fn a_terminate_request_is_reported_once() {
        TERMINATE_REQUESTED.store(true, Ordering::Release);

        assert!(take_terminate_request());
        assert!(!take_terminate_request());
    }

    #[test]
    #[serial]
    fn without_a_signal_there_is_no_terminate_request() {
        TERMINATE_REQUESTED.store(false, Ordering::Release);

        assert!(!take_terminate_request());
    }
}
