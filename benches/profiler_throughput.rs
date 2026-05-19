use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::{Path, PathBuf};
use std::time::Duration;

use autoparq::profiler::metadata::read_file_metadata;
use autoparq::profiler::sampler::sample_column;
use autoparq::profiler::stats::profile_column;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn bench_metadata_parse(c: &mut Criterion) {
    let path = fixture_path("multi_column.parquet");
    // Skip benchmark if fixture doesn't exist
    if !path.exists() {
        eprintln!("Skipping bench_metadata_parse: fixture not found at {:?}", path);
        return;
    }
    c.bench_function("metadata_parse", |b| {
        b.iter(|| {
            let _ = read_file_metadata(black_box(&path)).unwrap();
        })
    });
}

fn bench_sample_column(c: &mut Criterion) {
    let path = fixture_path("monotonic_ints.parquet");
    if !path.exists() {
        eprintln!("Skipping bench_sample_column: fixture not found");
        return;
    }
    c.bench_function("sample_column_id", |b| {
        b.iter(|| {
            let _ = sample_column(black_box(&path), black_box("id"), 0, 100_000).unwrap();
        })
    });
}

fn bench_profile_column_int64(c: &mut Criterion) {
    let path = fixture_path("monotonic_ints.parquet");
    if !path.exists() {
        eprintln!("Skipping bench_profile_column_int64: fixture not found");
        return;
    }
    // Pre-sample once, then benchmark profile_column only
    let sample = sample_column(&path, "id", 0, 100_000).unwrap();
    c.bench_function("profile_column_int64", |b| {
        b.iter(|| {
            let _ = profile_column(black_box(&sample));
        })
    });
}

fn bench_profile_column_string(c: &mut Criterion) {
    let path = fixture_path("low_cardinality_strings.parquet");
    if !path.exists() {
        eprintln!("Skipping bench_profile_column_string: fixture not found");
        return;
    }
    let sample = sample_column(&path, "status", 0, 100_000).unwrap();
    c.bench_function("profile_column_string", |b| {
        b.iter(|| {
            let _ = profile_column(black_box(&sample));
        })
    });
}

fn bench_rayon_vs_sequential(c: &mut Criterion) {
    let path = fixture_path("multi_column.parquet");
    if !path.exists() {
        eprintln!("Skipping bench_rayon_vs_sequential: fixture not found");
        return;
    }

    let meta = read_file_metadata(&path).unwrap();
    let column_names: Vec<String> = meta.columns.iter().map(|c| c.name.clone()).collect();

    let mut group = c.benchmark_group("parallel_vs_sequential");
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("parallel_rayon", |b| {
        b.iter(|| {
            use rayon::prelude::*;
            let _results: Vec<_> = column_names.par_iter().map(|col_name| {
                let s = sample_column(&path, col_name, 0, 10_000).unwrap();
                profile_column(&s)
            }).collect();
        })
    });

    group.bench_function("sequential", |b| {
        b.iter(|| {
            let _results: Vec<_> = column_names.iter().map(|col_name| {
                let s = sample_column(&path, col_name, 0, 10_000).unwrap();
                profile_column(&s)
            }).collect();
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_metadata_parse,
    bench_sample_column,
    bench_profile_column_int64,
    bench_profile_column_string,
    bench_rayon_vs_sequential,
);
criterion_main!(benches);
