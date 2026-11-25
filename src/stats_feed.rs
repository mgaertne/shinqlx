//! Takes the stats messages of the game module where the game module hands them to the engine.
//!
//! The game module builds one `Json::Value` per stats event and passes it, together with the name of the event
//! type, to a single function of the engine, which publishes it on the ZMQ stats socket. Hooking that function
//! delivers the same messages without the socket and without `zmq_stats_enable`.

use core::mem;
use std::sync::Mutex;

const MAX_PENDING: usize = 4096;
const EVENT_SLOT: usize = 0x5d0;
const REPORT_SLOT: usize = 0x5c8;
const GLOBAL_LOAD_OFFSET: usize = 0xdf;
const GLOBAL_LOAD_OPCODE: [u8; 3] = [0x4c, 0x8b, 0x2d];

static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Keeps a message until the next frame, so that plugins do not run in the middle of the game's own code.
pub(crate) fn queue_message(message: String) {
    if let Ok(mut pending) = PENDING.lock()
        && pending.len() < MAX_PENDING
    {
        pending.push(message);
    }
}

/// Hands out the messages that were queued since the last call.
pub(crate) fn take_pending() -> Vec<String> {
    PENDING
        .lock()
        .map(|mut pending| mem::take(&mut *pending))
        .unwrap_or_default()
}

/// The message as the stats socket carries it.
pub(crate) fn socket_message(kind: &str, data: &str) -> String {
    format!(
        r#"{{"DATA":{},"TYPE":"{kind}"}}"#,
        data.trim_end_matches('\n')
    )
}

/// Reads an engine function from the import table of the game module: the function that publishes a stats event
/// (`EVENT_SLOT`) or the one that publishes the report of a match (`REPORT_SLOT`).
///
/// `builder` is the address of the function of the game module that builds the `PLAYER_MEDAL` event. It loads
/// the pointer to the import table from a global variable, which this follows to the slot of the function.
///
/// # Safety
///
/// `builder` must be the address of that function in a loaded game module, so that its code and the memory it
/// refers to are readable.
pub(crate) unsafe fn locate_import(builder: usize, slot: usize) -> Option<usize> {
    // SAFETY: the caller guarantees that the code of the function is readable.
    unsafe { locate_import_at(builder + GLOBAL_LOAD_OFFSET, GLOBAL_LOAD_OPCODE, slot) }
}

/// Reads a slot of the import table of the game module through the instruction at `instruction`, which loads the
/// pointer to the table from a global variable and starts with `opcode`.
///
/// # Safety
///
/// `instruction` must be the address of such an instruction in a loaded game module, so that the code and the
/// memory it refers to are readable.
pub(crate) unsafe fn locate_import_at(
    instruction: usize,
    load_opcode: [u8; 3],
    slot: usize,
) -> Option<usize> {
    // SAFETY: the caller guarantees that the code of the function is readable.
    let opcode = unsafe { core::ptr::read_unaligned(instruction as *const [u8; 3]) };
    if opcode != load_opcode {
        return None;
    }

    // SAFETY: as above, the displacement follows the opcode.
    let displacement = unsafe { core::ptr::read_unaligned((instruction + 3) as *const i32) };
    let global_slot = (instruction + 7).checked_add_signed(displacement as isize)?;
    // SAFETY: the instruction loads this slot of the offset table of the module, which the loader filled in.
    let global = unsafe { core::ptr::read_unaligned(global_slot as *const usize) };
    if global == 0 {
        return None;
    }
    // SAFETY: the global variable holds the pointer to the import table once the game module is set up.
    let table = unsafe { core::ptr::read_unaligned(global as *const usize) };
    if table == 0 {
        return None;
    }
    // SAFETY: the import table has a slot at this offset.
    let import = unsafe { core::ptr::read_unaligned((table + slot) as *const usize) };
    (import != 0).then_some(import)
}

#[cfg(all(target_os = "linux", not(test)))]
pub(crate) use linux::install;

#[cfg(all(target_os = "linux", not(test)))]
mod linux {
    use alloc::ffi::CString;
    use core::{
        ffi::{CStr, c_char, c_int, c_void},
        hint::cold_path,
    };
    use std::sync::OnceLock;

    use arc_swap::ArcSwapOption;
    use detour::{Function, HookableWith, TypedDetour};
    use log::{debug, error};
    use procfs::process::{MMapPath, Process};

    use super::{EVENT_SLOT, REPORT_SLOT, locate_import, queue_message, socket_message};
    #[cfg(feature = "zmq-stats")]
    use crate::{MAIN_ENGINE, quake_live_engine::FindCVar};

    type ImportFn = extern "C" fn(*mut c_void, c_int, *const c_char, *const c_void);
    type ImportDetour = TypedDetour<ImportFn>;
    type ReportFn = extern "C" fn(*const c_void);
    type ReportDetour = TypedDetour<ReportFn>;

    type NewWriterFn = unsafe extern "C" fn(*mut c_void);
    type WriteFn = unsafe extern "C" fn(*mut *const c_char, *mut c_void, *const c_void);
    type DropStringFn = unsafe extern "C" fn(*mut *const c_char);

    const WRITER_SIZE: usize = 64;

    struct JsonWriter {
        new_writer: NewWriterFn,
        drop_writer: NewWriterFn,
        write: WriteFn,
        drop_string: DropStringFn,
    }

    // SAFETY: the functions are plain code pointers of loaded modules.
    unsafe impl Send for JsonWriter {}
    // SAFETY: as above.
    unsafe impl Sync for JsonWriter {}

    static DETOUR: OnceLock<ImportDetour> = OnceLock::new();
    static REPORT_DETOUR: OnceLock<ReportDetour> = OnceLock::new();
    static WRITER: ArcSwapOption<JsonWriter> = ArcSwapOption::const_empty();

    fn symbol<T: Copy>(handle: *mut c_void, name: &CStr) -> Option<T> {
        const { assert!(size_of::<T>() == size_of::<*mut c_void>()) };
        // SAFETY: the handle is a valid handle of a loaded module and the name is NUL-terminated.
        let address = unsafe { libc::dlsym(handle, name.as_ptr()) };
        // SAFETY: the symbol has the function type that the caller asks for.
        (!address.is_null())
            .then(|| unsafe { core::mem::transmute_copy::<*mut c_void, T>(&address) })
    }

    fn load_writer() -> Option<JsonWriter> {
        let path = Process::myself()
            .ok()?
            .maps()
            .ok()?
            .iter()
            .find_map(|mmap| match &mmap.pathname {
                MMapPath::Path(path)
                    if path.file_name().is_some_and(|name| {
                        name.to_string_lossy() == crate::quake_live_engine::QAGAME
                    }) =>
                {
                    Some(path.clone())
                }
                _ => None,
            })?;
        let path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        // SAFETY: the module is loaded already, RTLD_NOLOAD only returns its handle.
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOLOAD | libc::RTLD_LAZY) };
        if handle.is_null() {
            return None;
        }
        let writer = (|| {
            Some(JsonWriter {
                new_writer: symbol(handle, c"_ZN4Json10FastWriterC1Ev")?,
                drop_writer: symbol(handle, c"_ZN4Json10FastWriterD1Ev")?,
                write: symbol(handle, c"_ZN4Json10FastWriter5writeERKNS_5ValueE")?,
                drop_string: symbol(libc::RTLD_DEFAULT, c"_ZNSsD1Ev")?,
            })
        })();
        // SAFETY: the handle was opened above; the engine keeps the module loaded as long as it needs it.
        unsafe { libc::dlclose(handle) };
        writer
    }

    impl JsonWriter {
        /// # Safety
        ///
        /// `value` must point to a live `Json::Value` of the game module.
        unsafe fn to_text(&self, value: *const c_void) -> Option<String> {
            let mut buffer = [0usize; WRITER_SIZE / size_of::<usize>()];
            let writer = buffer.as_mut_ptr().cast::<c_void>();
            let mut text: *const c_char = core::ptr::null();
            // SAFETY: the buffer is large and aligned enough for a FastWriter, which is built, used and
            // destroyed here; the string it returns is the one-pointer string of the old libstdc++ ABI.
            unsafe {
                (self.new_writer)(writer);
                (self.write)(&mut text, writer, value);
                let result =
                    (!text.is_null()).then(|| CStr::from_ptr(text).to_string_lossy().into_owned());
                (self.drop_string)(&mut text);
                (self.drop_writer)(writer);
                result
            }
        }
    }

    extern "C" fn shinqlx_stats_import(
        owner: *mut c_void,
        number: c_int,
        kind: *const c_char,
        value: *const c_void,
    ) {
        if !kind.is_null() {
            // SAFETY: the game module passes a NUL-terminated type name.
            let kind = unsafe { CStr::from_ptr(kind) }.to_string_lossy();
            queue_value(&kind, value);
        }

        if let Some(detour) = DETOUR.get() {
            detour.call(owner, number, kind, value);
        }
    }

    extern "C" fn shinqlx_match_report(value: *const c_void) {
        queue_value(
            crate::ffi::python::string_enums::StatsType::MatchReport.as_str(),
            value,
        );

        if let Some(detour) = REPORT_DETOUR.get() {
            detour.call(value);
        }
    }

    /// With the `zmq-stats` feature the messages come from the socket while `zmq_stats_enable` is set.
    #[cfg(feature = "zmq-stats")]
    fn feed_wanted() -> bool {
        MAIN_ENGINE.load().as_ref().is_some_and(|main_engine| {
            main_engine
                .find_cvar("zmq_stats_enable")
                .is_none_or(|cvar| cvar.get_integer() == 0)
        })
    }

    #[cfg(not(feature = "zmq-stats"))]
    fn feed_wanted() -> bool {
        true
    }

    /// Queues the message of the game module, unless the stats socket delivers it.
    fn queue_value(kind: &str, value: *const c_void) {
        if feed_wanted()
            && !value.is_null()
            && let Some(writer) = WRITER.load().as_ref()
        {
            // SAFETY: the game module passes a live value.
            if let Some(data) = unsafe { writer.to_text(value) } {
                queue_message(socket_message(kind, &data));
            }
        }
    }

    /// Hooks the engine function at `import` once, with `replacement` as the new function.
    fn hook_import<F>(import: usize, replacement: F, cell: &OnceLock<TypedDetour<F>>)
    where
        F: Function + HookableWith<F> + Copy,
    {
        const { assert!(size_of::<F>() == size_of::<usize>()) };
        if cell.get().is_some() {
            return;
        }

        // SAFETY: the address is the engine function in the import table, with the signature of F.
        let function = unsafe { core::mem::transmute_copy::<usize, F>(&import) };
        // SAFETY: the original and the replacement are functions with the same signature.
        let Ok(detour) = (unsafe { TypedDetour::new(function, replacement) }) else {
            cold_path();
            error!(target: "shinqlx", func = "hook_import"; "The stats import at {import:#X} could not be hooked.");
            return;
        };
        // SAFETY: the detour was created from valid function addresses of the engine.
        if unsafe { detour.enable() }.is_err() {
            cold_path();
            error!(target: "shinqlx", func = "hook_import"; "The stats import at {import:#X} could not be hooked.");
            return;
        }

        if cell.set(detour).is_ok() {
            debug!(target: "shinqlx", func = "hook_import"; "Stats import hooked at {import:#X}");
        }
    }

    /// Hooks the engine function that publishes the stats events, if it was not hooked before.
    ///
    /// `builder` is the address of the function of the game module that builds the `PLAYER_MEDAL` event.
    pub(crate) fn install(builder: usize) {
        if builder == 0 {
            return;
        }

        WRITER.store(load_writer().map(Into::into));
        if WRITER.load().is_none() {
            cold_path();
            error!(target: "shinqlx", func = "install"; "The JSON writer of the game module was not found, no stats events are delivered.");
            return;
        }

        // SAFETY: the address comes from a pattern search of the loaded game module.
        let Some(import) = (unsafe { locate_import(builder, EVENT_SLOT) }) else {
            cold_path();
            error!(target: "shinqlx", func = "install"; "The stats import of the game module was not found, no stats events are delivered.");
            return;
        };
        hook_import(import, shinqlx_stats_import as ImportFn, &DETOUR);

        // SAFETY: as above.
        match unsafe { locate_import(builder, REPORT_SLOT) } {
            Some(report) => hook_import(report, shinqlx_match_report as ReportFn, &REPORT_DETOUR),
            None => {
                cold_path();
                error!(target: "shinqlx", func = "install"; "The match report import of the game module was not found, game_end is not delivered.");
            }
        }
    }
}

#[cfg(test)]
mod stats_feed_tests {
    use pretty_assertions::assert_eq;

    use super::{locate_import, queue_message, socket_message, take_pending};
    use crate::prelude::*;

    #[test]
    fn socket_message_wraps_the_data_with_its_type() {
        assert_eq!(
            socket_message("PLAYER_MEDAL", "{\"MEDAL\":\"DEFENSE\"}\n"),
            r#"{"DATA":{"MEDAL":"DEFENSE"},"TYPE":"PLAYER_MEDAL"}"#
        );
    }

    #[test]
    #[serial]
    fn queued_messages_are_taken_once_and_in_order() {
        let _ = take_pending();

        queue_message("first".to_string());
        queue_message("second".to_string());

        assert_eq!(take_pending(), vec!["first", "second"]);
        assert!(take_pending().is_empty());
    }

    #[test]
    #[serial]
    fn messages_beyond_the_limit_are_dropped() {
        let _ = take_pending();

        (0..super::MAX_PENDING + 10).for_each(|number| queue_message(number.to_string()));

        assert_eq!(take_pending().len(), super::MAX_PENDING);
    }

    struct FakeModule {
        memory: Vec<usize>,
    }

    impl FakeModule {
        const GLOBAL: usize = 0x101;
        const GLOBAL_SLOT: usize = 0x100;
        const TABLE: usize = 0x200;
        const WORDS: usize = 0x400;

        fn new(import: usize) -> Self {
            let mut memory = vec![0usize; Self::WORDS];
            let base = memory.as_ptr() as usize;
            memory[Self::GLOBAL_SLOT] = base + Self::GLOBAL * 8;
            memory[Self::GLOBAL] = base + Self::TABLE * 8;
            memory[Self::TABLE + super::EVENT_SLOT / 8] = import;
            memory[Self::TABLE + super::REPORT_SLOT / 8] = import + 1;
            Self { memory }
        }

        fn write_load(&mut self, opcode: [u8; 3]) -> usize {
            let base = self.memory.as_ptr() as usize;
            let next = base + super::GLOBAL_LOAD_OFFSET + 7;
            let displacement = (base + Self::GLOBAL_SLOT * 8) as isize - next as isize;
            let bytes = self.memory.as_mut_ptr().cast::<u8>();
            // SAFETY: the offsets lie inside the vector.
            unsafe {
                bytes
                    .add(super::GLOBAL_LOAD_OFFSET)
                    .copy_from_nonoverlapping(opcode.as_ptr(), 3);
                bytes
                    .add(super::GLOBAL_LOAD_OFFSET + 3)
                    .copy_from_nonoverlapping((displacement as i32).to_le_bytes().as_ptr(), 4);
            }
            base
        }
    }

    #[test]
    fn the_import_is_read_from_the_table_the_builder_refers_to() {
        let mut module = FakeModule::new(0xdead_beef);
        let builder = module.write_load(super::GLOBAL_LOAD_OPCODE);

        // SAFETY: the fake module is readable at every offset that is read.
        let import = unsafe { locate_import(builder, super::EVENT_SLOT) };

        assert_eq!(import, Some(0xdead_beef));
    }

    #[test]
    fn a_builder_with_other_code_gives_no_import() {
        let mut module = FakeModule::new(0xdead_beef);
        let builder = module.write_load([0x90, 0x90, 0x90]);

        // SAFETY: the fake module is readable at every offset that is read.
        let import = unsafe { locate_import(builder, super::EVENT_SLOT) };

        assert_eq!(import, None);
    }

    #[test]
    fn an_empty_import_table_gives_no_import() {
        let mut module = FakeModule::new(0);
        let builder = module.write_load(super::GLOBAL_LOAD_OPCODE);

        // SAFETY: the fake module is readable at every offset that is read.
        let import = unsafe { locate_import(builder, super::EVENT_SLOT) };

        assert_eq!(import, None);
    }
}
