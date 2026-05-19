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

BYTE_STREAM_SPLIT deinterleaves the bytes of floating-point values — writing all MSBs together, then all next bytes, etc. This groups similar bytes, improving codec compression by 10-30%.

## PLAIN

Used for:
- UUID strings (dictionary would overflow)
- Columns with no detectable pattern
- Boolean columns (RLE is applied automatically)

## Encoding priority

autoparq applies encoding rules in this order — first match wins:

1. BOOLEAN -> RLE (automatic)
2. INT32/INT64/TIMESTAMP + monotonicity >= 0.90 -> DELTA_BINARY_PACKED
3. cardinality_ratio < 0.10 AND dict fits in 512KB -> RLE_DICTIONARY
4. FLOAT/DOUBLE + cardinality_ratio > 0.50 -> BYTE_STREAM_SPLIT
5. BYTE_ARRAY + UUID pattern -> PLAIN
6. All others -> PLAIN
