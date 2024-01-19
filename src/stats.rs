use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use cli_table::{format::Justify, Table, WithTitle};
use indicatif::ProgressBar;
use serde::Deserialize;
use serde_json::Deserializer;
use stats::OnlineStats;

/// Baseline for compression ratios.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum Base {
    /// Compare with JSON stringify.
    Json,
    /// Compare with binary representation (8 bytes per value).
    ///
    /// This honors the base64 flag.
    Binary,
}

pub struct Args {
    /// Path to JSON file containing a stream of buckets.
    pub path: PathBuf,
    /// Sort input values before processing.
    pub sort: bool,
    /// base64 encode compressed output. Stats use base64 size instead.
    pub base64: bool,
    /// Skip buckets with fewer than this many values.
    pub min_values: Option<usize>,
    /// Baseline for compression ratios.
    pub base: Base,
}

impl fmt::Display for Args {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "  sort before compression: {}", self.sort)?;
        writeln!(f, "  encode as base64: {}", self.base64)?;
        writeln!(f, "  minimum value length: {:?}", self.min_values)?;
        writeln!(f, "  baseline: {:?}", self.base)?;

        Ok(())
    }
}

type CompressFn = dyn FnMut(&[f64]) -> Result<Vec<u8>>;
type DecompressFn = dyn FnMut(&[u8]) -> Result<Vec<f64>>;

pub struct Test {
    name: &'static str,
    write: Box<CompressFn>,
    read: Box<DecompressFn>,
    validate: bool,
}

impl Test {
    pub fn floats<W, R, E>(name: &'static str, mut write: W, mut read: R, validate: bool) -> Self
    where
        W: FnMut(&[f64]) -> Result<Vec<u8>, E> + 'static,
        R: FnMut(&[u8]) -> Result<Vec<f64>, E> + 'static,
        anyhow::Error: From<E>,
    {
        Self {
            name,
            write: Box::new(move |v| write(v).map_err(E::into)),
            read: Box::new(move |v| read(v).map_err(E::into)),
            validate,
        }
    }

    pub fn binary<W, R, E>(name: &'static str, mut write: W, mut read: R, validate: bool) -> Self
    where
        W: FnMut(&[u8]) -> Result<Vec<u8>, E> + 'static,
        R: FnMut(&[u8]) -> Result<Vec<u8>, E> + 'static,
        anyhow::Error: From<E>,
    {
        // NB: These functions transmute the machine-specific binary representation of the values.
        // In production code, this should be replaced with a portable binary representation that
        // writes into encoding writers.
        Self {
            name,
            write: Box::new(move |v| write(as_bytes(v)).map_err(E::into)),
            read: Box::new(move |v| Ok(into_floats(read(v)?))),
            validate,
        }
    }
}

fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn into_floats(mut buf: Vec<u8>) -> Vec<f64> {
    buf.shrink_to_fit();

    let size = std::mem::size_of::<f64>();
    let len = buf.len() / size;
    assert!(buf.len() % size == 0);

    let floats = unsafe { Vec::from_raw_parts(buf.as_mut_ptr() as *mut f64, len, len) };
    std::mem::forget(buf);
    floats
}

pub struct Benchmark {
    args: Args,
    tests: Vec<Test>,
    stats: BTreeMap<&'static str, TestStats>,
    total_size: usize,
}

impl Benchmark {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            tests: Vec::new(),
            stats: BTreeMap::new(),
            total_size: 0,
        }
    }

    pub fn add(&mut self, test: Test) -> &mut Self {
        self.tests.push(test);
        self
    }

    pub fn run(&mut self) -> Result<()> {
        let json = std::fs::read_to_string(&self.args.path)?;

        let lines = json.lines().count();
        let bar = ProgressBar::new(lines as u64);

        // Read the JSON contents of the file as an instance of `User`.
        for result in Deserializer::from_str(&json).into_iter::<MiniBucket>() {
            self.process_bucket(result?)?;
            bar.inc(1);
        }

        bar.finish_and_clear();

        self.print_results();

        Ok(())
    }

    fn print_results(&mut self) {
        println!("Compression ratios (100% is equal to JSON input):");
        let mut rows: Vec<_> = self
            .stats
            .iter()
            .map(|(name, s)| s.compress_row(name, self.total_size))
            .collect();

        rows.sort_by_key(|row| float_ord::FloatOrd(row.mean));
        let _ = cli_table::print_stdout(rows.with_title());

        println!();
        println!("Relative errors (lossless algorithms only):");
        let mut rows: Vec<_> = self
            .stats
            .iter()
            .filter_map(|(name, s)| s.error_row(name))
            .collect();

        rows.sort_by_key(|row| float_ord::FloatOrd(row.mean));
        let _ = cli_table::print_stdout(rows.with_title());
    }

    fn process_bucket(&mut self, mut bucket: MiniBucket) -> Result<()> {
        if let Some(min_values) = self.args.min_values {
            if bucket.value.len() < min_values {
                return Ok(());
            }
        }

        if self.args.sort {
            float_ord::sort(&mut bucket.value);
        }

        let base_size = match self.args.base {
            Base::Json => serde_json::to_vec(&bucket.value)?.len(),
            Base::Binary if self.args.base64 => STANDARD.encode(as_bytes(&bucket.value)).len(),
            Base::Binary => bucket.value.len() * std::mem::size_of::<f64>(),
        };
        self.total_size += base_size;

        for test in &mut self.tests {
            let stats = self.stats.entry(test.name).or_default();

            let mut compressed = (test.write)(&bucket.value)?;

            if test.validate {
                let decompressed = (test.read)(&compressed)?;
                assert_eq!(decompressed.len(), bucket.value.len());

                let mut errors = Stats::default();
                for (a, b) in bucket.value.iter().zip(decompressed.iter()) {
                    errors.add(relative_error(a, b));
                }
                stats.add_errors(errors);
            }

            if self.args.base64 {
                compressed = STANDARD.encode(&compressed).into_bytes();
            }

            stats.add(compressed.len(), base_size);
        }

        Ok(())
    }
}

fn relative_error(original: &f64, decoded: &f64) -> f64 {
    let abs_error = (original - decoded).abs();

    if original.abs() < f64::EPSILON {
        if abs_error < f64::EPSILON {
            0.0 // both are zero, no error
        } else {
            1.0 // original value is zero, assume 100% error
        }
    } else {
        abs_error / original.abs() // actual ratio
    }
}

#[derive(Debug, Deserialize)]
struct MiniBucket {
    value: Vec<f64>,
}

#[derive(Debug, Default)]
struct Stats {
    minmax: stats::MinMax<f64>,
    stats: OnlineStats,
}

impl Stats {
    pub fn add(&mut self, value: f64) {
        self.minmax.add(value);
        self.stats.add(value);
    }
}

#[derive(Debug, Default)]
struct TestStats {
    compression: Stats,
    error: Stats,
    total_size: usize,
}

impl TestStats {
    pub fn add(&mut self, size: usize, base: usize) {
        self.compression.add(size as f64 / base as f64);
        self.total_size += size;
    }

    pub fn add_errors(&mut self, errors: Stats) {
        if let Some(&min) = errors.minmax.min() {
            self.error.minmax.add(min);
        }
        if let Some(&max) = errors.minmax.max() {
            self.error.minmax.add(max);
        }

        // Track statistics over the mean error
        self.error.stats.add(errors.stats.mean());
    }

    pub fn compress_row(&self, name: &'static str, base_total: usize) -> CompressRow {
        CompressRow {
            name,
            mean: self.compression.stats.mean() * 100.0,
            stddev: self.compression.stats.stddev() * 100.0,
            min: self.compression.minmax.min().copied().unwrap_or_default() * 100.0,
            max: self.compression.minmax.max().copied().unwrap_or_default() * 100.0,
            total_size: self.total_size,
            total_ratio: self.total_size as f64 / base_total as f64 * 100.0,
        }
    }

    pub fn error_row(&self, name: &'static str) -> Option<ErrorRow> {
        if self.error.stats.len() == 0 {
            return None;
        }

        Some(ErrorRow {
            name,
            mean: self.error.stats.mean() * 100.0,
            stddev: self.error.stats.stddev() * 100.0,
            min: self.error.minmax.min().copied().unwrap_or_default() * 100.0,
            max: self.error.minmax.max().copied().unwrap_or_default() * 100.0,
        })
    }
}

#[derive(Debug, Table)]
struct CompressRow {
    #[table(title = "Algorithm")]
    name: &'static str,
    #[table(title = "Mean (%)")]
    mean: f64,
    #[table(title = "Deviation (%)")]
    stddev: f64,
    #[table(title = "Min (%)")]
    min: f64,
    #[table(title = "Max (%)")]
    max: f64,
    #[table(title = "Total (bytes)", justify = "Justify::Right")]
    total_size: usize,
    #[table(title = "Total (%)")]
    total_ratio: f64,
}

#[derive(Debug, Table)]
struct ErrorRow {
    #[table(title = "Algorithm")]
    name: &'static str,
    #[table(title = "Mean (%)")]
    mean: f64,
    #[table(title = "Deviation (%)")]
    stddev: f64,
    #[table(title = "Min (%)")]
    min: f64,
    #[table(title = "Max (%)")]
    max: f64,
}
