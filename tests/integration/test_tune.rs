use autoparq::tuner::build_tune_report;
use autoparq::recommender::codec::{Engine, Priority};
use insta::assert_json_snapshot;

fn run_tune(fixture: &str) -> autoparq::tuner::TuneReport {
    build_tune_report(
        std::path::Path::new(fixture),
        &Engine::Unknown,
        &Priority::Balanced,
        100_000,
        "brief",
    ).expect(&format!("tune should succeed on {}", fixture))
}

#[test]
#[ignore]
fn test_tune_monotonic_ints() {
    let report = run_tune("tests/fixtures/monotonic_ints.parquet");
    let id_col = report.columns.iter().find(|c| c.column_name == "id").unwrap();
    assert_eq!(id_col.recommended_encoding, "DELTA_BINARY_PACKED", "id column should use DELTA");
    assert!(id_col.impact_stars >= 3);

    // Snapshot the full report JSON so heuristic changes require explicit review.
    // cardinality_estimate and cardinality_ratio are redacted when HyperLogLog is used
    // because the ±0.81% error makes them non-deterministic across runs.
    let report_json = serde_json::to_value(&report).expect("TuneReport must serialize to JSON");
    assert_json_snapshot!("monotonic_ints_tune", &report_json, {
        ".scan_time_ms" => "[scan_time_ms]",
        ".file_path" => "[file_path]",
        ".file_size_bytes" => "[file_size_bytes]",
        ".columns[].cardinality_estimate" => "[cardinality_estimate]",
        ".columns[].cardinality_ratio" => "[cardinality_ratio]",
        ".column_profiles[].cardinality_estimate" => "[cardinality_estimate]",
        ".column_profiles[].cardinality_ratio" => "[cardinality_ratio]",
    });
}

#[test]
#[ignore]
fn test_tune_low_cardinality_strings() {
    let report = run_tune("tests/fixtures/low_cardinality_strings.parquet");
    let col = report.columns.iter().find(|c| c.column_name == "status").unwrap();
    assert_eq!(col.recommended_encoding, "RLE_DICTIONARY");
    assert!(col.impact_stars >= 1);
    assert!(report.predicted_size_reduction_pct > 0.0);

    // Snapshot the full report JSON so heuristic changes require explicit review.
    // cardinality_estimate and cardinality_ratio are redacted when HyperLogLog is used
    // because the ±0.81% error makes them non-deterministic across runs.
    let report_json = serde_json::to_value(&report).expect("TuneReport must serialize to JSON");
    assert_json_snapshot!("low_cardinality_strings_tune", &report_json, {
        ".scan_time_ms" => "[scan_time_ms]",
        ".file_path" => "[file_path]",
        ".file_size_bytes" => "[file_size_bytes]",
        ".columns[].cardinality_estimate" => "[cardinality_estimate]",
        ".columns[].cardinality_ratio" => "[cardinality_ratio]",
        ".column_profiles[].cardinality_estimate" => "[cardinality_estimate]",
        ".column_profiles[].cardinality_ratio" => "[cardinality_ratio]",
    });
}

#[test]
#[ignore]
fn test_tune_uuids() {
    let report = run_tune("tests/fixtures/uuids.parquet");
    let col = report.columns.iter().find(|c| c.column_name == "id").unwrap();
    assert_eq!(col.recommended_encoding, "PLAIN");
    assert_eq!(col.encoding_rule_fired, "PlainUuid");
}

#[test]
#[ignore]
fn test_tune_multi_column() {
    let report = run_tune("tests/fixtures/multi_column.parquet");
    assert_eq!(report.columns.len(), 6);
    // id should be DELTA (monotonic INT64)
    let id_col = report.columns.iter().find(|c| c.column_name == "id").unwrap();
    assert_eq!(id_col.recommended_encoding, "DELTA_BINARY_PACKED");
    // status should be RLE_DICTIONARY (5 distinct values)
    let status_col = report.columns.iter().find(|c| c.column_name == "status").unwrap();
    assert_eq!(status_col.recommended_encoding, "RLE_DICTIONARY");
    // flag should be BOOLEAN → RLE rule
    let flag_col = report.columns.iter().find(|c| c.column_name == "flag").unwrap();
    assert_eq!(flag_col.recommended_encoding, "RLE");
}

#[test]
#[ignore]
fn test_tune_spark_engine() {
    let report = build_tune_report(
        std::path::Path::new("tests/fixtures/multi_column.parquet"),
        &Engine::Spark,
        &Priority::Balanced,
        100_000,
        "brief",
    ).expect("tune should succeed on multi_column.parquet");

    let report_json = serde_json::to_value(&report).expect("TuneReport must serialize to JSON");
    let columns = report_json["columns"].as_array().unwrap();

    // With Spark + Balanced priority, all codecs should be SNAPPY (safe for all Spark versions)
    // or the column should carry a caveat explaining the engine constraint.
    for col in columns {
        let codec = col["recommended_codec"].as_str().unwrap_or("");
        let caveats = col["caveats"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(
            codec == "SNAPPY" || caveats > 0,
            "Expected SNAPPY or caveat for Spark engine, got codec={} caveats={}",
            codec,
            caveats,
        );
    }

    // Report must echo back the engine and priority fields
    assert_eq!(report_json["engine"].as_str().unwrap(), "spark");
    assert_eq!(report_json["priority"].as_str().unwrap(), "balanced");
    // options stub must be present
    assert!(report_json.get("options").is_some());
}

#[test]
#[ignore]
fn test_tune_full_explain_monotonic() {
    // Fixtures must be generated first: cargo run --example gen_fixtures
    let result = build_tune_report(
        std::path::Path::new("tests/fixtures/monotonic_ints.parquet"),
        &Engine::Unknown,
        &Priority::Balanced,
        100_000,
        "full",
    ).expect("tune should succeed on monotonic_ints.parquet");
    let report = serde_json::to_value(&result).expect("TuneReport must serialize to JSON");
    let col = &report["columns"][0];
    let full_explain = &col["full_explain"];
    assert!(!full_explain.is_null(), "full_explain should be populated for explain=full");
    let chain = full_explain["reasoning_chain"].as_array().unwrap();
    let delta_rule = chain.iter().find(|r| r["rule_name"] == "DeltaMonotonic").unwrap();
    assert_eq!(delta_rule["fired"], true, "DeltaMonotonic should have fired");
}
