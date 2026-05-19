# autoparq Reference Links — Conflicts and Ambiguities

Cross-referenced against: PRD.md, PLAN.md, TASKS.md, and the actual source files
(`src/recommender/codec.rs`, `web/src/render/codec-cards.js`, `web/src/render/columns.js`).

---

## C01 — R04 entropy keyword will never match (High)

**Severity: High** — The keyword matcher silently fails; no link ever appears on the entropy caveat.

Both PRD and TASKS specify `['entropy', 'uncompressed']` for the entropy caveat ref. The actual
caveat message in `src/recommender/codec.rs` line 83 is:

> "Compressing high-entropy data adds CPU overhead without reducing size"

The word `"uncompressed"` does not appear in this string. The matcher will never fire.

**Resolution:** Change the keywords to `['high-entropy']`. That substring appears in the actual
message and is specific enough to avoid false positives.

---

## C02 — R04 `['delta_binary_packed', 'spark']` matches no real caveat (High)

**Severity: High** — The entry exists in both PRD and TASKS but there is no corresponding caveat
in the Rust codebase.

The only DELTA_BINARY_PACKED caveat in `codec.rs` is:

> "Known bug in parquet-go: LZ4 + DELTA_BINARY_PACKED produces unreadable files. Use SNAPPY if your reader uses the parquet-go library."

This mentions neither "spark" nor "3.3". There is no Spark 3.3 / DELTA caveat in the codebase.

**Two options — choose one:**

A. **Remove the entry.** No caveat currently warrants it. If a Spark + DELTA caveat is added to
   Rust later, the keyword entry can be added then.

B. **Add the Rust caveat first.** The REFERENCES research found that Spark 3.3+ is required for
   the vectorized DELTA reader (SPARK-36879). Adding a caveat in `codec.rs` for `engine = Spark`
   + `encoding_rule_fired = DeltaMonotonic` would give the matcher something to match. This
   requires a Rust change and WASM rebuild, which is outside the scope of this spec.

**Recommended resolution:** Option A — remove the entry for now.

---

## C03 — R04 `['lz4', 'spark', '3.5']` and `['clickhouse', 'brotli']` also match nothing (Medium)

**Severity: Medium** — Two additional entries in TASKS that appear in neither the PRD nor any
actual Rust caveat message.

TASKS adds these two entries not present in the PRD:
- `['lz4', 'spark', '3.5']` — No Spark + LZ4 3.5 caveat exists in the codebase.
- `['clickhouse', 'brotli']` — No ClickHouse + brotli caveat exists in the codebase.

Dead keyword entries don't break anything, but they create maintenance confusion.

**Resolution:** Remove both entries from TASKS. The `CAVEAT_REFS` list that will actually match
real caveats as of today is:

```js
const CAVEAT_REFS = [
  {
    keywords: ['zstd', 'spark'],
    label: 'SPARK-25366',
    url: 'https://issues.apache.org/jira/browse/SPARK-25366',
  },
  {
    keywords: ['high-entropy'],       // fix from C01
    label: 'entropy heuristic',
    url: 'https://btrfs.readthedocs.io/en/latest/Compression.html',
  },
];
```

---

## C04 — PRD R04 SPARK-25366 keyword set too narrow (Medium)

**Severity: Medium** — PRD uses `['zstd', 'spark', 'version']`; TASKS uses `['zstd', 'spark']`.

The actual Rust codebase produces three distinct ZSTD/Spark caveat strings:
1. `"ZSTD requires Spark 3.2+; SNAPPY works on all versions. Use --engine spark (with version) to unlock ZSTD."` — contains "version" ✓
2. `"ZSTD requires Spark 3.2+"` — no "version" ✗
3. `"ZSTD requires Spark 3.2+; use --engine spark with Spark 3.2+ or switch --priority to use SNAPPY"` — no "version" ✗

PRD's `['zstd', 'spark', 'version']` would match only caveat #1. TASKS `['zstd', 'spark']`
matches all three.

**Resolution:** TASKS wins — use `['zstd', 'spark']`.

---

## C05 — R03 placement description conflict (Medium)

**Severity: Medium** — PRD and TASKS/PLAN disagree on where the "why?" link sits in the card.

PRD says: *"placed after the codec name chip in the card footer, same line."*

There is no "card footer" or "codec name chip" in the actual `codec-cards.js` DOM. The card
structure is: `header` (icon + title + badge) → `tradeoff` (paragraph) → `caveatList` → `snippetBtn`.
The codec description (`opt.codec_description`) lives inside the header's `titleArea`, not a footer.

PLAN and TASKS correctly place the link as a separate `<p>` element between `tradeoff` and
`caveatList`, which matches the actual code.

**Resolution:** TASKS/PLAN wins. The PRD description is imprecise. The link is a standalone
element below the tradeoff text, not inline with the codec chip in the header.

---

## C06 — `rel="noopener"` in PRD snippets contradicts PRD's own design principles (Low)

**Severity: Low** — Inconsistency within the PRD itself.

PRD's Visual Design Principles state: *"`target=\"_blank\" rel=\"noopener noreferrer\"` on all external links — no exceptions."*

However, the code snippets in PRD R02 and R05 use only `rel="noopener"` (missing `noreferrer`).

TASKS and PLAN consistently use `rel="noopener noreferrer"` throughout.

**Resolution:** TASKS/PLAN win. All implementations should use `rel="noopener noreferrer"`.

---

## C07 — R03 link text: "why?" (PRD) vs "why CODEC? →" (TASKS/PLAN) (Low)

**Severity: Low** — UX inconsistency. Two different text patterns described.

PRD: `why?` — generic, codec-agnostic.
TASKS/PLAN: `why ZSTD? →` / `why LZ4? →` — codec-specific with trailing arrow.

The codec-specific form is more informative and consistent with the "Further reading →" style used
in R02.

**Resolution:** TASKS/PLAN win — use `why ${baseCodec}? →`.

---

## C08 — R02 `PlainDefault` entry: explicit null (PRD) vs omitted (TASKS/PLAN) (Low)

**Severity: Low** — Both produce identical runtime behavior (no link shown for PLAIN columns).

PRD includes `PlainDefault: null` in the `ENCODING_REFS` map as an explicit entry. TASKS and
PLAN omit it entirely. Both result in `ENCODING_REFS['PlainDefault']` returning `undefined`
(or `null`), so the `if (ref)` guard suppresses the link either way.

**Resolution:** Omit the entry (TASKS/PLAN form). An explicit `null` entry adds noise without
changing behavior.

---

## C09 — R02 RleDictionary label: "and" (PRD) vs "&" (TASKS/PLAN) (Low)

**Severity: Low** — Visible in the rendered link text.

PRD: `'Dictionary encoding and the 1 MB fallback (Arrow blog)'`
TASKS/PLAN: `'Dictionary encoding & the 1 MB fallback (Arrow blog)'`

**Resolution:** Use `&amp;` in the HTML string to avoid potential entity issues in innerHTML
contexts. Effective text: `Dictionary encoding & the 1 MB fallback (Arrow blog)`. TASKS/PLAN
direction is correct; just ensure it is HTML-escaped if rendered via `innerHTML`.

---

## C10 — R05 footnote prose differs between PRD and TASKS (Low)

**Severity: Low** — Different sentence structures.

PRD: `"Claims in this report are based on measured file statistics. Reference sources →"`
TASKS: `"Recommendations are based on measured column statistics. See [link] for the research and benchmarks behind each claim."`

TASKS prose is clearer: "see [link] for..." is more explicit than a bare linked phrase.

**Resolution:** Use TASKS prose.

---

## Summary Table

| ID | Severity | Resolution | Status |
|----|----------|-----------|--------|
| C01 | High | Changed entropy keywords to `['high-entropy']` in PRD and TASKS | ✅ Fixed |
| C02 | High | Removed `['delta_binary_packed', 'spark']` entry from PRD and TASKS | ✅ Fixed |
| C03 | Medium | Removed `['lz4', 'spark', '3.5']` and `['clickhouse', 'brotli']` from TASKS | ✅ Fixed |
| C04 | Medium | Changed to `['zstd', 'spark']` in PRD (TASKS was already correct) | ✅ Fixed |
| C05 | Medium | Fixed PRD R03 placement description to match TASKS/PLAN | ✅ Fixed |
| C06 | Low | Changed all PRD snippets to `rel="noopener noreferrer"` | ✅ Fixed |
| C07 | Low | Updated PRD R03 to `why ${baseCodec}? →` (TASKS/PLAN form) | ✅ Fixed |
| C08 | Low | Removed `PlainDefault: null` from PRD ENCODING_REFS (TASKS already omitted it) | ✅ Fixed |
| C09 | Low | Changed to `&amp;` in RleDictionary label in both PRD and TASKS | ✅ Fixed |
| C10 | Low | Updated PRD R05 footnote prose to match TASKS | ✅ Fixed |
