# autoparq — Source References

Authoritative sources backing each category of claim made by autoparq. Organized by topic for use as "learn more" links in the UI or for internal citation.

---

## Row Group Sizing

The "64–256 MB" guideline is a synthesis of several independent defaults, not a single spec number:

| Source | Key claim | URL |
|--------|-----------|-----|
| Apache Parquet spec (official) | Recommends 512 MB–1 GB for batch/analytical workloads on HDFS: *"Larger groups require more buffering in the write path…"* | https://parquet.apache.org/docs/file-format/configurations/ |
| Apache Spark docs | `spark.sql.files.maxPartitionBytes` defaults to 128 MB — one task reads one row group, so groups beyond 128 MB don't improve Spark parallelism | https://spark.apache.org/docs/latest/sql-data-sources-parquet.html |
| Databricks / Delta Lake docs | Auto-tunes target file size to **256 MB** for moderate tables — the most common source of the "256 MB" ceiling in practice | https://docs.databricks.com/aws/en/delta/tune-file-size |
| DuckDB performance guide | Row groups < 5,000 rows cause **5–10× slower runtimes**; DuckDB can only parallelize over row groups, so need ≥ 1 group per CPU thread | https://duckdb.org/docs/current/guides/performance/file_formats |
| DuckDB Parquet tips | Recommends 100K–1M rows per group (~50–500 MB); discusses memory cost of large groups on concurrent reads | https://duckdb.org/docs/current/data/parquet/tips |
| Apache Arrow Rust (`ArrowWriter` docs) | *"Smaller row groups result in higher metadata overheads… memory usage can be limited by calling `Self::flush`… although this will likely increase overall file size and reduce query performance"* | https://arrow.apache.org/rust/parquet/arrow/arrow_writer/struct.ArrowWriter.html |
| ClickHouse engineering blog | ClickHouse decompresses column chunks independently; recommends 100 KB–10 MB per row group to bound per-thread memory | https://clickhouse.com/blog/apache-parquet-clickhouse-local-querying-writing-internals-row-groups |
| Netflix TechBlog | Production evidence for the "small files problem" — fragmented Parquet overwhelms metadata services at petabyte scale | https://netflixtechblog.com/optimizing-data-warehouse-storage-7b94a48fdcbe |

**Take-away:** 64–256 MB covers the Spark 128 MB partition default (lower bound for parallelism) through the Delta Lake 256 MB auto-tune default (upper bound for moderate tables). The Parquet spec itself recommends larger; ClickHouse recommends smaller. The right number is engine-specific.

---

## Encodings

### DELTA_BINARY_PACKED (monotonic integers and timestamps)

| Source | Key claim | URL |
|--------|-----------|-----|
| Apache Parquet Encodings spec | Adapted from *"Decoding billions of integers per second through vectorization"* (Lemire & Boytsov); near-zero overhead for perfectly monotonic runs | https://github.com/apache/parquet-format/blob/master/Encodings.md |
| Lemire & Boytsov (2012/2015) | Peer-reviewed paper behind the algorithm; SIMD-BP128 is ~2× faster than prior fastest integer encoding schemes | https://arxiv.org/abs/1209.2137 |
| DuckDB blog (Jan 2025) | For integer sequences, DELTA_BINARY_PACKED produced files **~99% smaller** and writing was **~2× faster** vs Parquet v1 baseline | https://duckdb.org/2025/01/22/parquet-encodings |

### RLE_DICTIONARY (low-cardinality columns)

| Source | Key claim | URL |
|--------|-----------|-----|
| Apache Parquet Encodings spec | *"If the dictionary grows too big, whether in size or number of distinct values, the encoding will fall back to the plain encoding"* | https://github.com/apache/parquet-format/blob/master/Encodings.md |
| parquet-java source | Default `dictionary_page_size = 1 MB` — the de facto industry threshold before fallback to PLAIN | https://github.com/apache/parquet-java/blob/master/parquet-hadoop/src/main/java/org/apache/parquet/hadoop/ParquetOutputFormat.java |
| Wes McKinney / Arrow blog (2019) | *"Most implementations will use dictionary encoding… until the dictionary reaches ~1 MB. At this point, the writer will 'fall back' to PLAIN encoding."* Dictionary is per-row-group, not global | https://arrow.apache.org/blog/2019/09/05/faster-strings-cpp-parquet/ |

**Note on the 512 KB heuristic:** autoparq's `cardinality_estimate × avg_value_bytes < 512 KB` threshold is deliberately half the 1 MB limit, to leave headroom for dictionary page headers and avoid triggering the fallback mid-row-group.

### BYTE_STREAM_SPLIT (high-cardinality floats/doubles)

| Source | Key claim | URL |
|--------|-----------|-----|
| Apache Parquet Encodings spec | *"Does not reduce the size of the data but can lead to a significantly better compression ratio and speed when a compression algorithm is used afterwards."* Groups byte positions across values so exponent bytes (which vary less) cluster together | https://github.com/apache/parquet-format/blob/master/Encodings.md |
| DuckDB PR #15653 (2024) | On TPC-H SF10 + ZSTD: BYTE_STREAM_SPLIT reduced file size from 466 MB → 284 MB (**39% reduction**) with no write time regression | https://github.com/duckdb/duckdb/pull/15653 |
| martinradev/arrow-fp-compression-bench | BYTE_STREAM_SPLIT + ZSTD achieves 1.11–17× compression ratios across 16 floating-point datasets vs 1.04–8.5× for PLAIN + ZSTD | https://github.com/martinradev/arrow-fp-compression-bench |

### PLAIN fallback (UUIDs and random byte data)

No single authoritative source states "use PLAIN for UUIDs" explicitly, but the reasoning is assembled from:
- The Parquet spec's fallback guarantee: dictionary encoding collapses to PLAIN immediately when the dictionary fills on high-cardinality data — writing PLAIN proactively avoids the wasted dictionary page write.
- `parquet-java` PARQUET-2052: documents an integer overflow bug triggered by writing large binary data with dictionary encoding, confirming this is an actively known hazard. https://github.com/apache/parquet-java/pull/910

---

## Codecs

### ZSTD:3 — balanced default

| Source | Key claim | URL |
|--------|-----------|-----|
| facebook/zstd README | Benchmark table: zstd level 1 = ratio 2.896 at 510 MB/s compression / 1550 MB/s decompression, vs snappy at ratio 2.089 / 1500 MB/s decompression. Level 3 (default) improves ratio further | https://github.com/facebook/zstd |
| AWS Athena docs | *"Athena uses ZSTD compression level 3 by default… Level 3 is suitable for many use cases"* | https://docs.aws.amazon.com/athena/latest/ug/compression-support-zstd-levels.html |

### LZ4 — fastest decompression

| Source | Key claim | URL |
|--------|-----------|-----|
| LZ4 official site | *"Extremely fast decoder, with speed in multiple GB/s per core (~1 Byte/cycle)"*; benchmark: **3850 MB/s** decompression vs zstd's 1550 MB/s | https://lz4.org/ |

### ZSTD:6+ — size priority

From the facebook/zstd README data: level 6 yields ~7% better compression ratio than level 3 at roughly half the compression speed. Beyond level 9 the ratio curve flattens significantly. AWS Athena docs note: *"Use levels greater than 19 with caution as they require more memory."* Practical range for storage optimization: levels 3–9.

### Snappy — Spark-safe default

| Source | Key claim | URL |
|--------|-----------|-----|
| Apache Spark docs | `spark.sql.parquet.compression.codec` defaults to **snappy** since Spark 2.1.0 | https://spark.apache.org/docs/latest/sql-data-sources-parquet.html |
| SPARK-25366 (Apache Jira) | Documents that ZSTD/Brotli required external Hadoop codec JARs before Spark 3.0 — the primary technical reason Snappy remained the safe cross-version default | https://issues.apache.org/jira/browse/SPARK-25366 |
| Databricks/Data+AI Summit 2021 | *"Different zstd-jni versions in Spark/Parquet/Avro/Kafka are incompatible."* Recommends Spark 3.2+ (DBR 8.0) as the practical threshold for stable ZSTD | https://www.slideshare.net/databricks/the-rise-of-zstandard-apache-sparkparquetorcavro |

### Engine version compatibility

| Engine | Codec | Version required | Source |
|--------|-------|-----------------|--------|
| Spark | ZSTD (Parquet, stable) | 3.2+ | SPARK-25366 + Databricks presentation |
| Spark | LZ4_RAW (Parquet) | 3.5+ | https://spark.apache.org/releases/spark-release-3-5-0.html |
| Spark | DELTA_BINARY_PACKED read | 3.3+ (vectorized reader) | https://spark.apache.org/releases/spark-release-3-3-0.html |
| DuckDB | SNAPPY, ZSTD, GZIP, LZ4 | All current versions | https://duckdb.org/docs/current/data/parquet/overview |
| ClickHouse | SNAPPY, LZ4, ZSTD, GZIP, BROTLI | All current versions | https://clickhouse.com/docs/interfaces/formats/Parquet |

**Caveat on ClickHouse brotli/gzip:** The official ClickHouse docs list both as supported for import and export. The practical compatibility risk is cross-tool (e.g., files written by Spark with brotli that ClickHouse's codec negotiation rejects in specific versions). This is a compatibility warning, not a documented hard block.

---

## Byte Entropy Threshold (> 7.5 → skip compression)

| Source | Key claim | URL |
|--------|-----------|-----|
| BTRFS kernel docs | Linux kernel uses Shannon entropy as a production heuristic to skip compression for incompressible data: *"data sampling, long repeated pattern detection, byte frequency, Shannon entropy"* | https://btrfs.readthedocs.io/en/latest/Compression.html |
| MDPI Entropy journal (2022) | Peer-reviewed: entropy > 7.2–7.5 is a reliable classifier for already-compressed or encrypted byte streams | https://www.mdpi.com/1099-4300/24/10/1503 |

---

## Known Bugs (cited caveats)

| Bug | Status | Notes |
|-----|--------|-------|
| parquet-go + LZ4 + DELTA_BINARY_PACKED → unreadable files | No public canonical issue found | Empirically observed; label as "compatibility warning, empirically reported" not a linked bug |
| ClickHouse + BROTLI import | No hard official exclusion found | Soften to "compatibility warning"; official docs list brotli as supported |
