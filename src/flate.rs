use std::io::{Result, Write};

use flate2::Compression;
use zstd::zstd_safe::CompressionLevel;

pub fn compress_deflate(values: &[u8], level: Compression) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), level);
    encoder.write_all(values)?;
    encoder.finish()
}

pub fn decompress_deflate(slice: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::write::DeflateDecoder::new(Vec::new());
    decoder.write_all(slice)?;
    decoder.finish()
}

pub fn compress_gz(values: &[u8], level: Compression) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), level);
    encoder.write_all(values)?;
    encoder.finish()
}

pub fn decompress_gz(slice: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::write::GzDecoder::new(Vec::new());
    decoder.write_all(slice)?;
    decoder.finish()
}

pub fn compress_zlib(values: &[u8], level: Compression) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), level);
    encoder.write_all(values)?;
    encoder.finish()
}

pub fn decompress_zlib(slice: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::write::ZlibDecoder::new(Vec::new());
    decoder.write_all(slice)?;
    decoder.finish()
}

pub fn compress_zstd(values: &[u8], level: CompressionLevel) -> Result<Vec<u8>> {
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), level)?;
    encoder.write_all(values)?;
    encoder.finish()
}

pub fn decompress_zstd(slice: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = zstd::stream::write::Decoder::new(Vec::new())?;
    decoder.write_all(slice)?;
    decoder.flush()?;
    Ok(decoder.into_inner())
}
