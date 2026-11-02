use core::ptr;

use region::{Protection, Result};

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
const OFFSET_VOTE_CLIENTKICK_FIX: usize = 0x11C8;
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
const PTRN_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"\x39\xFE\x0F\x8D\x90\x00\x00\x00\x48\x69\xD6\xF8\x0B\x00\x00\x48\x01\xD0\x90\x90\x90\x00\x00\x00\x00\x00\x00\x00\x0f\x85\x76\x00\x00\x00\x90\x90\x90\x90";
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
const MASK_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"XXXXXXXXXXXXXXXXXXXXX-------XXXXXXXXXX";

pub(crate) fn patch_by_mask(
    orig_addr: usize,
    offset: usize,
    pattern: &[u8],
    mask: &[u8],
) -> Result<()> {
    let target = ptr::with_exposed_provenance_mut::<u8>(orig_addr + offset);

    let _protect_guard =
        unsafe { region::protect_with_handle(target, mask.len(), Protection::READ_WRITE_EXECUTE) }?;
    (0..mask.len())
        .filter(|i| mask[*i] == b'X')
        .for_each(|i| unsafe { ptr::write_unaligned(target.wrapping_add(i), pattern[i]) });
    Ok(())
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn patch_callvote_f(orig_addr: usize) -> Result<()> {
    patch_by_mask(
        orig_addr,
        OFFSET_VOTE_CLIENTKICK_FIX,
        PTRN_VOTE_CLIENTKICK_FIX,
        MASK_VOTE_CLIENTKICK_FIX,
    )
}

#[cfg(test)]
mod patches_tests {
    use pretty_assertions::assert_eq;

    use super::{patch_by_mask, patch_callvote_f};

    const PAGE_SIZE: usize = 4096;

    #[repr(align(4096))]
    struct Page([u8; 2 * PAGE_SIZE]);

    fn filled_pages() -> &'static mut Page {
        Box::leak(Box::new(Page([0xAA; 2 * PAGE_SIZE])))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_overwrites_only_the_masked_bytes() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        let result = patch_by_mask(base, 4, &[1, 2, 3, 4], b"X-X-");

        assert!(result.is_ok());
        assert_eq!(page.0[2..10], [0xAA, 0xAA, 1, 0xAA, 3, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_restores_the_original_protection() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();
        let protection_before = region::query(page.0.as_ptr()).unwrap().protection();

        patch_by_mask(base, 0, &[1], b"X").unwrap();

        assert_eq!(
            region::query(page.0.as_ptr()).unwrap().protection(),
            protection_before
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_protects_both_pages_of_a_pattern_crossing_the_page_boundary() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        let result = patch_by_mask(base, PAGE_SIZE - 2, &[1, 2, 3, 4], b"XXXX");

        assert!(result.is_ok());
        assert_eq!(
            page.0[PAGE_SIZE - 3..PAGE_SIZE + 3],
            [0xAA, 1, 2, 3, 4, 0xAA]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_reports_an_unmapped_address() {
        assert!(patch_by_mask(0, 0, &[1], b"X").is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_reports_an_empty_mask() {
        let page = filled_pages();

        assert!(patch_by_mask(page.0.as_ptr().addr(), 0, &[], b"").is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_callvote_f_reports_an_unmapped_address() {
        assert!(patch_callvote_f(0).is_err());
    }
}
