//! Fills the table of spawn variables of the game module, from which it creates an entity.
//!
//! The game module reads the key/value pairs of an entity from a table in its level data, which it fills while it
//! parses the entity string of a map. Writing the pairs into that table and calling the function that creates an
//! entity from it spawns any entity with any keys, like the map would.

const NUM_VARS_OFFSET: usize = 0x994;
const VARS_OFFSET: usize = 0x998;
const NUM_CHARS_OFFSET: usize = 0xd98;
const CHARS_OFFSET: usize = 0xd9c;
const MAX_VARS: usize = 64;
const MAX_CHARS: usize = 4096;
const VAR_SIZE: usize = 16;

/// The table does not hold the pairs: too many, or too long together, or a string with a NUL byte.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SpawnVarsError;

/// Writes the pairs as the spawn variables of the level data at `level`.
///
/// # Safety
///
/// `level` must be the address of the level data of a loaded game module, or of memory with the same layout.
pub(crate) unsafe fn write_spawn_vars(
    level: usize,
    vars: &[(String, String)],
) -> Result<(), SpawnVarsError> {
    let needed = vars
        .iter()
        .map(|(key, value)| key.len() + value.len() + 2)
        .sum::<usize>();
    if vars.len() > MAX_VARS
        || needed > MAX_CHARS - 1
        || vars
            .iter()
            .any(|(key, value)| key.contains('\0') || value.contains('\0'))
    {
        return Err(SpawnVarsError);
    }

    let chars = level + CHARS_OFFSET;
    let mut used = 0;
    for (index, (key, value)) in vars.iter().enumerate() {
        let entry = level + VARS_OFFSET + index * VAR_SIZE;
        for (slot, text) in [(entry, key), (entry + 8, value)] {
            // SAFETY: the text and its NUL byte fit into the character table, which was checked above.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    text.as_ptr(),
                    (chars + used) as *mut u8,
                    text.len(),
                );
                ((chars + used + text.len()) as *mut u8).write(0);
                (slot as *mut usize).write_unaligned(chars + used);
            }
            used += text.len() + 1;
        }
    }
    // SAFETY: the counters are part of the level data.
    unsafe {
        ((level + NUM_VARS_OFFSET) as *mut i32).write_unaligned(vars.len() as i32);
        ((level + NUM_CHARS_OFFSET) as *mut i32).write_unaligned(used as i32);
    }
    Ok(())
}

/// Empties the table of spawn variables again.
///
/// # Safety
///
/// As for `write_spawn_vars`.
pub(crate) unsafe fn clear_spawn_vars(level: usize) {
    // SAFETY: the counters are part of the level data.
    unsafe {
        ((level + NUM_VARS_OFFSET) as *mut i32).write_unaligned(0);
        ((level + NUM_CHARS_OFFSET) as *mut i32).write_unaligned(0);
    }
}

#[cfg(test)]
mod spawn_vars_tests {
    use core::ffi::CStr;

    use pretty_assertions::assert_eq;

    use super::{
        CHARS_OFFSET, MAX_VARS, NUM_CHARS_OFFSET, NUM_VARS_OFFSET, SpawnVarsError, VARS_OFFSET,
        clear_spawn_vars, write_spawn_vars,
    };

    fn level() -> Vec<u64> {
        vec![0; (CHARS_OFFSET + 4096) / 8 + 1]
    }

    fn pairs(vars: &[(&str, &str)]) -> Vec<(String, String)> {
        vars.iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn read_i32(level: &[u64], offset: usize) -> i32 {
        // SAFETY: the offsets lie inside the vector.
        unsafe { ((level.as_ptr() as usize + offset) as *const i32).read_unaligned() }
    }

    #[test]
    fn the_pairs_are_written_as_strings_with_their_pointers() {
        let mut level = level();
        let base = level.as_mut_ptr() as usize;

        // SAFETY: the vector has the size of the level data.
        let result = unsafe {
            write_spawn_vars(
                base,
                &pairs(&[("classname", "item_armor_body"), ("origin", "1 2 3")]),
            )
        };

        assert_eq!(result, Ok(()));
        assert_eq!(read_i32(&level, NUM_VARS_OFFSET), 2);
        assert_eq!(
            read_i32(&level, NUM_CHARS_OFFSET),
            ("classname".len() + "item_armor_body".len() + "origin".len() + "1 2 3".len() + 4)
                as i32
        );
        let strings = (0..2)
            .flat_map(|index| [0, 8].map(|part| VARS_OFFSET + index * 16 + part))
            .map(|offset| {
                // SAFETY: the pointers were written by write_spawn_vars and point to NUL terminated strings.
                unsafe {
                    let pointer = ((base + offset) as *const usize).read_unaligned();
                    CStr::from_ptr(pointer as *const core::ffi::c_char)
                        .to_string_lossy()
                        .into_owned()
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(strings, ["classname", "item_armor_body", "origin", "1 2 3"]);
    }

    #[test]
    fn clearing_resets_both_counters() {
        let mut level = level();
        let base = level.as_mut_ptr() as usize;
        // SAFETY: the vector has the size of the level data.
        unsafe {
            write_spawn_vars(base, &pairs(&[("classname", "info_null")])).expect("fits");
            clear_spawn_vars(base);
        }

        assert_eq!(read_i32(&level, NUM_VARS_OFFSET), 0);
        assert_eq!(read_i32(&level, NUM_CHARS_OFFSET), 0);
    }

    #[test]
    fn too_many_pairs_too_much_text_and_nul_bytes_are_refused() {
        let mut level = level();
        let base = level.as_mut_ptr() as usize;
        let too_many = (0..=MAX_VARS)
            .map(|index| (format!("key{index}"), "v".to_string()))
            .collect::<Vec<_>>();
        let too_long = vec![("classname".to_string(), "x".repeat(5000))];

        // SAFETY: the vector has the size of the level data.
        unsafe {
            assert_eq!(write_spawn_vars(base, &too_many), Err(SpawnVarsError));
            assert_eq!(write_spawn_vars(base, &too_long), Err(SpawnVarsError));
            assert_eq!(
                write_spawn_vars(base, &pairs(&[("class\0name", "x")])),
                Err(SpawnVarsError)
            );
        }
        assert_eq!(read_i32(&level, NUM_VARS_OFFSET), 0);
    }
}
