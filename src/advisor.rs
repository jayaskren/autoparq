use serde::{Serialize, Deserialize};
use crate::profiler::metadata::FileProfile;
use crate::profiler::stats::ColumnProfile;
use crate::recommender::codec::Engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowGroupAdvisory {
    pub current_avg_mb: f64,
    pub current_min_mb: f64,
    pub current_max_mb: f64,
    pub recommended_range_mb: (f64, f64),
    pub workload_label: String,
    pub is_within_recommendation: bool,
    pub advice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortOrderAdvisory {
    pub declared_sort_columns: Vec<String>,
    pub inferred_sort_candidates: Vec<String>,
    pub advice: String,
}

pub fn analyze_row_groups(profile: &FileProfile, engine: &Engine) -> RowGroupAdvisory {
    let bytes = &profile.row_group_compressed_bytes;

    if bytes.is_empty() {
        let (recommended_range_mb, workload_label) = recommended_range_for_engine(engine);
        return RowGroupAdvisory {
            current_avg_mb: 0.0,
            current_min_mb: 0.0,
            current_max_mb: 0.0,
            recommended_range_mb,
            workload_label,
            is_within_recommendation: true,
            advice: "No row group data available.".to_string(),
        };
    }

    const MB: f64 = 1024.0 * 1024.0;
    let avg_mb = bytes.iter().map(|&b| b as f64).sum::<f64>() / bytes.len() as f64 / MB;
    let min_mb = bytes.iter().map(|&b| b as f64).fold(f64::INFINITY, f64::min) / MB;
    let max_mb = bytes.iter().map(|&b| b as f64).fold(f64::NEG_INFINITY, f64::max) / MB;

    let (recommended_range_mb, workload_label) = recommended_range_for_engine(engine);
    let (rec_min, rec_max) = recommended_range_mb;
    let is_within_recommendation = avg_mb >= rec_min && avg_mb <= rec_max;

    let advice = if is_within_recommendation {
        format!(
            "Row group size {:.0} MB is within the {}–{} MB recommendation for {}.",
            avg_mb, rec_min as u64, rec_max as u64, workload_label
        )
    } else if avg_mb < rec_min {
        format!(
            "Row group size {:.0} MB is below the {}–{} MB recommendation for {}. \
             Small row groups reduce compression effectiveness and increase predicate pushdown overhead.",
            avg_mb, rec_min as u64, rec_max as u64, workload_label
        )
    } else {
        format!(
            "Row group size {:.0} MB exceeds the {}–{} MB recommendation for {}. \
             Large row groups may cause excessive memory pressure during scans.",
            avg_mb, rec_min as u64, rec_max as u64, workload_label
        )
    };

    RowGroupAdvisory {
        current_avg_mb: avg_mb,
        current_min_mb: min_mb,
        current_max_mb: max_mb,
        recommended_range_mb,
        workload_label,
        is_within_recommendation,
        advice,
    }
}

fn recommended_range_for_engine(engine: &Engine) -> ((f64, f64), String) {
    match engine {
        Engine::DuckDB => ((64.0, 128.0), "DuckDB".to_string()),
        Engine::Spark => ((128.0, 512.0), "Spark".to_string()),
        Engine::ClickHouse => ((64.0, 256.0), "ClickHouse".to_string()),
        Engine::Polars => ((64.0, 128.0), "Polars".to_string()),
        Engine::Pandas => ((64.0, 128.0), "Pandas".to_string()),
        Engine::Unknown => ((64.0, 256.0), "general workloads".to_string()),
    }
}

pub fn detect_sort_order(
    _profile: &FileProfile,
    column_profiles: &[ColumnProfile],
) -> SortOrderAdvisory {
    // FileProfile has no declared sort column metadata — reserved for future enhancement.
    let declared_sort_columns: Vec<String> = Vec::new();

    let inferred_sort_candidates: Vec<String> = column_profiles
        .iter()
        .filter(|cp| {
            if let Some(score) = cp.monotonicity_score {
                if score > 0.95 {
                    // Only INT64 or TIMESTAMP logical type columns qualify
                    if cp.physical_type == "INT64" {
                        return true;
                    }
                    if let Some(ref lt) = cp.logical_type {
                        if lt.contains("TIMESTAMP") {
                            return true;
                        }
                    }
                }
            }
            false
        })
        .map(|cp| cp.column_name.clone())
        .collect();

    let advice = if declared_sort_columns.is_empty() && !inferred_sort_candidates.is_empty() {
        format!(
            "Column(s) {:?} appear sorted (monotonicity_score > 0.95) but no sort order is \
             declared in the file metadata. Declaring sort order enables better predicate pushdown.",
            inferred_sort_candidates
        )
    } else if inferred_sort_candidates.is_empty() {
        "No strongly monotonic columns detected. Consider sorting by your most common filter \
         predicate column before writing."
            .to_string()
    } else {
        format!(
            "Sort order declared for column(s) {:?}.",
            declared_sort_columns
        )
    };

    SortOrderAdvisory {
        declared_sort_columns,
        inferred_sort_candidates,
        advice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::stats::ColumnProfile;

    fn make_file_profile(rg_bytes: Vec<i64>) -> FileProfile {
        FileProfile {
            path: "test.parquet".to_string(),
            file_size_bytes: rg_bytes.iter().map(|&b| b as u64).sum(),
            parquet_version: 2,
            num_rows: 1000,
            num_row_groups: rg_bytes.len(),
            row_group_row_counts: rg_bytes.iter().map(|_| 1000_i64).collect(),
            row_group_compressed_bytes: rg_bytes,
            created_by: None,
            columns: Vec::new(),
        }
    }

    fn make_column_profile(column_name: &str, physical_type: &str, logical_type: Option<String>, monotonicity_score: Option<f64>) -> ColumnProfile {
        ColumnProfile {
            column_name: column_name.to_string(),
            physical_type: physical_type.to_string(),
            logical_type,
            sample_rows: 1000,
            total_file_rows: 1000,
            sample_fraction: 1.0,
            cardinality_estimate: 1000,
            cardinality_ratio: 1.0,
            cardinality_method: "exact".to_string(),
            monotonicity_score,
            run_length_score: 0.0,
            string_length_stats: None,
            uuid_pattern_detected: false,
            json_pattern_detected: false,
            byte_entropy: None,
            null_count_in_sample: 0,
            null_fraction: 0.0,
        }
    }

    #[test]
    fn test_rg_advisory_duckdb_too_small() {
        // 12 MB avg, engine=DuckDB → is_within_recommendation=false (range 64–128 MB)
        let bytes_12mb: i64 = 12 * 1024 * 1024;
        let profile = make_file_profile(vec![bytes_12mb]);
        let advisory = analyze_row_groups(&profile, &Engine::DuckDB);
        assert!(!advisory.is_within_recommendation);
        assert!(advisory.advice.contains("below"), "advice was: {}", advisory.advice);
    }

    #[test]
    fn test_rg_advisory_spark_within_range() {
        // 200 MB avg, engine=Spark → is_within_recommendation=true (range 128–512 MB)
        let bytes_200mb: i64 = 200 * 1024 * 1024;
        let profile = make_file_profile(vec![bytes_200mb]);
        let advisory = analyze_row_groups(&profile, &Engine::Spark);
        assert!(advisory.is_within_recommendation);
    }

    #[test]
    fn test_rg_advisory_too_large() {
        // 600 MB avg, engine=Spark → is_within_recommendation=false (range 128–512 MB)
        let bytes_600mb: i64 = 600 * 1024 * 1024;
        let profile = make_file_profile(vec![bytes_600mb]);
        let advisory = analyze_row_groups(&profile, &Engine::Spark);
        assert!(!advisory.is_within_recommendation);
        assert!(advisory.advice.contains("exceeds"), "advice was: {}", advisory.advice);
    }

    #[test]
    fn test_rg_advisory_empty() {
        let profile = make_file_profile(vec![]);
        let advisory = analyze_row_groups(&profile, &Engine::DuckDB);
        assert!(advisory.is_within_recommendation);
        assert_eq!(advisory.advice, "No row group data available.");
    }

    #[test]
    fn test_sort_inferred_int64() {
        let col = make_column_profile("id", "INT64", None, Some(0.97));
        let file_profile = make_file_profile(vec![]);
        let advisory = detect_sort_order(&file_profile, &[col]);
        assert!(
            advisory.inferred_sort_candidates.contains(&"id".to_string()),
            "candidates: {:?}",
            advisory.inferred_sort_candidates
        );
    }

    #[test]
    fn test_sort_inferred_timestamp() {
        let col = make_column_profile("ts", "INT64", Some("TIMESTAMP(MICROS, UTC)".to_string()), Some(0.97));
        let file_profile = make_file_profile(vec![]);
        let advisory = detect_sort_order(&file_profile, &[col]);
        assert!(
            advisory.inferred_sort_candidates.contains(&"ts".to_string()),
            "candidates: {:?}",
            advisory.inferred_sort_candidates
        );
    }

    #[test]
    fn test_sort_not_inferred_float() {
        // FLOAT/DOUBLE should not be added even if monotonic
        let col = make_column_profile("score", "DOUBLE", None, Some(0.97));
        let file_profile = make_file_profile(vec![]);
        let advisory = detect_sort_order(&file_profile, &[col]);
        assert!(
            !advisory.inferred_sort_candidates.contains(&"score".to_string()),
            "candidates: {:?}",
            advisory.inferred_sort_candidates
        );
    }

    #[test]
    fn test_sort_not_inferred_below_threshold() {
        // INT64 but monotonicity_score = 0.93 < 0.95 threshold
        let col = make_column_profile("id", "INT64", None, Some(0.93));
        let file_profile = make_file_profile(vec![]);
        let advisory = detect_sort_order(&file_profile, &[col]);
        assert!(
            !advisory.inferred_sort_candidates.contains(&"id".to_string()),
            "candidates: {:?}",
            advisory.inferred_sort_candidates
        );
    }

    #[test]
    fn test_sort_advice_no_candidates() {
        let col = make_column_profile("score", "DOUBLE", None, Some(0.97));
        let file_profile = make_file_profile(vec![]);
        let advisory = detect_sort_order(&file_profile, &[col]);
        assert!(advisory.advice.contains("No strongly monotonic columns detected"));
    }
}
