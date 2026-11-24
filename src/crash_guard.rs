//! Makes sure that a crashed server ends, so that a process monitor can restart it.
//!
//! The engine installs its own `SIGSEGV` handler during its start and replaces any earlier one, so this has to be
//! registered afterwards. [`install`] is cheap and does nothing while the handler is in place, so the hooks that run
//! after the start of the engine call it. The handler arms an alarm and then calls the one of the engine. If that one
//! does not finish in [`GRACE_SECONDS`], for example because its cleanup waits for a thread, the alarm ends the
//! process with the signal and a core dump.

use core::{
    ffi::{c_int, c_void},
    mem,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::io;

use signal_hook::{
    consts::{SIGALRM, SIGSEGV},
    low_level::emulate_default_handler,
};
use signal_hook_registry::register_signal_unchecked;

/// How long the handler of the engine may take before the process is ended.
const GRACE_SECONDS: libc::c_uint = 10;

static CRASHING: AtomicBool = AtomicBool::new(false);
static ALARM_REGISTERED: AtomicBool = AtomicBool::new(false);
static ENGINE_ADDRESS: AtomicUsize = AtomicUsize::new(libc::SIG_DFL);
static ENGINE_FLAGS: AtomicUsize = AtomicUsize::new(0);

type SigInfoHandler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);
type PlainHandler = extern "C" fn(c_int);

extern "C" fn on_segv(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    CRASHING.store(true, Ordering::Release);
    // SAFETY: `alarm` is async-signal-safe.
    unsafe { libc::alarm(GRACE_SECONDS) };

    let address = ENGINE_ADDRESS.load(Ordering::Acquire);
    let flags = ENGINE_FLAGS.load(Ordering::Acquire);
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

fn register_segv_handler() -> io::Result<()> {
    // SAFETY: all-zero is a valid `sigaction`, and `sigaction` is called with valid pointers.
    unsafe {
        let mut current: libc::sigaction = mem::zeroed();
        if libc::sigaction(SIGSEGV, core::ptr::null(), &raw mut current) != 0 {
            return Err(io::Error::last_os_error());
        }
        if current.sa_sigaction == on_segv as SigInfoHandler as usize {
            return Ok(());
        }
        ENGINE_FLAGS.store(current.sa_flags as usize, Ordering::Release);
        ENGINE_ADDRESS.store(current.sa_sigaction, Ordering::Release);

        let mut guard: libc::sigaction = mem::zeroed();
        guard.sa_sigaction = on_segv as SigInfoHandler as usize;
        guard.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;
        libc::sigemptyset(&raw mut guard.sa_mask);
        if libc::sigaction(SIGSEGV, &raw const guard, core::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Puts the guard in front of the current `SIGSEGV` handler, unless it already is.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn install() -> io::Result<()> {
    register_segv_handler()?;
    if !ALARM_REGISTERED.swap(true, Ordering::AcqRel) {
        // SAFETY: the action only uses an atomic and `emulate_default_handler`, which are async-signal-safe.
        unsafe {
            register_signal_unchecked(SIGALRM, || {
                if CRASHING.load(Ordering::Acquire) {
                    let _ = emulate_default_handler(SIGSEGV);
                }
            })?;
        }
    }

    Ok(())
}
