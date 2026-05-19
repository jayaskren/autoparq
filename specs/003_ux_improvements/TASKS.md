# autoparq UX Improvements — Tasks

## Task Index

| ID | Track | Description | Requires rebuild |
|----|-------|-------------|-----------------|
| U01 | B | Column size badge in card header | No |
| U02 | B | Left-border status indicator (amber/blue/gray) | No |
| U03 | A | Impact stars: weight by change delta + column size share | Yes |
| U04 | A | `format_compression`: include ZSTD/GZIP/BROTLI level | Yes |
| U05 | B | Codec comparison normalization in card (post-A2) | No |
| V-U1 | — | Validation gate: frontend fixes visible in dev | No |
| V-U2 | — | Validation gate: post-WASM-rebuild correctness | Yes |

Do U01 and U02 first (hot-reload). Then U03 + U04 together in one WASM rebuild. U05 (codec normalization) depends on U04 producing level-aware codec strings.

---

## U01 — Column size badge in card header

**File:** `web/src/render/columns.js`

**What:** Add a compressed size indicator to each column card header, color-coded by the column's share of total file size.

**Where to add:**

In `renderColumns()`, alongside the existing `currentEncodingMap` and `currentCodecMap` blocks, build two new maps:

```js
let totalUncompressed = 0;
const sizeMap = {};
if (report.file_profile?.columns) {
  totalUncompressed = report.file_profile.columns
    .reduce((s, c) => s + (c.uncompressed_bytes ?? 0), 0);
  for (const c of report.file_profile.columns) {
    sizeMap[c.name] = {
      compressed: c.compressed_bytes ?? 0,
      share: totalUncompressed > 0 ? (c.uncompressed_bytes ?? 0) / totalUncompressed : 0,
    };
  }
}
```

Add a `humanBytes(n)` helper near the top of the file:

```js
function humanBytes(n) {
  if (n >= 1_073_741_824) return (n / 1_073_741_824).toFixed(1) + ' GB';
  if (n >= 1_048_576)     return (n / 1_048_576).toFixed(1) + ' MB';
  if (n >= 1_024)         return (n / 1_024).toFixed(0) + ' KB';
  return n + ' B';
}
```

In `buildColumnCard()`, before the chevron, insert a size span:

```js
const sz = sizeMap[col.column_name];
if (sz) {
  const sizeSpan = document.createElement('span');
  const colorClass = sz.share >= 0.10 ? 'text-amber-400'
                   : sz.share >= 0.02 ? 'text-gray-400'
                   : 'text-gray-600';
  sizeSpan.className = `font-mono text-xs ${colorClass} ml-auto`;
  sizeSpan.textContent = humanBytes(sz.compressed);
  header.appendChild(sizeSpan);
}
```

Note: `ml-auto` pushes the size to the right. Move the chevron's `ml-auto` to just `ml-1` after the size span is in place.

**Acceptance criteria:**
- Every card shows a compressed size (e.g. `"2.4 MB"`) right-aligned before the chevron
- Columns > 10% of file total: amber text
- Columns 2–10%: gray-400
- Columns < 2%: gray-600 (dim)
- Size `0 B` or absent `file_profile` → size span omitted, no error

---

## U02 — Left-border status indicator

**File:** `web/src/render/columns.js`

**What:** Give each card a colored left border to immediately communicate whether any change is recommended, without reading any text.

**Colors:**
- **Amber** (`border-l-amber-500`): encoding will change
- **Blue** (`border-l-blue-600`): encoding unchanged, codec changes (level or codec name)
- **Gray + dimmed** (`border-l-gray-700 opacity-60`): no changes — already optimal

**Where to add:**

In `buildColumnCard()`, the `encChanged` and `codecChanged` booleans are already computed (for the before/after comparison row). Use them:

```js
const borderClass = encChanged
  ? 'border-l-4 border-l-amber-500'
  : codecChanged
    ? 'border-l-4 border-l-blue-700'
    : 'border-l-4 border-l-gray-700 opacity-70';

card.className = `bg-gray-900 rounded-xl border border-gray-800 mb-3 overflow-hidden ${borderClass}`;
```

**Acceptance criteria:**
- Cards where encoding changes have a visible amber left border
- Cards where only codec level changes have a blue left border
- Cards where nothing changes are visibly dimmed with gray border
- The visual split across all columns is scannable at a glance without expanding any card
- "Changed only" filter checkbox works correctly with dimmed cards still hidden

---

## U03 — Impact stars: weight by change delta and column size

**File:** `src/tuner.rs`

**What:** Rewrite `compute_impact_stars()` so stars represent "how much improvement does this recommendation deliver" rather than "how strongly did the rule fire."

**New signature:**

```rust
fn compute_impact_stars(
    enc: &EncodingRecommendation,
    codec: &CodecRecommendation,
    meta: &ColumnMetaSummary,
    file_total_uncompressed: i64,
) -> u8
```

**New logic:**

```rust
fn compute_impact_stars(
    enc: &EncodingRecommendation,
    codec: &CodecRecommendation,
    meta: &ColumnMetaSummary,
    file_total_uncompressed: i64,
) -> u8 {
    // Check if encoding already matches recommendation
    let enc_already_set = meta.encodings.iter().any(|e| e == &enc.encoding);
    // Check if codec base name already matches (ignore level for this check)
    let codec_already_set = meta.codec.starts_with(&codec.codec);

    if enc_already_set && codec_already_set {
        return 1; // No meaningful change
    }
    if enc_already_set {
        return 2; // Codec-only change
    }

    // Encoding will change — weight by column size share
    let col_uncompressed = meta.uncompressed_bytes;
    let share = if file_total_uncompressed > 0 {
        col_uncompressed as f64 / file_total_uncompressed as f64
    } else {
        0.0
    };

    match share {
        s if s >= 0.10 => 5,
        s if s >= 0.04 => 4,
        s if s >= 0.01 => 3,
        _ => 2,
    }
}
```

**Update all 5 call sites** — each must compute `file_total_uncompressed` and pass it. The value is:

```rust
let file_total_uncompressed: i64 = file_profile.columns.iter()
    .map(|c| c.uncompressed_bytes)
    .sum();
```

This sum is already computed in `predicted_size_reduction_pct()`; compute it once per `build_tune_report_*` function and thread it down.

Call sites to update:
1. `build_tune_report_from_path()` — ~line 585
2. `build_tune_report_from_path_with_progress()` — ~line 627
3. `build_tune_report_from_bytes()` — ~line 740
4. `build_tune_report_from_bytes_with_progress()` — ~line 872
5. `build_tune_report_from_profiles()` — ~line 996

**Snapshot tests:** Existing `insta` snapshots in `tests/` will need to be updated after this change. Run `cargo insta review` and accept new expected values.

**Acceptance criteria:**
- A column where `meta.encodings` contains the recommended encoding AND `meta.codec` starts with `codec.codec` → 1 star
- A column where encoding matches but codec doesn't → 2 stars
- A column with encoding change, < 1% of file → 2 stars
- A column with encoding change, > 10% of file → 5 stars
- In the NYC Taxi file (67 cols): string columns already using RLE_DICTIONARY are 1 star; timestamp/INT columns changing from PLAIN to DELTA are 3–5 stars depending on size

---

## U04 — `format_compression`: include compression level

**File:** `src/profiler/metadata.rs`

**What:** `format_compression()` currently drops the ZSTD compression level. Fix it to return `"ZSTD:3"` instead of `"ZSTD"` when the level is known.

**Current code (line 85–96):**

```rust
fn format_compression(c: Compression) -> String {
    match c {
        Compression::SNAPPY => "SNAPPY".to_string(),
        Compression::GZIP(_) => "GZIP".to_string(),
        Compression::LZO => "LZO".to_string(),
        Compression::BROTLI(_) => "BROTLI".to_string(),
        Compression::LZ4 => "LZ4".to_string(),
        Compression::ZSTD(_) => "ZSTD".to_string(),
        Compression::LZ4_RAW => "LZ4_RAW".to_string(),
        Compression::UNCOMPRESSED => "UNCOMPRESSED".to_string(),
    }
}
```

**Replacement:**

```rust
fn format_compression(c: Compression) -> String {
    match c {
        Compression::SNAPPY => "SNAPPY".to_string(),
        Compression::GZIP(level) => format!("GZIP:{}", level.compression_level()),
        Compression::LZO => "LZO".to_string(),
        Compression::BROTLI(level) => format!("BROTLI:{}", level.quality()),
        Compression::LZ4 => "LZ4".to_string(),
        Compression::ZSTD(level) => format!("ZSTD:{}", level.compression_level()),
        Compression::LZ4_RAW => "LZ4_RAW".to_string(),
        Compression::UNCOMPRESSED => "UNCOMPRESSED".to_string(),
    }
}
```

Note: In parquet 55, `Compression::ZSTD(ZstdLevel)` — the level is NOT optional, always present. `ZstdLevel::compression_level()` returns `i32`. `GzipLevel::compression_level()` returns `u32`. `BrotliLevel::quality()` returns `u32`.

**Also check:** `format_compression` is called in `render_info()` in `python/autoparq/render.py` via the Rust `info` command CLI path. The CLI output will now show `ZSTD:3` instead of `ZSTD` — this is a correct improvement. No change needed in the Python layer.

**Acceptance criteria:**
- `cargo check --features python` passes
- `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` passes
- A parquet file written with `ZSTD` level 3 now shows `ZSTD:3` in the `autoparq info` output
- `format_compression(Compression::ZSTD(ZstdLevel::try_new(3).unwrap()))` returns `"ZSTD:3"`

---

## U05 — Codec comparison normalization in card

**File:** `web/src/render/columns.js`

**Depends on:** U04 (WASM rebuild with level-aware codec strings)

**What:** After U04, the current codec string from `file_profile` will include the level (e.g. `"ZSTD:3"`). The recommended codec is stored as a base name (`"ZSTD"`) plus a separate level field (`recommended_codec_level: 3`). The card's `codecChanged` boolean must compare correctly:

**Current (broken after U04):**
```js
const codecChanged = currentCodec !== '—' && currentCodec !== col.recommended_codec;
// currentCodec = "ZSTD:3", col.recommended_codec = "ZSTD" → always true even when same
```

**Fix:**
```js
const recCodecFull = col.recommended_codec_level != null
  ? `${col.recommended_codec}:${col.recommended_codec_level}`
  : col.recommended_codec;
const codecChanged = currentCodec !== '—' && currentCodec !== recCodecFull;
```

Also update the display strings to use `recCodecFull` consistently in the before/after pill and in the Table view columns.

**Acceptance criteria:**
- A file using ZSTD:3 recommended to stay ZSTD:3 shows a single pill with no arrow
- A file using ZSTD:1 recommended to change to ZSTD:3 shows `ZSTD:1 → ZSTD:3` with arrow
- A file using ZSTD (unknown level — pre-U04 behavior, or from a writer that omits it) shows `ZSTD → ZSTD:3` with arrow

---

## V-U1 — Validation gate: frontend fixes (U01, U02)

1. Start `npm run dev:nowasm` in `web/`
2. Drop the NYC Taxi parquet file
3. Verify each card has a size label (compressed bytes) in the header
4. Verify columns > 10% of file show amber-colored size text
5. Verify the left border is amber for columns where encoding changes, gray+dimmed for no-change columns
6. Verify no JS console errors
7. Verify "Changed only" checkbox still filters correctly

---

## V-U2 — Validation gate: post-WASM-rebuild (U03, U04, U05)

1. `cargo check --features python` — passes
2. `cargo check --target wasm32-unknown-unknown --features wasm --no-default-features` — passes
3. `cargo test` — passes (update insta snapshots with `cargo insta review` if needed)
4. Rebuild WASM: `cd web && wasm-pack build .. --target web --out-dir web/pkg --release --features wasm --no-default-features`
5. Drop NYC Taxi file in browser
6. Verify string columns with existing RLE_DICTIONARY show 1 star
7. Verify timestamp/ID columns changing from PLAIN to DELTA show 3–5 stars
8. Verify current codec shows `ZSTD:3` (not `ZSTD`) when file uses level 3
9. Verify no arrow shown on codec when current and recommended are both `ZSTD:3`
