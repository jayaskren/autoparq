#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use serde::Serialize;
use bytes::Bytes;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::{Compression, Encoding};
use parquet::file::properties::WriterProperties;
use parquet::schema::types::ColumnPath;
#[cfg(not(target_arch = "wasm32"))]
use tempfile::NamedTempFile;

use crate::error::AutoparqError;
use crate::recommender::codec::{Engine, Priority};
#[cfg(not(target_arch = "wasm32"))]
use crate::tuner::build_tune_report;

#[derive(Debug, Clone, Serialize)]
pub struct RewriteResult {
    pub rows_written: i64,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    pub actual_reduction_pct: f64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnSizeDelta {
    pub column_name: String,
    pub before_compressed: i64,
    pub after_compressed: i64,
    pub before_encodings: Vec<String>,
    pub after_encodings: Vec<String>,
    pub before_codec: String,
    pub after_codec: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RewriteSummaryJson {
    pub rewrite: RewriteResult,
    pub per_column_diff: Vec<ColumnSizeDelta>,
}

pub struct RewriteResultWithOutput {
    pub output_bytes: Vec<u8>,
    pub summary: RewriteSummaryJson,
}

fn parse_encoding_for_writer(s: &str) -> Result<Encoding, AutoparqError> {
    match s.to_uppercase().as_str() {
        "PLAIN" => Ok(Encoding::PLAIN),
        "DELTA_BINARY_PACKED" => Ok(Encoding::DELTA_BINARY_PACKED),
        "RLE_DICTIONARY" => Ok(Encoding::RLE_DICTIONARY),
        "BYTE_STREAM_SPLIT" => Ok(Encoding::BYTE_STREAM_SPLIT),
        "RLE" => Ok(Encoding::RLE),
        _ => Ok(Encoding::PLAIN), // fallback
    }
}

fn parse_compression_for_writer(codec: &str, level: Option<i32>) -> Result<Compression, AutoparqError> {
    match codec.to_uppercase().as_str() {
        "SNAPPY" => Ok(Compression::SNAPPY),
        "LZ4" => Ok(Compression::LZ4),
        "UNCOMPRESSED" => Ok(Compression::UNCOMPRESSED),
        "ZSTD" => {
            use parquet::basic::ZstdLevel;
            let l = level.unwrap_or(3);
            Ok(Compression::ZSTD(ZstdLevel::try_new(l).map_err(|e| AutoparqError::UnsupportedType(e.to_string()))?))
        }
        "GZIP" => Ok(Compression::GZIP(parquet::basic::GzipLevel::default())),
        _ => Ok(Compression::SNAPPY), // fallback
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn rewrite_file(
    input_path: &Path,
    output_path: &Path,
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
) -> Result<RewriteResult, AutoparqError> {
    let start = Instant::now();

    let input_size = std::fs::metadata(input_path)?.len();

    let tune_report = build_tune_report(input_path, &engine, &priority, sample_rows, "brief")?;

    let mut builder = WriterProperties::builder();
    for col_rec in &tune_report.columns {
        let col_path = ColumnPath::from(col_rec.column_name.as_str());
        let encoding = parse_encoding_for_writer(&col_rec.recommended_encoding)?;
        let compression = parse_compression_for_writer(
            &col_rec.recommended_codec,
            col_rec.recommended_codec_level,
        )?;
        if encoding == Encoding::RLE_DICTIONARY {
            builder = builder.set_column_dictionary_enabled(col_path.clone(), true);
        } else {
            builder = builder
                .set_column_dictionary_enabled(col_path.clone(), false)
                .set_column_encoding(col_path.clone(), encoding);
        }
        builder = builder.set_column_compression(col_path, compression);
    }
    let props = builder.build();

    let input_file = std::fs::File::open(input_path)?;
    let reader_builder = ParquetRecordBatchReaderBuilder::try_new(input_file)?;
    let schema = reader_builder.schema().clone();
    let reader = reader_builder.build()?;

    let parent_dir = output_path.parent().unwrap_or(Path::new("."));
    let mut temp_file = NamedTempFile::new_in(parent_dir)?;

    let mut writer = ArrowWriter::try_new(&mut temp_file, schema, Some(props))?;

    let mut rows_written: i64 = 0;
    for batch in reader {
        let batch = batch?;
        rows_written += batch.num_rows() as i64;
        writer.write(&batch)?;
    }

    writer.close()?;

    temp_file
        .persist(output_path)
        .map_err(|e| AutoparqError::IoError(e.error))?;

    let output_size = std::fs::metadata(output_path)?.len();
    let actual_reduction_pct = if input_size > 0 {
        (1.0 - output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(RewriteResult {
        rows_written,
        input_size_bytes: input_size,
        output_size_bytes: output_size,
        actual_reduction_pct,
        elapsed_ms,
    })
}

/// Rewrite a parquet file in-memory (WASM-safe). Returns output bytes plus a
/// per-column before/after delta computed by re-parsing the output footer.
pub fn rewrite_file_from_bytes(
    data: &[u8],
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
) -> Result<RewriteResultWithOutput, AutoparqError> {
    use crate::profiler::metadata::read_file_metadata_from_bytes;
    use crate::tuner::build_tune_report_from_bytes;

    let start = Instant::now();
    let input_size = data.len() as u64;

    // Parse "before" profile for the delta. Errors here mean the input is not valid Parquet.
    let before_profile = read_file_metadata_from_bytes(data)?;

    // Profile + recommend against the input.
    let tune_report =
        build_tune_report_from_bytes(data, &engine, &priority, sample_rows, "brief")?;

    let mut builder = WriterProperties::builder();
    for col_rec in &tune_report.columns {
        let col_path = ColumnPath::from(col_rec.column_name.as_str());
        let encoding = parse_encoding_for_writer(&col_rec.recommended_encoding)?;
        let compression = parse_compression_for_writer(
            &col_rec.recommended_codec,
            col_rec.recommended_codec_level,
        )?;
        if encoding == Encoding::RLE_DICTIONARY {
            builder = builder.set_column_dictionary_enabled(col_path.clone(), true);
        } else {
            builder = builder
                .set_column_dictionary_enabled(col_path.clone(), false)
                .set_column_encoding(col_path.clone(), encoding);
        }
        builder = builder.set_column_compression(col_path, compression);
    }
    let props = builder.build();

    // Reader from in-memory bytes.
    let reader_bytes = Bytes::copy_from_slice(data);
    let reader_builder = ParquetRecordBatchReaderBuilder::try_new(reader_bytes)?;
    let schema = reader_builder.schema().clone();
    let reader = reader_builder.build()?;

    // Writer into an owned Vec<u8>.
    let mut output_buf: Vec<u8> = Vec::with_capacity(data.len());
    let mut writer = ArrowWriter::try_new(&mut output_buf, schema, Some(props))?;

    let mut rows_written: i64 = 0;
    for batch in reader {
        let batch = batch?;
        rows_written += batch.num_rows() as i64;
        writer.write(&batch)?;
    }
    writer.close()?;

    let output_size = output_buf.len() as u64;
    let actual_reduction_pct = if input_size > 0 {
        (1.0 - output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    // Parse "after" profile from the freshly written bytes.
    let after_profile = read_file_metadata_from_bytes(&output_buf)?;

    // Zip columns by name to build the per-column delta.
    let mut per_column_diff: Vec<ColumnSizeDelta> = Vec::with_capacity(before_profile.columns.len());
    for before in &before_profile.columns {
        let after = after_profile.columns.iter().find(|c| c.name == before.name);
        if let Some(after) = after {
            per_column_diff.push(ColumnSizeDelta {
                column_name: before.name.clone(),
                before_compressed: before.compressed_bytes,
                after_compressed: after.compressed_bytes,
                before_encodings: before.encodings.clone(),
                after_encodings: after.encodings.clone(),
                before_codec: before.codec.clone(),
                after_codec: after.codec.clone(),
            });
        }
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(RewriteResultWithOutput {
        output_bytes: output_buf,
        summary: RewriteSummaryJson {
            rewrite: RewriteResult {
                rows_written,
                input_size_bytes: input_size,
                output_size_bytes: output_size,
                actual_reduction_pct,
                elapsed_ms,
            },
            per_column_diff,
        },
    })
}
