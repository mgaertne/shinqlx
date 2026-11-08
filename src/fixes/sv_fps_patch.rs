use alloc::vec::Vec;
use core::{ffi::CStr, mem::size_of, ops::Range, ptr};

use region::Protection;

use crate::fixes::{PatchError, PatchOutcome};

const CVAR_NAME_SV_FPS: &[u8] = b"\0sv_fps\0";
const STOCK_SV_FPS: &CStr = c"40";
const PATCHED_SV_FPS: &CStr = c"125";
const TABLE_DEFAULT_SLOT: usize = 8;
const TABLE_FLAGS: usize = 16;
const TABLE_ENTRY_END: usize = 24;
const CVAR_ROM: u32 = 64;

/// A mapped, readable range of the game module.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only built from the running game process, not from tests"
    )
)]
pub(crate) struct ModuleRegion {
    pub(crate) range: Range<usize>,
    pub(crate) writable: bool,
}

fn region_bytes(region: &ModuleRegion) -> &[u8] {
    // SAFETY: the region is a readable mapping of the loaded game module.
    unsafe {
        core::slice::from_raw_parts(
            ptr::with_exposed_provenance::<u8>(region.range.start),
            region.range.len(),
        )
    }
}

fn read_u64(address: usize) -> u64 {
    // SAFETY: callers keep the address inside a readable region.
    unsafe { ptr::read_unaligned(ptr::with_exposed_provenance::<u64>(address)) }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: callers keep the address inside a readable region.
    unsafe { ptr::read_unaligned(ptr::with_exposed_provenance::<u32>(address)) }
}

fn c_string_at(regions: &[ModuleRegion], address: usize) -> Option<&CStr> {
    regions
        .iter()
        .find(|region| region.range.contains(&address))
        .and_then(|region| {
            CStr::from_bytes_until_nul(&region_bytes(region)[address - region.range.start..]).ok()
        })
}

fn name_addresses(regions: &[ModuleRegion]) -> Vec<usize> {
    regions
        .iter()
        .flat_map(|region| {
            region_bytes(region)
                .windows(CVAR_NAME_SV_FPS.len())
                .enumerate()
                .filter(|(_, window)| *window == CVAR_NAME_SV_FPS)
                .map(|(index, _)| region.range.start + index + 1)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Finds the entries `{ vmCvar, "sv_fps", "40", CVAR_ROM, .. }` of the cvar table of the game
/// module. Returns the address of their slot with the default value and whether the slot already
/// holds the patched default.
fn table_entries(regions: &[ModuleRegion]) -> Vec<(usize, bool)> {
    let names = name_addresses(regions);
    regions
        .iter()
        .filter(|region| region.writable)
        .flat_map(|region| {
            let last = region.range.end.saturating_sub(TABLE_ENTRY_END);
            (region.range.start..last)
                .step_by(size_of::<u64>())
                .filter(|&slot| names.contains(&(read_u64(slot) as usize)))
                .filter(|&slot| read_u32(slot + TABLE_FLAGS) == CVAR_ROM)
                .filter_map(|slot| {
                    let default_slot = slot + TABLE_DEFAULT_SLOT;
                    let default = c_string_at(regions, read_u64(default_slot) as usize)?;
                    (default == STOCK_SV_FPS || default == PATCHED_SV_FPS)
                        .then_some((default_slot, default == PATCHED_SV_FPS))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Makes the game module register `sv_fps` with the default 125. The game module registers the
/// cvar as read-only with the default 40, which resets any configured value.
///
/// The entry of the cvar table is found by its layout and not by an offset, and it is only
/// changed if exactly one entry looks like it.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn patch_sv_fps(regions: &[ModuleRegion]) -> Result<PatchOutcome, PatchError> {
    let entries = table_entries(regions);
    let &[(default_slot, patched)] = entries.as_slice() else {
        return Err(if entries.is_empty() {
            PatchError::TableEntryNotFound
        } else {
            PatchError::TableEntryAmbiguous
        });
    };
    if patched {
        return Ok(PatchOutcome::AlreadyApplied);
    }

    let target = ptr::with_exposed_provenance_mut::<u64>(default_slot);
    let _protect_guard =
        // SAFETY: the slot lies inside a writable region of the loaded game module.
        unsafe { region::protect_with_handle(target, size_of::<u64>(), Protection::READ_WRITE) }
            .map_err(PatchError::Protect)?;
    // SAFETY: the slot is writable and holds the address of the default value.
    unsafe { ptr::write_unaligned(target, PATCHED_SV_FPS.as_ptr().expose_provenance() as u64) };
    Ok(PatchOutcome::Applied)
}

#[cfg(test)]
mod sv_fps_tests {
    use pretty_assertions::assert_eq;

    use super::{ModuleRegion, PATCHED_SV_FPS, patch_sv_fps};
    use crate::fixes::{PatchError, PatchOutcome};

    const PAGE_SIZE: usize = 4096;
    const TABLE: usize = 128;
    const ENTRY_SIZE: usize = 32;
    const STOCK_DEFAULT: usize = 16;
    const PATCHED_DEFAULT: usize = 24;
    const FOREIGN_DEFAULT: usize = 32;

    #[repr(align(4096))]
    struct Page([u8; PAGE_SIZE]);

    struct FakeModule {
        page: &'static mut Page,
    }

    impl FakeModule {
        fn new(name: &[u8]) -> Self {
            let page = Box::leak(Box::new(Page([0; PAGE_SIZE])));
            page.0[..name.len()].copy_from_slice(name);
            page.0[STOCK_DEFAULT..STOCK_DEFAULT + 3].copy_from_slice(b"40\0");
            page.0[PATCHED_DEFAULT..PATCHED_DEFAULT + 4].copy_from_slice(b"125\0");
            page.0[FOREIGN_DEFAULT..FOREIGN_DEFAULT + 2].copy_from_slice(b"7\0");
            Self { page }
        }

        fn with_sv_fps() -> Self {
            Self::new(b"\0sv_fps\0")
        }

        fn address(&self, offset: usize) -> usize {
            self.page.0.as_ptr().addr() + offset
        }

        fn put(&mut self, at: usize, value: &[u8]) {
            self.page.0[at..at + value.len()].copy_from_slice(value);
        }

        fn add_entry(&mut self, index: usize, default: usize, flags: u32) {
            let at = TABLE + index * ENTRY_SIZE;
            let name = (self.address(1) as u64).to_ne_bytes();
            let default = (self.address(default) as u64).to_ne_bytes();
            self.put(at, &name);
            self.put(at + 8, &default);
            self.put(at + 16, &flags.to_ne_bytes());
        }

        fn default_pointer(&self, index: usize) -> u64 {
            let at = TABLE + index * ENTRY_SIZE + 8;
            u64::from_ne_bytes(self.page.0[at..at + 8].try_into().expect("eight bytes"))
        }

        fn regions(&self) -> Vec<ModuleRegion> {
            vec![ModuleRegion {
                range: self.address(0)..self.address(PAGE_SIZE),
                writable: true,
            }]
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_replaces_the_stock_default() {
        let mut module = FakeModule::with_sv_fps();
        module.add_entry(0, STOCK_DEFAULT, 64);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Ok(PatchOutcome::Applied)));
        assert_eq!(
            module.default_pointer(0),
            PATCHED_SV_FPS.as_ptr().addr() as u64
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_leaves_an_applied_patch_alone() {
        let mut module = FakeModule::with_sv_fps();
        module.add_entry(0, PATCHED_DEFAULT, 64);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Ok(PatchOutcome::AlreadyApplied)));
        assert_eq!(
            module.default_pointer(0),
            module.address(PATCHED_DEFAULT) as u64
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_ignores_entries_that_are_not_read_only() {
        let mut module = FakeModule::with_sv_fps();
        module.add_entry(0, STOCK_DEFAULT, 0);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Err(PatchError::TableEntryNotFound)));
        assert_eq!(
            module.default_pointer(0),
            module.address(STOCK_DEFAULT) as u64
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_ignores_entries_with_a_foreign_default() {
        let mut module = FakeModule::with_sv_fps();
        module.add_entry(0, FOREIGN_DEFAULT, 64);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Err(PatchError::TableEntryNotFound)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_refuses_ambiguous_entries() {
        let mut module = FakeModule::with_sv_fps();
        module.add_entry(0, STOCK_DEFAULT, 64);
        module.add_entry(1, STOCK_DEFAULT, 64);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Err(PatchError::TableEntryAmbiguous)));
        assert_eq!(
            module.default_pointer(0),
            module.address(STOCK_DEFAULT) as u64
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_needs_the_exact_cvar_name() {
        let mut module = FakeModule::new(b"\0xsv_fps\0");
        module.add_entry(0, STOCK_DEFAULT, 64);

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Err(PatchError::TableEntryNotFound)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_sv_fps_reports_a_module_without_entries() {
        let module = FakeModule::with_sv_fps();

        let result = patch_sv_fps(&module.regions());

        assert!(matches!(result, Err(PatchError::TableEntryNotFound)));
    }

    #[test]
    fn table_errors_describe_themselves() {
        assert_eq!(
            PatchError::TableEntryNotFound.to_string(),
            "no cvar table entry with the expected layout"
        );
        assert_eq!(
            PatchError::TableEntryAmbiguous.to_string(),
            "several cvar table entries with the expected layout"
        );
    }
}
