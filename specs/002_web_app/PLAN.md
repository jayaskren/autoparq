# autoparq Web App — Implementation Plan

## Approach

The implementation splits into three independent tracks that converge in the final phase:

- **Track A — Rust WASM core**: Feature-flag the existing Rust library for WASM compilation; add a bytes-based profiler path; expose `#[wasm_bindgen]` entry points
- **Track B — Code generation**: Extend the existing `codegen` module with PySpark and Polars snippet variants
- **Track C — Frontend**: Scaffold the `web/` directory; implement the full UI from drop zone through report rendering

Tracks A and B can begin immediately and run concurrently. Track C's WASM integration phase (W09) requires Track A to complete first, but the scaffolding and most of the rendering work can proceed against a mock report payload.

---

## Phase 1 — Rust WASM Foundation (Track A)

### Phase 1a: Cargo.toml feature flag isolation

**What:** Add `python` and `wasm` features to `Cargo.toml`. Move `rayon` to `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`. Add WASM-specific dependencies (`wasm-bindgen`, `js-sys`, `console_error_panic_hook`) as optional. Add `getrandom` with the `"js"` feature for the wasm32 target (required by the parquet/arrow transitive dependency chain). Gate the existing PyO3 code in `src/lib.rs` behind `#[cfg(feature = "python")]`.

**Confidence: 90%** — Feature flag pattern is standard; the only risk is discovering an unexpected dependency that pulls in non-WASM-safe code. The parquet and arrow crates are tested on WASM in the Arrow project itself.

**Risk (LOW):** `arrow` and `parquet` crates may pull in `chrono` which has a WASM-specific TZ issue. Mitigate with `chrono = { features = ["wasmbind"] }` if needed.

**Validation:** `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` compiles without error.

---

### Phase 1b: Profiler bytes path

**What:** Add `profile_cursor(data: &[u8]) -> Result<FileProfile>` to the profiler. The parquet crate's `ParquetRecordBatchReaderBuilder` is already generic over `T: ChunkReader`. `bytes::Bytes` implements `ChunkReader` and is already a transitive dependency. No new trait definitions needed.

Refactor the shared profiling logic into a generic `profile_chunk_reader<R: ChunkReader>()` internal function. The existing `profile_path(&Path)` opens the file and calls it; the new `profile_cursor` wraps the `&[u8]` in `bytes::Bytes::copy_from_slice()` and calls the same function.

Add `tune_from_cursor(data: &[u8], config: TuneConfig)` to `tuner.rs` alongside the existing `tune_from_path`.

**Confidence: 85%** — `bytes::Bytes` implementing `ChunkReader` is documented in the parquet crate. The main uncertainty is whether `ParquetRecordBatchReaderBuilder::try_new()` is generic enough to accept it without wrapping. If not, `std::io::Cursor<Vec<u8>>` implements `Read + Seek` and is a reliable fallback.

**Risk (MEDIUM):** The sampler currently calls `ParquetRecordBatchReaderBuilder::try_new(file)` where `file: std::fs::File`. Changing this to be generic requires threading the type parameter through the sampler's call sites. Scope the change carefully to avoid breaking the existing PyO3 path.

**Validation:** Unit test `profile_cursor` against one of the existing fixture files loaded via `std::fs::read()`.

---

### Phase 1c: WASM entry point

**What:** Create `src/wasm.rs`. Expose four `#[wasm_bindgen]` functions:

1. `tune_file_bytes(data, engine, priority, sample_rows)` → `Result<String, JsError>` — full analysis, returns `TuneReport` JSON
2. `tune_file_bytes_with_progress(data, engine, priority, sample_rows, on_progress)` — same but fires the JS callback with `(current, total, col_name)` per column
3. `generate_snippet(report_json, engine)` — pure transformation, returns code string
4. `check_file_size(byte_len)` → JSON `{ok, warning, error}`

Register `console_error_panic_hook` in a `#[wasm_bindgen(start)]` function.

**Confidence: 85%** — `JsError` and `js_sys::Function::call3` are stable wasm-bindgen APIs. The progress callback pattern (synchronous JS callback from inside WASM computation) is well-established.

**Risk (LOW):** The progress callback requires threading a `Fn` through the profiler call chain. Use a `Option<Box<dyn Fn(usize, usize, &str)>>` parameter at the `tuner.rs` level, defaulting to `None` for the non-progress path.

**Validation:** `wasm-pack build --target web --release --features wasm -- --no-default-features` produces a `pkg/` directory. Load in a minimal HTML test page and call `tune_file_bytes` with a fixture file's bytes.

---

## Phase 2 — Code Generation Extensions (Track B)

### Phase 2a: PySpark and Polars snippet variants

**What:** Add `pyspark` and `polars` generator functions to `src/codegen/`. The existing `generate_python_snippet` in `codegen.py` (Python side) currently generates PyArrow snippets. Move snippet generation into Rust in a new `src/codegen/` module (or extend the existing one) so all three variants are callable from WASM.

PySpark snippet notes:
- Per-column encoding hints require Spark 3.4+; include comment
- `ZSTD` requires Spark 3.2+; include caveat comment when Spark engine selected
- Use `df.write.option("parquet.compression", "zstd")` API form

Polars snippet notes:
- `pl.scan_parquet` / `pl.DataFrame.write_parquet` API
- Polars does not support per-column encoding hints via Python API as of Polars 0.20; emit a note

Expose `generate_snippet(report_json: &str, engine: &str)` at the Rust library level so both PyO3 and WASM bindings call the same function.

**Confidence: 80%** — Polars write API is straightforward. Spark API form requires verifying the correct `DataFrameWriter` option keys.

**Risk (LOW):** Polars per-column encoding support may have changed since knowledge cutoff; add a version comment in the generated snippet and mark it as something to verify.

**Validation:** Unit test each generator against the `multi_column` fixture report and assert the output is valid Python syntax (parse with Python `ast.parse` in pytest).

---

## Phase 3 — Frontend Scaffold and Core UI (Track C)

### Phase 3a: Project scaffold

**What:** Create `web/` directory with `package.json`, `vite.config.js`, `index.html`, `src/style.css`, `src/main.js`. Install: `vite`, `vite-plugin-wasm`, `vite-plugin-top-level-await`, `@tailwindcss/vite`, `tailwindcss`, `tabulator-tables`, `shiki`.

The `index.html` contains: sticky header slot, hero/drop zone section, progress section (hidden), report section (hidden). All state transitions are class-based (Tailwind `hidden`), not page navigations.

**Confidence: 95%** — Standard Vite setup; `vite-plugin-wasm` is the documented integration path for wasm-pack output.

**Validation:** `npm run dev:nowasm` starts without error and serves the empty shell page at localhost:5173.

---

### Phase 3b: Drop zone and analysis flow

**What:** Implement `App.js` and `ui.js`. The drop zone handles drag-over, drag-leave, drop, and file input change events. Validates file type (`.parquet` extension) and file size. Shows progress through named phases (init → read → analyze → render) with a real progress bar. Engine and priority selectors rendered in the options bar. Transitions between landing/analyzing/report states.

For Phase 3b, call a mock `analyzeFile` that returns a hardcoded `TuneReport` JSON object after a 2-second delay — this lets the frontend be developed and tested without the WASM build completing.

**Confidence: 90%** — Standard browser file API and DOM manipulation.

**Validation:** Drop a `.parquet` file; progress bar advances through named phases; mock report section becomes visible.

---

### Phase 3c: Report rendering — summary and column cards

**What:** Implement `render/report.js`, `render/summary.js`, and the column card components. Each column renders as an expandable card sorted by impact stars descending. The expand/collapse animation is a CSS height transition. Confidence badges use the three-color system. The one-sentence reason string is always visible. The filter bar (all columns / high impact / name search) filters the rendered card list.

**Confidence: 85%** — Pure DOM rendering against a known JSON schema. The main complexity is the expand/collapse height animation (content height is dynamic; use `max-height` transition with `overflow: hidden`).

**Risk (LOW):** For files with 50+ columns, rendering 50 expanded-capable cards simultaneously may cause initial render jank. Mitigate by rendering cards lazily (IntersectionObserver) or deferring the Tabulator table to a separate tab.

**Validation:** Mock report with 18 columns renders all cards. Expand/collapse works. Filter by name hides non-matching cards. Sort by impact reorders cards.

---

### Phase 3d: Codec option cards and code snippet panel

**What:** Implement `render/codec-cards.js` and `components/SnippetPanel.js`. Three codec option cards (Balanced / Smallest / Fastest) with qualitative tradeoff text. "Get snippet" button scrolls to and activates the code panel. Code panel shows engine tabs (PyArrow / PySpark / Polars) and bundle selector. Shiki highlights the Python code. Copy button with 2-second state change. Version caveats shown as plain text below the code block.

**Confidence: 85%** — Shiki lazy-import is the main complexity; test that it loads correctly in the Vite module graph.

**Validation:** All three engine tabs render syntax-highlighted Python code. Copy button works. Switching engine updates the snippet. Switching bundle updates the codec in the snippet.

---

### Phase 3e: Advisories, caveats, and glossary tooltips

**What:** Implement `render/advisories.js` and `render/caveats.js`. Row group and sort order advisories shown only when relevant. Caveats list at the bottom with ⚠/ℹ icons. Glossary tooltips on dotted-underline terms throughout the UI — implement as a single shared `Tooltip` component positioned via `getBoundingClientRect()` and `position: fixed`.

**Confidence: 80%** — Tooltip positioning edge cases (viewport overflow) require careful handling. Use a lightweight tooltip implementation rather than a library.

**Validation:** Load a mock report with row group advisory triggered. Advisory panel appears. Hover over `DELTA_BINARY_PACKED` anywhere in the report — tooltip appears with definition, stays open when cursor moves into it.

---

## Phase 4 — WASM Integration

### Phase 4a: Wire WASM to frontend

**What:** Implement `wasm-bridge.js` and the Web Worker (`src/workers/autoparq-worker.js`). The main thread hands the `ArrayBuffer` to the worker via transfer semantics. The worker loads the WASM module, calls `tune_file_bytes_with_progress`, and posts progress/result/error messages back. The main thread's `analyzeFile()` returns a Promise that resolves with the parsed `TuneReport`.

Replace the mock `analyzeFile` in `App.js` with the real WASM-backed version.

**Confidence: 80%** — Web Worker + WASM module loading is a known pattern. The main risk is WASM initialization in a Worker context, which requires the worker to be a `type: "module"` worker (supported in all modern browsers). Vite handles this with `new Worker(new URL(...), { type: 'module' })`.

**Risk (MEDIUM):** `wasm-pack --target web` output imports from a relative path. Inside a Worker, the relative path resolution differs from the main thread. Verify that the `init()` call in the Worker resolves the `.wasm` file URL correctly, or use `--target no-modules` with a `importScripts()` pattern as a fallback.

**Validation:** Drop a real `.parquet` fixture file. Analysis completes. Report renders with real data. Progress bar advances per column during analysis.

---

### Phase 4b: Engine selector re-render

**What:** Engine selector in the sticky header triggers a re-render of the report from the cached `TuneReport` JSON. The engine change must call `tune_file_bytes` again (engine affects recommendations) or — preferably — cache the `FileProfile` separately and call a `recommend_from_profile(profile_json, engine, priority)` WASM function that skips re-profiling.

Evaluate which approach is cleaner. If re-running the full analysis for a 50 MB file takes < 500ms (because the file bytes are already in Worker memory), re-profiling is simpler. If it is noticeable, add a `recommend_from_profile` WASM entry point.

**Confidence: 75%** — The re-profiling time depends on whether the Worker retains the `Uint8Array` after the first analysis. Workers can retain the bytes in a module-scope variable; the tradeoff is memory usage.

**Validation:** Change engine selector from DuckDB to Spark. Report recommendations update within 1 second. Spark-specific caveats appear on DELTA_BINARY_PACKED columns.

---

## Phase 5 — Polish, Sample Files, and Deployment

### Phase 5a: Sample files

**What:** Select or generate three representative sample Parquet files (< 15 MB each) that demonstrate different recommendation profiles:
- A file where most columns get RLE_DICTIONARY (e.g., a log table with status codes, user IDs)
- A file with DELTA_BINARY_PACKED triggers (sequential IDs, timestamps)
- A mixed file with floats (BYTE_STREAM_SPLIT) and a high-entropy column (UNCOMPRESSED)

Host sample files in `web/public/samples/`. Landing page buttons trigger the same `handleFile` flow with a `fetch()` response converted to a `File`-like object.

**Confidence: 90%** — The existing fixture generator in `examples/gen_fixtures.rs` can produce these. Export as larger files (500K–5M rows) for more realistic demo behavior.

---

### Phase 5b: End-to-end testing and polish

**What:** Manual test matrix across Chrome, Firefox, and Safari. Verify:
- Drop zone drag-and-drop
- Progress per column
- All three engine variants in code snippet
- Copy button (including `http://` fallback for `python3 -m http.server`)
- Engine selector change
- Expand/collapse all column cards
- Glossary tooltip stays open on hover
- Advisory panels visible for appropriate files
- Sample file buttons

Fix visual regressions and accessibility issues (keyboard navigation for tabs, focus management after expand/collapse).

---

### Phase 5c: CloudFront deployment

**What:** `npm run build` → upload `web/dist/` to S3. Create or update CloudFront distribution. Verify that the `.wasm` file is served with `Content-Type: application/wasm` (required for WASM streaming compilation). Set appropriate cache headers: long TTL for hashed assets, short TTL for `index.html`.

No `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy` headers required (no WASM threads).

---

## Critical Path

```
Phase 1a → Phase 1b → Phase 1c → Phase 4a → Phase 4b → Phase 5
Phase 2a ─────────────────────────────────────────────────────┘
Phase 3a → Phase 3b → Phase 3c → Phase 3d → Phase 3e → Phase 4a
```

The critical path is: **Rust WASM foundation → WASM entry point → Frontend WASM integration**. Everything else can proceed in parallel. The frontend can be developed against a mock payload up through Phase 3e, deferring the real WASM integration to Phase 4a.

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| parquet crate WASM compilation fails due to unsupported sys call | Medium | High | Test `cargo check --target wasm32` early (Phase 1a); fallback is to stub out the entropy computation which uses OS randomness |
| `bytes::Bytes` doesn't implement `ChunkReader` in the current version | Low | Medium | Fallback: `Cursor<Vec<u8>>` which implements `Read + Seek` and is accepted by `SerializedFileReader::new()` |
| WASM bundle > 3 MB uncompressed exceeds CloudFront free tier meaningfully | Low | Low | Disable unused arrow features; accept the size for a developer tool |
| Web Worker WASM init fails in Safari | Medium | Medium | Test early; fallback is running WASM on the main thread (blocks UI but functionally correct) |
| Shiki lazy-load fails in Vite worker context | Low | Low | Shiki is only loaded on the main thread for code highlighting; not in the Worker |
| wasm-pack `--target web` `.wasm` URL resolution fails in Worker | Medium | High | Test in Phase 4a immediately; fallback is `--target no-modules` with `importScripts()` |
