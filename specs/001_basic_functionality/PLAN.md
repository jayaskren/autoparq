# autoparq — Implementation Plan

## v0.1 — Core Profiler and Basic Recommendations

### Phase 1: Cargo/pyproject scaffold with PyO3 + maturin
**What:** Initialize `Cargo.toml` as a `cdylib`+`rlib` crate with dependencies (`parquet`, `arrow`, `pyo3`, `rayon`, `thiserror`, `serde_json`, `regex`, `hyperloglog-rs`). Create `pyproject.toml` with maturin build config, `python/autoparq/` package stubs, `src/lib.rs` PyO3 module, `src/error.rs` with `thiserror` error enum. Confirm `maturin develop` + `import autoparq._lib` works.

**Confidence: 95%** — Well-trodden pattern; neutrino/vortex provide working local references.

**Dependencies:** None

---

### Phase 2: Parquet footer metadata reader (`src/profiler/metadata.rs`)
**What:** `fn read_file_metadata(path: &Path) -> Result<FileProfile>` using `ParquetMetaDataReader::new().parse_metadata(&file)` — footer only, no row scanning. Extract: file version, row count, created-by string, per-column physical/logical types, encodings, codec, compressed/uncompressed sizes, null counts, min/max from statistics. Aggregate null counts and min/max across all row groups.

`FileProfile` and `ColumnMetaSummary` are plain data structs with `#[derive(Serialize)]`. Min/max decoded to strings in Rust before crossing the PyO3 boundary (no raw byte slices in Python).

**Confidence: 90%** — API confirmed: `ColumnChunkMetaData::statistics()` returns `Option<Statistics>` with `null_count_opt()`, `min_bytes_opt()`, `max_bytes_opt()`. All safely optional.

**Dependencies:** Phase 1

---

### Phase 3: `autoparq info` command — Rust + Python CLI
**What:** Expose `py_info_file(path: &str) -> PyResult<String>` returning JSON. `cli.py`: `@app.command("info")` with `--output text|json` and `--columns` filter. `render.py`: `render_info(profile)` renders a rich table (file header panel + per-column table matching PRD section 6.1 layout). Footnote when statistics are absent.

**Confidence: 90%** — Clean architecture: all logic in Rust, all rendering in Python, JSON string as bridge.

**Dependencies:** Phases 1, 2

---

### Phase 4: Single row-group sampler (`src/profiler/sampler.rs`)
**What:** `fn sample_column(path: &Path, column_name: &str, max_rows: usize) -> Result<ColumnSample>` using `ParquetRecordBatchReaderBuilder::try_new(file)?.with_row_groups(vec![0]).with_limit(max_rows)` with `ProjectionMask::columns` to read one column at a time. Returns `ColumnSample { column_name, physical_type, array: ArrayRef }`.

Columns are read individually (not all at once) to avoid loading a full wide file into memory.

**Confidence: 85%** — `with_row_groups`, `with_limit`, `ProjectionMask::columns` all confirmed in parquet crate API.

**Risk (MEDIUM):** A 2M-row sample of a wide file loaded all at once could use 500MB+ RAM. Per-column loading via `ProjectionMask` mitigates this; each column is processed then dropped before the next is loaded.

**Dependencies:** Phases 1, 2

---

### Phase 5: Statistical profiling engine (`src/profiler/stats.rs`)
**What:** `fn profile_column(sample: &ColumnSample) -> ColumnProfile` operating on `ArrayRef`:
- **Cardinality:** HyperLogLog (p=14, ±0.81% error). Fall back to exact `HashSet` count when `sample_rows < 50_000`.
- **Monotonicity score:** `count(v[i] <= v[i+1]) / (n-1)` for INT32/INT64/TIMESTAMP. Nulls treated as "breaks" (excluded from both numerator and denominator).
- **Run-length score:** `count(v[i] == v[i-1]) / (n-1)`.
- **String length stats:** min/max/mean/stddev of byte length for BYTE_ARRAY columns.
- **UUID detection:** regex on 1000-value sample.
- **JSON detection:** `value.trim_start().starts_with('{')` heuristic on sample.
- **Byte entropy:** Shannon entropy `H = -sum(p * log2(p))` over byte frequency histogram.
- **Value histogram:** 32 equal-width buckets for numeric types.

**Confidence: 75%** — Arrow downcast pattern (`array.as_any().downcast_ref::<Int64Array>()`) is standard. The HyperLogLog crate API requires hashing to u64 first; use `ahash` for this.

**Risk (LOW-MEDIUM):** `hyperloglog-rs` crate API is slightly awkward for heterogeneous types. If integration is painful, `hyperloglog` 1.0.3 (older, simpler API) is a drop-in alternative.

**Dependencies:** Phases 1, 4

---

### Phase 6: Encoding recommendation rules (`src/recommender/encoding.rs`)
**What:** `fn recommend_encoding(profile: &ColumnProfile, meta: &ColumnMetaSummary) -> EncodingRecommendation` implementing the 6-rule priority chain from CLAUDE.md exactly. Returns `EncodingRecommendation { encoding, rule_fired: RuleName, reason_brief, confidence: ConfidenceTier }`.

`reason_brief` includes the specific triggering statistic: e.g. `"monotonicity_score=0.94 > threshold 0.90"`. `ConfidenceTier`: `Low` if `sample_rows < 100_000` or `sample_fraction < 0.02`; `Medium` if cardinality ratio is within 20% of the 0.10 threshold; `High` otherwise.

Unit tests use synthetic `ColumnProfile` values — no file I/O. Every rule gets at least two tests (fires / does not fire).

**Confidence: 95%** — Pure data transformation, fully unit-testable.

**Dependencies:** Phases 1, 5

---

### Phase 7: Codec selection (`src/recommender/codec.rs`)
**What:** `fn recommend_codec(profile: &ColumnProfile, encoding: &EncodingRecommendation, priority: Priority, engine: Engine) -> CodecRecommendation`. Implements the codec table from CLAUDE.md. Returns `CodecRecommendation { compression, reason_brief, caveats: Vec<Caveat> }`.

Named caveats: Spark version warning (ZSTD→SNAPPY downgrade), parquet-go + LZ4 + DELTA bug, entropy gate for pre-compressed data.

**Confidence: 95%** — Pure logic, no I/O.

**Dependencies:** Phases 1, 6

---

### Phase 8: `autoparq tune` command — Rust orchestrator + Python CLI
**What:** `py_tune_file(path, engine, priority, explain, sample_rows) -> PyResult<String>`:
1. `read_file_metadata` (footer)
2. `sample_column` per column via `rayon::par_iter`
3. `profile_column` per column (parallel)
4. `recommend_encoding` + `recommend_codec` per column
5. Build `TuneReport`, serialize to JSON

`TuneReport` fields: file summary, per-column recommendations, `predicted_size_reduction_pct`, `predicted_read_speedup`, confidence, caveats, Python snippet, Spark config.

`cli.py`: `@app.command("tune")` wires all flags. `render.py`: `render_tune_text(report)` renders full terminal output. `codegen.py`: `generate_python_snippet(report)` and `generate_spark_snippet(report)` emit valid code.

Exit codes: `raise typer.Exit(1)` when predicted improvement ≥ `--min-improvement`; `raise typer.Exit(2)` on exception; `raise typer.Exit(0)` otherwise.

**Confidence: 85%** for core path. **60%** for size/speedup predictions.

**Risk (MEDIUM):** Predicted size reduction is heuristic-based, not measured. Use a rule-based estimate (e.g., DELTA on a PLAIN-encoded timestamp column → predict 50% reduction; RLE_DICTIONARY on a 0.001% cardinality string column → predict 90% reduction). Label all predictions explicitly as `"estimated"`. Actual measurement belongs to `bench` (Phase 16).

**Dependencies:** Phases 1–7

---

### Phase 9: Test fixtures + unit/integration/snapshot tests
**What:** `cargo run --example gen_fixtures` generates small (<1 MB) parquet files in `tests/fixtures/`:
- `monotonic_ints.parquet` — triggers DELTA rule
- `low_cardinality_strings.parquet` — triggers RLE_DICTIONARY
- `uuids.parquet` — triggers PLAIN/UUID
- `high_entropy.parquet` — triggers UNCOMPRESSED
- `high_cardinality_floats.parquet` — triggers BYTE_STREAM_SPLIT
- `no_statistics.parquet` — absent statistics code path
- `multi_column.parquet` — integration test baseline

`insta` snapshot tests call `py_tune_file` on each fixture and snapshot the JSON. Heuristic changes require `cargo insta review` — prevents silent regressions.

**Confidence: 90%** — Pattern proven in neutrino fixture generators.

**Dependencies:** Phases 1–8

---

## v0.2 — Engine Awareness and Confidence Tiers

### Phase 10: Engine compatibility matrix (`src/recommender/engine.rs`)
**What:** Static compatibility matrix as data (`HashMap<(Engine, Codec), EngineSupport>`). `fn check_encoding_compatibility(engine, encoding) -> EngineSupport`. `fn apply_engine_overrides(rec: &mut CodecRecommendation, engine)` post-processes recommendations to enforce engine-safe codecs, appending caveats. Full matrix from PRD section 6.6 including DELTA_BINARY_PACKED minimum version (Spark 3.2+).

**Confidence: 95%** — Pure data/logic.

**Dependencies:** Phases 1, 7

---

### Phase 11: Confidence tiers + JSON output mode
**What:** Add `sample_fraction: f64` to `ColumnProfile`. Add `confidence: ConfidenceTier` and `confidence_reason: String` to `ColumnRecommendation`. Expose `--output json` via `py_tune_file` `output` parameter — JSON mode returns the struct directly; text mode renders via rich. JSON schema must match PRD section 6.4 exactly.

**Confidence: 90%** — Field additions and serialization switch.

**Dependencies:** Phases 8, 10

---

### Phase 12: Exit codes + CI integration test
**What:** Wire `raise typer.Exit(code=N)` in `cli.py`. Integration test using `subprocess.run` against known fixtures: `low_cardinality_strings.parquet` with defaults → exit 1; a pre-optimized fixture → exit 0. Document that exit code 1 is advisory (heuristic-based) until `bench` (v0.4) enables measurement-based gating.

**Confidence: 90%** — Typer exit codes are well-documented.

**Dependencies:** Phases 8, 11

---

## v0.3 — Explainability and Option Bundles

### Phase 13: Full explain mode (`--explain full`)
**What:** Add `full_explain: Option<FullExplain>` to `ColumnRecommendation`, populated only when `explain = "full"`. `FullExplain` contains: raw stats table, `Vec<RuleEvaluation>` (each rule: name, evaluated, fired, threshold, actual value), alternatives considered, engine compat note, `teach_yourself` string (educational prose per rule, hardcoded). `render.py` renders nested rich panels for full mode.

**Confidence: 90%** — Structural extension, no new algorithms.

**Risk (LOW):** `teach_yourself` strings can become stale when rules change. The `insta` snapshots from Phase 9 catch this mechanically.

**Dependencies:** Phases 8, 11

---

### Phase 14: Three option bundles (A/B/C)
**What:** `fn generate_option_bundles(columns: &[ColumnRecommendation], engine: Engine) -> OptionBundles` producing Balanced / SmallestFile / FastestReads bundles. Each bundle re-runs codec selection with the corresponding `Priority` override and produces its own `estimated_size_reduction_pct`, `estimated_read_speedup`, Python snippet, and caveats.

All estimates labeled `"estimated (not measured)"` in output. Bundle comparisons note: `"Use autoparq bench to measure actual tradeoffs."`

**Confidence: 65%** — Logic is straightforward; the risk is that relative estimates (e.g., "LZ4 is 30% faster than ZSTD:3") are CPU and data-dependent and could mislead users. The disclaimer mitigates this.

**Dependencies:** Phases 7, 8, 11

---

### Phase 15: Row group size guidance + sort order advisory
**What:**
- `fn analyze_row_groups(profile: &FileProfile, engine: Engine) -> RowGroupAdvisory`: compare current avg row group size to engine-appropriate target from PRD table; emit advisory with specific `advice: String`.
- `fn detect_sort_order(profile: &FileProfile, column_profiles: &[ColumnProfile]) -> SortOrderAdvisory`: check `RowGroupMetaData::sorting_columns()`; fall back to inferring from high monotonicity score (>0.95). Emit `inferred_sort_candidates` for columns that look sorted but don't have a declared sort key.

**Confidence: 90%** for sort detection (API confirmed). **65%** for row group sizing — workload inference from engine alone is a coarse heuristic.

**Dependencies:** Phases 2, 8

---

## v0.4 — Bench and Apply Commands

### Phase 16: `autoparq bench` command
**What:** `fn benchmark_column(path: &Path, column: &str, combos: &[(Encoding, Compression)]) -> BenchResult`. For each combo: read column via `ProjectionMask`, write to in-memory `Vec<u8>` via `ArrowWriter` with specified `WriterProperties`, read back, record compressed size, write ms, read ms.

`fn valid_combinations(physical_type: Type) -> Vec<(Encoding, Compression)>` enforces type-valid combos (e.g., `DELTA_BINARY_PACKED` only for INT/TIMESTAMP). Default combo set: `[zstd:1, zstd:3, zstd:6, lz4, snappy, uncompressed]` × valid encodings for the column type.

`render.py`: rich table sorted by `compressed_bytes` ascending by default.

**Confidence: 70%** — `ArrowWriter` + `ProjectionMask` confirmed. Main risks: (1) in-memory benchmarks don't reflect I/O-bound real workloads; (2) encoding×type validation matrix is non-trivial to implement correctly across all physical types.

**Mitigation:** Document that results are relative comparisons, not absolute throughput measurements. Build the type-validity table from the Parquet spec, not heuristics.

**Dependencies:** Phases 1, 2, 4

---

### Phase 17: `autoparq apply` command
**What:** `fn rewrite_file(input: &Path, output: &Path, recs: &[ColumnRecommendation]) -> Result<()>`. Build `WriterProperties` with per-column `set_column_encoding` + `set_column_compression`. Stream input via `ParquetRecordBatchReaderBuilder` in 64K-row batches, write via `ArrowWriter`. Use `tempfile::NamedTempFile` + `persist()` for `--in-place` (safe atomic rename on Linux; documented limitation on Windows with open files).

CLI refuses to overwrite input path without explicit `--in-place` flag.

**Confidence: 85%** — `ArrowWriter` + `WriterProperties` per-column control confirmed; pattern proven in neutrino.

**Risk (LOW-MEDIUM):** Nested Parquet types (MAP, LIST, STRUCT) must be faithfully round-tripped. Arrow schema carries full type info so this works for standard cases; add a nested-type fixture to Phase 9 tests.

**Dependencies:** Phases 1, 2, 6, 7, 8

---

## v1.0 — Polish and Distribution

### Phase 18: PyPI wheel CI pipeline (GitHub Actions)
**What:** `.github/workflows/release.yml` with matrix: Linux x86_64 (manylinux_2_28), Linux aarch64 (maturin + zig cross-compile), macOS x86_64 (macos-13), macOS aarch64 (macos-14), Windows x86_64. Uses `PyO3/maturin-action@v1`. Publishes to PyPI on tagged release.

**Confidence: 70%** — Maturin action is mature and widely used. Specific risks:

**Risk 1 (MEDIUM) — Linux aarch64:** `zstd-sys` (C dependency in parquet crate) needs zig cross-compiler. Test `maturin build --zig --target aarch64-unknown-linux-gnu` locally in Docker before committing to CI. Fallback: disable native zstd in parquet features, use pure-Rust `zstd` codec only for the wheel (document the limitation).

**Risk 2 (LOW) — Windows:** Snappy (`snap` crate) is pure Rust; other deps have MSVC on the runner. Test early — Windows failures are painful to debug in CI.

**Risk 3 (LOW) — macOS x86_64 (macos-13):** GitHub is deprecating Intel runners. Pin to `macos-13` explicitly while it's still available; plan migration to cross-compile from aarch64.

**Mitigation:** Prototype Phase 18 in parallel with v0.3 work, not after v0.4. Platform issues are easier to fix early than at release time.

**Dependencies:** Phases 1–17

---

### Phase 19: Criterion benchmarks (`benches/profiler_throughput.rs`)
**What:** Benchmark groups: `metadata_parse` (<100ms target), `sample_row_group` on 10M-row file (<5s target), `profile_column_int64` and `profile_column_string` on 2M-row columns, `rayon_parallel_10_columns` vs sequential. 100 MB fixture generated at bench time (not committed).

**Confidence: 95%** — Pattern identical to neutrino criterion benches.

**Dependencies:** Phases 2, 4, 5

---

### Phase 20: Documentation site
**What:** `docs/` with MkDocs Material theme. Pages: overview + install, one page per command (info/tune/bench/apply) with flag reference and examples, concepts pages (encodings, codecs) reusing `teach_yourself` text from Phase 13. GitHub Actions deploy to GitHub Pages on push to main.

**Confidence: 95%** — Standard pattern.

**Dependencies:** Phases 1–18

---

## Cross-Cutting Risks

| Risk | Likelihood | Mitigation |
|------|------------|-----------|
| HyperLogLog inaccuracy on small samples (<50K rows) | 20% | Fall back to exact `HashSet` counting below threshold; expose `cardinality_method` field |
| Predicted size/speed estimates are heuristic, not measured | 80% | Label all predictions `"estimated"` in output; point users to `bench`; document in PRD |
| PyO3 `Bound<PyAny>` API must be used consistently (not deprecated `&PyAny`) | 15% | Use new API from Phase 1; vortex is a working reference |
| `teach_yourself` / `reason_brief` strings go stale when thresholds change | 25% | `insta` snapshots catch this on every heuristic change |
| Linux aarch64 wheel build (C deps + zig cross-compile) | 50% | Prototype early (parallel with v0.3); fallback is pure-Rust codec wheel |
| Encoding×type validity in `bench` command | 45% | Build lookup table from Parquet spec; return user-friendly error on invalid combos |

---

## Dependency Graph

```
Phase 1 (scaffold)
  └── Phase 2 (metadata reader)
        ├── Phase 3 (info command)          ← shippable after this
        ├── Phase 4 (sampler)
        │     └── Phase 5 (stats)
        │           ├── Phase 6 (encoding rules)
        │           │     └── Phase 7 (codec rules)
        │           │           └── Phase 8 (tune command)   ← v0.1 complete
        │           │                 └── Phase 9 (tests + fixtures)
        │           └── Phase 10 (engine compat)
        │                 └── Phase 11 (confidence + JSON)
        │                       └── Phase 12 (exit codes)    ← v0.2 complete
        └── Phases 13, 14, 15 (explain/bundles/advisories)  ← v0.3 complete
              └── Phases 16, 17 (bench + apply)              ← v0.4 complete
                    └── Phases 18, 19, 20 (CI + bench + docs) ← v1.0 complete
```

## Recommended Sequencing

| Week | Phases | Notes |
|------|-------|-------|
| 1 | 1, 2, 3 | `info` command ships; validates the full stack end-to-end early |
| 2 | 4, 5, 6, 7 | Core profiling + recommendation logic (all unit-testable) |
| 3 | 8, 9 | `tune` command + full test suite; v0.1 complete |
| 4 | 10, 11, 12 | Engine awareness + CI integration; v0.2 complete |
| 5 | 13, 14, 15 | Explainability + option bundles; v0.3 complete |
| 6 | 16, 17 | Bench + apply; also start Phase 18 in parallel |
| 7–8 | 18, 19, 20 | Wheel CI, benchmarks, docs; v1.0 complete |

**Start Phase 18 (wheel CI) during week 5–6**, not at the end. Cross-platform build issues are much easier to fix when they're not blocking a release.
