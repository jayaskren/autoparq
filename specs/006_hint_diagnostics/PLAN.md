# PLAN — Hint Diagnostics (Spec 006)

## Approach

Three phases, each testable independently:

1. **Rust metadata layer** — preserve per-row-group information that's currently being thrown away during aggregation.
2. **Rust diagnostics layer** — a new module that combines profile data + recommendation to produce a per-column diagnostic.
3. **JS UI** — three touchpoints: column card accordion section, card header pill, summary health count.

The heavy lift is Phase 1. Phase 2 is pure combinator logic. Phase 3 is rendering.

---

## Phase 1 — Rust: preserve per-row-group data

### Step 1.1 — Expand `ColumnMetaSummary`

Add new fields to the struct in [metadata.rs:5–19](src/profiler/metadata.rs#L5-L19):

```rust
pub per_row_group_encodings: Vec<Vec<String>>,
pub per_row_group_compressed_bytes: Vec<i64>,
pub per_row_group_uncompressed_bytes: Vec<i64>,
pub per_row_group_dict_page_bytes: Vec<Option<i64>>,
```

### Step 1.2 — Stop collapsing `PLAIN_DICTIONARY` → `RLE_DICTIONARY`

In `format_encoding` ([metadata.rs:98–110](src/profiler/metadata.rs#L98-L110)), split the match arm:

```rust
Encoding::RLE_DICTIONARY => "RLE_DICTIONARY".to_string(),
Encoding::PLAIN_DICTIONARY => "PLAIN_DICTIONARY".to_string(),
```

### Step 1.3 — Populate per-RG fields during metadata build

In `build_file_profile_from_metadata` ([metadata.rs:211 onward](src/profiler/metadata.rs#L211)), the current loop iterates `metadata.row_groups()` and accumulates. Extend the loop to also push per-RG values into the new fields.

For `per_row_group_dict_page_bytes`: use `col.dictionary_page_offset()` and `col.data_page_offset()`. When a dictionary page exists, `dict_bytes = data_page_offset - dict_page_offset`. When no dictionary page, push `None`.

### Step 1.4 — Propagate through serialisation

The `ColumnMetaSummary` is serialised into the TuneReport via `FileProfile`. Adding fields with `serde(default)` on the deserialise side keeps cached reports from older versions backwards-compatible (important because the WASM `recommend_from_profile` path deserialises cached profiles).

Add `#[serde(default)]` to the four new fields.

---

## Phase 2 — Rust: diagnostics module

### Step 2.1 — Create `src/diagnostics.rs`

```rust
pub struct ColumnDiagnostic {
    pub column_name: String,
    pub current_encodings: Vec<String>,        // full data-encoding set, sorted and deduped
    pub per_row_group_summary: Option<String>, // populated only when encodings vary across RGs
    pub current_codec: String,
    pub current_compression_ratio: f64,
    pub status: DiagnosticStatus,
    pub observation: String,
    pub cause_hypothesis: Option<String>,
    pub supporting_metric: Option<String>,  // e.g., "dict page = 1.02 MB"
}

pub enum DiagnosticStatus {
    Match,
    FallbackDictionary,
    IneffectiveEncoding,
    Mismatch,
}
```

### Step 2.2 — Encoding set aggregation (no single primary)

Show the *full* data-encoding set rather than picking a single primary (decision A01).

**Aggregated set (`current_encodings`):** union of all encodings across all RGs, with `RLE` and `BIT_PACKED` filtered out (these are definition/repetition-level encodings, not data encodings). Sort and dedup for stable display — e.g., `["PLAIN", "PLAIN_DICTIONARY", "RLE_DICTIONARY"]`.

**Per-RG variance summary (`per_row_group_summary`):** `None` when all RGs have identical encoding sets. Otherwise, a compact string grouping consecutive RGs with identical sets, e.g., `"RLE_DICTIONARY in RGs 0,1; PLAIN in RGs 2,3"`.

Rule-matching (R2–R6 below) still needs to detect specific encoding presence/absence; that operates on the aggregated set plus per-RG data, not on a single primary.

### Step 2.3 — Apply R1–R6 rules

The diagnostic function takes:
- `ColumnMetaSummary` (current state)
- The `ColumnRecommendation` from the recommender (recommended state)

And returns a `ColumnDiagnostic`. Apply rules in order; first match wins.

### Step 2.4 — Thread `ColumnDiagnostic` through `TuneReport`

Add `diagnostics: Vec<ColumnDiagnostic>` to the `TuneReport` struct. Populate in `build_tune_report_from_profiles` (the shared function used by CLI, native, and WASM paths).

---

## Phase 3 — JS: UI changes

### Step 3.1 — Column card: "Current vs Recommended" accordion section

Add at the **top** of the accordion content, above "Why this encoding?". For a Match status, render as a compact single line. For non-Match statuses, render:
- Two-column diff. The "Have" line joins `current_encodings` with `", "` (e.g., `PLAIN, PLAIN_DICTIONARY, RLE_DICTIONARY`) followed by `+ {codec}`.
- If `per_row_group_summary` is set, show it on a second line under "Have" in a muted style.
- Status text
- Cause hypothesis
- Supporting metric

The block uses the same visual pattern as the existing "Raw Statistics" block for consistency.

### Step 3.2 — Card header status pill

Add next to `ConfidenceBadge`:

| Status | Pill |
|--------|------|
| Match | `✓` green subtle |
| FallbackDictionary | `⚠ fallback` amber |
| IneffectiveEncoding | `◆ weak` gray |
| Mismatch | `✎ mismatch` gray |

### Step 3.3 — Summary: "File health" stat

In the left column of the Summary section (Size & Speed Estimates card), append a new row with a per-status breakout (decision A04):

```
File health: 10 of 14 match
             2 fallbacks, 1 weak, 1 mismatch
```

Lines for zero-count statuses are omitted (e.g., if there are no fallbacks, don't render that line).

Clicking navigates to Columns and applies a filter showing only non-Match columns. This extends the existing filter system — add a new filter option "Diagnostics ≠ match" alongside "Changed only".

### Step 3.4 — Filter update

The existing "Changed only" checkbox filters for columns where current encoding/codec differs from recommendation. That's close to but not the same as non-Match (a file in `FallbackDictionary` state is a non-Match case that may or may not register as "changed" depending on how the diff is defined). Change the filter to use `diagnostic.status !== 'Match'` as the source of truth.

Rename the checkbox label to "Non-matching only" to match the new semantics.

---

## Build and test

After Phase 1:
```bash
cargo test -p autoparq --test diagnostics   # new test file
cd web && npm run wasm:build                 # rebuild WASM
```

Manual test:
1. Load a sample file where we know the encoding state (e.g., a file with high-cardinality string column that triggers dict fallback).
2. Verify the column card shows "Fallback detected" for that column with the overflow cause.
3. Verify the summary shows the correct file health count.
4. Verify the filter toggles to show only non-matching columns.

---

## Risks

| Risk | Mitigation |
|------|-----------|
| Old cached reports in localStorage lack new fields | `#[serde(default)]` on new fields; empty vecs are valid |
| `dictionary_page_offset` may be absent even when encoding says `RLE_DICTIONARY` (some writers embed dict differently) | Treat `None` as "unknown dict size"; don't assert fallback cause from missing offset alone |
| R4/R5 false positives on small row groups | Skip effectiveness check when `uncompressed_bytes < 10 KB` — too small to judge |
| Recommender changes after file was written produce false Mismatch | Intended behaviour; the diff is the feature |
| `PLAIN + RLE_DICTIONARY` coexistence has another legitimate reason (non-fallback) | Document in the observation text that we assume fallback; users can judge |
