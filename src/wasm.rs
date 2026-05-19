#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;
use crate::recommender::codec::{Engine, Priority};

#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn tune_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
) -> Result<String, JsError> {
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let report = crate::tuner::build_tune_report_from_bytes(
        data, &eng, &pri, sample_rows as usize, "full",
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&report).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn tune_file_bytes_with_progress(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
    on_progress: &js_sys::Function,
) -> Result<String, JsError> {
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let this = JsValue::null();
    let report = crate::tuner::build_tune_report_from_bytes_with_progress(
        data,
        &eng,
        &pri,
        sample_rows as usize,
        "full",
        |current, total, col_name| {
            let _ = on_progress.call3(
                &this,
                &JsValue::from(current as u32),
                &JsValue::from(total as u32),
                &JsValue::from_str(col_name),
            );
        },
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&report).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn recommend_from_profile(
    report_json: &str,
    engine: &str,
    priority: &str,
) -> Result<String, JsError> {
    let cached: crate::tuner::TuneReport = serde_json::from_str(report_json)
        .map_err(|e| JsError::new(&format!("Invalid cached report: {}", e)))?;
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let report = crate::tuner::build_tune_report_from_profiles(
        &cached.file_profile,
        &cached.column_profiles,
        &eng,
        &pri,
        "full",
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&report).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn generate_snippet(report_json: &str, engine: &str) -> Result<String, JsError> {
    let report: crate::tuner::TuneReport = serde_json::from_str(report_json)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(crate::codegen::generate_snippet(&report, engine))
}

#[wasm_bindgen]
pub fn apply_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
) -> Result<JsValue, JsError> {
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let result = crate::apply::rewrite_file_from_bytes(data, eng, pri, 500_000)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let summary_json = serde_json::to_string(&result.summary)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let obj = js_sys::Object::new();
    let output_arr = js_sys::Uint8Array::from(result.output_bytes.as_slice());
    js_sys::Reflect::set(&obj, &JsValue::from_str("output"), &output_arr)
        .map_err(|_| JsError::new("failed to set output field"))?;
    js_sys::Reflect::set(&obj, &JsValue::from_str("summary"), &JsValue::from_str(&summary_json))
        .map_err(|_| JsError::new("failed to set summary field"))?;
    Ok(obj.into())
}

#[wasm_bindgen]
pub fn check_apply_file_size(byte_len: usize) -> String {
    const HARD: usize = 1_073_741_824;   // 1 GB
    const SEVERE: usize = 524_288_000;   // 500 MB
    const MILD: usize = 209_715_200;     // 200 MB

    if byte_len > HARD {
        return r#"{"ok":false,"warning":null,"severity":"blocked","error":"File exceeds 1 GB. Use the CLI snippet below."}"#.to_string();
    }
    if byte_len > SEVERE {
        let mb = byte_len / (1024 * 1024);
        return format!(
            r#"{{"ok":true,"warning":"Large file ({} MB). Rewriting this size may fail on memory-constrained browsers; if it fails, use the CLI snippet instead.","severity":"severe","error":null}}"#,
            mb
        );
    }
    if byte_len > MILD {
        let mb = byte_len / (1024 * 1024);
        return format!(
            r#"{{"ok":true,"warning":"Large file ({} MB). Rewrite may take 30+ seconds and requires up to 2 GB of browser memory.","severity":"mild","error":null}}"#,
            mb
        );
    }
    r#"{"ok":true,"warning":null,"severity":null,"error":null}"#.to_string()
}

#[wasm_bindgen]
pub fn bench_column_bytes(data: &[u8], column_name: &str) -> Result<String, JsError> {
    let result = crate::bench::benchmark_column_from_bytes(data, column_name)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn check_file_size(byte_len: usize) -> String {
    const HARD_LIMIT: usize = 1_073_741_824; // 1 GB
    const SOFT_LIMIT: usize = 209_715_200; // 200 MB
    if byte_len > HARD_LIMIT {
        return r#"{"ok":false,"warning":null,"error":"File exceeds 1 GB. Use the autoparq CLI for large files."}"#.to_string();
    }
    if byte_len > SOFT_LIMIT {
        let mb = byte_len / (1024 * 1024);
        return format!(
            r#"{{"ok":true,"warning":"Large file ({} MB). Analysis may take 10\u201330 seconds.","error":null}}"#,
            mb
        );
    }
    r#"{"ok":true,"warning":null,"error":null}"#.to_string()
}
