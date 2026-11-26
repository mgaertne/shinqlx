use pyo3::{intern, prelude::*};

use super::player_key;
use crate::python::{Player, PlayerMethods, clean_text};

pub(super) fn update_player(
    database: &Bound<'_, PyAny>,
    player: &Bound<'_, Player>,
) -> PyResult<()> {
    let py = database.py();
    let steam_id = player.get_steam_id();
    let base_key = player_key(steam_id);
    let pipeline = database.call_method0(intern!(py, "pipeline"))?;

    let ip = player.get_ip();
    if !ip.is_empty() {
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:ips", &ip))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("minqlx:ips:{ip}"), steam_id))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("{base_key}:ips"), &ip))?;
    }

    let name = player.get_name();
    if !database.contains(&base_key)? {
        pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:players", steam_id))?;
    } else {
        let clean_name = player.get_clean_name();
        let is_known = database
            .call_method1(intern!(py, "lrange"), (&base_key, 0, -1))?
            .try_iter()?
            .map(|known_name| known_name?.extract::<String>())
            .collect::<PyResult<Vec<_>>>()?
            .iter()
            .any(|known_name| clean_text(known_name) == clean_name);
        if !is_known {
            pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
            pipeline.call_method1(intern!(py, "ltrim"), (&base_key, 0, 19))?;
        }
    }

    pipeline.call_method0(intern!(py, "execute")).map(|_| ())
}
