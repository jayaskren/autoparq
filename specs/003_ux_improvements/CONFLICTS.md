# autoparq UX Improvements — Conflicts and Ambiguities

Identified by cross-referencing PRD.md, PLAN.md, TASKS.md, and the actual source files.

---

## C01 — PRD has wrong Rust code for ZSTD match arm

**Severity: High** — PRD's implementation code will not compile.

PRD Fix 2 (line ~109) shows:
```rust
Compression::ZSTD(Some(lvl)) => format!("ZSTD:{}", lvl.compression_level()),
Compression::ZSTD(None) => "ZSTD".to_string(),
```

This treats the ZSTD level as `Option<ZstdLevel>`. But in parquet 55.2.0 the variant is defined as `ZSTD(ZstdLevel)` — the level is always present, not optional. The PRD match arms would fail to compile.

PLAN.md correctly identifies this: "In parquet 55, `Compression::ZSTD(ZstdLevel)` — the level is NOT optional, always present."

**Resolution:** PRD Fix 2 code is wrong. Use the PLAN version:
```rust
Compression::ZSTD(level) => format!("ZSTD:{}", level.compression_level()),
```
No `Some`/`None` branching needed.

---

## C02 — PRD describes a "level unknown" UI case that cannot occur after U04

**Severity: Low** — Spurious complexity in spec.

PRD Fix 2 (line ~136) describes a third case: when the current codec is `"ZSTD"` (no level), show `"ZSTD → ZSTD:3"` with a note `"(level unknown)"`.

After U04 is applied, `format_compression()` will always return `"ZSTD:N"` because `Compression::ZSTD(ZstdLevel)` is never optional. There is no code path that produces a levelless `"ZSTD"` string from the metadata parser post-U04.

The only way `"ZSTD"` without a level could appear is from old report JSON cached before the WASM rebuild, which is not a scenario to handle.

**Resolution:** Drop the "(level unknown)" display case from PRD and U05. The two cases are:
1. `currentCodec == recCodecFull` → single pill, no arrow
2. `currentCodec != recCodecFull` → `current → recommended` with arrow

---

## C03 — Opacity value for no-change cards: 0.7 (PLAN) vs 0.6 (TASKS)

**Severity: Low** — Causes a visual inconsistency between spec and implementation.

PLAN says `opacity-70`; TASKS says `opacity-60`.

**Resolution needed:** Choose one. Recommendation: `opacity-60` (TASKS wins, as the implementation document). Update PLAN to match.

---

## C04 — Codec comparison already broken in current card view

**Severity: High** — Affects current production code regardless of U04.

In `web/src/render/columns.js`, the `codecChanged` comparison is:
```js
const codecChanged = currentCodec !== '—' && currentCodec !== col.recommended_codec;
```

`col.recommended_codec` is the base codec name (`"ZSTD"`) without the level. `recCodec` (built two lines earlier) correctly appends the level (`"ZSTD:3"`). The comparison uses the wrong variable — it will falsely report `codecChanged = true` whenever the recommended codec has a level, even if the current and recommended codec are identical.

This bug is latent now (because `format_compression()` currently discards levels too, so both sides are `"ZSTD"`). After U04, the bug becomes active.

**Resolution:** Fix the comparison in U05 to use `recCodecFull`:
```js
const recCodecFull = col.recommended_codec_level != null
  ? `${col.recommended_codec}:${col.recommended_codec_level}`
  : col.recommended_codec;
const codecChanged = currentCodec !== '—' && currentCodec !== recCodecFull;
```
U05 should be done alongside U01/U02 (not deferred to post-WASM-rebuild) since the comparison logic is broken now. The fact that it's not visible yet is an accident of U04 not existing yet.

---

## C05 — Same codec comparison bug in Table view not explicitly covered by U05

**Severity: Medium** — U05 fixes the card but may not fix the table.

The Table view `initTable()` in `columns.js` has an identical bug:
```js
const changed = cur && cur !== cell.getValue();
// cell.getValue() is col.recommended_codec (base name, no level)
```

TASKS U05 says "update the display strings to use `recCodecFull` consistently in the before/after pill and in the Table view columns" but doesn't call out the comparison fix explicitly — it could be read as only updating display strings, not the `changed` boolean.

**Resolution:** U05 acceptance criteria should explicitly include: "Table view 'Recommended codec' cell shows no highlight when `currentCodec == recCodecFull`." Fix both the `changed` boolean and display string in `initTable()`.

---

## C06 — Blue border color: `border-l-blue-700` (PLAN) vs `border-l-blue-600` (TASKS)

**Severity: Low** — Minor visual inconsistency.

**Resolution needed:** Choose one. Recommendation: `border-l-blue-600` (TASKS wins as the implementation document; 600 is more visible against the dark card background than 700). Update PLAN to match.

---

## C07 — Middle-size column color: `gray-300` (PRD) vs `gray-400` (PLAN and TASKS)

**Severity: Low** — Minor visual inconsistency.

PRD says `text-gray-300` for columns 2–10% of file size. PLAN and TASKS both say `text-gray-400`.

**Resolution:** `text-gray-400` (PLAN and TASKS win; gray-300 against a dark background is nearly white and would look more "important" than intended for mid-tier columns). Update PRD to match.

---

## C08 — "Changed only" filter logic is inconsistent with U02 border logic

**Severity: Medium** — After U01/U02, the filter and the border system disagree on what "changed" means.

Current filter in `columns.js`:
```js
cols = cols.filter(c => c.recommended_encoding !== 'PLAIN' || c.recommended_codec !== report.current_codec);
```

This checks `recommended_encoding !== 'PLAIN'` (not "encoding differs from current") and compares codec at the file level rather than the per-column level. A column where the encoding is already DELTA (no change needed) but `recommended_encoding` is `"DELTA_BINARY_PACKED"` (not PLAIN) would be shown. A column with only a codec level change would be hidden (because `recommended_codec` base matches `current_codec`).

U02 defines "changed" as `encChanged || codecChanged` — the per-column computed values. The filter should use the same definition.

**Resolution:** Update the "Changed only" filter to use the same `encChanged`/`codecChanged` logic computed in `buildColumnCard()`. This requires either extracting those booleans to the data layer or recomputing them in `getFilteredSortedColumns()`. Suggested approach — compute in the filter:

```js
if (changedOnly) {
  cols = cols.filter(col => {
    const curEnc = currentEncodingMap[col.column_name] ?? '—';
    const curCodec = currentCodecMap[col.column_name] ?? report.current_codec ?? '—';
    const recCodecFull = col.recommended_codec_level != null
      ? `${col.recommended_codec}:${col.recommended_codec_level}`
      : col.recommended_codec;
    return curEnc !== col.recommended_encoding || curCodec !== recCodecFull;
  });
}
```

---

## C09 — U05 sequencing should move earlier

**Severity: Medium** — The task ordering in TASKS.md creates an unnecessary window where C04 bug is live.

TASKS.md shows U05 after the WASM rebuild (deferred because "depends on U04"). But the codec comparison bug (C04) exists now. U05 is a pure JavaScript fix. Moving it to the frontend track alongside U01/U02 is safe and avoids introducing the active bug after U04.

**Resolution:** Move U05 to Track B (frontend, no rebuild). New order:

```
U01 + U02 + U05  (JS only, hot-reload)  →  V-U1
U03 + U04        (Rust, one rebuild)    →  V-U2
```

The post-U04 acceptance criteria for U05 (verifying `ZSTD:3 → ZSTD:3` shows no arrow) are still tested in V-U2.

---

## Summary Table

| ID | Severity | Status |
|----|----------|--------|
| C01 | High | PRD code wrong; use PLAN version (no `Option<>`) |
| C02 | Low | Drop "level unknown" case — cannot occur after U04 |
| C03 | Low | opacity-70 (PLAN wins; update TASKS) ✅ Resolved |
| C04 | High | Fix codec comparison bug in U05; move U05 to Track B |
| C05 | Medium | U05 must explicitly fix Table view comparison too |
| C06 | Low | border-l-blue-700 (PLAN wins; update TASKS) ✅ Resolved |
| C07 | Low | gray-400 (PLAN/TASKS win over PRD gray-300) |
| C08 | Medium | "Changed only" filter must use per-column encChanged/codecChanged |
| C09 | Medium | Move U05 to Track B (alongside U01/U02) |
