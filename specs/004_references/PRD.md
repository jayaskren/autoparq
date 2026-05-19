# PRD — Reference Links in the autoparq Report

## Problem

autoparq makes specific, quantified claims throughout the report:

- "Row group size 42 MB is below the 64–256 MB recommendation"
- "monotonicity_score=0.94 → DELTA_BINARY_PACKED"
- "cardinality_ratio=0.003 → RLE_DICTIONARY"
- "ZSTD:3 is the balanced default"
- "Entropy > 7.5 → leave uncompressed"

An experienced engineer reading these for the first time has no way to verify whether they are industry standards, tool-specific defaults, or the author's opinion. Adding lightweight "learn more" links per claim removes that doubt and makes the tool credible for sharing within a team.

## Goals

1. Every distinct *class* of claim has at least one authoritative source reachable with one click.
2. References feel like footnotes — they do not clutter the primary reading path.
3. Zero new Rust/WASM rebuild required — all changes are JavaScript and HTML.

## Non-Goals

- Inline citations on every instance of a claim (e.g., not every card that fires DELTA).
- Building a general-purpose link management system. This is a static map in JS.
- Hover tooltips with citation previews — a plain `<a>` link is sufficient.

---

## Where References Appear

### R01 — Row group advisory panel

**Location:** `web/src/render/advisories.js`, inside `hasRowGroup` block.

**Claim:** "Row group size X MB is below the 64–256 MB recommendation for general workloads."

**Change:** Add a "Sources" footnote line below the stats grid:

```
Sources: Apache Parquet spec · Apache Spark docs · DuckDB performance guide · Delta Lake docs
```

Each word is an `<a>` link opening in a new tab. Style: `text-xs text-gray-600 hover:text-gray-400`.

**Sources to link:**

| Label | URL |
|-------|-----|
| Apache Parquet spec | https://parquet.apache.org/docs/file-format/configurations/ |
| Apache Spark docs | https://spark.apache.org/docs/latest/sql-data-sources-parquet.html |
| DuckDB performance guide | https://duckdb.org/docs/current/guides/performance/file_formats |
| Delta Lake docs | https://docs.databricks.com/aws/en/delta/tune-file-size |

---

### R02 — Column card accordion: "Learn" box

**Location:** `web/src/render/columns.js`, inside the `fe.teach_yourself` block in the accordion.

**Current:** A blue info box with a plain text teaching paragraph.

**Change:** After the `teach_yourself` text, append a "Further reading →" link specific to the encoding rule that fired. The link is determined by `col.encoding_rule_fired`.

**Rule → source mapping** (static object in columns.js):

```js
const ENCODING_REFS = {
  DeltaMonotonic: {
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
    label: 'Dictionary overflow hazard (parquet-java PARQUET-2052)',
    url: 'https://github.com/apache/parquet-java/pull/910',
  },
  BooleanRle: {
    label: 'Parquet RLE encoding spec',
    url: 'https://github.com/apache/parquet-format/blob/master/Encodings.md',
  },
};
```

**Rendered as:**

```html
<a href="..." target="_blank" rel="noopener noreferrer"
   class="text-blue-400 hover:text-blue-300 underline underline-offset-2">
  Further reading →
</a>
```

Placed on a new line after `teach_yourself`, still inside the blue box.

---

### R03 — Codec option cards

**Location:** `web/src/render/codec-cards.js`

**Claim:** Each of the three bundle cards (Balanced / Smallest File / Fastest Reads) recommends a specific codec. Engineers may want to know why ZSTD:3 is "balanced" or why LZ4 is "fastest."

**Change:** Add a small "why?" link in the footer of each codec card that opens the relevant source.

**Codec → source mapping:**

| Codec | Label | URL |
|-------|-------|-----|
| ZSTD (any level) | facebook/zstd benchmarks | https://github.com/facebook/zstd |
| LZ4 | LZ4 official benchmark | https://lz4.org/ |
| SNAPPY | Apache Spark Parquet docs | https://spark.apache.org/docs/latest/sql-data-sources-parquet.html |
| UNCOMPRESSED | Shannon entropy heuristic (BTRFS kernel docs) | https://btrfs.readthedocs.io/en/latest/Compression.html |

**Rendered as:** A standalone `<p class="text-xs text-gray-600">` element inserted between the tradeoff paragraph and the caveats list, containing `<a>why ${baseCodec}? →</a>` (e.g., "why ZSTD? →", "why LZ4? →").

---

### R04 — Caveats section: known-bug caveats

**Location:** `web/src/render/caveats.js`

**Claim:** Certain caveats reference specific bugs or version constraints (e.g., "parquet-go + LZ4 + DELTA_BINARY_PACKED produces unreadable files", "Spark 3.2+ required for stable ZSTD").

**Change:** Detect caveat messages containing known keywords and append a source link. Use a keyword → source map:

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
```

For each caveat, check if the lowercased message matches any keyword set (all keywords present). If so, append `— <a>source</a>` after the message text.

**Note on parquet-go caveat:** This specific bug has no canonical public issue URL. Its caveat text should include the qualifier "empirically reported" rather than a source link. No change needed in the JS for this one — it's a content fix in the Rust caveat text.

---

### R05 — Summary section: file-level claims

**Location:** `web/src/render/summary.js`

**Change:** Add a single unobtrusive "Reference guide" link at the bottom of the Summary section that points to the hosted `docs/REFERENCES.md` (or a future docs page). This gives engineers one place to find all sources rather than hunting through the cards.

**Rendered as:**

```html
<p class="text-xs text-gray-600 mt-4">
  Recommendations are based on measured column statistics. See
  <a href="https://github.com/your-org/autoparq/blob/main/docs/REFERENCES.md"
     target="_blank" rel="noopener noreferrer"
     class="underline underline-offset-2 hover:text-gray-400">reference sources</a>
  for the research and benchmarks behind each claim.
</p>
```

The GitHub link is a placeholder; substitute the actual repo URL when the project is published.

---

## Visual Design Principles

- All reference links use `text-gray-600 hover:text-gray-400` or `text-blue-400 hover:text-blue-300` depending on context (dark backgrounds = blue, footnote contexts = gray).
- `target="_blank" rel="noopener noreferrer"` on all external links — no exceptions.
- Links are always supplementary — never load-bearing for understanding the primary recommendation. A user who ignores every link still gets the full value of the tool.
- No icons or external-link chevrons on footnote-style links; they add visual noise without value at this size.

---

## Implementation Order

All changes are pure JS/HTML, no WASM rebuild:

| ID | File | Task |
|----|------|------|
| R01 | `advisories.js` | Sources footnote below row group stats grid |
| R02 | `columns.js` | "Further reading →" per encoding rule in accordion teach box |
| R03 | `codec-cards.js` | "why?" link per codec in bundle card footer |
| R04 | `caveats.js` | Keyword-matched source link appended to caveat text |
| R05 | `summary.js` | Single "Reference guide →" footnote at section bottom |

R01 and R05 are straightforward insertions. R02 requires the `ENCODING_REFS` map and a check on `col.encoding_rule_fired`. R03 requires inspecting the codec-cards rendering to find where codec names are emitted. R04 requires the keyword matcher — keep it simple: `keywords.every(k => msg.toLowerCase().includes(k))`.

---

## Acceptance Criteria

- Row group advisory shows 4 source links below the stats grid; all open correct URLs in a new tab.
- Expanding a column card where DELTA fired shows "Further reading →" in the teach box linking to the Parquet Encodings spec.
- Expanding a column card where RLE_DICTIONARY fired shows the Arrow blog link.
- Each codec bundle card has a "why?" link that resolves to the correct source.
- Caveat text containing "ZSTD" and "Spark" keywords gets a "SPARK-25366" link appended.
- "Reference guide →" appears at the bottom of the Summary section.
- No link opens in the same tab. No JS console errors. No layout shifts.
