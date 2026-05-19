# TASKS — In-Browser Column Benchmarking (Spec 005)

## Phase 1 — Rust

### T01 — Fix `instant` import in `bench.rs`
**File:** `src/bench.rs`
**Change:** Replace `use std::time::Instant;` with `use instant::Instant;`
**Test:** `cargo build --target wasm32-unknown-unknown --features wasm` should not error on Instant.
**Effort:** Trivial (1 line)

---

### T02 — Add `sample_column_from_bytes` helper
**File:** `src/bench.rs`
**Change:** Add a private function `sample_column_from_bytes(data: &[u8], column_name: &str, max_rows: usize) -> Result<(Vec<Box<dyn Array>>, Schema), AutoparqError>`.

Implementation:
- Use `Bytes::copy_from_slice(data)` to construct the reader source
- Mirror `sample_column` logic (find row group, find column index, decode pages)
- No fallback to disk

**Test:** Unit test with a fixture file loaded via `include_bytes!`.
**Effort:** Medium (~40 lines, pattern copy from sampler.rs)

---

### T03 — Add `benchmark_column_from_bytes`
**File:** `src/bench.rs`
**Change:** Add public function `benchmark_column_from_bytes(data: &[u8], column_name: &str, codecs: &[(Codec, Option<i32>)], encodings: &[Encoding]) -> Result<BenchResult, AutoparqError>`.

Implementation:
- Call `sample_column_from_bytes` to get column data
- Run identical write/read loop as `benchmark_column` (no changes to loop logic)
- Return `BenchResult` (same struct as CLI bench)

**Test:** Integration test: call with a fixture, assert entries count matches `codecs.len() * encodings.len()`.
**Effort:** Medium (~50 lines, mostly copied from `benchmark_column`)

---

### T04 — Add `valid_encodings_from_bytes` helper
**File:** `src/bench.rs`
**Change:** Add public function `valid_encodings_from_bytes(data: &[u8], column_name: &str) -> Result<Vec<Encoding>, AutoparqError>`.

Implementation:
- Open the Parquet footer from bytes (no full read)
- Find the column by name, extract `physical_type`
- Call existing `valid_encodings_for_type(physical_type)`

**Test:** Assert returns `[DELTA_BINARY_PACKED, PLAIN]` for an INT64 column.
**Effort:** Small (~25 lines)

---

### T05 — Add `bench_column_bytes` WASM binding
**File:** `src/wasm.rs`
**Change:** Add `#[wasm_bindgen] pub fn bench_column_bytes(data: &[u8], column_name: &str) -> Result<String, JsError>`.

Implementation:
```rust
let codecs = crate::bench::default_codecs();
let encodings = crate::bench::valid_encodings_from_bytes(data, column_name)
    .map_err(|e| JsError::new(&e.to_string()))?;
let result = crate::bench::benchmark_column_from_bytes(data, column_name, &codecs, &encodings)
    .map_err(|e| JsError::new(&e.to_string()))?;
serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
```

**Test:** `cargo test --features wasm` compiles; manual WASM test after Phase 1 is done.
**Effort:** Small (~15 lines)

---

### T06 — Rebuild WASM
**Command:** `cd web && npm run wasm:build`
**Verify:** `web/pkg/` is updated, no WASM panics on a test file.
**Effort:** 2–5 min compile time

---

## Phase 2 — JavaScript: Column card bench

### T07 — Preserve file bytes in app state
**File:** `web/src/main.js` (or equivalent app entry)
**Change:** After file is read as `ArrayBuffer`, store a reference:
```js
let activeFileBytes = null;
// ... on file drop:
activeFileBytes = new Uint8Array(arrayBuffer);
```
Pass `activeFileBytes` to `renderColumns(container, report, activeFileBytes)`.

**Effort:** Small (~5 lines + plumbing through render call)

---

### T08 — Add bench button to column card accordion
**File:** `web/src/render/columns.js`
**Change:** In `buildColumnCard(col, fileBytes)` (add `fileBytes` parameter):
- Add a `<button>` at the bottom of the accordion content
- Wire click handler: disable button, show spinner, call `wasmModule.bench_column_bytes(fileBytes, col.column_name)`
- On resolve: call `renderBenchPanel(accordionContent, result, col.recommended_encoding, col.recommended_codec)`
- On reject: show error text, re-enable button

Button HTML (Tailwind):
```html
<button class="mt-4 text-xs px-3 py-1.5 rounded bg-gray-700 hover:bg-gray-600 text-gray-300 transition-colors">
  Benchmark this column
</button>
```

**Effort:** Medium (~40 lines)

---

### T09 — Implement `renderBenchPanel`
**File:** `web/src/render/columns.js` (or new `web/src/render/bench.js`)
**Change:** Add function:
```js
function renderBenchPanel(container, benchResult, recEncoding, recCodec)
```

Creates inline panel with:
- Table: Encoding | Codec | Compressed | Ratio | Write ms | Read ms
- Highlight row where `entry.encoding === recEncoding && entry.codec === recCodec` (normalised match)
- Footer: "Size measurements are exact. Timing resolution ≈ 1 ms (browser Spectre mitigation)."
- "Close" button that removes panel and re-shows bench button

**Effort:** Medium (~60 lines)

---

### T10 — Export `wasmModule` to column render context
**File:** `web/src/main.js` (or wherever WASM module is initialised)
**Change:** Ensure the WASM module instance is accessible from the bench button click handler. Options:
- Module-level variable in `main.js`, passed as parameter
- Import directly in `columns.js` (preferred if build permits)

**Effort:** Small (wiring only)

---

## Phase 3 — JavaScript: Summary changes

### T11 — Replace size estimate point with range
**File:** `web/src/render/summary.js`
**Change:** In `renderSummary`:
- Replace `report.predicted_size_reduction_pct.toFixed(1)` with `−${low}% to −${high}%`
  where `low = Math.floor(report.predicted_size_reduction_pct * 0.5)` and `high = Math.ceil(report.predicted_size_reduction_pct * 1.5)`
- Label: `[estimated range]` instead of `[estimated]`

**Effort:** Small (~5 lines)

---

### T12 — Remove `predicted_read_speedup` from summary
**File:** `web/src/render/summary.js`
**Change:** Delete the "Read speedup" row from the Size & Speed Estimates table.

**Effort:** Trivial (~5 lines deleted)

---

### T13 — Add bench CTA to summary
**File:** `web/src/render/summary.js`
**Change:** Add a line below the size estimate:
```html
<p class="mt-2 text-xs text-gray-500">
  Benchmark individual columns below for measured sizes.
</p>
```

**Effort:** Trivial (~3 lines)

---

### T14 — (Optional) Update summary with measured bench results
**Files:** `web/src/render/summary.js`, `web/src/main.js`
**Change:** Expose `updateSummaryStats(benchResultsMap)` that re-calculates the displayed size reduction using measured data for benched columns and estimated range for the rest. Called after each bench completes.

This task is marked optional. Implement only if T11–T13 feel insufficient after testing.

**Effort:** Medium (~40 lines)

---

## Task ordering

```
T01 → T02 → T03 → T04 → T05 → T06
                                ↓
                   T07 → T08 → T09 → T10
                                       ↓
                          T11 → T12 → T13 → T14(opt)
```

T01–T06 must complete before any JS work that calls the new WASM binding.
T07 must complete before T08 (fileBytes plumbing needed by bench button).
T11–T13 are independent of T07–T10 and can be done in parallel with Phase 2 if desired.
