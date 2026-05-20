# Delta String Encodings — Product Requirements Document

**Status:** Draft  
**Version:** 0.1  
**Date:** 2026-05-18  

---

## 1. Overview

Two Parquet encodings are valid for `BYTE_ARRAY` columns but are not currently recommended, applied, or benchmarked by autoparq:

- **`DELTA_BYTE_ARRAY`** — encodes shared string prefixes as length deltas. Highly effective for sorted or nearly-sorted string columns (file paths, hierarchical IDs, sorted keys). NVIDIA benchmark: up to 80% file size reduction for prefix-heavy columns.
- **`DELTA_LENGTH_BYTE_ARRAY`** — encodes only the sequence of string lengths as deltas, then appends raw bytes concatenated. Effective for any high-cardinality, short, variable-length string column regardless of sort order. NVIDIA benchmark: ~2.9% additional size reduction on top of ZSTD.

This spec adds both encodings to the full autoparq pipeline: profiling, recommendation rules, apply/rewrite, bench, code generation, and documentation.

---

## 2. Goals

- Recommend `DELTA_BYTE_ARRAY` for sorted short string columns with high cardinality
- Recommend `DELTA_LENGTH_BYTE_ARRAY` for unsorted short string columns with high cardinality
- Apply both encodings correctly in `autoparq apply` (including disabling dictionary per column in the Parquet writer)
- Include both in the default bench encoding set for `BYTE_ARRAY` columns
- Generate correct PyArrow snippets (with required `use_dictionary` handling) and Spark snippets (with read-only caveat)
- Update all documentation: CLAUDE.md heuristics, engine compat matrix, encodings concept page, tune and bench command pages

## 3. Non-goals

- Support for `FIXED_LEN_BYTE_ARRAY` with these encodings (ClickHouse compatibility issue #62141)
- Auto-detecting whether the file was written in sort order (sort order metadata is advisory in Parquet; profiling samples rows and measures directly)
- Spark write support (Spark cannot write per-column encodings via the DataFrame API)

---

## 4. Design Decision: `string_monotonicity_score` as a Separate Field

Two approaches were considered for capturing lexicographic sort order of string columns:

**Option A:** Reuse the existing `monotonicity_score: Option<f64>` field — return `Some(score)` for `BYTE_ARRAY`/Utf8 columns.  
**Option B:** Add a new `string_monotonicity_score: Option<f64>` field to `ColumnProfile`.

**Decision: Option B (separate field).**

Rationale:
1. Option A is a JSON breaking change — existing consumers that pattern-match `"monotonicity_score": null` for `BYTE_ARRAY` columns would need to be updated.
2. The two fields have different threshold semantics (0.90 for integers, 0.80 for strings). Keeping them separate makes the `--explain full` output unambiguous.
3. `reason_brief` strings use the field name directly (`string_monotonicity_score=0.87 >= threshold 0.80`) — a separate name is clearer than the generic `monotonicity_score`.
4. The numeric `monotonicity_score` field continues to return `None` for `BYTE_ARRAY` columns exactly as today.

---

## 5. Section 1 — Profiling Changes

### 5.1 New `ColumnProfile` field

**File:** `src/profiler/stats.rs`

Add one field to `ColumnProfile`:

```rust
pub string_monotonicity_score: Option<f64>,
```

Position it after `monotonicity_score`. Update `#[derive(Serialize, Deserialize)]` (already present on the struct — no additional derive needed).

Full updated struct excerpt:

```rust
pub struct ColumnProfile {
    // ... existing fields ...
    pub monotonicity_score: Option<f64>,          // integers/timestamps only (unchanged)
    pub string_monotonicity_score: Option<f64>,   // NEW: BYTE_ARRAY/Utf8/LargeUtf8 only
    pub run_length_score: f64,
    // ... remaining existing fields unchanged ...
}
```

### 5.2 Computing `string_monotonicity_score`

**File:** `src/profiler/stats.rs`

Add a new private function:

```rust
fn string_monotonicity_score(array: &ArrayRef) -> Option<f64> {
    let values: Vec<Option<&str>> = match array.data_type() {
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>()?;
            (0..arr.len())
                .map(|i| if arr.is_null(i) { None } else { Some(arr.value(i)) })
                .collect()
        }
        _ => return None,
    };

    let mut ascending = 0u64;
    let mut total = 0u64;
    let mut prev: Option<&str> = None;
    for val in &values {
        if let (Some(p), Some(v)) = (prev, *val) {
            total += 1;
            if v >= p {
                ascending += 1;
            }
        }
        prev = if val.is_some() { *val } else { None };
    }

    if total == 0 {
        None
    } else {
        Some(ascending as f64 / total as f64)
    }
}
```

**Formula:** `score = ascending_pairs / total_valid_pairs`  
**Comparison:** Rust's default `str` ordering — lexicographic over UTF-8 byte values.  
**Null handling:** A null at either position of a pair excludes that pair from both numerator and denominator. A null resets `prev` to `None`.  
**Empty strings:** Valid values. `""` < any non-empty string under Rust's `str` ordering.  
**Returns `None`:** When the array type is not `Utf8`/`LargeUtf8`, or when fewer than two consecutive non-null values exist (i.e., `total == 0`).

### 5.3 Calling the new function in `profile_column`

In `profile_column()`, add the call alongside the existing `monotonicity_score` call:

```rust
let monotonicity_score = monotonicity_score(array);         // unchanged
let string_monotonicity_score = string_monotonicity_score(array);  // new
```

Include the new field in the returned `ColumnProfile`:

```rust
ColumnProfile {
    // ... existing fields ...
    monotonicity_score,
    string_monotonicity_score,   // new
    // ...
}
```

### 5.4 Fallback profile

In `tuner.rs::fallback_profile()`, set:

```rust
string_monotonicity_score: None,
```

### 5.5 Impact on confidence tier logic

`string_monotonicity_score` uses the same HIGH/MEDIUM/LOW sample thresholds as all other fields. No new tier logic is introduced.

**Boundary downgrade for `DeltaByteArray`:** If `rule_fired = DeltaByteArray` and `string_monotonicity_score` is in `[0.64, 0.96)` (within 20% of the 0.80 threshold), downgrade confidence to MEDIUM regardless of sample size. The recommendation is uncertain near the sort threshold.

### 5.6 Serialization note

`string_monotonicity_score` is serialized as `null` for non-BYTE_ARRAY columns and as a float for BYTE_ARRAY columns. The existing `monotonicity_score` field is **unchanged** — it continues to return `null` for BYTE_ARRAY columns, exactly as today. Existing JSON consumers are not broken.

---

## 6. Section 2 — Recommendation Rules

### 6.1 Updated rule priority order

| Priority | Rule Name | Encoding | Fires When |
|----------|-----------|----------|-----------|
| 1 | BooleanRle | RLE | `physical_type == BOOLEAN` |
| 2 | DeltaBinaryPacked | DELTA_BINARY_PACKED | INT32/INT64/TIMESTAMP/DATE + `monotonicity_score >= 0.90` |
| 3 | RleDictionary | RLE_DICTIONARY | `cardinality_ratio < 0.10` AND dict fits in 512 KB |
| 4 | ByteStreamSplit | BYTE_STREAM_SPLIT | FLOAT/DOUBLE + `cardinality_ratio > 0.50` |
| 5 | PlainUuid | PLAIN | BYTE_ARRAY + UUID detected |
| **6** | **DeltaByteArray** | **DELTA_BYTE_ARRAY** | **see 6.2** |
| **7** | **DeltaLengthByteArray** | **DELTA_LENGTH_BYTE_ARRAY** | **see 6.3** |
| 8 | PlainDefault | PLAIN | catch-all |

**Ordering rationale:** Rule 6 is strictly more specific than Rule 7 — it requires all of Rule 7's conditions plus `string_monotonicity_score >= 0.80`. A column satisfying Rule 6 would also satisfy Rule 7; Rule 6 must come first to claim the higher-payoff encoding for sorted columns.

### 6.2 Rule 6: DeltaByteArray

**Encoding:** `DELTA_BYTE_ARRAY`

**All conditions must be true:**

| Field | Condition | Threshold |
|-------|-----------|-----------|
| `physical_type` | == | `"BYTE_ARRAY"` |
| `cardinality_ratio` | >= | `0.10` |
| `string_monotonicity_score` | >= | `0.80` |
| `string_length_stats.mean_len` | <= | `50.0` |
| `uuid_pattern_detected` | == | `false` |
| `json_pattern_detected` | == | `false` |

**Threshold rationale:**
- `string_monotonicity_score >= 0.80`: Lower than the integer threshold (0.90) to catch partially-sorted columns (log prefixes, date-prefixed IDs, path strings) where prefix sharing begins to outperform PLAIN+ZSTD. Do not reuse 0.90 — string entropy profiles differ from integer monotonicity.
- `mean_len <= 50`: DELTA_BYTE_ARRAY per-value overhead exceeds compression benefit above ~50 bytes. Use bytes (not characters).
- `cardinality_ratio >= 0.10`: Ensures Rule 3 (RleDictionary) did not fire.

**Engine compatibility caveats to emit:**

| Engine | Severity | Message |
|--------|----------|---------|
| Spark | Warning | "Requires Spark 3.3+ to read; Spark cannot write this encoding via the DataFrame API — write with PyArrow." |
| DuckDB | Warning | "DuckDB 1.2.0 can read DELTA_BYTE_ARRAY but its Parquet writer does not produce it. Write with PyArrow if you need to produce files DuckDB can read." |
| ClickHouse | Warning | "DELTA_BYTE_ARRAY had a repetitive-string decoding bug (fixed in ClickHouse PR #91929). Test on your ClickHouse version before deploying." |

**Confidence boundary downgrade:** If `string_monotonicity_score` is in `[0.64, 0.96)`, downgrade to MEDIUM regardless of sample size.

**`reason_brief` format:**  
`"string_monotonicity_score={:.3} >= threshold 0.80, mean_len={:.0}, cardinality_ratio={:.4}"`

**Unit tests (fires / does not fire):**

| Test | Conditions | Expected |
|------|-----------|----------|
| `test_rule6_fires_sorted_strings` | BYTE_ARRAY, cardinality_ratio=0.30, string_monotonicity=0.92, mean_len=20, uuid=false, json=false | DELTA_BYTE_ARRAY |
| `test_rule6_fires_at_threshold` | string_monotonicity=0.80, all else met | DELTA_BYTE_ARRAY |
| `test_rule6_no_fire_low_monotonicity` | string_monotonicity=0.79 | does not fire |
| `test_rule6_no_fire_uuid` | string_monotonicity=0.95, uuid=true | PlainUuid fires (Rule 5) |
| `test_rule6_no_fire_long_strings` | string_monotonicity=0.90, mean_len=51 | does not fire |
| `test_rule6_no_fire_low_cardinality` | cardinality_ratio=0.09, string_monotonicity=0.95 | RleDictionary fires (Rule 3) |
| `test_rule6_no_fire_json` | json_pattern_detected=true, string_monotonicity=0.90 | does not fire |
| `test_rule6_confidence_boundary_downgrade` | string_monotonicity=0.82, sample_fraction=0.50, sample_rows=500_000 | fires, confidence=MEDIUM |
| `test_rule6_confidence_high` | string_monotonicity=0.97, sample_fraction=0.15, sample_rows=200_000 | fires, confidence=HIGH |

### 6.3 Rule 7: DeltaLengthByteArray

**Encoding:** `DELTA_LENGTH_BYTE_ARRAY`

**All conditions must be true:**

| Field | Condition | Threshold |
|-------|-----------|-----------|
| `physical_type` | == | `"BYTE_ARRAY"` |
| `cardinality_ratio` | >= | `0.10` |
| `string_length_stats.mean_len` | <= | `50.0` |
| `uuid_pattern_detected` | == | `false` |
| `json_pattern_detected` | == | `false` |

`string_monotonicity_score` is **not** a precondition. This rule fires for high-cardinality short strings regardless of sort order.

**Threshold rationale:**
- `cardinality_ratio >= 0.10`: Same as Rule 6. Low-cardinality columns are handled by Rule 3.
- `mean_len <= 50`: DELTA_LENGTH_BYTE_ARRAY's length-delta overhead amortizes poorly for long strings.

**Engine compatibility caveats to emit:**

| Engine | Severity | Message |
|--------|----------|---------|
| Spark | Warning | "Requires Spark 3.3+ to read; Spark cannot write this encoding via the DataFrame API — write with PyArrow." |
| ClickHouse | Warning | "DELTA_LENGTH_BYTE_ARRAY had compatibility issues in older ClickHouse versions (issue #44505). Test on your ClickHouse version before deploying." |

No DuckDB caveat — DuckDB 1.2.0 supports read and write for DELTA_LENGTH_BYTE_ARRAY.

**Confidence boundary downgrade:** None. No near-threshold uncertainty for this rule.

**`reason_brief` format:**  
`"cardinality_ratio={:.4} >= 0.10 and mean_len={:.0} <= 50 — high-cardinality short strings"`

**Unit tests:**

| Test | Conditions | Expected |
|------|-----------|----------|
| `test_rule7_fires_basic` | BYTE_ARRAY, cardinality_ratio=0.40, string_monotonicity=Some(0.50), mean_len=15, uuid=false, json=false | DELTA_LENGTH_BYTE_ARRAY |
| `test_rule7_fires_null_monotonicity` | cardinality_ratio=0.30, string_monotonicity=None, mean_len=12 | DELTA_LENGTH_BYTE_ARRAY |
| `test_rule7_fires_at_cardinality_boundary` | cardinality_ratio=0.10, mean_len=20 | DELTA_LENGTH_BYTE_ARRAY |
| `test_rule7_no_fire_low_cardinality` | cardinality_ratio=0.09 | RleDictionary fires (Rule 3) |
| `test_rule7_no_fire_long_strings` | mean_len=51 | PlainDefault |
| `test_rule7_no_fire_uuid` | uuid=true, mean_len=36 | PlainUuid fires (Rule 5) |
| `test_rule7_no_fire_json` | json_pattern_detected=true | does not fire |
| `test_rule7_sorted_falls_to_rule6` | cardinality_ratio=0.30, string_monotonicity=0.85, mean_len=20 | Rule 6 fires (DELTA_BYTE_ARRAY) |
| `test_rule7_confidence_high` | sample_fraction=0.15, sample_rows=200_000 | fires, confidence=HIGH |
| `test_rule7_confidence_low` | sample_fraction=0.005, sample_rows=10_000 | fires, confidence=LOW |

---

## 7. Section 3 — Apply, Bench, and Code Generation Changes

### 7.1 apply.rs — `parse_encoding_for_writer`

Add two new match arms to `parse_encoding_for_writer` in `src/apply.rs`:

```rust
"DELTA_LENGTH_BYTE_ARRAY" => Ok(Encoding::DELTA_LENGTH_BYTE_ARRAY),
"DELTA_BYTE_ARRAY"        => Ok(Encoding::DELTA_BYTE_ARRAY),
```

Both `Encoding::DELTA_LENGTH_BYTE_ARRAY` and `Encoding::DELTA_BYTE_ARRAY` are present in the `parquet` crate's `Encoding` enum. No new dependency required.

**Writer dictionary flag:** The existing branch structure in `rewrite_file` and `rewrite_file_from_bytes` already handles this correctly:

```rust
if encoding == Encoding::RLE_DICTIONARY {
    builder = builder.set_column_dictionary_enabled(col_path.clone(), true);
} else {
    builder = builder
        .set_column_dictionary_enabled(col_path.clone(), false)
        .set_column_encoding(col_path.clone(), encoding);
}
```

Because neither new encoding is `RLE_DICTIONARY`, they land in the `else` branch which calls `set_column_dictionary_enabled(..., false)` before setting the encoding. No additional change needed — the fix is complete once the match arms are added. Apply identically to both `rewrite_file` and `rewrite_file_from_bytes`.

### 7.2 bench.rs — `valid_encodings_for_type` and `parse_encoding`

**`valid_encodings_for_type`** — update the `"BYTE_ARRAY"` arm:

```rust
"BYTE_ARRAY" => vec![
    "PLAIN".into(),
    "RLE_DICTIONARY".into(),
    "DELTA_LENGTH_BYTE_ARRAY".into(),
    "DELTA_BYTE_ARRAY".into(),
],
```

**`parse_encoding`** — add the same two arms as in apply.rs:

```rust
"DELTA_LENGTH_BYTE_ARRAY" => Ok(Encoding::DELTA_LENGTH_BYTE_ARRAY),
"DELTA_BYTE_ARRAY"        => Ok(Encoding::DELTA_BYTE_ARRAY),
```

The `else` branch in the bench writer loop already calls `set_column_dictionary_enabled(..., false)` for non-RLE_DICTIONARY encodings, identical to apply.rs. No additional change needed.

**Small-sample caveat:** Add an optional `caveats: Vec<String>` field to `BenchResult`. After taking the sample in `benchmark_column` and `benchmark_column_from_bytes`, populate it when the sample is small:

```rust
let mut caveats: Vec<String> = Vec::new();
if sample.physical_type == "BYTE_ARRAY" && sample.array.len() < 10_000 {
    caveats.push(
        "Small sample (<10,000 rows): DELTA_BYTE_ARRAY ratio may not reflect full-file behavior.".into()
    );
}
```

### 7.3 codegen.rs — PyArrow snippet

**File:** `src/codegen.rs`

Define a module-level constant (shared between `generate_pyarrow` and `generate_pyspark`):

```rust
const DELTA_STRING_ENCODINGS: &[&str] = &["DELTA_LENGTH_BYTE_ARRAY", "DELTA_BYTE_ARRAY"];
```

In `generate_pyarrow`, after emitting the `column_encoding` dict block and before `write_statistics`, classify columns:

```rust
let delta_string_cols: Vec<&str> = report.columns.iter()
    .filter(|c| DELTA_STRING_ENCODINGS.contains(&c.recommended_encoding.as_str()))
    .map(|c| c.column_name.as_str())
    .collect();

let dict_cols: Vec<&str> = report.columns.iter()
    .filter(|c| c.recommended_encoding == "RLE_DICTIONARY")
    .map(|c| c.column_name.as_str())
    .collect();
```

Then emit `use_dictionary` when needed (Approach B — selective, preserving dict for columns that need it):

```rust
if !delta_string_cols.is_empty() {
    if dict_cols.is_empty() {
        lines.push("    # PyArrow silently ignores DELTA_*_BYTE_ARRAY column_encoding without this.".into());
        lines.push("    \"use_dictionary\": False,".into());
    } else {
        let col_list = dict_cols.iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push("    # Disable dictionary globally except for columns using RLE_DICTIONARY.".into());
        lines.push("    # PyArrow silently ignores DELTA_*_BYTE_ARRAY column_encoding without this.".into());
        lines.push(format!("    \"use_dictionary\": [{}],", col_list));
    }
}
```

**Generated output example (mixed encodings):**

```python
PARQUET_WRITE_OPTIONS = {
    "compression": "zstd",
    "compression_level": 3,
    "column_encoding": {
        "tags": "DELTA_BYTE_ARRAY",
        "status": "RLE_DICTIONARY",
    },
    # Disable dictionary globally except for columns using RLE_DICTIONARY.
    # PyArrow silently ignores DELTA_*_BYTE_ARRAY column_encoding without this.
    "use_dictionary": ["status"],
    "write_statistics": True,
}
pq.write_table(table, "output.parquet", **PARQUET_WRITE_OPTIONS)
```

### 7.4 codegen.rs — Spark snippet

In `generate_pyspark`, when `delta_string_cols` is non-empty, prepend a warning comment before the spark session configuration:

```python
# WARNING: The following columns were recommended delta string encodings
# (DELTA_BYTE_ARRAY or DELTA_LENGTH_BYTE_ARRAY): tags
# Spark 3.3+ can READ these encodings but cannot WRITE them via the DataFrame API.
# Write this file using the PyArrow snippet instead, then read it with Spark normally.
```

### 7.5 Python codegen.py

No changes required. `python/autoparq/codegen.py` delegates to `_lib.py_generate_snippet` which calls the Rust codegen. All changes flow through automatically.

### 7.6 wasm.rs

No changes required. All three WASM-exposed paths pick up the underlying changes automatically:
- `generate_snippet` → calls `crate::codegen::generate_snippet` (picks up 7.3/7.4)
- `apply_file_bytes` → calls `rewrite_file_from_bytes` → calls `parse_encoding_for_writer` (picks up 7.1)
- `bench_column_bytes` → calls `valid_encodings_for_type` (picks up 7.2)

---

## 8. Section 4 — Documentation Updates

### 8.1 CLAUDE.md — Recommendation Heuristics (full replacement)

Replace the "Recommendation Heuristics (source of truth)" section entirely:

```
### Encoding rules — apply in priority order, first match wins

1. `BOOLEAN` → `RLE` *(automatic in all libraries; noted in output)*
2. `INT32/INT64` (physical type) OR `TIMESTAMP`/`DATE` (logical type) + `monotonicity_score >= 0.90` → `DELTA_BINARY_PACKED`
3. `(any type)` + `cardinality_ratio < 0.10` AND `cardinality_estimate × avg_value_bytes < 524_288` (512 KB) → `RLE_DICTIONARY`
4. `FLOAT/DOUBLE` + `cardinality_ratio > 0.50` → `BYTE_STREAM_SPLIT`
5. `BYTE_ARRAY` + UUID pattern detected → `PLAIN` *(dictionary overflow avoidance)*
6. `BYTE_ARRAY` + `cardinality_ratio >= 0.10` AND `string_monotonicity_score >= 0.80` AND `mean_len <= 50` AND not UUID AND not JSON → `DELTA_BYTE_ARRAY`
7. `BYTE_ARRAY` + `cardinality_ratio >= 0.10` AND `mean_len <= 50` AND not UUID AND not JSON → `DELTA_LENGTH_BYTE_ARRAY`
8. All others → `PLAIN`
```

Add notes:
- Rule 6 is evaluated before Rule 7. Rule 6 is a strict superset of Rule 7's conditions plus `string_monotonicity_score >= 0.80`. A column satisfying Rule 6 will always also satisfy Rule 7; Rule 6 wins.
- Both Rules 6 and 7: the generated PyArrow snippet must pass `use_dictionary=False` (global) or a per-column list. PyArrow enables dictionary by default; omitting this silently falls back to RLE_DICTIONARY.
- Rule 6 Spark write caveat: Spark cannot write DELTA_BYTE_ARRAY via the DataFrame API. Spark 3.3+ can read it.
- Rule 7 DuckDB write note: DuckDB 1.2.0 can write DELTA_LENGTH_BYTE_ARRAY in V2 mode.

### 8.2 CLAUDE.md — Profiling implementation details

Add to the "Monotonicity / run-length scores" subsection:

- **String monotonicity** (`string_monotonicity_score`): New field. Applies to `BYTE_ARRAY`/`Utf8`/`LargeUtf8` columns only. Uses lexicographic `<=` comparison: `score = fraction of consecutive non-null pairs where value[i] >= value[i-1]`. Returns `None` for non-BYTE_ARRAY types. Nulls in either position of a pair exclude the pair from both numerator and denominator. Empty strings are valid values.
- The existing `monotonicity_score` field is **unchanged** — still `None` for BYTE_ARRAY columns.
- Threshold for Rule 6 (string): `>= 0.80` (inclusive). Boundary downgrade: if score is in `[0.64, 0.96)`, confidence is MEDIUM regardless of sample size.

### 8.3 CLAUDE.md — Engine Compatibility Matrix

Add a new encoding compatibility table below the existing codec table:

```
**Encoding compatibility:**

| Encoding | Spark | DuckDB | ClickHouse | Polars/PyArrow |
|----------|-------|--------|------------|----------------|
| DELTA_BINARY_PACKED | 3.2+ read+write | read+write | read+write | read+write |
| DELTA_BYTE_ARRAY | 3.3+ read only | read only (1.2.0) | Warning (PR #91929) | read+write |
| DELTA_LENGTH_BYTE_ARRAY | 3.3+ read only | read+write (V2 mode) | Warning (issue #44505) | read+write |
```

Append to the "Known bugs to warn on" list:

- ClickHouse + `DELTA_BYTE_ARRAY` → repetitive-string decoding bug fixed in PR #91929. Emit Warning caveat.
- ClickHouse + `DELTA_LENGTH_BYTE_ARRAY` → compatibility issues fixed around issue #44505. Emit Warning caveat.
- DuckDB + `DELTA_BYTE_ARRAY` (write) → DuckDB 1.2.0 intentionally omits this from its writer. Emit Warning caveat for write path.
- Spark + `DELTA_BYTE_ARRAY` or `DELTA_LENGTH_BYTE_ARRAY` (write) → Spark cannot write these via the DataFrame API. The Spark snippet must note "read compatible Spark 3.3+; write with PyArrow".

### 8.4 docs/docs/concepts/encodings.md

Add two new sections between `BYTE_STREAM_SPLIT` and `PLAIN`.

**DELTA_BYTE_ARRAY section:**
- Best for: sorted or nearly-sorted string columns with shared prefixes (file paths, hierarchical IDs, sorted URIs, date-prefixed keys)
- What triggers it: `BYTE_ARRAY`, `cardinality_ratio >= 0.10`, `string_monotonicity_score >= 0.80`, `mean_len <= 50`, not UUID, not JSON
- Teach yourself: "DELTA_BYTE_ARRAY stores how much each string shares with the previous one (prefix length) and writes only the differing suffix. For columns where strings are sorted — or nearly sorted — neighboring values share long prefixes, so only tiny byte differences are stored. A column of file paths like `/usr/local/bin/...` can compress to 20–30% of its PLAIN size before any codec is applied. The catch: your reader must support Parquet V2 data pages, and ZSTD on top gives the best combined ratio."
- Engine notes: Spark 3.3+ read only; DuckDB read only (1.2.0 writer omits it); PyArrow read+write; ClickHouse — test version (PR #91929 bug fix)
- PyArrow note: requires `use_dictionary=False` — generated snippet includes this automatically

**DELTA_LENGTH_BYTE_ARRAY section:**
- Best for: high-cardinality, variable-length string columns that are not sorted; short strings (mean <= 50 bytes)
- What triggers it: `BYTE_ARRAY`, `cardinality_ratio >= 0.10`, `mean_len <= 50`, not UUID, not JSON (and not sorted — DELTA_BYTE_ARRAY fires first for sorted columns)
- Teach yourself: "DELTA_LENGTH_BYTE_ARRAY is a two-part encoding: it delta-encodes the sequence of string lengths (so lengths of 12, 13, 12, 14 bytes become the differences 0, +1, −1, +2), then appends all the raw string bytes together in one block. Codecs like ZSTD can find patterns in the concatenated bytes more effectively than in interleaved length+value PLAIN encoding. The improvement is modest — roughly 2–3% additional size reduction on top of ZSTD per the NVIDIA benchmark — but consistent for high-cardinality short-string columns."
- Engine notes: Spark 3.3+ read only; DuckDB read+write (V2 mode); PyArrow read+write; ClickHouse — test version (issue #44505)
- PyArrow note: requires `use_dictionary=False` — generated snippet includes this automatically

Update the encoding priority list at the bottom of the page to show all 8 rules in order.

### 8.5 docs/docs/commands/tune.md

Add a section after the Examples block explaining:
- DELTA_BYTE_ARRAY and DELTA_LENGTH_BYTE_ARRAY may now appear in `recommended_encoding` for BYTE_ARRAY columns
- Example `reason_brief` strings for each rule
- Why generated PyArrow snippets include `use_dictionary=False` for these encodings
- Spark `[Warning]` caveat is shown when either encoding fires with `--engine spark`

### 8.6 docs/docs/commands/bench.md

Update:
- `--encodings` flag description: default for BYTE_ARRAY columns is now `PLAIN,RLE_DICTIONARY,DELTA_BYTE_ARRAY,DELTA_LENGTH_BYTE_ARRAY`
- Add default encoding set table by physical type
- Note that bench automatically handles `use_dictionary=False` — no manual flag needed

---

## 9. Acceptance Criteria

### Profiling
- [ ] `ColumnProfile` has a `string_monotonicity_score: Option<f64>` field, serialized in JSON
- [ ] Returns `None` for INT64/INT32/FLOAT/DOUBLE/BOOLEAN columns
- [ ] Returns `Some(1.0)` for a perfectly sorted string column
- [ ] Returns `Some(0.0)` for a perfectly reverse-sorted string column
- [ ] Returns `None` when fewer than two consecutive non-null strings exist in the sample
- [ ] Existing `monotonicity_score` returns `None` for BYTE_ARRAY (unchanged)

### Recommendation rules
- [ ] All 10 unit tests in Section 6.2 pass for DeltaByteArray
- [ ] All 10 unit tests in Section 6.3 pass for DeltaLengthByteArray
- [ ] Sorted fixture file triggers DELTA_BYTE_ARRAY for its string columns in `autoparq tune`
- [ ] High-cardinality unsorted short string fixture triggers DELTA_LENGTH_BYTE_ARRAY

### Apply
- [ ] `autoparq apply` produces a valid Parquet file when recommended encoding is DELTA_BYTE_ARRAY or DELTA_LENGTH_BYTE_ARRAY
- [ ] The output file can be read back by PyArrow without error

### Bench
- [ ] `autoparq bench --column <byte_array_col>` includes DELTA_BYTE_ARRAY and DELTA_LENGTH_BYTE_ARRAY in results
- [ ] No error when benchmarking these encodings on any sample size

### Code generation
- [ ] PyArrow snippet includes `use_dictionary: False` when any column has DELTA_BYTE_ARRAY or DELTA_LENGTH_BYTE_ARRAY
- [ ] PyArrow snippet uses `use_dictionary: ["col1", "col2"]` (list form) when other columns use RLE_DICTIONARY
- [ ] Spark snippet includes the write-incompatibility warning when either encoding fires
- [ ] Browser (WASM) code panel shows correct snippets

### Documentation
- [ ] CLAUDE.md heuristics section shows exactly 8 rules; rules 6 and 7 match the conditions in Section 6 precisely
- [ ] Engine compatibility table in CLAUDE.md has rows for DELTA_BYTE_ARRAY and DELTA_LENGTH_BYTE_ARRAY
- [ ] ClickHouse warnings reference issue #44505 (DELTA_LENGTH) and PR #91929 (DELTA_BYTE)
- [ ] encodings.md has sections for both new encodings with `teach_yourself` paragraphs
- [ ] tune.md explains the `use_dictionary=False` requirement in generated snippets
- [ ] bench.md default encoding set table is updated

---

## 10. Open Questions

None — all questions from the initial research phase were resolved by the agent research:
- Both encodings are worth adding (confirmed by NVIDIA benchmark data)
- `string_monotonicity_score` as a separate field (resolved in Section 4)
- Threshold values: 0.80 for DELTA_BYTE_ARRAY, no threshold for DELTA_LENGTH_BYTE_ARRAY (confirmed)
- FIXED_LEN_BYTE_ARRAY excluded (ClickHouse issue #62141)
- PyArrow `use_dictionary` approach: selective list form (Approach B)
