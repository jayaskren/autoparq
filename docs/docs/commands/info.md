# autoparq info

Display column-level metadata for a Parquet file.

```
autoparq info <file> [--output text|json] [--columns COL1,COL2]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--output` | `text` | Output format: `text` or `json` |
| `--columns` | all | Comma-separated column names to display |

## Output columns

| Column | Description |
|--------|-------------|
| Column | Column name |
| Physical Type | Parquet physical type (INT64, BYTE_ARRAY, etc.) |
| Logical Type | Logical annotation (STRING, TIMESTAMP, etc.) |
| Encodings | Current encodings in the file |
| Codec | Compression codec |
| Nulls | Null percentage |
| Min | Minimum value (from footer statistics) |
| Max | Maximum value (from footer statistics) |
| Compressed | Compressed size across all row groups |
| Uncompressed | Uncompressed size |
| Ratio | Compression ratio |

## Examples

Basic usage:

```
autoparq info events.parquet
```

JSON output:

```
autoparq info events.parquet --output json | jq '.columns[].name'
```

Select specific columns:

```
autoparq info events.parquet --columns user_id,timestamp,status
```
