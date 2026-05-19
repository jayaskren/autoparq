# autoparq Web App — Product Requirements Document

**Status:** Draft  
**Version:** 0.1  
**Date:** 2026-04-19  

---

## 1. Overview

A browser-based Parquet file analyzer that lets data engineers drop in a Parquet file and receive a full compression recommendation report — with explanations that teach them *why* each setting matters. All analysis runs locally in the browser via WebAssembly. No file is ever uploaded to a server.

Hosted on AWS CloudFront (S3-backed static site). Can also be run locally with a single command.

### Goals

- **Advisor**: Recommend the best encoding and codec for each column, with specific statistics backing every recommendation
- **Learning tool**: Teach data engineers the underlying principles so they understand compression, not just copy settings
- **Privacy-first**: Enterprise-safe — no data leaves the browser
- **Self-hostable**: Check out the repo, run one command, open localhost

### Non-goals

- No schema restructuring or column type changes
- No server-side analysis or file storage
- No user accounts or saved history (v1)
- No CI/CD integration via the web UI (use the CLI for that)

---

## 2. Target Audience

**Primary:** Data engineers who work with Parquet files (Spark, DuckDB, Polars, ClickHouse) but are not compression experts. They know Python and SQL. They do not know what ZSTD:3 means, why column encoding matters, or when to use DELTA_BINARY_PACKED vs RLE_DICTIONARY.

**Secondary:** Platform/infrastructure engineers evaluating storage costs for a data lake.

**Not targeted:** ML engineers, data analysts, or non-technical users.

---

## 3. User Journey

```
LANDING → DROP FILE → ANALYZING → REPORT → COPY & APPLY
```

| Stage | User emotional state | Design goal |
|-------|---------------------|-------------|
| Landing | Skeptical ("is this real?") | Communicate value in 5 seconds, prove it's safe |
| Drop file | Mild anxiety (enterprise file, privacy?) | Reassure before the drop, not after |
| Analyzing | Anticipation, impatience | Show real progress — prove real work is happening |
| Report (first view) | Relief → information hunger | Headline number above the fold, progressive depth |
| Report (deep dive) | Flow state, learning | Teach without overwhelming |
| Copy & Apply | Satisfaction, mild skepticism | Actionable code, honest uncertainty labels |

---

## 4. Pages and Layout

### 4.1 Landing Page

Single-page app. The landing state and report state are the same URL — the page transitions in-place.

**Hero section** (full viewport):

```
autoparq                                          [GitHub ↗]

    Drop a Parquet file.
    Find out how much smaller it could be.

    No upload. No account. Runs in your browser.

┌─────────────────────────────────────────────────┐
│                                                 │
│       Drop your .parquet file here              │
│                                                 │
│              or  [Browse file]                  │
│                                                 │
└─────────────────────────────────────────────────┘
🔒 Your file is never sent to a server. Analysis runs entirely
   in your browser using WebAssembly.

── or try a sample file ──────────────────────────
[NYC Taxi trips (12 MB)]  [NOAA Weather (10 MB)]
```

**Copy rules:**
- Headline: outcome-first, no jargon
- Privacy notice: inline and permanent, above the fold — not a footnote
- No exclamation marks; no "magical AI" language
- Sample files let skeptical users try the tool without risking a work file

**Below hero — three trust signals** (not a features list):

| | | |
|--|--|--|
| Per-column analysis | Copy-paste code ready | Explains the why |
| Encoding and codec recommendation per column, not per file | PyArrow, PySpark, DuckDB, Polars snippets | Not just "use ZSTD" — shows the specific stat that triggered each rule |

### 4.2 File Drop Zone — States

| State | Visual behavior |
|-------|----------------|
| Idle | Dashed gray border, large target area |
| Drag-over | Solid blue border, light blue background tint, "Release to analyze" |
| File accepted | Collapses to slim file bar showing name + size, transitions to analysis view |
| Wrong file type | Red border flash (400ms), inline message: "autoparq only reads .parquet files" |
| File > 200 MB | Yellow warning before analysis: "Large file (NNN MB). Analysis may take 10–30 seconds." |
| File > 1 GB | Hard block: "File exceeds 1 GB. Use the native CLI for large files." |

No alert dialogs. All feedback is inline.

### 4.3 Analysis Progress View

Replaces the hero section entirely (not a modal). Signals serious work in progress.

```
Analyzing: orders_2024_q1.parquet  (234 MB)                      [×]

████████████████████████████░░░░░░░░░░░░░  67%

Profiling column: customer_id  (13 / 18 columns)
```

Progress is driven by actual work completion (per-column callbacks from WASM). Displays the current column name and column index so the user knows it is real.

### 4.4 Report Layout

Three-zone layout with a sticky summary header:

```
┌──────────────────────────────────────────────────────────────────┐
│  STICKY HEADER                                                   │
│  orders_2024_q1.parquet  18 columns  234 MB    Engine: [DuckDB ▾]│
│  Estimated savings:  ████████████████░░░  ~47% smaller [est.]    │
└──────────────────────────────────────────────────────────────────┘

┌────────────────────┐  ┌─────────────────────────────────────────┐
│  LEFT RAIL (nav)   │  │  MAIN CONTENT                           │
│                    │  │                                         │
│  Summary           │  │  ← active section renders here         │
│  Columns           │  │                                         │
│  Codec Options     │  │                                         │
│  Code Snippet      │  │                                         │
│  Advisories        │  │                                         │
└────────────────────┘  └─────────────────────────────────────────┘
```

Left rail: anchor-link nav, active section highlighted. Collapses to a horizontal tab row on mobile.

---

## 5. Report Sections

### 5.1 Summary Panel

First section, above the fold. Scannable, screenshot-able.

```
┌────────────────────────────┬─────────────────────────────────┐
│  Current size:    234 MB   │  Recommended settings           │
│  Estimated after: 124 MB   │  ─────────────────────────────  │
│  Reduction:       ~47%     │  Codec:   ZSTD level 3          │
│  [estimated]               │  3 cols:  DELTA_BINARY_PACKED   │
│                            │  8 cols:  RLE_DICTIONARY        │
│  Read speedup:    ~2.1×    │  7 cols:  PLAIN (no change)     │
│  [estimated]               │                                 │
└────────────────────────────┴─────────────────────────────────┘
```

All predicted numbers labeled `[estimated]`. No rounding up.

### 5.2 Column Recommendations

Two views of the same data, toggled above the filter bar:

```
[Cards  ◉]  [Table  ○]        [All columns ▾]  [High impact first ▾]  🔍 Filter by name…
```

**Cards view** (default): rich, expandable, educational. One card per column.  
**Table view**: dense Tabulator grid for files with many columns, or when the user wants a quick scan of all raw stats at once. This is also the "Full Stats" view.

Toggle state persists for the session. The filter bar and sort controls apply to both views.

Cards sorted by impact (descending) by default. Each card:

```
┌─ customer_id ────────────────────────────── ★★★★★  HIGH ─┐
│  INT64  •  0.02% null  •  cardinality 0.0001              │
│                                                           │
│  Encoding:  RLE_DICTIONARY        Codec:  ZSTD:3          │
│                                                           │
│  "cardinality_ratio=0.0001 — dictionary encodes 10,000    │
│   unique values into a 4-byte lookup"                     │
│                                                         ▼ │
└───────────────────────────────────────────────────────────┘
```

**Card fields:**
- Column name + impact stars (1–5) + confidence badge (HIGH/MED/LOW, color-coded)
- Physical type, null %, most relevant statistic (monospace)
- Recommended encoding + codec
- One-sentence reason string — always visible, never hidden (the primary teaching moment)
- Expand chevron `▼`

**Filter bar above cards:**
```
[All columns ▾]  [High impact first ▾]  🔍 Filter by name…
```

**Confidence badge colors:**
- HIGH → green pill
- MEDIUM → amber pill  
- LOW → gray pill with `?` tooltip: "Small sample or near a threshold — treat as a hint, not a fact"

#### Expanded column card

```
┌─ order_ts ──────────────────────────────── ★★★★☆  HIGH ─┐
│  INT64 (TIMESTAMP_MICROS)  •  0% null  •  mono. 0.94     │
│  Encoding:  DELTA_BINARY_PACKED    Codec:  ZSTD:3         │
│  "monotonicity_score=0.94 ≥ 0.90 → delta encoding"       │
├───────────────────────────────────────────────────────────┤
│  ▾ Why this encoding?                                     │
│                                                           │
│    DELTA_BINARY_PACKED stores the difference between      │
│    consecutive values. For a timestamp incrementing by    │
│    ~1000µs/row, deltas fit in 2 bytes instead of 8.       │
│                                                           │
│    Rules evaluated:                                       │
│    ├── BOOLEAN?            No (INT64)                     │
│    ├── Monotonic INT/TS?   ✓ score=0.94 ≥ 0.90  ← fired  │
│    └── (first match wins — remaining rules skipped)       │
│                                                           │
│  ▾ Raw statistics                                         │
│                                                           │
│    null_count:          0        sample_rows:   1,200,000 │
│    cardinality_ratio:  0.42      confidence:    HIGH      │
│    monotonicity_score: 0.94      sample_frac:   0.12      │
│                                                           │
│  ▾ Alternatives considered                                │
│                                                           │
│    RLE_DICTIONARY  ✗  cardinality_ratio=0.42 > 0.10       │
│    BYTE_STREAM_SPLIT ✗ not FLOAT/DOUBLE                   │
│                                                           │
│  ▾ Engine compatibility                                   │
│                                                           │
│    DELTA_BINARY_PACKED: Spark 3.2+, DuckDB ✓, CH ✓       │
│                                                 [Copy ⧉] │
└───────────────────────────────────────────────────────────┘
```

Each subsection within the expanded card is independently collapsible. Cards expand in-place (push content below, no modal, no navigation away).

### 5.3 Codec Option Cards

Three cards shown side-by-side:

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  ⚖ Balanced      │  │  📦 Smallest     │  │  ⚡ Fastest       │
│  ZSTD:3          │  │  File  ZSTD:6    │  │  Reads  LZ4      │
│  [RECOMMENDED]   │  │                  │  │                  │
│                  │  │  Best compression│  │  Fastest reads.  │
│  Best balance of │  │  ratio. 20-30%   │  │  Larger files    │
│  size and speed. │  │  slower writes   │  │  than ZSTD.      │
│                  │  │  than ZSTD:3.    │  │                  │
│  [Get snippet]   │  │  [Get snippet]   │  │  [Get snippet]   │
└──────────────────┘  └──────────────────┘  └──────────────────┘
```

Qualitative tradeoff descriptions only — no numeric size/speed estimates for codec-only differences (codec savings are highly data-dependent and cannot be reliably estimated without actually compressing). Users should run `autoparq bench` for real measurements.

"Get snippet" scrolls to the code section and pre-selects that bundle.

### 5.4 Code Snippet Panel

Full content width. Engine tabs + bundle selector. Code block max-height 400px with scroll.

```
Engine:  [PyArrow] [PySpark] [Polars]
Bundle:  [Balanced ✓] [Smallest] [Fastest]                    [Copy ⧉]
┌──────────────────────────────────────────────────────────────────────┐
│  import pyarrow as pa                                               │
│  import pyarrow.parquet as pq                                       │
│                                                                     │
│  # autoparq recommendation — estimated 47% size reduction           │
│  # Generated 2026-04-19 for: orders_2024_q1.parquet                 │
│                                                                     │
│  column_encodings = {                                               │
│      "customer_id":  "RLE_DICTIONARY",    # cardinality=0.0001      │
│      "order_ts":     "DELTA_BINARY_PACKED",  # monotonicity=0.94   │
│      "amount":       "BYTE_STREAM_SPLIT",    # float, card>0.50    │
│  }                                                                  │
│  ...                                                                │
└──────────────────────────────────────────────────────────────────────┘
💡 Column-level encoding requires PyArrow ≥ 14.0
```

**Behavior rules:**
- Engine tab switch regenerates snippet immediately from cached report (no re-analysis)
- Bundle switch updates only the codec in the snippet
- Comments in generated code include the triggering statistic per column — snippet is also a teaching document
- Copy button: changes to "Copied!" for 2 seconds, then reverts; no toast notification
- Spark snippet includes inline comment: `# Note: per-column hints require Spark 3.4+`
- Engine compatibility caveats appear as plain text below the code block

**Engine variants are generated by the Rust core** (same `codegen` module as the CLI). The web calls `generate_snippet(report_json, engine)` via WASM — no logic duplication between CLI and web.

Note: DuckDB does not have a Python Parquet write API that supports per-column encoding hints. Use the engine selector to tailor recommendations for DuckDB; use the PyArrow snippet to write the file.

### 5.5 Advisories

**Row Group Advisory** (shown only when outside recommended range):

```
┌─ Row Group Advisory ─────────────────────────────────────────────┐
│  Current avg: 8 MB/group. DuckDB works best with 64–128 MB.      │
│  Your file has 29 row groups at ~8 MB each. Consider merging     │
│  to 2–4 row groups of ~58 MB for faster range scans.             │
└──────────────────────────────────────────────────────────────────┘
```

**Sort Order Advisory** (shown when inferred sort candidates detected):

```
┌─ Sort Order Advisory ────────────────────────────────────────────┐
│  Column order_ts appears sorted (monotonicity=0.94) but the      │
│  Parquet footer has no sort metadata declared. Adding            │
│  sorting_columns metadata lets DuckDB skip row groups during     │
│  range queries.                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 5.6 Caveats and Warnings

List at the bottom of the report. Icons differentiate severity:

- `⚠` Warning — may affect correctness (e.g., LZ4 + DELTA_BINARY_PACKED + parquet-go bug)
- `ℹ` Info — version requirements, tradeoff notes

### 5.7 Full Stats (Table View)

The "Table" toggle in the Columns section switches from the card layout to a dense Tabulator grid containing all raw profiling statistics. This is the power-user view — every number the profiler computed, in one scannable place.

**Columns in the table:**

| Column | Source field | Notes |
|--------|-------------|-------|
| Column name | `column_name` | Frozen left column |
| Physical type | `physical_type` | e.g. INT64, BYTE_ARRAY |
| Logical type | `logical_type` | e.g. TIMESTAMP_MICROS, STRING |
| Null % | `null_fraction` | Formatted as 0.00% |
| Cardinality | `cardinality_estimate` | Raw count, formatted with commas |
| Cardinality % | `cardinality_ratio` | Formatted as 0.00% |
| Cardinality method | `cardinality_method` | "exact" or "hyperloglog" |
| Monotonicity | `monotonicity_score` | 0.000–1.000, `—` for non-integer types |
| Run-length score | `run_length_score` | 0.000–1.000 |
| Byte entropy | `byte_entropy` | 0.0–8.0 bits/byte, `—` for non-binary types |
| UUID detected | `uuid_detected` | ✓ / `—` |
| JSON detected | `json_detected` | ✓ / `—` |
| Mean string len | `string_length_stats.mean_len` | `—` for non-string types |
| Rec. encoding | `recommended_encoding` | Color-coded if different from current |
| Rec. codec | `recommended_codec` | Color-coded if different from current |
| Confidence | `confidence` | HIGH/MED/LOW badge |
| Impact | `impact_stars` | ★★★☆☆ |

**Table behavior:**
- Sortable by any column (click header)
- First column (column name) frozen during horizontal scroll
- Rows with a changed encoding or codec recommendation are highlighted with a subtle left border in indigo
- Column visibility toggle: a "Columns ▾" button allows hiding less-used stats columns to reduce horizontal scroll
- Default hidden: cardinality method, run-length score, UUID detected, JSON detected, mean string len (shown on demand)
- Clicking a row expands it inline to show the reasoning chain (same content as the card's "Why this encoding?" section)
- The same filter bar (name search, confidence filter) applies

**When to use:** Files with 30+ columns where the card view requires excessive scrolling, or when comparing raw stats across columns (e.g., "which columns have the highest cardinality?").

---

## 6. Education System

Three-tier progressive disclosure. Education is not optional or off to the side — it is woven into the report itself.

**Tier 1 — Always visible (no interaction required):**  
The `reason_brief` string on every column card. Contains the actual statistic and plain-English explanation. Statistic values in monospace. This is the primary learning touchpoint.

**Tier 2 — Expandable card section:**  
"Why this encoding?" section inside each expanded card. Contains:
- Plain-English explanation of the encoding and why it helps for this data pattern
- Rules-evaluated tree showing every rule checked and which one fired
- "First match wins" reminder

**Tier 3 — Glossary tooltips:**  
Technical terms with dotted underlines throughout the UI: `DELTA_BINARY_PACKED`, `cardinality_ratio`, `monotonicity_score`, `byte_entropy`, `RLE_DICTIONARY`. Hover shows a floating tooltip card (~280px):
1. One-sentence technical definition
2. One sentence on when it matters
3. Link to Parquet spec section

Tooltip stays open if the user moves into it (readable, not disappearing).

---

## 7. Engine Selector

Located in the sticky header. Present at all times once the report is shown.

**Default state:** `unknown`. Report shows maximally safe recommendations (SNAPPY codec, conservative encodings). Banner below header:

```
ℹ No engine selected. Recommendations use conservative settings.
  Select your query engine for tailored advice.
```

**Engine options:**
- Unknown
- Spark (sub-selector for version: 3.1 / 3.2 / 3.3 / 3.4+)
- DuckDB
- Polars / PyArrow
- ClickHouse
- pandas

**Behavior on change:**
- Re-renders from the cached `FileProfile` (raw profiling stats) — no re-sampling, no re-profiling. The recommendation pass runs in < 50ms.
- Implemented via a dedicated `recommend_from_profile(profile_json, engine, priority)` WASM function. The Worker caches the `FileProfile` JSON (< 100 KB) after the first analysis; the file bytes do not need to be retained.
- Engine-specific compatibility warnings activate inline on affected column cards
- Spark + version < 3.2 → warning on any `DELTA_BINARY_PACKED` column: `⚠ Requires Spark 3.2+`
- ClickHouse → BROTLI/GZIP columns show downgrade notice

---

## 8. Visual Design

### Typography
- UI font: system font stack (`-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`) — no web font loading
- Code and statistics: `"JetBrains Mono", "Fira Code", monospace` — all statistics, column names, type names, numeric values
- Base size: 16px. Headers use weight (600/700), not large sizes

### Color Palette

| Purpose | Color | Hex |
|---------|-------|-----|
| Primary action / links | Indigo | `#4F46E5` |
| HIGH confidence | Green | `#16A34A` |
| MEDIUM confidence | Amber | `#D97706` |
| LOW confidence | Gray | `#6B7280` |
| Error / warning | Red | `#DC2626` |
| Background | White | `#FFFFFF` |
| Card surface | Near-white | `#F9FAFB` |
| Border | Light gray | `#E5E7EB` |

No dark mode in v1.

### Copy Voice
- Direct, second-person where needed
- Show the number, name the rule: "~47% smaller [estimated]" not "almost 50% smaller!"
- No exclamation marks in UI (one exception: "Copied!")
- Errors describe what happened and what to do — not just "Analysis failed"

### Tone Reference
Bundlephobia / pkg-size.dev — tools engineers trust because they look built by engineers, not designed to impress. Information density of a well-designed `man` page. Copability of a Stack Overflow answer.

---

## 9. Mobile

Data engineers work on laptops. Mobile is a "read a report someone shared" use case, not the primary flow.

**Adaptations:**
- Left rail nav collapses to horizontal scrolling tab row pinned below sticky header
- Three codec option cards stack vertically
- Code block: "Show full code / Collapse" toggle (horizontal scroll on mobile code is bad UX)
- Touch targets: 44×44px minimum on expand chevrons and tab buttons
- `Alternatives considered` accordion closed by default on mobile

File analysis on mobile is supported (fallback `<input type="file">` button instead of drag-and-drop) but not the focus.

---

## 10. Technical Architecture

### WASM Entry Point

`src/wasm.rs` exposes via `#[wasm_bindgen]`:

```rust
pub fn tune_file_bytes(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
) -> Result<String, JsError>  // Returns TuneReport as JSON string

pub fn tune_file_bytes_with_progress(
    data: &[u8],
    engine: &str,
    priority: &str,
    sample_rows: u32,
    on_progress: &js_sys::Function,  // (current: u32, total: u32, col_name: &str)
) -> Result<String, JsError>

pub fn recommend_from_profile(
    profile_json: &str,  // FileProfile JSON (cached from first analysis)
    engine: &str,
    priority: &str,
) -> Result<String, JsError>  // Returns TuneReport JSON (no re-profiling)

pub fn generate_snippet(
    report_json: &str,
    engine: &str,  // "pyarrow" | "pyspark" | "polars"
) -> Result<String, JsError>

pub fn check_file_size(byte_len: usize) -> String  // Returns JSON {ok, warning, error}
```

### Profiler Refactor

Add `profile_cursor(bytes::Bytes)` path to the profiler. `bytes::Bytes` already implements `ChunkReader` in the parquet crate — no new traits needed. The existing `profile_path(&Path)` call is unchanged; `lib.rs` (PyO3) continues to call it.

```rust
// Two public entry points, one shared generic core
pub fn tune_from_path(path: &Path, config: TuneConfig)   -> Result<TuneReport, _>
pub fn tune_from_cursor(data: &[u8], config: TuneConfig) -> Result<TuneReport, _>
```

### Rayon / Threading

Rayon disabled for the WASM target via `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`. Sequential column loop used in WASM. No `SharedArrayBuffer` required — avoids the COOP/COEP header requirement that breaks third-party scripts on CloudFront.

### Web Worker

WASM analysis runs in a Web Worker to avoid blocking the main thread. The `File.arrayBuffer()` result is transferred (not copied) to the Worker:

```javascript
worker.postMessage({ buffer, engine, priority }, [buffer]);  // transfer
```

Worker posts `{ type: 'progress', current, total, colName }` messages back to the main thread during analysis.

### Cargo.toml Feature Flags

```toml
[features]
default = []
python  = ["dep:pyo3"]
wasm    = ["dep:wasm-bindgen", "dep:js-sys", "dep:console_error_panic_hook"]

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
rayon = "1.10"

[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom = { version = "0.2", features = ["js"] }
```

Build commands:
```bash
# WASM (run from web/ directory)
wasm-pack build .. --target web --out-dir pkg --release --features wasm -- --no-default-features

# Python (unchanged, run from repo root)
maturin develop --features python -- --no-default-features
```

### Bundle Size Budget

| Asset | Gzipped |
|-------|---------|
| WASM bundle (parquet + arrow crates) | ~1.2 MB |
| Tabulator 6 (column table) | ~35 KB |
| Shiki (syntax highlight, lazy) | ~180 KB |
| App JS | ~25 KB |
| Tailwind CSS (purged) | ~8 KB |
| **First load total** | **~1.3 MB** |

Rule: all JS libraries combined must stay under 300 KB gzipped.

---

## 11. Frontend Stack

| Concern | Choice |
|---------|--------|
| Framework | Vanilla JS (no React/Svelte/Vue) |
| Build tool | Vite 5 + `vite-plugin-wasm` + `vite-plugin-top-level-await` |
| Styling | Tailwind CSS v4 |
| Component library | None |
| Syntax highlighting | Shiki (lazy-loaded, Python grammar + github-dark theme) |
| Column table | Tabulator 6 |
| Charts | None (tables are sufficient) |

**Framework rationale:** The report is a one-way data render (WASM JSON → DOM). No live binding, no user-driven mutations beyond tab selection and copy. Vanilla JS with `document.createElement` is sufficient and readable by Rust/Python contributors without a framework tutorial.

---

## 12. Project Structure

```
autoparq/
├── src/                       ← Rust library (existing)
│   ├── wasm.rs                ← NEW: #[wasm_bindgen] entry points
│   ├── tuner.rs               ← ADD: tune_from_cursor() alongside tune_from_path()
│   └── ...
├── python/                    ← Python CLI (existing, unchanged)
├── web/                       ← NEW: frontend
│   ├── package.json
│   ├── vite.config.js
│   ├── index.html
│   ├── pkg/                   ← wasm-pack output (gitignored)
│   └── src/
│       ├── main.js
│       ├── App.js             ← Drop zone + file handling
│       ├── wasm-bridge.js     ← All WASM calls (single import point)
│       ├── style.css
│       ├── ui.js              ← Progress bar, error display
│       ├── render/
│       │   ├── report.js      ← Orchestrates full report
│       │   ├── summary.js
│       │   ├── codec-cards.js
│       │   ├── advisories.js
│       │   └── caveats.js
│       └── components/
│           ├── ColumnTable.js    ← Tabulator wrapper
│           ├── CodeBlock.js      ← Shiki + copy button
│           ├── SnippetPanel.js   ← Engine/bundle tab switcher
│           └── ConfidenceBadge.js
└── specs/
    └── 002_web_app/
        └── PRD.md             ← this document
```

---

## 13. Local Development

**Prerequisites:**
```bash
rustup target add wasm32-unknown-unknown
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
# Node LTS (only needed for the web UI)
```

**Run locally:**
```bash
cd web
npm install
npm run dev        # builds WASM + starts Vite dev server at localhost:5173
```

**Serve a production build without Node:**
```bash
cd web
npm run build      # outputs to web/dist/
cd dist
python3 -m http.server 8080
# open http://localhost:8080
```

Note: `file://` does not work. A local HTTP server is required because browsers block WASM module loading from the `file://` protocol.

**package.json scripts:**
```json
{
  "wasm:build":     "wasm-pack build .. --target web --out-dir pkg --release --features wasm -- --no-default-features",
  "wasm:build:dev": "wasm-pack build .. --target web --out-dir pkg --dev --features wasm -- --no-default-features",
  "dev":            "npm run wasm:build:dev && vite",
  "dev:nowasm":     "vite",
  "build":          "npm run wasm:build && vite build",
  "preview":        "vite preview"
}
```

---

## 14. Deployment

**CloudFront + S3:**
1. `npm run build` → `web/dist/`
2. `aws s3 sync web/dist/ s3://your-bucket/ --delete`
3. CloudFront distribution points to S3 bucket; no custom headers required (no WASM threads = no COOP/COEP needed)

**No backend required.** The S3 bucket serves static files. CloudFront handles CDN and HTTPS.

---

## 15. Implementation Tasks

See [TASKS.md](TASKS.md) for the authoritative task list with acceptance criteria.

---

## 16. Open Questions

1. **Analytics**: Should the site track usage anonymously (page views, file sizes analyzed)? Must be privacy-preserving (no file content or names ever sent). Options: Plausible Analytics (GDPR-compliant, no cookies), or none at all.

2. **Sharing reports**: Should users be able to share a report URL? Requires serializing the report JSON into the URL fragment (~50 KB base64 for a 50-column file). Server-side storage is out of scope for v1.

3. **`autoparq bench` integration**: Should the web show a "Run benchmark" option that actually compresses a column with each codec combination (using WASM) and shows real size numbers? This would make the Codec Options section data-driven rather than qualitative. Natural follow-on after v1 ships.
