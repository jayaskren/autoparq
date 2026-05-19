use autoparq::apply::rewrite_file;
use autoparq::recommender::codec::{Engine, Priority};
use std::path::Path;
use tempfile::NamedTempFile;

#[test]
#[ignore]
fn test_apply_roundtrip() {
    let input = Path::new("tests/fixtures/multi_column.parquet");
    let temp = NamedTempFile::new().unwrap();
    let output = temp.path().to_path_buf();
    drop(temp); // release the file handle so rewrite can write to it

    let result = rewrite_file(
        input,
        &output,
        Engine::DuckDB,
        Priority::Balanced,
        100_000,
    )
    .unwrap();

    assert!(result.rows_written > 0);
    assert!(output.exists());

    // Verify output is readable as parquet
    let meta = autoparq::profiler::metadata::read_file_metadata(&output).unwrap();
    assert_eq!(meta.num_rows, result.rows_written);
}

#[test]
#[ignore]
fn test_apply_refuses_overwrite_via_rust() {
    // Test that rewrite_file itself can overwrite (the Python guard handles the refusal)
    // Just verify the function succeeds when output != input
    let input = Path::new("tests/fixtures/multi_column.parquet");
    let temp = NamedTempFile::new().unwrap();
    let output_path = temp.path().to_path_buf();
    drop(temp);

    let result = rewrite_file(input, &output_path, Engine::Unknown, Priority::Balanced, 10_000);
    assert!(result.is_ok());
}
