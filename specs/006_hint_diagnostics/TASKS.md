# TASKS — Hint Diagnostics (Spec 006)

## Phase 1 — Rust metadata preservation

### T01 — Split `PLAIN_DICTIONARY` from `RLE_DICTIONARY` in `format_encoding`

**File:** [src/profiler/metadata.rs:107](src/profiler/metadata.rs#L107)

**Change:** Remove the `|` alias:

```rust
Encoding::RLE_DICTIONARY => "RLE_DICTIONARY".to_string(),
Encoding::PLAIN_DICTIONARY => "PLAIN_DICTIONARY".to_string(),
```

**Note:** This is a semantics change. Any downstream code that matches on `"RLE_DICTIONARY"` may now also need to handle `"PLAIN_DICTIONARY"`. Search call sites before/after.

**Effort:** Trivial (2 lines), then grep cleanup.

---

### T02 — Add per-row-group fields to `ColumnMetaSummary`

**File:** [src/profiler/metadata.rs:5-19](src/profiler/metadata.rs#L5-L19)

**Change:** Add to the struct:

```rust
#[serde(default)]
pub per_row_group_encodings: Vec<Vec<String>>,
#[serde(default)]
pub per_row_group_compressed_bytes: Vec<i64>,
#[serde(default)]
pub per_row_group_uncompressed_bytes: Vec<i64>,
#[serde(default)]
pub per_row_group_dict_page_bytes: Vec<Option<i64>>,
```

**Effort:** Trivial (5 lines).

---

### T03 — Populate per-RG fields in `build_file_profile_from_metadata`

**File:** [src/profiler/metadata.rs:211 onward](src/profiler/metadata.rs#L211)

**Change:** In the row group loop, push per-RG values into the new vecs. For dict page bytes, compute from `col.dictionary_page_offset()` and `col.data_page_offset()`:

```rust
let dict_bytes = match col.dictionary_page_offset() {
    Some(dict_off) => Some(col.data_page_offset() - dict_off),
    None => None,
};
per_row_group_dict_page_bytes.push(dict_bytes);

let mut rg_encodings: Vec<String> = col.encodings().iter().map(|e| format_encoding(*e)).collect();
rg_encodings.sort();
rg_encodings.dedup();
per_row_group_encodings.push(rg_encodings);

per_row_group_compressed_bytes.push(col.compressed_size());
per_row_group_uncompressed_bytes.push(col.uncompressed_size());
```

Initialise the vecs with the right capacity before the loop.

**Test:** Load any fixture, assert `summary.per_row_group_encodings.len() == num_row_groups`.

**Effort:** Small (~20 lines).

---

### T04 — Update `ColumnMetaSummary` construction to include new fields

**File:** [src/profiler/metadata.rs](src/profiler/metadata.rs) (same function, end of loop)

**Change:** Include the new fields when constructing the final `ColumnMetaSummary`. Keep the aggregated `encodings` field populated as before (HashSet → sorted Vec).

**Effort:** Trivial.

---

## Phase 2 — Rust diagnostics

### T05 — Create `src/diagnostics.rs`

**File:** new file at `src/diagnostics.rs`

**Change:** Define structs:

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticStatus {
    Match,
    FallbackDictionary,
    IneffectiveEncoding,
    Mismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDiagnostic {
    pub column_name: String,
    pub current_encodings: Vec<String>,
    pub per_row_group_summary: Option<String>,
    pub current_codec: String,
    pub current_compression_ratio: f64,
    pub status: DiagnosticStatus,
    pub observation: String,
    pub cause_hypothesis: Option<String>,
    pub supporting_metric: Option<String>,
}
```

Declare the module in [src/lib.rs](src/lib.rs).

**Effort:** Trivial.

---

### T06 — Implement encoding set helpers

**File:** `src/diagnostics.rs`

**Change:** Two helpers operating on `Vec<Vec<String>>` (per-RG encoding lists):

1. `aggregate_data_encodings(per_rg: &[Vec<String>]) -> Vec<String>`
   - Union of all encodings across all RGs.
   - Filter out `"RLE"` and `"BIT_PACKED"` (def/rep level encodings, not data).
   - Sort + dedup for stable display.

2. `per_row_group_variance_summary(per_rg: &[Vec<String>]) -> Option<String>`
   - `None` if every RG has an identical (filtered) encoding set.
   - Otherwise, group consecutive RGs with identical sets and format like `"RLE_DICTIONARY in RGs 0,1; PLAIN in RGs 2,3"`.

**Effort:** Small (~40 lines, unit-testable with no Parquet dependency).

---

### T07 — Implement diagnostic inference function

**File:** `src/diagnostics.rs`

**Change:** Define `fn diagnose_column(meta: &ColumnMetaSummary, rec: &ColumnRecommendation) -> ColumnDiagnostic`.

Apply rules in order (first match wins):

**R2 — Fallback (mid-chunk):** Any RG's encodings contain BOTH a dictionary variant AND `PLAIN` → `FallbackDictionary`. Observation includes which RGs. Supporting metric = largest dict page size.

**R2b — Fallback (cross-RG):** Some RGs have dictionary encoding, others have PLAIN only → `FallbackDictionary`. Observation: "Dictionary used in RGs X; fell back to PLAIN in RGs Y."

**R3 — Dictionary never attempted:** Recommendation is `RLE_DICTIONARY` but no RG has any dictionary encoding → `Mismatch` with cause "Writer did not apply dictionary encoding."

**R4 — Ineffective DELTA:** `DELTA_BINARY_PACKED` in encodings AND overall compression ratio < 1.5 AND recommender says PLAIN → `IneffectiveEncoding`.

**R5 — Ineffective BSS:** `BYTE_STREAM_SPLIT` in encodings AND overall ratio < 1.1 → `IneffectiveEncoding`.

**R1 — Match:** The recommended encoding is present in `current_encodings`, no fallback indicators present (no `PLAIN` coexisting with dictionary variants, no cross-RG variance that constitutes a fallback), AND current codec (with level) == recommended codec (with level), AND neither R4 nor R5 fired → `Match`.

**R6 — Mismatch:** None of the above → `Mismatch`.

Skip R4/R5 if `uncompressed_bytes < 10_000`.

**Test:** One test per rule (fires / does not fire).

**Effort:** Medium (~120 lines including tests).

---

### T08 — Thread `ColumnDiagnostic` through `TuneReport`

**Files:** [src/tuner.rs](src/tuner.rs)

**Change:** Add `diagnostics: Vec<ColumnDiagnostic>` to `TuneReport`. In `build_tune_report_from_profiles`, after computing recommendations, call `diagnose_column` for each column and push the results.

**Test:** CLI JSON output includes the `diagnostics` field.

**Effort:** Small (~15 lines).

---

### T09 — Rebuild WASM

**Command:** `cd web && npm run wasm:build`

**Effort:** 2–5 min compile.

---

## Phase 3 — JS UI

### T10 — Render "Current vs Recommended" section in column card

**File:** [web/src/render/columns.js](web/src/render/columns.js), in `buildColumnCard`

**Change:** Look up `report.diagnostics.find(d => d.column_name === col.column_name)`. Add a new accordion section at the top (above "Why this encoding?").

For `Match` status, render a single line: `✓ File matches recommendation.`

For other statuses, render a two-column table. The "Have" value is `current_encodings.join(", ")` followed by ` + {codec}`. If `per_row_group_summary` is set, show it on a muted second line under "Have":
```
Have        PLAIN, PLAIN_DICTIONARY, RLE_DICTIONARY + ZSTD:3  (ratio 1.4×)
            └ RLE_DICTIONARY in RGs 0,1; PLAIN in RGs 2,3
Recommend   PLAIN + ZSTD:3
Status      Fallback detected — dictionary overflowed in row group 2
Metric      dict page ≈ 1.02 MB
```

**Effort:** Medium (~50 lines).

---

### T11 — Card header status pill

**File:** [web/src/render/columns.js](web/src/render/columns.js), in `buildColumnCard` header

**Change:** Add a small pill next to the `ConfidenceBadge`. Colour-code:
- Match → `✓` green, `bg-green-900/40 text-green-300`
- FallbackDictionary → `⚠ fallback` amber, `bg-amber-900/40 text-amber-300`
- IneffectiveEncoding → `◆ weak` gray, `bg-gray-800 text-gray-400`
- Mismatch → `✎ diff` gray, `bg-gray-800 text-gray-400`

Omit the pill entirely if there's no diagnostic (graceful degradation).

**Effort:** Small (~30 lines).

---

### T12 — Summary: "File health" row

**File:** [web/src/render/summary.js](web/src/render/summary.js)

**Change:** Compute counts from `report.diagnostics` with all four statuses broken out (decision A04):

```js
const diags = report.diagnostics ?? [];
const total      = diags.length;
const matched    = diags.filter(d => d.status === 'Match').length;
const fallbacks  = diags.filter(d => d.status === 'FallbackDictionary').length;
const weak       = diags.filter(d => d.status === 'IneffectiveEncoding').length;
const mismatches = diags.filter(d => d.status === 'Mismatch').length;
```

Append after the Analysis confidence row:

```
File health:   {matched} of {total} match
               [if fallbacks > 0]  {fallbacks} fallback{s}
               [if weak > 0]       {weak} weak
               [if mismatches > 0] {mismatches} mismatch{es}
```

Lines for zero-count statuses are omitted so clean files show only the top line.

**Effort:** Small (~25 lines).

---

### T13 — Update filter: "Non-matching only"

**File:** [web/src/render/columns.js](web/src/render/columns.js)

**Change:** Rename the checkbox "Changed only" → "Non-matching only". Update the filter predicate to use `diagnostic.status !== 'Match'` instead of encoding/codec change detection.

Fall back to the old predicate (encoding/codec change detection) when `report.diagnostics` is absent (backwards compatibility for cached reports).

**Effort:** Small (~15 lines).

---

### T14 — Wire summary health count to filter navigation

**File:** [web/src/render/summary.js](web/src/render/summary.js), [web/src/render/columns.js](web/src/render/columns.js)

**Change:** Make the "File health" count clickable. On click:
1. Scroll to the Columns section.
2. Dispatch a custom event `autoparq:filter-non-matching` that Columns listens for and toggles its filter to "Non-matching only".

**Effort:** Small (~10 lines + event wiring).

---

## Task ordering

```
T01 → T02 → T03 → T04 → T05 → T06 → T07 → T08 → T09
                                                    ↓
                              T10 + T11 + T12 + T13 + T14 (JS, can parallelise)
```

T01–T04 must complete before T05 (new fields need to exist).
T05–T07 before T08 (diagnostics struct must exist before TuneReport references it).
T09 (WASM rebuild) gates all JS work.
T10–T14 are independent of each other within Phase 3.
