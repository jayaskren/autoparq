# autoparq Web App — Conflicts and Ambiguities

Identified by cross-referencing PRD.md, PLAN.md, and TASKS.md.
Each item needs a decision before implementation begins.

---

## C01 — Task ID collision between PRD §15 and TASKS.md

**Severity: High** — An agent reading both documents will be confused about what W05 or W09 means.

PRD §15 defines W01–W16 with one mapping. TASKS.md defines W01–W14 (plus validation gates V-W1 through V-W5) with a completely different mapping. Examples:

| ID | PRD §15 | TASKS.md |
|----|---------|----------|
| W03 | `Add tune_from_cursor()` | `PySpark and Polars snippet generators` |
| W05 | `Add PySpark/Polars to codegen` | `Web Project Scaffold` |
| W09 | `Implement wasm-bridge.js` | `Advisories, caveats, glossary tooltips` |

**Resolution needed:** Drop PRD §15 entirely (it is superseded by TASKS.md) or renumber one of them. TASKS.md is more detailed and should be the authoritative task list.

---

## C02 — `--out-dir` path bug in wasm-pack npm scripts

**Severity: High** — The build will create `web/web/pkg/` instead of `web/pkg/` and the Vite alias will fail to resolve WASM imports.

Both PRD §13 and TASKS W05 show:
```json
"wasm:build": "wasm-pack build .. --target web --out-dir web/pkg --release ..."
```

npm scripts run with `web/` as the working directory. wasm-pack's `--out-dir` is relative to the current directory. So `--out-dir web/pkg` resolves to `web/web/pkg/`.

**Correct value:** `--out-dir pkg` (outputs to `web/pkg/`, which is what the Vite `@wasm` alias resolves to).

**Fix in:** PRD §13 and TASKS W05 package.json scripts.

---

## C03 — wasm-pack commands missing feature flags in PRD §13

**Severity: Medium** — A contributor following the PRD's local dev instructions will get a build failure because PyO3 gets compiled into the WASM target.

PRD §13 shows:
```json
"wasm:build": "wasm-pack build .. --target web --out-dir web/pkg --release"
```

TASKS W05 correctly shows:
```json
"wasm:build": "wasm-pack build .. --target web --out-dir web/pkg --release --features wasm -- --no-default-features"
```

Without `--features wasm -- --no-default-features`, the build includes PyO3 which cannot compile to `wasm32-unknown-unknown`.

**Fix in:** PRD §13 wasm:build and wasm:build:dev scripts. Use the TASKS W05 versions.

---

## C04 — DuckDB code snippet tab defined in PRD but not in TASKS or PLAN

**Severity: Medium** — W03 (TASKS) and W08 (TASKS) both implement three engine variants: PyArrow, PySpark, Polars. The PRD (§5.4) shows a fourth tab: DuckDB.

DuckDB's Parquet write API is distinct:
```python
import duckdb
duckdb.sql("""
    COPY (SELECT * FROM 'input.parquet')
    TO 'output.parquet'
    (FORMAT PARQUET, CODEC 'ZSTD', COMPRESSION_LEVEL 3)
""")
```

DuckDB does not support per-column encoding hints via SQL — only file-level codec. This needs to be explicitly handled (similar to the Polars limitation note).

**Decision needed:**
- **Option A:** Add DuckDB as a fourth snippet variant (W03 scope expands; W08 adds a fourth tab)
- **Option B:** Remove DuckDB from the snippet tabs in PRD §5.4; the engine selector already has DuckDB for tailoring recommendations, which is sufficient

---

## C05 — Tabulator table vs. column cards: relationship undefined

**Severity: Medium** — The PRD describes per-column output as expandable cards (§5.2) AND includes `ColumnTable.js` (Tabulator wrapper) as a named component in §11/§12. TASKS implements both (W07 = cards, TASKS W11 = Tabulator). It is never stated whether these are:

- Alternative views of the same data (a toggle: "Cards | Table")
- The same section (the "table" is actually the cards layout)
- Different sections (cards are the primary view, table is a compact overview elsewhere)

TASKS W11 scopes Tabulator to include sort, column visibility toggle, and frozen first column — features that suggest it is a standalone dense-data view, not the same as the cards.

**Decision needed:**
- **Option A:** Cards are the primary "Columns" section. Tabulator is a compact secondary view accessible via a "Table view" toggle above the cards.
- **Option B:** The Tabulator IS the columns section. Cards are only shown when a row is expanded within the Tabulator (using Tabulator's row expansion feature). This reduces DOM complexity.
- **Option C:** Remove Tabulator entirely. The card filter/sort bar (already in PRD §5.2) provides enough navigation. Tabulator adds 120 KB for marginal benefit.

---

## C06 — "Full Stats" left rail nav item has no corresponding section

**Severity: Low** — PRD §4.4 lists "Full Stats" as a left rail navigation item, but §5 only defines: Summary, Column Recommendations, Codec Options, Code Snippet, Advisories, Caveats. No "Full Stats" section exists.

The raw statistics per column live inside each expanded column card (§5.2 "Raw statistics" accordion), not as a standalone section.

**Decision needed:**
- **Option A:** Remove "Full Stats" from the left rail nav. The raw stats are accessible by expanding any column card.
- **Option B:** Add a "Full Stats" section after "Advisories" that renders a flat table of all column profiles (one row per column, all raw stats as columns). This is essentially the Tabulator view from C05 — resolving both items together.

---

## C07 — Engine selector re-render: three different descriptions

**Severity: Medium** — Three documents describe the engine change behavior differently.

**PRD §7:** "Entire report re-renders from cached `FileProfile`" — implies the frontend caches a `FileProfile` object and re-runs only the recommendation pass (no re-profiling).

**PLAN Phase 4b:** "Evaluate which approach is cleaner. If re-running the full analysis for a 50 MB file takes < 500ms (because the file bytes are already in Worker memory), re-profiling is simpler. If it is noticeable, add a `recommend_from_profile` WASM entry point." — explicitly deferred.

**TASKS W11:** "The Worker retains the `Uint8Array` in module scope after the first analysis. Add a second message type `reanalyze` that reuses the cached bytes" — re-runs full `tune_file_bytes` (re-profiling included).

The PRD implies a `recommend_from_profile` WASM entry point should exist. TASKS W11 takes a simpler path (re-profile from cached bytes). These produce the same UX result but TASKS W11 is slower for large files because it re-samples all columns.

**Decision needed:**
- **Option A (PRD):** Add `recommend_from_profile(profile_json, engine, priority)` WASM function. Cache `FileProfile` JSON in the Worker. Engine change calls this instead of re-profiling. Faster, requires a new WASM entry point (expands W04 scope).
- **Option B (TASKS):** Re-run full `tune_file_bytes` from cached bytes on engine change. Simpler, slightly slower (1–3s for large files). Acceptable for v1.

---

## C08 — Sample file names and sourcing conflict

**Severity: Low** — Two different sets of sample files are described.

**PRD §4.1** (hero section copy):
```
[NYC Taxi (12 MB)]  [IoT Sensor Log (8 MB)]  [GitHub Events (15 MB)]
```
These imply real-world public datasets.

**TASKS W12** (implementation):
- `events_log.parquet` — generated synthetic
- `sensor_readings.parquet` — generated synthetic
- `mixed_analytics.parquet` — generated synthetic

Real-world datasets (NYC Taxi, GitHub Archive) are publicly available and would be more compelling demos. Generated synthetic files are simpler to produce consistently and avoid licensing questions.

**Decision needed:**
- **Option A:** Use real public datasets. Download slices of NYC Taxi and GitHub Archive public data, trim to < 15 MB. More compelling demo. Requires a download/trim script in the repo.
- **Option B:** Use synthetic generated files (TASKS W12 approach). Update PRD §4.1 hero copy to use descriptive names like "Events Log", "Sensor Readings", "Analytics Table".

---

## Summary Table

| ID | Severity | Resolution | Status |
|----|----------|------------|--------|
| C01 | High | PRD §15 replaced with pointer to TASKS.md | ✅ Resolved |
| C02 | High | `--out-dir pkg` in all npm scripts | ✅ Resolved |
| C03 | Medium | Feature flags added to PRD §13 build commands | ✅ Resolved |
| C04 | Medium | Drop DuckDB snippet tab; note in PRD §5.4 and TASKS W03/W08 | ✅ Resolved |
| C05 | Medium | Cards/Table toggle; Table view IS the Full Stats view (resolves C06) | ✅ Resolved |
| C06 | Low | Full Stats fleshed out as PRD §5.7; it is the Table toggle view | ✅ Resolved |
| C07 | Medium | Use `recommend_from_profile` WASM fn; added to W04, W11 rewritten | ✅ Resolved |
| C08 | Low | Real public datasets: NYC Taxi + NOAA Weather; W12 fully rewritten | ✅ Resolved |
