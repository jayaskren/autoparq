# autoparq Reference Links — Tasks

## Task Index

| ID | File | Description |
|----|------|-------------|
| R01 | `web/src/render/advisories.js` | Sources footnote below row group stats grid |
| R02 | `web/src/render/columns.js` | "Further reading →" link in encoding accordion teach box |
| R03 | `web/src/render/codec-cards.js` | "why CODEC? →" link on each bundle card |
| R04 | `web/src/render/caveats.js` | Keyword-matched source link appended to caveat text |
| R05 | `web/src/render/summary.js` | "Reference sources →" footnote at Summary section bottom |
| V-R1 | — | Validation gate |

All tasks are pure JavaScript. No Rust changes, no WASM rebuild.

---

## R01 — Sources footnote in row group advisory

**File:** `web/src/render/advisories.js`

**What:** Add a `refLink` helper and insert a four-source footnote line at the bottom of the row group advisory panel, below the stats grid.

**Add `refLink` at the top of the file** (before `export function renderAdvisories`):

```js
function refLink(label, url) {
  return `<a href="${url}" target="_blank" rel="noopener noreferrer"
     class="hover:text-gray-400 underline underline-offset-2 transition-colors">${label}</a>`;
}
```

**In the `hasRowGroup` panel template**, after the closing `</div>` of the `grid grid-cols-2 sm:grid-cols-4` div and before the outer `</div></div>` that closes the flex container, insert:

```js
<p class="mt-3 text-xs text-gray-600">
  Sources:
  ${refLink('Apache Parquet spec', 'https://parquet.apache.org/docs/file-format/configurations/')}
  ·
  ${refLink('Spark docs', 'https://spark.apache.org/docs/latest/sql-data-sources-parquet.html')}
  ·
  ${refLink('DuckDB performance guide', 'https://duckdb.org/docs/current/guides/performance/file_formats')}
  ·
  ${refLink('Delta Lake docs', 'https://docs.databricks.com/aws/en/delta/tune-file-size')}
</p>
```

**Acceptance criteria:**
- Row group advisory panel shows 4 gray links on a single line below the amber stats grid
- All 4 links open correct URLs in a new tab
- When no row group advisory is shown (file is within range), no links appear — no error
- Sort order advisory panel is unaffected

---

## R02 — "Further reading →" in column card teach box

**File:** `web/src/render/columns.js`

**What:** Add a static `ENCODING_REFS` map and extend the `teach_yourself` block in the accordion to append a "Further reading →" link specific to the encoding rule that fired.

**Add `ENCODING_REFS` immediately after the `humanBytes` function** (top of file, before `renderColumns`):

```js
const ENCODING_REFS = {
  DeltaMonotonic:  {
    label: 'Parquet DELTA_BINARY_PACKED spec',
    url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md',
  },
  RleDictionary: {
    label: 'Dictionary encoding &amp; the 1 MB fallback (Arrow blog)',
    url: 'https://arrow.apache.org/blog/2019/09/05/faster-strings-cpp-parquet/',
  },
  ByteStreamSplit: {
    label: 'Parquet BYTE_STREAM_SPLIT spec',
    url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md',
  },
  PlainUuid: {
    label: 'Dictionary overflow hazard (PARQUET-2052)',
    url: 'https://github.com/apache/parquet-java/pull/910',
  },
  BooleanRle: {
    label: 'Parquet RLE spec',
    url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md',
  },
};
```

**In `buildColumnCard`**, replace the existing `teach_yourself` conditional block:

```js
// Before:
${fe.teach_yourself ? `
  <div class="bg-blue-950/30 border border-blue-800/40 rounded-lg p-3 text-xs text-blue-200">
    <span class="font-semibold text-blue-300">Learn: </span>${fe.teach_yourself}
  </div>
` : ''}

// After:
${fe.teach_yourself ? (() => {
  const ref = ENCODING_REFS[col.encoding_rule_fired];
  const link = ref
    ? `<a href="${ref.url}" target="_blank" rel="noopener noreferrer"
          class="block mt-2 text-blue-400 hover:text-blue-300 underline underline-offset-2">
         Further reading: ${ref.label} →
       </a>`
    : '';
  return `
    <div class="bg-blue-950/30 border border-blue-800/40 rounded-lg p-3 text-xs text-blue-200">
      <span class="font-semibold text-blue-300">Learn: </span>${fe.teach_yourself}
      ${link}
    </div>`;
})() : ''}
```

**Note:** `col.encoding_rule_fired` is in scope — it's already used in the `reasoning_chain` rendering above this block. `PlainDefault` is intentionally absent from the map; the IIFE returns no link for it silently.

**Acceptance criteria:**
- Expanding a DELTA_BINARY_PACKED column → blue box ends with "Further reading: Parquet DELTA_BINARY_PACKED spec →"
- Expanding an RLE_DICTIONARY column → blue box ends with Arrow blog link
- Expanding a BYTE_STREAM_SPLIT column → blue box ends with Parquet BYTE_STREAM_SPLIT spec link
- Expanding a PLAIN (default) column → blue box shows `teach_yourself` text with no link
- Column with no `full_explain` (accordion closed state) → no error
- Link opens in new tab

---

## R03 — "why CODEC?" link on codec bundle cards

**File:** `web/src/render/codec-cards.js`

**What:** Add a `CODEC_REFS` map and insert a small "why?" link on each bundle card, between the tradeoff paragraph and the caveats list.

**Add `CODEC_REFS` at the top of the file** (before `export function renderCodecCards`):

```js
const CODEC_REFS = {
  ZSTD:         { label: 'zstd benchmarks', url: 'https://github.com/facebook/zstd' },
  LZ4:          { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  LZ4_RAW:      { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  SNAPPY:       { label: 'Spark Parquet docs', url: 'https://spark.apache.org/docs/latest/sql-data-sources-parquet.html' },
  UNCOMPRESSED: { label: 'entropy heuristic', url: 'https://btrfs.readthedocs.io/en/latest/Compression.html' },
};
```

**Inside the `Object.entries(report.options).forEach` loop**, after the `tradeoff` element is created and before `card.append(...)`, add:

```js
// Derive base codec name: "ZSTD:3" → "ZSTD", "LZ4" → "LZ4"
const baseCodec = (opt.codec_description ?? '').split(':')[0].toUpperCase();
const codecRef = CODEC_REFS[baseCodec];

const whyLink = document.createElement('p');
whyLink.className = 'text-xs text-gray-600';
if (codecRef) {
  whyLink.innerHTML = `<a href="${codecRef.url}" target="_blank" rel="noopener noreferrer"
    class="hover:text-gray-400 underline underline-offset-2">why ${baseCodec}? →</a>`;
}
```

**Update `card.append`:**

```js
// Before:
card.append(header, tradeoff, caveatList, snippetBtn);

// After:
card.append(header, tradeoff, whyLink, caveatList, snippetBtn);
```

**Note:** `opt.codec_description` is confirmed to be a codec string (`"ZSTD:6"`, `"LZ4"`, etc.) — not prose — so the `:` split is safe.

**Acceptance criteria:**
- Balanced card shows "why ZSTD? →" linking to github.com/facebook/zstd
- Smallest File card shows "why ZSTD? →" (same link; both use ZSTD at different levels)
- Fastest Reads card shows "why LZ4? →" linking to lz4.org
- Each link opens in a new tab
- Card layout, RECOMMENDED badge, and "Get snippet →" button are unaffected

---

## R04 — Keyword-matched source links in caveats

**File:** `web/src/render/caveats.js`

**What:** Add a `CAVEAT_REFS` keyword map and a `caveatRefLink()` helper. Append a source link to any caveat whose text matches a known keyword set.

**Add at the top of the file** (before `export function renderCaveats`):

```js
const CAVEAT_REFS = [
  {
    keywords: ['zstd', 'spark'],
    label: 'SPARK-25366',
    url: 'https://issues.apache.org/jira/browse/SPARK-25366',
  },
  {
    keywords: ['high-entropy'],
    label: 'entropy heuristic',
    url: 'https://btrfs.readthedocs.io/en/latest/Compression.html',
  },
];

function caveatRefLink(message) {
  const lower = message.toLowerCase();
  const match = CAVEAT_REFS.find(r => r.keywords.every(k => lower.includes(k)));
  if (!match) return '';
  return ` — <a href="${match.url}" target="_blank" rel="noopener noreferrer"
    class="underline underline-offset-2 hover:opacity-80">${match.label}</a>`;
}
```

**In the `li.innerHTML` template** (~line 45), change the `<span>${caveat.message}</span>` line:

```js
// Before:
<span>${caveat.message}</span>

// After:
<span>${caveat.message}${caveatRefLink(caveat.message)}</span>
```

**Acceptance criteria:**
- A caveat containing "ZSTD" and "Spark" (case-insensitive) renders with "— SPARK-25366" linked at the end
- A caveat containing "high-entropy" (case-insensitive) renders with "— entropy heuristic" linked at the end
- Caveats that match no keyword set are unaffected
- No JS error when `caveats` is empty or `caveat.message` is undefined

---

## R05 — "Reference sources →" footnote in Summary

**File:** `web/src/render/summary.js`

**What:** Append a one-line footnote below the two-column summary grid, linking to `docs/REFERENCES.md` in the repo.

**Location:** Inside `section.innerHTML`, after the closing `</div>` of the `grid grid-cols-1 md:grid-cols-2` div and before the final closing backtick of the template literal.

**Insert:**

```html
<p class="mt-4 text-xs text-gray-600">
  Recommendations are based on measured column statistics. See
  <a href="https://github.com/your-org/autoparq/blob/main/docs/REFERENCES.md"
     target="_blank" rel="noopener noreferrer"
     class="underline underline-offset-2 hover:text-gray-400">reference sources</a>
  for the research and benchmarks behind each claim.
</p>
```

**Note:** Replace `your-org/autoparq` with the real repo path when the project is published. Until then the link will 404 — acceptable since this is a placeholder. Alternatively, omit the `<a>` and render plain text until the repo is public.

**Acceptance criteria:**
- Footnote text appears below the two stat cards in the Summary section
- Gray, unobtrusive — does not compete visually with the numbers above it
- Link opens in a new tab (or is plain text if repo is not yet public)

---

## V-R1 — Validation gate

1. Start `npm run dev:nowasm` in `web/`
2. Drop the NYC Taxi parquet file
3. **Row group advisory:** Confirm 4 source links appear in gray below the amber stats grid; each opens the correct URL in a new tab
4. **Column cards — DELTA column:** Expand a column card where `encoding_rule_fired = DeltaMonotonic`; confirm "Further reading: Parquet DELTA_BINARY_PACKED spec →" appears in the blue box
5. **Column cards — RLE column:** Expand an RLE_DICTIONARY column; confirm Arrow blog link appears
6. **Column cards — PLAIN column:** Expand a PLAIN (default) column; confirm no "Further reading" link appears and no JS error
7. **Codec cards:** Confirm "why ZSTD? →" on Balanced and Smallest File cards; "why LZ4? →" on Fastest Reads; all open correct URLs
8. **Caveats:** Confirm any ZSTD/Spark caveat has "— SPARK-25366" linked at the end
9. **Summary:** Confirm footnote text appears below the stat cards
10. Check browser console — no JS errors
11. Check that "Changed only" filter, card expansion, and table view still work correctly
