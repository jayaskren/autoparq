use serde::Serialize;
use super::codec::{Engine, CodecRecommendation, Caveat, CaveatSeverity};

#[derive(Debug, Clone, Serialize)]
pub struct EngineSupport {
    pub supported: bool,
    pub min_version: Option<String>,
    pub notes: Option<String>,
}

pub fn check_codec_compatibility(engine: &Engine, codec: &str) -> EngineSupport {
    match engine {
        Engine::Spark => match codec {
            "SNAPPY" => EngineSupport {
                supported: true,
                min_version: None,
                notes: Some("Supported in all Spark versions".to_string()),
            },
            "ZSTD" => EngineSupport {
                supported: true,
                min_version: Some("3.2.0".to_string()),
                notes: Some("ZSTD requires Spark 3.2+".to_string()),
            },
            "LZ4" => EngineSupport {
                supported: true,
                min_version: Some("3.3.0".to_string()),
                notes: Some("LZ4 requires Spark 3.3+".to_string()),
            },
            "BROTLI" => EngineSupport {
                supported: true,
                min_version: Some("3.3.0".to_string()),
                notes: Some("BROTLI requires Spark 3.3+".to_string()),
            },
            "GZIP" => EngineSupport {
                supported: true,
                min_version: None,
                notes: Some("Supported in all Spark versions".to_string()),
            },
            "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}' for Spark", codec)),
            },
        },
        Engine::DuckDB => match codec {
            "SNAPPY" | "ZSTD" | "LZ4" | "BROTLI" | "GZIP" | "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}' for DuckDB", codec)),
            },
        },
        Engine::ClickHouse => match codec {
            "SNAPPY" | "ZSTD" | "LZ4" | "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            "BROTLI" => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some("ClickHouse does not support BROTLI for Parquet import".to_string()),
            },
            "GZIP" => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some("ClickHouse does not support GZIP for Parquet import".to_string()),
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}' for ClickHouse", codec)),
            },
        },
        Engine::Polars => match codec {
            "SNAPPY" | "ZSTD" | "LZ4" | "BROTLI" | "GZIP" | "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}' for Polars", codec)),
            },
        },
        Engine::Pandas => match codec {
            "SNAPPY" | "ZSTD" | "LZ4" | "BROTLI" | "GZIP" | "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}' for Pandas", codec)),
            },
        },
        Engine::Unknown => match codec {
            "SNAPPY" | "ZSTD" | "LZ4" | "GZIP" | "UNCOMPRESSED" => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
            "BROTLI" => EngineSupport {
                supported: true,
                min_version: None,
                notes: Some("BROTLI is broadly supported but verify your specific engine".to_string()),
            },
            _ => EngineSupport {
                supported: false,
                min_version: None,
                notes: Some(format!("Unknown codec '{}'", codec)),
            },
        },
    }
}

pub fn check_encoding_compatibility(engine: &Engine, encoding: &str) -> EngineSupport {
    match encoding {
        "DELTA_BINARY_PACKED" => match engine {
            Engine::Spark => EngineSupport {
                supported: true,
                min_version: Some("3.2.0".to_string()),
                notes: Some("DELTA_BINARY_PACKED requires Spark 3.2+".to_string()),
            },
            _ => EngineSupport {
                supported: true,
                min_version: None,
                notes: None,
            },
        },
        "BYTE_STREAM_SPLIT" | "RLE_DICTIONARY" | "PLAIN" | "RLE" => EngineSupport {
            supported: true,
            min_version: None,
            notes: None,
        },
        _ => EngineSupport {
            supported: true,
            min_version: None,
            notes: Some(format!("Encoding '{}' compatibility not explicitly tracked", encoding)),
        },
    }
}

pub fn apply_engine_overrides(rec: &mut CodecRecommendation, engine: &Engine) {
    match engine {
        Engine::Spark => {
            if rec.codec == "ZSTD" {
                rec.codec = "SNAPPY".to_string();
                rec.codec_level = None;
                rec.caveats.push(Caveat {
                    severity: CaveatSeverity::Info,
                    message: "Downgraded ZSTD→SNAPPY for Spark compatibility. Use --engine spark:3.2+ to keep ZSTD.".to_string(),
                });
            }
        }
        Engine::ClickHouse => {
            if rec.codec == "BROTLI" {
                rec.codec = "ZSTD".to_string();
                rec.codec_level = Some(3);
                rec.caveats.push(Caveat {
                    severity: CaveatSeverity::Warning,
                    message: "ClickHouse does not support BROTLI for Parquet; using ZSTD".to_string(),
                });
            } else if rec.codec == "GZIP" {
                rec.caveats.push(Caveat {
                    severity: CaveatSeverity::Warning,
                    message: "ClickHouse does not support GZIP for Parquet import".to_string(),
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_codec_rec(codec: &str, level: Option<i32>) -> CodecRecommendation {
        CodecRecommendation {
            codec: codec.to_string(),
            codec_level: level,
            reason_brief: "test".to_string(),
            caveats: vec![],
        }
    }

    #[test]
    fn test_spark_zstd_compat() {
        let support = check_codec_compatibility(&Engine::Spark, "ZSTD");
        assert!(support.supported);
        assert_eq!(support.min_version, Some("3.2.0".to_string()));
    }

    #[test]
    fn test_clickhouse_brotli_unsupported() {
        let support = check_codec_compatibility(&Engine::ClickHouse, "BROTLI");
        assert!(!support.supported);
    }

    #[test]
    fn test_apply_overrides_spark_zstd() {
        let mut rec = make_codec_rec("ZSTD", Some(3));
        apply_engine_overrides(&mut rec, &Engine::Spark);
        assert_eq!(rec.codec, "SNAPPY");
        assert!(rec.caveats.len() > 0);
    }

    #[test]
    fn test_delta_spark_min_version() {
        let support = check_encoding_compatibility(&Engine::Spark, "DELTA_BINARY_PACKED");
        assert_eq!(support.min_version, Some("3.2.0".to_string()));
    }
}
