# autoparq

autoparq profiles Parquet files and recommends optimal encoding and codec settings based on measured data characteristics.

## Install

```
pip install autoparq
```

## 30-Second Quickstart

Inspect a file:

```
autoparq info data.parquet
```

Get recommendations:

```
autoparq tune data.parquet --engine duckdb --priority balanced
```

Apply the recommendations:

```
autoparq apply data.parquet --output data_tuned.parquet
```

Validate with a benchmark:

```
autoparq bench data.parquet --column my_column
```

## Why autoparq?

Parquet compression involves two independent levers: encoding (how values are stored) and codec (how bytes are compressed). Choosing the right combination requires understanding your data's cardinality, monotonicity, and entropy — characteristics that are hard to reason about manually.

autoparq measures these statistics and applies evidence-based heuristics to recommend the best settings for your engine and use case. Every recommendation cites the specific statistic that triggered it.
