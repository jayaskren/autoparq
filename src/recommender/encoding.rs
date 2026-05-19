use serde::{Serialize, Deserialize};
use crate::profiler::{metadata::ColumnMetaSummary, stats::ColumnProfile};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuleName {
    BooleanRle,
    DeltaMonotonic,
    RleDictionary,
    ByteStreamSplit,
    PlainUuid,
    PlainDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfidenceTier {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingRecommendation {
    pub encoding: String,
    pub rule_fired: RuleName,
    pub reason_brief: String,
    pub confidence: ConfidenceTier,
    pub confidence_reason: String,
}

fn compute_confidence(profile: &ColumnProfile, rule: &RuleName) -> (ConfidenceTier, String) {
    let high = profile.sample_fraction >= 0.10 && profile.sample_rows >= 100_000;
    let medium = profile.sample_fraction >= 0.02 || profile.sample_rows >= 50_000;

    // Boundary downgrade for RleDictionary near the 0.10 threshold
    let near_boundary = matches!(rule, RuleName::RleDictionary)
        && profile.cardinality_ratio >= 0.08
        && profile.cardinality_ratio <= 0.12;

    if high && !near_boundary {
        (
            ConfidenceTier::High,
            format!(
                "sample_fraction={:.3} >= 0.10 and sample_rows={} >= 100,000",
                profile.sample_fraction, profile.sample_rows
            ),
        )
    } else if medium || near_boundary {
        let reason = if near_boundary {
            format!(
                "cardinality_ratio={:.4} is within 20% of 0.10 threshold — boundary case",
                profile.cardinality_ratio
            )
        } else {
            format!(
                "sample_fraction={:.3} or sample_rows={} meets medium threshold",
                profile.sample_fraction, profile.sample_rows
            )
        };
        (ConfidenceTier::Medium, reason)
    } else {
        (
            ConfidenceTier::Low,
            format!(
                "small sample: sample_fraction={:.3} < 0.02 and sample_rows={} < 50,000",
                profile.sample_fraction, profile.sample_rows
            ),
        )
    }
}

pub fn recommend_encoding(
    profile: &ColumnProfile,
    meta: &ColumnMetaSummary,
) -> EncodingRecommendation {
    // Rule 1: BooleanRle
    if meta.physical_type == "BOOLEAN" {
        let rule = RuleName::BooleanRle;
        let (confidence, confidence_reason) = compute_confidence(profile, &rule);
        return EncodingRecommendation {
            encoding: "RLE".to_string(),
            rule_fired: rule,
            reason_brief: "BOOLEAN columns use RLE automatically in all Parquet libraries"
                .to_string(),
            confidence,
            confidence_reason,
        };
    }

    // Rule 2: DeltaMonotonic
    let is_int_or_temporal = meta.physical_type == "INT32"
        || meta.physical_type == "INT64"
        || meta
            .logical_type
            .as_deref()
            .map_or(false, |lt| lt.starts_with("TIMESTAMP") || lt.starts_with("DATE"));
    if is_int_or_temporal {
        if let Some(score) = profile.monotonicity_score {
            if score >= 0.90 {
                let rule = RuleName::DeltaMonotonic;
                let (confidence, confidence_reason) = compute_confidence(profile, &rule);
                return EncodingRecommendation {
                    encoding: "DELTA_BINARY_PACKED".to_string(),
                    rule_fired: rule,
                    reason_brief: format!(
                        "monotonicity_score={:.3} >= threshold 0.90",
                        score
                    ),
                    confidence,
                    confidence_reason,
                };
            }
        }
    }

    // Rule 3: RleDictionary
    {
        let avg_value_bytes = match meta.physical_type.as_str() {
            "BYTE_ARRAY" => profile
                .string_length_stats
                .as_ref()
                .map_or(8.0, |s| s.mean_len),
            "INT32" | "FLOAT" => 4.0,
            "INT64" | "DOUBLE" => 8.0,
            "BOOLEAN" => 1.0,
            _ => 8.0,
        };
        let dict_size = profile.cardinality_estimate as f64 * avg_value_bytes;

        if profile.cardinality_ratio < 0.10 && dict_size < 524_288.0 {
            let rule = RuleName::RleDictionary;
            let (confidence, confidence_reason) = compute_confidence(profile, &rule);
            return EncodingRecommendation {
                encoding: "RLE_DICTIONARY".to_string(),
                rule_fired: rule,
                reason_brief: format!(
                    "cardinality_ratio={:.4} ({} distinct / {} rows) < threshold 0.10",
                    profile.cardinality_ratio,
                    profile.cardinality_estimate,
                    profile.sample_rows
                ),
                confidence,
                confidence_reason,
            };
        }
    }

    // Rule 4: ByteStreamSplit
    if (meta.physical_type == "FLOAT" || meta.physical_type == "DOUBLE")
        && profile.cardinality_ratio > 0.50
    {
        let rule = RuleName::ByteStreamSplit;
        let (confidence, confidence_reason) = compute_confidence(profile, &rule);
        return EncodingRecommendation {
            encoding: "BYTE_STREAM_SPLIT".to_string(),
            rule_fired: rule,
            reason_brief: format!(
                "high-entropy float column (cardinality_ratio={:.3} > 0.50)",
                profile.cardinality_ratio
            ),
            confidence,
            confidence_reason,
        };
    }

    // Rule 5: PlainUuid
    if meta.physical_type == "BYTE_ARRAY" && profile.uuid_pattern_detected {
        let rule = RuleName::PlainUuid;
        let (confidence, confidence_reason) = compute_confidence(profile, &rule);
        return EncodingRecommendation {
            encoding: "PLAIN".to_string(),
            rule_fired: rule,
            reason_brief: format!(
                "UUID pattern detected — dictionary encoding would overflow ({} distinct values)",
                profile.cardinality_estimate
            ),
            confidence,
            confidence_reason,
        };
    }

    // Rule 6: PlainDefault (catch-all)
    let rule = RuleName::PlainDefault;
    let (confidence, confidence_reason) = compute_confidence(profile, &rule);
    EncodingRecommendation {
        encoding: "PLAIN".to_string(),
        rule_fired: rule,
        reason_brief: "no specific pattern detected; PLAIN with codec is the safe baseline"
            .to_string(),
        confidence,
        confidence_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::stats::StringLengthStats;

    fn make_profile(
        physical_type: &str,
        logical_type: Option<&str>,
    ) -> (ColumnProfile, ColumnMetaSummary) {
        let profile = ColumnProfile {
            column_name: "col".to_string(),
            physical_type: physical_type.to_string(),
            logical_type: logical_type.map(str::to_string),
            sample_rows: 200_000,
            total_file_rows: 1_000_000,
            sample_fraction: 0.2,
            cardinality_estimate: 10,
            cardinality_ratio: 0.00005,
            cardinality_method: "exact".to_string(),
            monotonicity_score: Some(0.0),
            run_length_score: 0.0,
            string_length_stats: None,
            uuid_pattern_detected: false,
            json_pattern_detected: false,
            byte_entropy: None,
            null_count_in_sample: 0,
            null_fraction: 0.0,
        };
        let meta = ColumnMetaSummary {
            name: "col".to_string(),
            physical_type: physical_type.to_string(),
            logical_type: logical_type.map(str::to_string),
            encodings: vec!["PLAIN".to_string()],
            codec: "SNAPPY".to_string(),
            compressed_bytes: 1000,
            uncompressed_bytes: 2000,
            compression_ratio: 2.0,
            total_null_count: Some(0),
            min_value: None,
            max_value: None,
            statistics_available: true,
            per_row_group_encodings: vec![vec!["PLAIN".to_string()]],
            per_row_group_compressed_bytes: vec![1000],
            per_row_group_uncompressed_bytes: vec![2000],
            per_row_group_dict_page_bytes: vec![None],
        };
        (profile, meta)
    }

    #[test]
    fn test_rule1_boolean_fires() {
        let (profile, meta) = make_profile("BOOLEAN", None);
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::BooleanRle);
        assert_eq!(rec.encoding, "RLE");
    }

    #[test]
    fn test_rule2_delta_fires() {
        let (mut profile, meta) = make_profile("INT64", None);
        profile.monotonicity_score = Some(0.95);
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::DeltaMonotonic);
        assert_eq!(rec.encoding, "DELTA_BINARY_PACKED");
    }

    #[test]
    fn test_rule2_delta_no_fire_low_monotonicity() {
        let (mut profile, meta) = make_profile("INT64", None);
        profile.monotonicity_score = Some(0.5);
        let rec = recommend_encoding(&profile, &meta);
        assert_ne!(rec.rule_fired, RuleName::DeltaMonotonic);
    }

    #[test]
    fn test_rule3_rle_dict_fires() {
        let (mut profile, mut meta) = make_profile("BYTE_ARRAY", None);
        profile.cardinality_ratio = 0.001;
        profile.cardinality_estimate = 100;
        // dict_size = 100 * 8.0 = 800 < 524288
        meta.physical_type = "BYTE_ARRAY".to_string();
        profile.string_length_stats = None;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::RleDictionary);
    }

    #[test]
    fn test_rule3_no_fire_high_cardinality() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.cardinality_ratio = 0.8;
        let rec = recommend_encoding(&profile, &meta);
        assert_ne!(rec.rule_fired, RuleName::RleDictionary);
    }

    #[test]
    fn test_rule4_byte_stream_split() {
        let (mut profile, meta) = make_profile("DOUBLE", None);
        profile.cardinality_ratio = 0.9;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::ByteStreamSplit);
        assert_eq!(rec.encoding, "BYTE_STREAM_SPLIT");
    }

    #[test]
    fn test_rule5_plain_uuid() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.uuid_pattern_detected = true;
        profile.cardinality_ratio = 0.8;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::PlainUuid);
        assert_eq!(rec.encoding, "PLAIN");
    }

    #[test]
    fn test_rule6_plain_default() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.cardinality_ratio = 0.5;
        profile.uuid_pattern_detected = false;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::PlainDefault);
        assert_eq!(rec.encoding, "PLAIN");
    }

    #[test]
    fn test_rule_priority_delta_beats_dict() {
        let (mut profile, meta) = make_profile("INT64", None);
        profile.monotonicity_score = Some(0.95);
        profile.cardinality_ratio = 0.001;
        profile.cardinality_estimate = 100;
        let rec = recommend_encoding(&profile, &meta);
        // Rule 2 (DeltaMonotonic) should fire before Rule 3 (RleDictionary)
        assert_eq!(rec.rule_fired, RuleName::DeltaMonotonic);
        assert_eq!(rec.encoding, "DELTA_BINARY_PACKED");
    }

    #[test]
    fn test_confidence_high() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.sample_fraction = 0.15;
        profile.sample_rows = 200_000;
        profile.cardinality_ratio = 0.5;
        profile.uuid_pattern_detected = false;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.confidence, ConfidenceTier::High);
    }

    #[test]
    fn test_confidence_medium_low_fraction() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.sample_fraction = 0.01;
        profile.sample_rows = 200_000;
        profile.cardinality_ratio = 0.5;
        profile.uuid_pattern_detected = false;
        let rec = recommend_encoding(&profile, &meta);
        // sample_rows=200_000 >= 50_000 → medium
        assert_eq!(rec.confidence, ConfidenceTier::Medium);
    }

    #[test]
    fn test_confidence_low() {
        let (mut profile, meta) = make_profile("BYTE_ARRAY", None);
        profile.sample_fraction = 0.01;
        profile.sample_rows = 10_000;
        profile.cardinality_ratio = 0.5;
        profile.uuid_pattern_detected = false;
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.confidence, ConfidenceTier::Low);
    }

    #[test]
    fn test_confidence_rle_dict_boundary() {
        // cardinality_ratio=0.09 is in [0.08, 0.12] → boundary downgrade
        // Profile also has sample_fraction=0.15, sample_rows=200_000 (would be High otherwise)
        let (mut profile, mut meta) = make_profile("INT32", None);
        profile.sample_fraction = 0.15;
        profile.sample_rows = 200_000;
        profile.cardinality_ratio = 0.09;
        profile.cardinality_estimate = 18_000; // 18000 * 4.0 = 72000 < 524288
        meta.physical_type = "INT32".to_string();
        let rec = recommend_encoding(&profile, &meta);
        assert_eq!(rec.rule_fired, RuleName::RleDictionary);
        assert_eq!(rec.confidence, ConfidenceTier::Medium);
    }

    #[test]
    fn test_rule3_dict_size_overflow_skips() {
        // cardinality_ratio < 0.10 but dict_size >= 524288 → should NOT fire RleDictionary
        let (mut profile, mut meta) = make_profile("BYTE_ARRAY", None);
        profile.cardinality_ratio = 0.05;
        // mean_len = 100 bytes, cardinality_estimate = 6000 → dict_size = 600000 > 524288
        profile.cardinality_estimate = 6000;
        profile.string_length_stats = Some(StringLengthStats {
            min_len: 90,
            max_len: 110,
            mean_len: 100.0,
            stddev_len: 5.0,
        });
        meta.physical_type = "BYTE_ARRAY".to_string();
        let rec = recommend_encoding(&profile, &meta);
        assert_ne!(rec.rule_fired, RuleName::RleDictionary);
    }
}
