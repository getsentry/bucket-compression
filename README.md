# Bucket Compression Benchmark

This repository contains benchmarks for various compression algorithms for
metric buckets containing 64-bit floating point values. To look at results, see
the `results/` folder.

[Results](results/)

## Algorithms

Lossless algorithms:

- no compression
- lz4
- deflate, gzip, and zlib
- zstd
- [fpzip] reversible
- [zfp] reversible

Lossy algorithms:

- [fpzip]
- [zfp]
- tsz: [Gorilla] XOR-based floating point compression

## Data

*Data is not checked into this repository. In the future, we will add a script
that generates data on-the-fly based on configurable distributions.*

To run the benchmarks, supply a file containing one or more lines of
**distribution** metric bucket payloads in JSON (see the [schema]). This
benchmark will ignore all attributes other than the value and assumes the value
is an array of floats. There can contain any number and size of metric buckets.
Empty lines at the end of the file are supported.

Example:

```jsonb
{"name":"d:custom/foo@millisecond","type":"d","value":[9.114742279052734]}
{"name":"d:custom/bar@none","type":"d","value":[0,0,0,0,0]}
```

The benchmark results from the results folder were generated with real data. See
ticket [OPS-5059] for the source.

## Benchmarks

The benchmarks evaluate the compression ratio and speed/throughput of the
compression algorithms. The baseline for this comparison is a JSON string with
the list of values in the bucket. To stringify and parse this JSON, `serde_json`
is used in default configuration.

There are two main types of benchmarks:

1. **Compression ratio:** Measure the effectiveness of the compression
   algorithms. The ratios (e.g. 23%) are size of the binary output compared with
   the plain JSON string. Note that ratios larger than 100% are possible if the
   binary output is larger than the JSON string. This can happen particularly
   for buckets with a large number of zeros. These benchmarks are run over the
   full dataset provided.
2. **Performance:** Measure the throughput of buckets and elements based on a
   few select examples. The benchmark examples are configured directly in
   `main.rs`

The benchmarks can be configured using the `Args` structure in `main.rs`:

 - `path`: The path to the file containing JSON-encoded buckets.
 - `sort`: Sorts the values loaded from the file before running compression.
   This usually leads to better compression ratios. The sorting overhead is not
   measured in the performance benchmarks.
 - `base64`: Whether the binary output should be encoded with base64, increasing
   its size by a third. This is required if the buffer would be placed in a JSON
   container. When enabled, all size comparisons use the base64 size instead of
   the raw binary.
 - `min_values`: Skips all buckets with fewer than the given number of values.
   This allows to configure a cutoff where compression is not worth it. This
   effectively reduces the number of buckets for the test. This is ignored in
   the performance benchmark.
 - `baseline`: Use either the JSON representation or the binary buffer (fixed 8-byte per value) as baseline for all compression algorithms.

## Building and Running

Always build in Release mode. By default, debug information is enabled for
release builds in this workspace.

```
cargo build --release
```

To run compression benchmarks, run the binary:

```
RUST_BACKTRACE=1 cargo run --release
```

To run performance benchmarks, use the bench test suite:

```
cargo bench
```


[schema]: https://getsentry.github.io/relay/relay_metrics/struct.Bucket.html#json-representation
[ops-5059]: https://getsentry.atlassian.net/browse/OPS-5059
[zfp]: https://zfp.readthedocs.io/en/latest/
[fpzip]: https://github.com/LLNL/fpzip
[gorilla]: https://blog.acolyer.org/2016/05/03/gorilla-a-fast-scalable-in-memory-time-series-database/
