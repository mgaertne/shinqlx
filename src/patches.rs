use crate::CMD_CALLVOTE_F_ORIG_PTR;
use region::Protection;
use std::sync::atomic::Ordering;

const OFFSET_VOTE_CLIENTKICK_FIX: u64 = 0x11C8;
const PTRN_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"\x39\xFE\x0F\x8D\x90\x00\x00\x00\x48\x69\xD6\xF8\x0B\x00\x00\x48\x01\xD0\x90\x90\x90\x00\x00\x00\x00\x00\x00\x00\x0f\x85\x76\x00\x00\x00\x90\x90\x90\x90";
const MASK_VOTE_CLIENTKICK_FIX: &[u8; 38] = b"XXXXXXXXXXXXXXXXXXXXX-------XXXXXXXXXX";

fn patch_by_mask(orig_addr: u64, offset: u64, pattern: &[u8], mask: &[u8]) {
    let offset = (orig_addr + offset) as *mut u8;

    let page_size = region::page::size();

    match unsafe { region::protect_with_handle(offset, page_size, Protection::READ_WRITE_EXECUTE) }
    {
        Ok(_protect_guard) => {
            for i in 0..mask.len() {
                if mask[i] != b'X' {
                    continue;
                }
                unsafe { std::ptr::write_unaligned(offset.wrapping_add(i), pattern[i]) };
            }
        }
        Err(error) => {
            debug_println!(format!("{}", error));
        }
    }
}

pub(crate) fn patch_vm() {
    let cmd_callvote_addr = CMD_CALLVOTE_F_ORIG_PTR.load(Ordering::Relaxed);
    if cmd_callvote_addr == 0 {
        return;
    }
    patch_by_mask(
        cmd_callvote_addr,
        OFFSET_VOTE_CLIENTKICK_FIX,
        PTRN_VOTE_CLIENTKICK_FIX,
        MASK_VOTE_CLIENTKICK_FIX,
    );
}
