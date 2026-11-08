use core::ptr;

use region::Protection;

use super::{PatchError, PatchOutcome};

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
const PTRN_VOTE_CLIENTKICK_ORIG: &[u8; 38] = b"\x31\xd2\xeb\x09\x83\xc2\x01\x48\x05\xf8\x0b\x00\x00\x39\xfa\x0f\x8d\x83\x00\x00\x00\x83\xb8\x50\x02\x00\x00\x02\x75\xe6\x3b\xb0\x88\x00\x00\x00\x75\xde";
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

fn masked_bytes_equal(actual: &[u8], expected: &[u8], mask: &[u8]) -> bool {
    actual
        .iter()
        .zip(expected)
        .zip(mask)
        .all(|((actual, expected), mask)| *mask != b'X' || actual == expected)
}

/// Overwrites the bytes marked `X` in `mask` with `pattern`, but only if the code there still
/// consists of the `original` bytes. Code that already carries the patch is left untouched.
pub(crate) fn patch_by_mask(
    orig_addr: usize,
    offset: usize,
    original: &[u8],
    pattern: &[u8],
    mask: &[u8],
) -> Result<PatchOutcome, PatchError> {
    if original.len() != mask.len() || pattern.len() != mask.len() {
        return Err(PatchError::LengthMismatch);
    }

    let target = ptr::with_exposed_provenance_mut::<u8>(orig_addr + offset);

    let _protect_guard =
        // SAFETY: the target range belongs to the loaded qagame module.
        unsafe { region::protect_with_handle(target, mask.len(), Protection::READ_WRITE_EXECUTE) }
            .map_err(PatchError::Protect)?;
    // SAFETY: the range is readable and was made writable before.
    let current = unsafe { core::slice::from_raw_parts(target.cast_const(), mask.len()) };

    if masked_bytes_equal(current, pattern, mask) {
        return Ok(PatchOutcome::AlreadyApplied);
    }
    if !masked_bytes_equal(current, original, mask) {
        return Err(PatchError::UnexpectedCode);
    }

    (0..mask.len())
        .filter(|i| mask[*i] == b'X')
        // SAFETY: the range is writable and the index lies within the mask.
        .for_each(|i| unsafe { ptr::write_unaligned(target.wrapping_add(i), pattern[i]) });
    Ok(PatchOutcome::Applied)
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only called from the running game process, not from tests"
    )
)]
pub(crate) fn patch_callvote_f(orig_addr: usize) -> Result<PatchOutcome, PatchError> {
    patch_by_mask(
        orig_addr,
        OFFSET_VOTE_CLIENTKICK_FIX,
        PTRN_VOTE_CLIENTKICK_ORIG,
        PTRN_VOTE_CLIENTKICK_FIX,
        MASK_VOTE_CLIENTKICK_FIX,
    )
}

#[cfg(test)]
mod patches_tests {
    use pretty_assertions::assert_eq;

    use super::{
        MASK_VOTE_CLIENTKICK_FIX, OFFSET_VOTE_CLIENTKICK_FIX, PTRN_VOTE_CLIENTKICK_FIX,
        PTRN_VOTE_CLIENTKICK_ORIG, PatchError, PatchOutcome, patch_by_mask, patch_callvote_f,
    };

    const PAGE_SIZE: usize = 4096;

    #[repr(align(4096))]
    struct Page([u8; 2 * PAGE_SIZE]);

    fn filled_pages() -> &'static mut Page {
        Box::leak(Box::new(Page([0xAA; 2 * PAGE_SIZE])))
    }

    #[test]
    fn the_shipped_patch_has_consistent_lengths() {
        assert_eq!(
            PTRN_VOTE_CLIENTKICK_ORIG.len(),
            MASK_VOTE_CLIENTKICK_FIX.len()
        );
        assert_eq!(
            PTRN_VOTE_CLIENTKICK_FIX.len(),
            MASK_VOTE_CLIENTKICK_FIX.len()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_overwrites_only_the_masked_bytes() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        let result = patch_by_mask(base, 4, &[0xAA; 4], &[1, 2, 3, 4], b"X-X-");

        assert!(matches!(result, Ok(PatchOutcome::Applied)));
        assert_eq!(page.0[2..10], [0xAA, 0xAA, 1, 0xAA, 3, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_ignores_unmasked_bytes_when_comparing() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        let result = patch_by_mask(base, 4, &[0xAA, 0x55, 0xAA, 0x55], &[1, 2, 3, 4], b"X-X-");

        assert!(matches!(result, Ok(PatchOutcome::Applied)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_refuses_unexpected_code() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        let result = patch_by_mask(base, 4, &[0xAA, 0xAA, 0x77, 0xAA], &[1, 2, 3, 4], b"XXXX");

        assert!(matches!(result, Err(PatchError::UnexpectedCode)));
        assert_eq!(page.0[2..10], [0xAA; 8]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_leaves_already_patched_code_alone() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();
        patch_by_mask(base, 4, &[0xAA; 4], &[1, 2, 3, 4], b"XXXX").unwrap();

        let result = patch_by_mask(base, 4, &[0xAA; 4], &[1, 2, 3, 4], b"XXXX");

        assert!(matches!(result, Ok(PatchOutcome::AlreadyApplied)));
        assert_eq!(page.0[2..10], [0xAA, 0xAA, 1, 2, 3, 4, 0xAA, 0xAA]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_rejects_lengths_that_differ() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        assert!(matches!(
            patch_by_mask(base, 0, &[0xAA; 2], &[1, 2, 3], b"XXX"),
            Err(PatchError::LengthMismatch)
        ));
        assert!(matches!(
            patch_by_mask(base, 0, &[0xAA; 3], &[1, 2], b"XXX"),
            Err(PatchError::LengthMismatch)
        ));
        assert_eq!(page.0[0..4], [0xAA; 4]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_restores_the_original_protection() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();
        let protection_before = region::query(page.0.as_ptr()).unwrap().protection();

        patch_by_mask(base, 0, &[0xAA], &[1], b"X").unwrap();

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

        let result = patch_by_mask(base, PAGE_SIZE - 2, &[0xAA; 4], &[1, 2, 3, 4], b"XXXX");

        assert!(result.is_ok());
        assert_eq!(
            page.0[PAGE_SIZE - 3..PAGE_SIZE + 3],
            [0xAA, 1, 2, 3, 4, 0xAA]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_reports_an_unmapped_address() {
        assert!(matches!(
            patch_by_mask(0, 0, &[0xAA], &[1], b"X"),
            Err(PatchError::Protect(_))
        ));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_by_mask_reports_an_empty_mask() {
        let page = filled_pages();

        assert!(patch_by_mask(page.0.as_ptr().addr(), 0, &[], &[], b"").is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_callvote_f_reports_an_unmapped_address() {
        assert!(patch_callvote_f(0).is_err());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_callvote_f_patches_the_original_code_once() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();
        let target = OFFSET_VOTE_CLIENTKICK_FIX;
        page.0[target..target + PTRN_VOTE_CLIENTKICK_ORIG.len()]
            .copy_from_slice(PTRN_VOTE_CLIENTKICK_ORIG);

        assert!(matches!(patch_callvote_f(base), Ok(PatchOutcome::Applied)));
        assert!(matches!(
            patch_callvote_f(base),
            Ok(PatchOutcome::AlreadyApplied)
        ));
        assert_eq!(page.0[target..target + 4], PTRN_VOTE_CLIENTKICK_FIX[..4]);
        assert_eq!(
            page.0[target + 21..target + 28],
            PTRN_VOTE_CLIENTKICK_ORIG[21..28]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn patch_callvote_f_refuses_foreign_code() {
        let page = filled_pages();
        let base = page.0.as_ptr().addr();

        assert!(matches!(
            patch_callvote_f(base),
            Err(PatchError::UnexpectedCode)
        ));
        assert_eq!(page.0[..64], [0xAA; 64]);
    }

    #[test]
    fn patch_errors_describe_themselves() {
        assert_eq!(
            PatchError::LengthMismatch.to_string(),
            "original bytes, patch and mask differ in length"
        );
        assert_eq!(
            PatchError::UnexpectedCode.to_string(),
            "the code at the target is not the expected one"
        );
    }
}
