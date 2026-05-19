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
| `--encodings` | auto | Comma-separated: `PLAIN,DELTA_BINARY_PACKED` |
| `--measure` | `all` | What to measure: `read`, `write`, `size`, or `all` |

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

Note: Results are from an in-memory benchmark on the first row group sample. Actual I/O performance will differ.
