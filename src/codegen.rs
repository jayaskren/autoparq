use crate::tuner::TuneReport;

pub fn generate_snippet(report: &TuneReport, engine_str: &str) -> String {
    match engine_str.to_lowercase().as_str() {
        "pyarrow" | "pandas" | "unknown" | "" => generate_pyarrow(report),
        "pyspark" | "spark" => generate_pyspark(report),
        "polars" => generate_polars(report),
        _ => generate_pyarrow(report),
    }
}

fn most_common_codec(report: &TuneReport) -> (String, Option<i32>) {
    use std::collections::HashMap;

    let mut codec_counts: HashMap<(&str, Option<i32>), usize> = HashMap::new();
    for col in &report.columns {
        *codec_counts
            .entry((col.recommended_codec.as_str(), col.recommended_codec_level))
            .or_insert(0) += 1;
    }

    codec_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|((codec, level), _)| (codec.to_string(), level))
        .unwrap_or_else(|| ("ZSTD".to_string(), Some(3)))
}

fn generate_pyarrow(report: &TuneReport) -> String {
    let (codec, codec_level) = most_common_codec(report);
    let codec_lower = codec.to_lowercase();

    let non_plain: Vec<(&str, &str)> = report
        .columns
        .iter()
        .filter(|c| c.recommended_encoding != "PLAIN")
        .map(|c| (c.column_name.as_str(), c.recommended_encoding.as_str()))
        .collect();

    let mut lines = vec![
        "import pyarrow.parquet as pq".to_string(),
        String::new(),
        "PARQUET_WRITE_OPTIONS = {".to_string(),
        format!("    \"compression\": \"{}\",", codec_lower),
    ];

    if let Some(lvl) = codec_level {
        lines.push(format!("    \"compression_level\": {},", lvl));
    }

    if !non_plain.is_empty() {
        lines.push("    \"column_encoding\": {".to_string());
        for (col_name, encoding) in &non_plain {
            // Find the triggering stat for a comment
            let comment = report
                .columns
                .iter()
                .find(|c| c.column_name == *col_name)
                .map(|c| format!("  # {}", c.reason_brief))
                .unwrap_or_default();
            lines.push(format!("        \"{}\": \"{}\",{}", col_name, encoding, comment));
        }
        lines.push("    },".to_string());
    }

    lines.push("    \"write_statistics\": True,".to_string());
    lines.push("}".to_string());
    lines.push(String::new());
    lines.push("pq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)".to_string());
    lines.push("# NOTE: predictions are [estimated] — use autoparq bench to validate".to_string());

    lines.join("\n")
}

fn generate_pyspark(report: &TuneReport) -> String {
    let (codec, _) = most_common_codec(report);
    let codec_lower = codec.to_lowercase();

    let mut lines = vec![
        "from pyspark.sql import SparkSession".to_string(),
    ];

    if codec_lower == "zstd" {
        lines.push("# Note: ZSTD requires Spark 3.2+; per-column encoding hints require Spark 3.4+".to_string());
    } else {
        lines.push("# Note: Per-column encoding hints require Spark 3.4+".to_string());
    }

    lines.push(String::new());
    lines.push(format!(
        "spark.conf.set(\"spark.sql.parquet.compression.codec\", \"{}\")",
        codec_lower
    ));
    lines.push("# Per-column encoding (Spark 3.4+ only):".to_string());
    lines.push("# spark.conf.set(\"parquet.writer.version\", \"v2\")".to_string());
    lines.push("# For column-level hints, use PyArrow to write the file instead.".to_string());
    lines.push("df.write.mode(\"overwrite\").parquet(\"output.parquet\")".to_string());
    lines.push("# NOTE: predictions are [estimated] — use autoparq bench to validate".to_string());

    lines.join("\n")
}

fn generate_polars(report: &TuneReport) -> String {
    let (codec, codec_level) = most_common_codec(report);
    let codec_lower = codec.to_lowercase();

    let mut lines = vec![
        "import polars as pl".to_string(),
        "# Note: Polars does not support per-column encoding via Python API".to_string(),
        "# File-level codec only. Use PyArrow for per-column control.".to_string(),
        String::new(),
        "df.write_parquet(".to_string(),
        "    \"output.parquet\",".to_string(),
        format!("    compression=\"{}\",", codec_lower),
    ];

    if let Some(lvl) = codec_level {
        lines.push(format!("    compression_level={},", lvl));
    }

    lines.push(")".to_string());
    lines.push("# NOTE: predictions are [estimated] — use autoparq bench to validate".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuner::{TuneReport, ColumnRecommendation, OptionBundles, Bundle};
    use crate::advisor::{RowGroupAdvisory, SortOrderAdvisory};
    use crate::profiler::metadata::FileProfile;
    use crate::profiler::stats::ColumnProfile;

    fn make_report(codec: &str, level: Option<i32>, encoding: &str) -> TuneReport {
        let col = ColumnRecommendation {
            column_name: "id".to_string(),
            physical_type: "INT64".to_string(),
            logical_type: None,
            cardinality_estimate: 100,
            cardinality_ratio: 0.01,
            null_fraction: 0.0,
            recommended_encoding: encoding.to_string(),
            recommended_codec: codec.to_string(),
            recommended_codec_level: level,
            encoding_rule_fired: "RleDictionary".to_string(),
            reason_brief: "low cardinality".to_string(),
            confidence: "High".to_string(),
            confidence_reason: "large sample".to_string(),
            impact_stars: 4,
            engine_compatibility: None,
            caveats: vec![],
            full_explain: None,
        };
        let bundle = Bundle {
            label: "Balanced".to_string(),
            codec_description: "ZSTD:3".to_string(),
            tradeoff: "balanced".to_string(),
            python_snippet: String::new(),
            caveats: vec![],
        };
        let fp = FileProfile {
            path: "test.parquet".to_string(),
            file_size_bytes: 1024,
            parquet_version: 2,
            num_rows: 10000,
            num_row_groups: 1,
            row_group_row_counts: vec![10000],
            row_group_compressed_bytes: vec![1024],
            created_by: None,
            columns: vec![],
        };
        let cp = ColumnProfile {
            column_name: "id".to_string(),
            physical_type: "INT64".to_string(),
            logical_type: None,
            sample_rows: 10000,
            total_file_rows: 10000,
            sample_fraction: 1.0,
            cardinality_estimate: 100,
            cardinality_ratio: 0.01,
            cardinality_method: "exact".to_string(),
            monotonicity_score: None,
            run_length_score: 0.0,
            string_length_stats: None,
            uuid_pattern_detected: false,
            json_pattern_detected: false,
            byte_entropy: None,
            null_count_in_sample: 0,
            null_fraction: 0.0,
        };
        TuneReport {
            file_path: "test.parquet".to_string(),
            engine: "unknown".to_string(),
            priority: "balanced".to_string(),
            file_size_bytes: 1024,
            num_rows: 10000,
            num_columns: 1,
            current_codec: "SNAPPY".to_string(),
            scan_time_ms: 100,
            sample_fraction: 1.0,
            predicted_size_reduction_pct: 20.0,
            predicted_read_speedup: 1.14,
            overall_confidence: "High".to_string(),
            columns: vec![col],
            file_caveats: vec![],
            python_snippet: String::new(),
            spark_snippet: String::new(),
            options: OptionBundles {
                a: bundle.clone(),
                b: bundle.clone(),
                c: bundle.clone(),
            },
            row_group_advisory: RowGroupAdvisory {
                current_avg_mb: 1.0,
                current_min_mb: 1.0,
                current_max_mb: 1.0,
                recommended_range_mb: (128.0, 512.0),
                workload_label: "batch".to_string(),
                is_within_recommendation: false,
                advice: String::new(),
            },
            sort_advisory: SortOrderAdvisory {
                declared_sort_columns: vec![],
                inferred_sort_candidates: vec![],
                advice: String::new(),
            },
            file_profile: fp,
            column_profiles: vec![cp],
            diagnostics: vec![],
        }
    }

    #[test]
    fn test_pyarrow_snippet_contains_compression() {
        let report = make_report("ZSTD", Some(3), "RLE_DICTIONARY");
        let snippet = generate_snippet(&report, "pyarrow");
        assert!(snippet.contains("compression\": \"zstd\""));
        assert!(snippet.contains("compression_level\": 3"));
        assert!(snippet.contains("\"id\": \"RLE_DICTIONARY\""));
    }

    #[test]
    fn test_pyspark_snippet_contains_codec() {
        let report = make_report("ZSTD", Some(3), "PLAIN");
        let snippet = generate_snippet(&report, "spark");
        assert!(snippet.contains("zstd"));
        assert!(snippet.contains("Spark 3.2+"));
    }

    #[test]
    fn test_polars_snippet_contains_compression() {
        let report = make_report("ZSTD", Some(3), "PLAIN");
        let snippet = generate_snippet(&report, "polars");
        assert!(snippet.contains("compression=\"zstd\""));
        assert!(snippet.contains("compression_level=3"));
    }
}
