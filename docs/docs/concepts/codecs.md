# Parquet Codecs

Codecs compress the encoded bytes. Codec choice is secondary to encoding — a well-encoded column with Snappy often beats a PLAIN-encoded column with ZSTD:9.

## Codec comparison

| Codec | Compression | Read Speed | Write Speed | Compatibility |
|-------|-------------|------------|-------------|---------------|
| SNAPPY | Medium | Fast | Fast | All engines |
| ZSTD:3 | Good | Fast | Medium | Spark 3.2+, all others |
| ZSTD:6 | Best | Medium | Slow | Spark 3.2+, all others |
| LZ4 | Medium | Fastest | Fast | Spark 3.3+, all others |
| GZIP | Good | Slow | Slow | Most engines |
| UNCOMPRESSED | None | Fastest | Fastest | All engines |

## When to use each

**SNAPPY**: Default for Spark (all versions). Good all-around choice when compatibility matters.

**ZSTD:3**: Best default for modern engines (DuckDB, Polars, ClickHouse). Better compression than Snappy with similar read speed.

**ZSTD:6**: Use when storage cost is primary concern. Write speed is slower.

**LZ4**: Use when read throughput is critical. Fastest decompression.

**UNCOMPRESSED**: Use when data is already compressed (high byte entropy). Trying to compress pre-compressed data increases size.

## autoparq codec selection

autoparq selects codecs based on engine and priority:

| Condition | Codec |
|-----------|-------|
| byte_entropy > 7.5 | UNCOMPRESSED |
| engine=spark, priority!=size | SNAPPY |
| priority=speed | LZ4 |
| priority=size | ZSTD:6 |
| priority=balanced (default) | ZSTD:3 |
