# PLAN — Reference Links Integration

All changes are pure JavaScript. No Rust changes, no WASM rebuild.

---

## Shared helper: `refLink(label, url)`

Add once at the top of each file that needs it (or extract to a shared module if preferred):

```js
function refLink(label, url) {
  return `<a href="${url}" target="_blank" rel="noopener noreferrer"
     class="hover:text-gray-400 underline underline-offset-2 transition-colors">${label}</a>`;
}
```

Only needs to exist in the files where it's used — no new shared module required.

---

## R01 — Row group advisory sources footnote

**File:** `web/src/render/advisories.js`
**After:** closing `</div>` of the stats grid (currently last element inside the inner `<div>` before the outer `</div></div>` that closes the flex container), **before** `panel.innerHTML` ends.

**Current structure (simplified):**
```html
<div class="flex items-start gap-3">
  <span>⚠</span>
  <div>
    <h3>Row Group Size Advisory</h3>
    <p>${rga.advice}</p>
    <div class="grid ...">  <!-- 4 stat cells -->  </div>
    <!-- INSERT HERE -->
  </div>
</div>
```

**Insert after the closing `</div>` of the grid:**
```html
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

**Implementation:** `refLink` is a local function defined at the top of `advisories.js`. The insertion is a string append inside the existing template literal — no DOM manipulation needed.

---

## R02 — Encoding "Further reading →" in column card accordion

**File:** `web/src/render/columns.js`

**Step 1:** Add the static ref map near the top of the file (after the `humanBytes` function):

```js
const ENCODING_REFS = {
  DeltaMonotonic:  { label: 'Parquet DELTA_BINARY_PACKED spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
  RleDictionary:   { label: 'Dictionary encoding & the 1 MB fallback', url: 'https://arrow.apache.org/blog/2019/09/05/faster-strings-cpp-parquet/' },
  ByteStreamSplit: { label: 'Parquet BYTE_STREAM_SPLIT spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
  PlainUuid:       { label: 'Dictionary overflow hazard (PARQUET-2052)', url: 'https://github.com/apache/parquet-java/pull/910' },
  BooleanRle:      { label: 'Parquet RLE spec', url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md' },
};
```

**Step 2:** Locate the `teach_yourself` block in `buildColumnCard` (currently ~line 263):

```js
${fe.teach_yourself ? `
  <div class="bg-blue-950/30 border border-blue-800/40 rounded-lg p-3 text-xs text-blue-200">
    <span class="font-semibold text-blue-300">Learn: </span>${fe.teach_yourself}
  </div>
` : ''}
```

Replace with:

```js
${fe.teach_yourself ? (() => {
  const ref = ENCODING_REFS[col.encoding_rule_fired];
  const furtherReading = ref
    ? `<a href="${ref.url}" target="_blank" rel="noopener noreferrer"
          class="block mt-2 text-blue-400 hover:text-blue-300 underline underline-offset-2">
         Further reading: ${ref.label} →
       </a>`
    : '';
  return `
    <div class="bg-blue-950/30 border border-blue-800/40 rounded-lg p-3 text-xs text-blue-200">
      <span class="font-semibold text-blue-300">Learn: </span>${fe.teach_yourself}
      ${furtherReading}
    </div>`;
})() : ''}
```

`col.encoding_rule_fired` is already available in scope — it's used in the reasoning chain rendering just above this block.

---

## R03 — "why?" link on codec bundle cards

**File:** `web/src/render/codec-cards.js`

**Step 1:** Add codec → source map near the top of the file:

```js
const CODEC_REFS = {
  ZSTD:         { label: 'zstd benchmarks', url: 'https://github.com/facebook/zstd' },
  LZ4:          { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  LZ4_RAW:      { label: 'LZ4 benchmarks', url: 'https://lz4.org/' },
  SNAPPY:       { label: 'Spark Parquet docs', url: 'https://spark.apache.org/docs/latest/sql-data-sources-parquet.html' },
  UNCOMPRESSED: { label: 'entropy heuristic', url: 'https://btrfs.readthedocs.io/en/latest/Compression.html' },
};
```

**Step 2:** After the `tradeoff` paragraph is appended (~line 50), and before `card.append(...)`, insert a "why?" link derived from the option's codec:

```js
// Extract base codec name from opt.codec_description (e.g. "ZSTD:3" → "ZSTD")
const baseCodec = (opt.codec_description ?? '').split(':')[0].toUpperCase();
const codecRef = CODEC_REFS[baseCodec];

const whyLink = document.createElement('p');
whyLink.className = 'text-xs text-gray-600';
if (codecRef) {
  whyLink.innerHTML = `<a href="${codecRef.url}" target="_blank" rel="noopener noreferrer"
    class="hover:text-gray-400 underline underline-offset-2">why ${baseCodec}? →</a>`;
}
```

**Step 3:** Add `whyLink` to `card.append(...)`:

```js
// Before:
card.append(header, tradeoff, caveatList, snippetBtn);

// After:
card.append(header, tradeoff, whyLink, caveatList, snippetBtn);
```

**Note:** `opt.codec_description` is a field in the `TuneOptionBundle` struct. Verify its exact content in a sample report. If it's not the right field, use `opt.codec` (the base name) instead.

**Fallback:** If `opt.codec_description` is not structured (contains prose like "Balanced compression"), derive the codec from `opt.label` or leave `whyLink` empty — `if (codecRef)` handles the null case silently.

---

## R04 — Source links appended to matching caveats

**File:** `web/src/render/caveats.js`

**Step 1:** Add keyword → source map at the top of the file:

```js
const CAVEAT_REFS = [
  {
    keywords: ['zstd', 'spark'],
    label: 'SPARK-25366',
    url: 'https://issues.apache.org/jira/browse/SPARK-25366',
  },
  {
    keywords: ['delta_binary_packed', 'spark'],
    label: 'Spark 3.3.0 release notes',
    url: 'https://spark.apache.org/releases/spark-release-3-3-0.html',
  },
  {
    keywords: ['lz4', 'spark', '3.5'],
    label: 'Spark 3.5.0 release notes',
    url: 'https://spark.apache.org/releases/spark-release-3-5-0.html',
  },
  {
    keywords: ['entropy', 'uncompressed'],
    label: 'entropy heuristic',
    url: 'https://btrfs.readthedocs.io/en/latest/Compression.html',
  },
  {
    keywords: ['clickhouse', 'brotli'],
    label: 'ClickHouse Parquet docs',
    url: 'https://clickhouse.com/docs/interfaces/formats/Parquet',
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

**Step 2:** In the `li.innerHTML` template (~line 45), append the ref link after `caveat.message`:

```js
// Before:
<span>${caveat.message}</span>

// After:
<span>${caveat.message}${caveatRefLink(caveat.message)}</span>
```

---

## R05 — "Reference guide →" footnote in Summary

**File:** `web/src/render/summary.js`

**Location:** After the closing `</div>` of the two-column grid (line ~85), before `` ` `` closes `section.innerHTML`.

**Insert:**

```html
<p class="mt-4 text-xs text-gray-600">
  All recommendations are based on measured column statistics.
  <a href="https://github.com/your-org/autoparq/blob/main/docs/REFERENCES.md"
     target="_blank" rel="noopener noreferrer"
     class="underline underline-offset-2 hover:text-gray-400">
    Reference sources →
  </a>
</p>
```

**Note:** Replace `your-org/autoparq` with the actual repo path when the project is published. Until then, link to the local `docs/REFERENCES.md` is not linkable from a browser — leave the placeholder in place or omit R05 until the repo is public.

---

## Verification checklist

- [ ] Drop NYC Taxi file; expand a DELTA_BINARY_PACKED column → "Further reading: Parquet DELTA_BINARY_PACKED spec →" appears in blue box
- [ ] Expand an RLE_DICTIONARY column → Arrow blog link appears
- [ ] Row group advisory visible → 4 source links appear below the stats grid
- [ ] "why ZSTD? →" link appears on the Balanced and Smallest File cards
- [ ] "why LZ4? →" appears on the Fastest Reads card
- [ ] Any caveat containing "zstd" + "spark" gets "SPARK-25366" appended
- [ ] All links open in a new tab
- [ ] No JS console errors
- [ ] Cards/table with no advisory or no `full_explain` — no broken links or missing-ref errors
