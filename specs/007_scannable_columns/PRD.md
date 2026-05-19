# PRD — Scannable Columns: Fix Impact, Hoist Codec, Separate Tuned from Untuned (Spec 007)

## Problem

Three distinct issues surfaced while reviewing the Column Recommendations view:

### P1 — Impact stars are misleading

`compute_impact_stars` returns a flat `2` stars for any "codec-level-only" change, regardless of column size ([tuner.rs:336–338](src/tuner.rs#L336-L338)). In a recent NYC taxi file, a 430 KB VendorID column (~0.8% of file size) got the same ★★ rating as a 15 MB timestamp column with the same codec bump. Users sort "High impact first" and end up with trivial-impact columns near the top because the stars don't reflect actual file-level impact.

### P2 — Codec recommendations are file-wide but displayed per column

Parquet's format *allows* per-column codec, but in practice 95%+ of files use a single codec everywhere. Autoparq's recommender selects codec from the file-level priority (`size`/`speed`/`balanced`) plus one narrow per-column signal (`byte_entropy > 7.5` for pre-compressed blobs). For the vast majority of columns the codec recommendation is *identical*, so repeating it on every card is visual noise that makes the real per-column signal — the encoding — harder to scan.

### P3 — "Tuned" and "needs attention" cards look the same

The diagnostic pill (✓ match / ⚠ fallback / ◆ weak / ✎ diff) helps, but a user scrolling a long list of columns can't immediately tell which ones are already at the recommended settings vs. which ones have room for improvement. The HIGH/MEDIUM/LOW confidence pill sitting next to the impact stars also gets conflated with impact (a recent user literally read "HIGH" as "high impact"). Confidence and impact are orthogonal and shouldn't visually compete.

## Goals

1. Impact stars accurately reflect the **file-level effect** of applying a recommendation. A small codec-level bump on a tiny column gets ★, not ★★.
2. Codec is hoisted to the file-level Summary. Per-card codec display appears only when a column's recommended codec *differs* from the file-wide recommendation.
3. The Column Recommendations list makes "already tuned" vs "needs attention" obvious at a glance — even without reading card details.
4. Confidence and impact stop visually competing.

## Non-Goals

- Changing what codec the recommender picks (no rule changes, only presentation).
- Changing the impact-stars formula for encoding changes (those already weight by size share; only codec-only changes need fixing).
- A new section heading / grouping of cards. The sort order already surfaces high-impact columns; we can achieve scannability without re-architecting the list.

---

## Feature Breakdown

### F01 — Size-weighted impact stars for codec-only changes

`compute_impact_stars` in [src/tuner.rs:318–353](src/tuner.rs#L318-L353) currently has three branches:

```rust
if enc_already_set && codec_already_set { return 1; }
if enc_already_set { return 2; }  // ← bug: flat 2 regardless of size
// encoding changes → weight by share, scale 2–5
```

**Change:** Replace the flat `return 2` with a size-weighted scale that caps lower than the encoding-change scale. A codec-level bump (`ZSTD:1` → `ZSTD:3`) yields ~5–15% size improvement, not transformative — even for a big column it should max out at ★★★.

| Column's share of total uncompressed bytes | Encoding changes | Codec-only change |
|--------------------------------------------|------------------|-------------------|
| ≥ 10% | 5 | 3 |
| ≥ 4% | 4 | 2 |
| ≥ 1% | 3 | 1 |
| < 1% | 2 | 1 |

The "no change" case (`enc_already_set && codec_already_set`) still returns `1`.

### F02 — Hoist codec to the Summary; show per-card only when it differs

**Summary changes:**

The Summary already shows a single "Recommended codec" badge ([summary.js](web/src/render/summary.js)). Keep it. Add a small note below it: `"Applied file-wide. Columns that need a different codec are flagged individually."`

Currently the Summary hardcodes `ZSTD:3` as the recommended codec string — fix that to actually read from the report (use the codec that the recommender chose for the majority of columns, or the report's `options.a.codec_description`).

**Per-card changes:**

In [columns.js](web/src/render/columns.js) `buildColumnCard`:

- Compute a file-wide recommended codec (the mode of `getRecCodecFull(col)` across all columns).
- If this column's recommended codec equals the file-wide default:
  - Don't render the "codec" row in the Current/Recommended diff.
  - Don't render a codec pill separately.
- If this column's recommended codec *differs* (rare — typically `UNCOMPRESSED` for high-entropy):
  - Keep the current two-row diff layout with the codec difference clearly called out.
  - Add a caveat line: `"codec differs from file default — typically because byte_entropy is high"`.

Apply the same collapse logic to the "Current vs Recommended" diagnostic block in the accordion.

### F03 — Reposition confidence, collapse where redundant

The confidence pill (HIGH/MEDIUM/LOW) currently sits next to the impact stars, leading to conflation. Two changes:

1. **Move the confidence pill to the right side of the header**, alongside (or into) the type badge cluster — spatially separate from the impact stars.
2. **Omit the confidence pill entirely when `overall_confidence == "High"`** (the common case). Only show confidence when it's `Medium` or `Low`, which are the only cases that should influence user action.

### F04 — Visually differentiate "at recommended settings" cards

The current card has a left-border accent driven by change status:
- Amber border = encoding change
- Blue border = codec-only change
- Gray border (with `opacity-70`) = no change

**Tighten this pattern and make it the primary scannability signal:**

1. **Match cards** (`diagnostic.status === 'Match'`):
   - Compact layout: header only (name + tiny pill + size), no diff rows, no reason line.
   - Reduced visual weight: `opacity-60`, collapsed border, no hover accordion icon (or keep accordion for the "Why this encoding?" teaching content).
   - A small green left-border stripe (`border-l-green-600/40`) — a quiet "this one's fine" affirmation.
   - Footer line: `"At recommended settings."` in muted text.

2. **Non-match cards** (everything else):
   - Full current layout, colored left border by severity (amber for fallback, yellow for mismatch/weak).
   - Full opacity, always visible diff.

3. **Density cue:** when multiple match cards appear in sequence, render them at ~half height so they occupy less vertical space than non-match cards. A user scrolling a 40-column file can then skim past the "already tuned" region and focus on the cards that need attention.

This makes the visual gestalt carry the status message: "Most of this file is fine → here are the 3 columns to think about."

### F05 — Optional toggle: "Hide columns at recommended settings"

To complement the existing "Non-matching only" filter (which already does this but is a checkbox buried in the filter bar):

- Keep the checkbox.
- Consider wiring it to a one-click button in the Summary's "File health" line: already exists as of Spec 006. No change needed here — just confirm it works after F04's visual changes.

### F06 — GitHub-inspired visual refresh

The current design uses near-black (`bg-gray-900` / `bg-gray-800`) backgrounds with heavy indigo accents. It reads as very dark and generic. Shift to a **GitHub Primer**-inspired palette that feels familiar to developers and gets out of the way of the content.

**Default: GitHub-light.** GitHub's default web experience for most users is light, and "reminiscent of the GitHub website" most naturally maps to the light theme. We'll ship this as the default and can add a dark toggle later.

**Primer-style color tokens (CSS variables in `:root`):**

| Role | Light value | Usage |
|------|-------------|-------|
| `--color-canvas-default` | `#ffffff` | Page background |
| `--color-canvas-subtle` | `#f6f8fa` | Card/panel background, code blocks |
| `--color-canvas-inset` | `#eaeef2` | Nested panels, diff backgrounds |
| `--color-border-default` | `#d0d7de` | All 1px borders |
| `--color-border-muted` | `#d8dee4` | Subtle separators |
| `--color-fg-default` | `#1f2328` | Primary text |
| `--color-fg-muted` | `#656d76` | Labels, metadata |
| `--color-fg-subtle` | `#6e7781` | Hints, footnotes |
| `--color-accent-fg` | `#0969da` | Links, active tabs |
| `--color-accent-emphasis` | `#0969da` | Primary button, highlighted pills |
| `--color-success-fg` | `#1a7f37` | ✓ Match pill text |
| `--color-success-subtle` | `#dafbe1` | ✓ Match pill bg |
| `--color-attention-fg` | `#9a6700` | ⚠ Fallback pill text |
| `--color-attention-subtle` | `#fff8c5` | ⚠ Fallback pill bg |
| `--color-severe-fg` | `#bc4c00` | Severe warnings |
| `--color-danger-fg` | `#d1242f` | Errors |
| `--color-neutral-emphasis` | `#656d76` | Secondary pills text |
| `--color-neutral-muted` | `rgba(175,184,193,0.2)` | Secondary pills bg |

**Typography:**
- Body font stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji"`
- Monospace: `ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace`
- Base size: 14px body, 12px small/labels. The current Tailwind `text-sm`/`text-xs` sizing is already close to this.

**Component-level changes:**

1. **Cards** — switch from `bg-gray-900 border-gray-800` to `bg-canvas-subtle border-border-default` with 6px rounded corners and a 1px solid border (not a 4px colored left-border — GitHub uses full 1px borders consistently). Status signal moves from left-border color to:
   - Non-match cards: small colored dot / square icon in the header (amber for fallback, gray for mismatch).
   - Match cards (F04): a green ✓ icon in the header, `bg-canvas-subtle` still, no accent — muted by `opacity-70`.
2. **Pills/badges** — GitHub-style "label" pills: `rounded-full`, `px-2 py-0.5`, `text-xs`, 1px solid border matching the pill's color family. Use `--color-success-subtle` bg + `--color-success-fg` text for match, `--color-attention-subtle` + `--color-attention-fg` for fallback, etc.
3. **Buttons** — Primer button style: `bg-canvas-subtle` with `border-border-default`, `hover:bg-canvas-inset`. Primary buttons use `--color-accent-emphasis` (green like "New PR"? or blue? GitHub uses green `#2da44e` for primary CTAs; we'll stick with GitHub's primary blue for consistency with link color).
4. **Filter bar** — white card with `border-border-default` separator, replacing the current floating inputs.
5. **Impact stars** — keep the yellow star glyph (works on any bg); use `#bf8700` for filled stars in light mode.
6. **Table view** — remove dark-mode table chrome; use `border-border-default` grid lines and `bg-canvas-subtle` header row.
7. **Code blocks / monospace text** — `bg-canvas-subtle` with `border-border-muted`, not the current near-black.
8. **Chart view** — bar colors shift from bright saturated fills to GitHub's data-visualization palette: amber `#bf8700`, blue `#0969da`, gray `#afb8c1`.

**Implementation approach:**

- Define the color tokens as CSS custom properties in `index.html` or `style.css`.
- Replace hardcoded Tailwind color classes (`bg-gray-900`, `text-gray-400`, `text-indigo-300`, etc.) with classes that reference the tokens — either via Tailwind config extension (`bg-canvas-subtle`) or arbitrary values (`bg-[var(--color-canvas-subtle)]`).
- Scope the changes: start with `index.html` page shell, then `summary.js`, `columns.js`, accordion inside cards, `codec-cards.js`, `advisories.js`, `caveats.js`, `SnippetPanel.js`, `ConfidenceBadge.js`, `ImpactStars.js`.
- Preserve the existing layout structure — this is a color/typography refresh, not a layout redesign.

**Dark mode (deferred, not in this spec):**

Keep dark-mode tokens (`#0d1117`, `#161b22`, `#30363d`, etc.) in mind when naming variables, so a later `[data-theme="dark"]` override on `<html>` can swap them without touching components. Don't ship the dark toggle in this spec.

---

## Acceptance Criteria

- [ ] Impact stars for codec-only changes scale by column size share (F01).
- [ ] Summary shows the recommended codec once, from report data (not hardcoded) (F02).
- [ ] Per-card codec display only appears when it differs from the file-wide default (F02).
- [ ] Confidence pill moves away from impact stars and hides when `High` (F03).
- [ ] Match cards are visibly muted (reduced opacity, compact layout, green accent) (F04).
- [ ] Non-match cards retain full visual weight (F04).
- [ ] Scrolling a mostly-tuned file, a user can visually identify the non-match cards in under 2 seconds without reading any text.
- [ ] The overall UI reads as GitHub-inspired (light canvas, Primer pill styles, subtle 1px borders, clean typography) rather than generic dark-on-dark (F06).
- [ ] All text meets WCAG AA contrast on the new palette.
