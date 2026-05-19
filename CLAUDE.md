# autoparq

A Parquet compression analysis tool that profiles files and recommends optimal encoding and codec settings. Rust core wrapped in Python bindings via PyO3/maturin; Python CLI on top.

## Project Goals

- **Credible recommendations**: base every suggestion on measured data characteristics, not guesses. Show the user *why* each setting was chosen with specific statistics.
- **Easy to use**: two required inputs maximum (engine, priority). Everything else is measured or defaulted. Copy-paste ready output.
- **Educational**: explanations should teach the user the underlying principle so they don't need the tool next time.

## Architecture

```
autoparq/
├── src/                    # Rust core (library crate)
│   ├── lib.rs              # PyO3 module entry point
│   ├── error.rs            # thiserror error enum
│   ├── profiler/           # File + column profiling
│   │   ├── metadata.rs     # Footer-only statistics (free)
│   │   ├── sampler.rs      # Row group sampling
│   │   └── stats.rs        # ColumnProfile struct
│   ├── recommender/        # Heuristic engine
│   │   ├── encoding.rs     # Encoding selection rules
│   │   ├── codec.rs        # Codec + level selection
│   │   └── engine.rs       # Engine compatibility matrix
│   ├── tuner.rs            # Orchestrates profiling + recommendations → TuneReport
│   ├── bench.rs            # Column benchmarking logic
│   ├── apply.rs            # File rewrite logic
│   └── advisor.rs          # Row group + sort order advisories
├── python/
│   └── autoparq/
│       ├── __init__.py
│       ├── cli.py          # Typer CLI entry point
│       ├── render.py       # Terminal table rendering
│       └── codegen.py      # Python/Spark snippet generation
├── tests/
│   ├── fixtures/           # Small parquet test files (generated, not committed)
│   ├── integration/        # End-to-end Rust tests
│   └── python/             # Python CLI tests (pytest)
├── benches/
│   └── profiler_throughput.rs
├── examples/
│   └── gen_fixtures.rs     # Generates test fixtures
├── Cargo.toml
├── pyproject.toml
└── CLAUDE.md
```

## Rust Guidelines

**Idioms to follow:**
- Use `thiserror` for error types; derive `Error` on domain error enums. No `unwrap()` or `expect()` outside tests.
- Return `Result<T, AutoparqError>` at all library boundaries.
- Use `rayon` for parallel column profiling. Each column is independent — profile them concurrently.
- Prefer `&str` / `Cow<str>` over `String` in hot paths; own data in output structs.
- Use `serde` with `#[derive(Serialize, Deserialize)]` on all output types. JSON output is a first-class interface.
- Avoid `clone()` on large data; use references and lifetimes where the borrow is obvious.
- Profile structs (`ColumnProfile`, `FileProfile`) should be plain data — no methods that trigger I/O.
- Use `arrow` / `parquet` crates (Apache Arrow official Rust implementation). Do not bring in a second Parquet reader.
- PyO3 bindings must use the new `Bound<'py, T>` API throughout. Never use the deprecated `&PyModule` / `&PyAny` forms.
- CLI string inputs (e.g. `"spark"`, `"balanced"`) must be parsed into Rust enums at the PyO3 boundary in `lib.rs`. Enum variants use PascalCase (`Engine::Spark`, `Priority::Balanced`) internally; the CLI always accepts lowercase strings.

**Naming:**
- Snake case everywhere (Rust default). No abbreviations in public names (`cardinality_ratio` not `card_ratio`).
- Error variants: `AutoparqError::FileNotFound`, `AutoparqError::UnsupportedCodec`, etc.

**What to avoid:**
- No `panic!` in library code. Reserve panics for truly impossible states with a comment explaining why.
- No `unsafe` unless wrapping a C FFI boundary with a documented safety contract.
- Don't use `unwrap()` on `Option` from Parquet metadata — statistics are optional and often absent.

## Python Guidelines

- CLI built with `typer`. Use `typer.Argument` for the file path, `typer.Option` for everything else.
- Output rendering in `render.py` using `rich` (tables, colors, icons). The JSON path bypasses rich entirely.
- Keep `cli.py` thin: parse args → call Rust bindings → call renderer. No business logic in the CLI layer.
- Type-annotate all Python functions. Use dataclasses or Pydantic models for structured data passed between modules.

## CLI Design Contract

```
autoparq info  <file> [--output text|json] [--columns COL1,COL2]
autoparq tune  <file> [--engine ENGINE] [--priority size|speed|balanced]
                      [--explain brief|full] [--output text|json]
                      [--sample-rows N] [--min-improvement FLOAT]
autoparq bench <file> --column COL [--codecs zstd:3,lz4,snappy]
                      [--encodings PLAIN,DELTA_BINARY_PACKED]
                      [--measure read|write|size|all]
autoparq apply <file> --output <out-file> [--in-place]
                      [--engine ENGINE] [--priority size|speed|balanced]
```

**Flag reference:**

| Command | Flag | Default | Description |
|---------|------|---------|-------------|
| info | `--columns` | all | Comma-separated column names to display |
| tune | `--engine` | `unknown` | Target engine: `spark`, `duckdb`, `polars`, `clickhouse`, `pandas`, `unknown` |
| tune | `--priority` | `balanced` | Optimization target: `size`, `speed`, `balanced` |
| tune | `--explain` | `brief` | Explanation verbosity: `brief` or `full` |
| tune | `--sample-rows` | `2_000_000` | Max rows to sample per column |
| tune | `--min-improvement` | `10.0` | Exit-code-1 threshold (% predicted size reduction) |
| bench | `--column` | required | Column name to benchmark |
| bench | `--codecs` | all 6 standard | Comma-separated: `zstd:3,lz4,snappy` etc. |
| bench | `--measure` | `all` | What to measure: `read`, `write`, `size`, or `all` |
| apply | `--output` | required | Destination file path |
| apply | `--in-place` | false | Allow output path to equal input path (atomic rename) |

**Exit codes (tune only):**
- `0` — predicted improvement < `--min-improvement` threshold (already well-compressed)
- `1` — predicted improvement ≥ threshold (improvement available)
- `2` — error (file not found, unsupported format, I/O failure)

`info`, `bench`, and `apply` always exit `0` on success, `2` on error.

**Safety rules:**
- `apply` refuses to overwrite the input file unless `--in-place` is explicitly passed.
- `apply` refuses to overwrite an existing output file unless `--in-place` or explicit overwrite flag.
- Never silently apply defaults without telling the user what was assumed.

## Recommendation Heuristics (source of truth)

### Encoding rules — apply in priority order, first match wins

1. `BOOLEAN` → `RLE` *(automatic in all libraries; noted in output)*
2. `INT32/INT64` (physical type) OR `TIMESTAMP`/`DATE` (logical type) + `monotonicity_score >= 0.90` → `DELTA_BINARY_PACKED`
3. `(any type)` + `cardinality_ratio < 0.10` AND `cardinality_estimate × avg_value_bytes < 524_288` (512 KB) → `RLE_DICTIONARY`
4. `FLOAT/DOUBLE` + `cardinality_ratio > 0.50` → `BYTE_STREAM_SPLIT`
5. `BYTE_ARRAY` + UUID pattern detected → `PLAIN` *(dictionary overflow avoidance)*
6. All others → `PLAIN`

**Note on Rule 2:** TIMESTAMP and DATE are logical types layered on top of INT64/INT32 physical types. The check applies to both the physical type and the logical type annotation.

**Note on Rule 3 avg_value_bytes:** Use `string_length_stats.mean_len` for BYTE_ARRAY; `4` for INT32/FLOAT; `8` for INT64/DOUBLE; `1` for BOOLEAN.

### Codec selection — apply after encoding

| Condition | Codec |
|-----------|-------|
| `byte_entropy > 7.5` (pre-compressed data) | UNCOMPRESSED |
| `engine = spark` (unversioned) AND `priority ≠ size` | SNAPPY (safe for all Spark versions) |
| `priority = speed` | LZ4 *(warn if DELTA_BINARY_PACKED also selected — parquet-go bug)* |
| `priority = size` | ZSTD:6 |
| `priority = balanced` (default) | ZSTD:3 |

**Note:** `engine = spark, priority = size` overrides the Spark safety rule and uses ZSTD:6 with a version caveat.

### Profiling implementation details

**Cardinality estimation:**
- Sample size < 50,000 rows → exact counting via `HashSet` (`cardinality_method = "exact"`)
- Sample size ≥ 50,000 rows → HyperLogLog precision p=14, ±0.81% error (`cardinality_method = "hyperloglog"`)
- Null values are excluded from both methods.

**Monotonicity / run-length scores:**
- Nulls are treated as "breaks": any pair where either value is null is excluded from both numerator and denominator.
- Monotonicity only applies to INT32, INT64, TIMESTAMP, DATE columns. Returns `None` for all other types.
- Monotonicity threshold for Rule 2: `>= 0.90` (inclusive).

**UUID detection:** Regex `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$` on up to 1,000 sample values. Detected if ≥ 90% match.

**JSON detection:** Values starting with `{` or `[` (after `trim_start()`) on up to 1,000 sample values. Detected if ≥ 80% match.

**Byte entropy:** Shannon entropy `H = -Σ(p × log₂(p))` over 256-bucket byte frequency histogram. Only computed for BINARY and FIXED_LEN_BYTE_ARRAY columns.

### Confidence tier assignment

| Tier | Condition |
|------|-----------|
| HIGH | `sample_fraction >= 0.10` AND `sample_rows >= 100_000` |
| MEDIUM | (`sample_fraction >= 0.02` OR `sample_rows >= 50_000`) AND not HIGH |
| LOW | everything else |

**Boundary downgrade:** If `rule_fired = RleDictionary` and `cardinality_ratio` is within 20% of the 0.10 threshold (i.e., between 0.08 and 0.12), downgrade to MEDIUM regardless of sample size. The recommendation is uncertain near the threshold.

## Engine Compatibility Matrix

| Codec | Spark | DuckDB | ClickHouse | Polars/PyArrow |
|-------|-------|--------|------------|----------------|
| SNAPPY | ✓ all | ✓ | ✓ | ✓ |
| ZSTD | ✓ 3.2+ | ✓ | ✓ | ✓ |
| LZ4 | ✓ 3.3+ | ✓ | ✓ | ✓ |
| BROTLI | ✓ 3.3+ | ✓ | ✗ | ✓ |
| GZIP | ✓ all | ✓ | ✗ | ✓ |

`DELTA_BINARY_PACKED` encoding: Spark 3.2+, DuckDB ✓, ClickHouse ✓, PyArrow ✓.

Known bugs to warn on:
- `parquet-go` + LZ4 + `DELTA_BINARY_PACKED` → unreadable files. Emit Warning caveat.
- ClickHouse + BROTLI → unsupported for Parquet import. Downgrade to ZSTD with Warning caveat.
- ClickHouse + GZIP → unsupported for Parquet import. Emit Warning caveat.

Column-level encoding in Spark requires Spark 3.4+ when using per-column hints via the DataFrame API. The generated Spark snippet should include this note.

## Testing

- Unit test each heuristic rule in `recommender/` with synthetic column profiles (no file I/O). Every rule gets at least two tests (fires / does not fire).
- Integration tests use small fixture parquet files in `tests/fixtures/`. Keep fixtures < 1 MB; generate them via `cargo run --example gen_fixtures` rather than committing binary blobs.
- Use `insta` for snapshot testing of recommendation output — changes to heuristics require `cargo insta review`.
- Benchmark profiling throughput with `criterion` on a ~100 MB fixture. Targets: metadata parse < 100ms, 1M-row column sample < 2s.

## Output Quality Bar

Every recommendation must include:
1. The specific statistic that triggered it (e.g., `"monotonicity_score=0.94 >= threshold 0.90"`)
2. The rule that fired (human-readable `reason_brief` string)
3. A confidence tier: `HIGH` / `MEDIUM` / `LOW` with the reason it was assigned
4. An engine compatibility note if the recommendation requires a minimum engine version

All predicted size/speed values must be labeled `[estimated]` in text output and use the string `"estimated"` in the JSON `note` field of option bundles.

If a statistic cannot be computed reliably (absent parquet statistics, very small file, high variance between chunks), say so explicitly rather than guessing.

Min/max values in `info` output are truncated to 40 characters for display; the full value is in JSON output.
