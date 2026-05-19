# PRD — In-Browser Column Benchmarking (Spec 005)

## Problem

The Summary section displays two fabricated metrics that users have flagged as misleading:

- `predicted_size_reduction_pct` — hardcoded compression factors (3× for DELTA, 10×/5×/2× for RLE_DICTIONARY, 1.15× for BYTE_STREAM_SPLIT) applied to the original file size
- `predicted_read_speedup` — `1.0 + (size_reduction/100.0) * 0.7` where 0.7 is an empirically unsupported coefficient

These numbers look precise but are not derived from the actual file being analysed. Users noticed they show the same values across different files and complained they "always seem to say the same thing." Engineers sharing the report with teammates lose credibility when the estimates are questioned.

The tool already has a `bench` command (CLI) that measures real compressed sizes by writing each encoding/codec combination to an in-memory buffer and timing it. The Rust code for this is almost entirely WASM-compatible — no disk I/O, no threads, just `Vec<u8>` writes.

## Goals

1. Users can click "Benchmark" on any column card and see real measured compressed sizes for all encoding/codec combinations.
2. The Summary section stops showing fabricated point estimates and instead shows either a calibrated range or directs users to benchmark.
3. The browser remains responsive during benchmarking (no UI freeze).
4. Timing measurements are shown with an honest caveat (browser Spectre mitigations reduce `performance.now()` resolution to ~1 ms; size measurements are exact).

## Non-Goals

- Custom codec/encoding selector UI — bench with `default_codecs()` and `valid_encodings_for_type()` only.
- Batch benchmarking (all columns at once) — too slow; add in a follow-on spec.
- Persisting benchmark results across page reloads.
- Progress callbacks within a single benchmark run (WASM execution is synchronous; no intermediate updates possible).
- Export benchmark results to CSV or clipboard.

---

## Feature Breakdown

### B01 — Rust: `benchmark_column_from_bytes`

`bench.rs` currently has `benchmark_column(path: &Path, ...)` which calls `sample_column(path, ...)` (disk read) and uses `std::time::Instant` (not available in WASM). A new function is needed:

```rust
pub fn benchmark_column_from_bytes(
    data: &[u8],
    column_name: &str,
    codecs: &[(Codec, Option<i32>)],
    encodings: &[Encoding],
) -> Result<BenchResult, AutoparqError>
```

This function:
- Reads the Parquet file from `data: &[u8]` using `Bytes::copy_from_slice(data)`
- Extracts the column's row group data in-memory (same as the existing sampler, just from bytes)
- Runs the existing write/read loop from `benchmark_column`
- Uses `instant::Instant` instead of `std::time::Instant`

The core write/read loop in `benchmark_column` already uses in-memory buffers (`Vec<u8>`, `Cursor`, `Bytes::from(buf)`), so no logic needs to change — only the input source and the timer.

### B02 — Rust: `instant` crate fix in `bench.rs`

`bench.rs` line 3 imports `std::time::Instant`. On wasm32, `std::time::Instant` panics. Replace with the `instant` crate (already a dependency; already used in `tuner.rs`):

```rust
// Before
use std::time::Instant;

// After
use instant::Instant;
```

### B03 — Rust: WASM binding `bench_column_bytes`

Add to `wasm.rs`:

```rust
#[wasm_bindgen]
pub fn bench_column_bytes(
    data: &[u8],
    column_name: &str,
) -> Result<String, JsError> {
    let codecs = crate::bench::default_codecs();
    let encodings = // derive from physical type of column_name
    let result = crate::bench::benchmark_column_from_bytes(data, column_name, &codecs, &encodings)
        .map_err(|e| JsError::new(&e.to_string()))?;
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}
```

Returns JSON-serialised `BenchResult`:
```json
{
  "column_name": "user_id",
  "physical_type": "INT64",
  "uncompressed_bytes": 8000000,
  "entries": [
    {
      "encoding": "DELTA_BINARY_PACKED",
      "codec": "ZSTD",
      "codec_level": 3,
      "compressed_bytes": 450000,
      "write_ms": 120,
      "read_ms": 45,
      "compression_ratio": 17.78
    }
  ]
}
```

### B04 — JS: Bench button and results panel in column card

In `web/src/render/columns.js`, add a "Benchmark" button inside the column card accordion (below the "Confidence" section).

**Interaction flow:**
1. Button shows: "Benchmark this column"
2. Click → disable button, show spinner + "Benchmarking…"
3. Call `wasmModule.bench_column_bytes(fileBytes, columnName)`
4. On success → hide button, render results panel inline
5. On error → show error message, re-enable button

**Results panel:**
- Header: "Benchmark Results"
- Table columns: Encoding | Codec | Compressed | Ratio | Write ms | Read ms
- Highlight the recommended encoding+codec row (green background)
- Footer: "Size measurements are exact. Timing resolution ≈ 1 ms (browser Spectre mitigation)."
- "Close" button to collapse and show the Bench button again

**Column to benchmark:** derived from `col.column_name` already available in the card render context.

**File bytes:** the raw `ArrayBuffer` from the file drop must be kept in module scope so it's available when the button is clicked after the report renders.

### B05 — JS: Summary section — replace fabricated estimates

In `web/src/render/summary.js`:

**Before any benchmark:** Replace the point estimate with a range:
- Size reduction: `−${low}% to −${high}%` where `low = floor(estimate × 0.5)` and `high = ceil(estimate × 1.5)` — these bounds represent the uncertainty from using hardcoded factors.
- Drop `predicted_read_speedup` entirely — the 0.7 coefficient has no empirical basis.
- Add a note: "Benchmark individual columns below for measured sizes."

**After benchmarks are run:** Optionally update the range once real data is available (tracked in a `benchResults` map keyed by column name). If ≥ 1 column is benchmarked, show measured reduction for those columns alongside the estimated range for the rest.

This is opt-in; the summary automatically updates as the user runs benchmarks.

---

## UX Principles

- **Opt-in:** Benchmarking never runs automatically. Users choose which columns matter.
- **Honest uncertainty:** Every estimate is labelled `[estimated range]`. Measured results are labelled `[measured]`.
- **Progressive disclosure:** Results appear inline — users don't lose their place in the report.
- **No surprises:** If a column's physical type has only one valid encoding (e.g., BOOLEAN → RLE only), the results table will have only one row. This is correct and expected.

---

## Acceptance Criteria

- [ ] `bench_column_bytes` is callable from JS and returns valid JSON for all column types.
- [ ] Benchmark button appears in every column card accordion.
- [ ] Results table correctly highlights the recommended encoding/codec row.
- [ ] Summary section shows a range, not a point estimate, and does not show `predicted_read_speedup`.
- [ ] UI does not freeze visibly during benchmarking (test on a 50 MB file).
- [ ] Timing caveat appears in all results panels.
- [ ] No WASM panic on any tested file.
