# TASKS — Apply in Browser (Spec 008)

## Phase 1 — Rust

### T01 — Cfg-gate `Instant` in `apply.rs`
**File:** [src/apply.rs:2](src/apply.rs#L2)
**Change:** Replace `use std::time::Instant;` with:
```rust
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
```
**Test:** `cargo build` and `cargo build --target wasm32-unknown-unknown --features wasm --no-default-features`.
**Effort:** Trivial.

---

### T02 — Add `ColumnSizeDelta` and `RewriteSummaryJson` structs
**File:** [src/apply.rs](src/apply.rs)
**Change:** Add both structs with `Serialize` derives. Place after the existing `RewriteResult` definition. Also add a non-Serialize `RewriteResultWithOutput` that bundles `output_bytes: Vec<u8>` with the summary.
**Effort:** Small (~20 lines).

---

### T03 — Implement `rewrite_file_from_bytes`
**File:** [src/apply.rs](src/apply.rs)
**Change:** Add:
```rust
pub fn rewrite_file_from_bytes(
    data: &[u8],
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
) -> Result<RewriteResultWithOutput, AutoparqError>
```

Implementation mirrors `rewrite_file` but:
- Calls `crate::tuner::build_tune_report_from_bytes(data, &engine, &priority, sample_rows, "brief")` instead of `build_tune_report`.
- `Bytes::copy_from_slice(data)` as reader source.
- `Vec<u8>` as writer sink (no `NamedTempFile`, no `persist`).
- After `writer.close()`, calls `read_file_metadata_from_bytes(&output_buf)` twice: once on the original `data` (to get `before` stats) and once on `output_buf` (to get `after` stats).
- Zips columns by name into `Vec<ColumnSizeDelta>`. Handle the case where a column exists in one but not the other (skip with a log/note — shouldn't happen in practice since schema is preserved).

**Test:** Add a CLI integration test that calls `rewrite_file_from_bytes` on a fixture and asserts `output_bytes` is non-empty and `per_column_diff.len() == input_columns.len()`.
**Effort:** Medium (~80 lines).

---

## Phase 2 — WASM

### T04 — Add `apply_file_bytes` WASM binding
**File:** [src/wasm.rs](src/wasm.rs)
**Change:** Add `#[wasm_bindgen] pub fn apply_file_bytes(...)` per PLAN Step 2.1. Uses `js_sys::Object` + `Reflect::set` to return both the `Uint8Array` and a `summary` JSON string. `sample_rows` hardcoded at 500_000 (matches tuner default in `build_tune_report_from_bytes`).
**Effort:** Small (~25 lines).

---

### T05 — Add `check_apply_file_size` WASM binding
**File:** [src/wasm.rs](src/wasm.rs)
**Change:** Add the three-tier size gate per PLAN Step 2.2. Returns JSON with `severity` field (`mild` | `severe` | `blocked` | null).
**Effort:** Small (~15 lines).

---

### T06 — Rebuild WASM
**Command:** `cd web && npm run wasm:build`
**Effort:** ~3 min.

---

## Phase 3 — Bridge + worker + download helper

### T07 — Worker: `apply` and `checkApplySize` message types
**File:** [web/src/workers/autoparq-worker.js](web/src/workers/autoparq-worker.js)
**Change:**
- Import `apply_file_bytes` and `check_apply_file_size`.
- Handle `apply`: reads `_lastFileData`, calls `apply_file_bytes`, posts `{ outputBytes, summaryJson }` with `[outputBytes.buffer]` as transferables.
- Handle `checkApplySize`: calls `check_apply_file_size(byteLen)`, posts `{ result }`.
**Effort:** Small (~20 lines).

---

### T08 — Bridge: `applyFile` and `checkApplySize`
**File:** [web/src/wasm-bridge.js](web/src/wasm-bridge.js)
**Change:** Add both exports per PLAN Step 3.2. `applyFile` parses `summaryJson` and returns `{ outputBytes, rewrite, perColumnDiff }`.
**Effort:** Small (~15 lines).

---

### T09 — Download helper module
**File:** new [web/src/lib/download.js](web/src/lib/download.js)
**Change:** Add `triggerDownload(bytes, name)` and `tunedFilename(originalName)` per PLAN Step 3.3.
**Effort:** Small (~20 lines).

---

## Phase 4 — UI

### T10 — App state + `onApply` wiring
**File:** [web/src/App.js](web/src/App.js)
**Change:**
- Add module-level `_rewriteResult` and `_rewriteOutputBytes`.
- Add `onApply(engine, priority)` that calls `applyFile`, stores the result, triggers the download, and returns the data so callers can re-render.
- Clear both in `reset()` and at the top of `onEngineChange`.
- Thread `onApply` and a `getRewriteState()` accessor into `renderReport` (6th and 7th args).

**Effort:** Medium (~40 lines).

---

### T11 — Apply CTA block in Summary
**File:** [web/src/render/summary.js](web/src/render/summary.js)
**Change:**
- Accept `onApply` + `getRewriteState` params (thread through from `report.js`).
- When no rewrite has happened, render the primary CTA block per PRD A05.
- Button click:
  1. Call `checkApplySize(report.file_size_bytes)`. If blocked → show inline error, abort. If severe → show severe warning and ask for confirmation (simple `confirm()` is fine; no modal needed). If mild → show notice inline and proceed.
  2. Disable button, show "Rewriting… (may take up to 30 seconds)".
  3. Call `onApply(engine, priority)`.
  4. On resolve: trigger download, then re-render (the caller will invoke `renderReport` again with the new `getRewriteState`).
  5. On reject: show error with "Copy snippet instead" link that scrolls to the snippet panel.

**Effort:** Medium (~80 lines).

---

### T12 — Measured overlay in Summary
**File:** [web/src/render/summary.js](web/src/render/summary.js)
**Change:** When `getRewriteState()` returns a non-null result:
- Prepend a success strip ("✓ Applied. 47.3 MB → 40.3 MB (−14.8%). [Download again] [✕ Discard]") above the main grid.
- Replace "Estimated size after" row value with actual output size.
- Replace size-reduction range with `−{actual_reduction_pct}% [measured]`.
- Hide the "Apply these recommendations" CTA; instead render the success strip's two actions.
- "Download again" calls `triggerDownload(state.outputBytes, tunedFilename(fileName))`.
- "Discard" invokes an `onDiscard` callback that clears `_rewriteResult` and `_rewriteOutputBytes` in App and re-renders.

**Effort:** Medium (~60 lines).

---

### T13 — Measured overlay on column cards
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:**
- Accept `perColumnDiff` (optional) in `renderColumns`. Thread through from `report.js` via the existing function signature extension.
- Build a `deltaByName` map once, keyed by `column_name`.
- In `buildColumnCard`, when a delta is present, append the "Measured: X → Y (−Z%)" footer. Apply the three-tier colour rule from PRD A09.

**Effort:** Small (~40 lines).

---

### T14 — Propagate `onApply` / `getRewriteState` through `report.js`
**File:** [web/src/render/report.js](web/src/render/report.js)
**Change:** Extend `renderReport` signature:
```js
export function renderReport(container, report, fileName, benchColumnFn, onApply, getRewriteState, onDiscard)
```

Pass `onApply`, `getRewriteState`, `onDiscard` into `renderSummary` and `getRewriteState()?.perColumnDiff` into `renderColumns`.

**Effort:** Small (~10 lines).

---

### T15 — Wire up in App.js
**File:** [web/src/App.js](web/src/App.js)
**Change:** Update the two `renderReport(...)` call sites (in `showReport` and `onEngineChange`) to pass the new args. Implement `onDiscard` = clear both module vars + re-render.

**Effort:** Small (~15 lines).

---

## Phase 5 — Verification

### T16 — cargo test + build
**Commands:** `cargo build && cargo test --lib`
**Effort:** ~1 min.

### T17 — vite build
**Command:** `cd web && npx vite build`
**Effort:** ~15 s. Confirm no warnings about unknown utilities.

### T18 — Manual walkthrough
On a 10–50 MB fixture:
- [ ] Drop file, analysis runs, report shows.
- [ ] Click "Download optimized copy" → download appears with `_tuned.parquet` suffix.
- [ ] Summary shows `[measured]` values.
- [ ] Cards show the measured footer with correct delta colours.
- [ ] "Download again" re-triggers download without re-running WASM.
- [ ] "Discard" clears the overlay and restores the CTA.
- [ ] Changing engine clears the overlay.
- [ ] Dropping a new file clears the overlay.

### T19 — Size-gate spot check
Using browser console: `await window._bridge?.checkApplySize(300e6)` — confirm mild warning. Same for 600e6 (severe) and 1.5e9 (blocked).
*(Expose the bridge on `window` temporarily or inspect via network trace.)*
**Effort:** 5 min.

### T20 — Privacy verification
DevTools → Network tab, clear log, click apply. No requests should appear.
**Effort:** 1 min.

---

## Task ordering

```
T01 → T02 → T03
              ↓
             T04 → T05 → T06
                          ↓
                  T07 → T08 → T09
                               ↓
                              T10 → T11 → T12 → T13 → T14 → T15
                                                             ↓
                                                  T16 → T17 → T18 → T19 → T20
```

Rust (T01–T03) can finish and be tested against the CLI before any WASM/JS work. T06 (WASM rebuild) gates all JS work. Inside Phase 4, T13 can proceed in parallel with T11–T12.
