use core::ptr;
use log::error;
use region::Protection;

#[cfg_attr(test, allow(dead_code))]
const OFFSET_VOTE_CLIENTKICK_FIX: usize = 0x11C8;
#[cfg_attr(test, allow(dead_code))]
const PTRN_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"\x39\xFE\x0F\x8D\x90\x00\x00\x00\x48\x69\xD6\xF8\x0B\x00\x00\x48\x01\xD0\x90\x90\x90\x00\x00\x00\x00\x00\x00\x00\x0f\x85\x76\x00\x00\x00\x90\x90\x90\x90";
#[cfg_attr(test, allow(dead_code))]
const MASK_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"XXXXXXXXXXXXXXXXXXXXX-------XXXXXXXXXX";

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn patch_by_mask(orig_addr: usize, offset: usize, pattern: &[u8], mask: &[u8]) {
    let offset = (orig_addr + offset) as *mut u8;

    let page_size = region::page::size();

    match unsafe { region::protect_with_handle(offset, page_size, Protection::READ_WRITE_EXECUTE) }
    {
        Ok(_protect_guard) => {
            (0..mask.len())
                .filter(|i| mask[*i] == b'X')
                .for_each(|i| unsafe { ptr::write_unaligned(offset.wrapping_add(i), pattern[i]) });
        }
        Err(error) => {
            error!(target: "shinqlx", "{:?}", error);
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn patch_callvote_f(orig_addr: usize) {
    patch_by_mask(
        orig_addr,
        OFFSET_VOTE_CLIENTKICK_FIX,
        PTRN_VOTE_CLIENTKICK_FIX,
        MASK_VOTE_CLIENTKICK_FIX,
    );
}
