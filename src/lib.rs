use std::convert::Infallible;

pub mod fpzip;
pub mod stats;
pub mod zfp;

mod flate;
mod tsz_f64;

use crate::stats::Test;

pub fn tests() -> impl IntoIterator<Item = Test> {
    [
        Test::floats(
            "json uncompressed",
            serde_json::to_vec,
            |slice| serde_json::from_slice(slice),
            false,
        ),
        Test::binary::<_, _, Infallible>(
            "binary uncompressed",
            |values| Ok(values.to_vec()),
            |slice| Ok(slice.to_vec()),
            false,
        ),
        Test::binary(
            "lz4",
            |values| lz4::block::compress(values, None, false),
            |slice| lz4::block::decompress(slice, None),
            false,
        ),
        Test::binary(
            "deflate(1)",
            |values| flate::compress_deflate(values, flate2::Compression::fast()),
            flate::decompress_deflate,
            false,
        ),
        Test::binary(
            "deflate(6)",
            |values| flate::compress_deflate(values, flate2::Compression::default()),
            flate::decompress_deflate,
            false,
        ),
        Test::binary(
            "gzip(1)",
            |values| flate::compress_gz(values, flate2::Compression::fast()),
            flate::decompress_gz,
            false,
        ),
        Test::binary(
            "gzip(6)",
            |values| flate::compress_gz(values, flate2::Compression::default()),
            flate::decompress_gz,
            false,
        ),
        Test::binary(
            "zlib(1)",
            |values| flate::compress_zlib(values, flate2::Compression::fast()),
            flate::decompress_zlib,
            false,
        ),
        Test::binary(
            "zlib(6)",
            |values| flate::compress_zlib(values, flate2::Compression::default()),
            flate::decompress_zlib,
            false,
        ),
        Test::binary(
            "zstd(1)",
            |values| flate::compress_zstd(values, 1),
            flate::decompress_zstd,
            false,
        ),
        Test::binary(
            "zstd(3)",
            |values| flate::compress_zstd(values, zstd::DEFAULT_COMPRESSION_LEVEL),
            flate::decompress_zstd,
            false,
        ),
        Test::floats(
            "tsz",
            tsz_f64::compress_tsz,
            tsz_f64::decompress_tsz,
            false, /* NB: the tsz implementation is broken */
        ),
        Test::floats(
            "zfp reversible",
            |values| zfp::compress_slice(values, zfp::Compression::reversible()),
            zfp::decompress_slice,
            true,
        ),
        Test::floats(
            "zfp 24bit",
            |values| zfp::compress_slice(values, zfp::Compression::fixed_precision(24)),
            zfp::decompress_slice,
            true,
        ),
        Test::floats(
            "zfp 16bit",
            |values| zfp::compress_slice(values, zfp::Compression::fixed_precision(16)),
            zfp::decompress_slice,
            true,
        ),
        Test::floats(
            "fpzip reversible",
            |values| fpzip::compress_slice(values, fpzip::Compression::reversible()),
            fpzip::decompress_slice,
            true,
        ),
        Test::floats(
            "fpzip 24bit",
            |values| fpzip::compress_slice(values, fpzip::Compression::fixed_precision(24)),
            fpzip::decompress_slice,
            true,
        ),
        Test::floats(
            "fpzip 16bit",
            |values| fpzip::compress_slice(values, fpzip::Compression::fixed_precision(16)),
            fpzip::decompress_slice,
            true,
        ),
    ]
}
