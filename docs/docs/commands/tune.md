# autoparq tune

Profile a Parquet file and recommend optimal encoding and codec settings.

```
autoparq tune <file> [--engine ENGINE] [--priority size|speed|balanced] [--explain brief|full] [--output text|json] [--sample-rows N] [--min-improvement FLOAT]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--engine` | `unknown` | Target query engine: `spark`, `duckdb`, `polars`, `clickhouse`, `pandas`, `unknown` |
| `--priority` | `balanced` | Optimization target: `size`, `speed`, `balanced` |
| `--explain` | `brief` | Explanation verbosity: `brief` or `full` |
| `--output` | `text` | Output format: `text` or `json` |
| `--sample-rows` | `2000000` | Max rows sampled per column |
| `--min-improvement` | `10.0` | Exit code 1 threshold (% predicted size reduction) |

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Predicted improvement < `--min-improvement` (already well-compressed) |
| `1` | Predicted improvement >= threshold (improvement available) |
| `2` | Error (file not found, unsupported format, I/O failure) |

## Examples

Tune for DuckDB:

```
autoparq tune events.parquet --engine duckdb --priority balanced
```

Tune for Spark with full explanations:

```
autoparq tune events.parquet --engine spark --explain full
```

Get JSON output and pipe to jq:

```
autoparq tune events.parquet --output json | jq '.columns[].recommended_encoding'
```

Use in CI — fail build if improvement > 20%:

```
autoparq tune events.parquet --min-improvement 20 || echo "File needs recompression"
```

## Delta string encodings

`DELTA_BYTE_ARRAY` and `DELTA_LENGTH_BYTE_ARRAY` may appear in `recommended_encoding` for `BYTE_ARRAY` columns. These are Parquet V2 encodings that target high-cardinality string columns where dictionary encoding does not apply.

**Example `reason_brief` values:**

- `DELTA_BYTE_ARRAY`: `string_monotonicity_score=0.923 >= threshold 0.80, mean_len=18, cardinality_ratio=0.4200`
- `DELTA_LENGTH_BYTE_ARRAY`: `cardinality_ratio=0.4200 >= 0.10 and mean_len=18 <= 50 — high-cardinality short strings`

**Why the generated PyArrow snippet includes `use_dictionary`:**

PyArrow enables dictionary encoding by default for all columns. If `use_dictionary` is not explicitly disabled, PyArrow silently ignores a `column_encoding` of `DELTA_BYTE_ARRAY` or `DELTA_LENGTH_BYTE_ARRAY` and falls back to `RLE_DICTIONARY`. The generated snippet handles this automatically:

- If no columns use `RLE_DICTIONARY`: `"use_dictionary": False`
- If some columns use `RLE_DICTIONARY` and others use delta string encodings: `"use_dictionary": ["col_a", "col_b"]` (list form — only the dictionary columns keep it enabled)

**Spark `[Warning]` caveat:**

When `--engine spark` is set and either delta string encoding fires, the output includes a warning:

> Requires Spark 3.3+ to read; Spark cannot write this encoding via the DataFrame API — write with PyArrow.

This is shown because Spark 3.3+ can read files with these encodings but cannot produce them via the DataFrame API. Use the PyArrow snippet to write the file, then read it with Spark normally.
