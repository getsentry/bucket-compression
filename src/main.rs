use std::path::PathBuf;

use bucket_compression::stats::{Args, Base, Benchmark};

fn main() {
    let args = Args {
        path: PathBuf::from("data/custom.metrics.jsonb"),
        sort: true,
        base64: false,
        min_values: Some(400),
        base: Base::Binary,
    };

    println!("Configuration:");
    println!("{args}");

    let mut benchmark = Benchmark::new(args);
    for test in bucket_compression::tests() {
        benchmark.add(test);
    }
    benchmark.run().unwrap();
}
