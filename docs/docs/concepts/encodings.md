# Parquet Encodings

Encoding determines how individual column values are stored before compression. Choosing the right encoding is often more impactful than codec choice.

## DELTA_BINARY_PACKED

Best for: monotonically increasing integers (timestamps, auto-increment IDs)

DELTA_BINARY_PACKED stores differences between consecutive values instead of the values themselves. For sorted integers, deltas are tiny and pack into very few bits.

Rule of thumb: use DELTA on any monotonically increasing integer column. autoparq applies this when `monotonicity_score >= 0.90`.

## RLE_DICTIONARY

Best for: low-cardinality columns (status codes, categories, enums)

Dictionary encoding stores each distinct value once and replaces data values with small integer indices. When cardinality is low (few distinct values), these indices compress extremely well.

Rule of thumb: if cardinality < 10% of row count and the dictionary fits in ~512KB, use dictionary encoding.

## BYTE_STREAM_SPLIT

Best for: high-cardinality floating-point columns (measurements, scores)

BYTE_STREAM_SPLIT deinterleaves the bytes of floating-point values — writing all MSBs together, then all next bytes, etc. This groups similar bytes, improving codec compression by 10–30%.

## DELTA_BYTE_ARRAY

Best for: sorted or nearly-sorted string columns with shared prefixes (file paths, hierarchical IDs, sorted URIs, date-prefixed keys)

DELTA_BYTE_ARRAY stores how much each string shares with the previous one (prefix length) and writes only the differing suffix. For columns where strings are sorted — or nearly sorted — neighboring values share long prefixes, so only tiny byte differences are stored. A column of file paths like `/usr/local/bin/...` can compress to 20–30% of its PLAIN size before any codec is applied. The catch: your reader must support Parquet V2 data pages, and ZSTD on top gives the best combined ratio.

autoparq applies this when: `BYTE_ARRAY`, `cardinality_ratio >= 0.10`, `string_monotonicity_score >= 0.80`, `mean_len <= 50`, not UUID, not JSON.

**Engine notes:**

| Engine | Read | Write |
|--------|------|-------|
| Spark 3.3+ | ✓ | ✗ — write with PyArrow instead |
| DuckDB 1.2.0 | ✓ | ✗ — writer intentionally omits this encoding |
| ClickHouse | ✓ (test your version — PR #91929) | varies |
| PyArrow / Polars | ✓ | ✓ |

**PyArrow note:** Generated snippets include `use_dictionary=False` (or a selective column list) automatically. PyArrow enables dictionary by default and silently falls back to `RLE_DICTIONARY` without this flag.

## DELTA_LENGTH_BYTE_ARRAY

Best for: high-cardinality, variable-length string columns that are not sorted; short strings (mean length ≤ 50 bytes)

DELTA_LENGTH_BYTE_ARRAY is a two-part encoding: it delta-encodes the sequence of string lengths (so lengths of 12, 13, 12, 14 bytes become the differences 0, +1, −1, +2), then appends all the raw string bytes together in one block. Codecs like ZSTD can find patterns in the concatenated bytes more effectively than in interleaved length+value PLAIN encoding. The improvement is modest — roughly 2–3% additional size reduction on top of ZSTD — but consistent for high-cardinality short-string columns.

autoparq applies this when: `BYTE_ARRAY`, `cardinality_ratio >= 0.10`, `mean_len <= 50`, not UUID, not JSON (and not sorted — DELTA_BYTE_ARRAY fires first for sorted columns).

**Engine notes:**

| Engine | Read | Write |
|--------|------|-------|
| Spark 3.3+ | ✓ | ✗ — write with PyArrow instead |
| DuckDB 1.2.0 | ✓ | ✓ (V2 mode) |
| ClickHouse | ✓ (test your version — issue #44505) | varies |
| PyArrow / Polars | ✓ | ✓ |

**PyArrow note:** Same `use_dictionary=False` requirement as DELTA_BYTE_ARRAY above.

## PLAIN

Used for:
- UUID strings (dictionary would overflow)
- Columns with no detectable pattern
- Boolean columns (RLE is applied automatically)
- Long strings (mean length > 50 bytes) — delta encoding overhead exceeds the benefit

## Encoding priority

autoparq applies encoding rules in this order — first match wins:

1. `BOOLEAN` → `RLE` (automatic)
2. `INT32`/`INT64`/`TIMESTAMP`/`DATE` + `monotonicity_score >= 0.90` → `DELTA_BINARY_PACKED`
3. `cardinality_ratio < 0.10` AND dict fits in 512 KB → `RLE_DICTIONARY`
4. `FLOAT`/`DOUBLE` + `cardinality_ratio > 0.50` → `BYTE_STREAM_SPLIT`
5. `BYTE_ARRAY` + UUID pattern → `PLAIN`
6. `BYTE_ARRAY` + `cardinality_ratio >= 0.10` + `string_monotonicity_score >= 0.80` + `mean_len <= 50` + not UUID + not JSON → `DELTA_BYTE_ARRAY`
7. `BYTE_ARRAY` + `cardinality_ratio >= 0.10` + `mean_len <= 50` + not UUID + not JSON → `DELTA_LENGTH_BYTE_ARRAY`
8. All others → `PLAIN`

Rule 6 is a strict superset of Rule 7's conditions plus the sort-order check — a sorted column that qualifies for Rule 6 will always also qualify for Rule 7, so Rule 6 must come first.
