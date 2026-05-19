# CONFLICTS & OPEN QUESTIONS — Spec 005

## Conflicts with actual code

### C01 — `sample_column_from_bytes` already exists

**Location:** `src/profiler/sampler.rs` lines 101–167

**Issue:** PLAN Step 1.2 and TASKS T02 say to add `sample_column_from_bytes`. It already exists.

**Signature:**
```rust
pub fn sample_column_from_bytes(
    data: bytes::Bytes,
    column_name: &str,
    row_group_index: usize,
    max_rows: usize,
) -> Result<ColumnSample, AutoparqError>
```

**Resolution:** T02 can be deleted. `benchmark_column_from_bytes` (T03) calls this directly with `Bytes::copy_from_slice(data)` to convert from `&[u8]`.

---

### C02 — Wrong types in PLAN for `valid_encodings_from_bytes`

**Location:** PLAN Step 1.4, TASKS T04

**Issue:** The PLAN says `valid_encodings_from_bytes` should return `Vec<Encoding>`. But `valid_encodings_for_type` in `bench.rs` returns `Vec<String>`, and `benchmark_column` takes `encodings: &[String]`. The internal type is `String`, not the parquet crate's `Encoding` enum (parsing happens inside `benchmark_column` via `parse_encoding`).

**Resolution:** `valid_encodings_from_bytes` should return `Vec<String>`. The WASM binding passes this `Vec<String>` directly to `benchmark_column_from_bytes` without any type conversion. Update T04.

---

### C03 — `default_codecs()` returns `Vec<(String, Option<i32>)>`, not `Vec<(Codec, Option<i32>)>`

**Location:** PLAN Step 1.3, PRD B03, TASKS T03

**Issue:** PRD and PLAN use the notation `Vec<(Codec, Option<i32>)>` suggesting the `Codec` enum type. The actual return type is `Vec<(String, Option<i32>)>` — string names, not enum variants. The `parse_compression` function handles conversion inside the loop.

**Resolution:** Correct the type in the function signatures in PLAN/PRD. No behaviour change needed.

---

### C04 — TASKS T04 partially redundant

**Location:** TASKS T04 — `valid_encodings_from_bytes`

**Issue:** T04 proposes a new function that reads the Parquet footer from bytes to get the physical type, then calls `valid_encodings_for_type`. However, the WASM binding already has access to `bench_column_bytes(data, column_name)`. The physical type can be extracted by opening the metadata (which `sample_column_from_bytes` also does). This is a one-time overhead.

**Resolution:** Keep T04 but note that `valid_encodings_from_bytes` is a thin wrapper around `ParquetMetaDataReader` + `valid_encodings_for_type`. It can also be folded into `benchmark_column_from_bytes` directly — read the physical type as part of that function, derive encodings internally, and remove the need for a separate `valid_encodings_from_bytes` function entirely.

**Decision needed:** Should `bench_column_bytes` (WASM) call `valid_encodings_from_bytes` separately, or should `benchmark_column_from_bytes` derive valid encodings internally? The latter is cleaner (fewer exposed helpers) but less testable.

**Recommendation:** Fold into `benchmark_column_from_bytes` — it already reads the file metadata to extract the column sample, so the physical type is available at that point.

---

## Ambiguities

### A01 — Which row group to benchmark?

**Context:** `sample_column_from_bytes` takes a `row_group_index: usize`. When the WASM bench runs, which row group should it use?

**Options:**
- Always use row group 0 (simple, predictable)
- Use the same row group that was sampled during profiling (consistent, but requires tracking this)
- Use the first row group with ≥ N rows (handles files where row group 0 is small due to partial writes)

**Recommendation:** Use row group 0 with a `max_rows = 500_000` cap. Consistent with the existing CLI bench (`benchmark_column` line 106: `sample_column(path, column_name, 0, 500_000)`). Add a note in the results panel: "Benchmarked on first row group (up to 500,000 rows)."

**Status:** Open — confirm this matches user expectation.

---

### A02 — How to match recommended row in results table?

**Context:** The results table should highlight the row matching the recommendation (`col.recommended_encoding` + `col.recommended_codec`). But the report JSON has `recommended_encoding` (e.g. `"DELTA_BINARY_PACKED"`) and `recommended_codec` (e.g. `"ZSTD:3"`). The bench result entries have separate `codec` and `codec_level` fields.

**Resolution:** Normalise both sides to a common string form before comparing:
```js
function normaliseCodec(entry) {
  return entry.codec_level != null
    ? `${entry.codec}:${entry.codec_level}`
    : entry.codec;
}
// Match: entry.encoding === col.recommended_encoding && normaliseCodec(entry) === col.recommended_codec
```

This is the same pattern used elsewhere in the UI for codec display. Not a blocker, but should be explicit in the implementation notes.

---

### A03 — What if `wasmModule` is not yet initialised when bench button is clicked?

**Context:** WASM init is async. If a user somehow clicks the bench button before WASM finishes loading (unlikely but possible), the call to `bench_column_bytes` will fail.

**Resolution:** Gate the bench button: disable it while WASM is loading, enable it after `wasmModule` is initialised. The existing file drop handler already gates on WASM init — apply the same pattern.

---

### A04 — T14 (optional summary update with measured results) — worth doing?

**Context:** T14 proposes updating the summary stats after each bench completion to reflect measured data. This adds ~40 lines of JS state management. The simpler approach (T11–T13 only) replaces point estimates with a range and adds a CTA.

**Decision needed:** Is the dynamic summary update worth the complexity? The user gets real numbers in the column card itself; updating the summary is cosmetic polish.

**Recommendation:** Defer T14 to a follow-on spec. The core value (honest estimates + real bench results per column) is delivered by T01–T13.

---

## Open Questions

### Q01 — Should bench button be visible when accordion is collapsed?

If the button is only visible when the accordion is open (current spec), users must expand each card to discover it. Should there be a small "bench" icon in the card header visible at all times?

**Recommendation:** Keep it accordion-only for now. Cards are compact by default; adding a bench icon to every collapsed card adds visual clutter. The bench feature is opt-in for users who want to dig deeper.

---

### Q02 — Performance on large files: should we warn per-column?

The existing `check_file_size()` warns at 200 MB file-level. For bench specifically, a 200 MB file could take 10–30 seconds per column in WASM.

**Recommendation:** If `file_size_bytes > 200_000_000`, add a tooltip or inline note to the bench button: "Large file — benchmark may take 30+ seconds." Use the already-available `report.file_size_bytes` to conditionally add this text.

---

### Q03 — Should the bench results table be sorted the same way as the CLI (by compressed_bytes)?

The CLI `bench` command sorts `entries` by `compressed_bytes` ascending (smallest first). This matches the Rust code (`entries.sort_by_key(|e| e.compressed_bytes)`). The UI table should respect this default sort order and show smallest-first.

**Recommendation:** Yes, preserve the Rust sort order. Optionally allow clicking column headers to re-sort in the UI, but the default should be smallest-to-largest compressed size.
