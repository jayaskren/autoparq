from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich import box

console = Console()


def render_tune_text(report: dict, explain: str = "brief") -> None:
    # Header panel
    header_lines = [
        f"[bold]{report['file_path']}[/bold]",
        f"Size: [cyan]{_human_bytes(report['file_size_bytes'])}[/cyan]  "
        f"Rows: [cyan]{report['num_rows']:,}[/cyan]  "
        f"Columns: [cyan]{report['num_columns']}[/cyan]",
        f"Current codec: [cyan]{report['current_codec']}[/cyan]  "
        f"Scan time: {report['scan_time_ms']}ms  "
        f"Sample: {report['sample_fraction']*100:.1f}%",
        f"[bold yellow]Estimated impact: -{report['predicted_size_reduction_pct']:.0f}% size, "
        f"{report['predicted_read_speedup']:.1f}x read speed "
        f"({report['overall_confidence']} confidence) \\[estimated][/bold yellow]",
    ]
    console.print(Panel("\n".join(header_lines), title="autoparq tune", border_style="green"))

    # Per-column table
    table = Table(
        box=box.SIMPLE_HEAD,
        show_header=True,
        header_style="bold cyan",
        show_edge=False,
        pad_edge=False,
    )
    table.add_column("Column", max_width=25, no_wrap=True)
    table.add_column("Type", no_wrap=True)
    table.add_column("Cardinality", justify="right")
    table.add_column("Null%", justify="right")
    table.add_column("Encoding", no_wrap=True)
    table.add_column("Codec", no_wrap=True)
    table.add_column("Conf", no_wrap=True)
    table.add_column("Impact")

    for col in report["columns"]:
        stars = "\u2605" * col["impact_stars"] + "\u2606" * (5 - col["impact_stars"])
        codec_str = col["recommended_codec"]
        if col.get("recommended_codec_level"):
            codec_str += f":{col['recommended_codec_level']}"
        conf_color = {"High": "green", "Medium": "yellow", "Low": "red"}.get(col["confidence"], "white")
        conf_str = f"[{conf_color}]{col['confidence'][:3]}[/{conf_color}]"
        card_pct = f"{col['cardinality_ratio']*100:.2f}%"
        null_pct = f"{col['null_fraction']*100:.1f}%"

        table.add_row(
            col["column_name"],
            col["physical_type"],
            card_pct,
            null_pct,
            col["recommended_encoding"],
            codec_str,
            conf_str,
            stars,
        )
    console.print(table)

    # Why block
    console.print("\n[bold]Why these settings:[/bold]")
    for col in report["columns"]:
        if col["recommended_encoding"] != "PLAIN" or col["recommended_codec"] != report["current_codec"]:
            codec_str = col["recommended_codec"]
            if col.get("recommended_codec_level"):
                codec_str += f":{col['recommended_codec_level']}"
            console.print(
                f"  [cyan]{col['column_name']}[/cyan] \u2192 "
                f"{col['recommended_encoding']} + {codec_str}: {col['reason_brief']}"
            )

    # Full Explain sections (only rendered when explain="full")
    full_explain_cols = [col for col in report["columns"] if col.get("full_explain")]
    if full_explain_cols:
        console.print()
        for col in full_explain_cols:
            fe = col["full_explain"]
            console.print(f"\n[bold]\u2500\u2500 Full Explain: {col['column_name']} \u2500\u2500[/bold]")

            # Raw Stats table
            rs_table = Table(
                title="Raw Stats",
                box=box.SIMPLE_HEAD,
                show_header=True,
                header_style="bold",
                show_edge=False,
                pad_edge=False,
            )
            rs_table.add_column("Stat", no_wrap=True)
            rs_table.add_column("Value", no_wrap=True)
            for k, v in sorted(fe["raw_stats"].items()):
                rs_table.add_row(k, str(v) if v is not None else "null")
            console.print(rs_table)

            # Reasoning Chain table
            rc_table = Table(
                title="Reasoning Chain",
                box=box.SIMPLE_HEAD,
                show_header=True,
                header_style="bold",
                show_edge=False,
                pad_edge=False,
            )
            rc_table.add_column("Rule", no_wrap=True)
            rc_table.add_column("Fired", no_wrap=True)
            rc_table.add_column("Threshold", no_wrap=True)
            rc_table.add_column("Actual Value", no_wrap=True)
            rc_table.add_column("Outcome")
            for rule in fe.get("reasoning_chain", []):
                fired_str = "[green]yes[/green]" if rule["fired"] else "[dim]no[/dim]"
                rc_table.add_row(
                    rule["rule_name"],
                    fired_str,
                    rule["threshold"],
                    rule["actual_value"],
                    rule["outcome"],
                )
            console.print(rc_table)

            # Alternatives considered
            alts = fe.get("alternatives_considered", [])
            if alts:
                console.print("[bold]Alternatives considered:[/bold]")
                for alt in alts:
                    console.print(f"  [dim]{alt['encoding']}[/dim]: {alt['rejected_reason']}")

            # Teach Yourself panel
            teach = fe.get("teach_yourself", "")
            if teach:
                console.print(Panel(f"[dim italic]{teach}[/dim italic]", title="Teach Yourself", border_style="dim"))

    # Caveats
    all_caveats = list(report.get("file_caveats", []))
    for col in report["columns"]:
        for cav in col.get("caveats", []):
            all_caveats.append({"column": col["column_name"], **cav})
    if all_caveats:
        console.print("\n[bold yellow]Caveats:[/bold yellow]")
        for cav in all_caveats:
            col_prefix = f"[cyan]{cav['column']}[/cyan]: " if "column" in cav else ""
            icon = "\u26a0" if cav["severity"] == "Warning" else "\u2139"
            console.print(f"  {icon} {col_prefix}{cav['message']}")

    # Codec Options panel
    options = report.get("options", {})
    if options and "a" in options:
        console.print()
        from rich.table import Table as RichTable
        opt_table = RichTable(title="Codec Options", show_header=True)
        opt_table.add_column("", style="bold", no_wrap=True)
        opt_table.add_column("Label")
        opt_table.add_column("Codec")
        opt_table.add_column("Tradeoff")
        for key, badge in [("a", "\[RECOMMENDED]"), ("b", ""), ("c", "")]:
            b = options[key]
            opt_table.add_row(
                badge,
                b["label"],
                b["codec_description"],
                b.get("tradeoff", ""),
            )
        console.print(opt_table)
        console.print("[dim]Codec size differences are data-dependent. Use [bold]autoparq bench --column <col>[/bold] to measure before rewriting large files.[/dim]")

    # Row group advisory
    rg = report.get("row_group_advisory", {})
    if rg and not rg.get("is_within_recommendation", True):
        console.print()
        console.print(Panel(rg["advice"], title="Row Group Advisory", border_style="yellow"))

    # Sort order advisory
    sort = report.get("sort_advisory", {})
    if sort and sort.get("inferred_sort_candidates"):
        console.print()
        console.print(Panel(sort["advice"], title="Sort Order Advisory", border_style="cyan"))

    # Apply block
    console.print("\n[bold]Apply these settings:[/bold]")
    console.print("[dim]Python (PyArrow):[/dim]")
    from autoparq.codegen import generate_python_snippet
    from rich.markup import escape
    console.print(escape(generate_python_snippet(report)))

def render_bench(result: dict, measure: str = "all") -> None:
    from rich.table import Table

    entries = result.get("entries", [])

    t = Table(title=f"Bench: {result.get('column_name')} ({result.get('physical_type')})")
    t.add_column("Encoding")
    t.add_column("Codec")
    t.add_column("Compressed")
    t.add_column("Ratio")
    if measure in ("write", "all"):
        t.add_column("Write ms")
    if measure in ("read", "all"):
        t.add_column("Read ms")

    if not entries:
        console.print("[yellow]No results.[/yellow]")
        return

    min_size = min(e["compressed_bytes"] for e in entries)
    min_read = min(e["read_ms"] for e in entries) if measure in ("read", "all") else None
    min_write = min(e["write_ms"] for e in entries) if measure in ("write", "all") else None

    for e in entries:
        is_smallest = e["compressed_bytes"] == min_size
        is_fastest_read = min_read is not None and e["read_ms"] == min_read
        is_fastest_write = min_write is not None and e["write_ms"] == min_write

        badge = ""
        if is_smallest:
            badge += "[bold green]\u2605size[/bold green] "
        if is_fastest_read:
            badge += "[bold cyan]\u2605read[/bold cyan] "
        if is_fastest_write:
            badge += "[bold yellow]\u2605write[/bold yellow] "

        codec_str = e["codec"]
        if e.get("codec_level") is not None:
            codec_str += f":{e['codec_level']}"

        row = [
            e["encoding"] + (" " + badge.rstrip() if badge else ""),
            codec_str,
            _human_bytes(e["compressed_bytes"]),
            f"{e['compression_ratio']:.2f}x",
        ]
        if measure in ("write", "all"):
            row.append(str(e["write_ms"]))
        if measure in ("read", "all"):
            row.append(str(e["read_ms"]))

        t.add_row(*row)

    console.print(t)
    console.print(
        "[dim]Results from in-memory benchmark on first row group sample. "
        "Actual I/O performance may differ.[/dim]"
    )


def _human_bytes(n: int) -> str:
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if abs(n) < 1024.0:
            return f"{n:.1f} {unit}"
        n /= 1024.0
    return f"{n:.1f} PB"

def render_info(profile: dict) -> None:
    # --- File header panel ---
    num_rgs = profile["num_row_groups"]
    row_counts = profile["row_group_row_counts"]
    avg_rows = int(sum(row_counts) / len(row_counts)) if row_counts else 0
    created_by = profile.get("created_by") or "unknown"

    header_lines = [
        f"[bold]{profile['path']}[/bold]",
        f"Size: [cyan]{_human_bytes(profile['file_size_bytes'])}[/cyan]  "
        f"Parquet v{profile['parquet_version']}  "
        f"Rows: [cyan]{profile['num_rows']:,}[/cyan]",
        f"Row groups: [cyan]{num_rgs}[/cyan]  "
        f"Avg rows/group: [cyan]{avg_rows:,}[/cyan]",
        f"Written by: [dim]{created_by}[/dim]",
    ]
    console.print(Panel("\n".join(header_lines), title="autoparq info", border_style="blue"))

    # --- Per-column table ---
    cols = profile["columns"]

    table = Table(
        box=box.SIMPLE_HEAD,
        show_header=True,
        header_style="bold cyan",
        show_edge=False,
        pad_edge=False,
    )
    table.add_column("Column", max_width=30, no_wrap=True)
    table.add_column("Phys Type", no_wrap=True)
    table.add_column("Logical Type", no_wrap=True)
    table.add_column("Encodings", no_wrap=True)
    table.add_column("Codec", no_wrap=True)
    table.add_column("Nulls", justify="right", no_wrap=True)
    table.add_column("Min", max_width=20, no_wrap=True)
    table.add_column("Max", max_width=20, no_wrap=True)
    table.add_column("Compressed", justify="right", no_wrap=True)
    table.add_column("Uncompressed", justify="right", no_wrap=True)
    table.add_column("Ratio", justify="right", no_wrap=True)

    has_absent_stats = False
    for col in cols:
        stats_ok = col["statistics_available"]
        if not stats_ok:
            has_absent_stats = True

        null_str = f"{col['total_null_count']:,}" if col.get("total_null_count") is not None else "[dim]—[/dim]"
        min_str = col.get("min_value") or "[dim]—[/dim]"
        max_str = col.get("max_value") or "[dim]—[/dim]"
        ratio = col["compression_ratio"]
        ratio_str = f"{ratio:.1f}x" if ratio > 1.0 else "—"
        encodings_str = ", ".join(col["encodings"]) if col["encodings"] else "PLAIN"
        logical_str = col.get("logical_type") or "—"

        table.add_row(
            col["name"],
            col["physical_type"],
            logical_str,
            encodings_str,
            col["codec"],
            null_str,
            min_str,
            max_str,
            _human_bytes(col["compressed_bytes"]),
            _human_bytes(col["uncompressed_bytes"]),
            ratio_str,
        )

    console.print(table)

    if has_absent_stats:
        console.print(
            "[dim]¹ Statistics absent for one or more columns "
            "(writer did not compute them). Run [bold]autoparq tune[/bold] to profile via sampling.[/dim]"
        )
