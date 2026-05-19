use serde::{Serialize, Deserialize};
use crate::profiler::stats::ColumnProfile;
use super::encoding::EncodingRecommendation;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    Size,
    Speed,
    Balanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Engine {
    Spark,
    DuckDB,
    Polars,
    ClickHouse,
    Pandas,
    Unknown,
}

impl Engine {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "spark" => Engine::Spark,
            "duckdb" => Engine::DuckDB,
            "polars" => Engine::Polars,
            "clickhouse" => Engine::ClickHouse,
            "pandas" => Engine::Pandas,
            _ => Engine::Unknown,
        }
    }
}

impl Priority {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "size" => Priority::Size,
            "speed" => Priority::Speed,
            _ => Priority::Balanced,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CaveatSeverity {
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caveat {
    pub severity: CaveatSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecRecommendation {
    pub codec: String,
    pub codec_level: Option<i32>,
    pub reason_brief: String,
    pub caveats: Vec<Caveat>,
}

pub fn recommend_codec(
    profile: &ColumnProfile,
    encoding_rec: &EncodingRecommendation,
    priority: &Priority,
    engine: &Engine,
) -> CodecRecommendation {
    // 1. Entropy gate
    if let Some(entropy) = profile.byte_entropy {
        if entropy > 7.5 {
            return CodecRecommendation {
                codec: "UNCOMPRESSED".to_string(),
                codec_level: None,
                reason_brief: format!(
                    "byte entropy {:.2} > 7.5 — data appears pre-compressed or random",
                    entropy
                ),
                caveats: vec![Caveat {
                    severity: CaveatSeverity::Info,
                    message: "Compressing high-entropy data adds CPU overhead without reducing size"
                        .to_string(),
                }],
            };
        }
    }

    // 2. Spark safety (when not size priority)
    if matches!(engine, Engine::Spark) && !matches!(priority, Priority::Size) {
        return CodecRecommendation {
            codec: "SNAPPY".to_string(),
            codec_level: None,
            reason_brief: "SNAPPY is safe for all Spark versions".to_string(),
            caveats: vec![Caveat {
                severity: CaveatSeverity::Info,
                message: "ZSTD requires Spark 3.2+; SNAPPY works on all versions. Use --engine spark (with version) to unlock ZSTD.".to_string(),
            }],
        };
    }

    // 3. Speed priority → LZ4
    if matches!(priority, Priority::Speed) {
        let mut caveats = Vec::new();
        if encoding_rec.encoding == "DELTA_BINARY_PACKED" {
            caveats.push(Caveat {
                severity: CaveatSeverity::Warning,
                message: "Known bug in parquet-go: LZ4 + DELTA_BINARY_PACKED produces unreadable files. Use SNAPPY if your reader uses the parquet-go library.".to_string(),
            });
        }
        return CodecRecommendation {
            codec: "LZ4".to_string(),
            codec_level: None,
            reason_brief: "LZ4 has fastest decompression speed".to_string(),
            caveats,
        };
    }

    // 4. Size priority → ZSTD:6
    if matches!(priority, Priority::Size) {
        let mut caveats = Vec::new();
        if matches!(engine, Engine::Spark) {
            caveats.push(Caveat {
                severity: CaveatSeverity::Info,
                message: "ZSTD requires Spark 3.2+".to_string(),
            });
        }
        return CodecRecommendation {
            codec: "ZSTD".to_string(),
            codec_level: Some(6),
            reason_brief: "ZSTD:6 maximizes compression ratio".to_string(),
            caveats,
        };
    }

    // 5. Default → ZSTD:3
    let mut caveats = Vec::new();
    if matches!(engine, Engine::Spark) {
        caveats.push(Caveat {
            severity: CaveatSeverity::Info,
            message: "ZSTD requires Spark 3.2+; use --engine spark with Spark 3.2+ or switch --priority to use SNAPPY".to_string(),
        });
    }
    CodecRecommendation {
        codec: "ZSTD".to_string(),
        codec_level: Some(3),
        reason_brief: "ZSTD:3 balances compression ratio and read speed".to_string(),
        caveats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::stats::ColumnProfile;
    use crate::recommender::encoding::{EncodingRecommendation, RuleName, ConfidenceTier};

    fn make_profile() -> ColumnProfile {
        ColumnProfile {
            column_name: "col".to_string(),
            physical_type: "INT64".to_string(),
            logical_type: None,
            sample_rows: 200_000,
            total_file_rows: 1_000_000,
            sample_fraction: 0.2,
            cardinality_estimate: 10,
            cardinality_ratio: 0.5,
            cardinality_method: "exact".to_string(),
            monotonicity_score: None,
            run_length_score: 0.0,
            string_length_stats: None,
            uuid_pattern_detected: false,
            json_pattern_detected: false,
            byte_entropy: None,
            null_count_in_sample: 0,
            null_fraction: 0.0,
        }
    }

    fn make_encoding_rec(encoding: &str) -> EncodingRecommendation {
        EncodingRecommendation {
            encoding: encoding.to_string(),
            rule_fired: RuleName::PlainDefault,
            reason_brief: "test".to_string(),
            confidence: ConfidenceTier::High,
            confidence_reason: "test".to_string(),
        }
    }

    #[test]
    fn test_entropy_gate() {
        let mut profile = make_profile();
        profile.byte_entropy = Some(7.8);
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Balanced, &Engine::Unknown);
        assert_eq!(rec.codec, "UNCOMPRESSED");
        assert_eq!(rec.codec_level, None);
        assert_eq!(rec.caveats.len(), 1);
        assert_eq!(rec.caveats[0].severity, CaveatSeverity::Info);
    }

    #[test]
    fn test_spark_safety() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Balanced, &Engine::Spark);
        assert_eq!(rec.codec, "SNAPPY");
        assert_eq!(rec.codec_level, None);
        assert_eq!(rec.caveats.len(), 1);
        assert_eq!(rec.caveats[0].severity, CaveatSeverity::Info);
    }

    #[test]
    fn test_spark_size_override() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Size, &Engine::Spark);
        assert_eq!(rec.codec, "ZSTD");
        assert_eq!(rec.codec_level, Some(6));
    }

    #[test]
    fn test_speed_lz4() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Speed, &Engine::Unknown);
        assert_eq!(rec.codec, "LZ4");
        assert_eq!(rec.codec_level, None);
        assert!(rec.caveats.is_empty());
    }

    #[test]
    fn test_speed_lz4_delta_warning() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("DELTA_BINARY_PACKED");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Speed, &Engine::Unknown);
        assert_eq!(rec.codec, "LZ4");
        assert_eq!(rec.caveats.len(), 1);
        assert_eq!(rec.caveats[0].severity, CaveatSeverity::Warning);
    }

    #[test]
    fn test_balanced_default() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Balanced, &Engine::Unknown);
        assert_eq!(rec.codec, "ZSTD");
        assert_eq!(rec.codec_level, Some(3));
        assert!(rec.caveats.is_empty());
    }

    #[test]
    fn test_size_zstd6() {
        let profile = make_profile();
        let enc_rec = make_encoding_rec("PLAIN");
        let rec = recommend_codec(&profile, &enc_rec, &Priority::Size, &Engine::Unknown);
        assert_eq!(rec.codec, "ZSTD");
        assert_eq!(rec.codec_level, Some(6));
        assert!(rec.caveats.is_empty());
    }
}
