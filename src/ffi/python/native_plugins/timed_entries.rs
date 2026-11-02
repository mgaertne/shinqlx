use std::{
    collections::HashMap,
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

use pyo3::{
    exceptions::PyTypeError,
    intern,
    prelude::*,
    types::{IntoPyDict, PyDict, PyTuple},
};
use regex_lite::Regex;

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const SECONDS_PER_DAY: f64 = 86_400.0;

static LENGTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<number>[0-9]+) (?P<scale>seconds?|minutes?|hours?|days?|weeks?|months?|years?)",
    )
    .expect("the pattern for the length of an entry is a valid regex")
});

/// The latest active entry of a sorted set of timed entries, such as the bans or silences of a player.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub(super) expires: String,
    pub(super) score: f64,
    pub(super) reason: String,
}

pub(super) fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

pub(super) fn local_time(py: Python<'_>, epoch: f64) -> PyResult<String> {
    let time = py.import(intern!(py, "time"))?;
    time.call_method1(
        intern!(py, "strftime"),
        (
            TIME_FORMAT,
            time.call_method1(intern!(py, "localtime"), (epoch,))?,
        ),
    )?
    .extract()
}

pub(super) fn expiry_datetime<'py>(py: Python<'py>, expires: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(intern!(py, "datetime"))?
        .getattr(intern!(py, "datetime"))?
        .call_method1(intern!(py, "strptime"), (expires, TIME_FORMAT))
}

/// Converts lengths such as `2 days` into seconds. Months count as 30 days and years as 52 weeks.
pub(super) fn length_in_seconds(length: &str) -> Option<f64> {
    let length = length.to_lowercase();
    let captures = LENGTH.captures(&length)?;
    let number = captures["number"].parse::<f64>().ok()?;
    let unit = match captures["scale"].trim_end_matches('s') {
        "second" => 1.0,
        "minute" => 60.0,
        "hour" => 3_600.0,
        "day" => SECONDS_PER_DAY,
        "week" => 7.0 * SECONDS_PER_DAY,
        "month" => 30.0 * SECONDS_PER_DAY,
        "year" => 52.0 * 7.0 * SECONDS_PER_DAY,
        _ => return None,
    };
    (number > 0.0).then_some(number * unit)
}

fn redis_before_3(py: Python<'_>) -> PyResult<bool> {
    py.import(intern!(py, "redis"))?
        .getattr(intern!(py, "VERSION"))?
        .lt(PyTuple::new(py, [3])?)
}

/// The ids and expiries of the entries under `key` that have not yet expired, the latest last.
pub(super) fn active_entries(
    database: &Bound<'_, PyAny>,
    key: &str,
) -> PyResult<Vec<(String, f64)>> {
    let py = database.py();
    database
        .call_method(
            intern!(py, "zrangebyscore"),
            (key, now(), "+inf"),
            Some(&[("withscores", true)].into_py_dict(py)?),
        )?
        .extract()
}

pub(super) fn latest_entry(database: &Bound<'_, PyAny>, key: &str) -> PyResult<Option<Entry>> {
    let py = database.py();
    let entries = active_entries(database, key)?;
    let Some((entry_id, score)) = entries.last() else {
        return Ok(None);
    };

    let details = database
        .call_method1(intern!(py, "hgetall"), (format!("{key}:{entry_id}"),))?
        .extract::<HashMap<String, String>>()?;
    Ok(Some(Entry {
        expires: match details.get("expires") {
            Some(expires) => expires.clone(),
            None => local_time(py, *score)?,
        },
        score: *score,
        reason: details.get("reason").cloned().unwrap_or_default(),
    }))
}

fn queue_entry(
    pipeline: &Bound<'_, PyAny>,
    key: &str,
    entry_id: i64,
    score: f64,
    details: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let py = pipeline.py();
    if redis_before_3(py)? {
        pipeline.call_method1(intern!(py, "zadd"), (key, score, entry_id))?;
    } else {
        let scores = PyDict::new(py);
        scores.set_item(entry_id, score)?;
        pipeline.call_method1(intern!(py, "zadd"), (key, scores))?;
    }

    let details_key = format!("{key}:{entry_id}");
    let with_mapping = [("mapping", details)].into_py_dict(py)?;
    match pipeline.call_method(intern!(py, "hset"), (&details_key,), Some(&with_mapping)) {
        Err(err) if err.is_instance_of::<PyTypeError>(py) => {
            pipeline.call_method1(intern!(py, "hmset"), (&details_key, details))?;
        }
        result => {
            result?;
        }
    }
    Ok(())
}

/// Stores a new entry under `key` that lasts for `seconds` from now.
pub(super) fn store_entry(
    database: &Bound<'_, PyAny>,
    key: &str,
    issued_by: &Bound<'_, PyAny>,
    seconds: f64,
    reason: &str,
) -> PyResult<Entry> {
    let py = database.py();
    let issued_at = now();
    let score = issued_at + seconds;
    let expires = local_time(py, score)?;
    let entry_id = database
        .call_method1(intern!(py, "zcard"), (key,))?
        .extract::<i64>()?;
    let details = PyDict::new(py);
    details.set_item("expires", &expires)?;
    details.set_item("reason", reason)?;
    details.set_item("issued", local_time(py, issued_at)?)?;
    details.set_item("issued_by", issued_by)?;

    let pipeline = database.call_method0(intern!(py, "pipeline"))?;
    queue_entry(&pipeline, key, entry_id, score, &details)?;
    pipeline.call_method0(intern!(py, "execute"))?;

    Ok(Entry {
        expires,
        score,
        reason: reason.to_owned(),
    })
}

/// Expires the given entries immediately by lowering their score to zero.
pub(super) fn revoke_entries(
    database: &Bound<'_, PyAny>,
    key: &str,
    entries: &[(String, f64)],
) -> PyResult<()> {
    let py = database.py();
    let before_3 = redis_before_3(py)?;
    let pipeline = database.call_method0(intern!(py, "pipeline"))?;
    for (entry_id, score) in entries {
        if before_3 {
            pipeline.call_method1(intern!(py, "zincrby"), (key, entry_id, -score))?;
        } else {
            pipeline.call_method1(intern!(py, "zincrby"), (key, -score, entry_id))?;
        }
    }
    pipeline.call_method0(intern!(py, "execute")).map(|_| ())
}

#[cfg(test)]
mod timed_entries_tests {
    use rstest::rstest;

    use super::length_in_seconds;

    #[rstest]
    #[case("1 second", Some(1.0))]
    #[case("2 Minutes", Some(120.0))]
    #[case("1 hour", Some(3_600.0))]
    #[case("3 days", Some(259_200.0))]
    #[case("1 week", Some(604_800.0))]
    #[case("1 month", Some(2_592_000.0))]
    #[case("1 year", Some(31_449_600.0))]
    #[case("1 day and more", Some(86_400.0))]
    #[case("0 days", None)]
    #[case("x days", None)]
    #[case("5 parsecs", None)]
    #[case("1day", None)]
    #[case("", None)]
    fn lengths_are_converted_to_seconds(#[case] length: &str, #[case] expected: Option<f64>) {
        assert_eq!(length_in_seconds(length), expected);
    }
}
