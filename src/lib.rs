mod error;
pub use error::AutoparqError;

pub mod advisor;
pub mod apply;
pub mod bench;
pub mod codegen;
pub mod diagnostics;
pub mod profiler;
pub mod recommender;
pub mod tuner;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use profiler::metadata::read_file_metadata;
#[cfg(feature = "python")]
use recommender::codec::{Priority, Engine};
#[cfg(feature = "python")]
use tuner::build_tune_report;
#[cfg(feature = "python")]
use std::path::Path;

#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (path, columns_filter=None))]
fn py_info_file(path: &str, columns_filter: Option<Vec<String>>) -> PyResult<String> {
    let p = Path::new(path);
    let mut profile = read_file_metadata(p)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    // Apply column filter if provided
    if let Some(filter) = columns_filter {
        if !filter.is_empty() {
            profile.columns.retain(|c| filter.contains(&c.name));
        }
    }

    serde_json::to_string(&profile)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (path, engine="unknown", priority="balanced", sample_rows=2_000_000, explain="brief"))]
fn py_tune_file(path: &str, engine: &str, priority: &str, sample_rows: usize, explain: &str) -> PyResult<String> {
    let p = std::path::Path::new(path);
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let report = build_tune_report(p, &eng, &pri, sample_rows, explain)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    serde_json::to_string(&report)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (path, column_name, codecs_json, encodings_json))]
fn py_bench_column(
    path: &str,
    column_name: &str,
    codecs_json: &str,
    encodings_json: &str,
) -> PyResult<String> {
    use crate::bench::{benchmark_column, default_codecs, valid_encodings_for_type};

    let p = Path::new(path);

    let codecs: Vec<(String, Option<i32>)> = if codecs_json.is_empty() || codecs_json == "null" {
        default_codecs()
    } else {
        serde_json::from_str(codecs_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
    };

    let encodings: Vec<String> = if encodings_json.is_empty() || encodings_json == "null" {
        let meta = crate::profiler::metadata::read_file_metadata(p)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let col_meta = meta
            .columns
            .iter()
            .find(|c| c.name == column_name)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Column '{}' not found",
                    column_name
                ))
            })?;
        valid_encodings_for_type(&col_meta.physical_type)
    } else {
        serde_json::from_str(encodings_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
    };

    let result = benchmark_column(p, column_name, &codecs, &encodings)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    serde_json::to_string(&result)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (input_path, output_path, engine="unknown", priority="balanced", sample_rows=500_000))]
fn py_apply_file(
    input_path: &str,
    output_path: &str,
    engine: &str,
    priority: &str,
    sample_rows: usize,
) -> PyResult<String> {
    use crate::apply::rewrite_file;
    use std::path::Path;

    let engine_val = Engine::from_str(engine);
    let priority_val = Priority::from_str(priority);

    let result = rewrite_file(
        Path::new(input_path),
        Path::new(output_path),
        engine_val,
        priority_val,
        sample_rows,
    )
    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    serde_json::to_string(&result)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[cfg(feature = "python")]
#[pyfunction]
fn py_generate_snippet(report_json: &str, engine: &str) -> PyResult<String> {
    let report: crate::tuner::TuneReport = serde_json::from_str(report_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(crate::codegen::generate_snippet(&report, engine))
}

#[cfg(feature = "python")]
#[pymodule]
fn _lib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_info_file, m)?)?;
    m.add_function(wrap_pyfunction!(py_tune_file, m)?)?;
    m.add_function(wrap_pyfunction!(py_bench_column, m)?)?;
    m.add_function(wrap_pyfunction!(py_apply_file, m)?)?;
    m.add_function(wrap_pyfunction!(py_generate_snippet, m)?)?;
    Ok(())
}

#[cfg(feature = "wasm")]
mod wasm;
