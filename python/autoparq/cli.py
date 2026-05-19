import json
import sys
import typer
from typing import Annotated, Optional
from pathlib import Path
from enum import Enum

from autoparq import _lib
from autoparq import render as _render
from autoparq.render import _human_bytes

app = typer.Typer(name="autoparq", no_args_is_help=True, help="Parquet compression analyzer and tuner.")

class OutputFormat(str, Enum):
    text = "text"
    json = "json"

class Priority(str, Enum):
    size = "size"
    speed = "speed"
    balanced = "balanced"

class Engine(str, Enum):
    spark = "spark"
    duckdb = "duckdb"
    polars = "polars"
    clickhouse = "clickhouse"
    pandas = "pandas"
    unknown = "unknown"

@app.command("info")
def info(
    file: Path = typer.Argument(..., help="Parquet file to inspect"),
    output: OutputFormat = typer.Option(OutputFormat.text, "--output", "-o", help="Output format: text or json"),
    columns: Optional[str] = typer.Option(None, "--columns", help="Comma-separated column names to show"),
) -> None:
    """Print file and column metadata from the Parquet footer (no row scanning)."""
    try:
        columns_filter = [c.strip() for c in columns.split(",")] if columns else None
        raw = _lib.py_info_file(str(file), columns_filter)
        if output == OutputFormat.json:
            print(raw)
            raise typer.Exit(0)
        profile = json.loads(raw)
        _render.render_info(profile)
        raise typer.Exit(0)
    except typer.Exit:
        raise
    except Exception as e:
        typer.echo(f"Error: {e}", err=True)
        raise typer.Exit(2)


@app.command("tune")
def tune(
    file: Path = typer.Argument(..., help="Parquet file to analyze"),
    engine: Engine = typer.Option(Engine.unknown, "--engine", help="Target query engine"),
    priority: Priority = typer.Option(Priority.balanced, "--priority", help="Optimization target: size, speed, or balanced"),
    explain: str = typer.Option("brief", "--explain", help="Explanation verbosity: brief or full"),
    output: OutputFormat = typer.Option(OutputFormat.text, "--output", "-o"),
    sample_rows: int = typer.Option(2_000_000, "--sample-rows"),
    min_improvement: float = typer.Option(10.0, "--min-improvement"),
) -> None:
    """Analyze a Parquet file and recommend optimal compression settings."""
    try:
        raw = _lib.py_tune_file(str(file), engine.value, priority.value, sample_rows, explain)
        report = json.loads(raw)

        # Enrich report with Python/Spark snippets
        from autoparq.codegen import generate_python_snippet, generate_spark_snippet
        report["python_snippet"] = generate_python_snippet(report)
        report["spark_snippet"] = generate_spark_snippet(report)

        if output == OutputFormat.json:
            print(json.dumps(report))
        else:
            _render.render_tune_text(report, explain)

        if report["predicted_size_reduction_pct"] >= min_improvement:
            raise typer.Exit(1)
        raise typer.Exit(0)
    except typer.Exit:
        raise
    except Exception as e:
        typer.echo(f"Error: {e}", err=True)
        raise typer.Exit(2)


@app.command("bench")
def bench(
    file: Annotated[Path, typer.Argument(help="Parquet file to benchmark")],
    column: Annotated[str, typer.Option("--column", help="Column name to benchmark")],
    codecs: Annotated[Optional[str], typer.Option("--codecs", help="Comma-separated: zstd:3,lz4,snappy")] = None,
    encodings: Annotated[Optional[str], typer.Option("--encodings", help="Comma-separated: PLAIN,DELTA_BINARY_PACKED")] = None,
    measure: Annotated[str, typer.Option("--measure", help="read, write, size, or all")] = "all",
    output: Annotated[OutputFormat, typer.Option("--output", "-o")] = OutputFormat.text,
) -> None:
    """Benchmark encoding and codec combinations for a single column."""
    try:
        if codecs:
            pairs = []
            for c in codecs.split(","):
                c = c.strip()
                if ":" in c:
                    name, level = c.split(":", 1)
                    pairs.append([name.upper(), int(level)])
                else:
                    pairs.append([c.upper(), None])
            codecs_json = json.dumps(pairs)
        else:
            codecs_json = "null"

        encodings_json = (
            json.dumps([e.strip().upper() for e in encodings.split(",")])
            if encodings
            else "null"
        )

        result_json = _lib.py_bench_column(str(file), column, codecs_json, encodings_json)
        result = json.loads(result_json)

        if output == OutputFormat.json:
            typer.echo(result_json)
        else:
            from autoparq.render import render_bench
            render_bench(result, measure)
        raise typer.Exit(0)
    except typer.Exit:
        raise
    except Exception as e:
        typer.echo(f"Error: {e}", err=True)
        raise typer.Exit(2)


@app.command("apply")
def apply(
    file: Annotated[Path, typer.Argument(help="Input Parquet file")],
    output: Annotated[Path, typer.Option("--output", "-o", help="Destination file path")],
    in_place: Annotated[bool, typer.Option("--in-place", help="Allow overwriting input file")] = False,
    engine: Annotated[Engine, typer.Option("--engine", help="Target engine")] = Engine.unknown,
    priority: Annotated[Priority, typer.Option("--priority", help="Optimization priority")] = Priority.balanced,
    sample_rows: Annotated[int, typer.Option("--sample-rows")] = 500_000,
) -> None:
    """Rewrite a Parquet file with recommended encoding and compression settings."""
    try:
        if output.resolve() == file.resolve() and not in_place:
            typer.echo(
                "Error: Output path equals input path. Use --in-place to allow in-place rewrite.",
                err=True,
            )
            raise typer.Exit(2)
        if output.exists() and not in_place:
            typer.echo(
                f"Error: Output file '{output}' already exists. Use --in-place to allow overwrite.",
                err=True,
            )
            raise typer.Exit(2)

        result_json = _lib.py_apply_file(
            str(file),
            str(output),
            engine.value,
            priority.value,
            sample_rows,
        )
        result = json.loads(result_json)

        typer.echo(
            f"Rewrote {result['rows_written']:,} rows: "
            f"{_human_bytes(result['input_size_bytes'])} \u2192 "
            f"{_human_bytes(result['output_size_bytes'])} "
            f"({result['actual_reduction_pct']:.1f}% reduction) "
            f"in {result['elapsed_ms']}ms"
        )
        raise typer.Exit(0)
    except typer.Exit:
        raise
    except Exception as e:
        typer.echo(f"Error: {e}", err=True)
        raise typer.Exit(2)
