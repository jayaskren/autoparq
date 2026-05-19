# CONFLICTS & OPEN QUESTIONS — Spec 006

## Decisions log

All open questions below have been resolved (2026-04-22):

- **C01 → (A) accept.** Size-reduction calculation naturally updates when `PLAIN_DICTIONARY` stops being aliased; this is more correct semantically.
- **C02 → OK.** Snapshot diffs will be reviewed during implementation.
- **C03 → OK.** Codec+level are joined before comparison inside `diagnose_column`.
- **A01 → list all relevant encodings.** Replaces single-primary selection with a full data-encoding set (filtering out `RLE` / `BIT_PACKED` def/rep encodings) + an optional per-RG variance summary string. PLAN Step 2.2 and TASK T06 updated.
- **A02 → fixed thresholds for now.** DELTA ineffective below 1.5×, BSS ineffective below 1.1×. Revisit if false positives show up.
- **A03 → UNCOMPRESSED is always Mismatch.** No special-case downgrade. Users with intentional no-compression setups can ignore the diagnostic.
- **A04 → break out fallback / weak / mismatch separately.** PRD D06, PLAN Step 3.3, TASK T12 updated to show the per-status count.

---


## Conflicts with actual code

### C01 — Splitting `PLAIN_DICTIONARY` from `RLE_DICTIONARY` changes existing heuristic behaviour

**Location:** [src/tuner.rs:368–371](src/tuner.rs#L368-L371)

```rust
let currently_plain = meta.map_or(false, |m| {
    m.encodings.iter().any(|e| e == "PLAIN")
        && !m.encodings.iter().any(|e| e == "RLE_DICTIONARY" || e == "DELTA_BINARY_PACKED")
});
```

**Issue:** This check classifies a column as "currently plain" when the metadata shows `PLAIN` but not `RLE_DICTIONARY`/`DELTA_BINARY_PACKED`. Today, `PLAIN_DICTIONARY` is aliased to `"RLE_DICTIONARY"` by `format_encoding`, so a column with a dictionary page + PLAIN-fallback data pages has `encodings = ["PLAIN", "RLE_DICTIONARY"]` and is NOT classified as currently plain.

After T01 (splitting the alias), the same column will have `encodings = ["PLAIN", "PLAIN_DICTIONARY"]`, which IS classified as currently plain. This flips the predicted size reduction for all columns in the fallback state.

**Implication:** The summary's size-reduction range will change for files that contain mid-chunk dictionary fallbacks. This is arguably *more correct* semantically (a column whose data pages fell back to PLAIN IS effectively PLAIN), but it's a behaviour change.

**Resolution options:**
- (A) Accept the change — it's more correct. Update the `predicted_size_reduction_pct` calculation to treat `PLAIN_DICTIONARY` the same way, which is what the new logic naturally does.
- (B) Extend the check to also reject `"PLAIN_DICTIONARY"`: `!m.encodings.iter().any(|e| matches!(e.as_str(), "RLE_DICTIONARY" | "DELTA_BINARY_PACKED" | "PLAIN_DICTIONARY"))` — preserves existing behaviour.

**Recommendation:** (A). The current collapsing was hiding real information. Bonus: users will see a changed size-reduction estimate on files with fallbacks, which reinforces the diagnostic we're adding.

**Decision needed from user.**

---

### C02 — Mock report and tests reference `"RLE_DICTIONARY"` as current encoding

**Locations:**
- [web/src/mock-report.js](web/src/mock-report.js) — used for dev UI without a real file
- [tests/integration/snapshots/integration__test_tune__low_cardinality_strings_tune.snap](tests/integration/snapshots/integration__test_tune__low_cardinality_strings_tune.snap) — snapshot fixture

**Issue:** Snapshot tests will break if they include the `encodings` field with the old collapsed value. The insta snapshots need review post-change.

**Resolution:** After T01, run `cargo insta test` and review diffs. Update mock-report.js if it's used by the dev build path.

---

### C03 — `recommended_codec` is a string without level; level is a separate field

**Location:** [src/tuner.rs:54-56](src/tuner.rs#L54-L56)

```rust
pub recommended_encoding: String,
pub recommended_codec: String,
pub recommended_codec_level: Option<i32>,
```

**Issue:** The diagnostic compares "current codec" (in metadata) against "recommended codec." Current codec is stored in `ColumnMetaSummary.codec` as `"ZSTD:3"` (level embedded). Recommended is stored as `"ZSTD"` with level separate.

**Resolution:** In `diagnose_column`, construct the full recommended codec string as `format!("{}:{}", rec.recommended_codec, level)` when level is Some. This is the same pattern the JS UI already uses via `getRecCodecFull()`.

**No blocker; add to T07 implementation notes.**

---

## Ambiguities

### A01 — What counts as "primary encoding" for display?

**Context:** A single column chunk can contain multiple encodings simultaneously: `RLE` (for the definition/repetition levels), `PLAIN_DICTIONARY` (for the data pages that fell back), `RLE_DICTIONARY` (for data pages that reference the dictionary), etc. The raw list includes all of them.

**Recommendation:** Filter out `RLE` (definition/rep levels) and `BIT_PACKED` (also def/rep). Apply the precedence rule from PLAN Step 2.2 to the rest. Confirm before implementing.

**Decision needed.**

---

### A02 — Effectiveness thresholds (R4/R5) — are 1.5× and 1.1× the right numbers?

**Context:** PLAN suggests:
- R4 (DELTA ineffective): compression_ratio < 1.5
- R5 (BSS ineffective): compression_ratio < 1.1

These are picked to match the recommender's implicit thresholds, but they aren't explicitly defined there. A BSS column at 1.05× is clearly ineffective; at 1.08× it's borderline.

**Options:**
- (A) Fixed thresholds as proposed. Simple, predictable.
- (B) Compare to "what PLAIN would have achieved" — requires scan (too expensive).
- (C) Use compression ratio of uncompressed_bytes / compressed_bytes AND compare to a codec-implied floor (e.g., ZSTD on PLAIN bytes would achieve ~1.3× on mildly compressible data, so anything below 1.3 for DELTA+ZSTD is suspicious).

**Recommendation:** (A) for now. Ship with explicit supporting metric (`monotonicity_score` or ratio) so users can judge. Revisit if false positives are common.

**Decision needed.**

---

### A03 — Diagnostic for files with no file-level compression

**Context:** If the file uses `UNCOMPRESSED` as its codec, comparing current vs recommended codec is trivially a mismatch. Should this always show as `Mismatch`, or should UNCOMPRESSED be treated specially (e.g., if the file is intentionally uncompressed for speed)?

**Recommendation:** Treat it as `Mismatch` with observation "File is uncompressed." If entropy is high (we already compute this), add supporting metric "byte_entropy=7.8 — data is already compressed-like; uncompressed is reasonable" and downgrade to `Match` (special case).

**Decision needed.**

---

### A04 — File health count: how to render multi-status summary

**Context:** A file with 14 columns might have: 10 Match, 2 FallbackDictionary, 1 IneffectiveEncoding, 1 Mismatch. PLAN proposes:

```
File health: 12 of 14 columns match recommendation
             1 fallback detected, 1 mismatch
```

This hides "ineffective" in the "1 mismatch" count. Alternative:

```
File health: 10 of 14 match
             2 fallbacks, 1 weak, 1 mismatch
```

**Recommendation:** Second form (break out weak/ineffective). More informative; same space.

**Decision needed.**

---

## Open Questions

### Q01 — Does the `ColumnMetaSummary` struct need a stable version identifier for localStorage-cached reports?

**Context:** T02 adds `#[serde(default)]` to new fields, which means old cached reports deserialise cleanly with empty vecs. But empty vecs produce empty diagnostics, so users with old cached reports will see no diagnostics until they reprocess.

**Resolution:** This is acceptable. Files are re-analysed every session in practice (the app requires a file drop). No schema version needed. Document the behaviour.

---

### Q02 — Should the diagnostic section in the accordion be collapsible?

**Context:** For Match status, PLAN says render as a single line. For non-Match, render a multi-line block. Should the non-Match case also be collapsible with the details hidden by default?

**Recommendation:** No. The whole point of this feature is to surface the diagnostic. Hiding it behind another click defeats it. Keep it always-visible when non-Match, single-line when Match.

---

### Q03 — What happens when a file has no diagnostics (e.g., empty file, profiling failed)?

**Context:** If `report.diagnostics` is missing or empty, the Summary stat breaks ("0 of 0 columns match"), the card pill is omitted, and the accordion section is skipped.

**Recommendation:** Handle all three as graceful-degradation: skip the Summary row, skip the pill, skip the accordion section. No user-visible error.

---

### Q04 — Does the "File health" row click-to-filter behaviour conflict with the Analysis Confidence row?

**Context:** Currently the Summary section has no clickable rows. Adding one to File health sets a UX precedent. Should Analysis Confidence also be clickable?

**Recommendation:** Out of scope. Only File health gets the click-to-filter affordance in this spec. Add a subtle hover cue (text colour change) to signal interactivity.
