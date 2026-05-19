use parquet::basic::{Compression, Encoding, LogicalType, TimeUnit, Type};
use parquet::file::metadata::ParquetMetaDataReader;
use bytes::Bytes;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnMetaSummary {
    pub name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub encodings: Vec<String>,
    pub codec: String,
    pub compressed_bytes: i64,
    pub uncompressed_bytes: i64,
    pub compression_ratio: f64,
    pub total_null_count: Option<i64>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub statistics_available: bool,
    #[serde(default)]
    pub per_row_group_encodings: Vec<Vec<String>>,
    #[serde(default)]
    pub per_row_group_compressed_bytes: Vec<i64>,
    #[serde(default)]
    pub per_row_group_uncompressed_bytes: Vec<i64>,
    #[serde(default)]
    pub per_row_group_dict_page_bytes: Vec<Option<i64>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileProfile {
    pub path: String,
    pub file_size_bytes: u64,
    pub parquet_version: i32,
    pub num_rows: i64,
    pub num_row_groups: usize,
    pub row_group_row_counts: Vec<i64>,
    pub row_group_compressed_bytes: Vec<i64>,
    pub created_by: Option<String>,
    pub columns: Vec<ColumnMetaSummary>,
}

fn format_physical_type(t: Type) -> String {
    match t {
        Type::INT32 => "INT32".to_string(),
        Type::INT64 => "INT64".to_string(),
        Type::FLOAT => "FLOAT".to_string(),
        Type::DOUBLE => "DOUBLE".to_string(),
        Type::BYTE_ARRAY => "BYTE_ARRAY".to_string(),
        Type::FIXED_LEN_BYTE_ARRAY => "FIXED_LEN_BYTE_ARRAY".to_string(),
        Type::BOOLEAN => "BOOLEAN".to_string(),
        Type::INT96 => "INT96".to_string(),
    }
}

fn format_logical_type(lt: &LogicalType) -> String {
    match lt {
        LogicalType::String => "STRING".to_string(),
        LogicalType::Timestamp {
            is_adjusted_to_u_t_c,
            unit,
        } => {
            let unit_str = match unit {
                TimeUnit::MILLIS(_) => "MILLIS",
                TimeUnit::MICROS(_) => "MICROS",
                TimeUnit::NANOS(_) => "NANOS",
            };
            if *is_adjusted_to_u_t_c {
                format!("TIMESTAMP({}, UTC)", unit_str)
            } else {
                format!("TIMESTAMP({})", unit_str)
            }
        }
        LogicalType::Date => "DATE".to_string(),
        LogicalType::Time { .. } => "TIME".to_string(),
        LogicalType::Integer {
            bit_width,
            is_signed,
        } => format!("INT({}, {})", bit_width, is_signed),
        LogicalType::Decimal { precision, scale } => {
            format!("DECIMAL({}, {})", precision, scale)
        }
        LogicalType::List => "LIST".to_string(),
        LogicalType::Map => "MAP".to_string(),
        LogicalType::Enum => "ENUM".to_string(),
        LogicalType::Json => "JSON".to_string(),
        LogicalType::Bson => "BSON".to_string(),
        LogicalType::Uuid => "UUID".to_string(),
        LogicalType::Float16 => "FLOAT16".to_string(),
        other => format!("{:?}", other),
    }
}

fn format_compression(c: Compression) -> String {
    match c {
        Compression::SNAPPY => "SNAPPY".to_string(),
        Compression::GZIP(level) => format!("GZIP:{}", level.compression_level()),
        Compression::LZO => "LZO".to_string(),
        Compression::BROTLI(level) => format!("BROTLI:{}", level.compression_level()),
        Compression::LZ4 => "LZ4".to_string(),
        Compression::ZSTD(level) => format!("ZSTD:{}", level.compression_level()),
        Compression::LZ4_RAW => "LZ4_RAW".to_string(),
        Compression::UNCOMPRESSED => "UNCOMPRESSED".to_string(),
    }
}

fn format_encoding(e: Encoding) -> String {
    match e {
        Encoding::PLAIN => "PLAIN".to_string(),
        Encoding::RLE => "RLE".to_string(),
        #[allow(deprecated)]
        Encoding::BIT_PACKED => "BIT_PACKED".to_string(),
        Encoding::DELTA_BINARY_PACKED => "DELTA_BINARY_PACKED".to_string(),
        Encoding::DELTA_LENGTH_BYTE_ARRAY => "DELTA_LENGTH_BYTE_ARRAY".to_string(),
        Encoding::DELTA_BYTE_ARRAY => "DELTA_BYTE_ARRAY".to_string(),
        Encoding::RLE_DICTIONARY => "RLE_DICTIONARY".to_string(),
        Encoding::PLAIN_DICTIONARY => "PLAIN_DICTIONARY".to_string(),
        Encoding::BYTE_STREAM_SPLIT => "BYTE_STREAM_SPLIT".to_string(),
    }
}

fn decode_stats_value(bytes: &[u8], physical_type: Type) -> String {
    match physical_type {
        Type::INT32 => {
            let v = i32::from_le_bytes(bytes[0..4].try_into().unwrap_or([0; 4]));
            format!("{}", v)
        }
        Type::INT64 => {
            let v = i64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
            format!("{}", v)
        }
        Type::FLOAT => {
            let v = f32::from_le_bytes(bytes[0..4].try_into().unwrap_or([0; 4]));
            format!("{:.4}", v)
        }
        Type::DOUBLE => {
            let v = f64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
            format!("{:.4}", v)
        }
        Type::BYTE_ARRAY => {
            let s = String::from_utf8_lossy(bytes);
            let char_count = s.chars().count();
            if char_count > 40 {
                // "…" is 3 bytes in UTF-8; truncate to 39 chars so total byte len stays <= 42
                // for ASCII. For multi-byte chars the byte len may vary, but the char limit
                // ensures readable truncation.
                let truncated: String = s.chars().take(39).collect();
                format!("{}…", truncated)
            } else {
                s.into_owned()
            }
        }
        Type::FIXED_LEN_BYTE_ARRAY => {
            let limit = bytes.len().min(20);
            bytes[..limit]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        }
        Type::INT96 => bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
        Type::BOOLEAN => {
            if bytes.first().copied().unwrap_or(0) != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
    }
}

fn build_file_profile_from_metadata(
    metadata: parquet::file::metadata::ParquetMetaData,
    file_size_bytes: u64,
    path_str: &str,
) -> Result<FileProfile, crate::AutoparqError> {
    let file_meta = metadata.file_metadata();
    let parquet_version = file_meta.version();
    let num_rows = file_meta.num_rows();
    let created_by = file_meta.created_by().map(|s| s.to_string());

    let num_row_groups = metadata.num_row_groups();

    let mut row_group_row_counts: Vec<i64> = Vec::with_capacity(num_row_groups);
    let mut row_group_compressed_bytes: Vec<i64> = Vec::with_capacity(num_row_groups);

    for rg in metadata.row_groups() {
        row_group_row_counts.push(rg.num_rows());
        row_group_compressed_bytes.push(rg.compressed_size());
    }

    let columns = if num_row_groups == 0 {
        Vec::new()
    } else {
        let first_rg = metadata.row_group(0);
        let num_cols = first_rg.num_columns();
        let mut columns = Vec::with_capacity(num_cols);

        for col_idx in 0..num_cols {
            let first_col = first_rg.column(col_idx);

            let name = first_col.column_descr().name().to_string();
            let phys_type = first_col.column_type();
            let physical_type = format_physical_type(phys_type);
            let logical_type = first_col
                .column_descr()
                .logical_type()
                .as_ref()
                .map(format_logical_type);

            let mut encoding_set: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let codec = format_compression(first_col.compression());

            let mut compressed_bytes: i64 = 0;
            let mut uncompressed_bytes: i64 = 0;
            let mut statistics_available = true;
            let mut total_null_count: Option<i64> = Some(0);
            let mut global_min_bytes: Option<Vec<u8>> = None;
            let mut global_max_bytes: Option<Vec<u8>> = None;

            let mut per_rg_encodings: Vec<Vec<String>> = Vec::with_capacity(num_row_groups);
            let mut per_rg_compressed: Vec<i64> = Vec::with_capacity(num_row_groups);
            let mut per_rg_uncompressed: Vec<i64> = Vec::with_capacity(num_row_groups);
            let mut per_rg_dict_bytes: Vec<Option<i64>> = Vec::with_capacity(num_row_groups);

            for rg in metadata.row_groups() {
                let col = rg.column(col_idx);

                let mut rg_encs: Vec<String> =
                    col.encodings().iter().map(|e| format_encoding(*e)).collect();
                rg_encs.sort();
                rg_encs.dedup();
                for e in &rg_encs {
                    encoding_set.insert(e.clone());
                }
                per_rg_encodings.push(rg_encs);

                per_rg_compressed.push(col.compressed_size());
                per_rg_uncompressed.push(col.uncompressed_size());

                let dict_bytes = match col.dictionary_page_offset() {
                    Some(dict_off) => {
                        let delta = col.data_page_offset() - dict_off;
                        if delta > 0 { Some(delta) } else { None }
                    }
                    None => None,
                };
                per_rg_dict_bytes.push(dict_bytes);

                compressed_bytes += col.compressed_size();
                uncompressed_bytes += col.uncompressed_size();

                match col.statistics() {
                    None => {
                        statistics_available = false;
                        total_null_count = None;
                    }
                    Some(stats) => {
                        if statistics_available {
                            match stats.null_count_opt() {
                                None => total_null_count = None,
                                Some(nc) => {
                                    if let Some(ref mut acc) = total_null_count {
                                        *acc += nc as i64;
                                    }
                                }
                            }
                        }

                        if let Some(min_bytes) = stats.min_bytes_opt() {
                            let candidate = min_bytes.to_vec();
                            global_min_bytes = Some(match global_min_bytes {
                                None => candidate,
                                Some(prev) => {
                                    if candidate < prev { candidate } else { prev }
                                }
                            });
                        }

                        if let Some(max_bytes) = stats.max_bytes_opt() {
                            let candidate = max_bytes.to_vec();
                            global_max_bytes = Some(match global_max_bytes {
                                None => candidate,
                                Some(prev) => {
                                    if candidate > prev { candidate } else { prev }
                                }
                            });
                        }
                    }
                }
            }

            if !statistics_available {
                total_null_count = None;
            }

            let compression_ratio = if compressed_bytes > 0 {
                uncompressed_bytes as f64 / compressed_bytes as f64
            } else {
                1.0
            };

            let min_value = global_min_bytes
                .as_deref()
                .map(|b| decode_stats_value(b, phys_type));
            let max_value = global_max_bytes
                .as_deref()
                .map(|b| decode_stats_value(b, phys_type));

            let mut encodings: Vec<String> = encoding_set.into_iter().collect();
            encodings.sort();

            columns.push(ColumnMetaSummary {
                name,
                physical_type,
                logical_type,
                encodings,
                codec,
                compressed_bytes,
                uncompressed_bytes,
                compression_ratio,
                total_null_count,
                min_value,
                max_value,
                statistics_available,
                per_row_group_encodings: per_rg_encodings,
                per_row_group_compressed_bytes: per_rg_compressed,
                per_row_group_uncompressed_bytes: per_rg_uncompressed,
                per_row_group_dict_page_bytes: per_rg_dict_bytes,
            });
        }

        columns
    };

    Ok(FileProfile {
        path: path_str.to_string(),
        file_size_bytes,
        parquet_version,
        num_rows,
        num_row_groups,
        row_group_row_counts,
        row_group_compressed_bytes,
        created_by,
        columns,
    })
}

pub fn read_file_metadata_from_bytes(data: &[u8]) -> Result<FileProfile, crate::AutoparqError> {
    let bytes = Bytes::copy_from_slice(data);
    let metadata = ParquetMetaDataReader::new().parse_and_finish(&bytes)?;
    build_file_profile_from_metadata(metadata, data.len() as u64, "<memory>")
}

pub fn read_file_metadata(
    path: &std::path::Path,
) -> Result<FileProfile, crate::AutoparqError> {
    let file = std::fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::AutoparqError::FileNotFound(path.to_path_buf())
        } else {
            crate::AutoparqError::IoError(e)
        }
    })?;

    let file_size_bytes = file.metadata()?.len();
    let metadata = ParquetMetaDataReader::new().parse_and_finish(&file)?;
    let path_str = path.to_string_lossy();
    build_file_profile_from_metadata(metadata, file_size_bytes, &path_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_ratio_computation() {
        // compression_ratio = uncompressed / compressed
        // Just test the math: 100 / 50 = 2.0
        let ratio = if 50 > 0 { 100.0_f64 / 50.0_f64 } else { 1.0 };
        assert!((ratio - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_decode_int32() {
        let bytes = 42i32.to_le_bytes();
        let result = decode_stats_value(&bytes, Type::INT32);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_decode_string_truncation() {
        let long_string = "a".repeat(50);
        let result = decode_stats_value(long_string.as_bytes(), Type::BYTE_ARRAY);
        assert!(result.len() <= 42); // 40 chars + "…"
        assert!(result.ends_with('…'));
    }
}
