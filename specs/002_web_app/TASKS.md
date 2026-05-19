# autoparq Web App — Agent Task List

## How to Use This File

Each task is scoped for a single specialized agent. Tasks within the same milestone that share no output files can be assigned concurrently. Validation tasks must run after all implementation tasks in their milestone complete.

### Validation / Fix Loop Protocol

After every validation task:
- **PASS**: proceed to the next milestone
- **FAIL**: launch a fix agent with the validation output as input; re-run the validation after the fix; repeat until PASS

---

## Milestone W1 — Rust WASM Foundation

Tasks W01–W03 can run concurrently. W04 depends on W01 and W02.

---

### W01 — Cargo.toml Feature Flag Isolation

**Assign to:** Rust build tooling agent

**Goal:** Make the crate compile for both `wasm32-unknown-unknown` (WASM) and the existing native/PyO3 targets using feature flags. No existing functionality should change.

**Files to modify:**
- `Cargo.toml`
- `src/lib.rs`

**Cargo.toml changes required:**

1. Add features section:
```toml
[features]
default = []
python = ["dep:pyo3"]
wasm   = ["dep:wasm-bindgen", "dep:js-sys", "dep:console_error_panic_hook"]
```

2. Make pyo3 optional:
```toml
[dependencies.pyo3]
version  = "0.22"
features = ["extension-module"]
optional = true
```

3. Add wasm-bindgen optional deps:
```toml
[dependencies.wasm-bindgen]
version  = "0.2"
optional = true

[dependencies.js-sys]
version  = "0.3"
optional = true

[dependencies.console_error_panic_hook]
version  = "0.1"
optional = true
```

4. Move rayon to non-wasm target:
```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
rayon = "1.10"
```

5. Add WASM-required getrandom:
```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom = { version = "0.2", features = ["js"] }
```

6. Add bytes dep (needed for cursor path; already likely transitive but make explicit):
```toml
bytes = "1"
```

**src/lib.rs changes required:**
- Wrap all existing PyO3 content with `#[cfg(feature = "python")]`
- Rayon use statements in all files must be wrapped with `#[cfg(not(target_arch = "wasm32"))]`

**Acceptance criteria:**
- `cargo check --features python` succeeds (existing PyO3 path)
- `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` succeeds
- `cargo test` continues to pass all existing tests

---

### W02 — Profiler Bytes Path

**Assign to:** Rust core agent

**Goal:** Add a `profile_cursor(data: &[u8])` path to the profiler so WASM can pass file bytes directly without a file path. The existing `profile_path(&Path)` must remain unchanged.

**Files to modify:**
- `src/profiler/metadata.rs` (or wherever `profile_path` currently lives)
- `src/tuner.rs`

**Implementation approach:**

The parquet crate's `ParquetRecordBatchReaderBuilder::try_new()` is generic over `T: ChunkReader`. `bytes::Bytes` implements `ChunkReader`. Refactor to:

```rust
// Internal generic function
fn profile_chunk_reader<R: ChunkReader + 'static>(
    reader: R,
    config: &ProfileConfig,
) -> Result<FileProfile, AutoparqError> {
    // existing profiling logic moved here
}

// Existing public entry point — unchanged
pub fn profile_path(path: &Path, config: &ProfileConfig) -> Result<FileProfile, AutoparqError> {
    let file = std::fs::File::open(path)
        .map_err(|e| AutoparqError::FileNotFound(e.to_string()))?;
    profile_chunk_reader(file, config)
}

// New public entry point for WASM
pub fn profile_cursor(data: &[u8], config: &ProfileConfig) -> Result<FileProfile, AutoparqError> {
    let bytes = bytes::Bytes::copy_from_slice(data);
    profile_chunk_reader(bytes, config)
}
```

If `bytes::Bytes` does not implement `ChunkReader` in the current parquet version, fall back to:
```rust
let cursor = std::io::Cursor::new(data.to_vec());
// Use SerializedFileReader::new(cursor) instead
```

**Add to `src/tuner.rs`:**
```rust
pub fn tune_from_cursor(
    data: &[u8],
    engine: &Engine,
    priority: &Priority,
    sample_rows: usize,
    explain: &str,
) -> Result<TuneReport, AutoparqError> {
    let file_profile = crate::profiler::profile_cursor(data, /* appropriate config */)?;
    build_tune_report_from_profile(file_profile, engine, priority, sample_rows, explain)
}
```

Refactor the existing `build_tune_report` in `tuner.rs` to separate I/O (reading the file) from the profiling+recommendation logic so both `tune_from_path` and `tune_from_cursor` share the same downstream logic.

**Acceptance criteria:**
- New unit test: load `tests/fixtures/multi_column.parquet` via `std::fs::read()`, pass bytes to `profile_cursor`, assert `FileProfile.num_columns == 6`
- Existing `build_tune_report` (path-based) tests continue to pass
- `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` still succeeds after this change

---

### W03 — PySpark and Polars Snippet Generators

**Assign to:** Rust codegen agent

**Goal:** Add PySpark and Polars code snippet generators to the Rust `codegen` module so all three engine variants (PyArrow, PySpark, Polars) are generated by Rust and callable from both PyO3 and WASM.

**Files to create/modify:**
- `src/codegen.rs` or `src/codegen/mod.rs` (add `pyspark` and `polars` generators)
- Expose a unified `generate_snippet(report: &TuneReport, engine: &str) -> String` function

**PySpark snippet requirements:**
- Use `df.write.option("parquet.compression", codec)` API form
- Include per-column encoding hints via `spark.conf.set("parquet.column.codec.xxx", "yyy")` (Spark 3.4+)
- Generated snippet must include comment: `# Note: per-column encoding hints require Spark 3.4+`
- If codec is ZSTD, include comment: `# ZSTD requires Spark 3.2+`
- If engine override resulted in SNAPPY (Spark safety), include comment: `# SNAPPY used for Spark cross-version safety`

**Polars snippet requirements:**
- Use `df.write_parquet(path, compression="zstd", compression_level=3)` API form
- Note: Polars does not support per-column encoding via Python API (as of 0.20); generate snippet with file-level codec only and include comment explaining this limitation
- Comment: `# Per-column encoding not configurable via Polars Python API — applied at write time by the Parquet writer`

**Unified interface:**
```rust
pub fn generate_snippet(report: &TuneReport, engine_str: &str) -> String {
    match engine_str {
        "pyarrow" | "pandas" | "unknown" => generate_pyarrow_snippet(report),
        "pyspark" | "spark"              => generate_pyspark_snippet(report),
        "polars"                         => generate_polars_snippet(report),
        _                                => generate_pyarrow_snippet(report),
    }
}
```

Note: DuckDB is not included as a snippet variant. DuckDB's Python API does not support per-column encoding hints; users should select DuckDB as their engine for tailored recommendations, then use the PyArrow snippet to write the file.
```

Update the Python `codegen.py` to call into `_lib.generate_snippet(report_json, engine)` rather than containing its own template strings. This ensures CLI and web produce identical output.

**Acceptance criteria:**
- Unit tests assert PySpark snippet contains `spark.conf.set` calls for each changed column
- Unit tests assert Polars snippet contains `write_parquet` with the recommended codec
- Unit tests assert the Spark 3.4+ comment is present in PySpark output
- `ast.parse(snippet)` succeeds for all three variants (Python pytest)
- Existing `autoparq tune --output json` still includes `python_snippet` field (PyArrow)

---

### V-W1 — Milestone W1 Validation

**Assign to:** Validation agent

**Run after:** W01, W02, W03 all complete

**Checks:**

1. `cargo check --features python` — must succeed
2. `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` — must succeed
3. `cargo test` — all existing tests pass
4. `python -c "import autoparq; print('ok')"` after `maturin develop --features python` — must succeed
5. New test: `profile_cursor` round-trip on `multi_column.parquet`
6. PySpark/Polars snippet generators produce valid Python (`ast.parse`)
7. `generate_snippet(report, "pyspark")` output contains `spark.conf.set` per changed column

---

## Milestone W2 — WASM Entry Point

W04 depends on W01 and W02 completing.

---

### W04 — `src/wasm.rs` Entry Points

**Assign to:** Rust WASM binding agent

**Goal:** Create the WASM module entry point that the JavaScript frontend calls.

**Files to create:**
- `src/wasm.rs`

**Files to modify:**
- `src/lib.rs` (add `#[cfg(feature = "wasm")] mod wasm;`)

**Required exports (all gated with `#[cfg(feature = "wasm")]`):**

```rust
use wasm_bindgen::prelude::*;

// Called once at WASM module load
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
}

// Full analysis — returns TuneReport as JSON string
#[wasm_bindgen]
pub fn tune_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
) -> Result<String, JsError>

// Same but fires JS callback per column: fn(current: u32, total: u32, col_name: &str)
#[wasm_bindgen]
pub fn tune_file_bytes_with_progress(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
    on_progress: &js_sys::Function,
) -> Result<String, JsError>

// Re-run recommendations from cached FileProfile JSON — no re-profiling.
// profile_json is extracted from the TuneReport returned by the first analysis.
// Called when the user changes engine or priority after initial analysis.
#[wasm_bindgen]
pub fn recommend_from_profile(
    profile_json: &str,
    engine: &str,
    priority: &str,
) -> Result<String, JsError>  // Returns TuneReport JSON

// Pure transformation — no re-profiling
// engine: "pyarrow" | "pyspark" | "polars"
#[wasm_bindgen]
pub fn generate_snippet(report_json: &str, engine: &str) -> Result<String, JsError>

// Returns JSON: { "ok": bool, "warning": string|null, "error": string|null }
#[wasm_bindgen]
pub fn check_file_size(byte_len: usize) -> String
```

**`recommend_from_profile` requirements:**
- `FileProfile` must derive `Deserialize` (add if not already present)
- Deserialize `FileProfile` from `profile_json`, run recommender + advisor passes, return `TuneReport`
- `TuneReport` must include a `file_profile` field (serialized `FileProfile`) so the Worker can extract and cache it after the first `tune_file_bytes_with_progress` call. Add this field if not present.
- The function must be < 50ms for any `FileProfile` size (pure CPU, no I/O)

**Progress callback threading:**
The `on_progress` callback must be passed into `tune_from_cursor`. Add an optional progress callback parameter to `tuner.rs`:

```rust
pub fn tune_from_cursor_with_progress<F: Fn(usize, usize, &str)>(
    data: &[u8],
    engine: &Engine,
    priority: &Priority,
    sample_rows: usize,
    explain: &str,
    on_progress: F,
) -> Result<TuneReport, AutoparqError>
```

The existing `tune_from_cursor` passes `|_, _, _| {}` as the no-op callback.

**Error handling rules:**
- All `AutoparqError` variants must surface as `JsError::new(&e.to_string())`
- `AutoparqError` display strings must be user-readable (not Rust internals): "Not a valid Parquet file", "Column 'x' not found", etc.
- Never panic in WASM entry points; all `?` operators must be handled

**File size limits in `check_file_size`:**
- `byte_len > 1_073_741_824` (1 GB): `{ ok: false, error: "File exceeds 1 GB. Use the autoparq CLI for large files." }`
- `byte_len > 209_715_200` (200 MB): `{ ok: true, warning: "Large file (NNN MB). Analysis may take 10–30 seconds." }`
- Otherwise: `{ ok: true, warning: null, error: null }`

**Acceptance criteria:**
- `wasm-pack build web --target web --release --features wasm -- --no-default-features` completes and produces `web/pkg/autoparq.js` and `web/pkg/autoparq_bg.wasm`
- Manual test: load `web/pkg/autoparq.js` in a minimal HTML page, call `tune_file_bytes` with bytes from `tests/fixtures/multi_column.parquet` — returns valid JSON with 6 columns

---

### V-W2 — Milestone W2 Validation

**Assign to:** Validation agent

**Run after:** W04 complete

**Checks:**

1. `wasm-pack build .. --target web --release --features wasm -- --no-default-features` — must succeed
2. `ls web/pkg/*.wasm` — `.wasm` file must exist
3. WASM binary size: `wc -c web/pkg/autoparq_bg.wasm` — must be < 8 MB uncompressed
4. Load minimal HTML test page (create in `web/test/wasm-smoke.html`): call `tune_file_bytes` with `multi_column.parquet` bytes, assert response is valid JSON with `columns` array of length 6
5. Call `check_file_size(250_000_000)` — response must include a `warning` field
6. Call `tune_file_bytes` with 10 bytes of zeros — must throw a JS Error (not silently return garbage)

---

## Milestone W3 — Frontend Scaffold and Core UI

W05–W07 can run concurrently with W04 (no WASM dependency until W08).

---

### W05 — Web Project Scaffold

**Assign to:** Frontend scaffold agent

**Goal:** Create the `web/` directory with a working Vite dev server and the base HTML shell.

**Files to create:**

`web/package.json`:
```json
{
  "name": "autoparq-web",
  "private": true,
  "type": "module",
  "scripts": {
    "wasm:build":     "wasm-pack build .. --target web --out-dir pkg --release --features wasm -- --no-default-features",
    "wasm:build:dev": "wasm-pack build .. --target web --out-dir pkg --dev --features wasm -- --no-default-features",
    "dev":            "npm run wasm:build:dev && vite",
    "dev:nowasm":     "vite",
    "build":          "npm run wasm:build && vite build",
    "preview":        "vite preview"
  },
  "devDependencies": {
    "vite": "^5.0.0",
    "vite-plugin-wasm": "^3.3.0",
    "vite-plugin-top-level-await": "^1.4.0",
    "@tailwindcss/vite": "^4.0.0",
    "tailwindcss": "^4.0.0"
  },
  "dependencies": {
    "tabulator-tables": "^6.2.0",
    "shiki": "^1.0.0"
  }
}
```

`web/vite.config.js`:
```javascript
import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [wasm(), topLevelAwait(), tailwindcss()],
  build: {
    target: 'es2022',
    assetsInlineLimit: 0,  // never inline .wasm as base64
  },
  resolve: {
    alias: { '@wasm': '/pkg' },
  },
  root: '.',
  publicDir: 'public',
});
```

`web/index.html` — complete HTML shell with:
- `<meta>` tags, favicon reference
- Sticky header: logo, tagline, engine selector slot (empty), privacy badge
- Hero section: drop zone (dashed border), browse file input, privacy notice, sample file buttons
- Options bar (engine/priority selectors)
- Progress section (hidden by default)
- Report section (hidden by default)
- `<script type="module" src="/src/main.js">`

`web/src/style.css`:
```css
@import "tailwindcss";

@theme {
  --color-confidence-high-text: #166534;
  --color-confidence-high-bg: #dcfce7;
  --color-confidence-medium-text: #854d0e;
  --color-confidence-medium-bg: #fef9c3;
  --color-confidence-low-text: #991b1b;
  --color-confidence-low-bg: #fee2e2;
}
```

`web/src/main.js` — imports `style.css` and `App.js`, calls `App.init()`

`web/.gitignore`:
```
pkg/
dist/
node_modules/
```

**Acceptance criteria:**
- `npm install` succeeds in `web/`
- `npm run dev:nowasm` starts Vite dev server at localhost:5173 without error
- `index.html` loads in browser showing the drop zone and privacy notice

---

### W06 — Drop Zone, Progress, and State Machine

**Assign to:** Frontend UI agent

**Goal:** Implement the file drop interaction and analysis state machine. Uses a mock `analyzeFile` returning fake data after a 2-second delay.

**Files to create:**
- `web/src/App.js`
- `web/src/ui.js`
- `web/src/mock-report.js` (hardcoded `TuneReport` JSON for development)

**`App.js` responsibilities:**
- Attach drag-over / drag-leave / drop event listeners to drop zone
- Attach change listener to file input
- Validate file: extension must be `.parquet`; call `check_file_size` (mock version returning ok) for size warnings
- On file accepted: collapse hero, show progress section, call `analyzeFile(file, options, onProgress)`
- On analysis complete: hide progress, show report section, call `renderReport(container, report, fileName)`
- On error: hide progress, show error panel with message and "Try another file" button
- Engine/priority selector values read from DOM at analysis time

**`ui.js` responsibilities:**
- `showProgress(message, pct)` — updates progress bar width and label text
- `hideProgress()` — hides progress section
- `showError(title, message)` — renders error panel
- `renderEngineSelector(container)` — builds the engine select element (Unknown / Spark / DuckDB / Polars/PyArrow / ClickHouse / pandas)
- `renderPrioritySelector(container)` — builds the priority select (Balanced / Size / Speed)

**Mock `analyzeFile` in `App.js` (replaced in W08):**
```javascript
async function analyzeFile(file, options, onProgress) {
  onProgress({ message: 'Loading analyzer…', pct: 10 });
  await new Promise(r => setTimeout(r, 500));
  onProgress({ message: 'Reading file…', pct: 30 });
  await new Promise(r => setTimeout(r, 500));
  onProgress({ message: 'Profiling columns…', pct: 60 });
  await new Promise(r => setTimeout(r, 1000));
  onProgress({ message: 'Building report…', pct: 90 });
  await new Promise(r => setTimeout(r, 300));
  return import('./mock-report.js').then(m => m.MOCK_REPORT);
}
```

**`mock-report.js`:** A complete hardcoded `TuneReport` object with 6 columns covering all recommendation types: one RLE_DICTIONARY, one DELTA_BINARY_PACKED, one BYTE_STREAM_SPLIT, one PLAIN, one UNCOMPRESSED (high entropy), one boolean. Includes row group advisory, sort advisory, and at least one caveat.

**Acceptance criteria:**
- Drop a file (any `.parquet`); progress bar advances through named phases
- After delay, report section becomes visible (even if empty/placeholder)
- Drop a `.csv` file; inline error message appears, hero remains
- Engine selector renders all options
- "Try another file" on error resets to hero state

---

### W07 — Report Rendering: Summary and Column Cards

**Assign to:** Frontend report rendering agent

**Goal:** Implement the summary panel and per-column recommendation cards against the mock report.

**Files to create:**
- `web/src/render/report.js`
- `web/src/render/summary.js`
- `web/src/components/ConfidenceBadge.js`
- `web/src/components/ImpactStars.js`

**Summary panel (`render/summary.js`):**
- File name, size, row count, column count
- Estimated reduction % (labeled `[estimated]`)
- Estimated read speedup (labeled `[estimated]`)
- Breakdown: N cols RLE_DICTIONARY, N cols DELTA_BINARY_PACKED, N cols BYTE_STREAM_SPLIT, N cols PLAIN
- Codec: the primary recommended codec

**Column cards layout (in `render/report.js` initially, refactor later):**
- One card per column in `report.columns`, sorted by `impact_stars` descending
- Each card shows: column name, impact stars, confidence badge, physical type, null%, the most relevant statistic (cardinality_ratio, monotonicity_score, or byte_entropy — whichever is non-null), recommended encoding, recommended codec, `reason_brief`
- Expand chevron `▼` on the right; clicking expands the card in-place
- Expanded content sections (each independently collapsible as `<details>` elements):
  - "Why this encoding?" — `full_explain.reasoning_chain` as a tree (if present), else `reason_brief` in paragraph form
  - "Raw statistics" — key/value table from `full_explain.raw_stats`
  - "Alternatives considered" — list from `full_explain.alternatives_considered`
  - "Engine compatibility" — hardcoded table for the recommended encoding

**View toggle and filter bar:**
- View toggle buttons above the filter bar: `[Cards ◉] [Table ○]`
- Toggle state persists for the session; filter and sort controls apply to both views
- Name search input: filters by `column_name` substring (case-insensitive)
- Sort select: "High impact first" / "Column name A–Z" / "Confidence"
- "Show only changed columns" checkbox: hides columns where recommendation matches current settings

**`ConfidenceBadge.js`:** Returns a `<span>` element styled per tier (HIGH=green, MEDIUM=amber, LOW=gray).

**`ImpactStars.js`:** Returns a `<span>` with filled (★) and empty (☆) stars.

**Acceptance criteria:**
- Mock report renders 6 column cards sorted by impact descending
- Clicking expand chevron expands the card (height animates); clicking again collapses
- Name filter "customer" shows only `customer_id` card
- HIGH confidence badge is green, MEDIUM is amber
- All 5 impact stars filled for the highest-impact column
- Cards/Table toggle is visible; clicking "Table" switches to Tabulator view; clicking "Cards" switches back
- Filter and sort controls work in both views

---

### W08 — Codec Option Cards and Code Snippet Panel

**Assign to:** Frontend codegen UI agent

**Goal:** Implement the codec option cards and the multi-engine code snippet panel.

**Files to create:**
- `web/src/render/codec-cards.js`
- `web/src/components/SnippetPanel.js`
- `web/src/components/CodeBlock.js`

**Codec option cards (`render/codec-cards.js`):**
- Three cards: Balanced (ZSTD:3), Smallest (ZSTD:6), Fastest (LZ4)
- Each card: icon, label, codec name, qualitative tradeoff description (from `report.options.a/b/c.tradeoff`)
- "Get snippet" button scrolls to snippet panel and activates the corresponding bundle
- Recommended card (option `a`) has a blue border and `[RECOMMENDED]` label

**Snippet panel (`components/SnippetPanel.js`):**
- Engine tab row: PyArrow | PySpark | Polars (no DuckDB tab — DuckDB users use PyArrow to write)
- Bundle sub-selector: Balanced | Smallest | Fastest
- Code block area
- Copy button (top-right of code block)

**Code block (`components/CodeBlock.js`):**
- Lazy-loads Shiki on first call: `const { createHighlighter } = await import('shiki')`
- Creates highlighter with `{ themes: ['github-dark'], langs: ['python'] }`
- Renders highlighted HTML into the container
- Copy button calls `navigator.clipboard.writeText(rawCode)` with fallback for `http://` contexts:
  ```javascript
  // fallback for non-HTTPS (python3 -m http.server)
  const ta = document.createElement('textarea');
  ta.value = code;
  document.body.appendChild(ta);
  ta.select();
  document.execCommand('copy');
  document.body.removeChild(ta);
  ```
- Copy button label: "Copy" → "Copied!" for 2000ms → "Copy"

**Snippet source (in W08, use pre-built strings from mock report):**
The mock report should include `options.a.python_snippet`, `options.b.python_snippet`, `options.c.python_snippet`. For W08, these are static strings per engine. Real generation via WASM `generate_snippet` is wired in W09.

**Version caveat display:**
Below the code block, render `report.options[active_bundle].caveats` as plain text. Example: "Column-level encoding requires PyArrow ≥ 14.0".

**Acceptance criteria:**
- All three engine tabs render syntax-highlighted Python code
- Copy button works (test on both `https://` and `http://localhost`)
- Switching engine tab updates the code and preserves selected bundle
- Switching bundle updates the codec reference in the code
- "Get snippet" on a codec card scrolls to and focuses the snippet panel

---

### W09 — Advisories, Caveats, and Glossary Tooltips

**Assign to:** Frontend polish agent

**Goal:** Implement the advisory panels, caveats list, and glossary tooltip system.

**Files to create:**
- `web/src/render/advisories.js`
- `web/src/render/caveats.js`
- `web/src/components/Tooltip.js`

**Row group advisory (`render/advisories.js`):**
- Only renders if `report.row_group_advisory.is_within_recommendation === false`
- Shows: avg MB/group, recommended range for selected engine, advice text from `report.row_group_advisory.advice`
- Styled as an amber info panel

**Sort order advisory:**
- Only renders if `report.sort_advisory.inferred_sort_candidates` is non-empty
- Shows: column names, advice text from `report.sort_advisory.advice`
- Styled as a blue info panel

**Caveats list (`render/caveats.js`):**
- Aggregates file-level caveats (`report.file_caveats`) and column caveats
- ⚠ icon for `severity === "Warning"`, ℹ icon for Info
- Column name shown in cyan if caveat is column-level

**Glossary tooltip system (`components/Tooltip.js`):**

Apply the `data-glossary` attribute to all technical terms in the rendered report:

Terms to cover:
- `DELTA_BINARY_PACKED` — "Stores differences between consecutive values instead of full values. Highly effective for monotonically increasing integers and timestamps."
- `RLE_DICTIONARY` — "Stores each distinct value once in a dictionary, then replaces every occurrence with a small integer index. Ideal for low-cardinality columns."
- `BYTE_STREAM_SPLIT` — "Rearranges the bytes of floating-point values so similar byte positions are grouped, giving the codec more repetition to compress."
- `PLAIN` — "Stores values as-is. The safe baseline with no encoding overhead."
- `cardinality_ratio` — "The proportion of distinct values to total rows. 0.001 means 0.1% unique values — very low cardinality."
- `monotonicity_score` — "Fraction of consecutive pairs where the value is non-decreasing. 1.0 = perfectly ascending. ≥ 0.90 triggers DELTA_BINARY_PACKED."
- `byte_entropy` — "Shannon entropy of the byte distribution (0–8 bits/byte). > 7.5 indicates pre-compressed or random data that won't compress further."

Tooltip positioning:
- Appear on hover/focus of the term
- Position via `getBoundingClientRect()` + `position: fixed` to avoid overflow issues
- Stay open when cursor moves into the tooltip (use `mouseleave` with a 100ms delay and cancel on mouseenter)
- Close on Escape key
- Max width 280px; include a link to the Parquet spec if available

**Acceptance criteria:**
- Mock report with `is_within_recommendation: false` shows the row group advisory
- Mock report with `inferred_sort_candidates` shows the sort advisory
- Mock report with `file_caveats` and column caveats shows all in the caveats list
- Hover over `RLE_DICTIONARY` in any column card — tooltip appears within 200ms
- Move cursor from term into tooltip — tooltip stays open
- Press Escape — tooltip closes

---

### V-W3 — Milestone W3 Validation

**Assign to:** Validation agent

**Run after:** W05, W06, W07, W08, W09 all complete

**Checks:**

1. `npm run dev:nowasm` starts without error
2. Drop zone accepts `.parquet` file; mock report renders
3. All 6 mock columns render as cards
4. Expand all 6 cards — each shows the "Why this encoding?" and "Raw statistics" sections
5. Name filter works; sort reorders cards
6. PyArrow tab shows syntax-highlighted Python; copy button copies to clipboard
7. Shiki loads lazily (only after first report render, not on page load)
8. Advisory panel renders when mock report includes `is_within_recommendation: false`
9. Glossary tooltip appears on hover of `RLE_DICTIONARY`
10. Engine selector renders all 5 options; Spark shows version sub-selector

---

## Milestone W4 — WASM Integration

W10 depends on W04 (WASM entry point) and V-W3 (frontend complete).

---

### W10 — Wire WASM to Frontend

**Assign to:** WASM integration agent

**Goal:** Replace the mock `analyzeFile` with the real WASM-backed analysis running in a Web Worker.

**Files to create:**
- `web/src/workers/autoparq-worker.js`
- `web/src/wasm-bridge.js`

**Files to modify:**
- `web/src/App.js` (replace mock `analyzeFile` import with `wasm-bridge.js`)

**`web/src/workers/autoparq-worker.js`:**
```javascript
import init, { tune_file_bytes_with_progress } from '@wasm/autoparq.js';

let initialized = false;

self.onmessage = async ({ data: { type, payload } }) => {
  if (type === 'analyze') {
    if (!initialized) {
      await init();
      initialized = true;
    }
    const { buffer, engine, priority, sampleRows, requestId } = payload;
    const bytes = new Uint8Array(buffer);
    try {
      const json = tune_file_bytes_with_progress(
        bytes, engine, priority, sampleRows,
        (current, total, colName) => {
          self.postMessage({ type: 'progress', requestId, current, total, colName });
        }
      );
      self.postMessage({ type: 'result', requestId, json });
    } catch (e) {
      self.postMessage({ type: 'error', requestId, message: e.message });
    }
  }
};
```

**`web/src/wasm-bridge.js`:**

```javascript
import { check_file_size, generate_snippet } from '@wasm/autoparq.js';
import init from '@wasm/autoparq.js';

let _initPromise = null;
let _worker = null;
let _pendingRequests = new Map();

export function initWasm() {
  if (!_initPromise) _initPromise = init();
  return _initPromise;
}

export function getWorker() {
  if (!_worker) {
    _worker = new Worker(
      new URL('./workers/autoparq-worker.js', import.meta.url),
      { type: 'module' }
    );
    _worker.onmessage = ({ data }) => {
      const resolve = _pendingRequests.get(data.requestId + ':resolve');
      const reject  = _pendingRequests.get(data.requestId + ':reject');
      const onProgress = _pendingRequests.get(data.requestId + ':progress');
      if (data.type === 'progress' && onProgress) {
        onProgress(data);
      } else if (data.type === 'result') {
        _pendingRequests.delete(data.requestId + ':resolve');
        _pendingRequests.delete(data.requestId + ':reject');
        _pendingRequests.delete(data.requestId + ':progress');
        resolve(JSON.parse(data.json));
      } else if (data.type === 'error') {
        _pendingRequests.delete(data.requestId + ':resolve');
        _pendingRequests.delete(data.requestId + ':reject');
        _pendingRequests.delete(data.requestId + ':progress');
        reject(new Error(data.message));
      }
    };
  }
  return _worker;
}

export async function analyzeFile(file, options, onProgress) {
  onProgress({ message: 'Loading analyzer…', pct: 5 });
  await initWasm();  // warm up main-thread WASM for generate_snippet calls

  onProgress({ message: `Reading ${file.name}…`, pct: 15 });
  const buffer = await file.arrayBuffer();

  onProgress({ message: 'Profiling columns…', pct: 30 });

  const requestId = crypto.randomUUID();
  const worker = getWorker();

  return new Promise((resolve, reject) => {
    _pendingRequests.set(requestId + ':resolve', resolve);
    _pendingRequests.set(requestId + ':reject', reject);
    _pendingRequests.set(requestId + ':progress', ({ current, total, colName }) => {
      onProgress({
        message: `Profiling column: ${colName} (${current}/${total})`,
        pct: 30 + Math.round((current / total) * 55),
      });
    });

    worker.postMessage(
      { type: 'analyze', payload: {
          buffer,
          engine:    options.engine    ?? 'unknown',
          priority:  options.priority  ?? 'balanced',
          sampleRows: options.sampleRows ?? 2_000_000,
          requestId,
        }
      },
      [buffer]  // transfer — zero-copy
    );
  });
}

export async function getSnippets(reportJson) {
  await initWasm();
  return {
    pyarrow: generate_snippet(reportJson, 'pyarrow'),
    pyspark: generate_snippet(reportJson, 'pyspark'),
    polars:  generate_snippet(reportJson, 'polars'),
  };
}

export async function checkFileSize(byteLen) {
  await initWasm();
  return JSON.parse(check_file_size(byteLen));
}
```

**Update `App.js`:**
- Replace mock `analyzeFile` with the real one from `wasm-bridge.js`
- After analysis complete, call `getSnippets(JSON.stringify(report))` to populate code panel
- Wire `checkFileSize` to the file size warning flow

**Warm-up:** Pre-initialize WASM in the Worker on page load before the user drops a file:
```javascript
// In main.js, after page load
getWorker();  // spawns worker and starts WASM init
```

**Acceptance criteria:**
- Drop a real `.parquet` fixture file — real recommendations render (not mock data)
- Progress bar updates per column with real column names
- `generate_snippet` produces real PyArrow/PySpark/Polars code
- Drop a non-Parquet file — clear error message appears

---

### W11 — Engine Selector Re-Render via `recommend_from_profile`

**Assign to:** Frontend state agent

**Goal:** Make engine/priority selector changes update recommendations instantly (< 50ms) by calling `recommend_from_profile` with the cached `FileProfile` JSON — no re-profiling, no re-reading the file.

**Approach:**

After the first `tune_file_bytes_with_progress` call returns, the Worker extracts and caches the `FileProfile` JSON from the returned `TuneReport`. Engine/priority changes call `recommend_from_profile` directly in the Worker (synchronous, fast).

```javascript
// In autoparq-worker.js
import init, {
  tune_file_bytes_with_progress,
  recommend_from_profile,
} from '@wasm/autoparq.js';

let cachedProfileJson = null;  // FileProfile JSON, small (<100KB)

self.onmessage = async ({ data: { type, payload } }) => {
  if (type === 'analyze') {
    // ... existing analyze logic ...
    // After tune_file_bytes_with_progress returns json:
    const report = JSON.parse(json);
    cachedProfileJson = JSON.stringify(report.file_profile);  // cache the profile
    self.postMessage({ type: 'result', requestId, json });

  } else if (type === 'recommend') {
    // Engine/priority changed — re-recommend from cached profile, instant
    if (!cachedProfileJson) {
      self.postMessage({ type: 'error', requestId: payload.requestId,
                         message: 'No profile cached. Please re-upload the file.' });
      return;
    }
    try {
      const json = recommend_from_profile(
        cachedProfileJson,
        payload.engine,
        payload.priority,
      );
      self.postMessage({ type: 'result', requestId: payload.requestId, json });
    } catch (e) {
      self.postMessage({ type: 'error', requestId: payload.requestId, message: e.message });
    }
  }
};
```

In `wasm-bridge.js`, add:
```javascript
export function reRecommend(options) {
  const requestId = crypto.randomUUID();
  const worker = getWorker();
  return new Promise((resolve, reject) => {
    _pendingRequests.set(requestId + ':resolve', resolve);
    _pendingRequests.set(requestId + ':reject', reject);
    worker.postMessage({ type: 'recommend', payload: {
      engine:   options.engine   ?? 'unknown',
      priority: options.priority ?? 'balanced',
      requestId,
    }});
  });
}
```

In `App.js`, attach `change` listeners to engine and priority selectors. When they change and a report is shown:
```javascript
const newReport = await reRecommend({ engine, priority });
renderReport(container, newReport, currentFileName);
```

Note: `TuneReport` must embed `file_profile: FileProfile` for this to work (required by W04 task).

**Acceptance criteria:**
- With a report shown, change engine from DuckDB to Spark — recommendations update in < 500ms (visually instant)
- Spark-specific warnings appear on DELTA_BINARY_PACKED columns after switching to Spark 3.1
- Priority change from Balanced to Speed changes ZSTD:3 recommendations to LZ4
- Switching engine does not require re-dropping the file
- Network tab shows no new fetch after engine change (no file re-read)

---

### V-W4 — Milestone W4 Validation

**Assign to:** Validation agent

**Run after:** W10, W11 complete

**Checks:**

1. `npm run dev` — builds WASM and starts Vite; no console errors
2. Drop `tests/fixtures/multi_column.parquet` — 6 real columns render with correct recommendations
3. Drop `tests/fixtures/low_cardinality_strings.parquet` — `status` column shows RLE_DICTIONARY with HIGH confidence
4. Drop `tests/fixtures/high_entropy.parquet` — `blob` column shows UNCOMPRESSED recommendation
5. Progress bar advances per column with real column names
6. Change engine to Spark 3.1 — DELTA_BINARY_PACKED columns show version caveat
7. Copy button copies real PyArrow snippet
8. Drop a 0-byte file — clear error message, no crash
9. Drop a JPEG file renamed to `.parquet` — clear error message about invalid Parquet format

---

## Milestone W5 — Polish and Deployment

---

### W12 — Sample Files

**Assign to:** Fixtures and content agent

**Goal:** Download, trim, and bundle two real public-domain Parquet files that demonstrate different recommendation profiles. Files must be < 15 MB each and clearly show different encoding recommendations.

**Sources:**

**NYC Taxi trips** (`web/public/samples/nyc_taxi.parquet`):
- Source: NYC TLC Trip Record Data (public domain, https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page)
- Use the yellow taxi data for any recent month; available as Parquet
- Trim to ~100,000 rows using PyArrow: `pq.write_table(pq.read_table(src).slice(0, 100_000), dst)`
- Expected recommendations: `tpep_pickup_datetime`/`tpep_dropoff_datetime` → DELTA_BINARY_PACKED (monotonic timestamps); `VendorID`/`payment_type`/`RatecodeID` → RLE_DICTIONARY (low cardinality); `trip_distance`/`fare_amount`/`tip_amount` → BYTE_STREAM_SPLIT (floats)
- Target size: ~8–12 MB

**NOAA Weather** (`web/public/samples/noaa_weather.parquet`):
- Source: NOAA Global Surface Summary of Day (GSOD), available via AWS Open Data (s3://noaa-gsod-pds/)
- Use a single year (e.g., 2023) of data; combine several stations
- Trim to ~150,000 rows
- Expected recommendations: `DATE` → DELTA_BINARY_PACKED; `STATION`/`NAME`/`COUNTRY` → RLE_DICTIONARY (low cardinality strings); `TEMP`/`DEWP`/`WDSP`/`PRCP` → BYTE_STREAM_SPLIT (float measurements); quality flag columns → RLE_DICTIONARY
- Target size: ~10–15 MB

**Prep script** (`web/scripts/download-samples.py`):
Create a Python script that downloads and trims both files. The script should be runnable standalone and is checked into the repo so the trim is reproducible:

```python
#!/usr/bin/env python3
"""Download and prepare sample Parquet files for the autoparq web demo.
Usage: python web/scripts/download-samples.py
Outputs: web/public/samples/nyc_taxi.parquet and noaa_weather.parquet
"""
import pyarrow.parquet as pq
import pyarrow as pa
import urllib.request, os, pathlib

OUTPUT_DIR = pathlib.Path(__file__).parent.parent / "public" / "samples"
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

# NYC Taxi — yellow taxi, most recent available month
NYC_TAXI_URL = "https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet"

def download_nyc_taxi():
    tmp = "/tmp/nyc_taxi_full.parquet"
    print("Downloading NYC Taxi data...")
    urllib.request.urlretrieve(NYC_TAXI_URL, tmp)
    table = pq.read_table(tmp).slice(0, 100_000)
    out = OUTPUT_DIR / "nyc_taxi.parquet"
    pq.write_table(table, out, compression="snappy")
    print(f"Written: {out} ({out.stat().st_size // 1024} KB)")

# NOAA GSOD — via AWS Open Data
def download_noaa_weather():
    import subprocess
    tmp_dir = "/tmp/noaa_gsod"
    os.makedirs(tmp_dir, exist_ok=True)
    print("Downloading NOAA GSOD 2023 sample...")
    subprocess.run([
        "aws", "s3", "cp",
        "s3://noaa-gsod-pds/2023/", tmp_dir,
        "--recursive", "--no-sign-request",
        "--exclude", "*", "--include", "72*.csv",
        "--quiet"
    ], check=True)
    # Read, concat, write as Parquet
    import glob, csv
    import pyarrow as pa
    rows = []
    for f in sorted(glob.glob(f"{tmp_dir}/*.csv"))[:300]:
        with open(f) as fh:
            rows.extend(list(csv.DictReader(fh)))
        if len(rows) >= 150_000:
            break
    rows = rows[:150_000]
    # Build Arrow table from dicts
    table = pa.Table.from_pylist(rows)
    out = OUTPUT_DIR / "noaa_weather.parquet"
    pq.write_table(table, out, compression="snappy")
    print(f"Written: {out} ({out.stat().st_size // 1024} KB)")

if __name__ == "__main__":
    download_nyc_taxi()
    download_noaa_weather()
```

Note: NOAA path requires AWS CLI with `--no-sign-request`. If AWS CLI is unavailable, the NOAA data can also be downloaded from NOAA's FTP at `ftp.ncdc.noaa.gov/pub/data/gsod/`.

**`web/src/samples.js`:**
```javascript
export const SAMPLE_FILES = [
  {
    label: 'NYC Taxi trips (~10 MB)',
    path: '/samples/nyc_taxi.parquet',
    description: 'Demonstrates DELTA_BINARY_PACKED (timestamps), RLE_DICTIONARY (vendor/payment codes), BYTE_STREAM_SPLIT (fares, distances)',
  },
  {
    label: 'NOAA Weather (~12 MB)',
    path: '/samples/noaa_weather.parquet',
    description: 'Demonstrates RLE_DICTIONARY (station codes, countries), BYTE_STREAM_SPLIT (temperature, precipitation)',
  },
];

export async function loadSample(path, onProgress) {
  onProgress({ message: `Fetching ${path.split('/').pop()}…`, pct: 5 });
  const response = await fetch(path);
  if (!response.ok) throw new Error(`Failed to fetch sample: ${response.status}`);
  const blob = await response.blob();
  return new File([blob], path.split('/').pop(), { type: 'application/octet-stream' });
}
```

Wire sample file buttons in `App.js` to call `loadSample` then `handleFile`. Add a small description tooltip under each sample button so users know what recommendations to expect.

**The sample files are committed to the repo** (< 15 MB each, acceptable for git). Add a note in the root README that `web/public/samples/` was generated by `web/scripts/download-samples.py` and the data sources and licenses.

**Acceptance criteria:**
- Both sample files present in `web/public/samples/`; each < 15 MB
- Clicking "NYC Taxi trips" button triggers analysis
- `tpep_pickup_datetime` column in NYC Taxi shows DELTA_BINARY_PACKED recommendation
- `VendorID` or `payment_type` shows RLE_DICTIONARY
- Clicking "NOAA Weather" triggers analysis
- Temperature columns show BYTE_STREAM_SPLIT
- `download-samples.py` runs successfully in a clean environment with PyArrow installed

---

### W13 — Cross-Browser Testing and Accessibility

**Assign to:** QA and accessibility agent

**Goal:** Verify the full flow in Chrome, Firefox, and Safari. Fix any regressions. Ensure keyboard accessibility for core interactions.

**Test matrix:**
- Chrome (latest), Firefox (latest), Safari (latest on macOS)
- Each browser: drop a sample file, verify full report renders, copy snippet, switch engine

**Accessibility checks:**
- Drop zone is keyboard-accessible: Tab to reach it, Enter/Space to open file browser
- Engine selector is keyboard-navigable
- Expand/collapse chevrons have `aria-expanded` attribute
- Color is not the only differentiator for confidence badges (add text label)
- Glossary tooltips are accessible via keyboard focus (not only mouse hover)
- All interactive elements have visible focus rings

**Fixes to make:**
- Any Safari-specific WASM loading issues (test `--target web` Worker init in Safari; fix if broken)
- Any Firefox layout issues with the sticky header + scroll nav
- Code block horizontal scroll on narrow viewports

**Acceptance criteria:**
- Full flow works in all three browsers
- Keyboard user can complete the full flow (drop file → view report → copy snippet) without a mouse
- All `<img>` elements have `alt` attributes (there may be none, but verify)
- No console errors in any browser during normal use

---

### W14 — Production Build and CloudFront Deployment

**Assign to:** DevOps agent

**Goal:** `npm run build` produces a deployable `web/dist/` directory. Document the S3/CloudFront deployment process.

**Files to create/modify:**
- `web/vite.config.js` — verify `base` path is correct for S3 root hosting
- `web/public/_headers` (for Netlify/Cloudflare compatibility, optional) or CloudFront response headers policy
- `docs/deploy-web.md` — deployment instructions

**Build verification checklist:**
- `npm run build` completes without error
- `web/dist/index.html` exists
- `web/dist/assets/` contains hashed JS, CSS, and WASM files
- WASM file `autoparq_bg-[hash].wasm` is present (not inlined as base64)
- Total `web/dist/` size: document actual size in `docs/deploy-web.md`

**CloudFront requirements:**
- S3 bucket: `Block all public access` OFF (or use OAI/OAC with CloudFront)
- CloudFront distribution: default root object = `index.html`
- Custom error response: 404 → `/index.html` with 200 status (for direct URL access)
- `.wasm` file must be served with `Content-Type: application/wasm` — verify S3 serves this correctly (S3 uses `application/wasm` for `.wasm` by default since 2020; verify)
- Cache policy: long TTL for hashed assets (`/assets/*`), short TTL for `index.html`

**`docs/deploy-web.md` must include:**
1. Prerequisites (Node, wasm-pack, AWS CLI)
2. Build command
3. S3 sync command
4. CloudFront invalidation command
5. Local test command (`python3 -m http.server 8080` in `web/dist/`)
6. Note explaining why `file://` doesn't work

**Acceptance criteria:**
- `npm run build && cd dist && python3 -m http.server 8080` — site loads at localhost:8080, full analysis works
- WASM file is not base64-encoded in the JS bundle
- `deploy-web.md` exists and is accurate

---

### V-W5 — Final Validation

**Assign to:** Validation agent

**Run after:** W12, W13, W14 complete

**Checks:**

1. Production build serves correctly from `python3 -m http.server 8080`
2. All three sample files analyze successfully from the production build
3. Full analysis flow works in Chrome, Firefox, Safari
4. WASM file served with `Content-Type: application/wasm` (check Network tab)
5. No console errors during full analysis in any browser
6. Total page weight on first load: document actual gzip size (target: < 2 MB)
7. Copy button works on `http://localhost` (non-HTTPS fallback)
8. Engine switch re-analyzes without re-upload
9. `docs/deploy-web.md` exists and the S3 sync command is syntactically valid
10. Keyboard-only flow: Tab → Enter on drop zone → file picker opens

---

## Task Dependency Summary

```
W01 ──┐
W02 ──┼── W04 (WASM entry point)
W03 ──┘         │
                │
W05 ──┐         │
W06 ──┤         │
W07 ──┼── V-W3 ─┼── W10 ── W11 ── V-W4 ──┐
W08 ──┤         │                         │
W09 ──┘         │                         │
                │                         ├── W12 ──┐
V-W1 ── V-W2 ───┘                         │         ├── W13 ── W14 ── V-W5
                                          └── ──────┘
```

Tracks A (W01–W04) and C (W05–W09) run fully in parallel. W10 is the integration point where they converge.
