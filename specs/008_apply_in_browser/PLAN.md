# PLAN — Apply in Browser (Spec 008)

## Approach

Four phases. Phase 1 (Rust core) is independent and testable against the existing CLI path. Phases 2–4 layer on top. The bench feature (Spec 005) already established every pattern we need: in-memory parquet I/O, WASM binding that returns JSON, worker message dispatch, transfer of `Uint8Array` to main thread.

---

## Phase 1 — Rust: in-memory rewrite

### Step 1.1 — Add result structs in `src/apply.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ColumnSizeDelta {
    pub column_name: String,
    pub before_compressed: i64,
    pub after_compressed: i64,
    pub before_encodings: Vec<String>,
    pub after_encodings: Vec<String>,
    pub before_codec: String,
    pub after_codec: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RewriteSummaryJson {
    pub rewrite: RewriteResult,
    pub per_column_diff: Vec<ColumnSizeDelta>,
}

pub struct RewriteResultWithOutput {
    pub output_bytes: Vec<u8>,
    pub summary: RewriteSummaryJson,
}
```

`RewriteResult` already exists (lines 16–22 of [apply.rs](src/apply.rs#L16-L22)) — reuse.

### Step 1.2 — Cfg-gate `Instant` AND `tempfile` imports (C01)

[apply.rs:2](src/apply.rs#L2) currently does `use std::time::Instant;`. [apply.rs:9](src/apply.rs#L9) does `use tempfile::NamedTempFile;`. `tempfile` is not WASM-compatible, so both need conditional compilation:

```rust
#[cfg(target_arch = "wasm32")]
use instant::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
use tempfile::NamedTempFile;
```

Also gate the entire `rewrite_file` function with `#[cfg(not(target_arch = "wasm32"))]` since it uses `NamedTempFile`. The new `rewrite_file_from_bytes` (Step 1.3) stays unconditional and becomes the shared path.

### Step 1.3 — Add `rewrite_file_from_bytes`

```rust
pub fn rewrite_file_from_bytes(
    data: &[u8],
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
) -> Result<RewriteResultWithOutput, AutoparqError>
```

Mirror `rewrite_file` but:
- Call `build_tune_report_from_bytes(data, …, "brief")` — function already exists in [tuner.rs:867](src/tuner.rs#L867).
- Read input via `Bytes::copy_from_slice(data)` → `ParquetRecordBatchReaderBuilder::try_new(bytes)`.
- Write to `Vec<u8>` via `ArrowWriter::try_new(&mut buf, schema, Some(props))`. No `NamedTempFile`, no `persist`.
- After `writer.close()`, parse the output bytes via `read_file_metadata_from_bytes(&output_buf)` (exists at [metadata.rs:348](src/profiler/metadata.rs#L348)) to get the "after" `FileProfile`.
- Parse the input via the same function on `data` for the "before" profile.
- Build a `Vec<ColumnSizeDelta>` by zipping columns by name.

### Step 1.4 — Native CLI unchanged

`rewrite_file` keeps its disk-based signature. The new function is additive. Existing tests pass.

---

## Phase 2 — WASM binding + file-size gate

### Step 2.1 — Add `apply_file_bytes` in `src/wasm.rs`

```rust
#[wasm_bindgen]
pub fn apply_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
) -> Result<JsValue, JsError> {
    let eng = Engine::from_str(engine);
    let pri = Priority::from_str(priority);
    let result = crate::apply::rewrite_file_from_bytes(data, eng, pri, 500_000)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let obj = js_sys::Object::new();
    let output_arr = js_sys::Uint8Array::from(result.output_bytes.as_slice());
    let summary_json = serde_json::to_string(&result.summary)
        .map_err(|e| JsError::new(&e.to_string()))?;

    js_sys::Reflect::set(&obj, &"output".into(), &output_arr)?;
    js_sys::Reflect::set(&obj, &"summary".into(), &summary_json.into())?;
    Ok(obj.into())
}
```

Uses `js_sys` + `Reflect::set` — no new dependency needed. The `Uint8Array::from` is a one-copy (WASM linear memory → JS heap). Subsequent transfer from Worker → main thread is truly zero-copy via the transferables list.

### Step 2.2 — Extend `check_file_size` with an `operation` parameter

Rather than duplicate logic, add an optional mode:

```rust
#[wasm_bindgen]
pub fn check_apply_file_size(byte_len: usize) -> String {
    const HARD: usize = 1_073_741_824;       // 1 GB
    const SEVERE: usize = 524_288_000;        // 500 MB
    const MILD: usize = 209_715_200;          // 200 MB

    if byte_len > HARD {
        return r#"{"ok":false,"warning":null,"severity":"blocked","error":"File exceeds 1 GB. Use the CLI snippet below."}"#.to_string();
    }
    if byte_len > SEVERE {
        let mb = byte_len / (1024 * 1024);
        return format!(
            r#"{{"ok":true,"warning":"Large file ({} MB). Rewriting this size may fail on memory-constrained browsers; if it fails, use the CLI snippet instead.","severity":"severe","error":null}}"#,
            mb
        );
    }
    if byte_len > MILD {
        let mb = byte_len / (1024 * 1024);
        return format!(
            r#"{{"ok":true,"warning":"Large file ({} MB). Rewrite may take 30+ seconds and requires up to 2 GB of browser memory.","severity":"mild","error":null}}"#,
            mb
        );
    }
    r#"{"ok":true,"warning":null,"severity":null,"error":null}"#.to_string()
}
```

### Step 2.3 — Rebuild WASM

```bash
cd web && npm run wasm:build
```

---

## Phase 3 — Worker + bridge + download helper

### Step 3.1 — Worker handler

[web/src/workers/autoparq-worker.js](web/src/workers/autoparq-worker.js):
- Import `apply_file_bytes` and `check_apply_file_size`.
- New `apply` message type:
  ```js
  if (type === 'apply') {
    const { engine, priority } = payload;
    if (!_lastFileData) throw new Error('No file loaded.');
    const result = apply_file_bytes(_lastFileData, engine, priority);
    const outputBytes = result.output;  // Uint8Array
    const summaryJson = result.summary; // string
    // Transfer the ArrayBuffer zero-copy; summary is a small string
    self.postMessage(
      { id, type: 'result', payload: { outputBytes, summaryJson } },
      [outputBytes.buffer],
    );
    return;
  }
  if (type === 'checkApplySize') { ... }
  ```

### Step 3.2 — Bridge

[web/src/wasm-bridge.js](web/src/wasm-bridge.js):

```js
export async function applyFile(engine, priority) {
  const { outputBytes, summaryJson } = await call('apply', { engine, priority });
  const summary = JSON.parse(summaryJson);
  return { outputBytes, rewrite: summary.rewrite, perColumnDiff: summary.per_column_diff };
}

export async function checkApplySize(byteLen) {
  const { result } = await call('checkApplySize', { byteLen });
  return JSON.parse(result);
}
```

The existing `call` helper needs to handle a `result` payload that includes a non-string `Uint8Array` field — it already passes the whole `payload` through, so no change needed.

### Step 3.3 — Download helper

New file [web/src/lib/download.js](web/src/lib/download.js):

```js
export function triggerDownload(bytes, suggestedName) {
  const blob = new Blob([bytes], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = suggestedName;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  // Revoke after a tick so Safari has a chance to start the download
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

export function tunedFilename(originalName) {
  const idx = originalName.lastIndexOf('.');
  const base = idx > 0 ? originalName.slice(0, idx) : originalName;
  const ext = idx > 0 ? originalName.slice(idx) : '.parquet';
  return `${base}_tuned${ext}`;
}
```

---

## Phase 4 — UI: button, download, overlay

### Step 4.1 — App state (A07, A10)

[web/src/App.js](web/src/App.js): add module-level variables:

```js
let _rewriteResult = null;       // { rewrite, perColumnDiff, engine, priority, appliedAt }
let _rewriteOutputBytes = null;  // Uint8Array retained for re-download
let _rewriteDiscardTimer = null; // setTimeout handle for auto-discard
let _applying = false;           // gates the engine/priority selectors while apply runs
```

Per C04, refactor `renderReport` to accept an options object:

```js
renderReport(container, report, fileName, {
  benchColumnFn,
  onApply,      // (engine, priority) => Promise<result>
  getRewriteState, // () => _rewriteResult
  onDiscard,    // clears rewrite state + re-renders
  isApplying,   // boolean — disables engine/priority selects in header
});
```

On apply completion:
1. Store `_rewriteResult` including engine/priority.
2. Clear any existing `_rewriteDiscardTimer`, set a new 60s `setTimeout` that clears `_rewriteOutputBytes` only (leaves the measured overlay intact). Downstream "Download again" clicks reset this timer.
3. Re-render.

When engine/priority changes and apply is *not* in flight, clear all state per A10. The engine-change handler in App already does the re-render; just add the clear.

**Engine/priority disable during apply (A01):** When `_applying === true`, add the `disabled` attribute to both `<select>` elements in the header cluster and sticky header.

### Step 4.2 — Apply CTA block in Summary (A05)

[web/src/render/summary.js](web/src/render/summary.js):
- After the main grid, render a primary-coloured CTA card with an "Apply these recommendations" heading, explanatory copy, and a "Download optimized copy" button.
- The button uses `bg-accent-emphasis text-fg-on-emphasis`.
- Wire a click handler that:
  1. Calls `checkApplySize(report.file_size_bytes)` first. If blocked → render the "too large" error inline, don't proceed. If severe/mild → inline warning + still proceed on click.
  2. Calls `onApply(engine, priority)` (passed in from App.js).
  3. On resolve: triggers `triggerDownload(outputBytes, tunedFilename(fileName))`, then re-renders the Summary and Columns with the new overlay.
  4. On reject: shows an error message below the button with a "Copy snippet instead" fallback link.

### Step 4.3 — Download trigger (A06)

Already covered by `triggerDownload` from Step 3.3. The button handler calls it directly.

### Step 4.4 — Measured overlay in Summary (A08)

When `rewriteResult != null`, Summary swaps:
- "Estimated size after" row → "Actual size after" row with `[measured]` label (green).
- "Size reduction" row → shows `actual_reduction_pct` with `[measured]` label.
- Prepend a success strip above the main grid:
  ```
  ✓ Applied. 47.3 MB → 40.3 MB (−14.8%).  [Download again]  [✕ Discard]
  ```

"Download again" calls `triggerDownload(_rewriteOutputBytes, …)` — no re-run. "Discard" clears `_rewriteOutputBytes` and `_rewriteResult` and re-renders.

### Step 4.5 — Measured overlay on column cards (A09)

[web/src/render/columns.js](web/src/render/columns.js):
- Accept a `rewriteResult` arg (or build a `deltaByName` map from it).
- In `buildColumnCard`, when a delta is available, append a measured footer inside the card:
  ```html
  <div class="px-4 pb-3 text-xs font-mono">
    <span class="text-fg-subtle">Measured: </span>
    <span class="text-fg-default">{humanBytes(before)} → {humanBytes(after)}</span>
    <span class="{green/neutral/red}">({sign}{pct}%)</span>
  </div>
  ```
- Colour rules:
  - Reduction ≥ 5% → `text-success-fg`
  - Between −5% and +5% → `text-fg-muted`
  - Column grew → `text-danger-fg`

### Step 4.6 — Engine/priority invalidation (A10)

In `onEngineChange` in App.js:
```js
_rewriteResult = null;
_rewriteOutputBytes = null;
```

Then re-render via the existing path. The CTA returns to the "Download optimized copy" initial state.

---

## Phase 5 — Verification

1. `cargo build && cargo test --lib` — confirm native path still green.
2. `cd web && npm run wasm:build` — WASM rebuilds cleanly.
3. `npx vite build` — no unknown-utility warnings.
4. Manual walkthrough on the NYC taxi fixture:
   - Apply → download completes with `<name>_tuned.parquet`.
   - Summary shows `[measured]` rather than `[estimated range]`.
   - Cards show the measured before/after footer.
   - Change engine → overlay clears, button returns to default.
   - Drop a new file → overlay clears.
5. Memory sanity: in DevTools Memory tab, confirm the WASM heap doesn't grow unbounded across successive apply calls.
6. File-size gates: artificially check the three thresholds with `check_apply_file_size` in the console.
7. Privacy check: the Network tab stays empty during apply.

---

## Risks

| Risk | Mitigation |
|------|-----------|
| WASM OOM on 1 GB file | Accept the risk per A03; show severe warning above 500 MB. Error surfaces through the existing reject-path fallback (snippet link). |
| `Uint8Array` from WASM is a view into WASM linear memory — must copy out before posting | `Uint8Array::from(slice)` in the binding already makes a fresh JS-heap copy, so the returned buffer is truly detachable/transferable. Verify during T11. |
| Hosted browsers throttle huge Blob downloads | Real but rare above ~2 GB; well below our 1 GB output cap. |
| `_rewriteOutputBytes` pinning memory | "✕ Discard" button frees it explicitly; engine change also clears. |
| Writer fails on a column type our recommender picks a bad encoding for | Existing `rewrite_file` has no such failures in CLI tests; if a runtime writer error occurs, surface it via the error path. |
| Apply on Spark-unsupported combos | Out of scope — engine compatibility is already flagged at recommendation time. Apply trusts the recommendations. |
