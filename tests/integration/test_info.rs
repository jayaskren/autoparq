use autoparq::profiler::metadata::read_file_metadata;

#[test]
#[ignore]  // run with: cargo test -- --ignored
fn test_info_multi_column() {
    let profile = read_file_metadata(std::path::Path::new("tests/fixtures/multi_column.parquet"))
        .expect("should read multi_column.parquet");
    assert_eq!(profile.columns.len(), 6);
    for col in &profile.columns {
        // Compression ratio can be slightly below 1.0 due to codec overhead on incompressible data
        assert!(col.compression_ratio >= 0.95, "ratio should be >= 0.95 for {}", col.name);
    }
}

#[test]
#[ignore]
fn test_info_no_statistics() {
    let profile = read_file_metadata(std::path::Path::new("tests/fixtures/no_statistics.parquet"))
        .expect("should read no_statistics.parquet");
    for col in &profile.columns {
        assert!(!col.statistics_available, "stats should be absent for {}", col.name);
    }
}
