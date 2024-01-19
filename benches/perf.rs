use bucket_compression::stats::MiniBucket;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

const SORT: bool = true;
const PATH: &str = "data/custom.metrics.jsonb";

// Benchmark inputs from the file (name, line index starting at zero)
const INPUTS: &[(&str, usize)] = &[
    ("12k zeros", 1119),
    ("12k 32bit", 1032),
    ("12k 64bit", 346),
    ("430 64bit", 13),
];

fn load_values(json: &str, line: usize) -> Vec<f64> {
    let json = json.lines().nth(line).unwrap();
    let mut values = serde_json::from_str::<MiniBucket>(json).unwrap().value;

    if SORT {
        float_ord::sort(&mut values);
    }

    values
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let json = std::fs::read_to_string(PATH).unwrap();

    let mut group = c.benchmark_group("write");
    for mut test in bucket_compression::tests() {
        for (input_name, line_index) in INPUTS {
            let values = load_values(&json, *line_index);

            group.throughput(Throughput::Elements(values.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(test.name(), input_name),
                values.as_slice(),
                |b, values| b.iter_with_large_drop(|| test.write(values)),
            );
        }
    }
    group.finish();

    let mut group = c.benchmark_group("read");
    for mut test in bucket_compression::tests() {
        for (input_name, line_index) in INPUTS {
            let values = load_values(&json, *line_index);
            let compressed = test.write(values.as_slice()).unwrap();

            group.throughput(Throughput::Elements(values.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(test.name(), input_name),
                compressed.as_slice(),
                |b, slice| b.iter_with_large_drop(|| test.read(slice)),
            );
        }
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
