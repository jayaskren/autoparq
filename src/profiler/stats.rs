use arrow::array::{
    Array, ArrayRef, BinaryArray, BooleanArray, Date32Array, Date64Array, FixedSizeBinaryArray,
    Float32Array, Float64Array, Int32Array, Int64Array, LargeBinaryArray, LargeStringArray,
    StringArray,
};
use arrow::compute::cast;
use arrow::datatypes::DataType;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StringLengthStats {
    pub min_len: usize,
    pub max_len: usize,
    pub mean_len: f64,
    pub stddev_len: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnProfile {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub sample_rows: usize,
    pub total_file_rows: i64,
    pub sample_fraction: f64,
    pub cardinality_estimate: u64,
    pub cardinality_ratio: f64,
    pub cardinality_method: String,
    pub monotonicity_score: Option<f64>,
    pub string_monotonicity_score: Option<f64>,
    pub run_length_score: f64,
    pub string_length_stats: Option<StringLengthStats>,
    pub uuid_pattern_detected: bool,
    pub json_pattern_detected: bool,
    pub byte_entropy: Option<f64>,
    pub null_count_in_sample: usize,
    pub null_fraction: f64,
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = ahash::AHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn monotonicity_score(array: &ArrayRef) -> Option<f64> {
    let values: Vec<Option<i64>> = match array.data_type() {
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        DataType::Timestamp(_, _) => {
            // Timestamp variants (Second, Millisecond, etc.) are not Int64Array;
            // use Arrow's cast kernel to convert to Int64 first.
            let casted = cast(array.as_ref(), &DataType::Int64).ok()?;
            let arr = casted.as_any().downcast_ref::<Int64Array>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i) as i64) })
                .collect()
        }
        DataType::Date32 => {
            let arr = array.as_any().downcast_ref::<Date32Array>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i) as i64) })
                .collect()
        }
        DataType::Date64 => {
            let arr = array.as_any().downcast_ref::<Date64Array>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        _ => return None,
    };

    let mut ascending = 0u64;
    let mut total = 0u64;
    let mut prev: Option<i64> = None;
    for val in &values {
        if let (Some(p), Some(v)) = (prev, val) {
            total += 1;
            if v >= &p {
                ascending += 1;
            }
        }
        if val.is_some() {
            prev = *val;
        } else {
            prev = None;
        }
    }
    if total == 0 {
        Some(0.0)
    } else {
        Some(ascending as f64 / total as f64)
    }
}

fn string_monotonicity_score(array: &ArrayRef) -> Option<f64> {
    let values: Vec<Option<&str>> = match array.data_type() {
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        _ => return None,
    };

    let mut ascending = 0u64;
    let mut total = 0u64;
    let mut prev: Option<&str> = None;
    for val in &values {
        if let (Some(p), Some(v)) = (prev, *val) {
            total += 1;
            if v >= p {
                ascending += 1;
            }
        }
        prev = if val.is_some() { *val } else { None };
    }

    if total == 0 {
        None
    } else {
        Some(ascending as f64 / total as f64)
    }
}

fn run_length_score(array: &ArrayRef) -> f64 {
    use arrow::util::display::{ArrayFormatter, FormatOptions};

    let opts = FormatOptions::default();
    let formatter = match ArrayFormatter::try_new(array.as_ref(), &opts) {
        Ok(f) => f,
        Err(_) => return 0.0,
    };

    let n = array.len();
    if n < 2 {
        return 0.0;
    }

    let mut equal = 0u64;
    let mut total = 0u64;

    for i in 1..n {
        if array.is_null(i - 1) || array.is_null(i) {
            continue;
        }
        total += 1;
        let prev = formatter.value(i - 1).to_string();
        let curr = formatter.value(i).to_string();
        if prev == curr {
            equal += 1;
        }
    }

    if total == 0 {
        0.0
    } else {
        equal as f64 / total as f64
    }
}

fn get_value_bytes(array: &ArrayRef, i: usize) -> Vec<u8> {
    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>()
                .expect("downcast guaranteed by DataType match arm");
            if arr.value(i) { vec![1u8] } else { vec![0u8] }
        }
        DataType::Int32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Int32Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Date32 => {
            let arr = array
                .as_any()
                .downcast_ref::<Date32Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Date64 => {
            let arr = array
                .as_any()
                .downcast_ref::<Date64Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Timestamp(_, _) => {
            // Timestamp variants are not Int64Array; cast via Arrow's cast kernel.
            if let Ok(casted) = cast(array.as_ref(), &DataType::Int64) {
                if let Some(arr) = casted.as_any().downcast_ref::<Int64Array>() {
                    return arr.value(i).to_le_bytes().to_vec();
                }
            }
            vec![]
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_le_bytes().to_vec()
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).as_bytes().to_vec()
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).as_bytes().to_vec()
        }
        DataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_vec()
        }
        DataType::FixedSizeBinary(_) => {
            let arr = array
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .expect("downcast guaranteed by DataType match arm");
            arr.value(i).to_vec()
        }
        _ => {
            // Fall back to string representation bytes
            use arrow::util::display::{ArrayFormatter, FormatOptions};
            let opts = FormatOptions::default();
            if let Ok(formatter) = ArrayFormatter::try_new(array.as_ref(), &opts) {
                formatter.value(i).to_string().into_bytes()
            } else {
                vec![]
            }
        }
    }
}

fn cardinality_info(array: &ArrayRef) -> (u64, String) {
    let non_null_count = array.len() - array.null_count();

    if non_null_count < 50_000 {
        let mut set = std::collections::HashSet::<u64>::new();
        for i in 0..array.len() {
            if array.is_null(i) {
                continue;
            }
            let bytes = get_value_bytes(array, i);
            set.insert(hash_bytes(&bytes));
        }
        return (set.len() as u64, "exact".to_string());
    }

    let mut hll = hyperloglog::HyperLogLog::new(0.01);
    for i in 0..array.len() {
        if array.is_null(i) {
            continue;
        }
        let bytes = get_value_bytes(array, i);
        hll.insert(&bytes);
    }
    (hll.len() as u64, "hyperloglog".to_string())
}

fn string_length_stats(array: &ArrayRef) -> Option<StringLengthStats> {
    let lengths: Vec<usize> = match array.data_type() {
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>()?;
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .map(|i| arr.value(i).len())
                .collect()
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>()?;
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .map(|i| arr.value(i).len())
                .collect()
        }
        _ => return None,
    };

    if lengths.is_empty() {
        return None;
    }

    let min_len = *lengths.iter().min().expect("lengths non-empty checked above");
    let max_len = *lengths.iter().max().expect("lengths non-empty checked above");
    let n = lengths.len() as f64;
    let mean_len = lengths.iter().sum::<usize>() as f64 / n;
    let variance = lengths
        .iter()
        .map(|&l| {
            let diff = l as f64 - mean_len;
            diff * diff
        })
        .sum::<f64>()
        / n;
    let stddev_len = variance.sqrt();

    Some(StringLengthStats {
        min_len,
        max_len,
        mean_len,
        stddev_len,
    })
}

fn uuid_pattern_detected(array: &ArrayRef) -> bool {
    let pattern = regex::Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
    )
    .expect("static UUID regex is valid");

    let samples: Vec<&str> = match array.data_type() {
        DataType::Utf8 => {
            let arr = match array.as_any().downcast_ref::<StringArray>() {
                Some(a) => a,
                None => return false,
            };
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .take(1000)
                .map(|i| arr.value(i))
                .collect()
        }
        DataType::LargeUtf8 => {
            let arr = match array.as_any().downcast_ref::<LargeStringArray>() {
                Some(a) => a,
                None => return false,
            };
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .take(1000)
                .map(|i| arr.value(i))
                .collect()
        }
        _ => return false,
    };

    if samples.is_empty() {
        return false;
    }

    let matches = samples.iter().filter(|s| pattern.is_match(s)).count();
    matches as f64 / samples.len() as f64 >= 0.9
}

fn json_pattern_detected(array: &ArrayRef) -> bool {
    let samples: Vec<String> = match array.data_type() {
        DataType::Utf8 => {
            let arr = match array.as_any().downcast_ref::<StringArray>() {
                Some(a) => a,
                None => return false,
            };
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .take(1000)
                .map(|i| arr.value(i).to_string())
                .collect()
        }
        DataType::LargeUtf8 => {
            let arr = match array.as_any().downcast_ref::<LargeStringArray>() {
                Some(a) => a,
                None => return false,
            };
            (0..arr.len())
                .filter(|&i| !arr.is_null(i))
                .take(1000)
                .map(|i| arr.value(i).to_string())
                .collect()
        }
        _ => return false,
    };

    if samples.is_empty() {
        return false;
    }

    let matches = samples
        .iter()
        .filter(|s| {
            let t = s.trim_start();
            t.starts_with('{') || t.starts_with('[')
        })
        .count();
    matches as f64 / samples.len() as f64 >= 0.8
}

fn byte_entropy(array: &ArrayRef) -> Option<f64> {
    match array.data_type() {
        DataType::Binary | DataType::LargeBinary | DataType::FixedSizeBinary(_) => {}
        _ => return None,
    }

    let mut freq = [0u64; 256];
    let mut total_bytes = 0u64;

    match array.data_type() {
        DataType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>()?;
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    continue;
                }
                for &b in arr.value(i) {
                    freq[b as usize] += 1;
                    total_bytes += 1;
                }
            }
        }
        DataType::LargeBinary => {
            let arr = array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .expect("downcast guaranteed by DataType match");
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    continue;
                }
                for &b in arr.value(i) {
                    freq[b as usize] += 1;
                    total_bytes += 1;
                }
            }
        }
        DataType::FixedSizeBinary(_) => {
            let arr = array.as_any().downcast_ref::<FixedSizeBinaryArray>()?;
            for i in 0..arr.len() {
                if arr.is_null(i) {
                    continue;
                }
                for &b in arr.value(i) {
                    freq[b as usize] += 1;
                    total_bytes += 1;
                }
            }
        }
        _ => return None,
    }

    if total_bytes == 0 {
        return Some(0.0);
    }

    let entropy = freq
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / total_bytes as f64;
            -p * p.log2()
        })
        .sum::<f64>();

    Some(entropy)
}

pub fn profile_column(sample: &crate::profiler::sampler::ColumnSample) -> ColumnProfile {
    let array = &sample.array;
    let sample_rows = array.len();
    let total_file_rows = sample.total_rows_in_file;

    let sample_fraction = if total_file_rows > 0 {
        sample_rows as f64 / total_file_rows as f64
    } else {
        0.0
    };

    let null_count_in_sample = array.null_count();
    let null_fraction = if sample_rows > 0 {
        null_count_in_sample as f64 / sample_rows as f64
    } else {
        0.0
    };

    let (cardinality_estimate, cardinality_method) = cardinality_info(array);
    let non_null_count = sample_rows - null_count_in_sample;
    let cardinality_ratio = if non_null_count > 0 {
        cardinality_estimate as f64 / non_null_count as f64
    } else {
        0.0
    };

    let monotonicity_score = monotonicity_score(array);
    let string_monotonicity_score = string_monotonicity_score(array);
    let run_length_score = run_length_score(array);

    let string_length_stats = string_length_stats(array);
    let uuid_pattern_detected = uuid_pattern_detected(array);
    let json_pattern_detected = json_pattern_detected(array);
    let byte_entropy = byte_entropy(array);

    ColumnProfile {
        column_name: sample.column_name.clone(),
        physical_type: sample.physical_type.clone(),
        logical_type: sample.logical_type.clone(),
        sample_rows,
        total_file_rows,
        sample_fraction,
        cardinality_estimate,
        cardinality_ratio,
        cardinality_method,
        monotonicity_score,
        string_monotonicity_score,
        run_length_score,
        string_length_stats,
        uuid_pattern_detected,
        json_pattern_detected,
        byte_entropy,
        null_count_in_sample,
        null_fraction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{BooleanArray, Int64Array, LargeBinaryArray, StringArray};
    use std::sync::Arc;

    fn make_sample(
        array: ArrayRef,
        physical_type: &str,
    ) -> crate::profiler::sampler::ColumnSample {
        crate::profiler::sampler::ColumnSample {
            column_name: "test".to_string(),
            physical_type: physical_type.to_string(),
            logical_type: None,
            array,
            total_rows_in_file: 1000,
            sampled_rows: 0,
        }
    }

    #[test]
    fn test_string_monotonicity_sorted() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec!["apple", "banana", "cherry", "date"]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert!(profile.string_monotonicity_score.unwrap() > 0.99);
    }

    #[test]
    fn test_string_monotonicity_reverse_sorted() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec!["z", "y", "x", "w"]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert_eq!(profile.string_monotonicity_score.unwrap(), 0.0);
    }

    #[test]
    fn test_string_monotonicity_none_for_int() {
        let arr: ArrayRef = Arc::new(Int64Array::from(vec![1i64, 2, 3]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert!(profile.string_monotonicity_score.is_none());
    }

    #[test]
    fn test_string_monotonicity_none_for_single_value() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec!["only_one"]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert!(profile.string_monotonicity_score.is_none());
    }

    #[test]
    fn test_monotonicity_sequential() {
        let arr: ArrayRef = Arc::new(Int64Array::from(vec![1i64, 2, 3, 4, 5]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert!(profile.monotonicity_score.unwrap() > 0.99);
    }

    #[test]
    fn test_monotonicity_random() {
        let arr: ArrayRef = Arc::new(Int64Array::from(vec![5i64, 2, 8, 1, 9]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert!(profile.monotonicity_score.unwrap() < 0.6);
    }

    #[test]
    fn test_run_length_all_same() {
        let arr: ArrayRef = Arc::new(Int64Array::from(vec![1i64, 1, 1, 1]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert!(profile.run_length_score > 0.99);
    }

    #[test]
    fn test_cardinality_exact_small() {
        let arr: ArrayRef = Arc::new(Int64Array::from(vec![1i64, 2, 3, 1, 2]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert_eq!(profile.cardinality_method, "exact");
        assert_eq!(profile.cardinality_estimate, 3);
    }

    #[test]
    fn test_uuid_detection_positive() {
        let uuids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ];
        let arr: ArrayRef = Arc::new(StringArray::from(uuids));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert!(profile.uuid_pattern_detected);
    }

    #[test]
    fn test_uuid_detection_negative() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec!["hello", "world", "foo"]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert!(!profile.uuid_pattern_detected);
    }

    #[test]
    fn test_null_fraction() {
        let arr: ArrayRef =
            Arc::new(Int64Array::from(vec![Some(1i64), None, Some(3), None]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        assert!((profile.null_fraction - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_monotonicity_with_nulls() {
        let arr: ArrayRef =
            Arc::new(Int64Array::from(vec![Some(1i64), None, Some(3), Some(4)]));
        let sample = make_sample(arr, "INT64");
        let profile = profile_column(&sample);
        let score = profile.monotonicity_score.unwrap();
        assert!(score > 0.99, "score was {}", score);
    }

    #[test]
    fn test_boolean_cardinality() {
        let arr: ArrayRef = Arc::new(BooleanArray::from(vec![true, false, true, true]));
        let sample = make_sample(arr, "BOOLEAN");
        let profile = profile_column(&sample);
        assert_eq!(profile.cardinality_estimate, 2);
        assert_eq!(profile.cardinality_method, "exact");
    }

    #[test]
    fn test_string_length_stats() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec!["hi", "hello", "hey"]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        let sls = profile.string_length_stats.unwrap();
        assert_eq!(sls.min_len, 2);
        assert_eq!(sls.max_len, 5);
    }

    #[test]
    fn test_json_detection_positive() {
        let arr: ArrayRef = Arc::new(StringArray::from(vec![
            r#"{"key": "value"}"#,
            r#"{"a": 1}"#,
            r#"["item1", "item2"]"#,
        ]));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        assert!(profile.json_pattern_detected);
    }

    #[test]
    fn test_byte_entropy_large_binary() {
        // LargeBinaryArray is what the parquet reader returns for BYTE_ARRAY columns.
        // Verify byte_entropy is computed (non-None) and produces a plausible value.
        let data: Vec<Option<Vec<u8>>> = (0u8..=255u8)
            .map(|b| Some(vec![b, b.wrapping_add(1), b.wrapping_mul(3)]))
            .collect();
        let refs: Vec<Option<&[u8]>> = data.iter().map(|v| v.as_deref()).collect();
        let arr: ArrayRef = Arc::new(LargeBinaryArray::from(refs));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        let entropy = profile.byte_entropy.expect("LargeBinary byte_entropy must be Some");
        // All 256 byte values are present across the values — entropy should be high
        assert!(entropy > 6.0, "expected high entropy, got {}", entropy);
    }

    #[test]
    fn test_byte_entropy_large_binary_high_entropy() {
        // Uniform distribution over all 256 byte values gives maximum entropy ~8.0
        let data: Vec<Option<Vec<u8>>> = (0u8..=255u8).map(|b| Some(vec![b])).collect();
        let refs: Vec<Option<&[u8]>> = data.iter().map(|v| v.as_deref()).collect();
        let arr: ArrayRef = Arc::new(LargeBinaryArray::from(refs));
        let sample = make_sample(arr, "BYTE_ARRAY");
        let profile = profile_column(&sample);
        let entropy = profile.byte_entropy.expect("LargeBinary byte_entropy must be Some");
        // Perfect uniform distribution → entropy = log2(256) = 8.0
        assert!((entropy - 8.0).abs() < 0.01, "expected ~8.0, got {}", entropy);
    }
}
