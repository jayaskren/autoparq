use serde::{Serialize, Deserialize};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use std::path::Path;
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use crate::advisor::{analyze_row_groups, detect_sort_order, RowGroupAdvisory, SortOrderAdvisory};
use crate::profiler::{
    metadata::{read_file_metadata, ColumnMetaSummary, FileProfile},
    sampler::{sample_column, list_column_names},
    stats::{profile_column, ColumnProfile},
};
use crate::recommender::{
    encoding::{recommend_encoding, EncodingRecommendation, RuleName},
    codec::{recommend_codec, Priority, Engine, Caveat, CodecRecommendation},
    engine::{apply_engine_overrides, check_encoding_compatibility},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub rule_name: String,
    pub evaluated: bool,
    pub fired: bool,
    pub threshold: String,
    pub actual_value: String,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeExplain {
    pub encoding: String,
    pub rejected_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullExplain {
    pub raw_stats: std::collections::BTreeMap<String, serde_json::Value>,
    pub reasoning_chain: Vec<RuleEvaluation>,
    pub alternatives_considered: Vec<AlternativeExplain>,
    pub engine_compatibility: Option<String>,
    pub teach_yourself: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnRecommendation {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub cardinality_estimate: u64,
    pub cardinality_ratio: f64,
    pub null_fraction: f64,
    pub recommended_encoding: String,
    pub recommended_codec: String,
    pub recommended_codec_level: Option<i32>,
    pub encoding_rule_fired: String,
    pub reason_brief: String,
    pub confidence: String,
    pub confidence_reason: String,
    pub impact_stars: u8,
    pub engine_compatibility: Option<String>,
    pub caveats: Vec<Caveat>,
    pub full_explain: Option<FullExplain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneReport {
    pub file_path: String,
    pub engine: String,
    pub priority: String,
    pub file_size_bytes: u64,
    pub num_rows: i64,
    pub num_columns: usize,
    pub current_codec: String,
    pub scan_time_ms: u64,
    pub sample_fraction: f64,
    pub predicted_size_reduction_pct: f64,
    pub predicted_read_speedup: f64,
    pub overall_confidence: String,
    pub columns: Vec<ColumnRecommendation>,
    pub file_caveats: Vec<Caveat>,
    pub python_snippet: String,
    pub spark_snippet: String,
    pub options: OptionBundles,
    pub row_group_advisory: RowGroupAdvisory,
    pub sort_advisory: SortOrderAdvisory,
    pub file_profile: FileProfile,
    pub column_profiles: Vec<ColumnProfile>,
    #[serde(default)]
    pub diagnostics: Vec<crate::diagnostics::ColumnDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub label: String,
    pub codec_description: String,
    pub tradeoff: String,    // qualitative description, e.g. "Best size/speed balance"
    pub python_snippet: String,
    pub caveats: Vec<Caveat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionBundles {
    pub a: Bundle,
    pub b: Bundle,
    pub c: Bundle,
}

fn teach_yourself_text(rule: &RuleName) -> &'static str {
    match rule {
        RuleName::BooleanRle => "BOOLEAN columns in Parquet use run-length encoding automatically. The only tuning lever is the codec on top.",
        RuleName::DeltaMonotonic => "DELTA_BINARY_PACKED stores differences between consecutive values instead of the values themselves. For sorted integers (timestamps, auto-increment IDs), deltas are tiny and pack into very few bits. Rule of thumb: use DELTA on any monotonically increasing integer column.",
        RuleName::RleDictionary => "Dictionary encoding stores each distinct value once and replaces data values with small integer indices. When cardinality is low (few distinct values), these indices compress extremely well with run-length encoding. Rule of thumb: if cardinality < 10% of row count and the dictionary fits in ~512KB, use dictionary encoding.",
        RuleName::ByteStreamSplit => "BYTE_STREAM_SPLIT deinterleaves the bytes of floating-point values — writing all MSBs together, then all next bytes, etc. For physically-related floats (measurements in a range), this groups similar bytes together, improving codec compression by 10–30%.",
        RuleName::PlainUuid => "UUID columns have cardinality equal to row count, so dictionary encoding would require storing every UUID in the dictionary — as large as the original column. PLAIN with ZSTD is optimal for high-cardinality string columns.",
        RuleName::DeltaByteArray => "DELTA_BYTE_ARRAY stores how much each string shares with the previous one (prefix length) and writes only the differing suffix. For sorted or nearly-sorted string columns, neighboring values share long prefixes, so only tiny byte differences are stored. A column of sorted paths, keys, or date-prefixed IDs can compress to 20–30% of its PLAIN size before any codec is applied. Requires Parquet V2 data pages; Spark 3.3+ can read it but cannot write it via the DataFrame API.",
        RuleName::DeltaLengthByteArray => "DELTA_LENGTH_BYTE_ARRAY is a two-part encoding: it delta-encodes the sequence of string lengths, then appends all raw string bytes together in one block. Codecs like ZSTD find patterns in the concatenated bytes more effectively than in interleaved length+value PLAIN encoding. The improvement is modest — roughly 2–3% additional size reduction on top of ZSTD — but consistent for high-cardinality short-string columns.",
        RuleName::PlainDefault => "No specific pattern was detected. PLAIN encoding with a byte-oriented codec (ZSTD) is the safe baseline.",
    }
}

fn build_full_explain(
    profile: &ColumnProfile,
    enc_rec: &EncodingRecommendation,
    engine_compat: &Option<String>,
) -> FullExplain {
    use serde_json::json;

    // 1. raw_stats
    let mut raw_stats = std::collections::BTreeMap::new();
    raw_stats.insert("cardinality_estimate".to_string(), json!(profile.cardinality_estimate));
    raw_stats.insert("cardinality_ratio".to_string(), json!(profile.cardinality_ratio));
    raw_stats.insert("cardinality_method".to_string(), json!(profile.cardinality_method));
    raw_stats.insert(
        "monotonicity_score".to_string(),
        profile.monotonicity_score.map_or(serde_json::Value::Null, |v| json!(v)),
    );
    raw_stats.insert(
        "string_monotonicity_score".to_string(),
        profile.string_monotonicity_score.map_or(serde_json::Value::Null, |v| json!(v)),
    );
    raw_stats.insert("run_length_score".to_string(), json!(profile.run_length_score));
    raw_stats.insert("null_fraction".to_string(), json!(profile.null_fraction));
    raw_stats.insert("uuid_pattern_detected".to_string(), json!(profile.uuid_pattern_detected));
    raw_stats.insert("json_pattern_detected".to_string(), json!(profile.json_pattern_detected));
    raw_stats.insert("sample_rows".to_string(), json!(profile.sample_rows));
    raw_stats.insert("sample_fraction".to_string(), json!(profile.sample_fraction));
    raw_stats.insert(
        "byte_entropy".to_string(),
        profile.byte_entropy.map_or(serde_json::Value::Null, |v| json!(v)),
    );

    // 2. reasoning_chain — one entry per rule in priority order
    let is_int_or_temporal = profile.physical_type == "INT32"
        || profile.physical_type == "INT64"
        || profile
            .logical_type
            .as_deref()
            .map_or(false, |lt| lt.starts_with("TIMESTAMP") || lt.starts_with("DATE"));
    let is_float = profile.physical_type == "FLOAT" || profile.physical_type == "DOUBLE";
    let is_byte_array = profile.physical_type == "BYTE_ARRAY";

    let mut reasoning_chain: Vec<RuleEvaluation> = Vec::new();

    // Rule 1: BooleanRle
    {
        let evaluated = profile.physical_type == "BOOLEAN";
        let fired = enc_rec.rule_fired == RuleName::BooleanRle;
        reasoning_chain.push(RuleEvaluation {
            rule_name: "BooleanRle".to_string(),
            evaluated,
            fired,
            threshold: "physical_type == BOOLEAN".to_string(),
            actual_value: profile.physical_type.clone(),
            outcome: if fired {
                "Fired: BOOLEAN column → RLE".to_string()
            } else if evaluated {
                "Evaluated but superseded".to_string()
            } else {
                format!("Skipped: physical_type={} is not BOOLEAN", profile.physical_type)
            },
        });
    }

    // Rule 2: DeltaMonotonic
    {
        let evaluated = is_int_or_temporal;
        let fired = enc_rec.rule_fired == RuleName::DeltaMonotonic;
        let mono_str = profile
            .monotonicity_score
            .map_or("null".to_string(), |s| format!("{:.4}", s));
        let outcome = if fired {
            format!("Fired: monotonicity_score={} >= threshold 0.90 → DELTA_BINARY_PACKED", mono_str)
        } else if evaluated {
            match profile.monotonicity_score {
                Some(s) => format!("Rejected: monotonicity_score={:.4} < threshold 0.90", s),
                None => "Rejected: monotonicity_score not available".to_string(),
            }
        } else {
            format!("Skipped: physical_type={} is not int/temporal", profile.physical_type)
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "DeltaMonotonic".to_string(),
            evaluated,
            fired,
            threshold: "0.90".to_string(),
            actual_value: mono_str,
            outcome,
        });
    }

    // Rule 3: RleDictionary
    {
        let fired = enc_rec.rule_fired == RuleName::RleDictionary;
        let card_str = format!("{:.4}", profile.cardinality_ratio);
        let outcome = if fired {
            format!(
                "Fired: cardinality_ratio={:.4} < 0.10 → RLE_DICTIONARY",
                profile.cardinality_ratio
            )
        } else {
            format!(
                "Rejected: cardinality_ratio={:.4} >= threshold 0.10",
                profile.cardinality_ratio
            )
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "RleDictionary".to_string(),
            evaluated: true,
            fired,
            threshold: "0.10".to_string(),
            actual_value: card_str,
            outcome,
        });
    }

    // Rule 4: ByteStreamSplit
    {
        let evaluated = is_float;
        let fired = enc_rec.rule_fired == RuleName::ByteStreamSplit;
        let card_str = format!("{:.4}", profile.cardinality_ratio);
        let outcome = if fired {
            format!(
                "Fired: cardinality_ratio={:.4} > 0.50 → BYTE_STREAM_SPLIT",
                profile.cardinality_ratio
            )
        } else if evaluated {
            format!(
                "Rejected: cardinality_ratio={:.4} <= threshold 0.50",
                profile.cardinality_ratio
            )
        } else {
            format!("Skipped: physical_type={} is not float", profile.physical_type)
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "ByteStreamSplit".to_string(),
            evaluated,
            fired,
            threshold: "0.50".to_string(),
            actual_value: card_str,
            outcome,
        });
    }

    // Rule 5: PlainUuid
    {
        let evaluated = is_byte_array;
        let fired = enc_rec.rule_fired == RuleName::PlainUuid;
        let actual = profile.uuid_pattern_detected.to_string();
        let outcome = if fired {
            "Fired: UUID pattern detected → PLAIN".to_string()
        } else if evaluated {
            "Rejected: UUID pattern not detected".to_string()
        } else {
            format!("Skipped: physical_type={} is not BYTE_ARRAY", profile.physical_type)
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "PlainUuid".to_string(),
            evaluated,
            fired,
            threshold: "90% UUID match".to_string(),
            actual_value: actual,
            outcome,
        });
    }

    // Rule 6: DeltaByteArray
    {
        let evaluated = is_byte_array;
        let fired = enc_rec.rule_fired == RuleName::DeltaByteArray;
        let sms_str = profile.string_monotonicity_score
            .map_or("null".to_string(), |s| format!("{:.4}", s));
        let mean_len_str = profile.string_length_stats.as_ref()
            .map_or("null".to_string(), |s| format!("{:.1}", s.mean_len));
        let outcome = if fired {
            format!("Fired: string_monotonicity_score={} >= 0.80, mean_len={} <= 50 → DELTA_BYTE_ARRAY", sms_str, mean_len_str)
        } else if evaluated {
            match profile.string_monotonicity_score {
                Some(s) if s >= 0.80 => format!("Rejected: mean_len={} > 50", mean_len_str),
                Some(s) => format!("Rejected: string_monotonicity_score={:.4} < threshold 0.80", s),
                None => "Rejected: string_monotonicity_score not available".to_string(),
            }
        } else {
            format!("Skipped: physical_type={} is not BYTE_ARRAY", profile.physical_type)
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "DeltaByteArray".to_string(),
            evaluated,
            fired,
            threshold: "string_monotonicity_score >= 0.80 and mean_len <= 50".to_string(),
            actual_value: format!("sms={}, mean_len={}", sms_str, mean_len_str),
            outcome,
        });
    }

    // Rule 7: DeltaLengthByteArray
    {
        let evaluated = is_byte_array;
        let fired = enc_rec.rule_fired == RuleName::DeltaLengthByteArray;
        let mean_len_str = profile.string_length_stats.as_ref()
            .map_or("null".to_string(), |s| format!("{:.1}", s.mean_len));
        let outcome = if fired {
            format!("Fired: cardinality_ratio={:.4} >= 0.10, mean_len={} <= 50 → DELTA_LENGTH_BYTE_ARRAY", profile.cardinality_ratio, mean_len_str)
        } else if evaluated {
            format!("Rejected: mean_len={} > 50 or cardinality_ratio={:.4} < 0.10", mean_len_str, profile.cardinality_ratio)
        } else {
            format!("Skipped: physical_type={} is not BYTE_ARRAY", profile.physical_type)
        };
        reasoning_chain.push(RuleEvaluation {
            rule_name: "DeltaLengthByteArray".to_string(),
            evaluated,
            fired,
            threshold: "cardinality_ratio >= 0.10 and mean_len <= 50".to_string(),
            actual_value: format!("cardinality_ratio={:.4}, mean_len={}", profile.cardinality_ratio, mean_len_str),
            outcome,
        });
    }

    // Rule 8: PlainDefault
    {
        let fired = enc_rec.rule_fired == RuleName::PlainDefault;
        reasoning_chain.push(RuleEvaluation {
            rule_name: "PlainDefault".to_string(),
            evaluated: true,
            fired,
            threshold: "catch-all".to_string(),
            actual_value: "n/a".to_string(),
            outcome: if fired {
                "Fired: no other rule matched → PLAIN".to_string()
            } else {
                "Not reached: an earlier rule fired".to_string()
            },
        });
    }

    // 3. alternatives_considered — rules that were evaluated but not fired
    let alternatives_considered: Vec<AlternativeExplain> = reasoning_chain
        .iter()
        .filter(|r| r.evaluated && !r.fired)
        .map(|r| AlternativeExplain {
            encoding: r.rule_name.clone(),
            rejected_reason: r.outcome.clone(),
        })
        .collect();

    FullExplain {
        raw_stats,
        reasoning_chain,
        alternatives_considered,
        engine_compatibility: engine_compat.clone(),
        teach_yourself: teach_yourself_text(&enc_rec.rule_fired).to_string(),
    }
}

fn compute_impact_stars(
    enc: &EncodingRecommendation,
    codec: &CodecRecommendation,
    meta: &ColumnMetaSummary,
    file_total_uncompressed: i64,
) -> u8 {
    let enc_already_set = meta.encodings.iter().any(|e| e == &enc.encoding);
    let rec_codec_full = match codec.codec_level {
        Some(lvl) => format!("{}:{}", codec.codec, lvl),
        None => codec.codec.clone(),
    };
    let codec_already_set = meta.codec == rec_codec_full;

    if enc_already_set && codec_already_set {
        return 1;
    }

    let share = if file_total_uncompressed > 0 {
        meta.uncompressed_bytes as f64 / file_total_uncompressed as f64
    } else {
        0.0
    };

    if enc_already_set {
        // Codec-only change: scale by size, cap at 3. A level bump is
        // ~5–15% gain — meaningful only on large columns.
        return match share {
            s if s >= 0.10 => 3,
            s if s >= 0.04 => 2,
            _ => 1,
        };
    }

    // Encoding change — existing scale, caps at 5.
    match share {
        s if s >= 0.10 => 5,
        s if s >= 0.04 => 4,
        s if s >= 0.01 => 3,
        _ => 2,
    }
}

fn compute_engine_compat(enc: &EncodingRecommendation, engine: &Engine, engine_name: &str) -> Option<String> {
    let es = check_encoding_compatibility(engine, &enc.encoding);
    es.min_version.map(|v| format!("{}>={}", engine_name, v))
}

fn predicted_size_reduction_pct(columns: &[ColumnRecommendation], file_profile: &FileProfile) -> f64 {
    let total_uncompressed: i64 = file_profile.columns.iter().map(|c| c.uncompressed_bytes).sum();
    if total_uncompressed == 0 {
        return 0.0;
    }

    let weighted_sum: f64 = columns.iter().map(|col_rec| {
        let meta = file_profile.columns.iter().find(|c| c.name == col_rec.column_name);
        let col_weight = meta.map_or(0.0, |m| m.uncompressed_bytes as f64 / total_uncompressed as f64);

        let currently_plain = meta.map_or(false, |m| {
            m.encodings.iter().any(|e| e == "PLAIN")
                && !m.encodings.iter().any(|e| e == "RLE_DICTIONARY" || e == "DELTA_BINARY_PACKED")
        });

        let factor = if currently_plain {
            match col_rec.encoding_rule_fired.as_str() {
                "DeltaMonotonic" => 3.0,
                "RleDictionary" => {
                    if col_rec.cardinality_ratio < 0.001 { 10.0 }
                    else if col_rec.cardinality_ratio < 0.01 { 5.0 }
                    else { 2.0 }
                }
                "ByteStreamSplit" => 1.15,
                "DeltaByteArray" => 2.5,
                "DeltaLengthByteArray" => 1.05,
                _ => if meta.map_or(false, |m| m.codec != col_rec.recommended_codec) { 1.25 } else { 1.0 }
            }
        } else {
            // Even if encoding is already good, codec change can help
            if meta.map_or(false, |m| m.codec != col_rec.recommended_codec) { 1.15 } else { 1.0 }
        };
        col_weight * (1.0 - 1.0 / factor)
    }).sum();

    (weighted_sum * 100.0).min(90.0)
}

/// Creates a minimal ColumnProfile from metadata when sampling/profiling fails.
fn fallback_profile(col_name: &str, meta: &ColumnMetaSummary, total_rows: i64) -> ColumnProfile {
    ColumnProfile {
        column_name: col_name.to_string(),
        physical_type: meta.physical_type.clone(),
        logical_type: meta.logical_type.clone(),
        sample_rows: 0,
        total_file_rows: total_rows,
        sample_fraction: 0.0,
        cardinality_estimate: 0,
        cardinality_ratio: 0.0,
        cardinality_method: "unavailable".to_string(),
        monotonicity_score: None,
        string_monotonicity_score: None,
        run_length_score: 0.0,
        string_length_stats: None,
        uuid_pattern_detected: false,
        json_pattern_detected: false,
        byte_entropy: None,
        null_count_in_sample: 0,
        null_fraction: meta.total_null_count.map(|nc| {
            if total_rows > 0 { nc as f64 / total_rows as f64 } else { 0.0 }
        }).unwrap_or(0.0),
    }
}

fn mode_codec(columns: &[ColumnMetaSummary]) -> String {
    if columns.is_empty() {
        return "UNCOMPRESSED".to_string();
    }
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for col in columns {
        *counts.entry(col.codec.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(codec, _)| codec.to_string())
        .unwrap_or_else(|| "UNCOMPRESSED".to_string())
}

fn engine_to_str(engine: &Engine) -> &'static str {
    match engine {
        Engine::Spark => "spark",
        Engine::DuckDB => "duckdb",
        Engine::Polars => "polars",
        Engine::ClickHouse => "clickhouse",
        Engine::Pandas => "pandas",
        Engine::Unknown => "unknown",
    }
}

fn priority_to_str(priority: &Priority) -> &'static str {
    match priority {
        Priority::Size => "size",
        Priority::Speed => "speed",
        Priority::Balanced => "balanced",
    }
}

fn bundle_python_snippet(
    column_recs: &[ColumnRecommendation],
    codec: &str,
    codec_level: Option<i32>,
) -> String {
    let codec_name = codec.to_lowercase();
    let level_line = match codec_level {
        Some(lvl) => format!("\n    \"compression_level\": {},", lvl),
        None => String::new(),
    };
    let enc_lines: Vec<String> = column_recs.iter()
        .filter(|col| col.recommended_encoding != "PLAIN")
        .map(|col| format!("        \"{}\": \"{}\"", col.column_name, col.recommended_encoding))
        .collect();
    let enc_block = if enc_lines.is_empty() {
        "        # no per-column encoding overrides needed".to_string()
    } else {
        enc_lines.join(",\n")
    };
    format!(
        "PARQUET_WRITE_OPTIONS = {{\n    \"compression\": \"{}\",{}\n    \"column_encoding\": {{\n{}\n    }}\n}}\npq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)",
        codec_name,
        level_line,
        enc_block,
    )
}

fn generate_option_bundles(
    _file_profile: &FileProfile,
    column_recs: &[ColumnRecommendation],
    engine: &Engine,
    current_enc_recs: &[EncodingRecommendation],
) -> OptionBundles {
    // Codec size differences are highly data-dependent and cannot be reliably
    // estimated without actually compressing. We only provide codec descriptions
    // and copy-paste snippets. Use `autoparq bench` for measured comparisons.

    let dummy_profiles: Vec<crate::profiler::stats::ColumnProfile> = column_recs.iter().map(|cr| {
        crate::profiler::stats::ColumnProfile {
            column_name: cr.column_name.clone(),
            physical_type: cr.physical_type.clone(),
            logical_type: cr.logical_type.clone(),
            sample_rows: 100_000,
            total_file_rows: 1_000_000,
            sample_fraction: 0.2,
            cardinality_estimate: cr.cardinality_estimate,
            cardinality_ratio: cr.cardinality_ratio,
            cardinality_method: "exact".to_string(),
            monotonicity_score: None,
            string_monotonicity_score: None,
            run_length_score: 0.0,
            string_length_stats: None,
            uuid_pattern_detected: false,
            json_pattern_detected: false,
            byte_entropy: None,
            null_count_in_sample: 0,
            null_fraction: cr.null_fraction,
        }
    }).collect();

    // Bundle A: Balanced — use the actual balanced recommendation
    let a_caveats: Vec<Caveat> = current_enc_recs.iter().zip(dummy_profiles.iter()).flat_map(|(enc_rec, dp)| {
        let mut codec_rec = recommend_codec(dp, enc_rec, &Priority::Balanced, engine);
        apply_engine_overrides(&mut codec_rec, engine);
        codec_rec.caveats
    }).collect();
    let a_codec_sample = {
        let dp = dummy_profiles.first();
        let enc_rec = current_enc_recs.first();
        match (dp, enc_rec) {
            (Some(dp), Some(enc_rec)) => {
                let mut cr = recommend_codec(dp, enc_rec, &Priority::Balanced, engine);
                apply_engine_overrides(&mut cr, engine);
                (cr.codec, cr.codec_level)
            }
            _ => ("ZSTD".to_string(), Some(3)),
        }
    };
    let a_codec_desc = match (&a_codec_sample.0 as &str, a_codec_sample.1) {
        ("ZSTD", Some(3)) => "ZSTD:3".to_string(),
        ("ZSTD", Some(lvl)) => format!("ZSTD:{}", lvl),
        ("SNAPPY", _) => "SNAPPY".to_string(),
        (codec, Some(lvl)) => format!("{}:{}", codec, lvl),
        (codec, None) => codec.to_string(),
    };
    let a_snippet = bundle_python_snippet(column_recs, &a_codec_sample.0, a_codec_sample.1);

    // Bundle B: Smallest File (ZSTD:6)
    let b_caveats: Vec<Caveat> = current_enc_recs.iter().zip(dummy_profiles.iter()).flat_map(|(enc_rec, dp)| {
        let mut codec_rec = recommend_codec(dp, enc_rec, &Priority::Size, engine);
        apply_engine_overrides(&mut codec_rec, engine);
        codec_rec.caveats
    }).collect();
    let b_snippet = bundle_python_snippet(column_recs, "ZSTD", Some(6));

    // Bundle C: Fastest Reads (LZ4)
    let c_caveats: Vec<Caveat> = current_enc_recs.iter().zip(dummy_profiles.iter()).flat_map(|(enc_rec, dp)| {
        let mut codec_rec = recommend_codec(dp, enc_rec, &Priority::Speed, engine);
        apply_engine_overrides(&mut codec_rec, engine);
        codec_rec.caveats
    }).collect();
    let c_snippet = bundle_python_snippet(column_recs, "LZ4", None);

    OptionBundles {
        a: Bundle {
            label: "Balanced".to_string(),
            codec_description: a_codec_desc,
            tradeoff: "Best balance of file size and read speed".to_string(),
            python_snippet: a_snippet,
            caveats: a_caveats,
        },
        b: Bundle {
            label: "Smallest File".to_string(),
            codec_description: "ZSTD:6".to_string(),
            tradeoff: "Smallest file; slower writes, slightly slower reads than ZSTD:3".to_string(),
            python_snippet: b_snippet,
            caveats: b_caveats,
        },
        c: Bundle {
            label: "Fastest Reads".to_string(),
            codec_description: "LZ4".to_string(),
            tradeoff: "Fastest decompression; files are slightly larger than ZSTD".to_string(),
            python_snippet: c_snippet,
            caveats: c_caveats,
        },
    }
}

pub fn build_tune_report(
    path: &Path,
    engine: &Engine,
    priority: &Priority,
    sample_rows: usize,
    explain: &str,
) -> Result<TuneReport, crate::AutoparqError> {
    let start = Instant::now();

    let file_profile = read_file_metadata(path)?;
    let column_names = list_column_names(path)?;
    let file_total_uncompressed: i64 = file_profile.columns.iter().map(|c| c.uncompressed_bytes).sum();

    #[cfg(not(target_arch = "wasm32"))]
    let column_results: Vec<(ColumnRecommendation, EncodingRecommendation, ColumnProfile)> = column_names.par_iter()
        .filter_map(|col_name| {
            let meta = file_profile.columns.iter().find(|c| &c.name == col_name)?;
            let profile = sample_column(path, col_name, 0, sample_rows)
                .map(|sample| profile_column(&sample))
                .unwrap_or_else(|_| fallback_profile(col_name, meta, file_profile.num_rows));
            let enc_rec = recommend_encoding(&profile, meta);
            let mut codec_rec = recommend_codec(&profile, &enc_rec, priority, engine);
            apply_engine_overrides(&mut codec_rec, engine);
            let impact_stars = compute_impact_stars(&enc_rec, &codec_rec, meta, file_total_uncompressed);
            let engine_name = engine_to_str(engine);
            let engine_compatibility = compute_engine_compat(&enc_rec, engine, engine_name);
            let confidence_str = format!("{:?}", enc_rec.confidence);
            let full_explain = if explain == "full" {
                Some(build_full_explain(&profile, &enc_rec, &engine_compatibility))
            } else {
                None
            };
            let col_rec = ColumnRecommendation {
                column_name: col_name.clone(),
                physical_type: meta.physical_type.clone(),
                logical_type: meta.logical_type.clone(),
                cardinality_estimate: profile.cardinality_estimate,
                cardinality_ratio: profile.cardinality_ratio,
                null_fraction: profile.null_fraction,
                recommended_encoding: enc_rec.encoding.clone(),
                recommended_codec: codec_rec.codec.clone(),
                recommended_codec_level: codec_rec.codec_level,
                encoding_rule_fired: format!("{:?}", enc_rec.rule_fired),
                reason_brief: enc_rec.reason_brief.clone(),
                confidence: confidence_str,
                confidence_reason: enc_rec.confidence_reason.clone(),
                impact_stars,
                engine_compatibility,
                caveats: codec_rec.caveats.clone(),
                full_explain,
            };
            Some((col_rec, enc_rec, profile))
        })
        .collect();

    #[cfg(target_arch = "wasm32")]
    let column_results: Vec<(ColumnRecommendation, EncodingRecommendation, ColumnProfile)> = column_names.iter()
        .filter_map(|col_name| {
            let meta = file_profile.columns.iter().find(|c| &c.name == col_name)?;
            let profile = sample_column(path, col_name, 0, sample_rows)
                .map(|sample| profile_column(&sample))
                .unwrap_or_else(|_| fallback_profile(col_name, meta, file_profile.num_rows));
            let enc_rec = recommend_encoding(&profile, meta);
            let mut codec_rec = recommend_codec(&profile, &enc_rec, priority, engine);
            apply_engine_overrides(&mut codec_rec, engine);
            let impact_stars = compute_impact_stars(&enc_rec, &codec_rec, meta, file_total_uncompressed);
            let engine_name = engine_to_str(engine);
            let engine_compatibility = compute_engine_compat(&enc_rec, engine, engine_name);
            let confidence_str = format!("{:?}", enc_rec.confidence);
            let full_explain = if explain == "full" {
                Some(build_full_explain(&profile, &enc_rec, &engine_compatibility))
            } else {
                None
            };
            let col_rec = ColumnRecommendation {
                column_name: col_name.clone(),
                physical_type: meta.physical_type.clone(),
                logical_type: meta.logical_type.clone(),
                cardinality_estimate: profile.cardinality_estimate,
                cardinality_ratio: profile.cardinality_ratio,
                null_fraction: profile.null_fraction,
                recommended_encoding: enc_rec.encoding.clone(),
                recommended_codec: codec_rec.codec.clone(),
                recommended_codec_level: codec_rec.codec_level,
                encoding_rule_fired: format!("{:?}", enc_rec.rule_fired),
                reason_brief: enc_rec.reason_brief.clone(),
                confidence: confidence_str,
                confidence_reason: enc_rec.confidence_reason.clone(),
                impact_stars,
                engine_compatibility,
                caveats: codec_rec.caveats.clone(),
                full_explain,
            };
            Some((col_rec, enc_rec, profile))
        })
        .collect();

    let mut column_recs: Vec<ColumnRecommendation> = Vec::with_capacity(column_results.len());
    let mut enc_recs: Vec<EncodingRecommendation> = Vec::with_capacity(column_results.len());
    let mut column_profiles_vec: Vec<ColumnProfile> = Vec::with_capacity(column_results.len());
    for (col_rec, enc_rec, profile) in column_results {
        column_recs.push(col_rec);
        enc_recs.push(enc_rec);
        column_profiles_vec.push(profile);
    }

    let current_codec = mode_codec(&file_profile.columns);

    let sample_fraction = if file_profile.num_rows > 0 {
        (sample_rows as f64 / file_profile.num_rows as f64).min(1.0)
    } else {
        1.0
    };

    let size_reduction = predicted_size_reduction_pct(&column_recs, &file_profile);
    let predicted_read_speedup = 1.0 + (size_reduction / 100.0) * 0.7;

    let overall_confidence = if column_recs.iter().any(|c| c.confidence == "Low") {
        "Low".to_string()
    } else if column_recs.iter().any(|c| c.confidence == "Medium") {
        "Medium".to_string()
    } else {
        "High".to_string()
    };

    let scan_time_ms = start.elapsed().as_millis() as u64;

    // Generate snippets (placeholder — overridden by Python layer)
    let python_snippet = String::new();
    let spark_snippet = String::new();

    let options = generate_option_bundles(
        &file_profile,
        &column_recs,
        engine,
        &enc_recs,
    );

    let row_group_advisory = analyze_row_groups(&file_profile, engine);
    let sort_advisory = detect_sort_order(&file_profile, &column_profiles_vec);
    let diagnostics = crate::diagnostics::diagnose_all(&file_profile, &column_recs);

    Ok(TuneReport {
        file_path: path.to_string_lossy().to_string(),
        engine: engine_to_str(engine).to_string(),
        priority: priority_to_str(priority).to_string(),
        file_size_bytes: file_profile.file_size_bytes,
        num_rows: file_profile.num_rows,
        num_columns: column_recs.len(),
        current_codec,
        scan_time_ms,
        sample_fraction,
        predicted_size_reduction_pct: size_reduction,
        predicted_read_speedup,
        overall_confidence,
        columns: column_recs,
        file_caveats: vec![],
        python_snippet,
        spark_snippet,
        options,
        row_group_advisory,
        sort_advisory,
        file_profile,
        column_profiles: column_profiles_vec,
        diagnostics,
    })
}

fn process_columns_from_profiles(
    file_profile: &FileProfile,
    column_profiles: &[ColumnProfile],
    engine: &Engine,
    priority: &Priority,
    explain: &str,
) -> Vec<(ColumnRecommendation, EncodingRecommendation, ColumnProfile)> {
    let file_total_uncompressed: i64 = file_profile.columns.iter().map(|c| c.uncompressed_bytes).sum();
    column_profiles.iter().filter_map(|profile| {
        let meta = file_profile.columns.iter().find(|c| c.name == profile.column_name)?;
        let enc_rec = recommend_encoding(profile, meta);
        let mut codec_rec = recommend_codec(profile, &enc_rec, priority, engine);
        apply_engine_overrides(&mut codec_rec, engine);
        let impact_stars = compute_impact_stars(&enc_rec, &codec_rec, meta, file_total_uncompressed);
        let engine_name = engine_to_str(engine);
        let engine_compatibility = compute_engine_compat(&enc_rec, engine, engine_name);
        let confidence_str = format!("{:?}", enc_rec.confidence);
        let full_explain = if explain == "full" {
            Some(build_full_explain(profile, &enc_rec, &engine_compatibility))
        } else {
            None
        };
        let col_rec = ColumnRecommendation {
            column_name: profile.column_name.clone(),
            physical_type: meta.physical_type.clone(),
            logical_type: meta.logical_type.clone(),
            cardinality_estimate: profile.cardinality_estimate,
            cardinality_ratio: profile.cardinality_ratio,
            null_fraction: profile.null_fraction,
            recommended_encoding: enc_rec.encoding.clone(),
            recommended_codec: codec_rec.codec.clone(),
            recommended_codec_level: codec_rec.codec_level,
            encoding_rule_fired: format!("{:?}", enc_rec.rule_fired),
            reason_brief: enc_rec.reason_brief.clone(),
            confidence: confidence_str,
            confidence_reason: enc_rec.confidence_reason.clone(),
            impact_stars,
            engine_compatibility,
            caveats: codec_rec.caveats.clone(),
            full_explain,
        };
        Some((col_rec, enc_rec, profile.clone()))
    }).collect()
}

pub fn build_tune_report_from_profiles(
    file_profile: &FileProfile,
    column_profiles: &[ColumnProfile],
    engine: &Engine,
    priority: &Priority,
    explain: &str,
) -> Result<TuneReport, crate::AutoparqError> {
    let start = Instant::now();

    let column_results = process_columns_from_profiles(file_profile, column_profiles, engine, priority, explain);

    let mut column_recs: Vec<ColumnRecommendation> = Vec::with_capacity(column_results.len());
    let mut enc_recs: Vec<EncodingRecommendation> = Vec::with_capacity(column_results.len());
    let mut column_profiles_out: Vec<ColumnProfile> = Vec::with_capacity(column_results.len());
    for (col_rec, enc_rec, profile) in column_results {
        column_recs.push(col_rec);
        enc_recs.push(enc_rec);
        column_profiles_out.push(profile);
    }

    let current_codec = mode_codec(&file_profile.columns);

    let total_rows = file_profile.num_rows;
    let sample_rows = column_profiles.first().map(|p| p.sample_rows).unwrap_or(0);
    let sample_fraction = if total_rows > 0 {
        (sample_rows as f64 / total_rows as f64).min(1.0)
    } else {
        1.0
    };

    let size_reduction = predicted_size_reduction_pct(&column_recs, file_profile);
    let predicted_read_speedup = 1.0 + (size_reduction / 100.0) * 0.7;

    let overall_confidence = if column_recs.iter().any(|c| c.confidence == "Low") {
        "Low".to_string()
    } else if column_recs.iter().any(|c| c.confidence == "Medium") {
        "Medium".to_string()
    } else {
        "High".to_string()
    };

    let scan_time_ms = start.elapsed().as_millis() as u64;
    let python_snippet = String::new();
    let spark_snippet = String::new();

    let options = generate_option_bundles(file_profile, &column_recs, engine, &enc_recs);
    let row_group_advisory = analyze_row_groups(file_profile, engine);
    let sort_advisory = detect_sort_order(file_profile, &column_profiles_out);
    let diagnostics = crate::diagnostics::diagnose_all(file_profile, &column_recs);

    Ok(TuneReport {
        file_path: file_profile.path.clone(),
        engine: engine_to_str(engine).to_string(),
        priority: priority_to_str(priority).to_string(),
        file_size_bytes: file_profile.file_size_bytes,
        num_rows: file_profile.num_rows,
        num_columns: column_recs.len(),
        current_codec,
        scan_time_ms,
        sample_fraction,
        predicted_size_reduction_pct: size_reduction,
        predicted_read_speedup,
        overall_confidence,
        columns: column_recs,
        file_caveats: vec![],
        python_snippet,
        spark_snippet,
        options,
        row_group_advisory,
        sort_advisory,
        file_profile: file_profile.clone(),
        column_profiles: column_profiles_out,
        diagnostics,
    })
}

pub fn build_tune_report_from_bytes(
    data: &[u8],
    engine: &Engine,
    priority: &Priority,
    sample_rows: usize,
    explain: &str,
) -> Result<TuneReport, crate::AutoparqError> {
    use crate::profiler::metadata::read_file_metadata_from_bytes;
    use crate::profiler::sampler::sample_column_from_bytes;

    let start = Instant::now();

    let file_profile = read_file_metadata_from_bytes(data)?;
    let bytes = bytes::Bytes::copy_from_slice(data);
    let file_total_uncompressed: i64 = file_profile.columns.iter().map(|c| c.uncompressed_bytes).sum();

    let column_names: Vec<String> = file_profile.columns.iter().map(|c| c.name.clone()).collect();

    let column_results: Vec<(ColumnRecommendation, EncodingRecommendation, ColumnProfile)> =
        column_names.iter().filter_map(|col_name| {
            let meta = file_profile.columns.iter().find(|c| &c.name == col_name)?;
            let profile = sample_column_from_bytes(bytes.clone(), col_name, 0, sample_rows)
                .map(|sample| profile_column(&sample))
                .unwrap_or_else(|_| fallback_profile(col_name, meta, file_profile.num_rows));
            let enc_rec = recommend_encoding(&profile, meta);
            let mut codec_rec = recommend_codec(&profile, &enc_rec, priority, engine);
            apply_engine_overrides(&mut codec_rec, engine);
            let impact_stars = compute_impact_stars(&enc_rec, &codec_rec, meta, file_total_uncompressed);
            let engine_name = engine_to_str(engine);
            let engine_compatibility = compute_engine_compat(&enc_rec, engine, engine_name);
            let confidence_str = format!("{:?}", enc_rec.confidence);
            let full_explain = if explain == "full" {
                Some(build_full_explain(&profile, &enc_rec, &engine_compatibility))
            } else {
                None
            };
            let col_rec = ColumnRecommendation {
                column_name: col_name.clone(),
                physical_type: meta.physical_type.clone(),
                logical_type: meta.logical_type.clone(),
                cardinality_estimate: profile.cardinality_estimate,
                cardinality_ratio: profile.cardinality_ratio,
                null_fraction: profile.null_fraction,
                recommended_encoding: enc_rec.encoding.clone(),
                recommended_codec: codec_rec.codec.clone(),
                recommended_codec_level: codec_rec.codec_level,
                encoding_rule_fired: format!("{:?}", enc_rec.rule_fired),
                reason_brief: enc_rec.reason_brief.clone(),
                confidence: confidence_str,
                confidence_reason: enc_rec.confidence_reason.clone(),
                impact_stars,
                engine_compatibility,
                caveats: codec_rec.caveats.clone(),
                full_explain,
            };
            Some((col_rec, enc_rec, profile))
        }).collect();

    let mut column_recs: Vec<ColumnRecommendation> = Vec::with_capacity(column_results.len());
    let mut enc_recs: Vec<EncodingRecommendation> = Vec::with_capacity(column_results.len());
    let mut column_profiles_vec: Vec<ColumnProfile> = Vec::with_capacity(column_results.len());
    for (col_rec, enc_rec, profile) in column_results {
        column_recs.push(col_rec);
        enc_recs.push(enc_rec);
        column_profiles_vec.push(profile);
    }

    let current_codec = mode_codec(&file_profile.columns);

    let sample_fraction = if file_profile.num_rows > 0 {
        (sample_rows as f64 / file_profile.num_rows as f64).min(1.0)
    } else {
        1.0
    };

    let size_reduction = predicted_size_reduction_pct(&column_recs, &file_profile);
    let predicted_read_speedup = 1.0 + (size_reduction / 100.0) * 0.7;

    let overall_confidence = if column_recs.iter().any(|c| c.confidence == "Low") {
        "Low".to_string()
    } else if column_recs.iter().any(|c| c.confidence == "Medium") {
        "Medium".to_string()
    } else {
        "High".to_string()
    };

    let scan_time_ms = start.elapsed().as_millis() as u64;
    let python_snippet = String::new();
    let spark_snippet = String::new();

    let options = generate_option_bundles(&file_profile, &column_recs, engine, &enc_recs);
    let row_group_advisory = analyze_row_groups(&file_profile, engine);
    let sort_advisory = detect_sort_order(&file_profile, &column_profiles_vec);
    let diagnostics = crate::diagnostics::diagnose_all(&file_profile, &column_recs);

    Ok(TuneReport {
        file_path: "<memory>".to_string(),
        engine: engine_to_str(engine).to_string(),
        priority: priority_to_str(priority).to_string(),
        file_size_bytes: file_profile.file_size_bytes,
        num_rows: file_profile.num_rows,
        num_columns: column_recs.len(),
        current_codec,
        scan_time_ms,
        sample_fraction,
        predicted_size_reduction_pct: size_reduction,
        predicted_read_speedup,
        overall_confidence,
        columns: column_recs,
        file_caveats: vec![],
        python_snippet,
        spark_snippet,
        options,
        row_group_advisory,
        sort_advisory,
        file_profile,
        column_profiles: column_profiles_vec,
        diagnostics,
    })
}

pub fn build_tune_report_from_bytes_with_progress<F: Fn(usize, usize, &str)>(
    data: &[u8],
    engine: &Engine,
    priority: &Priority,
    sample_rows: usize,
    explain: &str,
    on_progress: F,
) -> Result<TuneReport, crate::AutoparqError> {
    use crate::profiler::metadata::read_file_metadata_from_bytes;
    use crate::profiler::sampler::sample_column_from_bytes;

    let start = Instant::now();

    let file_profile = read_file_metadata_from_bytes(data)?;
    let bytes = bytes::Bytes::copy_from_slice(data);
    let file_total_uncompressed: i64 = file_profile.columns.iter().map(|c| c.uncompressed_bytes).sum();

    let column_names: Vec<String> = file_profile.columns.iter().map(|c| c.name.clone()).collect();
    let total_cols = column_names.len();

    let mut column_results: Vec<(ColumnRecommendation, EncodingRecommendation, ColumnProfile)> = Vec::new();
    for (col_index, col_name) in column_names.iter().enumerate() {
        on_progress(col_index, total_cols, col_name);
        let meta = match file_profile.columns.iter().find(|c| &c.name == col_name) {
            Some(m) => m,
            None => continue,
        };
        let profile = sample_column_from_bytes(bytes.clone(), col_name, 0, sample_rows)
            .map(|sample| profile_column(&sample))
            .unwrap_or_else(|_| fallback_profile(col_name, meta, file_profile.num_rows));
        let enc_rec = recommend_encoding(&profile, meta);
        let mut codec_rec = recommend_codec(&profile, &enc_rec, priority, engine);
        apply_engine_overrides(&mut codec_rec, engine);
        let impact_stars = compute_impact_stars(&enc_rec, &codec_rec, meta, file_total_uncompressed);
        let engine_name = engine_to_str(engine);
        let engine_compatibility = compute_engine_compat(&enc_rec, engine, engine_name);
        let confidence_str = format!("{:?}", enc_rec.confidence);
        let full_explain = if explain == "full" {
            Some(build_full_explain(&profile, &enc_rec, &engine_compatibility))
        } else {
            None
        };
        let col_rec = ColumnRecommendation {
            column_name: col_name.clone(),
            physical_type: meta.physical_type.clone(),
            logical_type: meta.logical_type.clone(),
            cardinality_estimate: profile.cardinality_estimate,
            cardinality_ratio: profile.cardinality_ratio,
            null_fraction: profile.null_fraction,
            recommended_encoding: enc_rec.encoding.clone(),
            recommended_codec: codec_rec.codec.clone(),
            recommended_codec_level: codec_rec.codec_level,
            encoding_rule_fired: format!("{:?}", enc_rec.rule_fired),
            reason_brief: enc_rec.reason_brief.clone(),
            confidence: confidence_str,
            confidence_reason: enc_rec.confidence_reason.clone(),
            impact_stars,
            engine_compatibility,
            caveats: codec_rec.caveats.clone(),
            full_explain,
        };
        column_results.push((col_rec, enc_rec, profile));
    }

    let mut column_recs: Vec<ColumnRecommendation> = Vec::with_capacity(column_results.len());
    let mut enc_recs: Vec<EncodingRecommendation> = Vec::with_capacity(column_results.len());
    let mut column_profiles_vec: Vec<ColumnProfile> = Vec::with_capacity(column_results.len());
    for (col_rec, enc_rec, profile) in column_results {
        column_recs.push(col_rec);
        enc_recs.push(enc_rec);
        column_profiles_vec.push(profile);
    }

    let current_codec = mode_codec(&file_profile.columns);

    let sample_fraction = if file_profile.num_rows > 0 {
        (sample_rows as f64 / file_profile.num_rows as f64).min(1.0)
    } else {
        1.0
    };

    let size_reduction = predicted_size_reduction_pct(&column_recs, &file_profile);
    let predicted_read_speedup = 1.0 + (size_reduction / 100.0) * 0.7;

    let overall_confidence = if column_recs.iter().any(|c| c.confidence == "Low") {
        "Low".to_string()
    } else if column_recs.iter().any(|c| c.confidence == "Medium") {
        "Medium".to_string()
    } else {
        "High".to_string()
    };

    let scan_time_ms = start.elapsed().as_millis() as u64;
    let python_snippet = String::new();
    let spark_snippet = String::new();

    let options = generate_option_bundles(&file_profile, &column_recs, engine, &enc_recs);
    let row_group_advisory = analyze_row_groups(&file_profile, engine);
    let sort_advisory = detect_sort_order(&file_profile, &column_profiles_vec);
    let diagnostics = crate::diagnostics::diagnose_all(&file_profile, &column_recs);

    Ok(TuneReport {
        file_path: "<memory>".to_string(),
        engine: engine_to_str(engine).to_string(),
        priority: priority_to_str(priority).to_string(),
        file_size_bytes: file_profile.file_size_bytes,
        num_rows: file_profile.num_rows,
        num_columns: column_recs.len(),
        current_codec,
        scan_time_ms,
        sample_fraction,
        predicted_size_reduction_pct: size_reduction,
        predicted_read_speedup,
        overall_confidence,
        columns: column_recs,
        file_caveats: vec![],
        python_snippet,
        spark_snippet,
        options,
        row_group_advisory,
        sort_advisory,
        file_profile,
        column_profiles: column_profiles_vec,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recommender::codec::CodecRecommendation;
    use crate::recommender::encoding::{ConfidenceTier, EncodingRecommendation, RuleName};

    #[test]
    fn test_bundle_labels_are_distinct() {
        let labels = ["Balanced", "Smallest File", "Fastest Reads"];
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }

    fn mk_enc(encoding: &str) -> EncodingRecommendation {
        EncodingRecommendation {
            encoding: encoding.to_string(),
            rule_fired: RuleName::PlainDefault,
            reason_brief: String::new(),
            confidence: ConfidenceTier::High,
            confidence_reason: String::new(),
        }
    }

    fn mk_codec(codec: &str, level: Option<i32>) -> CodecRecommendation {
        CodecRecommendation {
            codec: codec.to_string(),
            codec_level: level,
            reason_brief: String::new(),
            caveats: vec![],
        }
    }

    fn mk_meta(
        encodings: Vec<&str>,
        codec: &str,
        uncompressed_bytes: i64,
    ) -> crate::profiler::metadata::ColumnMetaSummary {
        crate::profiler::metadata::ColumnMetaSummary {
            name: "c".to_string(),
            physical_type: "INT32".to_string(),
            logical_type: None,
            encodings: encodings.into_iter().map(String::from).collect(),
            codec: codec.to_string(),
            compressed_bytes: uncompressed_bytes / 2,
            uncompressed_bytes,
            compression_ratio: 2.0,
            total_null_count: Some(0),
            min_value: None,
            max_value: None,
            statistics_available: true,
            per_row_group_encodings: vec![],
            per_row_group_compressed_bytes: vec![],
            per_row_group_uncompressed_bytes: vec![],
            per_row_group_dict_page_bytes: vec![],
        }
    }

    #[test]
    fn impact_stars_no_change_returns_one() {
        let enc = mk_enc("RLE_DICTIONARY");
        let codec = mk_codec("ZSTD", Some(3));
        let meta = mk_meta(vec!["RLE_DICTIONARY"], "ZSTD:3", 1_000_000);
        assert_eq!(compute_impact_stars(&enc, &codec, &meta, 10_000_000), 1);
    }

    #[test]
    fn impact_stars_codec_only_small_column_returns_one() {
        // Column is 0.5% of file — codec-only change shouldn't look impactful.
        let enc = mk_enc("RLE_DICTIONARY");
        let codec = mk_codec("ZSTD", Some(3));
        let meta = mk_meta(vec!["RLE_DICTIONARY"], "ZSTD:1", 500_000);
        assert_eq!(compute_impact_stars(&enc, &codec, &meta, 100_000_000), 1);
    }

    #[test]
    fn impact_stars_codec_only_big_column_returns_three() {
        // Column is 15% of file — codec-only change is visible.
        let enc = mk_enc("RLE_DICTIONARY");
        let codec = mk_codec("ZSTD", Some(3));
        let meta = mk_meta(vec!["RLE_DICTIONARY"], "ZSTD:1", 15_000_000);
        assert_eq!(compute_impact_stars(&enc, &codec, &meta, 100_000_000), 3);
    }

    #[test]
    fn impact_stars_encoding_change_big_column_returns_five() {
        // Column is 20% of file, encoding will change.
        let enc = mk_enc("RLE_DICTIONARY");
        let codec = mk_codec("ZSTD", Some(3));
        let meta = mk_meta(vec!["PLAIN"], "ZSTD:3", 20_000_000);
        assert_eq!(compute_impact_stars(&enc, &codec, &meta, 100_000_000), 5);
    }

    #[test]
    fn impact_stars_encoding_change_tiny_column_returns_two() {
        // Column is <1% of file, encoding will change.
        let enc = mk_enc("RLE_DICTIONARY");
        let codec = mk_codec("ZSTD", Some(3));
        let meta = mk_meta(vec!["PLAIN"], "ZSTD:3", 500_000);
        assert_eq!(compute_impact_stars(&enc, &codec, &meta, 100_000_000), 2);
    }
}
