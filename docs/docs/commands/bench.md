# autoparq bench

Benchmark encoding and codec combinations for a specific column.

```
autoparq bench <file> --column COL [--codecs zstd:3,lz4,snappy] [--encodings PLAIN,DELTA_BINARY_PACKED] [--measure read|write|size|all]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--column` | required | Column name to benchmark |
| `--codecs` | all 6 standard | Comma-separated codec list: `zstd:3,lz4,snappy` |
| `--encodings` | type-dependent (see below) | Comma-separated: `PLAIN,DELTA_BINARY_PACKED` |
| `--measure` | `all` | What to measure: `read`, `write`, `size`, or `all` |

## Default encoding sets

When `--encodings` is not specified, autoparq selects encodings based on the column's physical type:

| Physical type | Default encodings tested |
|---------------|--------------------------|
| `INT32`, `INT64` | `PLAIN`, `DELTA_BINARY_PACKED`, `RLE_DICTIONARY` |
| `BYTE_ARRAY` | `PLAIN`, `RLE_DICTIONARY`, `DELTA_LENGTH_BYTE_ARRAY`, `DELTA_BYTE_ARRAY` |
| `FLOAT`, `DOUBLE` | `PLAIN`, `BYTE_STREAM_SPLIT` |
| `BOOLEAN` | `PLAIN` |
| Other | `PLAIN` |

## Examples

Benchmark all codecs on a column:

```
autoparq bench events.parquet --column user_id
```

Compare only ZSTD levels:

```
autoparq bench events.parquet --column user_id --codecs "zstd:1,zstd:3,zstd:6"
```

Focus on read performance:

```
autoparq bench events.parquet --column user_id --measure read
```

Benchmark only the delta string encodings:

```
autoparq bench events.parquet --column tags --encodings "PLAIN,DELTA_BYTE_ARRAY,DELTA_LENGTH_BYTE_ARRAY"
```

## Notes on delta string encodings

When benchmarking `DELTA_BYTE_ARRAY` or `DELTA_LENGTH_BYTE_ARRAY`, the bench writer automatically sets `use_dictionary=False` for the column being benchmarked. No manual flag is needed. Without this, the Parquet writer silently falls back to `RLE_DICTIONARY` and the benchmark result would not reflect the intended encoding.

Note: Results are from an in-memory benchmark on the first row group sample. Actual I/O performance will differ.
