# autoparq — Product Requirements Document

## 1. Problem Statement

Choosing optimal Parquet compression settings is non-trivial. The right encoding and codec depend on data characteristics (cardinality, sort order, null density, value distribution) and the target query engine. Most engineers accept defaults — PLAIN encoding with Snappy — leaving 3–10× compression savings on the table and query performance degraded by poorly structured files.

There is no widely-used tool that:
- Profiles an actual file and recommends settings based on measured data characteristics
- Explains *why* each setting was chosen in terms the engineer can learn from
- Accounts for the target query engine's codec and encoding support
- Integrates naturally into existing data engineering workflows

## 2. Target Users

**Primary:** Data engineers who write or manage Parquet files in pipelines (Airflow, dbt, Spark, DuckDB).

**Secondary:** Analytics engineers and data platform teams who want to enforce compression standards across a team or project.

**Non-goals:** End consumers of data who don't control how files are written; real-time / per-record streaming (Parquet's row-group buffering makes it unsuitable for per-record writes).

## 3. Goals

1. **Credible recommendations** — every suggestion is backed by a specific measured statistic from the file, not a generic heuristic applied blindly.
2. **Actionable output** — the engineer can copy-paste a Python or Spark snippet and immediately apply the recommendations.
3. **Educational** — explanations teach the underlying principle so engineers build intuition and need the tool less over time.
4. **Pipeline-friendly** — JSON output mode and exit codes make it usable as a CI quality gate.
5. **Ergonomic** — two required inputs maximum; safe defaults for everything else.

## 4. Non-Goals

- Automatically rewriting files in production without explicit user action (`apply` command is opt-in)
- Supporting non-Parquet formats (ORC, Avro, CSV)
- Providing a web UI in v1
- Optimizing for write performance (focus is read performance and storage cost)

## 5. User Stories

### Core flow

**US-1** As a data engineer, I can point the tool at a Parquet file and receive per-column encoding and codec recommendations so that I know exactly what settings to use when writing the file.

**US-2** As a data engineer, I can specify my target engine (Spark, DuckDB, ClickHouse, Polars) so that the tool only recommends encodings and codecs my engine actually supports.

**US-3** As a data engineer, I can specify whether I care more about file size or read speed so that the tool's tradeoffs match my use case.

**US-4** As a data engineer, I can see *why* each recommendation was made (the specific statistic and rule that fired) so that I understand the reasoning and can apply it manually in future.

**US-5** As a data engineer, I receive a ready-to-paste Python snippet applying the recommendations so that adoption requires minimal effort.

### Advanced use

**US-6** As a pipeline author, I can run the tool in JSON output mode and check the exit code so that I can gate a CI/CD pipeline on compression quality.

**US-7** As a data engineer, I can benchmark specific codec and encoding combinations on the actual file to validate predictions before committing to a setting.

**US-8** As a data engineer, I can apply recommendations automatically to rewrite a file with the suggested settings.

### Explainability

**US-9** As a data engineer, I can request a full explanation (`--explain full`) to get a detailed breakdown of the statistics, the rule that fired, alternatives considered, and a "teach yourself" note for each column.

**US-10** As a data engineer, I see explicit confidence tiers (HIGH / MEDIUM / LOW) on each recommendation so that I know when to trust the tool and when to benchmark manually.

**US-11** As a data engineer, I can inspect the raw metadata of a Parquet file — schema, row group layout, column statistics, and current encoding settings — without triggering any sampling, so that I can quickly understand what a file contains before deciding whether to tune it.

## 6. Functional Requirements

### 6.1 Commands

#### `autoparq tune <file>`

Primary command. Profiles the file and outputs recommendations.

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--engine` | enum | `unknown` | Target engine: `spark`, `duckdb`, `polars`, `clickhouse`, `pandas`, `unknown` |
| `--priority` | enum | `balanced` | Optimization target: `size`, `speed`, `balanced` |
| `--explain` | enum | `brief` | Explanation verbosity: `brief` (one line), `full` (detailed breakdown) |
| `--output` | enum | `text` | Output format: `text` (rich terminal), `json` |
| `--sample-rows` | int | `2_000_000` | Max rows to sample for profiling |
| `--min-improvement` | float | `10.0` | Exit code 1 threshold (% improvement required) |

#### `autoparq bench <file>`

Benchmarks specific codec/encoding combinations against actual file data.

| Flag | Required | Description |
|------|----------|-------------|
| `--column` | yes | Column name to benchmark |
| `--codecs` | no | Comma-separated list: `zstd:3,lz4,snappy` (default: reasonable set) |
| `--measure` | no | What to measure: `read`, `write`, `size` (default: all three) |

#### `autoparq info <file>`

Prints file and column metadata from the Parquet footer only — no row scanning. Fast on any file size.

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--output` | enum | `text` | Output format: `text` (rich terminal), `json` |
| `--columns` | string | all | Comma-separated column names to show (default: all) |

**Text output includes:**

*File-level block:*
- File path, total size on disk
- Format version (Parquet v1 / v2)
- Row count, row group count, rows per row group (min/max/avg)
- Created-by string (writer library and version, if present)

*Per-column table:*

| Column | Physical Type | Logical Type | Encoding(s) | Codec | Null Count | Min | Max | Compressed | Uncompressed | Ratio |
|--------|--------------|-------------|-------------|-------|-----------|-----|-----|-----------|-------------|-------|

- Encodings listed as comma-separated (e.g. `PLAIN, RLE_DICTIONARY`)
- Compression ratio = uncompressed / compressed; `—` if codec is UNCOMPRESSED
- Min/max truncated to 40 chars for display; full value in JSON output
- Columns with absent statistics (writer did not compute them) shown with `—` and a footnote

**Example output:**
```
────────────────────────────────────────────────────────
 events.parquet  │  1.2 GB  │  Parquet v2  │  48,312,441 rows
 Row groups: 10  │  ~4.8M rows/group  │  Written by: parquet-go v0.29
────────────────────────────────────────────────────────

 COLUMNS (12)
 ┌─────────────────┬───────────┬────────────────┬──────────────────────┬────────┬────────────┬──────────────┬──────────────┬───────┐
 │ Column          │ Phys Type │ Logical Type   │ Encodings            │ Codec  │ Null Count │ Min          │ Max          │ Ratio │
 ├─────────────────┼───────────┼────────────────┼──────────────────────┼────────┼────────────┼──────────────┼──────────────┼───────┤
 │ event_type      │ BYTE_ARRAY│ STRING         │ PLAIN, RLE_DICTIONARY│ SNAPPY │ 0          │ "click"      │ "view"       │ 8.3x  │
 │ user_id         │ INT64     │ —              │ PLAIN                │ SNAPPY │ 512        │ 1000001      │ 9999999      │ 1.4x  │
 │ timestamp_ms    │ INT64     │ TIMESTAMP(ms)  │ PLAIN                │ SNAPPY │ 0          │ 1680000000000│ 1712000000000│ 1.1x  │
 │ revenue_usd     │ DOUBLE    │ —              │ PLAIN                │ SNAPPY │ 29,538,350 │ 0.01         │ 9999.99      │ 1.0x  │
 │ country_code    │ BYTE_ARRAY│ STRING         │ PLAIN, RLE_DICTIONARY│ SNAPPY │ 1,021      │ "AE"         │ "ZW"         │ 12.1x │
 │ ...             │           │                │                      │        │            │              │              │       │
 └─────────────────┴───────────┴────────────────┴──────────────────────┴────────┴────────────┴──────────────┴──────────────┴───────┘

 ¹ Statistics absent for 2 columns (writer did not compute them). Run `autoparq tune` to profile via sampling.
────────────────────────────────────────────────────────
```

#### `autoparq apply <file>`

Rewrites the file with the recommended encoding and codec settings. Internally runs the full tune pipeline to determine settings, then streams the input in 64K-row batches through an `ArrowWriter` configured with per-column encoding/compression. Writes to a temporary file first, then atomically renames to the output path.

| Flag | Required | Description |
|------|----------|-------------|
| `--output` | yes | Destination file path |
| `--in-place` | no | Allow `--output` to equal the input path; uses atomic temp-file rename |
| `--engine` | no | Same as `tune` |
| `--priority` | no | Same as `tune` |

**Safety rules:** Refuses to overwrite the input file without `--in-place`. Refuses to overwrite an existing output file without `--in-place`. The atomic rename via `tempfile::NamedTempFile::persist()` ensures the output is never partially written.

### 6.2 Profiling

The profiler operates in two phases:

**Phase 1 — Metadata (free, always run):**
- Existing codec per column chunk
- Compressed vs uncompressed size per column chunk → current compression ratio
- Null counts from row group statistics
- Min/max values from row group statistics
- Schema: physical type, logical type, encoding list
- Row group count, sizes, and row counts
- Page sizes (if page index present)

**Phase 2 — Sampling (reads one row group, ~50ms per column):**
- Cardinality estimation via HyperLogLog precision p=14 (±0.81% error); falls back to exact counting for samples < 50,000 rows
- Monotonicity score: `count(v[i] <= v[i+1]) / valid_pairs` on sampled rows in file order; null values excluded from pairs (treated as breaks); only computed for INT32/INT64/TIMESTAMP/DATE columns
- Run-length score: `count(v[i] == v[i-1]) / valid_pairs`; null values excluded from pairs
- Value distribution histogram (32 buckets)
- String length statistics: min, max, mean, stddev (BYTE_ARRAY columns only)
- UUID pattern detection (regex on sample)
- JSON pattern detection (heuristic parse attempt on sample)
- Byte entropy estimate (for binary columns)

### 6.3 Recommendation Engine

Encoding rules applied in priority order (first match wins):

1. `BOOLEAN` → `RLE` *(automatic in all libraries; noted in output)*
2. `INT32/INT64` (physical) OR `TIMESTAMP/DATE` (logical) + `monotonicity_score >= 0.90` → `DELTA_BINARY_PACKED`
3. `(any type)` + `cardinality_ratio < 0.10` AND `cardinality_estimate × avg_value_bytes < 524_288` (512 KB) → `RLE_DICTIONARY`
4. `FLOAT/DOUBLE` + `cardinality_ratio > 0.50` → `BYTE_STREAM_SPLIT`
5. `BYTE_ARRAY` + UUID pattern detected → `PLAIN` *(dictionary overflow avoidance)*
6. All others → `PLAIN`

Codec selection (applied after encoding):

| Condition | Codec |
|-----------|-------|
| `priority=speed` | LZ4 *(warn if delta encoding also selected — parquet-go bug)* |
| `priority=size` | ZSTD:6 |
| `priority=balanced` (default) | ZSTD:3 |
| `engine=spark`, unversioned | SNAPPY *(safe; ZSTD needs Spark 3.2+)* |
| Byte entropy > 7.5 bits/byte | UNCOMPRESSED |

Row group size guidance (advisory, not enforced):

| Workload | Recommended Size |
|----------|-----------------|
| Cold analytics / data lake | 256 MB – 1 GB |
| Standard ETL (Spark default) | 128 MB |
| Interactive query (DuckDB) | 64 – 128 MB |
| Streaming ingestion | 32 – 64 MB |

### 6.4 Output Requirements

**Text output must include:**
- File summary: path, size, row count, column count, current codec, scan time
- Estimated impact: predicted size reduction %, predicted read speedup, confidence tier
- Per-column table: column name, type, cardinality, null%, recommended encoding, recommended codec, impact stars (1–5)
- "Why" block: one-line reason per column citing the specific statistic
- Three ranked option bundles: Balanced (default), Smallest file, Fastest reads
- Caveats block: medium/low confidence warnings, engine compatibility notes
- Apply block: copy-paste Python snippet and Spark config

**Full explain (`--explain full`) additionally includes per column:**
- Raw statistics table (all measured values)
- The reasoning chain (which rules were evaluated and why each fired or was rejected)
- Alternatives considered and why they lost
- Engine compatibility note with minimum version
- "Teach yourself" principle

**JSON output must include:**
- All fields from text output in structured form
- `predicted_size_reduction_pct`, `predicted_read_speedup`, `confidence` at file level
- Per-column: `recommended_encoding`, `recommended_codec`, `codec_level`, `impact_stars`, `reason_code`, `reason_brief`, `engine_compatibility`
- `caveats` array with `severity`, `column`, `message`
- `options` object with A/B/C bundles

### 6.5 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | No significant improvement available (below `--min-improvement` threshold) |
| 1 | Improvement ≥ threshold available (file could be better compressed) |
| 2 | Error (file unreadable, unsupported format, I/O failure) |

### 6.6 Engine Compatibility Matrix

| Codec | Spark | DuckDB | ClickHouse | Polars/PyArrow |
|-------|-------|--------|------------|----------------|
| SNAPPY | ✓ all | ✓ | ✓ | ✓ |
| ZSTD | ✓ 3.2+ | ✓ | ✓ | ✓ |
| LZ4 | ✓ 3.3+ | ✓ | ✓ | ✓ |
| BROTLI | ✓ 3.3+ | ✓ | ✗ | ✓ |
| GZIP | ✓ all | ✓ | ✗ | ✓ |

`DELTA_BINARY_PACKED` encoding: Spark 3.2+, DuckDB ✓, ClickHouse ✓, PyArrow ✓.

Known compatibility bug: `parquet-go` + LZ4 codec + `DELTA_BINARY_PACKED` encoding produces files that cannot be read back. The tool warns when this combination is present in the target environment.

## 7. Non-Functional Requirements

### Performance
- Metadata-only phase completes in < 100ms for any file size
- Sampling phase completes in < 5 seconds for files up to 10 GB (single row group sampled)
- Column profiling is parallelized across columns using all available CPU cores

### Accuracy
- Size predictions accurate to ±20% on representative samples (> 2% of file)
- Confidence tier `LOW` assigned when sample fraction < 2% or high variance detected between row group chunks
- Tool never claims `HIGH` confidence on a sample of fewer than 100K rows

### Usability
- Default invocation (`autoparq tune file.parquet`) produces useful output with no other flags
- All terminal output fits in 120-column terminal without wrapping
- Copy-paste snippets are syntactically valid Python and Spark for the most recent stable versions

### Distribution
- Distributed as a PyPI wheel with compiled Rust extension (no Rust toolchain required by end user)
- Wheels built for: Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64, Windows x86_64
- Python ≥ 3.9 required
- Optional standalone binary via `cargo install autoparq` for users without Python

## 8. Technical Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Core profiler | Rust + `parquet` + `arrow` crates | Performance for data reading; official Apache implementation |
| Parallelism | `rayon` | Data-parallel column profiling with no manual thread management |
| Python bindings | `PyO3` + `maturin` | Mature, ergonomic; maturin handles wheel building and PyPI publishing |
| CLI | `typer` (Python) | Clean, type-annotated CLIs with minimal boilerplate |
| Terminal rendering | `rich` (Python) | Tables, colors, progress bars without curses complexity |
| Error handling | `thiserror` (Rust) | Idiomatic derive-based error types |
| JSON serialization | `serde_json` (Rust) | Zero-cost serialization of output structs |
| Snapshot testing | `insta` (Rust) | Recommendation changes produce reviewable diffs |
| Benchmarking | `criterion` (Rust) | Statistically rigorous microbenchmarks for profiler throughput |

## 9. Scope and Milestones

### v0.1 — Core profiler and basic recommendations
- Metadata parsing (Phase 1 profiling)
- `info` command: full metadata display from footer only
- Single-row-group sampling (Phase 2 profiling)
- Encoding + codec recommendations for all column types
- Text output with per-column table and "why" block (brief)
- Python/Spark apply snippet in output
- `tune` command

### v0.2 — Engine awareness and confidence tiers
- `--engine` flag and compatibility matrix
- Confidence tier computation
- Engine compatibility warnings in output
- JSON output mode
- Exit codes for CI integration

### v0.3 — Explainability and options
- `--explain full` mode
- Three ranked option bundles (A/B/C)
- Row group size guidance
- Sort order detection and advisory

### v0.4 — Bench and apply commands
- `autoparq bench` command with actual codec benchmarking
- `autoparq apply` command
- `--config` file support

### v1.0 — Polish and distribution
- PyPI wheels for all target platforms
- Standalone binary build
- Documentation site
- Comprehensive test fixture library

## 10. Key Design Decisions and Rationale

**Rust core + Python CLI:** The performance-critical path (reading and sampling large files) benefits from Rust. The heuristic logic and CLI are more maintainable in Python and don't need to be fast. PyO3/maturin makes this split ergonomic.

**Encoding before codec:** The biggest compression gains come from encoding strategy (DELTA, RLE_DICTIONARY), not codec choice. A well-encoded file with Snappy often beats a PLAIN-encoded file with ZSTD:9. The tool leads with encoding recommendations, not codec recommendations.

**Two required inputs:** User research (implicit from data engineering conventions) shows that engine and priority/tradeoff are the two dimensions that most affect the recommendation. Everything else is either measured from the file or has a safe default.

**Never silently wrong:** If the tool can't compute a statistic reliably (missing parquet statistics, very small file, high variance sample), it says so explicitly with a `LOW` confidence tag rather than presenting a confident-looking incorrect recommendation.

**Sort order advisory, not enforced:** Detecting whether data *should* be sorted (to improve future writes) is in scope. Resorting data as part of `apply` is out of scope for v1 — it changes row order, which is a more dangerous operation than changing encoding settings.
