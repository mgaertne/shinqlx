//! The virtual file system of the engine as a [`MapFileSystem`].
//!
//! Listing and reading call the engine's own functions (`FS_ListFilteredFiles`, `FS_ReadFile`), so what the engine
//! would find is what the scan finds. The source of a file comes from the engine's search path list, which is read
//! from memory once. Everything here must run on the main thread of the server after its file system has started.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
};
use core::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
};
use std::path::Path;

use derive_more::Display;

#[cfg(target_os = "linux")]
use crate::maps::scan::Snapshot;
use crate::maps::{
    filesystem::{FsError, MapFileSystem},
    sources::{SearchEntry, SourceIndex},
};

// Layout of the engine's search path structures (ioq3 with MAX_OSPATH 4096), checked against qzeroded 1069.
const NODE_NEXT: usize = 0;
const NODE_PACK: usize = 8;
const NODE_DIR: usize = 16;
const PACK_FILENAME: usize = 0;
const PACK_NUMFILES: usize = 0x3010;
const PACK_HASHSIZE: usize = 0x3018;
const PACK_HASHTABLE: usize = 0x3020;
const ENTRY_NAME: usize = 0;
const ENTRY_NEXT: usize = 16;
const DIR_PATH: usize = 0;
const DIR_GAMEDIR: usize = 0x1000;

const MAX_NODES: usize = 10_000;
const MAX_PACK_FILES: usize = 5_000_000;
const MAX_HASH_SIZE: usize = 1 << 22;
const MAX_STRING: usize = 4096;
/// `cmp QWORD PTR [rip+disp32], 0` sits this many bytes into `FS_ReadFile`.
const SEARCHPATHS_CHECK_OFFSET: usize = 0x10;
const SEARCHPATHS_CHECK_OPCODE: [u8; 3] = [0x48, 0x83, 0x3d];
const SEARCHPATHS_CHECK_LENGTH: usize = 8;

/// Why the engine's file system could not be used.
#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub(crate) enum EngineFsError {
    #[display("function {_0} not found in the engine")]
    FunctionNotFound(&'static str),
    #[display("the search path list was not found")]
    SearchPathsNotFound,
}

/// # Safety
///
/// `address` must be readable memory.
unsafe fn read<T: Copy>(address: usize) -> T {
    // SAFETY: the caller guarantees that the address is readable.
    unsafe { ptr::read_unaligned(ptr::with_exposed_provenance::<T>(address)) }
}

/// # Safety
///
/// `address` must point at readable memory up to a NUL byte or [`MAX_STRING`] bytes.
unsafe fn c_string(address: usize) -> Option<String> {
    let bytes = (0..MAX_STRING)
        // SAFETY: the caller guarantees the memory up to the NUL byte.
        .map(|offset| unsafe { read::<u8>(address + offset) })
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    (bytes.len() < MAX_STRING).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// The target of the `cmp QWORD PTR [rip+disp32], 0` at `function + 0x10`, which is the engine's `fs_searchpaths`.
///
/// # Safety
///
/// `function` must be the address of the engine's `FS_ReadFile`, or of readable memory of at least 0x18 bytes.
unsafe fn searchpaths_address(function: usize) -> Option<usize> {
    let instruction = function + SEARCHPATHS_CHECK_OFFSET;
    // SAFETY: the caller guarantees readable memory at the function.
    let opcode = unsafe { read::<[u8; 3]>(instruction) };
    if opcode != SEARCHPATHS_CHECK_OPCODE {
        return None;
    }
    // SAFETY: as above.
    let displacement = unsafe { read::<i32>(instruction + 3) };
    (instruction + SEARCHPATHS_CHECK_LENGTH).checked_add_signed(isize::try_from(displacement).ok()?)
}

/// The search path list that starts at the node pointer stored at `head_address`, in the engine's order. Archives
/// whose entry count differs from their `numfiles` are skipped and named in the second value.
///
/// # Safety
///
/// `head_address` must be the address of the engine's `fs_searchpaths` or a structure of the same layout.
unsafe fn read_search_paths(head_address: usize) -> (Vec<SearchEntry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut problems = Vec::new();
    // SAFETY: the caller guarantees the layout; every pointer is read before it is followed.
    let mut node = unsafe { read::<usize>(head_address) };
    for _ in 0..MAX_NODES {
        if node == 0 {
            break;
        }
        // SAFETY: `node` is a search path node of the list.
        let (pack, dir) = unsafe {
            (
                read::<usize>(node + NODE_PACK),
                read::<usize>(node + NODE_DIR),
            )
        };
        if pack != 0 {
            // SAFETY: `pack` is the pack of a node.
            match unsafe { read_pack(pack) } {
                Ok(entry) => entries.push(entry),
                Err(problem) => problems.push(problem),
            }
        } else if dir != 0 {
            // SAFETY: `dir` is the directory of a node.
            let (path, gamedir) =
                unsafe { (c_string(dir + DIR_PATH), c_string(dir + DIR_GAMEDIR)) };
            if let Some((path, gamedir)) = path.zip(gamedir) {
                entries.push(SearchEntry::Directory { path, gamedir });
            }
        }
        // SAFETY: as above.
        node = unsafe { read::<usize>(node + NODE_NEXT) };
    }
    (entries, problems)
}

/// # Safety
///
/// `pack` must be the address of a pack structure of the engine.
unsafe fn read_pack(pack: usize) -> Result<SearchEntry, String> {
    // SAFETY: the caller guarantees the pack structure.
    let (path, numfiles, hash_size, table) = unsafe {
        (
            c_string(pack + PACK_FILENAME),
            usize::try_from(read::<i32>(pack + PACK_NUMFILES)),
            usize::try_from(read::<i32>(pack + PACK_HASHSIZE)),
            read::<usize>(pack + PACK_HASHTABLE),
        )
    };
    let path = path.ok_or_else(|| "archive without a readable name".to_owned())?;
    let (Ok(numfiles), Ok(hash_size)) = (numfiles, hash_size) else {
        return Err(format!("{path}: implausible header"));
    };
    if numfiles > MAX_PACK_FILES || hash_size == 0 || hash_size > MAX_HASH_SIZE || table == 0 {
        return Err(format!("{path}: implausible header"));
    }

    let mut names = BTreeMap::new();
    let mut counted = 0_usize;
    for bucket in 0..hash_size {
        // SAFETY: `bucket` lies inside the hash table of `hash_size` pointers.
        let mut entry = unsafe { read::<usize>(table + bucket * size_of::<usize>()) };
        while entry != 0 {
            counted += 1;
            if counted > numfiles {
                return Err(format!("{path}: more entries than numfiles"));
            }
            // SAFETY: `entry` is a file entry of the pack.
            let name = unsafe { read::<usize>(entry + ENTRY_NAME) };
            // SAFETY: as above.
            if let Some(name) = unsafe { c_string(name) } {
                names.insert(name.to_ascii_lowercase(), name.to_owned());
            }
            // SAFETY: as above.
            entry = unsafe { read::<usize>(entry + ENTRY_NEXT) };
        }
    }
    if counted == numfiles {
        Ok(SearchEntry::Pack { path, names })
    } else {
        Err(format!("{path}: {counted} entries, expected {numfiles}"))
    }
}

type ReadFileFn = extern "C" fn(*const c_char, *mut *mut c_void) -> c_int;
type FreeFileFn = extern "C" fn(*mut c_void);
type ListFilesFn =
    extern "C" fn(*const c_char, *const c_char, *const c_char, *mut c_int) -> *mut *mut c_char;
type FreeFileListFn = extern "C" fn(*mut *mut c_char);

/// The engine's file system.
pub(crate) struct EngineFileSystem {
    read_file: ReadFileFn,
    free_file: FreeFileFn,
    list_files: ListFilesFn,
    free_file_list: FreeFileListFn,
    sources: SourceIndex,
    /// Archives the search path list read could not use.
    pub(crate) problems: Vec<String>,
}

impl EngineFileSystem {
    /// A copy of the search path list, for readers that do not go through the engine.
    pub(crate) fn source_index(&self) -> SourceIndex {
        self.sources.clone()
    }

    /// Finds the functions in the memory of the engine and reads the search path list.
    ///
    /// # Safety
    ///
    /// Must be called on the main thread of the server once its file system has started, in the process of the
    /// engine version that the byte patterns and structure layout were made for.
    #[cfg(target_os = "linux")]
    pub(crate) unsafe fn locate() -> Result<Self, EngineFsError> {
        use procfs::process::{MMapPath, Process};

        use crate::{
            QZERODED,
            engine::patterns::{QuakeLiveFunction, pattern_search_module},
        };

        let maps = Process::myself()
            .and_then(|process| process.maps())
            .map_err(|err| {
                log::error!(target: "shinqlx", func = "engine_fs"; "reading the memory maps failed: {err}");
                EngineFsError::FunctionNotFound("memory maps")
            })?;
        let engine_maps = maps
            .iter()
            .filter(|map| {
                matches!(&map.pathname, MMapPath::Path(path)
                    if path.file_name().is_some_and(|name| name.to_string_lossy() == QZERODED))
            })
            .collect::<Vec<_>>();
        let find = |function: QuakeLiveFunction, name: &'static str| {
            pattern_search_module(&engine_maps, function)
                .ok_or(EngineFsError::FunctionNotFound(name))
        };
        let read_file = find(QuakeLiveFunction::FS_ReadFile, "FS_ReadFile")?;
        let free_file = find(QuakeLiveFunction::FS_FreeFile, "FS_FreeFile")?;
        let list_files = find(
            QuakeLiveFunction::FS_ListFilteredFiles,
            "FS_ListFilteredFiles",
        )?;
        let free_file_list = find(QuakeLiveFunction::FS_FreeFileList, "FS_FreeFileList")?;
        // SAFETY: `read_file` is the address of FS_ReadFile in the mapped engine.
        let head =
            unsafe { searchpaths_address(read_file) }.ok_or(EngineFsError::SearchPathsNotFound)?;
        // SAFETY: `head` is the address of fs_searchpaths; the functions have the signatures of the typedefs.
        unsafe {
            Ok(Self::from_addresses(
                read_file,
                free_file,
                list_files,
                free_file_list,
                head,
            ))
        }
    }

    /// # Safety
    ///
    /// The addresses must be those of the engine's functions with the signatures of the type aliases, and `head`
    /// that of its `fs_searchpaths`.
    unsafe fn from_addresses(
        read_file: usize,
        free_file: usize,
        list_files: usize,
        free_file_list: usize,
        head: usize,
    ) -> Self {
        // SAFETY: the caller guarantees the layout at `head`.
        let (entries, problems) = unsafe { read_search_paths(head) };
        // SAFETY: the caller guarantees the signatures.
        unsafe {
            Self {
                read_file: core::mem::transmute::<usize, ReadFileFn>(read_file),
                free_file: core::mem::transmute::<usize, FreeFileFn>(free_file),
                list_files: core::mem::transmute::<usize, ListFilesFn>(list_files),
                free_file_list: core::mem::transmute::<usize, FreeFileListFn>(free_file_list),
                sources: SourceIndex::new(entries),
                problems,
            }
        }
    }
}

impl MapFileSystem for EngineFileSystem {
    fn list_files(&self, dir: &str, extension: &str) -> Vec<String> {
        let (Ok(dir), Ok(extension)) = (CString::new(dir), CString::new(extension)) else {
            return Vec::new();
        };
        let mut count: c_int = 0;
        let list = (self.list_files)(
            dir.as_ptr(),
            extension.as_ptr(),
            ptr::null(),
            &raw mut count,
        );
        if list.is_null() {
            return Vec::new();
        }
        let mut names = (0..usize::try_from(count).unwrap_or(0))
            .filter_map(|index| {
                // SAFETY: the engine returned `count` valid C strings.
                let name = unsafe { *list.add(index) };
                (!name.is_null()).then(|| {
                    // SAFETY: the engine returned valid C strings.
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .collect::<Vec<_>>();
        (self.free_file_list)(list);
        names.sort();
        names.dedup();
        names
    }

    fn read_file(&self, path: &str, limit: u64) -> Result<Vec<u8>, FsError> {
        let path = CString::new(path).map_err(|_| FsError::NotFound)?;
        let mut buffer: *mut c_void = ptr::null_mut();
        let length = (self.read_file)(path.as_ptr(), &raw mut buffer);
        let Ok(length) = usize::try_from(length) else {
            return Err(FsError::NotFound);
        };
        let result = if length as u64 > limit {
            Err(FsError::TooLarge { limit })
        } else if buffer.is_null() {
            Ok(Vec::new())
        } else {
            // SAFETY: the engine returned a buffer of `length` bytes.
            Ok(unsafe { core::slice::from_raw_parts(buffer.cast::<u8>(), length) }.to_vec())
        };
        if !buffer.is_null() {
            (self.free_file)(buffer);
        }
        result
    }

    fn source_of(&self, path: &str) -> Option<String> {
        self.sources
            .source_of(path, &|full_path| Path::new(full_path).is_file())
    }
}

/// Scans again in a thread of its own, without the engine, and logs how the result differs from `reference`.
#[cfg(target_os = "linux")]
fn compare_with_archive_scan(reference: &Snapshot, index: SourceIndex, homepath: &str) {
    use std::time::Instant;

    use log::warn;

    use crate::maps::{
        archive::ArchiveFileSystem,
        scan::{scan, scan_parallel},
    };

    for workers in [0_usize, 1, 2, 4] {
        let started = Instant::now();
        let homepath = homepath.to_owned();
        let index = index.clone();
        let Ok(other) = std::thread::spawn(move || {
            if workers == 0 {
                return scan(&ArchiveFileSystem::new(index), &homepath);
            }
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .expect("a pool of a few threads can be built");
            scan_parallel(
                &ArchiveFileSystem::new(index.clone()),
                &homepath,
                &pool,
                || ArchiveFileSystem::new(index.clone()),
            )
        })
        .join() else {
            warn!(target: "shinqlx", func = "compare_with_archive_scan"; "The archive scan panicked");
            return;
        };
        log_difference(reference, &other, workers, started.elapsed().as_millis());
    }
}

#[cfg(target_os = "linux")]
fn log_difference(reference: &Snapshot, other: &Snapshot, workers: usize, millis: u128) {
    use log::{info, warn};

    let differing = reference
        .maps
        .keys()
        .chain(other.maps.keys())
        .filter(|key| reference.maps.get(*key) != other.maps.get(*key))
        .collect::<BTreeSet<_>>();
    info!(
        target: "shinqlx", func = "compare_with_archive_scan";
        "Archive scan with {workers} workers: {} maps and {} factories in {millis} ms, \
        {} maps differ, factories {}",
        other.maps.len(), other.factories.len(), differing.len(),
        if other.factories == reference.factories { "equal" } else { "differ" }
    );
    for problem in other
        .problems
        .iter()
        .filter(|problem| !reference.problems.contains(problem))
        .take(20)
    {
        warn!(target: "shinqlx", func = "log_difference"; "only in the archive scan: {problem}");
    }
    for key in differing.into_iter().take(20) {
        warn!(
            target: "shinqlx", func = "compare_with_archive_scan";
            "{key}: engine {:?} / archive {:?}", reference.maps.get(key), other.maps.get(key)
        );
    }
}

/// Prepares the map scan of the server start: freezes the search path list once, so that `reload_maps()` works even
/// when the start-up scan is switched off, and starts the scan unless `scan_setting` (the cvar `qlx_mapsScan`) is `0`
/// or a snapshot exists or is being made. The setting `2` scans through the engine and compares with the archive scan.
///
/// # Safety
///
/// Same as [`EngineFileSystem::locate`].
#[cfg(target_os = "linux")]
pub(crate) unsafe fn start_map_scan(homepath: &str, scan_setting: &str) {
    use log::warn;

    use crate::maps::{loader, store};

    if !loader::is_configured() {
        // SAFETY: the caller guarantees the conditions of `locate`.
        if let Some(index) = unsafe { freeze_search_paths() } {
            loader::configure(index, homepath);
        }
    }
    if !store::scan_enabled(scan_setting) || store::has_snapshot() || loader::is_running() {
        return;
    }
    if scan_setting.trim() == "2" {
        // SAFETY: the caller guarantees the conditions of `locate`.
        if let Some(snapshot) = unsafe { scan_installed_maps(homepath, true) } {
            store::publish(snapshot);
        }
    } else if loader::is_configured()
        && let Err(error) = loader::start_scan()
    {
        warn!(target: "shinqlx", func = "start_map_scan"; "No map scan: {error}");
    }
}

/// Answers a request of `reload_maps()`: reads the search path list anew, because items may have been installed since,
/// and starts a scan. Called every frame on the main thread; a running scan keeps the request for a later frame.
///
/// # Safety
///
/// Same as [`EngineFileSystem::locate`].
#[cfg(target_os = "linux")]
pub(crate) unsafe fn refresh_if_requested() {
    use log::warn;

    use crate::maps::loader;

    if !loader::refresh_requested() || loader::is_running() {
        return;
    }
    loader::clear_refresh_request();

    // SAFETY: the caller guarantees the conditions of `locate`.
    if let Some(index) = unsafe { freeze_search_paths() }
        && loader::reconfigure(index)
        && let Err(error) = loader::start_scan()
    {
        warn!(target: "shinqlx", func = "refresh_if_requested"; "No map scan: {error}");
    }
}

/// Reads the engine's search path list once, for scans that do not go through the engine; `None` when it cannot be
/// found.
///
/// # Safety
///
/// Same as [`EngineFileSystem::locate`].
#[cfg(target_os = "linux")]
pub(crate) unsafe fn freeze_search_paths() -> Option<SourceIndex> {
    use log::{error, warn};

    // SAFETY: the caller guarantees the conditions of `locate`.
    match unsafe { EngineFileSystem::locate() } {
        Ok(engine_fs) => {
            for problem in &engine_fs.problems {
                warn!(target: "shinqlx", func = "freeze_search_paths"; "{problem}");
            }
            Some(engine_fs.source_index())
        }
        Err(error) => {
            error!(target: "shinqlx", func = "freeze_search_paths"; "Cannot scan the maps: {error}");
            None
        }
    }
}

/// Scans the installed maps through the engine's file system and writes what it found to the log; `None` when the
/// engine's file system could not be used.
///
/// # Safety
///
/// Same as [`EngineFileSystem::locate`].
#[cfg(target_os = "linux")]
pub(crate) unsafe fn scan_installed_maps(
    homepath: &str,
    compare_with_archives: bool,
) -> Option<Snapshot> {
    use std::time::Instant;

    use log::{error, info, warn};

    use crate::maps::scan::scan;

    let started = Instant::now();
    // SAFETY: the caller guarantees the conditions of `locate`.
    let engine_fs = match unsafe { EngineFileSystem::locate() } {
        Ok(engine_fs) => engine_fs,
        Err(error) => {
            error!(target: "shinqlx", func = "scan_installed_maps"; "Cannot scan the maps: {error}");
            return None;
        }
    };
    let mut snapshot = scan(&engine_fs, homepath);
    snapshot
        .problems
        .splice(0..0, engine_fs.problems.iter().cloned());
    info!(
        target: "shinqlx", func = "scan_installed_maps";
        "Scanned {} maps and {} factories in {} ms",
        snapshot.maps.len(),
        snapshot.factories.len(), started.elapsed().as_millis()
    );
    for problem in snapshot.problems.iter().take(50) {
        warn!(target: "shinqlx", func = "scan_installed_maps"; "{problem}");
    }
    if compare_with_archives {
        compare_with_archive_scan(&snapshot, engine_fs.source_index(), homepath);
    }
    Some(snapshot)
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use pretty_assertions::assert_eq;
    use serial_test::serial;

    use super::*;

    /// Memory laid out like the engine's structures, owned by the test.
    struct FakeEngine {
        blocks: Vec<Vec<u8>>,
    }

    impl FakeEngine {
        fn new() -> Self {
            Self { blocks: Vec::new() }
        }

        fn alloc(&mut self, size: usize) -> usize {
            self.blocks.push(vec![0; size]);
            self.blocks
                .last()
                .map(|block| block.as_ptr() as usize)
                .unwrap_or_default()
        }

        fn write(&mut self, address: usize, bytes: &[u8]) {
            let block = self
                .blocks
                .iter_mut()
                .find(|block| {
                    let start = block.as_ptr() as usize;
                    (start..start + block.len()).contains(&address)
                })
                .expect("address of a fake block");
            let offset = address - block.as_ptr() as usize;
            block[offset..offset + bytes.len()].copy_from_slice(bytes);
        }

        fn text(&mut self, text: &str) -> usize {
            let address = self.alloc(text.len() + 1);
            self.write(address, text.as_bytes());
            address
        }

        fn directory(&mut self, path: &str, gamedir: &str) -> usize {
            let dir = self.alloc(0x3000);
            self.write(dir + DIR_PATH, path.as_bytes());
            self.write(dir + DIR_GAMEDIR, gamedir.as_bytes());
            dir
        }

        fn pack(&mut self, path: &str, names: &[&str], numfiles: i32) -> usize {
            let pack = self.alloc(0x3030);
            self.write(pack + PACK_FILENAME, path.as_bytes());
            let hash_size = 4_usize;
            let table = self.alloc(hash_size * 8);
            let mut heads = vec![0_usize; hash_size];
            for name in names {
                let entry = self.alloc(24);
                let name_address = self.text(name);
                let bucket = name.len() % hash_size;
                self.write(entry + ENTRY_NAME, &name_address.to_le_bytes());
                self.write(entry + ENTRY_NEXT, &heads[bucket].to_le_bytes());
                heads[bucket] = entry;
            }
            for (bucket, head) in heads.iter().enumerate() {
                self.write(table + bucket * 8, &head.to_le_bytes());
            }
            self.write(pack + PACK_NUMFILES, &numfiles.to_le_bytes());
            self.write(
                pack + PACK_HASHSIZE,
                &i32::try_from(hash_size).unwrap().to_le_bytes(),
            );
            self.write(pack + PACK_HASHTABLE, &table.to_le_bytes());
            pack
        }

        /// Links the nodes `(pack, dir)` in order and returns the address of the head pointer.
        fn list(&mut self, nodes: &[(usize, usize)]) -> usize {
            let head = self.alloc(8);
            let mut next = 0_usize;
            for (pack, dir) in nodes.iter().rev() {
                let node = self.alloc(24);
                self.write(node + NODE_NEXT, &next.to_le_bytes());
                self.write(node + NODE_PACK, &pack.to_le_bytes());
                self.write(node + NODE_DIR, &dir.to_le_bytes());
                next = node;
            }
            self.write(head, &next.to_le_bytes());
            head
        }
    }

    #[test]
    fn reads_the_list_in_order_with_lowercase_keys_and_original_names() {
        let mut engine = FakeEngine::new();
        let home = engine.directory("/home", "baseq3");
        let pak = engine.pack(
            "/base/baseq3/pak00.pk3",
            &["maps/A.bsp", "scripts/arenas.txt"],
            2,
        );
        let head = engine.list(&[(0, home), (pak, 0)]);

        // SAFETY: `head` points at the structures built by `FakeEngine`.
        let (entries, problems) = unsafe { read_search_paths(head) };

        assert!(problems.is_empty());
        assert_eq!(
            entries,
            vec![
                SearchEntry::Directory {
                    path: "/home".into(),
                    gamedir: "baseq3".into()
                },
                SearchEntry::Pack {
                    path: "/base/baseq3/pak00.pk3".into(),
                    names: [
                        ("maps/a.bsp".to_owned(), "maps/A.bsp".to_owned()),
                        (
                            "scripts/arenas.txt".to_owned(),
                            "scripts/arenas.txt".to_owned()
                        )
                    ]
                    .into_iter()
                    .collect()
                },
            ]
        );
    }

    #[test]
    fn an_archive_whose_count_differs_from_numfiles_is_skipped_and_reported() {
        let mut engine = FakeEngine::new();
        let good = engine.pack("/good.pk3", &["a"], 1);
        let bad = engine.pack("/bad.pk3", &["a", "b"], 5);
        let head = engine.list(&[(bad, 0), (good, 0)]);

        // SAFETY: `head` points at the structures built by `FakeEngine`.
        let (entries, problems) = unsafe { read_search_paths(head) };

        assert_eq!(entries.len(), 1);
        assert_eq!(problems, vec!["/bad.pk3: 2 entries, expected 5".to_owned()]);
    }

    #[test]
    fn an_empty_list_gives_nothing() {
        let mut engine = FakeEngine::new();
        let head = engine.list(&[]);

        // SAFETY: `head` points at the structure built by `FakeEngine`.
        let result = unsafe { read_search_paths(head) };

        assert_eq!(result, (Vec::new(), Vec::new()));
    }

    #[test]
    fn the_head_address_comes_from_the_rip_relative_operand() {
        let mut code = [0_u8; 0x20];
        code[0x10..0x13].copy_from_slice(&SEARCHPATHS_CHECK_OPCODE);
        code[0x13..0x17].copy_from_slice(&0x480b50_i32.to_le_bytes());
        let function = code.as_ptr() as usize;

        // SAFETY: `function` points at the 0x20 readable bytes of `code`.
        let address = unsafe { searchpaths_address(function) };
        assert_eq!(address, Some(function + 0x18 + 0x480b50));

        code[0x10] = 0x90;
        // SAFETY: as above.
        let address = unsafe { searchpaths_address(code.as_ptr() as usize) };
        assert_eq!(address, None);
    }

    static FREED_FILES: AtomicUsize = AtomicUsize::new(0);
    static FREED_LISTS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn fake_read_file(path: *const c_char, buffer: *mut *mut c_void) -> c_int {
        // SAFETY: the engine file system passes a valid C string.
        let path = unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned();
        match path.as_str() {
            "maps/a.bsp" => {
                let data = Box::leak(Box::new(*b"BSPDATA"));
                // SAFETY: the engine file system passes a valid pointer for the buffer.
                unsafe { *buffer = data.as_mut_ptr().cast() };
                7
            }
            "empty.bsp" => 0,
            _ => -1,
        }
    }

    extern "C" fn fake_free_file(_buffer: *mut c_void) {
        FREED_FILES.fetch_add(1, Ordering::AcqRel);
    }

    extern "C" fn fake_list_files(
        dir: *const c_char,
        _extension: *const c_char,
        _filter: *const c_char,
        count: *mut c_int,
    ) -> *mut *mut c_char {
        // SAFETY: the engine file system passes a valid C string.
        let dir = unsafe { CStr::from_ptr(dir) }
            .to_string_lossy()
            .into_owned();
        let names: Vec<*mut c_char> = match dir.as_str() {
            "maps" => ["b.bsp", "a.bsp", "a.bsp"]
                .iter()
                .map(|name| CString::new(*name).unwrap().into_raw())
                .collect(),
            "holes" => vec![ptr::null_mut(), CString::new("x.bsp").unwrap().into_raw()],
            _ => Vec::new(),
        };
        // SAFETY: the engine file system passes a valid pointer for the count.
        unsafe { *count = c_int::try_from(names.len()).unwrap_or_default() };
        if names.is_empty() {
            return ptr::null_mut();
        }
        Box::leak(names.into_boxed_slice()).as_mut_ptr()
    }

    extern "C" fn fake_free_file_list(_list: *mut *mut c_char) {
        FREED_LISTS.fetch_add(1, Ordering::AcqRel);
    }

    fn fake_engine_fs(engine: &mut FakeEngine) -> EngineFileSystem {
        let pak = engine.pack("/base/baseq3/pak00.pk3", &["maps/A.bsp"], 1);
        let head = engine.list(&[(pak, 0)]);
        FREED_FILES.store(0, Ordering::Release);
        FREED_LISTS.store(0, Ordering::Release);
        // SAFETY: the functions have the signatures of the type aliases and `head` is built by `FakeEngine`.
        unsafe {
            EngineFileSystem::from_addresses(
                fake_read_file as *const () as usize,
                fake_free_file as *const () as usize,
                fake_list_files as *const () as usize,
                fake_free_file_list as *const () as usize,
                head,
            )
        }
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "turns addresses into function pointers")]
    fn the_listing_of_the_engine_is_sorted_without_duplicates_and_freed() {
        let mut engine = FakeEngine::new();
        let fs = fake_engine_fs(&mut engine);

        assert_eq!(fs.list_files("maps", ".bsp"), vec!["a.bsp", "b.bsp"]);
        assert_eq!(FREED_LISTS.load(Ordering::Acquire), 1);
        assert_eq!(fs.list_files("holes", ".bsp"), vec!["x.bsp"]);
        assert!(fs.list_files("nothing", ".bsp").is_empty());
        assert!(fs.list_files("ma\0ps", ".bsp").is_empty());
        assert!(fs.list_files("maps", ".b\0sp").is_empty());
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "turns addresses into function pointers")]
    fn reading_a_file_of_the_engine_returns_the_bytes_and_frees_the_buffer() {
        let mut engine = FakeEngine::new();
        let fs = fake_engine_fs(&mut engine);

        assert_eq!(fs.read_file("maps/a.bsp", 100), Ok(b"BSPDATA".to_vec()));
        assert_eq!(FREED_FILES.load(Ordering::Acquire), 1);
        assert_eq!(
            fs.read_file("maps/a.bsp", 3),
            Err(FsError::TooLarge { limit: 3 })
        );
        assert_eq!(FREED_FILES.load(Ordering::Acquire), 2);
        assert_eq!(fs.read_file("empty.bsp", 100), Ok(Vec::new()));
        assert_eq!(FREED_FILES.load(Ordering::Acquire), 2);
        assert_eq!(fs.read_file("missing.bsp", 100), Err(FsError::NotFound));
        assert_eq!(fs.read_file("a\0b.bsp", 100), Err(FsError::NotFound));
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "turns addresses into function pointers")]
    fn the_source_of_a_file_is_the_archive_that_the_search_path_list_names() {
        let mut engine = FakeEngine::new();
        let fs = fake_engine_fs(&mut engine);

        assert_eq!(
            fs.source_of("maps/a.bsp").as_deref(),
            Some("/base/baseq3/pak00.pk3")
        );
        assert_eq!(fs.source_of("maps/none.bsp"), None);
        assert_eq!(
            fs.source_index().source_of("maps/a.bsp", &|_| false),
            fs.source_of("maps/a.bsp")
        );
        assert!(fs.problems.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "reads the memory maps of the process")]
    fn without_the_engine_nothing_can_be_located_or_scanned() {
        crate::maps::loader::unconfigure();

        // SAFETY: the test process has no engine, so nothing is read or called.
        unsafe {
            assert_eq!(
                EngineFileSystem::locate().err(),
                Some(EngineFsError::FunctionNotFound("FS_ReadFile"))
            );
            assert!(freeze_search_paths().is_none());
            assert!(scan_installed_maps("/nowhere", true).is_none());
            start_map_scan("/nowhere", "0");
            start_map_scan("/nowhere", "1");
            start_map_scan("/nowhere", "2");
        }

        assert!(!crate::maps::loader::is_configured());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "reads the memory maps of the process")]
    fn a_refresh_request_is_answered_and_cleared_even_when_the_search_paths_cannot_be_read() {
        use crate::maps::loader;

        loader::unconfigure();
        // SAFETY: the test process has no engine, so nothing is read or called.
        unsafe { refresh_if_requested() };
        assert!(!loader::refresh_requested());

        loader::configure(SourceIndex::default(), "/nowhere");
        assert!(loader::request_refresh());
        // SAFETY: as above.
        unsafe { refresh_if_requested() };

        assert!(!loader::refresh_requested());
        assert!(!loader::is_running());
        loader::unconfigure();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "starts threads")]
    fn the_archive_scan_is_compared_with_the_result_of_the_engine() {
        let reference = Snapshot::default();
        let other = Snapshot {
            problems: vec!["only in the archive scan".to_owned()],
            ..Snapshot::default()
        };

        log_difference(&reference, &other, 2, 5);
        compare_with_archive_scan(&reference, SourceIndex::default(), "/nowhere");
    }
}
