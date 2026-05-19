# CONFLICTS & OPEN QUESTIONS — Spec 007

## Conflicts with actual code

### C01 — Left-border accent pattern is currently the only status signal for columns without diagnostics

**Location:** [web/src/render/columns.js:~195](web/src/render/columns.js#L195) — `borderClass` computation in `buildColumnCard`:

```js
const borderClass = encChanged
  ? 'border-l-4 border-l-amber-500'
  : codecChanged
    ? 'border-l-4 border-l-blue-700'
    : 'border-l-4 border-l-gray-700 opacity-70';
```

**Issue:** T18 removes this. The PLAN assumes the replacement signal is the `DIAG_STATUS_META` header pill, which is only populated when `report.diagnostics` is present. Cached reports from before Spec 006 may not have `diagnostics`, so those cards would render with no status signal at all after the left-border is gone.

**Resolution options:**
- (A) Keep a minimal fallback in `buildColumnCard`: if no diagnostic is available, fall back to the old encChanged/codecChanged detection to drive a header dot (not a left-border).
- (B) Treat this as out of scope — by the time this ships, reports will always have diagnostics (they're produced fresh every session via WASM). Cached reports from old sessions are a non-issue.

**Recommendation:** (B). Reports are transient and re-analyzed on every file drop. No cached pre-Spec-006 reports exist in real use.

**Decision needed.**

---

### C02 — The Summary's hardcoded `"ZSTD:3"` badge is in an HTML template literal, not a variable

**Location:** [web/src/render/summary.js:~74](web/src/render/summary.js#L74) — inside the template string:

```html
<span class="...">ZSTD:3</span>
```

**Issue:** T13 says "replace the hardcoded string with the file-wide recommended codec." The fix is straightforward but worth noting: the value is interpolated into a template literal, not read from a `const`. The PLAN compute-then-interpolate flow works cleanly.

**Resolution:** T13 adds the `modeCodec` helper; inject `${fileWideCodec}` in place of the literal `ZSTD:3`. No structural changes.

---

### C03 — Tabulator has no official "bootstrap5-light" theme — file name check

**Location:** [web/src/style.css:2](web/src/style.css#L2) — `@import "tabulator-tables/dist/css/tabulator_midnight.min.css"`

**Issue:** The PLAN (T05) proposes `tabulator_simple.min.css`. Need to verify this file exists in the installed `tabulator-tables` package version.

**Resolution:** Run `ls node_modules/tabulator-tables/dist/css/` during T05 to confirm the filename. Alternatives: `tabulator.min.css` (default), `tabulator_bootstrap5.min.css`, `tabulator_materialize.min.css`. All are light-ish. Pick the one that renders cleanest with our Primer borders.

**No blocker; confirm at implementation time.**

---

### C04 — Tailwind v4 `@theme` token naming convention

**Issue:** PLAN assumes `--color-canvas-default` in `@theme` auto-generates `bg-canvas-default`, `text-canvas-default`, `border-canvas-default`. Tailwind v4's CSS-first theme system does work this way — the `--color-` prefix is stripped and the rest becomes the class suffix — but the `/30` opacity syntax on border classes needs empirical confirmation.

**Resolution:** Verify during T06 by running `npx vite build` and inspecting the generated CSS for `.bg-canvas-subtle` etc. If a custom color doesn't support alpha, fall back to arbitrary-value syntax: `border-[color-mix(in_oklch,var(--color-accent-emphasis)_30%,transparent)]`.

**No blocker.**

---

### C05 — `@theme` confidence-badge color names become Tailwind utilities with awkward names

**Issue:** `--color-confidence-high-text` generates a class `text-confidence-high-text`, which is verbose and grammatically odd.

**Resolution:** These are only referenced via `var(--color-confidence-*-text)` in [ConfidenceBadge.js](web/src/components/ConfidenceBadge.js), not via Tailwind utility classes. The auto-generated utility classes are unused and harmless. Leave names as-is.

**Optional improvement:** Rename to `--color-conf-high`, `--color-conf-high-bg` etc. for cleaner class names. Out of scope for this spec.

---

## Ambiguities

### A01 — What does "muted" mean for a match card's accordion content?

**Context:** T17 keeps the accordion toggle working on match cards so users can drill into the teaching content. But with `opacity-70` on the card root, the accordion-revealed content inherits the muting — making it slightly hard to read when it's actually being examined.

**Options:**
- (A) `opacity-70` only applies to the header and one-line footer; the accordion content restores to full opacity when open.
- (B) Keep `opacity-70` on the whole card; the `:hover` opacity-1 rescue handles readability.
- (C) Remove the opacity entirely and rely on compact layout + `text-fg-subtle` muting for the "At recommended settings" footer.

**Recommendation:** (A). Match-card accordion content is there on purpose when the user clicks; make it fully legible. Implement with a compound selector like:

```css
.card-muted > *:not(.accordion-content) { opacity: 0.7; }
.card-muted > *:not(.accordion-content):hover { opacity: 1; }
.card-muted > .accordion-content.open { opacity: 1; }
```

**Decision needed.**

---

### A02 — Chart-view bar colors on light canvas

**Context:** T08 says switch chart bars to `#bf8700` (amber) / `#0969da` (blue) / `#afb8c1` (gray). On the dark canvas these were bright saturated fills (`bg-amber-500`, `bg-indigo-500`, `bg-gray-600`). The GitHub equivalents are somewhat darker/more muted to work on white.

**Decision needed:** Confirm this palette choice. Alternative is to use GitHub's data-viz scale (teal, purple, coral, etc.) but those don't map to the "encoding change" / "codec-only change" / "no change" semantics already in use.

**Recommendation:** Stay with the proposed three colors — they're the same hue family as our semantic pills, just at light-mode emphasis levels.

---

### A03 — Is there a hover state for non-match cards?

**Context:** Current card hover uses `hover:bg-gray-800/50`. On light canvas, the equivalent subtle-darkening is `hover:bg-canvas-inset`.

**Recommendation:** `hover:bg-canvas-inset` for both match (when opacity-1 on hover) and non-match cards. Adds responsiveness without fighting the Primer aesthetic.

---

### A04 — Confidence badge position when Medium/Low (F03)

**Context:** T15 moves ConfidenceBadge out of the left cluster and hides it when High. When Medium/Low, where exactly does it sit?

**Options:**
- (A) Right-aligned inside the header, immediately before the size span — grouped with other metadata.
- (B) Below the header, on its own line — more prominent since Low confidence warrants attention.
- (C) Appended to the diff block in the accordion — hidden by default.

**Recommendation:** (A). Right-aligned next to the type badge. Medium/Low confidence is a signal but rarely the reason to take action; keeping it in metadata row is correct priority.

---

## Open Questions

### Q01 — Should the "Bench this column" button (from Spec 005) get a new light-mode styling?

**Context:** [web/src/render/columns.js](web/src/render/columns.js) has a bench button in the accordion. It currently uses `bg-gray-700 hover:bg-gray-600 text-gray-300`. This needs updating to Primer button style.

**Resolution:** Out of explicit scope but natural to address during T08. Use the default Primer button style: `bg-canvas-subtle text-fg-default border border-border-default rounded-md hover:bg-canvas-inset`.

**No decision needed; absorb into T08.**

---

### Q02 — Does the snippet panel need a copy-button restyle?

**Context:** [SnippetPanel.js](web/src/components/SnippetPanel.js) has a "Copy" button. Current styling likely uses dark palette.

**Resolution:** Addressed by T12. No separate task.

---

### Q03 — Do we need to add a dark-mode toggle now, or defer?

**Context:** PRD F06 explicitly defers dark mode.

**Resolution:** Defer. Tokens named with Primer conventions so a future `[data-theme="dark"]` override is a straightforward follow-on.

**No decision needed.**

---

## Scope summary

**Files touched (count):**
- Rust: 1 (`src/tuner.rs`) — T01
- CSS: 1 (`web/src/style.css`) — T03, T04, T05, T16
- HTML: 1 (`web/index.html`) — T06
- JS: 10 (`render/*.js` × 5, `components/*.js` × 5) — T07–T12, T13, T14, T15, T17, T18

**Approximate class-name substitutions:** ~180 across 8 files (grep count).

**Estimated effort:** Medium spec. F01 ships in under an hour. F06 (Phase 2–3) is the long tail — roughly half a day of careful substitution + visual verification. F02–F04 (Phase 4) is another 2–3 hours.
