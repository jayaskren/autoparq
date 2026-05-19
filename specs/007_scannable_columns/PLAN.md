# PLAN — Scannable Columns & GitHub-Inspired Refresh (Spec 007)

## Approach

Five phases. Phase 1 (Rust) ships independently. Phases 2–3 are the theme groundwork that has to come before the behavioral/layout changes (F02–F04) so we don't restyle twice.

The project uses **Tailwind v4 with `@theme` in CSS** ([web/src/style.css](web/src/style.css)) — no separate `tailwind.config.js`. New color tokens and font families go directly in the `@theme` block; Tailwind auto-generates utility classes (`bg-canvas-subtle`, `text-fg-muted`, etc.).

---

## Phase 1 — F01: Rust impact-star fix

**Scope:** [src/tuner.rs:318–353](src/tuner.rs#L318-L353) only.

### Step 1.1 — Rewrite `compute_impact_stars` for codec-only changes

Replace the flat `return 2` with a size-weighted scale capped at 3. Preserve the "no change" and "encoding-change" branches:

```rust
fn compute_impact_stars(
    enc: &EncodingRecommendation,
    codec: &CodecRecommendation,
    meta: &ColumnMetaSummary,
    file_total_uncompressed: i64,
) -> u8 {
    let enc_already_set = meta.encodings.iter().any(|e| e == &enc.encoding);
    let rec_codec_full = match codec.codec_level {
        Some(lvl) => format!("{}:{}", codec.codec, lvl),
        None => codec.codec.clone(),
    };
    let codec_already_set = meta.codec == rec_codec_full;

    if enc_already_set && codec_already_set {
        return 1;
    }

    let share = if file_total_uncompressed > 0 {
        meta.uncompressed_bytes as f64 / file_total_uncompressed as f64
    } else {
        0.0
    };

    if enc_already_set {
        // codec-only change — scale by size, cap at 3
        return match share {
            s if s >= 0.10 => 3,
            s if s >= 0.04 => 2,
            _ => 1,
        };
    }

    // encoding change — existing scale
    match share {
        s if s >= 0.10 => 5,
        s if s >= 0.04 => 4,
        s if s >= 0.01 => 3,
        _ => 2,
    }
}
```

### Step 1.2 — Rebuild WASM

```bash
cd web && npm run wasm:build
```

---

## Phase 2 — F06 foundation: color tokens, fonts, base styles

### Step 2.1 — Define Primer color tokens in `@theme`

Replace the existing `@theme` block in [web/src/style.css](web/src/style.css) with a GitHub Primer-inspired token set:

```css
@theme {
  /* Canvas */
  --color-canvas-default: #ffffff;
  --color-canvas-subtle: #f6f8fa;
  --color-canvas-inset: #eaeef2;

  /* Borders */
  --color-border-default: #d0d7de;
  --color-border-muted: #d8dee4;

  /* Foreground text */
  --color-fg-default: #1f2328;
  --color-fg-muted: #656d76;
  --color-fg-subtle: #6e7781;
  --color-fg-on-emphasis: #ffffff;

  /* Accent (blue — links, primary actions) */
  --color-accent-fg: #0969da;
  --color-accent-emphasis: #0969da;
  --color-accent-subtle: #ddf4ff;

  /* Success (green — Match) */
  --color-success-fg: #1a7f37;
  --color-success-emphasis: #1f883d;
  --color-success-subtle: #dafbe1;

  /* Attention (yellow — Fallback) */
  --color-attention-fg: #9a6700;
  --color-attention-emphasis: #bf8700;
  --color-attention-subtle: #fff8c5;

  /* Severe / Danger */
  --color-severe-fg: #bc4c00;
  --color-danger-fg: #d1242f;
  --color-danger-subtle: #ffebe9;

  /* Neutral (gray pills) */
  --color-neutral-fg: #656d76;
  --color-neutral-subtle: #eaeef2;

  /* Confidence badges (keep light-mode-compatible colors) */
  --color-confidence-high-text: #1a7f37;
  --color-confidence-high-bg: #dafbe1;
  --color-confidence-medium-text: #9a6700;
  --color-confidence-medium-bg: #fff8c5;
  --color-confidence-low-text: #d1242f;
  --color-confidence-low-bg: #ffebe9;

  /* Typography */
  --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji";
  --font-mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
}
```

Tailwind v4 auto-generates: `bg-canvas-subtle`, `text-fg-muted`, `border-border-default`, `font-sans`, etc.

### Step 2.2 — Update `body` and page-shell base styles

In `style.css`:
- Set `body { background: var(--color-canvas-default); color: var(--color-fg-default); font-family: var(--font-sans); }`
- Set base font-size to 14px (GitHub default).
- Replace Tabulator's `tabulator_midnight.min.css` import with the light `tabulator_bootstrap5.min.css` (or `tabulator_simple.min.css`).
- Rewrite the Tabulator override block to use the new tokens instead of `#111827`/`#374151`.

### Step 2.3 — Update `index.html` shell

Open [web/index.html](web/index.html), replace every hardcoded dark color class (`bg-gray-900`, `text-gray-100`, `border-gray-800`, etc.) with the Primer equivalents. The `<body>` tag and any page-wide wrapper divs are the main targets.

---

## Phase 3 — F06 components: restyle every render module

Work through these files in order, each one replacing dark-mode Tailwind classes with Primer-token classes:

### Step 3.1 — `render/summary.js`
Swap `bg-gray-900 border border-gray-800 rounded-xl p-5` → `bg-canvas-subtle border border-border-default rounded-md p-5`. Text: `text-gray-400` → `text-fg-muted`; `text-gray-200` → `text-fg-default`. Pills and badges use `bg-neutral-subtle text-neutral-fg` or accent variants.

### Step 3.2 — `render/columns.js`
The largest file. Card wrapper: remove the `border-l-4 border-l-amber-500` accent pattern (per F04); use `bg-canvas-subtle border border-border-default rounded-md`. Filter bar inputs use `bg-canvas-default border-border-default`.

### Step 3.3 — `render/codec-cards.js`, `render/advisories.js`, `render/caveats.js`
Same substitution pattern. Advisories/caveats keep their severity color via `text-attention-fg bg-attention-subtle border-attention-emphasis` etc.

### Step 3.4 — `components/ConfidenceBadge.js`, `components/ImpactStars.js`
Confidence badge already references `--color-confidence-*-bg/text` — these will auto-update when the tokens change to light-mode values. Impact stars: change filled-star color from `#facc15` to `#bf8700` (GitHub attention emphasis).

### Step 3.5 — `components/SnippetPanel.js`, `components/CodeBlock.js`, `components/Tooltip.js`
Code blocks: `bg-canvas-subtle` with `border-border-muted`. Tooltips: `bg-canvas-inset` with subtle border.

### Step 3.6 — `render/report.js`
Page-level chrome, section headers, snippet panel wrapper. Replace dark borders/backgrounds.

---

## Phase 4 — F02/F03/F04 layout and scannability

### Step 4.1 — F02: hoist codec recommendation

**Summary (`render/summary.js`):**
- Replace the hardcoded `"ZSTD:3"` string in the "Recommended codec" badge with the file-wide recommended codec, computed from the report. Use the mode of `(col.recommended_codec, col.recommended_codec_level)` across all columns.
- Below the badge, add a muted footnote: "Applied file-wide. Columns flagged below use a different codec."

**Column cards (`render/columns.js`):**
- In `renderColumns`, pre-compute the file-wide recommended codec (mode across all cols).
- In `buildColumnCard`, check whether `getRecCodecFull(col) === fileWideCodec`. If yes, omit the codec row in the Current/Recommended diff and in the diagnostic block (F06's header-icon status will still communicate the overall state). If no, keep the codec row and append a tiny footnote: "codec differs — typically because this column contains already-compressed data (high byte entropy)."

### Step 4.2 — F03: reposition + conditional confidence badge

In `render/columns.js` `buildColumnCard`:
- Move the `ConfidenceBadge` out of the cluster next to `ImpactStars`. Place it after the type badge, on the right side of the header.
- Render it only when `col.confidence !== 'High'`. High confidence is the common case and carries no actionable signal; showing it everywhere just creates clutter.

### Step 4.3 — F04: match-card muting + header-icon status

**Replace** the left-border accent pattern with a compact header-icon system on all cards (compatible with F06's flat-bordered GitHub style):

- Non-match cards: status icon in the header (colored dot + label). Card body renders in full.
- Match cards: `opacity-70` on the whole card. Compact layout — header only; the Current/Recommended diff block collapses to a single-line footer ("At recommended settings."). The accordion still opens to reveal the teaching content.

**Implementation details:**

- Add a CSS helper class `.card-muted` in `style.css`:
  ```css
  .card-muted { opacity: 0.7; }
  .card-muted:hover { opacity: 1; }
  ```
- In `buildColumnCard`, when `diagnostic.status === 'Match'`:
  - Add `card-muted` to the card root.
  - Skip rendering `compareRow` and `reasonRow`.
  - Add a `<div>` in their place: `<div class="px-4 pb-3 text-xs text-fg-subtle">At recommended settings.</div>`
  - Keep the accordion toggle so users can still drill in for the teaching content.

### Step 4.4 — F05: verify existing filter/nav after restyle

Manually exercise:
1. "Non-matching only" checkbox in the filter bar — still filters correctly.
2. "File health: N of M match" button in the Summary — still dispatches `autoparq:filter-non-matching` and scrolls to the Columns section.
3. Chart view — bars use new palette tokens.
4. Table view — Tabulator now renders in light mode without visual artifacts.

---

## Phase 5 — Verification

1. `cargo build && cargo test --lib` — confirm Rust still green after F01.
2. `cd web && npm run wasm:build` — rebuild if F01 changed.
3. `npx vite build` — confirm no class-name regressions.
4. `npm run dev:nowasm` — manual walk-through on a real parquet file:
   - Drop the NYC taxi file used for testing.
   - Confirm VendorID shows ★ (not ★★) per F01.
   - Confirm codec isn't repeated on every card per F02.
   - Confirm confidence badge is absent on High-confidence columns per F03.
   - Confirm match cards are muted and compact per F04.
   - Confirm overall visual reads as GitHub-light per F06.
5. Contrast check: use a Chrome DevTools contrast pass on 5–10 representative text-on-background combinations. All must meet WCAG AA (4.5:1 for normal text, 3:1 for large text).

---

## Risks

| Risk | Mitigation |
|------|-----------|
| Tailwind v4 `@theme` doesn't auto-generate color utilities for arbitrary names | Verify by checking if `bg-canvas-subtle` produces `background: var(--color-canvas-subtle)` during build; fall back to arbitrary-value syntax if needed |
| Tabulator light theme doesn't exist or looks very different | Use `tabulator_simple.min.css` which is theme-neutral; apply our own overrides |
| Light mode exposes accessibility issues (too-light text on subtle backgrounds) | Run contrast check in Phase 5; adjust individual tokens |
| Existing snapshot tests reference class names | Unlikely — the snapshots are from Rust `insta`, which tests CLI JSON output, not web HTML |
| "File health" click-to-filter breaks when Summary card is restyled | Test in Phase 5 Step 4; the event dispatch is decoupled from styling |
