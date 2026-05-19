# autoparq — Agent Task List

## How to Use This File

Each task below is scoped for a single specialized agent. Tasks within the same milestone that have no shared output files can be assigned concurrently. Validation tasks must run after all implementation tasks in their milestone complete.

### Validation / Fix Loop Protocol

After every validation task:
- If **PASS**: proceed to the next milestone.
- If **FAIL**: launch a fix agent with the validation report as input. After the fix agent completes, re-launch the same validation agent. Repeat until PASS. Only then proceed.

After the final evaluation task (T-EVAL):
- If **PASS**: project is complete.
- If **FAIL**: launch a targeted fix agent for each failed criterion, then re-run T-EVAL.

---

## Milestone v0.1 — Core Profiler and Basic Recommendations

---

### T01 — Project Scaffold

**Assign to:** Rust/Python build tooling agent

**Inputs required:** Nothing. This is the first task.

**Files to create:**
```
Cargo.toml
pyproject.toml
.cargo/config.toml          (linker hints for cross-compile)
src/lib.rs                  (PyO3 module entry point)
src/error.rs                (thiserror error enum)
python/autoparq/__init__.py
python/autoparq/cli.py      (stub: app = typer.Typer())
python/autoparq/render.py   (stub: empty module)
python/autoparq/codegen.py  (stub: empty module)
```

**Cargo.toml requirements:**
- Crate type: `["cdylib", "rlib"]`
- Dependencies: `parquet` (features: `arrow`, `snap`, `lz4`, `zstd`, `brotli`), `arrow`, `pyo3` (features: `extension-module`), `rayon`, `thiserror`, `serde`, `serde_json`, `regex`, `ahash`
- Dev dependencies: `insta`, `tempfile`

**pyproject.toml requirements:**
- `[build-system]` requires maturin `>=1.7,<2.0`
- `[tool.maturin]` sets `python-source = "python"` and `module-name = "autoparq._lib"`
- `[project.scripts]` defines `autoparq = "autoparq.cli:app"`
- Python `>=3.9`

**src/error.rs requirements:**
- `#[derive(Debug, thiserror::Error)]` enum `AutoparqError` with variants:
  - `FileNotFound(PathBuf)`
  - `IoError(#[from] std::io::Error)`
  - `ParquetError(#[from] parquet::errors::ParquetError)`
  - `ArrowError(#[from] arrow::error::ArrowError)`
  - `UnsupportedType(String)`

**src/lib.rs requirements:**
- `#[pymodule]` function `autoparq_lib(m: &Bound<PyModule>)` (use new PyO3 `Bound` API, not deprecated `&PyModule`)
- Export a stub `fn py_info_file(path: &str) -> PyResult<String>` that returns `Ok("{}".to_string())`

**Acceptance criteria:**
- `maturin develop` completes without errors
- `python -c "import autoparq._lib; print('ok')"` succeeds
- `autoparq --help` shows the Typer help text
- `cargo test` passes (no tests yet, but must compile)

**Must not touch:** Any files outside the list above.

---

### T02 — Parquet Footer Metadata Reader

**Assign to:** Rust data structures agent

**Inputs required:** T01 complete (`src/error.rs` and `Cargo.toml` exist).

**Files to create:**
```
src/profiler/mod.rs
src/profiler/metadata.rs
```

**Files to modify:**
```
src/lib.rs   (add mod profiler;)
```

**Structs to define in `metadata.rs`** (all `#[derive(Debug, Serialize, Clone)]`):

```rust
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

pub struct ColumnMetaSummary {
    pub name: String,
    pub physical_type: String,          // "INT64", "BYTE_ARRAY", etc.
    pub logical_type: Option<String>,   // "STRING", "TIMESTAMP(MILLIS)", etc.
    pub encodings: Vec<String>,         // ["PLAIN", "RLE_DICTIONARY"]
    pub codec: String,                  // "SNAPPY", "ZSTD", etc.
    pub compressed_bytes: i64,          // summed across all row groups
    pub uncompressed_bytes: i64,        // summed across all row groups
    pub compression_ratio: f64,         // uncompressed / compressed
    pub total_null_count: Option<i64>,  // None if statistics absent
    pub min_value: Option<String>,      // decoded to string, None if absent
    pub max_value: Option<String>,      // decoded to string, None if absent
    pub statistics_available: bool,
}
```

**Function to implement:**
```rust
pub fn read_file_metadata(path: &Path) -> Result<FileProfile, AutoparqError>
```
- Use `ParquetMetaDataReader::new().parse_metadata(&file)` — footer only, zero row scanning
- Aggregate `null_count`, `compressed_size`, `uncompressed_size` across ALL row groups per column
- For global `min_value`/`max_value`: take the minimum of all row group minimums, maximum of all row group maximums
- Decode min/max bytes to human-readable strings based on physical type:
  - INT32: `i32::from_le_bytes`
  - INT64: `i64::from_le_bytes`
  - FLOAT: `f32::from_le_bytes`
  - DOUBLE: `f64::from_le_bytes`
  - BYTE_ARRAY: UTF-8 decode, truncate to 40 chars, append "…" if truncated
  - FIXED_LEN_BYTE_ARRAY: hex-encode first 20 bytes
- `statistics_available`: true only if ALL row groups have statistics for this column
- `logical_type`: format as human-readable string (e.g. `"TIMESTAMP(MILLIS, UTC)"`, `"STRING"`, `"DATE"`)

**Unit tests** (in `metadata.rs` under `#[cfg(test)]`):
- Test with `tests/fixtures/multi_column.parquet` once T09 creates it; for now, skip with `#[ignore]`
- Test `compression_ratio` computation: uncompressed=100, compressed=50 → ratio=2.0
- Test min/max decoding for each physical type with known byte values

**Must not touch:** `src/profiler/sampler.rs`, `src/profiler/stats.rs`, `src/recommender/`, `python/`

---

### T03 — `autoparq info` Command

**Assign to:** Python CLI + rendering agent

**Inputs required:** T02 complete (metadata reader exists and is tested).

**Files to create:**
```
python/autoparq/render.py   (replace stub)
```

**Files to modify:**
```
src/lib.rs                  (expose py_info_file)
python/autoparq/cli.py      (replace stub with info command)
```

**Rust addition to `src/lib.rs`:**
```rust
#[pyfunction]
fn py_info_file(path: &str, columns_filter: Option<Vec<String>>) -> PyResult<String> {
    // calls read_file_metadata, optionally filters columns, serializes to JSON
}
// register in #[pymodule]
```

**Python `cli.py` requirements:**
- `app = typer.Typer(name="autoparq", no_args_is_help=True)`
- `@app.command("info")` with arguments:
  - `file: Annotated[Path, typer.Argument(..., help="Parquet file to inspect")]`
  - `output: Annotated[OutputFormat, typer.Option("--output", "-o")] = OutputFormat.text`
  - `columns: Annotated[Optional[str], typer.Option("--columns", help="Comma-separated column names")] = None`
- `OutputFormat` enum: `text = "text"`, `json = "json"`
- On `output=json`: print raw JSON from Rust, exit 0
- On `output=text`: call `render.render_info(profile_dict)`
- On any exception: print error to stderr, `raise typer.Exit(2)`

**Python `render.py` requirements — `render_info(profile: dict)`:**
- Use `rich.console.Console()` and `rich.table.Table`
- File header panel (rich Panel): path, file size (human-readable), parquet version, row count (comma-formatted), row group count, avg rows/group, created_by string
- Per-column table columns (in order): `Column`, `Physical Type`, `Logical Type`, `Encodings`, `Codec`, `Nulls`, `Min`, `Max`, `Compressed`, `Uncompressed`, `Ratio`
- `Ratio` displayed as `8.3x` format
- `Compressed`/`Uncompressed` in human-readable bytes (KB/MB/GB)
- If `statistics_available=false` for a column: show `—` in Nulls/Min/Max cells, color them dim
- Footer footnote if any column has `statistics_available=false`: `"¹ Statistics absent — run autoparq tune to profile via sampling"`
- Total table width ≤ 120 characters; truncate column name at 30 chars if needed

**Acceptance criteria:**
- `autoparq info --help` shows all flags
- `autoparq info tests/fixtures/multi_column.parquet` renders without errors (once fixture exists)
- `autoparq info tests/fixtures/multi_column.parquet --output json` emits valid JSON matching `FileProfile` schema
- `autoparq info /nonexistent.parquet` exits with code 2 and prints error to stderr

**Must not touch:** `src/profiler/sampler.rs`, `src/profiler/stats.rs`, `src/recommender/`

---

### T04 — Single Row-Group Column Sampler

**Assign to:** Rust data I/O agent

**Inputs required:** T01 complete (Cargo.toml has `parquet`/`arrow` deps), T02 complete (`src/profiler/mod.rs` exists).

**Files to create:**
```
src/profiler/sampler.rs
```

**Files to modify:**
```
src/profiler/mod.rs   (add pub mod sampler;)
```

**Structs to define:**
```rust
pub struct ColumnSample {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub array: arrow::array::ArrayRef,
    pub total_rows_in_file: i64,   // from file metadata, for sample_fraction
    pub sampled_rows: usize,
}
```

**Function to implement:**
```rust
pub fn sample_column(
    path: &Path,
    column_name: &str,
    row_group_index: usize,
    max_rows: usize,
) -> Result<ColumnSample, AutoparqError>
```
- Use `ParquetRecordBatchReaderBuilder::try_new(file)?`
- Apply `.with_row_groups(vec![row_group_index])`
- Apply `.with_limit(max_rows)`
- Apply `ProjectionMask::columns(schema_desc, &[column_name])` to read ONLY the requested column
- Concatenate all record batches into a single `ArrayRef` using `arrow::compute::concat`
- Return `UnsupportedType` error if column name not found in schema
- `total_rows_in_file`: call `read_file_metadata` internally to get the total row count (reuse from T02)

**Additional function:**
```rust
pub fn list_column_names(path: &Path) -> Result<Vec<String>, AutoparqError>
```
- Footer-only; returns all column names in schema order

**Unit tests:**
- All tests tagged `#[ignore]` until T09 creates fixtures
- Skeleton test: `test_sample_returns_correct_row_count` — after fixture exists, assert `sampled_rows <= max_rows`
- Skeleton test: `test_sample_single_column_only` — assert result array has correct data type

**Must not touch:** `src/profiler/stats.rs`, `src/recommender/`, `python/`, `src/profiler/metadata.rs`

---

### T05 — Statistical Profiling Engine

**Assign to:** Rust statistics agent

**Inputs required:** T04 complete (`ColumnSample` struct defined).

**Files to create:**
```
src/profiler/stats.rs
```

**Files to modify:**
```
src/profiler/mod.rs   (add pub mod stats;)
Cargo.toml            (add hyperloglog dependency — use `hyperloglog = "1.0"` for simpler API,
                       OR `hyperloglog-rs` — confirm which compiles cleanly first and commit to one)
```

**Structs to define:**
```rust
pub struct StringLengthStats {
    pub min_len: usize,
    pub max_len: usize,
    pub mean_len: f64,
    pub stddev_len: f64,
}

pub struct ColumnProfile {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub sample_rows: usize,
    pub total_file_rows: i64,
    pub sample_fraction: f64,           // sample_rows / total_file_rows
    pub cardinality_estimate: u64,
    pub cardinality_ratio: f64,         // cardinality_estimate / sample_rows
    pub cardinality_method: String,     // "exact" or "hyperloglog"
    pub monotonicity_score: Option<f64>,// None for non-numeric types
    pub run_length_score: f64,
    pub string_length_stats: Option<StringLengthStats>, // Some for BYTE_ARRAY only
    pub uuid_pattern_detected: bool,
    pub json_pattern_detected: bool,
    pub byte_entropy: Option<f64>,      // Some for BINARY/FIXED_LEN_BYTE_ARRAY
    pub null_count_in_sample: usize,
    pub null_fraction: f64,
}
```

**Function to implement:**
```rust
pub fn profile_column(sample: &ColumnSample) -> ColumnProfile
```

Implementation requirements per metric:

**Cardinality:**
- If `sample.sampled_rows < 50_000`: use exact `HashSet<u64>` with `ahash::AHashSet`, hashing each value's byte representation. Set `cardinality_method = "exact"`.
- Otherwise: use HyperLogLog with precision p=14. Set `cardinality_method = "hyperloglog"`.
- Handle nulls: do not insert null values into the HLL/set.

**Monotonicity score** (only for INT32, INT64, physical types; also TIMESTAMP logical type):
- Downcast to `Int64Array` (or `Int32Array` cast to Int64)
- Iterate adjacent pairs; skip any pair where either value is null
- `monotonicity_score = valid_ascending_pairs / total_valid_pairs`
- Return `None` for FLOAT, DOUBLE, BYTE_ARRAY, BOOLEAN, FIXED_LEN_BYTE_ARRAY

**Run-length score** (all types):
- Iterate adjacent pairs; skip pairs containing nulls
- `run_length_score = equal_adjacent_pairs / total_valid_pairs`
- Return 0.0 if `total_valid_pairs == 0`

**String length stats** (BYTE_ARRAY only):
- Downcast to `StringArray` or `LargeStringArray`
- Compute min/max/mean/stddev of `.len()` per non-null value

**UUID detection** (BYTE_ARRAY only):
- Sample up to 1000 non-null values
- Regex: `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`
- `uuid_pattern_detected = true` if ≥ 90% of sampled values match

**JSON detection** (BYTE_ARRAY only):
- Sample up to 1000 non-null values
- `json_pattern_detected = true` if ≥ 80% start with `{` or `[` after `trim_start()`

**Byte entropy** (FIXED_LEN_BYTE_ARRAY and BINARY physical types only):
- Build 256-bucket byte frequency histogram over all bytes of all non-null values
- `H = -sum(p * p.log2())` where `p = count / total_bytes`
- Return `None` for other types

**Null counting:**
- `null_count_in_sample`: count null slots in the Arrow array
- `null_fraction = null_count_in_sample as f64 / sample_rows as f64`

**Unit tests** (no file I/O — construct Arrow arrays directly):
- `test_monotonicity_sequential`: array `[1,2,3,4,5]` → score = 1.0
- `test_monotonicity_random`: array `[5,2,8,1,9]` → score < 0.5
- `test_run_length_all_same`: array `[1,1,1,1]` → score = 1.0
- `test_cardinality_exact_small`: 100-row array with 5 distinct values → exact method, estimate = 5
- `test_uuid_detection_positive`: array of valid UUIDs → `uuid_pattern_detected = true`
- `test_uuid_detection_negative`: array of random strings → `uuid_pattern_detected = false`
- `test_null_fraction`: array with 50% nulls → `null_fraction = 0.5`
- `test_monotonicity_with_nulls`: `[1, null, 3, 4]` → skips null pair, computes from remaining

**Must not touch:** `src/recommender/`, `python/`, `src/profiler/metadata.rs`, `src/profiler/sampler.rs`

---

### T06 — Encoding Recommendation Rules

**Assign to:** Rust heuristics agent

**Inputs required:** T05 complete (`ColumnProfile` struct defined with all fields).

**Files to create:**
```
src/recommender/mod.rs
src/recommender/encoding.rs
```

**Files to modify:**
```
src/lib.rs   (add mod recommender;)
```

**Enums/structs to define:**
```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum RuleName {
    BooleanRle,
    DeltaMonotonic,
    RleDictionary,
    ByteStreamSplit,
    PlainUuid,
    PlainDefault,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ConfidenceTier {
    High,   // sample_fraction >= 0.10 AND sample_rows >= 100_000
    Medium, // sample_fraction >= 0.02 OR sample_rows >= 50_000
    Low,    // everything else
}

#[derive(Debug, Clone, Serialize)]
pub struct EncodingRecommendation {
    pub encoding: String,               // "DELTA_BINARY_PACKED", "RLE_DICTIONARY", etc.
    pub rule_fired: RuleName,
    pub reason_brief: String,           // cites specific statistic, e.g. "monotonicity_score=0.94 > threshold 0.90"
    pub confidence: ConfidenceTier,
    pub confidence_reason: String,      // explains why this tier was assigned
}
```

**Function to implement:**
```rust
pub fn recommend_encoding(
    profile: &ColumnProfile,
    meta: &ColumnMetaSummary,
) -> EncodingRecommendation
```

**Rules — apply in this exact priority order, first match wins:**

1. `physical_type == "BOOLEAN"` → encoding `"RLE"`, rule `BooleanRle`, reason `"BOOLEAN columns use RLE automatically in all libraries"`

2. `physical_type IN ["INT32","INT64"] OR logical_type IN ["TIMESTAMP","DATE"]`
   AND `monotonicity_score >= 0.90`
   → encoding `"DELTA_BINARY_PACKED"`, rule `DeltaMonotonic`
   → reason: `"monotonicity_score={:.3} >= threshold 0.90"`

3. `cardinality_ratio < 0.10`
   AND `cardinality_estimate * avg_value_bytes < 524_288` (512 KB)
   → encoding `"RLE_DICTIONARY"`, rule `RleDictionary`
   → reason: `"cardinality_ratio={:.4} ({} distinct / {} rows) < threshold 0.10"`
   → `avg_value_bytes`: use `string_length_stats.mean_len` for BYTE_ARRAY; 4 for INT32; 8 for INT64/DOUBLE; 4 for FLOAT; 1 for BOOLEAN

4. `physical_type IN ["FLOAT","DOUBLE"]`
   AND `cardinality_ratio > 0.50`
   → encoding `"BYTE_STREAM_SPLIT"`, rule `ByteStreamSplit`
   → reason: `"high-entropy float column (cardinality_ratio={:.3} > 0.50)"`

5. `physical_type == "BYTE_ARRAY"`
   AND `uuid_pattern_detected == true`
   → encoding `"PLAIN"`, rule `PlainUuid`
   → reason: `"UUID pattern detected — dictionary encoding would overflow ({} distinct values)"`

6. All others → encoding `"PLAIN"`, rule `PlainDefault`, reason `"no specific pattern detected"`

**Confidence tier assignment:**
```
High:   sample_fraction >= 0.10 AND sample_rows >= 100_000
Medium: (sample_fraction >= 0.02 OR sample_rows >= 50_000) AND not High
Low:    everything else
```
Additional Medium→Low downgrade: if `rule_fired == RleDictionary` and `cardinality_ratio` is within 20% of the 0.10 threshold (i.e., 0.08–0.12), downgrade to Medium even if sample stats would say High.

**Unit tests** — all use synthetic `ColumnProfile` and `ColumnMetaSummary` values (no file I/O):
- One "fires" test and one "does not fire" test per rule (12 tests minimum)
- `test_rule_priority_delta_beats_dict`: column that qualifies for both rules 2 and 3 → DELTA wins
- `test_confidence_high`: sample_fraction=0.15, sample_rows=200_000 → High
- `test_confidence_medium_low_fraction`: sample_fraction=0.01, sample_rows=200_000 → Medium
- `test_confidence_low`: sample_fraction=0.01, sample_rows=10_000 → Low
- `test_confidence_rle_dict_boundary`: cardinality_ratio=0.09 (within 20% of 0.10) → Medium even with large sample

**Must not touch:** `src/recommender/codec.rs`, `src/recommender/engine.rs`, `python/`, `src/profiler/`

---

### T07 — Codec Selection

**Assign to:** Rust heuristics agent (can run in parallel with T06 if `EncodingRecommendation` struct is already defined from T06 — wait for T06's struct definitions before starting)

**Inputs required:** T06 complete (`EncodingRecommendation` struct and `RuleName` enum defined).

**Files to create:**
```
src/recommender/codec.rs
```

**Enums/structs to define:**
```rust
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Priority { Size, Speed, Balanced }

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Engine { Spark, DuckDB, Polars, ClickHouse, Pandas, Unknown }

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum CaveatSeverity { Warning, Info }

#[derive(Debug, Clone, Serialize)]
pub struct Caveat {
    pub severity: CaveatSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodecRecommendation {
    pub codec: String,               // "ZSTD", "SNAPPY", "LZ4", "UNCOMPRESSED"
    pub codec_level: Option<i32>,    // Some(3) for ZSTD:3, None for SNAPPY
    pub reason_brief: String,
    pub caveats: Vec<Caveat>,
}
```

**Function to implement:**
```rust
pub fn recommend_codec(
    profile: &ColumnProfile,
    encoding_rec: &EncodingRecommendation,
    priority: Priority,
    engine: Engine,
) -> CodecRecommendation
```

**Codec selection logic (apply in order, first match wins):**

1. `byte_entropy > 7.5` → `UNCOMPRESSED`, reason: `"byte entropy {:.2} > 7.5 — data appears pre-compressed"`, add Info caveat: `"Compressing this column would increase size; using UNCOMPRESSED"`

2. `engine == Engine::Spark && priority != Priority::Size` → `SNAPPY`, reason: `"SNAPPY is safe for all Spark versions"`, add Info caveat: `"ZSTD requires Spark 3.2+; use --engine spark:3.2+ to unlock"`

3. `priority == Priority::Speed` → `LZ4`, level: None, reason: `"LZ4 has fastest decompression"`
   - If `encoding_rec.encoding == "DELTA_BINARY_PACKED"`: add Warning caveat: `"parquet-go library has a known bug with LZ4 + DELTA_BINARY_PACKED — files may be unreadable. Use SNAPPY if your reader uses parquet-go."`

4. `priority == Priority::Size` → `ZSTD`, level: `Some(6)`, reason: `"ZSTD:6 maximizes compression ratio"`

5. Default (`priority == Priority::Balanced`) → `ZSTD`, level: `Some(3)`, reason: `"ZSTD:3 balances size and read speed"`
   - If `engine == Engine::ClickHouse` and codec would be `BROTLI`: add Warning caveat: `"ClickHouse does not support BROTLI for Parquet import; using ZSTD instead"`

**Unit tests** — all use synthetic inputs:
- `test_entropy_gate`: `byte_entropy=7.8` → UNCOMPRESSED regardless of priority
- `test_spark_safety_snappy`: `engine=Spark, priority=Balanced` → SNAPPY with caveat
- `test_spark_size_zstd`: `engine=Spark, priority=Size` → ZSTD:6 (size priority overrides Spark safety)
- `test_speed_lz4`: `priority=Speed` → LZ4
- `test_speed_lz4_with_delta_caveat`: `priority=Speed, encoding=DELTA_BINARY_PACKED` → LZ4 with Warning caveat
- `test_balanced_default`: `engine=Unknown, priority=Balanced` → ZSTD:3
- `test_size_zstd6`: `engine=Unknown, priority=Size` → ZSTD:6

**Must not touch:** `src/recommender/encoding.rs`, `src/recommender/engine.rs`, `python/`, `src/profiler/`

---

### T08 — `autoparq tune` Command (Rust Orchestrator + Python CLI)

**Assign to:** Full-stack integration agent

**Inputs required:** T01–T07 all complete.

**Files to create:**
```
src/tuner.rs
python/autoparq/codegen.py   (replace stub)
```

**Files to modify:**
```
src/lib.rs                   (expose py_tune_file, add mod tuner)
python/autoparq/cli.py       (add tune command)
python/autoparq/render.py    (add render_tune_text)
```

**New structs in `src/tuner.rs`:**
```rust
pub struct ColumnRecommendation {
    pub column_name: String,
    pub physical_type: String,
    pub logical_type: Option<String>,
    pub cardinality_estimate: u64,
    pub cardinality_ratio: f64,
    pub null_fraction: f64,
    pub recommended_encoding: String,
    pub recommended_codec: String,
    pub recommended_codec_level: Option<i32>,
    pub encoding_rule_fired: String,
    pub reason_brief: String,
    pub confidence: String,             // "High", "Medium", "Low"
    pub confidence_reason: String,
    pub impact_stars: u8,               // 1–5, computed from predicted improvement
    pub caveats: Vec<Caveat>,
}

pub struct TuneReport {
    pub file_path: String,
    pub file_size_bytes: u64,
    pub num_rows: i64,
    pub num_columns: usize,
    pub current_codec: String,          // most common codec across columns
    pub scan_time_ms: u64,
    pub sample_fraction: f64,
    pub predicted_size_reduction_pct: f64,
    pub predicted_read_speedup: f64,
    pub overall_confidence: String,
    pub columns: Vec<ColumnRecommendation>,
    pub caveats: Vec<Caveat>,
    pub python_snippet: String,
    pub spark_snippet: String,
}
```

**`impact_stars` computation:**
- 5★: encoding changes from PLAIN to DELTA or PLAIN to RLE_DICTIONARY with cardinality_ratio < 0.01
- 4★: encoding changes and cardinality_ratio < 0.10, or DELTA on sorted timestamps
- 3★: codec changes to ZSTD from SNAPPY with no encoding change, or BYTE_STREAM_SPLIT recommended
- 2★: minor codec improvement only
- 1★: no change from current settings (already optimal or near-optimal)

**Rust function in `src/lib.rs`:**
```rust
#[pyfunction]
fn py_tune_file(
    path: &str,
    engine: &str,
    priority: &str,
    sample_rows: usize,
) -> PyResult<String>
```
Orchestration steps:
1. `read_file_metadata(path)` → `FileProfile`
2. `list_column_names(path)` → column list
3. `rayon::par_iter` over columns:
   - `sample_column(path, col, 0, sample_rows)` → `ColumnSample`
   - `profile_column(&sample)` → `ColumnProfile`
   - `recommend_encoding(&profile, &meta)` → `EncodingRecommendation`
   - `recommend_codec(&profile, &enc_rec, priority, engine)` → `CodecRecommendation`
   - Assemble `ColumnRecommendation`
4. Compute `predicted_size_reduction_pct` (heuristic — see below)
5. Build `TuneReport`, serialize with `serde_json::to_string`

**Predicted size reduction heuristic:**
- Per column, estimate improvement factor:
  - DELTA on currently-PLAIN INT64/TIMESTAMP: `estimated_factor = 3.0`
  - RLE_DICTIONARY on currently-PLAIN with `cardinality_ratio < 0.001`: `estimated_factor = 10.0`
  - RLE_DICTIONARY on currently-PLAIN with `cardinality_ratio < 0.01`: `estimated_factor = 5.0`
  - RLE_DICTIONARY on currently-PLAIN with `cardinality_ratio < 0.10`: `estimated_factor = 2.0`
  - ZSTD replacing SNAPPY (no encoding change): `estimated_factor = 1.25`
  - BYTE_STREAM_SPLIT: `estimated_factor = 1.15`
  - No change: `estimated_factor = 1.0`
- Weight each column by its `uncompressed_bytes` share of total
- `predicted_size_reduction_pct = (1.0 - 1.0/weighted_avg_factor) * 100.0`
- All predictions labeled `"estimated"` in JSON and text output

**Python `codegen.py` — two functions:**

`generate_python_snippet(report: dict) -> str`:
```python
import pyarrow.parquet as pq

PARQUET_WRITE_OPTIONS = {
    "compression": "<codec>",
    "compression_level": <level>,   # omit line if None
    "column_encoding": {
        "<col>": "<encoding>",
        ...   # only include columns where encoding != "PLAIN"
    },
    "write_statistics": True,
}
pq.write_table(table, "output.parquet", **PARQUET_WRITE_OPTIONS)
```

`generate_spark_snippet(report: dict) -> str`:
```
spark.conf.set("spark.sql.parquet.compression.codec", "<codec>")
# Column-level encoding requires Spark 3.4+ via per-column hints
```

**Python `cli.py` — `@app.command("tune")`:**
- Arguments: `file: Path`
- Options: `--engine` (choices: spark/duckdb/polars/clickhouse/pandas/unknown, default: unknown), `--priority` (size/speed/balanced, default: balanced), `--output` (text/json, default: text), `--sample-rows` (int, default: 2_000_000), `--min-improvement` (float, default: 10.0)
- Exit codes: `raise typer.Exit(0)` if `predicted_size_reduction_pct < min_improvement`; `raise typer.Exit(1)` if ≥ threshold; `raise typer.Exit(2)` on any exception

**Python `render.py` — `render_tune_text(report: dict)`:**
- Header panel: file path, size, rows, columns, current codec, scan time, sample fraction
- Summary line: `"Estimated impact: -{:.0f}% size, {:.1f}x read speed ({} confidence)"`
- Per-column table: `Column | Type | Cardinality | Null% | Encoding | Codec | Impact`
- Impact shown as filled stars: `★★★★☆` for 4 stars
- "Why" block: one line per column citing `reason_brief`
- Caveats block (only if caveats exist): grouped by severity
- Apply block: Python snippet from `codegen.py`
- All predictions followed by `[estimated]` label

**Must not touch:** `src/recommender/engine.rs`, `src/profiler/stats.rs` (read only)

---

### T09 — Test Fixtures + Test Infrastructure

**Assign to:** Rust testing agent

**Inputs required:** T01–T08 all complete.

**Files to create:**
```
examples/gen_fixtures.rs
tests/fixtures/.gitkeep         (fixtures generated at test time, not committed)
tests/integration/mod.rs
tests/integration/test_info.rs
tests/integration/test_tune.rs
```

**Files to modify:**
```
Cargo.toml   (add [[example]] gen_fixtures; add insta dev dependency)
src/profiler/metadata.rs  (remove #[ignore] from tests that need fixtures)
src/profiler/sampler.rs   (remove #[ignore] from tests that need fixtures)
```

**`examples/gen_fixtures.rs` — generates these files in `tests/fixtures/`:**

| Fixture | Schema | Purpose |
|---------|--------|---------|
| `monotonic_ints.parquet` | `id: INT64` sequential 0..100_000 | Triggers DELTA rule |
| `low_cardinality_strings.parquet` | `status: STRING` with 5 values, 100K rows | Triggers RLE_DICTIONARY |
| `uuids.parquet` | `id: STRING` UUID v4 format, 10K rows | Triggers PlainUuid rule |
| `high_entropy.parquet` | `blob: BINARY` random bytes 32-len, 10K rows | Triggers UNCOMPRESSED recommendation |
| `high_cardinality_floats.parquet` | `value: DOUBLE` random f64, 100K rows | Triggers BYTE_STREAM_SPLIT |
| `no_statistics.parquet` | `x: INT32` 10K rows, `EnabledStatistics::None` | Tests absent-stats path |
| `multi_column.parquet` | 6 columns: id(INT64 seq), status(STRING 5-val), score(DOUBLE random), ts(INT64 timestamp seq), name(STRING UUID), flag(BOOLEAN) | Integration tests |
| `nested_type.parquet` | `tags: LIST<STRING>`, `meta: MAP<STRING,INT32>` | Tests apply command round-trip |

All fixtures use `WriterVersion::PARQUET_2_0`, row group size 50K rows, SNAPPY codec (to give recommendations something to improve).

**`tests/integration/test_info.rs`:**
- `test_info_multi_column`: call `py_info_file` on `multi_column.parquet`, parse JSON, assert correct column count, compression_ratio > 1.0 for all columns
- `test_info_no_statistics`: call on `no_statistics.parquet`, assert `statistics_available=false` for all columns

**`tests/integration/test_tune.rs` — insta snapshot tests:**
- `test_tune_monotonic_ints`: call `py_tune_file("tests/fixtures/monotonic_ints.parquet", "unknown", "balanced", 100_000)`, use `insta::assert_json_snapshot!` on the result. The snapshot must show `recommended_encoding: "DELTA_BINARY_PACKED"` for the `id` column.
- `test_tune_low_cardinality_strings`: snapshot must show `recommended_encoding: "RLE_DICTIONARY"` for `status` column, `impact_stars >= 4`
- `test_tune_uuids`: snapshot must show `recommended_encoding: "PLAIN"` and `encoding_rule_fired: "PlainUuid"`
- `test_tune_multi_column`: full snapshot of all 6 columns

**`insta` configuration:** Add `[package.metadata.insta]` to `Cargo.toml` with `review = true` so changes require explicit `cargo insta review` approval.

**Must not touch:** Any implementation files in `src/`. Only creates test infrastructure and fixture generator.

---

### V01 — Validate Milestone v0.1

**Assign to:** Validation agent

**Inputs required:** T01–T09 all complete and passing.

**Run these checks — report PASS/FAIL for each:**

**Build checks:**
- [ ] `maturin develop` completes without warnings or errors
- [ ] `cargo test` passes (all tests, no ignored failures)
- [ ] `cargo test -- --include-ignored` passes after fixture generation
- [ ] `cargo run --example gen_fixtures` creates all 8 fixture files in `tests/fixtures/`
- [ ] `cargo insta test` passes (all snapshots match)

**CLI checks:**
- [ ] `autoparq --help` lists `info` and `tune` subcommands
- [ ] `autoparq info --help` shows `--output` and `--columns` flags
- [ ] `autoparq tune --help` shows all flags: `--engine`, `--priority`, `--output`, `--sample-rows`, `--min-improvement`
- [ ] `autoparq info tests/fixtures/multi_column.parquet` renders without error, output ≤ 120 cols wide
- [ ] `autoparq info tests/fixtures/multi_column.parquet --output json` is valid JSON
- [ ] `autoparq info tests/fixtures/no_statistics.parquet` shows `—` for null/min/max and footnote
- [ ] `autoparq info /nonexistent.parquet` exits with code 2
- [ ] `autoparq tune tests/fixtures/low_cardinality_strings.parquet` exits with code 1 (improvement available)
- [ ] `autoparq tune tests/fixtures/low_cardinality_strings.parquet --output json` is valid JSON with all required fields from PRD section 6.4

**CLAUDE.md compliance:**
- [ ] No `unwrap()` calls in `src/profiler/` or `src/recommender/` (grep check)
- [ ] No `panic!` in library code (grep check — allowed only in tests)
- [ ] All public functions in `src/` return `Result<T, AutoparqError>` (spot check 5 functions)
- [ ] `src/lib.rs` uses `Bound<PyModule>` not `&PyModule` (PyO3 new API)
- [ ] `python/autoparq/cli.py` has no business logic — only arg parsing + calls to Rust + rendering

**PRD compliance:**
- [ ] `info` command output includes all columns from PRD section 6.1 table
- [ ] `tune` JSON output includes `predicted_size_reduction_pct`, `predicted_read_speedup`, `confidence`, `columns[*].reason_brief`, `columns[*].recommended_encoding`, `columns[*].recommended_codec`
- [ ] All predictions labeled `"estimated"` in text output
- [ ] Exit code 0/1/2 semantics work as specified in PRD section 6.5
- [ ] Encoding rule priority order matches CLAUDE.md exactly (verify with `monotonic_ints.parquet` → DELTA, `low_cardinality_strings.parquet` → RLE_DICTIONARY, `uuids.parquet` → PLAIN)

**Output:** A `validation/v01_report.md` file with PASS/FAIL for every check above and details on any failures.

---

## Milestone v0.2 — Engine Awareness and Confidence Tiers

---

### T10 — Engine Compatibility Matrix

**Assign to:** Rust data agent

**Inputs required:** T07 complete (`Priority`, `Engine`, `CodecRecommendation` types defined).

**Files to create:**
```
src/recommender/engine.rs
```

**Files to modify:**
```
src/recommender/mod.rs   (add pub mod engine;)
```

**Structs to define:**
```rust
pub struct EngineSupport {
    pub supported: bool,
    pub min_version: Option<String>,   // e.g. "3.2.0"
    pub notes: Option<String>,
}
```

**Functions to implement:**

```rust
pub fn check_codec_compatibility(engine: &Engine, codec: &str) -> EngineSupport
pub fn check_encoding_compatibility(engine: &Engine, encoding: &str) -> EngineSupport
pub fn apply_engine_overrides(rec: &mut CodecRecommendation, engine: &Engine)
```

**Compatibility data — implement as match tables, not external files:**

Codec support:

| Codec | Spark | DuckDB | ClickHouse | Polars | Pandas | Unknown |
|-------|-------|--------|------------|--------|--------|---------|
| SNAPPY | all | ✓ | ✓ | ✓ | ✓ | ✓ |
| ZSTD | 3.2+ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LZ4 | 3.3+ | ✓ | ✓ | ✓ | ✓ | ✓ |
| BROTLI | 3.3+ | ✓ | ✗ | ✓ | ✓ | — |
| GZIP | all | ✓ | ✗ | ✓ | ✓ | ✓ |
| UNCOMPRESSED | all | ✓ | ✓ | ✓ | ✓ | ✓ |

Encoding support:
- `DELTA_BINARY_PACKED`: Spark 3.2+, all others ✓
- `BYTE_STREAM_SPLIT`: all engines ✓
- `RLE_DICTIONARY`: all engines ✓

`apply_engine_overrides` behavior:
- If `engine == Spark` and `codec == "ZSTD"` and no `min_version` check passes (treat as unversioned):
  - Change codec to `"SNAPPY"`, append Info caveat: `"Downgraded ZSTD→SNAPPY for Spark compatibility. Use --engine spark:3.2+ to keep ZSTD."`
- If `engine == ClickHouse` and `codec == "BROTLI"`:
  - Change to `"ZSTD"` level 3, append Warning caveat: `"ClickHouse does not support BROTLI for Parquet; using ZSTD"`
- If `engine == ClickHouse` and `codec == "GZIP"`:
  - Append Warning caveat: `"ClickHouse does not support GZIP for Parquet import"`

**Unit tests:**
- `test_spark_zstd_unsupported`: `check_codec_compatibility(Spark, "ZSTD")` → `supported=true, min_version=Some("3.2.0")`
- `test_clickhouse_brotli_unsupported`: `check_codec_compatibility(ClickHouse, "BROTLI")` → `supported=false`
- `test_apply_overrides_spark_zstd`: after override, codec changed to SNAPPY and caveat added
- `test_delta_spark_min_version`: `check_encoding_compatibility(Spark, "DELTA_BINARY_PACKED")` → `min_version=Some("3.2.0")`

**Must not touch:** `src/recommender/encoding.rs`, `src/recommender/codec.rs`, `python/`, `src/profiler/`

---

### T11 — Confidence Tiers + JSON Output Mode

**Assign to:** Rust/Python integration agent

**Inputs required:** T08, T10 complete.

**Files to modify:**
```
src/tuner.rs       (update TuneReport and orchestration to call apply_engine_overrides)
src/lib.rs         (update py_tune_file signature to accept engine, apply overrides)
python/autoparq/cli.py   (wire --engine flag to py_tune_file)
python/autoparq/render.py (add confidence tier display)
```

**Changes to `py_tune_file`:**
- After `recommend_codec`, call `apply_engine_overrides(&mut codec_rec, &engine)` for each column
- Add `engine: &str` parameter (was already there from T08 but now actually routes to T10 logic)
- `ColumnRecommendation` must include `engine_compatibility: Option<String>` derived from `check_encoding_compatibility` (e.g. `"spark>=3.2"`, `None` if no version restriction)

**JSON output schema additions** (must match PRD section 6.4 exactly):
```json
{
  "file": "...",
  "engine": "spark",
  "priority": "balanced",
  "sample_fraction": 0.042,
  "confidence": "High",
  "predicted_size_reduction_pct": 67.2,
  "predicted_read_speedup": 2.1,
  "columns": [{
    "name": "...",
    "recommended_encoding": "...",
    "recommended_codec": "...",
    "codec_level": 3,
    "impact_stars": 4,
    "reason_brief": "...",
    "confidence": "High",
    "confidence_reason": "...",
    "engine_compatibility": "spark>=3.2",
    "caveats": []
  }],
  "caveats": [],
  "options": { "A": {}, "B": {}, "C": {} }
}
```
Note: `options.A/B/C` can be stub `{}` objects in T11 — they are filled in T14.

**Confidence tier display in `render.py`:**
- Per-column: show confidence after codec: e.g. `ZSTD:3 [HIGH]`; use rich colors: High=green, Medium=yellow, Low=red
- Summary line updated: `"Estimated impact: -{:.0f}% size ({} confidence)"`

**Unit tests in `src/recommender/engine.rs`:** Already required in T10.

**Integration test additions in `tests/integration/test_tune.rs`:**
- `test_tune_spark_engine`: `py_tune_file("...multi_column.parquet", "spark", "balanced", 100_000)` — assert `columns[*].caveats` contains SNAPPY downgrade note for any column that would have gotten ZSTD

**Must not touch:** `src/recommender/encoding.rs`, `src/profiler/`

---

### T12 — Exit Codes + CI Integration Test

**Assign to:** Python testing agent

**Inputs required:** T11 complete.

**Files to modify:**
```
python/autoparq/cli.py   (finalize exit code logic)
```

**Files to create:**
```
tests/python/test_cli_exit_codes.py
```

**Exit code implementation in `cli.py`** — must match exactly:
```python
try:
    report = json.loads(_lib.py_tune_file(...))
    render_or_print(report, output)
    if report["predicted_size_reduction_pct"] >= min_improvement:
        raise typer.Exit(1)
    raise typer.Exit(0)
except typer.Exit:
    raise
except Exception as e:
    typer.echo(f"Error: {e}", err=True)
    raise typer.Exit(2)
```
- `typer.Exit` must be re-raised (not caught) — otherwise the exit code is lost.
- The `info` command always exits 0 on success, 2 on error.

**`tests/python/test_cli_exit_codes.py`** using `subprocess.run`:
- `test_tune_exit_1_improvement_available`: run tune on `low_cardinality_strings.parquet` (PLAIN encoded, 5-value STRING column) → assert returncode == 1
- `test_tune_exit_0_already_optimal`: create a pre-tuned fixture (written with RLE_DICTIONARY + ZSTD) → assert returncode == 0
- `test_tune_exit_2_bad_file`: run tune on `/nonexistent.parquet` → assert returncode == 2
- `test_info_exit_0`: run info on any valid fixture → assert returncode == 0
- `test_info_exit_2_bad_file`: run info on `/nonexistent.parquet` → assert returncode == 2

Add `pytest` and `pytest-subprocess` (or just `subprocess`) to `pyproject.toml` dev dependencies.

**Must not touch:** `src/`, Rust code

---

### V02 — Validate Milestone v0.2

**Assign to:** Validation agent

**Inputs required:** T10–T12 complete.

**Run these checks:**

**Build and test:**
- [ ] `cargo test` still passes (no regressions from T10–T12 changes)
- [ ] `pytest tests/python/` passes (all exit code tests)

**Engine compatibility:**
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --engine spark --output json` → all codecs are SNAPPY (Spark safety)
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --engine spark --output json` → JSON contains caveats with "ZSTD requires Spark 3.2+" message
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --engine duckdb --output json` → codecs include ZSTD (DuckDB supports it)
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --engine clickhouse --output json` → no BROTLI or GZIP codecs in output

**Confidence tiers:**
- [ ] Tuning `monotonic_ints.parquet` (100K rows) shows `confidence: "High"` for id column
- [ ] A tiny fixture (<1000 rows) shows `confidence: "Low"`

**JSON schema:**
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --output json` output has all fields listed in PRD section 6.4 (check with `jq` or Python json.loads)
- [ ] `options` key exists in JSON (even if stub)

**Exit codes:**
- [ ] pytest exit code tests all pass

**PRD compliance:**
- [ ] Engine flag values `spark`, `duckdb`, `polars`, `clickhouse`, `pandas`, `unknown` all accepted without error
- [ ] `--engine unknown` produces output without engine-specific caveats

**Output:** `validation/v02_report.md`

---

## Milestone v0.3 — Explainability and Option Bundles

---

### T13 — Full Explain Mode

**Assign to:** Rust/Python agent

**Inputs required:** T11 complete.

**Files to modify:**
```
src/tuner.rs           (add FullExplain struct and population logic)
src/lib.rs             (add explain parameter to py_tune_file)
python/autoparq/cli.py (add --explain flag)
python/autoparq/render.py (add full explain rendering)
```

**New structs in `src/tuner.rs`:**
```rust
pub struct RuleEvaluation {
    pub rule_name: String,
    pub evaluated: bool,
    pub fired: bool,
    pub threshold: String,
    pub actual_value: String,
    pub outcome: String,
}

pub struct AlternativeExplain {
    pub encoding: String,
    pub rejected_reason: String,
}

pub struct FullExplain {
    pub raw_stats: std::collections::BTreeMap<String, serde_json::Value>,
    pub reasoning_chain: Vec<RuleEvaluation>,
    pub alternatives_considered: Vec<AlternativeExplain>,
    pub engine_compatibility: Option<String>,
    pub teach_yourself: String,
}
```

Add `pub full_explain: Option<FullExplain>` to `ColumnRecommendation`. Only populated when `explain == "full"`.

**`teach_yourself` strings — hardcoded per `RuleName`:**
- `BooleanRle`: `"BOOLEAN columns in Parquet use run-length encoding automatically. The only tuning lever is the codec on top."`
- `DeltaMonotonic`: `"DELTA_BINARY_PACKED stores differences between consecutive values instead of the values themselves. For sorted integers (timestamps, auto-increment IDs), deltas are tiny and pack into very few bits. Rule of thumb: use DELTA on any monotonically increasing integer column."`
- `RleDictionary`: `"Dictionary encoding stores each distinct value once and replaces data values with small integer indices. When cardinality is low (few distinct values), these indices compress extremely well with run-length encoding. Rule of thumb: if cardinality < 10% of row count and the dictionary fits in ~512KB, use dictionary encoding."`
- `ByteStreamSplit`: `"BYTE_STREAM_SPLIT deinterleaves the bytes of floating-point values — writing all MSBs together, then all next bytes, etc. For physically-related floats (measurements in a range), this groups similar bytes together, improving codec compression by 10–30%."`
- `PlainUuid`: `"UUID columns have cardinality equal to row count, so dictionary encoding would require storing every UUID in the dictionary — as large as the original column. PLAIN with ZSTD is optimal for high-cardinality string columns."`
- `PlainDefault`: `"No specific pattern was detected. PLAIN encoding with a byte-oriented codec (ZSTD) is the safe baseline."`

**`py_tune_file` signature change:** Add `explain: &str` parameter (`"brief"` or `"full"`). Only compute `FullExplain` when `explain == "full"`.

**`render.py` — `render_tune_text` update:**
- When `explain == "full"`: after the standard "Why" block, add per-column expand panels using `rich.Panel`:
  - Raw stats table (all `ColumnProfile` numeric fields)
  - Reasoning chain table: Rule | Evaluated | Fired | Threshold | Actual Value
  - Alternatives considered
  - Engine compat note
  - "Teach yourself" in a dim italic panel

**CLI change:**
- Add `--explain` option with choices `brief`/`full`, default `brief`

**Snapshot test additions in `test_tune.rs`:**
- `test_tune_full_explain_monotonic`: call with `explain="full"`, assert `full_explain` is populated and `reasoning_chain` contains an entry for `DeltaMonotonic` with `fired=true`

**Must not touch:** `src/profiler/`, `src/recommender/`

---

### T14 — Three Option Bundles (A/B/C)

**Assign to:** Rust agent (can run in parallel with T13)

**Inputs required:** T11 complete. T13's struct changes must NOT conflict — T14 only modifies `TuneReport`, not `ColumnRecommendation`.

**Files to modify:**
```
src/tuner.rs   (add Bundle struct and generate_option_bundles fn)
```

**New structs in `src/tuner.rs`:**
```rust
pub struct Bundle {
    pub label: String,           // "Balanced", "Smallest File", "Fastest Reads"
    pub codec_description: String,
    pub estimated_size_bytes: u64,
    pub estimated_size_reduction_pct: f64,
    pub estimated_read_speedup: f64,
    pub python_snippet: String,
    pub caveats: Vec<Caveat>,
    pub note: String,            // always: "estimated (not measured) — use autoparq bench to validate"
}

pub struct OptionBundles {
    pub a: Bundle,   // Balanced (mirrors default recommendations)
    pub b: Bundle,   // Smallest File (priority=Size override)
    pub c: Bundle,   // Fastest Reads (priority=Speed override)
}
```

Add `pub options: OptionBundles` to `TuneReport`.

**`fn generate_option_bundles`:**
- Bundle A: use existing `TuneReport.columns` recommendations (already computed with user's priority)
- Bundle B: re-run `recommend_codec` for each column with `Priority::Size` override (ZSTD:6 everywhere)
- Bundle C: re-run `recommend_codec` for each column with `Priority::Speed` override (LZ4 where safe, SNAPPY otherwise)
- Each bundle's `estimated_size_bytes` computed from `file_size_bytes * (1.0 - estimated_size_reduction_pct/100.0)`
- Each bundle's `estimated_read_speedup` heuristic:
  - Bundle B (ZSTD:6): speedup = 0.8 (slightly slower reads than ZSTD:3)
  - Bundle C (LZ4/SNAPPY): speedup = base_speedup * 1.3
  - Bundle A: speedup = `TuneReport.predicted_read_speedup`

**`render.py` update:** After the per-column table, add a "Ranked Options" panel showing all three bundles with size and speed estimates, a `[RECOMMENDED]` badge on Bundle A.

**JSON output:** `options.a`, `options.b`, `options.c` must now be fully populated (not stubs).

**Unit test:**
- `test_bundle_b_uses_zstd6`: generate bundles with `priority=Balanced`, assert Bundle B codec is `ZSTD` level 6
- `test_bundle_c_no_delta_lz4_warning`: if any column uses DELTA encoding, Bundle C must include the parquet-go caveat

**Must not touch:** `src/profiler/`, `src/recommender/encoding.rs`, `src/recommender/codec.rs`

---

### T15 — Row Group Size Guidance + Sort Order Advisory

**Assign to:** Rust agent (can run in parallel with T13 and T14)

**Inputs required:** T08 complete. T13/T14 may run in parallel.

**Files to create:**
```
src/advisor.rs
```

**Files to modify:**
```
src/lib.rs      (add mod advisor;)
src/tuner.rs    (add advisories to TuneReport)
```

**Structs in `src/advisor.rs`:**
```rust
pub struct RowGroupAdvisory {
    pub current_avg_mb: f64,
    pub current_min_mb: f64,
    pub current_max_mb: f64,
    pub recommended_range_mb: (f64, f64),
    pub workload_label: String,
    pub is_within_recommendation: bool,
    pub advice: String,
}

pub struct SortOrderAdvisory {
    pub declared_sort_columns: Vec<String>,
    pub inferred_sort_candidates: Vec<String>,  // columns with monotonicity_score > 0.95
    pub advice: String,
}
```

**Functions:**
```rust
pub fn analyze_row_groups(profile: &FileProfile, engine: &Engine) -> RowGroupAdvisory
pub fn detect_sort_order(
    profile: &FileProfile,
    column_profiles: &[ColumnProfile],
) -> SortOrderAdvisory
```

**`analyze_row_groups` logic:**
- Compute avg/min/max from `profile.row_group_compressed_bytes`
- Recommended range by engine:
  - DuckDB: 64–128 MB
  - Spark: 128–512 MB
  - ClickHouse: 64–256 MB (import use case)
  - Polars/Pandas: 64–128 MB
  - Unknown: 64–256 MB (wide range)
- `is_within_recommendation`: true if avg is within range
- `advice`: human-readable string, e.g. `"Current avg row group 12 MB is below the 64–128 MB recommendation for DuckDB. Small row groups reduce compression effectiveness and increase predicate pushdown overhead."`

**`detect_sort_order` logic:**
- Check `profile` for declared sort columns via `RowGroupMetaData::sorting_columns()` (return column names if present)
- For each `ColumnProfile`, if `monotonicity_score > 0.95` AND column is INT64/TIMESTAMP: add to `inferred_sort_candidates`
- `advice`: if `declared_sort_columns` empty but `inferred_sort_candidates` non-empty: `"Column '{name}' appears sorted (monotonicity_score={:.2}) but no sort order is declared in the file. Declaring sort order enables better predicate pushdown."`

Add `row_group_advisory: RowGroupAdvisory` and `sort_advisory: SortOrderAdvisory` to `TuneReport`.

**`render.py` update:** Add an "Advisories" section after the column table, rendering both advisories if non-trivial (skip if row groups are within range and no sort candidates).

**Unit tests:**
- `test_rg_advisory_duckdb_too_small`: 12 MB avg, engine=DuckDB → `is_within_recommendation=false`
- `test_rg_advisory_within_range`: 100 MB avg, engine=Spark → `is_within_recommendation=true`
- `test_sort_inferred`: column profile with `monotonicity_score=0.97`, INT64 → appears in `inferred_sort_candidates`
- `test_sort_not_inferred_float`: FLOAT column with monotonicity 0.97 → does NOT appear (floats excluded)

**Must not touch:** `src/profiler/`, `src/recommender/`

---

### V03 — Validate Milestone v0.3

**Assign to:** Validation agent

**Inputs required:** T13, T14, T15 complete.

**Run these checks:**

**Explain mode:**
- [ ] `autoparq tune tests/fixtures/monotonic_ints.parquet --explain full` renders without error
- [ ] Full explain output contains "Reasoning chain" section
- [ ] `teach_yourself` text appears for DELTA rule
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --explain full --output json` → each column has `full_explain` object populated
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --explain brief` does NOT include `full_explain` in JSON (field is null/absent)

**Option bundles:**
- [ ] `autoparq tune tests/fixtures/multi_column.parquet --output json | jq '.options'` returns object with `a`, `b`, `c` keys, all fully populated
- [ ] Bundle B has higher estimated compression than Bundle A
- [ ] Bundle C has higher estimated read speedup than Bundle A
- [ ] All bundle estimates contain the word "estimated" in the `note` field
- [ ] `render` output shows "Ranked Options" panel

**Advisories:**
- [ ] `autoparq tune tests/fixtures/monotonic_ints.parquet --engine duckdb --output json | jq '.sort_advisory'` → `inferred_sort_candidates` contains `"id"`
- [ ] `autoparq tune tests/fixtures/monotonic_ints.parquet --engine duckdb --output json | jq '.row_group_advisory'` → advisory present with advice string

**Regression check:**
- [ ] `cargo test` still passes
- [ ] `cargo insta test` still passes (or new snapshots reviewed)
- [ ] All v0.1 and v0.2 exit code tests still pass

**Output:** `validation/v03_report.md`

---

## Milestone v0.4 — Bench and Apply Commands

---

### T16 — `autoparq bench` Command

**Assign to:** Rust/Python agent

**Inputs required:** T01, T02, T04 complete.

**Files to create:**
```
src/bench.rs
```

**Files to modify:**
```
src/lib.rs             (expose py_bench_column, add mod bench)
python/autoparq/cli.py (add bench command)
python/autoparq/render.py (add render_bench)
```

**Type validity lookup table** — implement as a `fn valid_combos(physical_type: &str) -> Vec<(&'static str, Option<i32>)>` returning `(encoding, codec_level_or_none)` pairs:
- INT32/INT64: encodings: `["PLAIN", "DELTA_BINARY_PACKED", "RLE_DICTIONARY"]`
- BYTE_ARRAY: encodings: `["PLAIN", "RLE_DICTIONARY"]`
- FLOAT/DOUBLE: encodings: `["PLAIN", "BYTE_STREAM_SPLIT"]`
- BOOLEAN: encodings: `["PLAIN"]` (RLE is automatic, not a WriterProperties setting)
- Others: `["PLAIN"]`
- Codecs (all types): `SNAPPY`, `ZSTD:1`, `ZSTD:3`, `ZSTD:6`, `LZ4`, `UNCOMPRESSED`

**`fn benchmark_column`:**
```rust
pub struct BenchEntry {
    pub encoding: String,
    pub codec: String,
    pub codec_level: Option<i32>,
    pub compressed_bytes: usize,
    pub write_ms: u64,
    pub read_ms: u64,
    pub compression_ratio: f64,
}

pub struct BenchResult {
    pub column_name: String,
    pub physical_type: String,
    pub uncompressed_bytes: usize,
    pub entries: Vec<BenchEntry>,  // sorted by compressed_bytes ascending
}

pub fn benchmark_column(
    path: &Path,
    column_name: &str,
    combos: &[(String, Option<i32>)],  // (codec, level)
    encodings: &[String],
) -> Result<BenchResult, AutoparqError>
```

For each `(encoding, codec, level)` combination:
1. `sample_column(path, column_name, 0, 500_000)` → `ColumnSample`
2. Build `WriterProperties` with `set_column_encoding(col_path, encoding)` and `set_column_compression(col_path, compression)`
3. Write to `Vec<u8>` via `ArrowWriter::try_new(cursor, schema, Some(props))`, measure with `Instant::now()`
4. Read back via `ParquetRecordBatchReaderBuilder::try_new(Cursor::new(bytes))`, measure read time
5. Record `compressed_bytes = bytes.len()`, `write_ms`, `read_ms`

Return error `UnsupportedType` if user requests an invalid encoding/type combination.

**Python CLI `@app.command("bench")`:**
- `file: Path` argument
- `--column` (required String)
- `--codecs` (optional String, comma-separated like `"zstd:3,lz4,snappy"`, default: all 6 standard codecs)
- `--encodings` (optional String, comma-separated, default: all valid for the column's type)
- `--measure` (optional, choices: `read`/`write`/`size`/`all`, default: `all`)
- On completion: call `render.render_bench(result, measure)`

**`render_bench(result, measure)`:**
- Rich table sorted by `compressed_bytes` ascending
- Columns: `Encoding | Codec | Compressed | Ratio | Write ms | Read ms`
- Highlight the winner row in each of: smallest, fastest-read, fastest-write
- Note at bottom: `"Results from in-memory benchmark on first row group sample. Actual I/O performance may differ."`

**Must not touch:** `src/recommender/`, `src/tuner.rs`

---

### T17 — `autoparq apply` Command

**Assign to:** Rust/Python agent (can run in parallel with T16)

**Inputs required:** T08 complete (full tune pipeline exists).

**Files to create:**
```
src/apply.rs
```

**Files to modify:**
```
src/lib.rs             (expose py_apply_file, add mod apply)
python/autoparq/cli.py (add apply command)
```

**`fn rewrite_file`:**
```rust
pub fn rewrite_file(
    input_path: &Path,
    output_path: &Path,
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
    progress_callback: Option<Box<dyn Fn(i64, i64) + Send>>,
) -> Result<RewriteResult, AutoparqError>

pub struct RewriteResult {
    pub rows_written: i64,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    pub actual_reduction_pct: f64,
    pub elapsed_ms: u64,
}
```

Implementation:
1. Run full tune pipeline (same as `py_tune_file`) to get recommendations
2. Build `WriterProperties` with per-column `set_column_encoding` + `set_column_compression` for each column
3. Write to `tempfile::NamedTempFile::new_in(output_path.parent())`
4. Read input in batches of 65_536 rows via `ParquetRecordBatchReaderBuilder`
5. Write via `ArrowWriter::try_new(temp_file, schema, Some(props))`
6. Call `progress_callback(rows_written, total_rows)` after each batch if provided
7. After all rows written, call `writer.close()`
8. Call `temp_file.persist(output_path)` for atomic rename

**Python CLI `@app.command("apply")`:**
- `file: Path` argument
- `--output` (required Path, the destination file)
- `--in-place` (bool flag, default False; if True, sets output = input)
- `--engine`, `--priority` (same as tune command)
- Guard: if `output == file` and not `--in-place`, print error and exit 2
- Guard: if `output` exists and not `--in-place`, print error and exit 2 (never silently overwrite)
- Progress bar via `rich.progress.Progress` during rewrite
- On completion: print `RewriteResult` summary: `"Rewrote {rows} rows: {input_size} → {output_size} ({reduction_pct:.1f}% reduction) in {elapsed_ms}ms"`

**Integration test** (add to `tests/integration/test_apply.rs`):
- `test_apply_roundtrip`: apply on `multi_column.parquet` → output file readable, same row count
- `test_apply_refuses_overwrite`: apply with output=input and no --in-place → error exit
- `test_apply_nested_types`: apply on `nested_type.parquet` → output readable, schema preserved

**Must not touch:** `src/profiler/stats.rs`, `src/recommender/`

---

### V04 — Validate Milestone v0.4

**Assign to:** Validation agent

**Inputs required:** T16, T17 complete.

**Run these checks:**

**Bench command:**
- [ ] `autoparq bench tests/fixtures/multi_column.parquet --column status` runs without error (status is low-cardinality STRING)
- [ ] `autoparq bench tests/fixtures/multi_column.parquet --column status --output json` is valid JSON with `entries` array
- [ ] Bench output contains note about in-memory results
- [ ] `autoparq bench tests/fixtures/multi_column.parquet --column status --codecs zstd:3,snappy` only tests those two codecs
- [ ] Requesting an invalid encoding for the column type produces a user-friendly error (not a panic)

**Apply command:**
- [ ] `autoparq apply tests/fixtures/multi_column.parquet --output /tmp/test_out.parquet` creates output file
- [ ] Output file is readable: `autoparq info /tmp/test_out.parquet` succeeds
- [ ] Output file has same row count as input (verify via JSON output of info)
- [ ] `autoparq apply tests/fixtures/multi_column.parquet --output tests/fixtures/multi_column.parquet` exits with code 2 (refuses overwrite without --in-place)
- [ ] `autoparq apply tests/fixtures/nested_type.parquet --output /tmp/nested_out.parquet` succeeds (nested type round-trip)

**Regression:**
- [ ] All v0.1–v0.3 validation checks still pass
- [ ] `cargo test` passes including new apply integration tests

**Output:** `validation/v04_report.md`

---

## Milestone v1.0 — Polish and Distribution

---

### T18 — PyPI Wheel CI Pipeline

**Assign to:** DevOps/CI agent

**Inputs required:** T01–T17 complete.

**Files to create:**
```
.github/workflows/release.yml
.github/workflows/ci.yml
```

**`ci.yml` — runs on every push/PR:**
```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test
      - run: maturin develop
      - run: pytest tests/python/
```

**`release.yml` — runs on `v*` tags, builds wheels:**

Matrix:
```
Linux x86_64:   ubuntu-latest, target: x86_64-unknown-linux-gnu, manylinux: auto
Linux aarch64:  ubuntu-latest, target: aarch64-unknown-linux-gnu, manylinux: auto, args: --zig
macOS x86_64:   macos-13,      target: x86_64-apple-darwin
macOS aarch64:  macos-14,      target: aarch64-apple-darwin
Windows x86_64: windows-latest,target: x86_64-pc-windows-msvc
```

Each matrix entry:
1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable` with target
3. `PyO3/maturin-action@v1` with `args: "--release --strip"`
4. `actions/upload-artifact` with wheel

Final job: `publish` (needs all matrix jobs):
1. `actions/download-artifact`
2. `pypa/gh-action-pypi-publish@v1` using `PYPI_API_TOKEN` secret

**Linux aarch64 risk mitigation:** Add a test step before the aarch64 job that runs `maturin build --zig --target aarch64-unknown-linux-gnu` and validates the wheel is non-empty. If this fails, the job should fail loudly (not silently produce a broken wheel).

**Must not touch:** Any source files in `src/` or `python/`

---

### T19 — Criterion Benchmarks

**Assign to:** Rust benchmarking agent (can run in parallel with T18)

**Inputs required:** T02, T04, T05 complete.

**Files to create:**
```
benches/profiler_throughput.rs
```

**Files to modify:**
```
Cargo.toml   (add criterion dev dependency, add [[bench]] entry)
```

**Benchmark groups:**

```rust
fn bench_metadata_parse(c: &mut Criterion) {
    // fixture: tests/fixtures/multi_column.parquet
    // target: < 100ms
    c.bench_function("metadata_parse", |b| {
        b.iter(|| read_file_metadata(black_box(path)))
    });
}

fn bench_sample_column(c: &mut Criterion) {
    // fixture: generate a 1M-row single-column INT64 file in temp at bench start
    // target: < 2s for 1M rows
    c.bench_function("sample_column_1m_rows", |b| {
        b.iter(|| sample_column(path, "id", 0, 1_000_000))
    });
}

fn bench_profile_column_int64(c: &mut Criterion) {
    // pre-sample 2M rows into an ArrayRef, then benchmark profile_column only
    c.bench_function("profile_column_int64_2m", |b| {
        b.iter(|| profile_column(black_box(&sample)))
    });
}

fn bench_profile_column_string(c: &mut Criterion) {
    // low-cardinality STRING column, 2M rows
    c.bench_function("profile_column_string_2m", |b| {
        b.iter(|| profile_column(black_box(&sample)))
    });
}

fn bench_rayon_10_columns(c: &mut Criterion) {
    // multi_column.parquet, profile all columns in parallel vs sequential
    c.bench_function("rayon_parallel_10_cols", |b| { ... });
    c.bench_function("sequential_10_cols", |b| { ... });
}
```

**Acceptance criteria:**
- `cargo bench` runs without error
- `metadata_parse` result < 100ms (assert via criterion `measurement_time`)
- Rayon parallel benchmark must be faster than sequential (verified by criterion's output)

**Must not touch:** `src/` implementation files

---

### T20 — Documentation Site

**Assign to:** Documentation agent (can run in parallel with T18, T19)

**Inputs required:** T01–T17 complete (needs final CLI flag list).

**Files to create:**
```
docs/mkdocs.yml
docs/docs/index.md
docs/docs/commands/info.md
docs/docs/commands/tune.md
docs/docs/commands/bench.md
docs/docs/commands/apply.md
docs/docs/concepts/encodings.md
docs/docs/concepts/codecs.md
docs/docs/contributing.md
.github/workflows/docs.yml
```

**`mkdocs.yml`:** Use Material theme, navigation matching the files above.

**`index.md`:** Overview, install instructions (`pip install autoparq`), 30-second quickstart example showing `info` → `tune` → `apply` workflow.

**Each command page:** Full flag reference table (name, type, default, description), 2–3 annotated example invocations with sample output, common use cases.

**`concepts/encodings.md`:** Pull `teach_yourself` text from T13 for each rule. Add visual examples showing before/after byte sizes.

**`concepts/codecs.md`:** Codec comparison table (size, read speed, write speed, compatibility). When to use each.

**`contributing.md`:** Development setup: `maturin develop`, `cargo test`, `pytest tests/python/`, `cargo insta review`.

**`docs.yml`:** Deploy to GitHub Pages on push to `main`.

**Must not touch:** `src/`, `python/`

---

### V-FINAL — Final Evaluation

**Assign to:** Senior evaluation agent

**Inputs required:** T18–T20 complete. All previous validation reports exist in `validation/`.

**This agent must verify the complete system end-to-end against the PRD and CLAUDE.md. Run every check below. Report PASS/FAIL with evidence.**

**Completeness — all PRD section 6.1 commands exist:**
- [ ] `autoparq info` — PRD section 6.1
- [ ] `autoparq tune` — PRD section 6.1
- [ ] `autoparq bench` — PRD section 6.1
- [ ] `autoparq apply` — PRD section 6.1

**All PRD flags are implemented:**
- [ ] `tune`: `--engine`, `--priority`, `--explain`, `--output`, `--sample-rows`, `--min-improvement`
- [ ] `info`: `--output`, `--columns`
- [ ] `bench`: `--column`, `--codecs`, `--measure`
- [ ] `apply`: `--output`, `--in-place`, `--engine`, `--priority`

**End-to-end workflow test:**
- [ ] Generate a realistic test file: 1M rows, 8 columns (mix of types), all PLAIN + SNAPPY
- [ ] `autoparq info <file>` completes in < 200ms and shows correct schema
- [ ] `autoparq tune <file> --engine duckdb --priority balanced` completes in < 30s
- [ ] `autoparq tune <file> --output json` is valid JSON matching PRD section 6.4 schema exactly
- [ ] `autoparq tune <file> --explain full` renders all `teach_yourself` blocks
- [ ] `autoparq tune <file> --output json | jq '.options.b.estimated_size_reduction_pct'` returns a number > 0
- [ ] `autoparq bench <file> --column <int64_col>` shows DELTA_BINARY_PACKED in results
- [ ] `autoparq apply <file> --output /tmp/out.parquet --engine duckdb` produces a smaller file
- [ ] Output of apply is readable by `autoparq info`
- [ ] Exit code 1 when tuning an unoptimized file, exit 0 when tuning the apply output

**Recommendation quality spot-check:**
- [ ] Sequential INT64 column → DELTA_BINARY_PACKED recommended
- [ ] 5-value STRING column (100K rows) → RLE_DICTIONARY recommended
- [ ] UUID STRING column → PLAIN recommended with PlainUuid rule
- [ ] High-entropy BINARY column → UNCOMPRESSED recommended
- [ ] DOUBLE column with random floats → BYTE_STREAM_SPLIT recommended
- [ ] With `--engine spark`: all codecs are SNAPPY or include "Spark 3.2+" caveat

**Explanation quality spot-check:**
- [ ] Every `reason_brief` cites a specific statistic (grep for `=` in reason_brief values)
- [ ] `--explain full` output contains `teach_yourself` text for each fired rule
- [ ] Confidence tier `Low` appears for small files (<5K rows), `High` for large files (>100K rows)

**Output quality:**
- [ ] `info` text output fits in 120 columns (test with 80-char wide terminal simulation)
- [ ] Python snippet is syntactically valid Python (run `python -c "exec(open(snippet_file).read())"`)
- [ ] JSON output is valid JSON for all commands that support `--output json`
- [ ] All predictions in text output are labeled `[estimated]`

**CLAUDE.md compliance audit:**
- [ ] Zero `unwrap()` calls in `src/` outside `#[cfg(test)]` blocks
- [ ] Zero `panic!` calls in `src/` outside test blocks
- [ ] Zero `unsafe` blocks in `src/`
- [ ] All error types use `thiserror`
- [ ] `python/autoparq/cli.py` contains no heuristic logic (only routing)
- [ ] All Python functions are type-annotated

**CI:**
- [ ] `ci.yml` exists and has correct structure
- [ ] `release.yml` has matrix for all 5 platforms
- [ ] Wheel artifacts are generated for at least Linux x86_64 and macOS aarch64 (test locally with `maturin build`)

**Performance:**
- [ ] `autoparq info` on a 1 GB file completes in < 500ms
- [ ] `autoparq tune` on a 100 MB, 10-column file completes in < 10s

**Output:** `validation/final_evaluation_report.md` with every check above marked PASS/FAIL, plus a summary section: overall PASS/FAIL, list of any failing items, recommended fix actions for each failure.

---

## Parallelization Map

Tasks that can run concurrently (same milestone, no shared output files):

```
v0.1:  T04 ║ T03   (T04 and T03 both depend on T02 but touch different files)
v0.1:  T06 ║ T07   (T07 can start once T06 defines its structs)
v0.3:  T13 ║ T14 ║ T15
v0.4:  T16 ║ T17
v1.0:  T18 ║ T19 ║ T20
```

Tasks that are strictly sequential:
```
T01 → T02 → T04 → T05 → T06 → T07 → T08 → T09 → V01
V01 → T10 → T11 → T12 → V02
V02 → [T13, T14, T15 in parallel] → V03
V03 → [T16, T17 in parallel] → V04
V04 → [T18, T19, T20 in parallel] → V-FINAL
```
