//! What Steam knows about the download of a workshop item.

const INSTALLED: u32 = 4;
const NEEDS_UPDATE: u32 = 8;
const DOWNLOADING: u32 = 16;
const DOWNLOAD_PENDING: u32 = 32;
const BYTES_PER_MEGABYTE: u64 = 1024 * 1024;

/// The flags of `EItemState` of the Steam UGC interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ItemState(pub(crate) u32);

impl ItemState {
    pub(crate) fn is_installed(self) -> bool {
        self.0 & INSTALLED != 0
    }

    pub(crate) fn needs_update(self) -> bool {
        self.0 & NEEDS_UPDATE != 0
    }

    /// Steam is downloading the item or has queued it.
    pub(crate) fn is_busy(self) -> bool {
        self.0 & (DOWNLOADING | DOWNLOAD_PENDING) != 0
    }

    pub(crate) fn is_queued(self) -> bool {
        self.0 & DOWNLOADING == 0 && self.0 & DOWNLOAD_PENDING != 0
    }
}

/// The bytes of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Progress {
    pub(crate) downloaded: u64,
    pub(crate) total: u64,
}

impl Progress {
    /// Like `42 % (120 of 285 MB)`; nothing while Steam does not know the size yet.
    pub(crate) fn describe(self) -> Option<String> {
        let percent = self
            .downloaded
            .saturating_mul(100)
            .checked_div(self.total)?;
        Some(format!(
            "{percent} % ({} of {} MB)",
            self.downloaded / BYTES_PER_MEGABYTE,
            self.total.div_ceil(BYTES_PER_MEGABYTE)
        ))
    }
}

/// The workshop of Steam as a game server sees it.
pub(crate) trait SteamWorkshop: Send + Sync {
    /// `None` when Steam cannot be asked.
    fn state(&self, id: u64) -> Option<ItemState>;

    fn progress(&self, id: u64) -> Option<Progress>;

    /// Asks Steam to fetch the newest version of the item; returns whether Steam accepted that.
    fn download(&self, id: u64) -> bool;
}

/// The item and the `EResult` code of the line the engine prints when a download fails.
pub(crate) fn download_failure(line: &str) -> Option<(u64, u32)> {
    let (id, code) = line
        .trim_end()
        .strip_prefix("Download item ")?
        .split_once(" failed with EResult code ")?;
    Some((id.parse().ok()?, code.parse().ok()?))
}

/// The Steam library that the engine has loaded.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SteamLibrary;

#[cfg(all(target_os = "linux", not(miri)))]
mod library {
    use core::ffi::{CStr, c_void};

    use super::{ItemState, Progress, SteamLibrary, SteamWorkshop};

    type InterfaceFn = unsafe extern "C" fn() -> *mut c_void;
    type StateFn = unsafe extern "C" fn(*mut c_void, u64) -> u32;
    type ProgressFn = unsafe extern "C" fn(*mut c_void, u64, *mut u64, *mut u64) -> bool;
    type DownloadFn = unsafe extern "C" fn(*mut c_void, u64, bool) -> bool;

    /// # Safety
    ///
    /// `T` must be the function pointer type of the symbol `name`.
    unsafe fn symbol<T: Copy>(name: &CStr) -> Option<T> {
        const { assert!(size_of::<T>() == size_of::<*mut c_void>()) };
        // SAFETY: the name is NUL-terminated.
        let address = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        // SAFETY: the caller guarantees the type of the symbol.
        (!address.is_null())
            .then(|| unsafe { core::mem::transmute_copy::<*mut c_void, T>(&address) })
    }

    fn game_server_ugc() -> Option<*mut c_void> {
        // SAFETY: the symbol takes nothing and returns the interface pointer.
        let accessor = unsafe { symbol::<InterfaceFn>(c"SteamGameServerUGC") }?;
        // SAFETY: the accessor only reads a global of the Steam library.
        let interface = unsafe { accessor() };
        (!interface.is_null()).then_some(interface)
    }

    impl SteamWorkshop for SteamLibrary {
        fn state(&self, id: u64) -> Option<ItemState> {
            let interface = game_server_ugc()?;
            // SAFETY: the flat API function of the Steam library has this signature.
            let state = unsafe { symbol::<StateFn>(c"SteamAPI_ISteamUGC_GetItemState") }?;
            // SAFETY: the interface pointer is the one of the game server.
            Some(ItemState(unsafe { state(interface, id) }))
        }

        fn progress(&self, id: u64) -> Option<Progress> {
            let interface = game_server_ugc()?;
            // SAFETY: the flat API function of the Steam library has this signature.
            let info = unsafe { symbol::<ProgressFn>(c"SteamAPI_ISteamUGC_GetItemDownloadInfo") }?;
            let (mut downloaded, mut total) = (0, 0);
            // SAFETY: the interface pointer is the one of the game server and both outputs are valid.
            unsafe { info(interface, id, &mut downloaded, &mut total) }
                .then_some(Progress { downloaded, total })
        }

        fn download(&self, id: u64) -> bool {
            let Some(interface) = game_server_ugc() else {
                return false;
            };
            // SAFETY: the flat API function of the Steam library has this signature.
            let Some(download) =
                (unsafe { symbol::<DownloadFn>(c"SteamAPI_ISteamUGC_DownloadItem") })
            else {
                return false;
            };
            // SAFETY: the interface pointer is the one of the game server.
            unsafe { download(interface, id, true) }
        }
    }
}

#[cfg(not(all(target_os = "linux", not(miri))))]
impl SteamWorkshop for SteamLibrary {
    fn state(&self, _id: u64) -> Option<ItemState> {
        None
    }

    fn progress(&self, _id: u64) -> Option<Progress> {
        None
    }

    fn download(&self, _id: u64) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::{ItemState, Progress, SteamLibrary, SteamWorkshop, download_failure};

    #[rstest]
    #[case(4, false, false, false)]
    #[case(4 | 8, true, false, false)]
    #[case(16, false, true, false)]
    #[case(32, false, true, true)]
    #[case(16 | 32, false, true, false)]
    fn reads_the_flags_of_an_item(
        #[case] flags: u32,
        #[case] needs_update: bool,
        #[case] busy: bool,
        #[case] queued: bool,
    ) {
        let state = ItemState(flags);

        assert_eq!(
            (state.needs_update(), state.is_busy(), state.is_queued()),
            (needs_update, busy, queued)
        );
    }

    #[test]
    fn describes_the_progress_once_the_size_is_known() {
        let unknown = Progress {
            downloaded: 0,
            total: 0,
        };
        let known = Progress {
            downloaded: 120 * 1024 * 1024,
            total: 285 * 1024 * 1024,
        };

        assert_eq!(unknown.describe(), None);
        assert_eq!(known.describe().as_deref(), Some("42 % (120 of 285 MB)"));
    }

    #[rstest]
    #[case("Download item 17 failed with EResult code 9\n", Some((17, 9)))]
    #[case("Download item 17 failed with EResult code 9", Some((17, 9)))]
    #[case("Download item x failed with EResult code 9", None)]
    #[case("Download completed for all steamworks items", None)]
    #[case("player: Download item 17 failed with EResult code 9", None)]
    fn reads_the_failure_line_of_the_engine(
        #[case] line: &str,
        #[case] expected: Option<(u64, u32)>,
    ) {
        assert_eq!(download_failure(line), expected);
    }

    #[test]
    fn without_the_steam_library_nothing_is_known() {
        assert_eq!(SteamLibrary.state(1), None);
        assert_eq!(SteamLibrary.progress(1), None);
        assert!(!SteamLibrary.download(1));
    }
}
