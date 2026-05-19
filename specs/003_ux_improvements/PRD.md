# autoparq Web App — UX Improvements PRD

**Version:** 1.0  
**Status:** Draft  
**Scope:** Column card clarity, impact scoring, codec display, and column size surfacing

---

## 1. Background

The web app successfully analyzes Parquet files in-browser and renders per-column recommendations. User testing with a 152 MB flight dataset (67 columns, 7.5M rows) revealed four issues that make the report hard to act on:

1. **Impact stars mislead**: High-cardinality dictionary columns show 5 stars even though their encoding is already correct (RLE_DICTIONARY present). Stars quantify how much the *current* encoding matches the rule trigger, not how much *improvement* applying the recommendation would deliver. A user looking at the report cannot tell which columns to focus on.

2. **ZSTD vs ZSTD:3 ambiguity**: The profiler reads the current file's codec as `"ZSTD"` (level discarded). The recommender returns `"ZSTD:3"`. These appear different on screen but may be identical. The user cannot tell whether a codec change is needed.

3. **No column sizes**: Column cards show encoding and codec but no byte count. A user cannot judge ROI — changing encoding on a 50 KB string column is not worth the same as changing a 30 MB integer column.

4. **No "needs action" signal**: There is no visual separation between columns where the recommendation differs from the current state and columns that are already well-configured. Every column looks equally important to read.

---

## 2. Goals

- A user should be able to scan the column list and immediately know which columns to act on and which to ignore.
- A user should never be confused about whether "current" and "recommended" settings are actually different.
- Column size context should always be visible without expanding any accordion.

### Non-goals

- Re-architecting the recommendation engine heuristics beyond the impact scoring fix.
- Adding new recommendation rules.
- Changing the CLI output format.

---

## 3. Fix 1 — Impact Stars: measure potential gain, not rule match strength

### Problem

`compute_impact_stars()` in `src/tuner.rs` currently awards stars based on which encoding rule fired and whether the column is currently PLAIN. A column with `cardinality_ratio=0.0000` (10 distinct / 1M rows) scores 5 stars regardless of whether the file already uses RLE_DICTIONARY. Since this file already used RLE_DICTIONARY on these columns, the user sees 5 stars for zero-change columns.

### Correct Semantics

**Stars represent: how much improvement does the recommended change deliver over the current state?**

| Stars | Meaning |
|-------|---------|
| ★★★★★ | Major change; will meaningfully reduce size or improve read speed |
| ★★★★☆ | Significant change |
| ★★★☆☆ | Moderate change |
| ★★☆☆☆ | Minor change (codec level only, or small column) |
| ★☆☆☆☆ | No encoding change; file is already at or near the recommendation |

### New Scoring Logic

Stars are determined by comparing recommended state to current state. In priority order:

1. **Encoding changes from PLAIN to a compressed encoding** (PLAIN → RLE_DICTIONARY, PLAIN → DELTA_BINARY_PACKED, PLAIN → BYTE_STREAM_SPLIT): **4–5 stars** based on column weight (size / total file size).
2. **Encoding already matches recommendation** but codec level changes (e.g., ZSTD → ZSTD:3, ZSTD:1 → ZSTD:6): **2 stars**.
3. **Encoding already matches AND codec already matches**: **1 star** — no action needed.
4. **BOOLEAN (RLE is automatic)**: **1 star** (handled by the writer, no user action).
5. **PlainUuid (keep PLAIN to avoid dictionary overflow)**: **1 star** — this is "do nothing" advice.

For rules 1 the exact star count is:

```
fn compute_impact_stars(enc, codec, meta, file_profile) -> u8 {
    let enc_already_set = meta.encodings contains enc.encoding;
    let codec_already_set = current_codec_level == recommended_codec_level;

    if enc_already_set && codec_already_set { return 1; }
    if enc_already_set { return 2; }  // codec change only

    // Encoding will change — weight by column size
    let col_weight = meta.uncompressed_bytes / file_total_uncompressed_bytes;
    match col_weight {
        w if w >= 0.10 => 5,
        w if w >= 0.04 => 4,
        w if w >= 0.01 => 3,
        _ => 2,
    }
}
```

**Implementation location:** `src/tuner.rs`, function `compute_impact_stars()` (currently lines 316–334). The function signature needs to gain access to `file_total_uncompressed_bytes` (available at call site from `file_profile`).

### Frontend change

None required — stars render the same way; only the Rust values change.

---

## 4. Fix 2 — Codec display: include compression level from file metadata

### Problem

`format_compression()` in `src/profiler/metadata.rs` (line 92) discards the ZSTD level:

```rust
Compression::ZSTD(_) => "ZSTD".to_string(),
```

The `parquet` crate's `Compression::ZSTD` variant carries an `Option<ZstdLevel>`. When level is present it should be included in the display string.

### Fix

```rust
Compression::ZSTD(level) => match level {
    Some(lvl) => format!("ZSTD:{}", lvl.compression_level()),
    None => "ZSTD".to_string(),
},
```

Same treatment for `GZIP` and `BROTLI` which also carry levels:

```rust
Compression::GZIP(level) => match level {
    Some(lvl) => format!("GZIP:{}", lvl.compression_level()),
    None => "GZIP".to_string(),
},
Compression::BROTLI(level) => match level {
    Some(lvl) => format!("BROTLI:{}", lvl.quality()),
    None => "BROTLI".to_string(),
},
```

**Implementation location:** `src/profiler/metadata.rs`, `format_compression()`.

### Frontend change

The comparison logic in `columns.js` compares `currentCodec` to `col.recommended_codec` (without level). When levels are now included in the current codec string, the comparison should normalize: strip `:level` suffix for the equality check, but display both levels when they differ. Concretely:

- Current: `ZSTD:3`, Recommended: `ZSTD:3` → show single pill `ZSTD:3`, no arrow (no change)
- Current: `ZSTD:1`, Recommended: `ZSTD:3` → show `ZSTD:1 → ZSTD:3` with arrow
- Current: `ZSTD` (unknown level), Recommended: `ZSTD:3` → show `ZSTD → ZSTD:3` with a note "(level unknown)"

**Implementation location:** `web/src/render/columns.js`, `buildColumnCard()` codec comparison block.

---

## 5. Fix 3 — Column sizes on every card

### Problem

`ColumnMetaSummary` (in `file_profile.columns`) carries `compressed_bytes` and `uncompressed_bytes` per column. These are currently only available in the expanded accordion's raw stats. The card header has no size indicator.

### Fix — Frontend only (no Rust change needed)

`report.file_profile.columns` is already sent to the browser as part of `TuneReport`. Build a `sizeMap` in `renderColumns()` alongside the existing `currentEncodingMap`:

```js
const sizeMap = {};
for (const c of report.file_profile?.columns ?? []) {
  sizeMap[c.name] = { compressed: c.compressed_bytes, uncompressed: c.uncompressed_bytes };
}
```

Add to the card header, right-aligned before the chevron:

```
[column name] [stars] [badge] [type]    [1.4 MB / 6.2 MB]  ▼
```

Where `1.4 MB` is compressed and `6.2 MB` is uncompressed. Format with `_human_bytes()` equivalent in JS. Color the size based on its share of total file size:

| Share of file | Color |
|---------------|-------|
| > 10% | `text-amber-400` — large column, change has real ROI |
| 2–10% | `text-gray-300` |
| < 2% | `text-gray-600` — small column, low ROI |

**Implementation location:** `web/src/render/columns.js`, `buildColumnCard()` header section.

---

## 6. Fix 4 — "Needs action" status indicator

### Problem

Every column card looks the same visually. A file with 67 columns where 33 are already optimal and 34 need encoding changes requires scanning every card to understand the split. The summary section shows an encoding breakdown count but it doesn't tell you which columns are which.

### Fix — Frontend only

Add a left-border color to each card based on whether any change is recommended:

| State | Left border | Meaning |
|-------|-------------|---------|
| Encoding changes | `border-l-4 border-l-amber-500` | Action recommended |
| Codec level change only | `border-l-4 border-l-blue-700` | Minor tweak |
| Already optimal | `border-l-4 border-l-gray-700` | No action needed |

Logic:

```js
const encChanged = currentEnc !== '—' && currentEnc !== recEnc;
const codecChanged = /* base codec name differs */;
const borderClass = encChanged
  ? 'border-l-4 border-l-amber-500'
  : codecChanged
    ? 'border-l-4 border-l-blue-700'
    : 'border-l-4 border-l-gray-700';
```

Additionally, the "Changed only" filter checkbox already exists — once impact stars are fixed (Fix 1) it becomes more useful as a "show only amber-bordered columns" proxy.

**Implementation location:** `web/src/render/columns.js`, `buildColumnCard()`.

---

## 7. Summary of changes by file

| File | Type | Change |
|------|------|--------|
| `src/tuner.rs` | Rust | Rewrite `compute_impact_stars()` to weight by column size share and current-state delta |
| `src/profiler/metadata.rs` | Rust | `format_compression()` — include ZSTD/GZIP/BROTLI level in returned string |
| `web/src/render/columns.js` | JS | Size badge in card header; left-border status indicator; codec comparison normalization |

Rust changes require a WASM rebuild. JS changes hot-reload in dev.

---

## 8. Acceptance Criteria

**Fix 1 (impact stars):**
- A column where `current encodings` already contains the recommended encoding AND current codec matches → 1 star
- A column where only the codec level differs → 2 stars  
- A column where encoding changes from PLAIN → any non-PLAIN → 3–5 stars based on size share
- The 5-star columns in the NYC Taxi file are timestamp/ID columns that currently use PLAIN, not the string columns that already use RLE_DICTIONARY

**Fix 2 (ZSTD level):**
- A file written with ZSTD level 3 shows current codec as `ZSTD:3` not `ZSTD`
- A file written with ZSTD (no explicit level) shows `ZSTD`
- When current and recommended codec are both `ZSTD:3`, the card shows a single pill with no arrow

**Fix 3 (column sizes):**
- Every card header shows `X.X MB / Y.Y MB` (compressed / uncompressed)
- Columns consuming > 10% of file size have amber-colored size text
- Columns < 2% of file size have dim gray size text

**Fix 4 (status indicator):**
- Columns where encoding changes → amber left border
- Columns where only codec level changes → blue left border  
- Columns already at recommended settings → gray left border
- A 67-column file shows the amber/blue/gray split at a glance without expanding any card
