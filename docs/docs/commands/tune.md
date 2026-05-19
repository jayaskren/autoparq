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
