use pyo3::{intern, prelude::*};

use super::{player_key, plugin_class, steam_id_of};

pub(super) fn update_player(
    database: &Bound<'_, PyAny>,
    player: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = database.py();
    let steam_id = steam_id_of(player)?;
    let base_key = player_key(steam_id);
    let pipeline = database.call_method0(intern!(py, "pipeline"))?;

    let ip = player.getattr(intern!(py, "ip"))?.extract::<String>()?;
    if !ip.is_empty() {
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:ips", &ip))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("minqlx:ips:{ip}"), steam_id))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("{base_key}:ips"), &ip))?;
    }

    let name = player.getattr(intern!(py, "name"))?;
    if !database.contains(&base_key)? {
        pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:players", steam_id))?;
    } else {
        let clean_name = player.getattr(intern!(py, "clean_name"))?;
        let known = database
            .call_method1(intern!(py, "lrange"), (&base_key, 0, -1))?
            .try_iter()?
            .map(|known_name| {
                plugin_class(py).call_method1(intern!(py, "clean_text"), (known_name?,))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let mut is_known = false;
        for known_name in &known {
            is_known |= known_name.eq(&clean_name)?;
        }
        if !is_known {
            pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
            pipeline.call_method1(intern!(py, "ltrim"), (&base_key, 0, 19))?;
        }
    }

    pipeline.call_method0(intern!(py, "execute")).map(|_| ())
}
