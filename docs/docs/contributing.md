# Contributing

## Development setup

Prerequisites: Rust (stable), Python 3.9+, maturin 1.7+

Clone and set up:

```
git clone https://github.com/YOUR_USERNAME/autoparq
cd autoparq
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest
```

Build and install in development mode:

```
maturin develop
```

Generate test fixtures:

```
cargo run --example gen_fixtures
```

## Running tests

Rust unit tests:

```
cargo test
```

Integration tests (requires fixtures):

```
cargo test -- --ignored
```

Python CLI tests:

```
pytest tests/python/ -v
```

Snapshot tests:

```
cargo insta test
```

Review changed snapshots:

```
cargo insta review
```

## Code structure

- `src/` — Rust core (profiler, recommender, tuner, bench, apply)
- `python/autoparq/` — Python CLI and rendering
- `tests/integration/` — Rust integration tests
- `tests/python/` — Python subprocess tests
- `benches/` — Criterion benchmarks
- `examples/gen_fixtures.rs` — Test fixture generator
