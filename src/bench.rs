use std::io::Cursor;
use std::path::Path;

use bytes::Bytes;
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use arrow::array::RecordBatch;
use arrow::datatypes::{Field, Schema};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::{Compression, Encoding};
use parquet::file::properties::WriterProperties;
use parquet::schema::types::ColumnPath;
use serde::Serialize;

use crate::error::AutoparqError;
use crate::profiler::sampler::sample_column;

#[derive(Debug, Clone, Serialize)]
pub struct BenchEntry {
    pub encoding: String,
    pub codec: String,
    pub codec_level: Option<i32>,
    pub compressed_bytes: usize,
    pub write_ms: u64,
    pub read_ms: u64,
    pub compression_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchResult {
    pub column_name: String,
    pub physical_type: String,
    pub uncompressed_bytes: usize,
    pub entries: Vec<BenchEntry>, // sorted by compressed_bytes ascending
}

fn parse_encoding(s: &str) -> Result<Encoding, AutoparqError> {
    match s.to_uppercase().as_str() {
        "PLAIN" => Ok(Encoding::PLAIN),
        "DELTA_BINARY_PACKED" => Ok(Encoding::DELTA_BINARY_PACKED),
        "RLE_DICTIONARY" | "DICT" => Ok(Encoding::RLE_DICTIONARY),
        "BYTE_STREAM_SPLIT" => Ok(Encoding::BYTE_STREAM_SPLIT),
        "RLE" => Ok(Encoding::RLE),
        other => Err(AutoparqError::UnsupportedType(format!(
            "Unknown encoding: {}",
            other
        ))),
    }
}

fn parse_compression(codec: &str, level: Option<i32>) -> Result<Compression, AutoparqError> {
    match codec.to_uppercase().as_str() {
        "SNAPPY" => Ok(Compression::SNAPPY),
        "LZ4" => Ok(Compression::LZ4),
        "UNCOMPRESSED" => Ok(Compression::UNCOMPRESSED),
        "ZSTD" => {
            use parquet::basic::ZstdLevel;
            let l = level.unwrap_or(3);
            Ok(Compression::ZSTD(
                ZstdLevel::try_new(l).map_err(|e| AutoparqError::UnsupportedType(e.to_string()))?,
            ))
        }
        "GZIP" => {
            use parquet::basic::GzipLevel;
            Ok(Compression::GZIP(GzipLevel::default()))
        }
        other => Err(AutoparqError::UnsupportedType(format!(
            "Unknown codec: {}",
            other
        ))),
    }
}

pub fn valid_encodings_for_type(physical_type: &str) -> Vec<String> {
    match physical_type.to_uppercase().as_str() {
        "INT32" | "INT64" => vec![
            "PLAIN".into(),
            "DELTA_BINARY_PACKED".into(),
            "RLE_DICTIONARY".into(),
        ],
        "BYTE_ARRAY" => vec!["PLAIN".into(), "RLE_DICTIONARY".into()],
        "FLOAT" | "DOUBLE" => vec!["PLAIN".into(), "BYTE_STREAM_SPLIT".into()],
        "BOOLEAN" => vec!["PLAIN".into()],
        _ => vec!["PLAIN".into()],
    }
}

pub fn default_codecs() -> Vec<(String, Option<i32>)> {
    vec![
        ("SNAPPY".into(), None),
        ("ZSTD".into(), Some(1)),
        ("ZSTD".into(), Some(3)),
        ("ZSTD".into(), Some(6)),
        ("LZ4".into(), None),
        ("UNCOMPRESSED".into(), None),
    ]
}

pub fn benchmark_column(
    path: &Path,
    column_name: &str,
    codecs: &[(String, Option<i32>)],
    encodings: &[String],
) -> Result<BenchResult, AutoparqError> {
    let sample = sample_column(path, column_name, 0, 500_000)?;

    let schema = std::sync::Arc::new(Schema::new(vec![Field::new(
        column_name,
        sample.array.data_type().clone(),
        true,
    )]));

    // Compute uncompressed baseline: write with PLAIN + UNCOMPRESSED
    let uncompressed_bytes = {
        let props = WriterProperties::builder()
            .set_column_encoding(
                ColumnPath::from(column_name),
                Encoding::PLAIN,
            )
            .set_compression(Compression::UNCOMPRESSED)
            .build();
        let mut buf: Vec<u8> = Vec::new();
        let batch = RecordBatch::try_new(schema.clone(), vec![sample.array.clone()])?;
        let mut writer = ArrowWriter::try_new(Cursor::new(&mut buf), schema.clone(), Some(props))?;
        writer.write(&batch)?;
        writer.close()?;
        buf.len()
    };

    let mut entries: Vec<BenchEntry> = Vec::new();

    for encoding_str in encodings {
        let encoding = parse_encoding(encoding_str)?;

        for (codec_str, codec_level) in codecs {
            let compression = parse_compression(codec_str, *codec_level)?;

            let col_path = ColumnPath::from(column_name);
            let mut builder = WriterProperties::builder().set_compression(compression);
            if encoding == Encoding::RLE_DICTIONARY {
                // RLE_DICTIONARY must be enabled via dictionary, not as a column encoding
                builder = builder.set_column_dictionary_enabled(col_path, true);
            } else {
                builder = builder
                    .set_column_dictionary_enabled(col_path.clone(), false)
                    .set_column_encoding(col_path, encoding);
            }
            let props = builder.build();

            let batch = RecordBatch::try_new(schema.clone(), vec![sample.array.clone()])?;

            // Write
            let write_start = Instant::now();
            let mut buf: Vec<u8> = Vec::new();
            let mut writer =
                ArrowWriter::try_new(Cursor::new(&mut buf), schema.clone(), Some(props))?;
            writer.write(&batch)?;
            writer.close()?;
            let write_ms = write_start.elapsed().as_millis() as u64;

            let compressed_bytes = buf.len();

            // Read back — Bytes implements ChunkReader; Cursor<Vec<u8>> does not
            let read_start = Instant::now();
            let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(buf))?.build()?;
            for batch_result in reader {
                let _ = batch_result?;
            }
            let read_ms = read_start.elapsed().as_millis() as u64;

            let compression_ratio = if compressed_bytes > 0 {
                uncompressed_bytes as f64 / compressed_bytes as f64
            } else {
                1.0
            };

            entries.push(BenchEntry {
                encoding: encoding_str.clone(),
                codec: codec_str.clone(),
                codec_level: *codec_level,
                compressed_bytes,
                write_ms,
                read_ms,
                compression_ratio,
            });
        }
    }

    entries.sort_by_key(|e| e.compressed_bytes);

    Ok(BenchResult {
        column_name: column_name.to_string(),
        physical_type: sample.physical_type,
        uncompressed_bytes,
        entries,
    })
}

/// Benchmark a column from raw Parquet bytes (no disk I/O — WASM-compatible).
/// Uses default_codecs() and valid_encodings_for_type() for the column's physical type.
pub fn benchmark_column_from_bytes(
    data: &[u8],
    column_name: &str,
) -> Result<BenchResult, AutoparqError> {
    let bytes = Bytes::copy_from_slice(data);
    let sample = crate::profiler::sampler::sample_column_from_bytes(bytes, column_name, 0, 500_000)?;

    let encodings = valid_encodings_for_type(&sample.physical_type);
    let codecs = default_codecs();

    let schema = std::sync::Arc::new(Schema::new(vec![Field::new(
        column_name,
        sample.array.data_type().clone(),
        true,
    )]));

    let uncompressed_bytes = {
        let props = WriterProperties::builder()
            .set_column_encoding(ColumnPath::from(column_name), Encoding::PLAIN)
            .set_compression(Compression::UNCOMPRESSED)
            .build();
        let mut buf: Vec<u8> = Vec::new();
        let batch = RecordBatch::try_new(schema.clone(), vec![sample.array.clone()])?;
        let mut writer = ArrowWriter::try_new(Cursor::new(&mut buf), schema.clone(), Some(props))?;
        writer.write(&batch)?;
        writer.close()?;
        buf.len()
    };

    let mut entries: Vec<BenchEntry> = Vec::new();

    for encoding_str in &encodings {
        let encoding = parse_encoding(encoding_str)?;

        for (codec_str, codec_level) in &codecs {
            let compression = parse_compression(codec_str, *codec_level)?;

            let col_path = ColumnPath::from(column_name);
            let mut builder = WriterProperties::builder().set_compression(compression);
            if encoding == Encoding::RLE_DICTIONARY {
                builder = builder.set_column_dictionary_enabled(col_path, true);
            } else {
                builder = builder
                    .set_column_dictionary_enabled(col_path.clone(), false)
                    .set_column_encoding(col_path, encoding);
            }
            let props = builder.build();

            let batch = RecordBatch::try_new(schema.clone(), vec![sample.array.clone()])?;

            let write_start = Instant::now();
            let mut buf: Vec<u8> = Vec::new();
            let mut writer =
                ArrowWriter::try_new(Cursor::new(&mut buf), schema.clone(), Some(props))?;
            writer.write(&batch)?;
            writer.close()?;
            let write_ms = write_start.elapsed().as_millis() as u64;

            let compressed_bytes = buf.len();

            let read_start = Instant::now();
            let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::from(buf))?.build()?;
            for batch_result in reader {
                let _ = batch_result?;
            }
            let read_ms = read_start.elapsed().as_millis() as u64;

            let compression_ratio = if compressed_bytes > 0 {
                uncompressed_bytes as f64 / compressed_bytes as f64
            } else {
                1.0
            };

            entries.push(BenchEntry {
                encoding: encoding_str.clone(),
                codec: codec_str.clone(),
                codec_level: *codec_level,
                compressed_bytes,
                write_ms,
                read_ms,
                compression_ratio,
            });
        }
    }

    entries.sort_by_key(|e| e.compressed_bytes);

    Ok(BenchResult {
        column_name: column_name.to_string(),
        physical_type: sample.physical_type,
        uncompressed_bytes,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_encoding_valid() {
        assert!(parse_encoding("PLAIN").is_ok());
        assert!(parse_encoding("DELTA_BINARY_PACKED").is_ok());
        assert!(parse_encoding("RLE_DICTIONARY").is_ok());
        assert!(parse_encoding("DICT").is_ok());
        assert!(parse_encoding("BYTE_STREAM_SPLIT").is_ok());
        assert!(parse_encoding("RLE").is_ok());
    }

    #[test]
    fn test_parse_encoding_invalid() {
        assert!(parse_encoding("BOGUS_ENCODING").is_err());
    }

    #[test]
    fn test_parse_compression_valid() {
        assert!(parse_compression("SNAPPY", None).is_ok());
        assert!(parse_compression("LZ4", None).is_ok());
        assert!(parse_compression("UNCOMPRESSED", None).is_ok());
        assert!(parse_compression("ZSTD", Some(3)).is_ok());
        assert!(parse_compression("GZIP", None).is_ok());
    }

    #[test]
    fn test_parse_compression_invalid() {
        assert!(parse_compression("BOGUS_CODEC", None).is_err());
    }

    #[test]
    fn test_valid_encodings_for_type() {
        let int_encs = valid_encodings_for_type("INT32");
        assert!(int_encs.contains(&"DELTA_BINARY_PACKED".to_string()));

        let float_encs = valid_encodings_for_type("FLOAT");
        assert!(float_encs.contains(&"BYTE_STREAM_SPLIT".to_string()));

        let bool_encs = valid_encodings_for_type("BOOLEAN");
        assert_eq!(bool_encs, vec!["PLAIN".to_string()]);
    }

    #[test]
    fn test_default_codecs_count() {
        assert_eq!(default_codecs().len(), 6);
    }
}
