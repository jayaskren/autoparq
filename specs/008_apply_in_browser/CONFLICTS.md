# CONFLICTS & OPEN QUESTIONS — Spec 008

## Decisions log

All open questions resolved:

- **C01 → (A) cfg-gate `tempfile` and `rewrite_file` together** for `cfg(not(target_arch = "wasm32"))`. New `rewrite_file_from_bytes` stays unconditional.
- **C04 → (B) refactor `renderReport` to an options object.** 4th arg becomes `{ benchColumnFn, onApply, getRewriteState, onDiscard }`. Minor call-site cleanup at both invocation points in `App.js`.
- **A01 → (A)+(C).** Disable engine/priority selectors during apply. Store `engine` + `priority` alongside `_rewriteResult` so the success strip shows which settings were applied. Engine/priority change *after* apply completes still clears the overlay per PRD A10.
- **A02 → (B) inline custom "Proceed anyway" banner** for the 500 MB–1 GB severe tier. No `window.confirm()`.
- **A03 → (B) auto-discard after 60 seconds** of no re-download activity. No explicit discard button. The success strip surfaces a countdown or a subtle "(discarding soon)" hint near the end.
- **A04 (answered Q04) — Yes.** Success strip includes the engine/priority that was applied, as muted text under the headline reduction number.

---


## Conflicts with actual code

### C01 — `tempfile` is not WASM-compatible and is a required import in `apply.rs`

**Location:** [src/apply.rs:9](src/apply.rs#L9) — `use tempfile::NamedTempFile;`

**Issue:** The `tempfile` crate is used by the existing `rewrite_file` function for atomic disk writes. When we build `apply.rs` for `wasm32-unknown-unknown`, the `tempfile` import will attempt to resolve and fail (tempfile depends on `std::fs` APIs not available in wasm32).

**Resolution:**

Either:
- **(A)** Cfg-gate the `tempfile` import *and* the entire `rewrite_file` function to `cfg(not(target_arch = "wasm32"))`. The new `rewrite_file_from_bytes` is wasm-safe and stays unconditional. CLI still uses `rewrite_file`; browser uses `rewrite_file_from_bytes`.
- **(B)** Move `rewrite_file` into a separate `apply/disk.rs` module behind `cfg(not(target_arch = "wasm32"))` and put the new bytes-in function in `apply/mod.rs`.

**Recommendation:** (A) is less disruptive. TASKS should add a step to `src/apply.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
use tempfile::NamedTempFile;

// ... existing `rewrite_file` ...
#[cfg(not(target_arch = "wasm32"))]
pub fn rewrite_file(...) -> Result<RewriteResult, AutoparqError> { ... }
```

**Action:** Update TASKS T01 (or insert T01a) to cfg-gate `tempfile` and `rewrite_file` together with the `Instant` change.

---

### C02 — `rewrite_file_from_bytes` parses the input twice

**Location:** PLAN Step 1.3 — the proposed implementation calls `build_tune_report_from_bytes` (which parses the input parquet to profile columns) and then `ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(data))` (which parses it again to read row groups).

**Issue:** Two footer parses per apply. Not a correctness issue — CPU only, and the existing CLI `rewrite_file` does the same thing (calls `build_tune_report` which reads the footer, then re-opens the file to stream rows). Worth noting so it isn't treated as a defect during review.

**Resolution:** Accept as-is. It mirrors the CLI behaviour. Optimising would require threading the `TuneReport` *plus* an open reader through the same function, which is a larger refactor.

---

### C03 — The `call` helper in `wasm-bridge.js` doesn't pass transferables on response

**Location:** [web/src/wasm-bridge.js:39](web/src/wasm-bridge.js#L39) — `onmessage` handler just does `pending.resolve(payload)`.

**Issue:** Not actually a bug. The Worker posts `[outputBytes.buffer]` as transferables when sending the response via `self.postMessage(msg, transferables)`. The main-thread `onmessage` handler just resolves the promise with the received payload, and the `Uint8Array` inside it is already owned by the main thread after the transfer. No bridge-side change needed.

**Action:** Verify during T18 that the `outputBytes.buffer` arriving in JS-main is detached (i.e., cannot be read from the Worker afterward). If a regression ever appears, the issue is likely in the Worker's `postMessage` call, not the bridge.

---

### C04 — `renderReport` signature is accumulating positional arguments

**Location:** [web/src/render/report.js:12](web/src/render/report.js#L12)

**Issue:** Current signature is 4 args: `(container, report, fileName, benchColumnFn)`. Spec 008 wants to add 3 more: `onApply`, `getRewriteState`, `onDiscard`. Total of 7 positional args. At that length, call sites become error-prone.

**Resolution options:**
- **(A)** Accept the 7-arg signature for consistency with the Spec-005/006/007 pattern. Add clear doc comments.
- **(B)** Refactor to an options object: `renderReport(container, report, fileName, { benchColumnFn, onApply, getRewriteState, onDiscard })`. Cleaner but touches unrelated code.

**Recommendation:** (B) as a small in-spec cleanup. Low-risk — single call site in App.js (actually two: `showReport` and `onEngineChange`), plus the prop-through into `renderSummary` and `renderColumns`.

**Decision needed.**

---

### C05 — `check_apply_file_size` JSON shape differs from `check_file_size`

**Location:** Existing [src/wasm.rs:93](src/wasm.rs#L93) returns `{ok, warning, error}`. PLAN Step 2.2 introduces a new `check_apply_file_size` that returns `{ok, warning, severity, error}` — extra `severity` field.

**Issue:** Two shapes for essentially the same thing. Consumers need to know which to parse.

**Resolution options:**
- **(A)** Keep two functions. Document the shape difference; the JS bridge has two typed wrappers already.
- **(B)** Add `severity` to the original `check_file_size` response too (defaulting to `"mild"`/`"blocked"`/null). One shape.

**Recommendation:** (A). The two operations have different thresholds and different user-facing consequences — two functions signals that clearly. JS side already has `checkFileSizeWasm` and will get `checkApplySize`; typed APIs protect callers.

---

## Ambiguities

### A01 — What happens if the engine/priority changes while apply is in flight?

**Context:** User clicks "Download optimized copy", apply starts (takes 5–30s), user changes engine mid-rewrite. The apply was computed against the old engine; when it completes, the measured overlay would be shown against the *current* (new) recommendations — a stale mismatch.

**Options:**
- **(A)** Disable the engine/priority selects while apply is in progress, like `_analyzing` already gates.
- **(B)** Cancel the in-flight apply on engine change (difficult: WASM doesn't support cancellation).
- **(C)** Show the measured overlay against the engine/priority that was in effect when apply was invoked (store it alongside `_rewriteResult`). If user changes engine after apply completes, clear as today.

**Recommendation:** (A) + (C). Disable the selects during apply (simplest, safest), and if apply completes before the user changes engine, the overlay is valid. If engine changes after, clear per existing A10. Record the "apply was computed against X engine/priority" in state for clarity in the success strip.

**Decision needed** — confirm (A)+(C).

---

### A02 — Severe-warning confirmation: `confirm()` modal or inline?

**Context:** TASKS T11 proposes `window.confirm()` for the 500 MB–1 GB tier. Native browser modal is quick but ugly and feels unprofessional.

**Options:**
- **(A)** `window.confirm()`. Zero code, accessible.
- **(B)** Custom inline warning with a "Proceed anyway" button that requires explicit second click. Cleaner, matches the Primer aesthetic.
- **(C)** No confirmation at severe tier — just surface the warning inline and let the first click proceed (accept the risk silently).

**Recommendation:** (B). The app is already visually refined; a native modal is a jarring break. The cost is ~20 extra lines in `summary.js`.

**Decision needed.**

---

### A03 — Discarding the cached output: explicit button or garbage-collect?

**Context:** PRD A09/A10 proposes a "✕ Discard optimized copy" link to free `_rewriteOutputBytes`. For a 1 GB file, that's 1 GB of retained memory.

**Options:**
- **(A)** Explicit discard button as specified.
- **(B)** Auto-discard after N seconds if no download is re-triggered.
- **(C)** Auto-discard after the first successful download, require re-apply for a second download.

**Recommendation:** (A). Predictable and user-controlled. Add a size hint inside the button: "✕ Discard optimized copy (1.0 GB)".

---

### A04 — Per-column `before_encodings` — which form?

**Context:** `ColumnSizeDelta` has `before_encodings: Vec<String>` and `after_encodings: Vec<String>`. The diagnostic field already uses the full multi-encoding set including `PLAIN_DICTIONARY` (post-spec-006 changes). We should reuse the same form so the UI can compare apples to apples.

**Recommendation:** Use `ColumnMetaSummary.encodings` (the aggregated Vec<String>, sorted, already filtered via Spec 006 changes). The `RewriteResultWithOutput` struct gets its stats from `read_file_metadata_from_bytes`, which produces `FileProfile`, which contains `ColumnMetaSummary` — so this falls out naturally.

**No decision needed.**

---

## Open Questions

### Q01 — Should we emit a per-batch progress callback during rewrite?

**Context:** WASM doesn't natively support progress callbacks inside a synchronous function. The bench feature already lives with this limitation and just shows "Benchmarking…".

**Resolution:** Out of scope for this spec. The rewrite shows an indeterminate spinner. Could add in a follow-on spec by plumbing a progress `js_sys::Function` callback into the writer loop (similar to `tune_file_bytes_with_progress`).

---

### Q02 — Do we need to clean up the Blob URL on the main thread?

**Context:** PLAN Step 3.3 uses `URL.createObjectURL` + `setTimeout(…, 2000)` to revoke. 2-second delay is a Safari workaround.

**Resolution:** Pattern is standard. If it leaks, the URL is tied to the document's lifetime and released on navigation/reload. Low risk.

---

### Q03 — Is `apply_file_bytes` safe to call concurrently?

**Context:** The worker is single-threaded, so two `apply` messages enqueue and run serially. Good. But what if the UI allows firing two applies rapidly?

**Resolution:** T11's "disable button while rewriting" already prevents this. The worker's single-threaded queue is a belt-and-suspenders guarantee.

---

### Q04 — Should the success strip mention which engine/priority was applied?

**Context:** Per A01 recommendation, we store the engine/priority at apply time. Surfacing this in the success strip — e.g., "✓ Applied with engine=duckdb priority=balanced" — helps users who experiment with multiple settings.

**Recommendation:** Yes. One extra line of muted text under the headline reduction number.

---

## Scope summary

**Files touched:**
- Rust (3): `src/apply.rs` (most of the work), `src/wasm.rs`, possibly `src/lib.rs` for re-exports.
- CSS (0): no new CSS needed; existing Primer tokens cover the CTA and success strip.
- JS new files (1): `web/src/lib/download.js`.
- JS modified (5): `App.js`, `wasm-bridge.js`, `workers/autoparq-worker.js`, `render/report.js`, `render/summary.js`, `render/columns.js`.

**Estimated effort:**
- Rust Phase 1: 2–3 h (T01–T03 including tests).
- WASM Phase 2: 30 min (T04–T06).
- Worker/bridge Phase 3: 1 h (T07–T09).
- UI Phase 4: 3–4 h (T10–T15, most work is in T11 + T12).
- Verification Phase 5: 30 min.

**Total:** ~8–10 hours of focused work. Half a day if nothing surprising surfaces.
