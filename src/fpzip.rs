use std::fmt;

use float_ord::FloatOrd;
use fpzip_sys::*;

#[derive(Debug)]
pub struct CompressError(&'static str);

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "compression failed: {}", self.0)
    }
}

impl std::error::Error for CompressError {}

#[derive(Clone, Copy, Debug)]
pub struct Compression {
    precision: u32,
}

impl Compression {
    pub fn fixed_precision(precision: u32) -> Self {
        Self { precision }
    }

    pub fn reversible() -> Self {
        Self { precision: 0 }
    }
}

pub fn compress_slice<T>(slice: &[T], opts: Compression) -> Result<Vec<u8>, CompressError>
where
    T: Compress,
{
    unsafe { compress_slice_inner(slice, opts) }
}

unsafe fn compress_slice_inner<T>(slice: &[T], opts: Compression) -> Result<Vec<u8>, CompressError>
where
    T: Compress,
{
    if slice.len() > i32::MAX as usize {
        return Err(CompressError("slice too large"));
    }

    let max_size = std::mem::size_of_val(slice) + 192; // 192 is the header size
    let mut buf: Vec<u8> = vec![0; max_size];

    let fpz = fpzip_write_to_buffer(buf.as_mut_ptr() as *mut _, max_size);
    if fpz.is_null() {
        return Err(CompressError("could not create buffer"));
    }

    (*fpz).type_ = T::fpzip_type() as i32;
    (*fpz).prec = opts.precision as i32;
    (*fpz).nx = slice.len() as i32;
    (*fpz).ny = 1;
    (*fpz).nz = 1;
    (*fpz).nf = 1;

    // TODO: Is decompression possible w/o the header?
    let res = fpzip_write_header(fpz);
    if res == 0 {
        fpzip_write_close(fpz);
        return Err(CompressError("could not write header"));
    }

    let compressed_size = fpzip_write(fpz, slice.as_ptr() as *const _);

    // Deallocate the fpzip stream
    fpzip_write_close(fpz);

    if compressed_size == 0 {
        eprintln!("prec {}", opts.precision);
        Err(CompressError("could not compress"))
    } else {
        buf.truncate(compressed_size + 192);
        Ok(buf)
    }
}

pub fn decompress_slice<T>(slice: &[u8]) -> Result<Vec<T>, CompressError>
where
    T: Compress + Copy,
{
    unsafe { decompress_slice_inner(slice) }
}

unsafe fn decompress_slice_inner<T>(slice: &[u8]) -> Result<Vec<T>, CompressError>
where
    T: Compress + Copy,
{
    let fpz = fpzip_read_from_buffer(slice.as_ptr() as *const _);
    if fpz.is_null() {
        return Err(CompressError("could not open buffer"));
    }

    let res = fpzip_read_header(fpz);
    if res == 0 {
        fpzip_read_close(fpz);
        return Err(CompressError("could not read header"));
    }

    if (*fpz).type_ != T::fpzip_type() as i32 {
        fpzip_read_close(fpz);
        return Err(CompressError("invalid type"));
    }

    let mut buf: Vec<T> = vec![std::mem::zeroed(); (*fpz).nx as usize];
    let decompressed_size = fpzip_read(fpz, buf.as_mut_ptr() as *mut _);

    // Deallocate the fpzip stream
    fpzip_read_close(fpz);

    if decompressed_size == 0 {
        Err(CompressError("could not decompress"))
    } else {
        Ok(buf)
    }
}

pub trait Compress {
    fn fpzip_type() -> u32;
}

impl Compress for f32 {
    fn fpzip_type() -> u32 {
        FPZIP_TYPE_FLOAT
    }
}

impl Compress for f64 {
    fn fpzip_type() -> u32 {
        FPZIP_TYPE_DOUBLE
    }
}

impl<T: Compress> Compress for FloatOrd<T> {
    fn fpzip_type() -> u32 {
        T::fpzip_type()
    }
}
