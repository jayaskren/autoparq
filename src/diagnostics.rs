use serde::{Deserialize, Serialize};

use crate::profiler::metadata::ColumnMetaSummary;
use crate::tuner::ColumnRecommendation;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticStatus {
    Match,
    FallbackDictionary,
    IneffectiveEncoding,
    Mismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDiagnostic {
    pub column_name: String,
    pub current_encodings: Vec<String>,
    pub per_row_group_summary: Option<String>,
    pub current_codec: String,
    pub current_compression_ratio: f64,
    pub status: DiagnosticStatus,
    pub observation: String,
    pub cause_hypothesis: Option<String>,
    pub supporting_metric: Option<String>,
}

/// Encodings that represent def/rep level packing, not column data.
fn is_level_encoding(enc: &str) -> bool {
    matches!(enc, "RLE" | "BIT_PACKED")
}

/// Union of all data encodings across row groups (filtered, sorted, deduped).
pub fn aggregate_data_encodings(per_rg: &[Vec<String>]) -> Vec<String> {
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for rg in per_rg {
        for e in rg {
            if !is_level_encoding(e) {
                set.insert(e.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// Filter to data-only encodings, sorted+deduped (per row group).
fn filtered_rg(rg: &[String]) -> Vec<String> {
    let mut v: Vec<String> = rg
        .iter()
        .filter(|e| !is_level_encoding(e))
        .cloned()
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Format a comma-joined encoding list for display.
fn fmt_set(encs: &[String]) -> String {
    if encs.is_empty() {
        "(none)".to_string()
    } else {
        encs.join(",")
    }
}

/// Format a list of row-group indices as a compact range/list string.
fn fmt_rg_list(indices: &[usize]) -> String {
    if indices.is_empty() {
        return String::new();
    }
    if indices.len() == 1 {
        return format!("RG {}", indices[0]);
    }
    let mut parts: Vec<String> = Vec::new();
    let mut start = indices[0];
    let mut prev = indices[0];
    for &i in &indices[1..] {
        if i == prev + 1 {
            prev = i;
        } else {
            if start == prev {
                parts.push(format!("{}", start));
            } else {
                parts.push(format!("{}-{}", start, prev));
            }
            start = i;
            prev = i;
        }
    }
    if start == prev {
        parts.push(format!("{}", start));
    } else {
        parts.push(format!("{}-{}", start, prev));
    }
    format!("RGs {}", parts.join(","))
}

/// `None` when all RGs share an identical (filtered) encoding set.
/// Otherwise, a compact description grouping RGs with identical sets.
pub fn per_row_group_variance_summary(per_rg: &[Vec<String>]) -> Option<String> {
    if per_rg.len() < 2 {
        return None;
    }
    let filtered: Vec<Vec<String>> = per_rg.iter().map(|r| filtered_rg(r)).collect();
    let first = &filtered[0];
    if filtered.iter().all(|f| f == first) {
        return None;
    }

    // Group consecutive RGs with identical sets.
    let mut groups: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
    for (idx, enc_set) in filtered.iter().enumerate() {
        match groups.last_mut() {
            Some((last_set, last_idxs)) if last_set == enc_set => {
                last_idxs.push(idx);
            }
            _ => groups.push((enc_set.clone(), vec![idx])),
        }
    }

    let parts: Vec<String> = groups
        .into_iter()
        .map(|(enc_set, idxs)| format!("{} in {}", fmt_set(&enc_set), fmt_rg_list(&idxs)))
        .collect();
    Some(parts.join("; "))
}

fn has_dict_encoding(encs: &[String]) -> bool {
    encs.iter()
        .any(|e| e == "RLE_DICTIONARY" || e == "PLAIN_DICTIONARY")
}

fn has_plain_data(encs: &[String]) -> bool {
    encs.iter().any(|e| e == "PLAIN")
}

/// Mid-chunk fallback is only reliably detectable from the footer when the
/// dictionary page is near the writer's default overflow limit (~1 MB).
///
/// A healthy dictionary-encoded column chunk always contains *both* encodings:
/// `PLAIN` for the dictionary page itself (which stores the distinct values)
/// and `RLE_DICTIONARY` for the data pages that reference it. So the
/// coexistence of those two encodings is NOT by itself a fallback signal —
/// it's the normal state. The real fallback signature is coexistence + a
/// dictionary page that filled up to the writer's limit.
///
/// We require dict_page_bytes >= 900 KB before flagging a chunk as a
/// mid-chunk fallback. Below that, the dictionary fit comfortably and the
/// PLAIN encoding is just the dictionary page's internal encoding.
const DICT_OVERFLOW_FLOOR_BYTES: i64 = 900_000;

fn mid_chunk_fallback_rgs(
    per_rg: &[Vec<String>],
    dict_bytes: &[Option<i64>],
) -> Vec<usize> {
    per_rg
        .iter()
        .enumerate()
        .filter(|(idx, encs)| {
            if !(has_dict_encoding(encs) && has_plain_data(encs)) {
                return false;
            }
            matches!(dict_bytes.get(*idx).copied().flatten(),
                     Some(b) if b >= DICT_OVERFLOW_FLOOR_BYTES)
        })
        .map(|(idx, _)| idx)
        .collect()
}

/// Cross-RG fallback: some RGs have dictionary encoding, others have PLAIN only.
fn cross_rg_dict_mix(per_rg: &[Vec<String>]) -> (Vec<usize>, Vec<usize>) {
    let dict_rgs: Vec<usize> = per_rg
        .iter()
        .enumerate()
        .filter(|(_, encs)| has_dict_encoding(encs))
        .map(|(idx, _)| idx)
        .collect();
    let plain_only_rgs: Vec<usize> = per_rg
        .iter()
        .enumerate()
        .filter(|(_, encs)| !has_dict_encoding(encs) && has_plain_data(encs))
        .map(|(idx, _)| idx)
        .collect();
    (dict_rgs, plain_only_rgs)
}

fn largest_dict_page_bytes(per_rg_dict: &[Option<i64>]) -> Option<i64> {
    per_rg_dict.iter().filter_map(|x| *x).max()
}

fn human_bytes(n: i64) -> String {
    if n >= 1_048_576 {
        format!("{:.2} MB", n as f64 / 1_048_576.0)
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{} B", n)
    }
}

fn recommended_codec_full(rec: &ColumnRecommendation) -> String {
    match rec.recommended_codec_level {
        Some(l) => format!("{}:{}", rec.recommended_codec, l),
        None => rec.recommended_codec.clone(),
    }
}

const DELTA_RATIO_THRESHOLD: f64 = 1.5;
const BSS_RATIO_THRESHOLD: f64 = 1.1;
const MIN_BYTES_FOR_EFFECTIVENESS_CHECK: i64 = 10_000;

/// Apply rules R1–R6 (first match wins except R1 which is the default-success case).
pub fn diagnose_column(
    meta: &ColumnMetaSummary,
    rec: &ColumnRecommendation,
) -> ColumnDiagnostic {
    let current_encodings = aggregate_data_encodings(&meta.per_row_group_encodings);
    let per_row_group_summary = per_row_group_variance_summary(&meta.per_row_group_encodings);
    let current_codec = meta.codec.clone();
    let current_compression_ratio = meta.compression_ratio;
    let rec_codec_full = recommended_codec_full(rec);

    // --- R2: mid-chunk dictionary fallback (only when dict page is near overflow limit) ---
    let mid_chunk = mid_chunk_fallback_rgs(
        &meta.per_row_group_encodings,
        &meta.per_row_group_dict_page_bytes,
    );
    if !mid_chunk.is_empty() {
        let dict_mb = largest_dict_page_bytes(&meta.per_row_group_dict_page_bytes);
        let supporting_metric = dict_mb.map(|b| format!("largest dict page \u{2248} {}", human_bytes(b)));
        let observation = format!(
            "Dictionary encoding overflowed mid-chunk in {}. Data pages fell back to PLAIN within those row groups.",
            fmt_rg_list(&mid_chunk)
        );
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::FallbackDictionary,
            observation,
            cause_hypothesis: Some(
                "Dictionary page exceeded the writer's limit (default 1 MB) — cardinality too high for dictionary encoding.".to_string(),
            ),
            supporting_metric,
        };
    }

    // --- R2b: cross-RG fallback ---
    let (dict_rgs, plain_only_rgs) = cross_rg_dict_mix(&meta.per_row_group_encodings);
    if !dict_rgs.is_empty() && !plain_only_rgs.is_empty() {
        let dict_mb = largest_dict_page_bytes(&meta.per_row_group_dict_page_bytes);
        let supporting_metric = dict_mb.map(|b| format!("largest dict page \u{2248} {}", human_bytes(b)));
        let observation = format!(
            "Dictionary encoding used in {}; fell back to PLAIN in {}.",
            fmt_rg_list(&dict_rgs),
            fmt_rg_list(&plain_only_rgs)
        );
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::FallbackDictionary,
            observation,
            cause_hypothesis: Some(
                "Dictionary page filled up in some row groups (likely cardinality overflow) — consider increasing the dictionary page size or accepting PLAIN for this column.".to_string(),
            ),
            supporting_metric,
        };
    }

    // --- R3: recommendation is dictionary, but no dictionary encoding found ---
    if rec.recommended_encoding == "RLE_DICTIONARY"
        && !current_encodings
            .iter()
            .any(|e| e == "RLE_DICTIONARY" || e == "PLAIN_DICTIONARY")
    {
        let observation = format!(
            "Column is currently {} + {}; writer did not apply dictionary encoding.",
            fmt_set(&current_encodings),
            current_codec
        );
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::Mismatch,
            observation,
            cause_hypothesis: Some(
                "Writer did not hint dictionary encoding — applying our recommendation would likely reduce size substantially.".to_string(),
            ),
            supporting_metric: Some(format!(
                "cardinality_ratio={:.4}",
                rec.cardinality_ratio
            )),
        };
    }

    // --- R4: DELTA applied but ineffective ---
    let big_enough = meta.uncompressed_bytes >= MIN_BYTES_FOR_EFFECTIVENESS_CHECK;
    if big_enough
        && current_encodings.iter().any(|e| e == "DELTA_BINARY_PACKED")
        && rec.recommended_encoding != "DELTA_BINARY_PACKED"
        && current_compression_ratio < DELTA_RATIO_THRESHOLD
    {
        let observation = format!(
            "DELTA_BINARY_PACKED is applied but achieving only {:.2}\u{00D7} compression — values likely aren't monotonic.",
            current_compression_ratio
        );
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::IneffectiveEncoding,
            observation,
            cause_hypothesis: Some(
                "DELTA encoding benefits require monotonic (usually sorted) values. Use PLAIN for arbitrary integer distributions.".to_string(),
            ),
            supporting_metric: Some(format!("ratio={:.2}\u{00D7}", current_compression_ratio)),
        };
    }

    // --- R5: BYTE_STREAM_SPLIT applied but ineffective ---
    if big_enough
        && current_encodings.iter().any(|e| e == "BYTE_STREAM_SPLIT")
        && current_compression_ratio < BSS_RATIO_THRESHOLD
    {
        let observation = format!(
            "BYTE_STREAM_SPLIT is applied but achieving only {:.2}\u{00D7} compression \u{2014} byte positions aren't well correlated across values.",
            current_compression_ratio
        );
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::IneffectiveEncoding,
            observation,
            cause_hypothesis: Some(
                "BSS benefits floats whose same byte-positions have similar distributions (e.g., physical measurements in a range). Arbitrary floats get no benefit.".to_string(),
            ),
            supporting_metric: Some(format!("ratio={:.2}\u{00D7}", current_compression_ratio)),
        };
    }

    // --- R1: Match ---
    // Consider Match when the recommended encoding is present in the current set
    // AND the codec with level matches.
    let enc_match = current_encodings.contains(&rec.recommended_encoding);
    let codec_match = current_codec == rec_codec_full;
    if enc_match && codec_match {
        return ColumnDiagnostic {
            column_name: meta.name.clone(),
            current_encodings,
            per_row_group_summary,
            current_codec,
            current_compression_ratio,
            status: DiagnosticStatus::Match,
            observation: "File matches recommendation.".to_string(),
            cause_hypothesis: None,
            supporting_metric: Some(format!("ratio={:.2}\u{00D7}", current_compression_ratio)),
        };
    }

    // --- R6: Mismatch (default) ---
    let diffs: Vec<&str> = [
        if !enc_match { Some("encoding") } else { None },
        if !codec_match { Some("codec") } else { None },
    ]
    .into_iter()
    .flatten()
    .collect();
    let observation = format!(
        "Current {} differs from recommendation (have: {} + {}; recommend: {} + {}).",
        diffs.join(" and "),
        fmt_set(&current_encodings),
        current_codec,
        rec.recommended_encoding,
        rec_codec_full
    );
    ColumnDiagnostic {
        column_name: meta.name.clone(),
        current_encodings,
        per_row_group_summary,
        current_codec,
        current_compression_ratio,
        status: DiagnosticStatus::Mismatch,
        observation,
        cause_hypothesis: None,
        supporting_metric: Some(format!("ratio={:.2}\u{00D7}", current_compression_ratio)),
    }
}

/// Produce diagnostics for every column in a file profile that has a recommendation.
pub fn diagnose_all(
    file_profile: &crate::profiler::metadata::FileProfile,
    recs: &[ColumnRecommendation],
) -> Vec<ColumnDiagnostic> {
    recs.iter()
        .filter_map(|rec| {
            let meta = file_profile
                .columns
                .iter()
                .find(|c| c.name == rec.column_name)?;
            Some(diagnose_column(meta, rec))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_filters_level_encodings() {
        let per_rg = vec![
            vec!["PLAIN".to_string(), "RLE".to_string()],
            vec!["RLE_DICTIONARY".to_string(), "RLE".to_string()],
        ];
        let agg = aggregate_data_encodings(&per_rg);
        assert_eq!(agg, vec!["PLAIN", "RLE_DICTIONARY"]);
    }

    #[test]
    fn variance_summary_none_when_identical() {
        let per_rg = vec![
            vec!["PLAIN".to_string()],
            vec!["PLAIN".to_string()],
            vec!["PLAIN".to_string()],
        ];
        assert_eq!(per_row_group_variance_summary(&per_rg), None);
    }

    #[test]
    fn variance_summary_reports_cross_rg_fallback() {
        let per_rg = vec![
            vec!["RLE_DICTIONARY".to_string()],
            vec!["RLE_DICTIONARY".to_string()],
            vec!["PLAIN".to_string()],
            vec!["PLAIN".to_string()],
        ];
        let s = per_row_group_variance_summary(&per_rg).unwrap();
        assert!(s.contains("RLE_DICTIONARY"));
        assert!(s.contains("PLAIN"));
        assert!(s.contains("RGs 0-1"));
        assert!(s.contains("RGs 2-3"));
    }

    #[test]
    fn mid_chunk_fallback_requires_dict_near_limit() {
        let per_rg = vec![
            vec!["PLAIN".to_string(), "RLE_DICTIONARY".to_string()],
            vec!["RLE_DICTIONARY".to_string()],
        ];

        // Small dict page -> normal dict-encoded column, NOT a fallback
        let small = vec![Some(2_000), None];
        assert!(mid_chunk_fallback_rgs(&per_rg, &small).is_empty());

        // Dict page near 1 MB limit -> real fallback
        let near_limit = vec![Some(950_000), None];
        assert_eq!(mid_chunk_fallback_rgs(&per_rg, &near_limit), vec![0]);

        // No dict page size info -> conservative, don't flag
        let unknown = vec![None, None];
        assert!(mid_chunk_fallback_rgs(&per_rg, &unknown).is_empty());
    }

    #[test]
    fn cross_rg_mix_detected() {
        let per_rg = vec![
            vec!["RLE_DICTIONARY".to_string()],
            vec!["PLAIN".to_string()],
        ];
        let (dict, plain) = cross_rg_dict_mix(&per_rg);
        assert_eq!(dict, vec![0]);
        assert_eq!(plain, vec![1]);
    }

    fn make_meta(name: &str, per_rg: Vec<Vec<String>>, codec: &str, ratio: f64) -> ColumnMetaSummary {
        let total_compressed: i64 = 1_000_000;
        let total_uncompressed = (total_compressed as f64 * ratio) as i64;
        let num = per_rg.len();
        ColumnMetaSummary {
            name: name.to_string(),
            physical_type: "INT64".to_string(),
            logical_type: None,
            encodings: {
                let mut s: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for rg in &per_rg {
                    for e in rg {
                        s.insert(e.clone());
                    }
                }
                s.into_iter().collect()
            },
            codec: codec.to_string(),
            compressed_bytes: total_compressed,
            uncompressed_bytes: total_uncompressed,
            compression_ratio: ratio,
            total_null_count: Some(0),
            min_value: None,
            max_value: None,
            statistics_available: true,
            per_row_group_encodings: per_rg,
            per_row_group_compressed_bytes: vec![total_compressed / num as i64; num],
            per_row_group_uncompressed_bytes: vec![total_uncompressed / num as i64; num],
            per_row_group_dict_page_bytes: vec![None; num],
        }
    }

    fn make_rec(col_name: &str, enc: &str, codec: &str, level: Option<i32>) -> ColumnRecommendation {
        ColumnRecommendation {
            column_name: col_name.to_string(),
            physical_type: "INT64".to_string(),
            logical_type: None,
            cardinality_estimate: 100,
            cardinality_ratio: 0.01,
            null_fraction: 0.0,
            recommended_encoding: enc.to_string(),
            recommended_codec: codec.to_string(),
            recommended_codec_level: level,
            encoding_rule_fired: "Test".to_string(),
            reason_brief: String::new(),
            confidence: "High".to_string(),
            confidence_reason: String::new(),
            impact_stars: 3,
            engine_compatibility: None,
            caveats: vec![],
            full_explain: None,
        }
    }

    #[test]
    fn r1_match_when_encoding_and_codec_align() {
        let meta = make_meta(
            "id",
            vec![vec!["DELTA_BINARY_PACKED".to_string()]],
            "ZSTD:3",
            5.0,
        );
        let rec = make_rec("id", "DELTA_BINARY_PACKED", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::Match);
    }

    #[test]
    fn r2_healthy_dict_column_does_not_flag_as_fallback() {
        // A healthy dictionary-encoded column legitimately contains both PLAIN
        // (from the dictionary page) and RLE_DICTIONARY (from the data pages).
        // With a small dict page, this must NOT flag as a fallback.
        let mut meta = make_meta(
            "col",
            vec![vec!["PLAIN".to_string(), "RLE_DICTIONARY".to_string()]],
            "ZSTD:3",
            3.0,
        );
        meta.per_row_group_dict_page_bytes = vec![Some(2_000)];
        let rec = make_rec("col", "RLE_DICTIONARY", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::Match);
    }

    #[test]
    fn r2_mid_chunk_fallback_flagged_when_dict_near_limit() {
        let mut meta = make_meta(
            "col",
            vec![vec!["PLAIN".to_string(), "RLE_DICTIONARY".to_string()]],
            "ZSTD:3",
            3.0,
        );
        meta.per_row_group_dict_page_bytes = vec![Some(1_000_000)];
        let rec = make_rec("col", "RLE_DICTIONARY", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::FallbackDictionary);
        assert!(diag.observation.contains("mid-chunk"));
    }

    #[test]
    fn r2b_cross_rg_fallback_flagged() {
        let meta = make_meta(
            "col",
            vec![
                vec!["RLE_DICTIONARY".to_string()],
                vec!["PLAIN".to_string()],
            ],
            "ZSTD:3",
            2.0,
        );
        let rec = make_rec("col", "RLE_DICTIONARY", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::FallbackDictionary);
    }

    #[test]
    fn r3_dictionary_never_attempted() {
        let meta = make_meta(
            "col",
            vec![vec!["PLAIN".to_string()]],
            "ZSTD:3",
            1.8,
        );
        let rec = make_rec("col", "RLE_DICTIONARY", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::Mismatch);
        assert!(diag.observation.contains("did not apply dictionary"));
    }

    #[test]
    fn r4_delta_ineffective() {
        let meta = make_meta(
            "col",
            vec![vec!["DELTA_BINARY_PACKED".to_string()]],
            "ZSTD:3",
            1.2,
        );
        let rec = make_rec("col", "PLAIN", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::IneffectiveEncoding);
    }

    #[test]
    fn r5_bss_ineffective() {
        let mut meta = make_meta(
            "col",
            vec![vec!["BYTE_STREAM_SPLIT".to_string()]],
            "ZSTD:3",
            1.05,
        );
        meta.physical_type = "DOUBLE".to_string();
        let rec = make_rec("col", "PLAIN", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::IneffectiveEncoding);
    }

    #[test]
    fn r6_mismatch_default() {
        let meta = make_meta(
            "col",
            vec![vec!["PLAIN".to_string()]],
            "SNAPPY",
            2.0,
        );
        let rec = make_rec("col", "PLAIN", "ZSTD", Some(3));
        let diag = diagnose_column(&meta, &rec);
        assert_eq!(diag.status, DiagnosticStatus::Mismatch);
    }
}
