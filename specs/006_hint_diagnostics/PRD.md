# PRD — Hint Diagnostics: "What You Have" vs "What We Recommend" (Spec 006)

## Problem

Parquet writers accept encoding and codec hints (e.g., "use RLE_DICTIONARY for this column"), but writers can silently fall back. Users have no feedback about whether their hints actually took effect:

- **Dictionary encoding** falls back to PLAIN when the dictionary page overflows the writer's limit (default 1 MB). Once it falls back for a row group, it stays PLAIN for that row group. Subsequent row groups may still use dictionary if the per-chunk dictionary fits.
- **DELTA_BINARY_PACKED** doesn't "fail" mechanically, but is ineffective when values aren't monotonic — the writer uses it anyway and the user doesn't realise the hint provided no benefit.
- **BYTE_STREAM_SPLIT** on floats is similar — applied as hinted, but only effective when byte distributions correlate across values.

Right now autoparq tells the user what to set, but never tells them what is *currently* set or whether the current state is effective. A user who tuned a table months ago has no way to audit it.

## Goals

1. For every column, show the user a side-by-side **diff**: "What the file currently has" vs "What we'd recommend now."
2. When current and recommended differ, explain the likely cause (dictionary overflow, ineffective encoding, suboptimal codec).
3. Use only footer + page-header reads — no data scan beyond what the profiler already does.
4. Every observation comes with the specific metric that supports it (no speculation).

## Non-Goals

- Asking the user what they hinted. We see the result; we don't ask for intent.
- Diagnosing runtime codec substitution (e.g., ZSTD → SNAPPY because ZSTD lib missing at write time). Rare and not detectable from the file alone.
- Recomputing recommendations differently based on what's in the file. The existing recommender is the "what we recommend" side.
- Rewriting the file. This is a read-only diagnostic.

---

## Framing: The Diff

The central UX primitive is a **per-column diff**:

```
┌───────────────────────── user_id ─────────────────────────┐
│ Have:        DELTA_BINARY_PACKED + ZSTD:3                 │
│ Recommend:   DELTA_BINARY_PACKED + ZSTD:3                 │
│ Status:      File matches recommendation.                 │
└───────────────────────────────────────────────────────────┘

┌──────────────────────── status_code ──────────────────────┐
│ Have:        PLAIN + ZSTD:3  (ratio 1.4×)                 │
│ Recommend:   RLE_DICTIONARY + ZSTD:3                      │
│ Status:      Dictionary encoding was not used.            │
│              Cardinality = 7 distinct values, well within │
│              the default 1 MB dictionary page. The file   │
│              would benefit from retuning.                 │
└───────────────────────────────────────────────────────────┘

┌──────────────────────── event_blob ───────────────────────┐
│ Have:        PLAIN, PLAIN_DICTIONARY, RLE_DICTIONARY      │
│              + ZSTD:3  (dict page reached 1.02 MB in RG 2)│
│ Recommend:   PLAIN + ZSTD:3                               │
│ Status:      Dictionary overflowed mid-chunk in RG 2,     │
│              fell back to PLAIN. High cardinality makes   │
│              PLAIN the right choice here.                 │
└───────────────────────────────────────────────────────────┘
```

Three possible statuses per column:

| Status | Meaning |
|--------|---------|
| **Match** | Current encoding+codec equal the recommendation; encoding is effective (ratio meets expectations). |
| **Fallback detected** | Mixed encodings across row groups — dictionary was used in some, PLAIN in others. |
| **Mismatch** | Current differs from recommendation. May or may not be a problem; diagnostic explains. |

---

## Feature Breakdown

### D01 — Preserve per-row-group encoding information

`metadata.rs` currently aggregates encodings across row groups into a single deduplicated `Vec<String>` ([metadata.rs:200–216](src/profiler/metadata.rs#L200-L216)). It also aliases `Encoding::PLAIN_DICTIONARY` and `Encoding::RLE_DICTIONARY` to the single string `"RLE_DICTIONARY"` ([metadata.rs:107](src/profiler/metadata.rs#L107)), losing the fallback signal.

**Changes:**
- Add `per_row_group_encodings: Vec<Vec<String>>` to `ColumnMetaSummary` (outer index = row group, inner vec = encodings present in that RG's chunk).
- Keep the aggregated `encodings` field for backwards compatibility but make it truthful: do NOT collapse `PLAIN_DICTIONARY` → `RLE_DICTIONARY`. These distinguish the dictionary-page-reference encoding from the data-pages-fell-back-to-plain encoding.

### D02 — Per-row-group compression ratio

**Changes:**
- Add `per_row_group_compressed_bytes: Vec<i64>` and `per_row_group_uncompressed_bytes: Vec<i64>` to `ColumnMetaSummary`.
- JS derives the per-RG ratio from these as needed.

### D03 — Dictionary page size extraction

Each `ColumnChunkMetaData` exposes `dictionary_page_offset()` and `data_page_offset()`. When a dictionary page is present, the dictionary's on-disk size is approximately `data_page_offset - dictionary_page_offset` (minus page header bytes, but that's small and consistent).

**Changes:**
- Add `per_row_group_dict_page_bytes: Vec<Option<i64>>` to `ColumnMetaSummary`. `None` when no dictionary page exists for that chunk.
- Populate from offset difference.

### D04 — Diagnostic inference

Add a new module `src/diagnostics.rs` with a per-column diagnosis:

```rust
pub struct ColumnDiagnostic {
    pub column_name: String,
    pub current_encodings: Vec<String>,  // full deduplicated data-encoding set (excl. RLE/BIT_PACKED for def/rep)
    pub current_codec: String,
    pub current_compression_ratio: f64,
    pub status: DiagnosticStatus,
    pub observation: String,             // human-readable "what we see"
    pub cause_hypothesis: Option<String>, // "Dictionary page overflowed in RG 2" etc.
}

pub enum DiagnosticStatus {
    Match,               // current == recommended, encoding effective
    Fallback,            // dictionary fell back to PLAIN in some RGs
    IneffectiveEncoding, // encoding applied but ratio below expectations
    Mismatch,            // current != recommended, neither Fallback nor Ineffective
}
```

Detection rules:

| Rule | Status | Condition |
|------|--------|-----------|
| R1 | Match | `current_encoding == recommended_encoding` AND `current_codec == recommended_codec` AND encoding is effective (per R4 below). |
| R2 | Fallback (dictionary) | Some RGs contain `PLAIN_DICTIONARY`/`RLE_DICTIONARY`, others contain `PLAIN` only. Cause: dictionary overflow. |
| R3 | Fallback (writer didn't apply) | Recommendation is `RLE_DICTIONARY` but NO RG contains any dictionary encoding. Cause: writer never attempted dictionary. |
| R4 | IneffectiveEncoding (DELTA) | `DELTA_BINARY_PACKED` present AND `compression_ratio < 1.5` AND recommended encoding is PLAIN. |
| R5 | IneffectiveEncoding (BSS) | `BYTE_STREAM_SPLIT` present AND `compression_ratio < 1.1`. |
| R6 | Mismatch | Current and recommended differ, none of R2–R5 apply. Cause: file predates current recommendation, or priority/engine changed. |

### D05 — UI: per-column diff section

In the column card accordion, add a new section **"Current vs Recommended"** at the top, before "Why this encoding?":

- Shows the two-line diff (Have / Recommend). The "Have" line lists the full set of data encodings found in the file (e.g., `PLAIN, RLE_DICTIONARY`). If encodings differ across row groups, a per-RG breakdown appears below the main line (e.g., `"RLE_DICTIONARY in RGs 0,1; PLAIN in RGs 2,3"`).
- Shows a status badge: ✓ Match / ⚠ Fallback / ◆ Ineffective / ✎ Mismatch.
- Shows the cause hypothesis and any supporting metric (e.g., "dict page = 1.02 MB", "monotonicity 0.54").

For columns where Status = Match, the section collapses to a single line ("File matches recommendation.").

### D06 — UI: summary-level health metric

In the Summary section, add a new stat broken out by status:

```
File health:   10 of 14 match
               2 fallbacks, 1 weak, 1 mismatch
```

Lines for zero-count statuses are omitted.

Clicking this scrolls to the Columns section and auto-filters to "Changed only" = effectively "non-match only." (Reuses the existing "Changed only" filter semantics; we may extend it to a three-state filter: all / match-only / non-match-only.)

### D07 — Card badge next to confidence

In the column card header, add a small status pill next to the confidence badge:

- ✓ (green) Match
- ⚠ (amber) Fallback
- ◆ (gray) Ineffective / Mismatch (combined — visual emphasis reserved for fallback)

Users scanning cards can see file health at a glance without expanding.

---

## Detection rule details

### R2 — Dictionary fallback detection

A column's chunk in a given row group contains some subset of the following encodings:

- `PLAIN` — data pages encoded as plain values
- `RLE_DICTIONARY` — the normal "dictionary was used" signal (dictionary page present, data pages reference it)
- `PLAIN_DICTIONARY` — the fallback signal: dictionary was started, filled up, subsequent data pages bailed to PLAIN

The canonical fallback signature is: a chunk contains BOTH `RLE_DICTIONARY` (or `PLAIN_DICTIONARY`) AND `PLAIN`. This means the writer started with a dictionary, the dictionary overflowed mid-chunk, and subsequent data pages switched to PLAIN.

Fallback across row groups: RG 0 contains `RLE_DICTIONARY` only, RG 3 contains `PLAIN` only → writer attempted dictionary per row group; some fit, some didn't.

**Both signals are detectable from D01 (per-RG encodings).**

Supporting metric: `per_row_group_dict_page_bytes[rg]` — if > 900 KB, the dict page was near the 1 MB limit and overflow is the likely cause.

### R4/R5 — Ineffective encoding thresholds

These thresholds are heuristics, not certainties. Ship them with the explicit supporting metric shown in the UI (e.g., "DELTA present, ratio=1.2×"). Users can judge for themselves if they disagree.

The thresholds (`1.5` for DELTA, `1.1` for BSS) should match the effectiveness thresholds implicit in our own recommender. They are not a new source of truth — they're the same line our recommender uses to decide whether to recommend the encoding in the first place.

---

## Acceptance Criteria

- [ ] `ColumnMetaSummary` contains `per_row_group_encodings`, `per_row_group_compressed_bytes`, `per_row_group_uncompressed_bytes`, `per_row_group_dict_page_bytes`.
- [ ] `PLAIN_DICTIONARY` and `RLE_DICTIONARY` are reported distinctly (no collapsing).
- [ ] `src/diagnostics.rs` produces a `ColumnDiagnostic` per column, serialised in the TuneReport.
- [ ] Diagnostic exposes the full data-encoding set found in the file (not a single primary).
- [ ] Summary file-health count breaks out fallbacks, weak, and mismatches separately.
- [ ] `PLAIN_DICTIONARY` fallbacks are classified as currently-plain for size prediction purposes (C01 accepted).
- [ ] Every column card shows a "Current vs Recommended" block with the status and cause hypothesis (when non-Match).
- [ ] Summary section shows file health count.
- [ ] Card header shows a status pill next to the confidence badge.
- [ ] No additional data scanning beyond what the profiler already does.
