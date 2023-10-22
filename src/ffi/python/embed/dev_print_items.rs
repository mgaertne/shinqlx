use crate::prelude::*;
use crate::quake_live_engine::{ComPrintf, SendServerCommand};
use crate::MAIN_ENGINE;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::{pyfunction, PyResult, Python};

/// Prints all items and entity numbers to server console.
#[pyfunction]
#[pyo3(name = "dev_print_items")]
pub(crate) fn minqlx_dev_print_items(py: Python<'_>) -> PyResult<()> {
    let formatted_items: Vec<String> = py.allow_threads(|| {
        (0..MAX_GENTITIES)
            .filter_map(|i| GameEntity::try_from(i as i32).ok())
            .filter(|game_entity| {
                game_entity.in_use() && game_entity.is_game_item(entityType_t::ET_ITEM)
            })
            .map(|game_entity| {
                format!(
                    "{} {}",
                    game_entity.get_entity_id(),
                    game_entity.get_classname()
                )
            })
            .collect()
    });
    let mut str_length = 0;
    let printed_items: Vec<String> = formatted_items
        .iter()
        .take_while(|&item| {
            str_length += item.len();
            str_length < 1024
        })
        .map(|item| item.into())
        .collect();

    py.allow_threads(move || {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        if printed_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"No items found in the map\n\"".to_string(),
            );
            return Ok(());
        }
        main_engine.send_server_command(
            None::<Client>,
            format!("print \"{}\n\"", printed_items.join("\n")),
        );

        let remaining_items: Vec<String> = formatted_items
            .iter()
            .skip(printed_items.len())
            .map(|item| item.into())
            .collect();

        if !remaining_items.is_empty() {
            #[allow(clippy::unnecessary_to_owned)]
            main_engine.send_server_command(
                None::<Client>,
                "print \"Check server console for other items\n\"\n".to_string(),
            );
            remaining_items
                .into_iter()
                .for_each(|item| main_engine.com_printf(item));
        }

        Ok(())
    })
}
