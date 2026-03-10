//! Performance benchmarks for graph rendering.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_placeholder(c: &mut Criterion) {
    c.bench_function("placeholder", |b| {
        b.iter(|| {
            // Placeholder benchmark
            let sum: u64 = (0..1000).sum();
            sum
        })
    });
}

criterion_group!(benches, bench_placeholder);
criterion_main!(benches);
