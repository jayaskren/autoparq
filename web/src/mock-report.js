export const MOCK_REPORT = {
  "file_path": "orders_2024_q1.parquet",
  "engine": "duckdb",
  "priority": "balanced",
  "file_size_bytes": 245678901,
  "num_rows": 1000000,
  "num_columns": 6,
  "current_codec": "SNAPPY",
  "scan_time_ms": 2341,
  "sample_fraction": 0.12,
  "predicted_size_reduction_pct": 47.3,
  "predicted_read_speedup": 1.98,
  "overall_confidence": "High",
  "columns": [
    {
      "column_name": "customer_id",
      "physical_type": "INT64",
      "logical_type": null,
      "cardinality_estimate": 10000,
      "cardinality_ratio": 0.0001,
      "null_fraction": 0.002,
      "recommended_encoding": "RLE_DICTIONARY",
      "recommended_codec": "ZSTD",
      "recommended_codec_level": 3,
      "encoding_rule_fired": "RleDictionary",
      "reason_brief": "cardinality_ratio=0.0001 (10000 distinct / 1000000 rows) → dictionary encodes 10000 unique values into a 4-byte lookup",
      "confidence": "High",
      "confidence_reason": "sample_fraction=0.12 AND sample_rows=120000",
      "impact_stars": 5,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": {
        "raw_stats": {
          "cardinality_estimate": 10000,
          "cardinality_ratio": 0.0001,
          "cardinality_method": "hyperloglog",
          "monotonicity_score": null,
          "null_fraction": 0.002,
          "sample_rows": 120000,
          "sample_fraction": 0.12,
          "uuid_pattern_detected": false,
          "json_pattern_detected": false,
          "byte_entropy": null
        },
        "reasoning_chain": [
          {
            "rule_name": "BooleanRle",
            "evaluated": false,
            "fired": false,
            "threshold": "physical_type == BOOLEAN",
            "actual_value": "INT64",
            "outcome": "Skipped: physical_type=INT64 is not BOOLEAN"
          },
          {
            "rule_name": "DeltaMonotonic",
            "evaluated": true,
            "fired": false,
            "threshold": "0.90",
            "actual_value": "0.0431",
            "outcome": "Rejected: monotonicity_score=0.0431 < threshold 0.90"
          },
          {
            "rule_name": "RleDictionary",
            "evaluated": true,
            "fired": true,
            "threshold": "cardinality_ratio < 0.10 AND dict_size < 512KB",
            "actual_value": "cardinality_ratio=0.0001",
            "outcome": "Fired: cardinality_ratio=0.0001 < 0.10 → RLE_DICTIONARY"
          }
        ],
        "alternatives_considered": [
          {
            "encoding": "DELTA_BINARY_PACKED",
            "rejected_reason": "monotonicity_score=0.0431 < threshold 0.90"
          },
          {
            "encoding": "BYTE_STREAM_SPLIT",
            "rejected_reason": "not FLOAT/DOUBLE physical type"
          }
        ],
        "engine_compatibility": null,
        "teach_yourself": "Dictionary encoding stores each distinct value once and replaces data values with small integer indices. When cardinality is low (few distinct values), these indices compress extremely well with run-length encoding."
      }
    },
    {
      "column_name": "order_ts",
      "physical_type": "INT64",
      "logical_type": "TIMESTAMP(MICROS, UTC)",
      "cardinality_estimate": 987234,
      "cardinality_ratio": 0.987,
      "null_fraction": 0.0,
      "recommended_encoding": "DELTA_BINARY_PACKED",
      "recommended_codec": "ZSTD",
      "recommended_codec_level": 3,
      "encoding_rule_fired": "DeltaMonotonic",
      "reason_brief": "monotonicity_score=0.9412 >= threshold 0.90 → delta stores differences, ~60% savings for sorted timestamps",
      "confidence": "High",
      "confidence_reason": "sample_fraction=0.12 AND sample_rows=120000",
      "impact_stars": 4,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": null
    },
    {
      "column_name": "amount",
      "physical_type": "DOUBLE",
      "logical_type": null,
      "cardinality_estimate": 823456,
      "cardinality_ratio": 0.823,
      "null_fraction": 0.01,
      "recommended_encoding": "BYTE_STREAM_SPLIT",
      "recommended_codec": "ZSTD",
      "recommended_codec_level": 3,
      "encoding_rule_fired": "ByteStreamSplit",
      "reason_brief": "DOUBLE with cardinality_ratio=0.823 > 0.50 → BYTE_STREAM_SPLIT deinterleaves bytes for better codec compression",
      "confidence": "High",
      "confidence_reason": "sample_fraction=0.12 AND sample_rows=120000",
      "impact_stars": 3,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": null
    },
    {
      "column_name": "status",
      "physical_type": "BYTE_ARRAY",
      "logical_type": "STRING",
      "cardinality_estimate": 5,
      "cardinality_ratio": 0.000005,
      "null_fraction": 0.0,
      "recommended_encoding": "RLE_DICTIONARY",
      "recommended_codec": "ZSTD",
      "recommended_codec_level": 3,
      "encoding_rule_fired": "RleDictionary",
      "reason_brief": "cardinality_ratio=0.000005 (5 distinct values) → dictionary encodes 5 unique strings into a 4-byte lookup",
      "confidence": "High",
      "confidence_reason": "sample_fraction=0.12 AND sample_rows=120000",
      "impact_stars": 5,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": null
    },
    {
      "column_name": "blob_data",
      "physical_type": "BYTE_ARRAY",
      "logical_type": null,
      "cardinality_estimate": 120000,
      "cardinality_ratio": 1.0,
      "null_fraction": 0.05,
      "recommended_encoding": "PLAIN",
      "recommended_codec": "UNCOMPRESSED",
      "recommended_codec_level": null,
      "encoding_rule_fired": "PlainDefault",
      "reason_brief": "byte_entropy=7.92 > 7.5 → pre-compressed or random data; compression would increase file size",
      "confidence": "Medium",
      "confidence_reason": "sample_fraction=0.12 but sample_rows=120000",
      "impact_stars": 2,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": null
    },
    {
      "column_name": "is_active",
      "physical_type": "BOOLEAN",
      "logical_type": null,
      "cardinality_estimate": 2,
      "cardinality_ratio": 0.000002,
      "null_fraction": 0.0,
      "recommended_encoding": "RLE",
      "recommended_codec": "ZSTD",
      "recommended_codec_level": 3,
      "encoding_rule_fired": "BooleanRle",
      "reason_brief": "BOOLEAN column → RLE encoding (automatic in all Parquet writers)",
      "confidence": "High",
      "confidence_reason": "sample_fraction=0.12 AND sample_rows=120000",
      "impact_stars": 1,
      "engine_compatibility": null,
      "caveats": [],
      "full_explain": null
    }
  ],
  "file_caveats": [
    {
      "severity": "Info",
      "message": "File was written without sort metadata. See Sort Order Advisory."
    }
  ],
  "python_snippet": "import pyarrow.parquet as pq\n\nPARQUET_WRITE_OPTIONS = {\n    \"compression\": \"zstd\",\n    \"compression_level\": 3,\n    \"column_encoding\": {\n        \"customer_id\": \"RLE_DICTIONARY\",\n        \"order_ts\": \"DELTA_BINARY_PACKED\",\n        \"amount\": \"BYTE_STREAM_SPLIT\",\n        \"status\": \"RLE_DICTIONARY\",\n    },\n    \"write_statistics\": True,\n}\npq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)\n# NOTE: predictions are [estimated] — use autoparq bench to validate",
  "spark_snippet": "spark.conf.set(\"spark.sql.parquet.compression.codec\", \"zstd\")\n# ZSTD requires Spark 3.2+; per-column encoding hints require Spark 3.4+",
  "options": {
    "a": {
      "label": "Balanced",
      "codec_description": "ZSTD level 3",
      "tradeoff": "Best balance of size and read speed. Default recommendation.",
      "python_snippet": "import pyarrow.parquet as pq\n\nPARQUET_WRITE_OPTIONS = {\n    \"compression\": \"zstd\",\n    \"compression_level\": 3,\n    \"column_encoding\": {\n        \"customer_id\": \"RLE_DICTIONARY\",\n        \"order_ts\": \"DELTA_BINARY_PACKED\",\n        \"amount\": \"BYTE_STREAM_SPLIT\",\n        \"status\": \"RLE_DICTIONARY\",\n    },\n    \"write_statistics\": True,\n}\npq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)\n# NOTE: predictions are [estimated]",
      "caveats": []
    },
    "b": {
      "label": "Smallest File",
      "codec_description": "ZSTD level 6",
      "tradeoff": "20-30% smaller files than ZSTD:3. ~1.5x slower writes. Best for archival.",
      "python_snippet": "import pyarrow.parquet as pq\n\nPARQUET_WRITE_OPTIONS = {\n    \"compression\": \"zstd\",\n    \"compression_level\": 6,\n    \"column_encoding\": {\n        \"customer_id\": \"RLE_DICTIONARY\",\n        \"order_ts\": \"DELTA_BINARY_PACKED\",\n        \"amount\": \"BYTE_STREAM_SPLIT\",\n        \"status\": \"RLE_DICTIONARY\",\n    },\n    \"write_statistics\": True,\n}\npq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)\n# NOTE: predictions are [estimated]",
      "caveats": []
    },
    "c": {
      "label": "Fastest Reads",
      "codec_description": "LZ4",
      "tradeoff": "Fastest decompression. ~20-30% larger files than ZSTD:3. Best for hot query paths.",
      "python_snippet": "import pyarrow.parquet as pq\n\nPARQUET_WRITE_OPTIONS = {\n    \"compression\": \"lz4\",\n    \"column_encoding\": {\n        \"customer_id\": \"RLE_DICTIONARY\",\n        \"order_ts\": \"DELTA_BINARY_PACKED\",\n        \"amount\": \"BYTE_STREAM_SPLIT\",\n        \"status\": \"RLE_DICTIONARY\",\n    },\n    \"write_statistics\": True,\n}\npq.write_table(table, \"output.parquet\", **PARQUET_WRITE_OPTIONS)\n# NOTE: predictions are [estimated]",
      "caveats": []
    }
  },
  "row_group_advisory": {
    "num_row_groups": 29,
    "avg_bytes_per_group": 8469203,
    "min_bytes_per_group": 7234567,
    "max_bytes_per_group": 9123456,
    "recommended_min_bytes": 67108864,
    "recommended_max_bytes": 134217728,
    "is_within_recommendation": false,
    "advice": "Current avg row group size is 8.1 MB. DuckDB performs best with 64–128 MB row groups. Consider merging to 2–4 row groups when rewriting."
  },
  "sort_advisory": {
    "declared_sort_columns": [],
    "inferred_sort_candidates": ["order_ts"],
    "advice": "Column 'order_ts' appears sorted (monotonicity_score=0.94) but the Parquet footer has no sort metadata declared. Adding sort metadata lets DuckDB skip row groups during range queries on this column."
  },
  "file_profile": {},
  "column_profiles": []
};
