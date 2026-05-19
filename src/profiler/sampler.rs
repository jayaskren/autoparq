use arrow::array::ArrayRef;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ProjectionMask;
use parquet::basic::{LogicalType, TimeUnit, Type};
use parquet::file::metadata::ParquetMetaDataReader;

pub struct ColumnSample {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub array: ArrayRef,
    pub total_rows_in_file: i64,
    pub sampled_rows: usize,
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

pub fn list_column_names_from_bytes(
    data: bytes::Bytes,
) -> Result<Vec<String>, crate::AutoparqError> {
    let metadata = ParquetMetaDataReader::new().parse_and_finish(&data)?;
    let schema_desc = metadata.file_metadata().schema_descr();
    let names = schema_desc
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    Ok(names)
}

pub fn list_column_names(
    path: &std::path::Path,
) -> Result<Vec<String>, crate::AutoparqError> {
    let file = std::fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::AutoparqError::FileNotFound(path.to_path_buf())
        } else {
            crate::AutoparqError::IoError(e)
        }
    })?;

    let metadata = ParquetMetaDataReader::new().parse_and_finish(&file)?;
    let schema_desc = metadata.file_metadata().schema_descr();
    let names = schema_desc
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    Ok(names)
}

pub fn sample_column_from_bytes(
    data: bytes::Bytes,
    column_name: &str,
    row_group_index: usize,
    max_rows: usize,
) -> Result<ColumnSample, crate::AutoparqError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(data)?;

    let schema_desc = builder.parquet_schema();
    let col_idx = schema_desc
        .columns()
        .iter()
        .position(|c| c.name() == column_name)
        .ok_or_else(|| {
            crate::AutoparqError::UnsupportedType(format!(
                "Column '{}' not found",
                column_name
            ))
        })?;

    let col_desc = &schema_desc.columns()[col_idx];
    let physical_type = format_physical_type(col_desc.physical_type());
    let logical_type = col_desc.logical_type().as_ref().map(format_logical_type);

    let total_rows_in_file = builder.metadata().file_metadata().num_rows();

    let mask = ProjectionMask::leaves(schema_desc, [col_idx]);

    let reader = builder
        .with_row_groups(vec![row_group_index])
        .with_limit(max_rows)
        .with_projection(mask)
        .build()?;

    let mut batches = Vec::new();
    for batch_result in reader {
        let batch = batch_result?;
        batches.push(batch);
    }

    let arrays: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();

    if arrays.is_empty() {
        use arrow::array::Int64Array;
        return Ok(ColumnSample {
            column_name: column_name.to_string(),
            physical_type,
            logical_type,
            array: std::sync::Arc::new(Int64Array::from(Vec::<i64>::new())),
            total_rows_in_file,
            sampled_rows: 0,
        });
    }

    let refs: Vec<&dyn arrow::array::Array> = arrays.iter().map(|a| a.as_ref()).collect();
    let concatenated = arrow::compute::concat(&refs)?;
    let sampled_rows = concatenated.len();

    Ok(ColumnSample {
        column_name: column_name.to_string(),
        physical_type,
        logical_type,
        array: concatenated,
        total_rows_in_file,
        sampled_rows,
    })
}

pub fn sample_column(
    path: &std::path::Path,
    column_name: &str,
    row_group_index: usize,
    max_rows: usize,
) -> Result<ColumnSample, crate::AutoparqError> {
    let file = std::fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::AutoparqError::FileNotFound(path.to_path_buf())
        } else {
            crate::AutoparqError::IoError(e)
        }
    })?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;

    // Resolve column index and type info from the parquet schema
    let schema_desc = builder.parquet_schema();
    let col_idx = schema_desc
        .columns()
        .iter()
        .position(|c| c.name() == column_name)
        .ok_or_else(|| {
            crate::AutoparqError::UnsupportedType(format!(
                "Column '{}' not found",
                column_name
            ))
        })?;

    let col_desc = &schema_desc.columns()[col_idx];
    let physical_type = format_physical_type(col_desc.physical_type());
    let logical_type = col_desc.logical_type().as_ref().map(format_logical_type);

    let total_rows_in_file = builder.metadata().file_metadata().num_rows();

    let mask = ProjectionMask::leaves(schema_desc, [col_idx]);

    let reader = builder
        .with_row_groups(vec![row_group_index])
        .with_limit(max_rows)
        .with_projection(mask)
        .build()?;

    let mut batches = Vec::new();
    for batch_result in reader {
        let batch = batch_result?;
        batches.push(batch);
    }

    let arrays: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();

    if arrays.is_empty() {
        use arrow::array::Int64Array;
        return Ok(ColumnSample {
            column_name: column_name.to_string(),
            physical_type,
            logical_type,
            array: std::sync::Arc::new(Int64Array::from(Vec::<i64>::new())),
            total_rows_in_file,
            sampled_rows: 0,
        });
    }

    let refs: Vec<&dyn arrow::array::Array> = arrays.iter().map(|a| a.as_ref()).collect();
    let concatenated = arrow::compute::concat(&refs)?;
    let sampled_rows = concatenated.len();

    Ok(ColumnSample {
        column_name: column_name.to_string(),
        physical_type,
        logical_type,
        array: concatenated,
        total_rows_in_file,
        sampled_rows,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn test_sample_returns_correct_row_count() {
        // After fixtures exist:
        // let sample = super::sample_column(
        //     std::path::Path::new("tests/fixtures/monotonic_ints.parquet"),
        //     "id", 0, 100_000
        // ).unwrap();
        // assert!(sample.sampled_rows <= 100_000);
        // assert!(sample.sampled_rows > 0);
    }

    #[test]
    #[ignore]
    fn test_list_column_names() {
        // After fixtures exist:
        // let names = super::list_column_names(
        //     std::path::Path::new("tests/fixtures/multi_column.parquet")
        // ).unwrap();
        // assert!(!names.is_empty());
    }
}
