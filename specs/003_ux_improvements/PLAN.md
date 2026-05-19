# autoparq UX Improvements — Implementation Plan

## Overview

Four targeted fixes across two layers:

- **Rust layer (requires WASM rebuild):** Fix 1 (impact stars), Fix 2 (ZSTD level display)
- **Frontend layer (hot-reload, no rebuild):** Fix 3 (column sizes), Fix 4 (status indicator)

Do the JS fixes first — they show up immediately in the running dev server. Do the Rust fixes together in one WASM rebuild.

---

## Track A — Rust fixes (one WASM rebuild)

### A1: Impact stars — `src/tuner.rs`

**Current:** `compute_impact_stars(enc, codec, meta)` measures how strongly a rule fired, not how much improvement applying the recommendation delivers. Result: columns already using RLE_DICTIONARY score 5 stars; columns that need encoding changes score 3–4.

**Approach:**

Add two parameters: `file_total_uncompressed_bytes: i64` (for size-weighting) and pass the full `meta: &ColumnMetaSummary` (already present) so we can check whether the recommended encoding already appears in `meta.encodings`.

New logic in priority order:

1. If recommended encoding already appears in `meta.encodings` **and** recommended codec base equals current codec base → **1 star** (no meaningful change)
2. If recommended encoding already appears in `meta.encodings` but codec base differs → **2 stars** (codec-only change)
3. If encoding will change (currently PLAIN or different), weight by column size share:
   - ≥ 10% of file → 5 stars
   - ≥ 4% → 4 stars
   - ≥ 1% → 3 stars
   - < 1% → 2 stars

The `file_total_uncompressed_bytes` is already computed at call sites (e.g. `predicted_size_reduction_pct` already sums `file_profile.columns` uncompressed bytes). Thread it through.

**Call sites** (all must be updated):
- Line 585 (path-based tuner)
- Line 627 (path-based tuner, with progress)
- Line 740 (bytes-based tuner)
- Line 872 (bytes-based tuner, with progress)
- Line 996 (from-profiles tuner)

**Confidence: 95%** — straightforward logic change; no new dependencies.

---

### A2: ZSTD/GZIP/BROTLI level in `format_compression` — `src/profiler/metadata.rs`

**Current:** Line 92 discards compression level: `Compression::ZSTD(_) => "ZSTD".to_string()`

**Approach:**

Use `ZstdLevel::compression_level() -> i32` (confirmed in parquet 55.2.0 at `src/compression.rs:570`). Import `parquet::basic::{ZstdLevel, GzipLevel, BrotliLevel}` at the top of the file (already imported via `parquet::basic::Compression`; the level types are in scope via `parquet::compression`).

```rust
Compression::ZSTD(level) => format!("ZSTD:{}", level.compression_level()),
Compression::GZIP(level) => format!("GZIP:{}", level.compression_level()),
Compression::BROTLI(level) => format!("BROTLI:{}", level.quality()),
```

**Risk (LOW):** The parquet `Compression::ZSTD` variant takes `ZstdLevel` (not `Option<ZstdLevel>`) in parquet 55. Verify the variant shape before matching — the bench.rs usage pattern confirms `Compression::ZSTD(ZstdLevel::try_new(l)?)` which means the level is always present.

**Frontend normalization** (no Rust change): In `web/src/render/columns.js`, the codec comparison currently compares the full current codec string (e.g. `"ZSTD:3"`) against `col.recommended_codec` (e.g. `"ZSTD"`) with `col.recommended_codec_level` (e.g. `3`) separate. Normalize the comparison by building a canonical string for both sides before diffing.

**Confidence: 90%**

---

## Track B — Frontend fixes (no WASM rebuild)

### B1: Column size badge — `web/src/render/columns.js`

**Approach:**

In `renderColumns()`, build a `sizeMap` from `report.file_profile?.columns`:

```js
const totalUncompressed = report.file_profile?.columns
  .reduce((s, c) => s + c.uncompressed_bytes, 0) ?? 0;
const sizeMap = {};
for (const c of report.file_profile?.columns ?? []) {
  sizeMap[c.name] = {
    compressed: c.compressed_bytes,
    share: totalUncompressed > 0 ? c.uncompressed_bytes / totalUncompressed : 0,
  };
}
```

In `buildColumnCard()`, add a size span in the header before the chevron:

```
"1.4 MB"  (colored amber if share > 0.10, gray-400 if 0.02–0.10, gray-600 if < 0.02)
```

Use a `humanBytes(n)` helper (convert to KB/MB/GB).

**Confidence: 98%** — purely additive, no logic changes.

---

### B2: Left-border status indicator — `web/src/render/columns.js`

**Approach:**

In `buildColumnCard()`, compute the card's border class from the already-available `encChanged` and `codecChanged` booleans (built for Fix 3/the existing before-after display):

```js
const borderClass = encChanged
  ? 'border-l-4 border-l-amber-500'
  : codecChanged
    ? 'border-l-4 border-l-blue-700'
    : 'border-l-4 border-l-gray-700 opacity-70';
```

Apply to the card's outer `div` alongside the existing `bg-gray-900 rounded-xl border border-gray-800` classes.

Also dim cards with no changes (`opacity-70`) so the actionable columns visually pop.

**Confidence: 98%**

---

## Sequencing

```
B1 + B2 (parallel, hot-reload) → verify in browser
A1 + A2 (together, one wasm-pack build ~3 min) → rebuild → verify in browser
```

No dependencies between B1 and B2. A1 and A2 are independent Rust changes but share one rebuild.

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `Compression::ZSTD` variant shape differs from expected | Low | Low | Check parquet 55 source before matching; fallback to `"ZSTD"` |
| Size-weighting in impact stars causes unexpected star regressions in tests | Low | Medium | Review `insta` snapshots after change; update expected values |
| `file_profile` missing on some TuneReport paths (e.g. from-profiles) | Low | Low | Null-check in JS with `?? []` guard already planned |
