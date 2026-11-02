use core::hint::cold_path;

use pyo3::{exceptions::PyValueError, types::PyTuple};

use super::prelude::*;

/// Extracts exactly `N` integers from a tuple; `subject` names the values in the length errors.
pub(super) fn extract_int_array<const N: usize>(
    values: &Bound<'_, PyTuple>,
    subject: &str,
    invalid_value_message: &str,
) -> PyResult<[i32; N]> {
    if values.len() < N {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "tuple did not provide values for all {N} {subject}"
        )));
    }

    if values.len() > N {
        cold_path();
        return Err(PyValueError::new_err(format!(
            "tuple did provide values for more than {N} {subject}"
        )));
    }

    let mut result = [0; N];
    for (slot, item) in result.iter_mut().zip(values.iter()) {
        *slot = item.extract::<i32>().map_err(|_| {
            cold_path();
            PyValueError::new_err(invalid_value_message.to_owned())
        })?;
    }
    Ok(result)
}
