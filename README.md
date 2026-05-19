# autoparq

**autoparq** profiles Parquet files and recommends the best compression settings for your data and query engine. Instead of guessing, it measures your actual data characteristics — cardinality, sort order, entropy — and tells you exactly what to change and why.

## Why this exists

Parquet compression has two independent knobs: **encoding** (how values are stored) and **codec** (how bytes are compressed). Getting both right can reduce file size by 50–80% and speed up queries 2–5×. Getting them wrong wastes storage and slows everything down.

The catch: the optimal settings depend on what your data actually looks like. A column of 5 repeated status codes needs completely different settings than a column of random floating-point measurements. autoparq measures your data and applies evidence-based rules so you don't have to figure this out manually.

## Install

```bash
pip install autoparq
```

Requires Python 3.9+. Pre-built wheels are available for Linux, macOS, and Windows.

## Quick start

**Step 1 — Inspect your file:**
```bash
autoparq info events.parquet
```

**Step 2 — Get recommendations:**
```bash
autoparq tune events.parquet --engine duckdb --priority balanced
```

**Step 3 — Apply them:**
```bash
autoparq apply events.parquet --output events_tuned.parquet --engine duckdb
```

**Step 4 — Verify the improvement (optional):**
```bash
autoparq bench events.parquet --column status
```

---

## Commands

### `autoparq info` — inspect a file

Shows the schema, size, and compression stats for every column. Useful for understanding what you have before deciding how to compress it.

```
autoparq info <file> [--output text|json] [--columns COL1,COL2]
```

**Example:**
```
╭────────────────────── autoparq info ──────────────────────╮
│ events.parquet                                            │
│ Size: 2.2 MB  Parquet v2  Rows: 50,000                    │
│ Row groups: 1  Avg rows/group: 50,000                     │
╰───────────────────────────────────────────────────────────╯
  Column   Type      Encodings     Codec   Nulls   Min      Max
  ──────────────────────────────────────────────────────────────
  id       INT64     PLAIN         SNAPPY    0%    0        49999
  status   STRING    PLAIN, RLE    SNAPPY    0%    active   suspended
  score    DOUBLE    PLAIN         SNAPPY    0%    -100.0   100.0
  ts       TIMESTAMP PLAIN         SNAPPY    0%    2023-…   2024-…
```

---

### `autoparq tune` — get recommendations

Samples your data, measures it, and recommends the best encoding and codec for each column. Shows *why* each setting was chosen with the specific statistic that triggered it.

```
autoparq tune <file> [--engine ENGINE] [--priority size|speed|balanced]
                     [--explain brief|full] [--output text|json]
                     [--sample-rows N] [--min-improvement FLOAT]
```

| Flag | Default | What it does |
|------|---------|-------------|
| `--engine` | `unknown` | Tailors recommendations for your query engine. Options: `spark`, `duckdb`, `polars`, `clickhouse`, `pandas`, `unknown` |
| `--priority` | `balanced` | Optimize for `size` (smallest file), `speed` (fastest reads), or `balanced` |
| `--explain` | `brief` | Use `full` for a detailed breakdown of every rule evaluated |
| `--min-improvement` | `10.0` | Exit code 1 if predicted improvement exceeds this % — useful in CI pipelines |

**Example output:**
```
╭──────────────────────────── autoparq tune ─────────────────────────────╮
│ events.parquet                                                         │
│ Estimated impact: -13% size, 1.1x read speed (Medium confidence)      │
│ [estimated]                                                            │
╰────────────────────────────────────────────────────────────────────────╯
Column   Encoding              Codec    Conf    Impact
──────────────────────────────────────────────────────
id       DELTA_BINARY_PACKED   ZSTD:3   Med     ★★★☆☆
status   RLE_DICTIONARY        ZSTD:3   Med     ★★★★★
score    BYTE_STREAM_SPLIT     ZSTD:3   Med     ★★★☆☆
ts       DELTA_BINARY_PACKED   ZSTD:3   Med     ★★★☆☆

Why these settings:
  id → DELTA_BINARY_PACKED: monotonicity_score=1.000 >= threshold 0.90
  status → RLE_DICTIONARY: cardinality_ratio=0.0001 (5 distinct / 50000 rows)
  score → BYTE_STREAM_SPLIT: high-entropy float (cardinality_ratio=0.93 > 0.50)
  ts → DELTA_BINARY_PACKED: monotonicity_score=1.000 >= threshold 0.90
```

The output also includes:
- **Three ranked options** (Balanced / Smallest File / Fastest Reads) with estimated size and speed tradeoffs
- **Row group advisory** — whether your row group size is appropriate for your engine
- **Sort order advisory** — whether your file is sorted but missing sort metadata

**Use `--explain full`** to see the full reasoning chain for each column, including every rule that was evaluated and why it was accepted or rejected.

**Exit codes** (useful for CI):
- `0` — already well-compressed (improvement below `--min-improvement`)
- `1` — improvement available (improvement ≥ `--min-improvement`)
- `2` — error

```bash
# Fail CI if a file needs more than 20% improvement
autoparq tune events.parquet --engine spark --min-improvement 20
```

---

### `autoparq bench` — measure before committing

Benchmarks encoding and codec combinations in memory so you can see actual numbers before rewriting a large file.

```
autoparq bench <file> --column COL [--codecs zstd:3,lz4,snappy]
                      [--encodings PLAIN,DELTA_BINARY_PACKED]
                      [--measure read|write|size|all]
```

```
autoparq bench events.parquet --column status

               Bench: status (BYTE_ARRAY)
 Encoding        Codec    Compressed   Ratio   Write ms   Read ms
 ─────────────────────────────────────────────────────────────────
 PLAIN ★size     ZSTD:3   900 B        21.8x   10         5
 RLE_DICTIONARY  ZSTD:3   984 B        19.9x   19         5
 PLAIN           SNAPPY   3.1 KB       6.3x    12         9
```

Results are from an in-memory benchmark on the first row group. Actual I/O performance will differ, but the relative ordering is reliable.

---

### `autoparq apply` — rewrite the file

Rewrites the file with the recommended settings. Uses an atomic rename so a partial write never leaves a corrupt file.

```
autoparq apply <file> --output <out-file> [--in-place]
               [--engine ENGINE] [--priority size|speed|balanced]
```

```bash
# Write to a new file
autoparq apply events.parquet --output events_tuned.parquet --engine duckdb

# Overwrite in place
autoparq apply events.parquet --output events.parquet --in-place
```

**Safety rules:**
- Refuses to overwrite the input file unless `--in-place` is explicitly passed
- Refuses to overwrite any existing output file unless `--in-place` is passed

---

## What gets recommended and why

autoparq applies six encoding rules in priority order. The first rule that matches wins.

| Your data looks like… | Encoding recommended | Why |
|-----------------------|----------------------|-----|
| Auto-increment IDs, sequential timestamps | **DELTA_BINARY_PACKED** | Stores differences between values. For sorted integers, differences are tiny — compresses 3–10× better than storing full values |
| Status codes, categories, enums (few distinct values) | **RLE_DICTIONARY** | Stores each distinct value once, replaces all occurrences with a small integer index. If you have 5 statuses in 1M rows, the dictionary has 5 entries |
| Random floating-point measurements | **BYTE_STREAM_SPLIT** | Rearranges the bytes so similar byte positions are grouped together, giving the codec more to work with |
| UUIDs | **PLAIN** | Dictionary encoding would require storing every UUID, which is as large as the original data |
| Everything else | **PLAIN** | Safe baseline |

Codec is chosen after encoding:

| Condition | Codec |
|-----------|-------|
| Data appears pre-compressed (high byte entropy) | UNCOMPRESSED — compressing it would make the file *larger* |
| Engine is Spark (unversioned) | SNAPPY — works with all Spark versions; ZSTD requires 3.2+ |
| Priority = speed | LZ4 — fastest decompression |
| Priority = size | ZSTD:6 — maximum compression |
| Priority = balanced (default) | ZSTD:3 — good compression, fast reads |

---

## Confidence levels

autoparq tells you how confident it is in each recommendation:

| Level | Meaning |
|-------|---------|
| **High** | Sampled ≥10% of rows AND ≥100,000 rows — recommendation is reliable |
| **Medium** | Sampled ≥2% or ≥50,000 rows — likely correct, verify with bench for critical pipelines |
| **Low** | Small sample — treat as directional guidance, not a guarantee |

For small files, autoparq samples 100% of rows and may still show Medium because the absolute row count is low.

---

## Engine-specific behavior

Pass `--engine` to get recommendations that are safe for your stack:

```bash
autoparq tune events.parquet --engine spark
autoparq tune events.parquet --engine duckdb
autoparq tune events.parquet --engine clickhouse
```

| Engine | Notable behavior |
|--------|-----------------|
| `spark` | Defaults to SNAPPY (safe for all versions). ZSTD requires Spark 3.2+; you'll see a note if that would help |
| `clickhouse` | BROTLI and GZIP are automatically downgraded to ZSTD since ClickHouse doesn't support them for Parquet import |
| `duckdb`, `polars`, `pandas` | Full codec support; ZSTD:3 is the balanced default |

---

## Using the JSON output

Every command supports `--output json` for pipeline integration:

```bash
# Get recommendations as JSON
autoparq tune events.parquet --output json | jq '.columns[].recommended_encoding'

# Check if a file needs recompression in CI
autoparq tune events.parquet --output json | jq '.predicted_size_reduction_pct'

# Extract the copy-paste PyArrow snippet
autoparq tune events.parquet --output json | jq -r '.python_snippet'
```

The tune JSON output includes a ready-to-use PyArrow snippet:

```python
import pyarrow.parquet as pq

PARQUET_WRITE_OPTIONS = {
    "compression": "zstd",
    "compression_level": 3,
    "column_encoding": {
        "id": "DELTA_BINARY_PACKED",
        "status": "RLE_DICTIONARY",
    },
    "write_statistics": True,
}
pq.write_table(table, "events_tuned.parquet", **PARQUET_WRITE_OPTIONS)
```

---

## What autoparq doesn't do

- It doesn't read your entire file. Profiling reads a sample (default: 2M rows). Footer metadata is free.
- It doesn't guarantee the predicted improvement. Use `autoparq bench` on representative data to validate before rewriting terabytes.
- It doesn't restructure your schema or change column types.

---

## Web UI

autoparq also ships a browser-based UI. Drop a `.parquet` file onto the page and get the full analysis — all processing runs locally in WebAssembly, nothing is uploaded.

### Running the dev server

**Prerequisites:** Rust toolchain, `wasm-pack`, and Node.js 18+.

```bash
# Install wasm-pack if you don't have it
cargo install wasm-pack

# Install JS dependencies
cd web
npm install
```

**Option A — full build (builds WASM then starts Vite):**
```bash
cd web
npm run dev
```
This compiles the Rust core to WebAssembly (~2 min first time, cached after) then starts the Vite dev server at `http://localhost:5173`.

**Option B — skip WASM rebuild (faster, use existing `web/pkg/`):**
```bash
cd web
npm run dev:nowasm
```
Use this when you're only editing JavaScript/CSS and the WASM output is already built.

**Production build:**
```bash
cd web
npm run build        # builds WASM + bundles JS into web/dist/
npm run preview      # serves the production bundle locally
```

**Rebuild WASM only** (e.g. after changing Rust code):
```bash
cd web
npm run wasm:build   # release build
npm run wasm:build:dev  # debug build (larger, faster to compile)
```

### Architecture

The web UI is a static Vite app — no server, no backend. The Rust core is compiled to `web/pkg/` via `wasm-pack` and imported directly. Analysis runs entirely in the browser.

---

## Development

```bash
git clone https://github.com/YOUR_USERNAME/autoparq
cd autoparq
python -m venv .venv && source .venv/bin/activate
pip install maturin pytest
maturin develop
cargo run --example gen_fixtures
cargo test
pytest tests/python/
```

The core is written in Rust (Apache Arrow parquet crate + rayon for parallel profiling) and exposed to Python via PyO3. The CLI and rendering are pure Python (typer + rich).
