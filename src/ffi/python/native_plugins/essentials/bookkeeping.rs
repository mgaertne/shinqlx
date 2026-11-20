use pyo3::{intern, prelude::*, types::PyString};

use super::{player_key, steam_id_of};
use crate::ffi::python::clean_py_text;

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
            .map(|known_name| clean_py_text(known_name?.cast::<PyString>()?))
            .collect::<PyResult<Vec<_>>>()?;
        let mut is_known = false;
        for known_name in &known {
            is_known |= known_name.as_any().eq(&clean_name)?;
        }
        if !is_known {
            pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
            pipeline.call_method1(intern!(py, "ltrim"), (&base_key, 0, 19))?;
        }
    }

    pipeline.call_method0(intern!(py, "execute")).map(|_| ())
}
