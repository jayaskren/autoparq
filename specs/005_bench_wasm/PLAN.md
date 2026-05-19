# PLAN — In-Browser Column Benchmarking (Spec 005)

## Approach

Ship in three sequential layers: Rust core → WASM binding → UI. Each layer is testable independently before the next.

The Rust changes are minimal: one new function (`benchmark_column_from_bytes`) that refactors the existing `benchmark_column` to accept `&[u8]` instead of `&Path`, plus a one-line timer fix. No new algorithms. No new dependencies beyond what already exist.

---

## Phase 1 — Rust: WASM-compatible bench

### Step 1.1 — Fix `instant` import in `bench.rs`

Change `use std::time::Instant;` to `use instant::Instant;`. The `instant` crate is already in `Cargo.toml` and already used in `tuner.rs`.

### Step 1.2 — Extract in-memory sampling

The existing `benchmark_column` calls `sample_column(path, ...)` which reads from disk. Extract a helper:

```rust
fn sample_column_from_bytes(
    data: &[u8],
    column_name: &str,
    max_rows: usize,
) -> Result<(Vec<Box<dyn Array>>, Schema), AutoparqError>
```

This mirrors `sample_column` but uses `Bytes::copy_from_slice(data)` as the reader source. The rest of the sampling logic stays identical.

### Step 1.3 — Add `benchmark_column_from_bytes`

```rust
pub fn benchmark_column_from_bytes(
    data: &[u8],
    column_name: &str,
    codecs: &[(Codec, Option<i32>)],
    encodings: &[Encoding],
) -> Result<BenchResult, AutoparqError>
```

This calls `sample_column_from_bytes`, then runs the identical write/read loop that `benchmark_column` uses. The only difference is the data source.

### Step 1.4 — Derive valid encodings from file metadata

`bench_column_bytes` (the WASM binding) needs to derive valid encodings without the caller specifying them. Add a helper:

```rust
pub fn valid_encodings_from_bytes(
    data: &[u8],
    column_name: &str,
) -> Result<Vec<Encoding>, AutoparqError>
```

This reads the Parquet footer, finds the column's physical type, and calls the existing `valid_encodings_for_type(physical_type)`.

### Step 1.5 — Add WASM binding in `wasm.rs`

```rust
#[wasm_bindgen]
pub fn bench_column_bytes(
    data: &[u8],
    column_name: &str,
) -> Result<String, JsError>
```

Calls `default_codecs()`, `valid_encodings_from_bytes()`, then `benchmark_column_from_bytes()`, serialises `BenchResult` to JSON.

---

## Phase 2 — JS: Column card bench button

### Step 2.1 — Preserve file bytes in app state

After the user drops a file, the `ArrayBuffer` is currently only held during the initial WASM call. Keep a reference:

```js
// In main app module
let fileBytes = null; // Set on drop, cleared on new file
```

Pass `fileBytes` into the render functions that need it (column card builder).

### Step 2.2 — Add bench button to column card accordion

In `buildColumnCard(col, fileBytes)`:
- Add a button at the bottom of the accordion content section
- Wire up a click handler that calls `bench_column_bytes` and renders results

### Step 2.3 — Render bench results table

```js
function renderBenchResults(container, benchResult, recommendedEncoding, recommendedCodec)
```

Creates an inline panel with:
- A table showing all `BenchResult.entries`
- Row highlighting for the recommended encoding+codec combination
- Timing caveat footer

### Step 2.4 — Loading and error states

- While bench runs: disable button, show spinner + "Benchmarking…"
- On error: show error message below button, re-enable button

---

## Phase 3 — JS: Summary section changes

### Step 3.1 — Replace point estimate with range

In `renderSummary(container, report)`:
- Compute `low = Math.floor(report.predicted_size_reduction_pct * 0.5)`
- Compute `high = Math.ceil(report.predicted_size_reduction_pct * 1.5)`
- Display: `−${low}% to −${high}% [estimated range]`
- Remove `predicted_read_speedup` display entirely

### Step 3.2 — Add bench CTA to summary

After the estimates, add a small note:
> "Benchmark individual columns below for measured sizes."

### Step 3.3 — (Optional) Update summary with measured results

If this is in scope: expose a `updateSummaryWithBenchResults(benchResultsMap)` function that re-renders the size reduction stat using measured data for benched columns and estimated range for the rest. Called from the bench completion handler.

This step is marked optional — the core value is steps 3.1 and 3.2. Step 3.3 adds polish.

---

## Build and test

After Phase 1, rebuild WASM:
```bash
cd web && npm run wasm:build
```

Manual test:
1. Drop a .parquet file.
2. Open a column card, expand accordion.
3. Click "Benchmark this column".
4. Verify results table appears with correct data.
5. Verify the recommended row is highlighted.
6. Verify summary shows range not point estimate.

---

## Risks and mitigations

| Risk | Mitigation |
|------|-----------|
| Large file makes bench take >10s | Show file-size warning on bench button if file > 200 MB; already have `check_file_size()` |
| WASM panic on unsupported column type | `valid_encodings_for_type` already handles all physical types; returns `PLAIN` as fallback |
| `std::time::Instant` panic missed | Caught in Phase 1 Step 1.1 as first change |
| `sample_column_from_bytes` re-reads entire file per bench call | Acceptable: file is already in browser memory; no network I/O; cost is CPU-only |
| Recommended row not found in bench results | Match by normalising both sides to `"ENCODING + CODEC:level"` string |
