# TASKS — Scannable Columns & GitHub-Inspired Refresh (Spec 007)

## Phase 1 — Rust impact-star fix (F01)

### T01 — Rewrite `compute_impact_stars` for codec-only size weighting
**File:** [src/tuner.rs:318–353](src/tuner.rs#L318-L353)
**Change:** Replace the flat `if enc_already_set { return 2; }` branch with a size-share-weighted scale (max 3). See PLAN Step 1.1 for the full function body.
**Test:** Add a unit test asserting:
- enc_already_set + codec_already_set → 1
- enc_already_set + codec differs, share < 1% → 1
- enc_already_set + codec differs, share ≥ 10% → 3
- enc changes, share < 1% → 2 (unchanged from existing behaviour)
**Effort:** Small (~20 lines).

### T02 — Rebuild WASM
**Command:** `cd web && npm run wasm:build`
**Effort:** 2–3 min.

---

## Phase 2 — F06 foundation

### T03 — Replace `@theme` block in `style.css` with Primer tokens
**File:** [web/src/style.css](web/src/style.css)
**Change:** Replace the current `@theme { … }` block with the Primer token set from PLAN Step 2.1. Add `--font-sans` and `--font-mono`. Keep the confidence-badge tokens but swap their values to light-mode equivalents (`#1a7f37` / `#dafbe1`, etc.).
**Effort:** Small.

### T04 — Update `body` and base rules in `style.css`
**File:** [web/src/style.css](web/src/style.css)
**Change:** Add a `body` rule setting `background`, `color`, and `font-family` to the new tokens. Set `html { font-size: 14px; }` to match GitHub's base size. Remove the dark Tabulator color overrides (we'll rewrite after swapping the theme import).
**Effort:** Small.

### T05 — Swap Tabulator theme import from midnight to simple
**File:** [web/src/style.css](web/src/style.css)
**Change:** Replace `@import "tabulator-tables/dist/css/tabulator_midnight.min.css";` with `@import "tabulator-tables/dist/css/tabulator_simple.min.css";`. Add new override rules using Primer tokens (`border-border-default` grid, `bg-canvas-subtle` header row, `text-fg-default` content).
**Test:** Open the Table view on a loaded parquet file; rows/headers should look clean on white background.
**Effort:** Small.

### T06 — Update `index.html` page shell to light palette
**File:** [web/index.html](web/index.html)
**Change:** Replace every dark Tailwind color class. Common substitutions:
- `bg-gray-950` / `bg-gray-900` → `bg-canvas-default` or `bg-canvas-subtle`
- `bg-gray-800` → `bg-canvas-subtle` / `bg-canvas-inset`
- `text-gray-100` / `text-gray-200` → `text-fg-default`
- `text-gray-400` / `text-gray-500` → `text-fg-muted`
- `text-gray-600` → `text-fg-subtle`
- `border-gray-800` / `border-gray-700` → `border-border-default`
- `text-indigo-300` → `text-accent-fg`
- `bg-indigo-600` → `bg-accent-emphasis`
**Test:** `npx vite build` succeeds; page shell renders on white.
**Effort:** Medium (~30–50 replacements).

---

## Phase 3 — F06 components

### T07 — Restyle `render/summary.js`
**File:** [web/src/render/summary.js](web/src/render/summary.js)
**Change:** Substitute dark classes per the mapping in T06. The "Recommended codec" badge should use `bg-accent-subtle text-accent-fg border-accent-emphasis/30`.
**Effort:** Small (~15 replacements).

### T08 — Restyle `render/columns.js`
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:** Largest JS file. Replace dark classes throughout. Remove the current `border-l-4 border-l-amber-500` pattern in card wrappers (F04 will replace the signal with header icons). Update `DIAG_STATUS_META` pill styles to Primer:
- Match: `bg-success-subtle text-success-fg border border-success-emphasis/30`
- FallbackDictionary: `bg-attention-subtle text-attention-fg border border-attention-emphasis/30`
- IneffectiveEncoding / Mismatch: `bg-neutral-subtle text-neutral-fg border border-border-default`
Update the chart-view bar colors to `#bf8700` (amber), `#0969da` (blue), `#afb8c1` (gray).
**Effort:** Medium (~60 replacements).

### T09 — Restyle `render/codec-cards.js`
**File:** [web/src/render/codec-cards.js](web/src/render/codec-cards.js)
**Change:** Dark → light substitutions. Primary "selected" card uses `border-accent-emphasis ring-1 ring-accent-emphasis/30 bg-accent-subtle`.
**Effort:** Small.

### T10 — Restyle `render/advisories.js` and `render/caveats.js`
**Files:** [web/src/render/advisories.js](web/src/render/advisories.js), [web/src/render/caveats.js](web/src/render/caveats.js)
**Change:** Warning/info callouts use Primer callout pattern: `bg-attention-subtle text-attention-fg border border-attention-emphasis/30 rounded-md p-3` for warnings; `bg-accent-subtle text-accent-fg border border-accent-emphasis/30` for info.
**Effort:** Small.

### T11 — Restyle `render/report.js` page chrome
**File:** [web/src/render/report.js](web/src/render/report.js)
**Change:** Header border, nav rail, section backgrounds. Nav rail active link: `bg-accent-subtle text-accent-fg`.
**Effort:** Small.

### T12 — Update components to Primer palette
**Files:**
- [web/src/components/ConfidenceBadge.js](web/src/components/ConfidenceBadge.js) — already references `var(--color-confidence-*-*)`, updates auto-apply from T03.
- [web/src/components/ImpactStars.js](web/src/components/ImpactStars.js) — change filled-star color from `#facc15` to `#bf8700`; empty-star color from `#374151` to `#afb8c1`.
- [web/src/components/SnippetPanel.js](web/src/components/SnippetPanel.js) — code block `bg-canvas-inset text-fg-default border-border-muted`.
- [web/src/components/CodeBlock.js](web/src/components/CodeBlock.js) — same as SnippetPanel.
- [web/src/components/Tooltip.js](web/src/components/Tooltip.js) — `bg-canvas-inset text-fg-default border-border-default`.
**Effort:** Small.

---

## Phase 4 — F02/F03/F04 layout & scannability

### T13 — F02a: Summary codec badge reads from report
**File:** [web/src/render/summary.js](web/src/render/summary.js)
**Change:** Replace the hardcoded `"ZSTD:3"` string in the recommended-codec badge with the file-wide codec computed from `report.columns`. Use the mode of `(recommended_codec, recommended_codec_level)`:

```js
function modeCodec(cols) {
  const counts = new Map();
  for (const c of cols) {
    const full = c.recommended_codec_level != null
      ? `${c.recommended_codec}:${c.recommended_codec_level}`
      : c.recommended_codec;
    counts.set(full, (counts.get(full) ?? 0) + 1);
  }
  let best = null, bestCount = -1;
  for (const [codec, count] of counts) {
    if (count > bestCount) { best = codec; bestCount = count; }
  }
  return best ?? 'ZSTD:3';
}
```

Append a muted footnote under the badge: "Applied file-wide. Columns flagged below use a different codec."
**Effort:** Small (~15 lines).

### T14 — F02b: Column cards omit codec when file-wide
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:** In `renderColumns`, after building the columns list, compute `fileWideCodec` using the same `modeCodec` helper (extract to a shared util, e.g., `web/src/lib/codec.js`).

In `buildColumnCard`:
- Compute `codecDiffers = getRecCodecFull(col) !== fileWideCodec`.
- If `!codecDiffers`:
  - Omit the codec pill from the `Current` and `Recommended` columns of the compare row.
  - In the accordion diagnostic block, omit the `+ {codec}` suffix on the Have/Recommend lines.
- If `codecDiffers`:
  - Keep both codec rows visible.
  - Append a one-line caveat below the compare row: "codec differs from file default — typically for pre-compressed data."

**Effort:** Medium (~40 lines).

### T15 — F03: Reposition ConfidenceBadge; hide when High
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:** In `buildColumnCard`, render the `ConfidenceBadge` only if `col.confidence !== 'High'`. Move it out of the `nameSpan/stars/badge` cluster; append it to the right-hand side of the header, after the type badge but before the size span (or repurpose a new dedicated slot).
**Effort:** Small (~10 lines).

### T16 — F04: `.card-muted` helper class
**File:** [web/src/style.css](web/src/style.css)
**Change:** Add:
```css
.card-muted { opacity: 0.7; transition: opacity 0.15s ease; }
.card-muted:hover { opacity: 1; }
```
**Effort:** Trivial.

### T17 — F04: Compact layout for Match cards
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:** In `buildColumnCard`, when `diagnostic?.status === 'Match'`:
- Add `card-muted` to the card root element's class list.
- Skip appending `compareRow` and `reasonRow`; instead append:
  ```html
  <div class="px-4 pb-3 pt-1 text-xs text-fg-subtle">At recommended settings.</div>
  ```
- Keep the accordion content populated (teaching section, raw stats, etc.) so users can still drill in.
- Keep the accordion toggle (click on header still opens).

For non-match cards, render the existing compare row and reason row unchanged.
**Effort:** Medium (~25 lines).

### T18 — Remove obsolete left-border accents from card
**File:** [web/src/render/columns.js](web/src/render/columns.js)
**Change:** In `buildColumnCard`, delete the `borderClass` computation and the `border-l-4 border-l-*` classes. The status signal now comes from the header pill (existing) plus F04's compact layout.
**Effort:** Trivial (~5 lines removed).

### T19 — F05: Verify filter & Summary click still work
**No code change.** Manual test after T17:
1. Drop a parquet file.
2. Toggle "Non-matching only" — only non-match cards should appear.
3. Click "File health: N of M match" in the Summary — same filter engages.
4. Confirm Chart view bars use the new palette.
**Effort:** 5 min.

---

## Phase 5 — Verification

### T20 — Rust tests
**Command:** `cargo build && cargo test --lib`
**Effort:** 30 s.

### T21 — Vite build
**Command:** `cd web && npx vite build`
**Effort:** 10 s. Confirm no "unknown utility" warnings from unrecognized Tailwind classes.

### T22 — Manual walkthrough on a real file
Drop the NYC taxi parquet used for prior testing. Verify each acceptance criterion from the PRD:
- [ ] VendorID shows ★ (1 star), not ★★.
- [ ] Codec pill in Summary reads the correct file-wide default (no hardcoded string).
- [ ] Most column cards don't show a codec row.
- [ ] Columns flagged as Match are muted/compact.
- [ ] HIGH confidence pill is absent from High-confidence columns.
- [ ] Page reads as GitHub-light.
**Effort:** 5 min.

### T23 — Contrast audit
Use Chrome DevTools accessibility pane. Check at least: body text on canvas, muted labels on subtle cards, pill text on pill backgrounds, link text. All must meet WCAG AA.
**Effort:** 10 min.

---

## Task ordering

```
T01 → T02
         ↓
T03 → T04 → T05 → T06
                     ↓
    (T07, T08, T09, T10, T11, T12 — parallelisable)
                     ↓
T13 → T14 → T15 → T16 → T17 → T18
                               ↓
                              T19 → T20 → T21 → T22 → T23
```

T01–T02 (F01) ship independently of everything else. T03–T06 must complete before any component restyle (T07–T12). T13–T18 depend on the Primer tokens being in place. T19 is manual verification. T20–T23 are final QA.
