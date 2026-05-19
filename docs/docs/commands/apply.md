# autoparq apply

Rewrite a Parquet file with recommended encoding and codec settings.

```
autoparq apply <file> --output <out-file> [--in-place] [--engine ENGINE] [--priority size|speed|balanced]
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--output` | required | Destination file path |
| `--in-place` | `false` | Allow output path to equal input path |
| `--engine` | `unknown` | Target engine |
| `--priority` | `balanced` | Optimization priority |
| `--sample-rows` | `500000` | Rows sampled for recommendations |

## Safety

- Refuses to overwrite the input file unless `--in-place` is passed
- Refuses to overwrite an existing output file unless `--in-place` is passed
- Uses atomic rename (write to temp file, then rename) to avoid partial writes

## Examples

Apply recommendations to a new file:

```
autoparq apply events.parquet --output events_tuned.parquet --engine duckdb
```

In-place rewrite:

```
autoparq apply events.parquet --output events.parquet --in-place
```

Apply with Spark optimizations:

```
autoparq apply events.parquet --output events_spark.parquet --engine spark --priority balanced
```
