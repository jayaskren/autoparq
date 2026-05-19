# PRD — Apply in Browser: Download Tuned File + Measured Diff Overlay (Spec 008)

## Problem

autoparq currently ends its value flow at "here are the recommendations." A user who wants to act on them has to:

1. Copy the generated PyArrow/PySpark snippet.
2. Open a Python environment.
3. Install pyarrow.
4. Run the snippet against their file.
5. Come back to check whether the predicted improvement matched reality.

Each step is a drop-off point. Meanwhile, `src/apply.rs` already knows how to rewrite a Parquet file with per-column encodings and codec — it just hasn't been exposed to the browser.

Two things are missing:

- **No in-browser apply.** The user cannot download a tuned copy directly from the page.
- **No post-action measurement.** Even after applying the recommendations elsewhere, they have no easy way to see what actually changed per column. Our own report shows predicted ranges, never measured results.

## Goals

1. A single "Download optimized copy" action in the Summary rewrites the file with the recommended settings (runs entirely in the browser) and triggers a download of the result.
2. After the rewrite completes, the page **overlays measured results** on top of the existing recommendations so the user can see what actually changed: per-column actual size, actual ratio, and a file-level measured reduction.
3. No file data leaves the browser. The rewrite happens in WASM, same privacy posture as the analysis.
4. Graceful degradation on large files: stricter gate than the existing 1 GB analysis limit because apply is a 2× memory operation.

## Non-Goals

- Applying a *subset* of recommendations (e.g., "only rewrite columns the user selected"). The download applies the full recommendation set as shown.
- In-browser editing of the recommendations before apply (users can change engine/priority; that re-runs recommendations and any subsequent apply uses the new set).
- Streaming rewrite for multi-GB files.
- Preserving the rewritten bytes across page reloads.

---

## User Flow

```
┌──────────────────────────────────────────────────────────────┐
│ Summary section                                              │
│                                                              │
│  Size reduction: −12% – −18% [estimated range]              │
│  File health:    10 of 14 match                             │
│                                                              │
│  ┌────────────────────────────────────────────────┐         │
│  │  Download optimized copy  ↓                    │         │
│  │  Rewrites the file with the recommendations    │         │
│  │  shown above. Runs locally in your browser.    │         │
│  └────────────────────────────────────────────────┘         │
└──────────────────────────────────────────────────────────────┘
```

1. User clicks **Download optimized copy**.
2. Button enters loading state: "Rewriting… (this may take a few seconds on large files)".
3. On completion:
   - Browser download dialog appears with `<original_name>_tuned.parquet`.
   - Summary updates: `Size reduction: −14.8% [measured]` replaces the estimated range.
   - Each column card gains a small "measured" footer: `4.1 MB → 3.2 MB (−22%)`.
   - A compact file-level badge appears next to the download button: `✓ Applied — 47 MB → 40 MB (−14.8%)`.
4. User can re-download from the same button (bytes are cached; no re-rewrite).
5. Changing engine/priority invalidates the measured overlay and returns to estimates; the user can re-apply.

---

## Feature Breakdown

### A01 — Rust: `rewrite_file_from_bytes`

New function in [src/apply.rs](src/apply.rs):

```rust
pub fn rewrite_file_from_bytes(
    data: &[u8],
    engine: Engine,
    priority: Priority,
    sample_rows: usize,
) -> Result<RewriteResultWithOutput, AutoparqError>
```

Where:

```rust
pub struct RewriteResultWithOutput {
    pub output_bytes: Vec<u8>,
    pub rewrite: RewriteResult,                // input size, output size, actual %, elapsed ms
    pub per_column_diff: Vec<ColumnSizeDelta>, // per-column before/after compressed bytes
}

pub struct ColumnSizeDelta {
    pub column_name: String,
    pub before_compressed: i64,
    pub after_compressed: i64,
    pub before_encodings: Vec<String>,
    pub after_encodings: Vec<String>,
    pub before_codec: String,
    pub after_codec: String,
}
```

**Implementation**: mirrors `rewrite_file` but:
- `Bytes::copy_from_slice(data)` as reader source (no `std::fs::File`).
- `Vec<u8>` as writer sink (no `NamedTempFile`).
- After `writer.close()`, re-parse the output buffer via `read_file_metadata_from_bytes` to derive per-column `after` stats.
- Cross-reference with the input `FileProfile` (also parsed from `data`) to build `ColumnSizeDelta` entries.
- Cfg-gate `Instant` (same pattern as `tuner.rs` and `bench.rs`).

### A02 — WASM binding

Add to [src/wasm.rs](src/wasm.rs):

```rust
#[wasm_bindgen]
pub fn apply_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
) -> Result<JsValue, JsError>
```

Returns a structured object with two fields so JS can handle the bytes without a full JSON round-trip on a 100 MB buffer:

```rust
{
  output: Uint8Array,   // the rewritten parquet
  report_json: String,  // RewriteResultWithOutput minus `output_bytes`, serialised
}
```

Use `serde_wasm_bindgen` or return a hand-rolled `js_sys::Object` with both fields. The output bytes are **transferred, not cloned**, to avoid a 100+ MB memcpy.

### A03 — File-size gate for apply

Extend `check_file_size` or add a parallel `check_apply_file_size`:

- **Hard limit: 1 GB.** Matches the existing analyze hard limit. Rewrite peaks at ~2× input RAM (input buffer + output buffer + working batches), so 1 GB input ≈ 2+ GB peak, near the 4 GB WASM32 ceiling. Some memory-constrained browsers/devices will still OOM — show a clear warning and let the user try.
- **Soft warning: 200 MB.** "Large file — rewrite may take 30+ seconds and requires up to 2 GB of browser memory."
- **Severe warning: 500 MB.** "Rewriting this size may fail on memory-constrained browsers. If it fails, use the CLI snippet instead."

The Apply button is enabled up to the hard limit. Between 500 MB and 1 GB, the button shows the severe warning but stays clickable. Above 1 GB, the button is disabled with a pointer to the snippet panel.

**Acceptance criterion update (A03):** the UI warning tiers match these three thresholds; the hard cap is 1 GB.

### A04 — Worker + bridge plumbing

[web/src/workers/autoparq-worker.js](web/src/workers/autoparq-worker.js):
- Import `apply_file_bytes`.
- Add `apply` message type: reads `_lastFileData` (already cached for bench), calls the WASM function, posts `{ outputBytes, reportJson }` back. Use `transferables` so the output `Uint8Array.buffer` is transferred (zero-copy).

[web/src/wasm-bridge.js](web/src/wasm-bridge.js):
- `applyFile(engine, priority) -> { outputBytes: Uint8Array, rewrite: RewriteResult, perColumnDiff: ColumnSizeDelta[] }`.

### A05 — Download button in Summary

[web/src/render/summary.js](web/src/render/summary.js):

Below the "Recommended Settings" right-hand card, add a primary CTA block:

```html
<div class="mt-4 bg-accent-subtle border border-accent-emphasis/30 rounded-md p-5">
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div>
      <h3 class="font-semibold text-fg-default">Apply these recommendations</h3>
      <p class="text-sm text-fg-muted mt-1">
        Download a new copy of this file with the recommended encodings and codec.
        Runs locally in your browser — nothing is uploaded.
      </p>
    </div>
    <button id="apply-btn" class="bg-accent-emphasis text-fg-on-emphasis rounded-md px-4 py-2 font-medium hover:bg-accent-fg/90 transition-colors">
      Download optimized copy
    </button>
  </div>
</div>
```

On click: disable button → show "Rewriting…" → call `applyFile` → trigger download → swap the button for a result strip.

### A06 — Download trigger

Standard Blob-URL dance:

```js
const blob = new Blob([bytes], { type: 'application/octet-stream' });
const url = URL.createObjectURL(blob);
const a = document.createElement('a');
a.href = url;
a.download = suggestedFileName;   // '<original>_tuned.parquet'
a.click();
URL.revokeObjectURL(url);
```

Filename: `<originalBasename>_tuned.parquet` (derived from `_currentFileName` in [App.js](web/src/App.js)).

### A07 — Measured-overlay state in App

[web/src/App.js](web/src/App.js) gains:

```js
let _rewriteResult = null;       // { rewrite, perColumnDiff, engine, priority }
let _rewriteOutputBytes = null;  // retained for re-download; auto-discarded after 60s
let _rewriteDiscardTimer = null; // setTimeout handle
let _applying = false;           // gates engine/priority selectors while rewrite runs
```

When rewrite completes:
- Store all four fields. `engine` and `priority` are the values that were active at apply time.
- Start a 60s `setTimeout` that clears `_rewriteOutputBytes` (but not `_rewriteResult` — the measured overlay stays). Any "Download again" click resets the timer.
- `renderReport` is called with a single options object (C04): `{ benchColumnFn, onApply, getRewriteState, onDiscard, isApplying }`.

When engine/priority changes *after* apply completes: clear all state per PRD A10. While `_applying === true`, engine/priority selectors are disabled.

### A08 — Measured overlay in Summary

When `rewriteResult` is present, Summary replaces the estimated range with measured numbers:

```
Current size:       47.3 MB
Actual size after:  40.3 MB  [measured]
Size reduction:     −14.8%   [measured]
File health:        10 of 14 match
```

The `[estimated range]` labels become `[measured]` (green, like success emphasis).

Add a success strip at the top of the Summary:

```
✓ Applied. 47.3 MB → 40.3 MB (−14.8%).  [Download again]
   duckdb · balanced · downloaded copy expires in 52s
```

- Second line is muted; shows the engine/priority that were active at apply time, plus a countdown of the auto-discard window.
- Clicking "Download again" re-triggers the Blob download without re-running the rewrite AND resets the 60s discard timer.
- After the timer expires, the strip persists but the "Download again" button is replaced with "Re-apply to download" (which kicks off a fresh rewrite).

### A09 — Measured overlay on column cards

In [columns.js](web/src/render/columns.js), when `rewriteResult.perColumnDiff` is present:

- For each column, look up its `ColumnSizeDelta`.
- Add a small measured footer inside the card, below the existing `compactFooter` / `reasonRow`:

  ```
  Measured: 4.1 MB → 3.2 MB (−22%)
  ```

- Colour the delta: green if reduction ≥ 5%, neutral if between −5% and +5%, red if it actually grew.

- For Match cards: the measured delta confirms "no regression" — helpful reassurance.

- For previously-FallbackDictionary columns: the post-rewrite bytes are the receipt that the fallback is resolved.

### A10 — Engine/priority change invalidates the overlay

In `onEngineChange`:

```js
_rewriteResult = null;
_rewriteOutputBytes = null;
```

Re-render. The apply button resets to its initial state; the Summary returns to the estimated range.

---

## UX details

- **Primary CTA emphasis.** The Download button is the only primary-coloured button in the report; everything else (view toggles, filter) is secondary. This is the main action the page invites.
- **Privacy copy.** Explicit "Runs locally in your browser — nothing is uploaded." Reassures about a feature that *looks* like a server round-trip.
- **Progress honesty.** WASM has no per-column progress callback during rewrite. Show an indeterminate spinner with "Rewriting <N MB file>… may take up to 30 seconds."
- **Error handling.** If rewrite throws (unsupported encoding combo, OOM, etc.), show the error below the button and offer a "Copy snippet instead" fallback that scrolls to the existing snippet panel.
- **Re-download.** Cached bytes live until engine/priority change or until the user drops a new file. Re-download is one click, no rewrite.
- **Freeing memory.** Holding 500 MB of output bytes is real. After re-download the user can click a small "✕ Discard optimized copy" link to clear `_rewriteOutputBytes`.

---

## Acceptance Criteria

- [ ] Rust: `rewrite_file_from_bytes` returns output bytes + per-column before/after deltas for any valid Parquet input up to 1 GB.
- [ ] WASM binding `apply_file_bytes` works on the worker thread and transfers the output buffer without a full copy.
- [ ] A "Download optimized copy" button in the Summary produces a browser download of `<name>_tuned.parquet`.
- [ ] After apply, the Summary shows measured size reduction (not the estimated range).
- [ ] Each column card shows a measured before/after footer when the rewrite is present.
- [ ] Changing engine or priority clears the measured overlay and resets the apply button.
- [ ] Files above 1 GB are blocked with a clear "too large for in-browser apply" message plus a pointer to the CLI snippet; 500 MB–1 GB shows a severe OOM-risk warning; 200 MB–500 MB shows a mild "this will take a while" warning.
- [ ] Download re-use works without re-running the rewrite.
- [ ] No file bytes leave the browser (inspected via DevTools network tab during rewrite).
