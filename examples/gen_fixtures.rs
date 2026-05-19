use std::path::Path;
use std::sync::Arc;
use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::{EnabledStatistics, WriterProperties};

fn write_parquet(path: &Path, batch: arrow::record_batch::RecordBatch, props: WriterProperties) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    println!("Written: {}", path.display());
}

fn default_props() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .set_statistics_enabled(EnabledStatistics::Chunk)
        .build()
}

fn main() {
    std::fs::create_dir_all("tests/fixtures").unwrap();

    // 1. monotonic_ints.parquet — INT64 sequential, triggers DELTA rule
    {
        let ids: Vec<i64> = (0i64..100_000).collect();
        let ids_arr = Int64Array::from(ids);
        let schema = Schema::new(vec![Field::new("id", DataType::Int64, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(ids_arr)]).unwrap();
        write_parquet(Path::new("tests/fixtures/monotonic_ints.parquet"), batch, default_props());
    }

    // 2. low_cardinality_strings.parquet — STRING 5 values, triggers RLE_DICTIONARY
    {
        let statuses = ["active", "inactive", "pending", "deleted", "suspended"];
        let vals: Vec<Option<&str>> = (0..100_000).map(|i| Some(statuses[i % 5])).collect();
        let vals_arr = StringArray::from(vals);
        let schema = Schema::new(vec![Field::new("status", DataType::Utf8, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(vals_arr)]).unwrap();
        write_parquet(Path::new("tests/fixtures/low_cardinality_strings.parquet"), batch, default_props());
    }

    // 3. uuids.parquet — UUID-format strings, triggers PlainUuid rule
    {
        let strs: Vec<String> = (0..10_000u64).map(|i| {
            format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
                i, (i>>16)&0xffff, i&0xfff, 0x8000|(i&0x3fff), i*1234567)
        }).collect();
        let vals: Vec<Option<&str>> = strs.iter().map(|s| Some(s.as_str())).collect();
        let vals_arr = StringArray::from(vals);
        let schema = Schema::new(vec![Field::new("id", DataType::Utf8, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(vals_arr)]).unwrap();
        write_parquet(Path::new("tests/fixtures/uuids.parquet"), batch, default_props());
    }

    // 4. high_entropy.parquet — BINARY random bytes, triggers UNCOMPRESSED recommendation
    // Uses xorshift64 to produce near-uniform byte distribution (entropy ~7.99 bits/byte)
    {
        let mut rng: u64 = 0xdeadbeef_cafebabe;
        let blobs: Vec<Vec<u8>> = (0..10_000u64).map(|_| {
            let mut bytes = Vec::with_capacity(32);
            for _ in 0..4 {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                bytes.extend_from_slice(&rng.to_le_bytes());
            }
            bytes
        }).collect();
        let vals: Vec<Option<&[u8]>> = blobs.iter().map(|b| Some(b.as_slice())).collect();
        let vals_arr = BinaryArray::from(vals);
        let schema = Schema::new(vec![Field::new("blob", DataType::Binary, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(vals_arr)]).unwrap();
        write_parquet(Path::new("tests/fixtures/high_entropy.parquet"), batch, default_props());
    }

    // 5. high_cardinality_floats.parquet — DOUBLE random, triggers BYTE_STREAM_SPLIT
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let vals: Vec<f64> = (0..100_000u64).map(|i| {
            let mut h = DefaultHasher::new();
            i.hash(&mut h);
            f64::from_bits(h.finish())
        }).collect();
        let vals_arr = Float64Array::from(vals);
        let schema = Schema::new(vec![Field::new("value", DataType::Float64, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(vals_arr)]).unwrap();
        write_parquet(Path::new("tests/fixtures/high_cardinality_floats.parquet"), batch, default_props());
    }

    // 6. no_statistics.parquet — no stats
    {
        let vals: Vec<i32> = (0..10_000i32).collect();
        let vals_arr = Int32Array::from(vals);
        let schema = Schema::new(vec![Field::new("x", DataType::Int32, false)]);
        let batch = arrow::record_batch::RecordBatch::try_new(Arc::new(schema), vec![Arc::new(vals_arr)]).unwrap();
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .set_statistics_enabled(EnabledStatistics::None)
            .build();
        write_parquet(Path::new("tests/fixtures/no_statistics.parquet"), batch, props);
    }

    // 7. multi_column.parquet — 6 mixed columns
    {
        let n = 50_000usize;
        let ids: Vec<i64> = (0..n as i64).collect();
        let ids_arr = Int64Array::from(ids);

        let statuses = ["active", "inactive", "pending", "deleted", "suspended"];
        let status_vals: Vec<Option<&str>> = (0..n).map(|i| Some(statuses[i % 5])).collect();
        let status_arr = StringArray::from(status_vals);

        let scores: Vec<f64> = (0..n as u64).map(|i| (i as f64).sin() * 100.0).collect();
        let scores_arr = Float64Array::from(scores);

        let ts_vals: Vec<i64> = (0..n as i64).map(|i| 1_700_000_000_000i64 + i * 1000).collect();
        let ts_arr = TimestampMillisecondArray::from(ts_vals);

        let name_strs: Vec<String> = (0..n as u64).map(|i| {
            format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
                i, (i>>16)&0xffff, i&0xfff, 0x8000|(i&0x3fff), i*1234567)
        }).collect();
        let names_vals: Vec<Option<&str>> = name_strs.iter().map(|s| Some(s.as_str())).collect();
        let names_arr = StringArray::from(names_vals);

        let flags: Vec<Option<bool>> = (0..n).map(|i| Some(i % 3 == 0)).collect();
        let flags_arr = BooleanArray::from(flags);

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("score", DataType::Float64, false),
            Field::new("ts", DataType::Timestamp(TimeUnit::Millisecond, None), false),
            Field::new("name", DataType::Utf8, false),
            Field::new("flag", DataType::Boolean, false),
        ]);
        let batch = arrow::record_batch::RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(ids_arr),
                Arc::new(status_arr),
                Arc::new(scores_arr),
                Arc::new(ts_arr),
                Arc::new(names_arr),
                Arc::new(flags_arr),
            ],
        ).unwrap();
        write_parquet(Path::new("tests/fixtures/multi_column.parquet"), batch, default_props());
    }

    println!("All fixtures generated.");
}
