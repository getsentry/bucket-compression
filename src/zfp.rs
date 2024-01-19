use std::fmt;

use float_ord::FloatOrd;
use zfp_sys::*;

// https://zfp.readthedocs.io/en/release1.0.1/high-level-api.html

#[non_exhaustive]
#[derive(Debug)]
pub struct CompressError;

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "compression failed")
    }
}

impl std::error::Error for CompressError {}

#[derive(Clone, Copy, Debug)]
enum CompressionMode {
    Rate(f64, zfp_bool),
    Precision(u32),
    Accuracy(f64),
    Reversible,
}

/// Specifies how the data is to be compressed to meet various constraints on accuracy or size.
///
/// In streaming I/O applications, the fixed-accuracy mode is preferred, as it provides the highest
/// quality (in the absolute error sense) per bit of compressed storage.
#[derive(Clone, Copy, Debug)]
pub struct Compression {
    mode: CompressionMode,
}

impl Compression {
    /// Set rate for [fixed-rate] mode in compressed bits per value.
    ///
    /// The Boolean `align` should be `true` if word alignment is needed, e.g., to support random
    /// access writes of blocks for zfp’s compressed arrays. Such alignment may further constrain
    /// the rate.
    ///
    /// In fixed-rate mode, each d-dimensional compressed block of 4d values is stored using a fixed
    /// number. Fixed-rate mode is needed to support random access to blocks, and also is the mode
    /// used in the implementation of zfp’s compressed arrays. Fixed-rate mode also ensures a
    /// predictable memory/storage footprint, but usually results in far worse accuracy per bit than
    /// the variable-rate fixed-precision and fixed-accuracy modes.
    ///
    /// **Note**: Use fixed-rate mode only if you have to bound the compressed size or need read and
    /// write random access to blocks.
    ///
    /// [fixed-rate]: https://zfp.readthedocs.io/en/latest/modes.html#fixed-rate-mode
    pub fn fixed_rate(rate: f64, align: bool) -> Self {
        Self {
            mode: CompressionMode::Rate(rate, if align { 1 } else { 0 }),
        }
    }

    /// Set precision for [fixed-precision] mode.
    ///
    /// The precision specifies how many uncompressed bits per value to store, and indirectly
    /// governs the relative error. In fixed-precision mode, the number of bits used to encode a
    /// block may vary, but the number of bit planes (i.e., the precision) encoded for the transform
    /// coefficients is fixed.
    ///
    /// To preserve a certain floating-point mantissa or integer precision in the decompressed data,
    /// see [FAQ 21].
    ///
    /// **Note**: Fixed-precision mode is preferable when relative rather than absolute errors
    /// matter.
    ///
    /// [fixed-precision]: https://zfp.readthedocs.io/en/latest/modes.html#fixed-precision-mode
    /// [FAQ 21]: https://zfp.readthedocs.io/en/latest/faq.html#q-lossless
    pub fn fixed_precision(precision: u32) -> Self {
        Self {
            mode: CompressionMode::Precision(precision),
        }
    }

    /// Set absolute error tolerance for [fixed-accuracy] mode.
    ///
    /// The tolerance ensures that values in the decompressed array differ from the input array by
    /// no more than this tolerance (in all but exceptional circumstances; see [FAQ 17]). This
    /// compression mode should be used only with floating-point (not integer) data.
    ///
    /// [fixed-accuracy]: https://zfp.readthedocs.io/en/latest/modes.html#fixed-accuracy-mode
    /// [FAQ 17]: https://zfp.readthedocs.io/en/latest/faq.html#q-tolerance
    pub fn fixed_accuracy(accuracy: f64) -> Self {
        Self {
            mode: CompressionMode::Accuracy(accuracy),
        }
    }

    /// Enable reversible (lossless) compression.
    ///
    /// As with the other compression modes, each block is compressed and decompressed
    /// independently, but reversible mode uses a different compression algorithm that ensures a
    /// bit-for-bit identical reconstruction of integer and floating-point data. For IEEE-754
    /// floating-point data, reversible mode preserves special values such as subnormals,
    /// infinities, NaNs, and positive and negative zero.
    ///
    /// [reversible]: https://zfp.readthedocs.io/en/latest/modes.html#reversible-mode
    pub fn reversible() -> Self {
        Self {
            mode: CompressionMode::Reversible,
        }
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
    let data_type = T::zfp_type();
    debug_assert_eq!(
        zfp_type_size(data_type),
        std::mem::size_of::<T>(),
        "invalid ZFP type mapping"
    );

    // allocate meta data for a compressed stream
    let stream = zfp_stream_open(std::ptr::null_mut());

    // set compression mode and parameters via one of three functions
    // see https://zfp.readthedocs.io/en/latest/modes.html

    match opts.mode {
        CompressionMode::Rate(rate, align) => {
            zfp_stream_set_rate(stream, rate, data_type, 1, align);
        }
        CompressionMode::Precision(precision) => {
            zfp_stream_set_precision(stream, precision);
        }
        CompressionMode::Accuracy(accuracy) if T::supports_accuracy() => {
            zfp_stream_set_accuracy(stream, accuracy);
        }
        CompressionMode::Reversible => {
            zfp_stream_set_reversible(stream);
        }
        _ => {
            zfp_stream_close(stream);
            return Err(CompressError);
        }
    }

    let field = zfp_field_1d(
        slice.as_ptr() as *mut std::ffi::c_void, // TODO(ja): not mut?
        data_type,
        slice.len(),
    );

    // allocate buffer for compressed data
    let max_size = zfp_stream_maximum_size(stream, field);
    let mut buffer: Vec<u8> = vec![0; max_size as usize];

    // associate buffer as bit stream with allocated buffer
    let bitstream = stream_open(buffer.as_mut_ptr() as *mut std::ffi::c_void, max_size);
    zfp_stream_set_bit_stream(stream, bitstream);
    zfp_stream_rewind(stream);

    // compress array and output compressed stream
    zfp_write_header(stream, field, ZFP_HEADER_FULL); // TODO: Check errors
    let compressed_size = zfp_compress(stream, field);

    // cleanup before returning
    zfp_field_free(field);
    zfp_stream_close(stream);
    stream_close(bitstream);

    if compressed_size == 0 {
        Err(CompressError)
    } else {
        // TODO: Added this, correct?
        buffer.truncate(compressed_size);
        Ok(buffer)
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
    let data_type = T::zfp_type();
    debug_assert_eq!(
        zfp_type_size(data_type),
        std::mem::size_of::<T>(),
        "invalid ZFP type mapping"
    );

    let bitstream = stream_open(slice.as_ptr() as *mut std::ffi::c_void, slice.len());
    let stream = zfp_stream_open(bitstream);
    let field = zfp_field_alloc();

    if zfp_read_header(stream, field, ZFP_HEADER_FULL) == 0 {
        zfp_field_free(field);
        zfp_stream_close(stream);
        return Err(CompressError);
    }

    let len = zfp_field_size(field, std::ptr::null_mut());
    let mut buffer: Vec<T> = vec![std::mem::zeroed(); len as usize];
    zfp_field_set_pointer(field, buffer.as_mut_ptr() as *mut std::ffi::c_void);
    let decompressed_size = zfp_decompress(stream, field);

    // cleanup before returning
    zfp_field_free(field);
    zfp_stream_close(stream);
    stream_close(bitstream);

    if decompressed_size == 0 {
        Err(CompressError)
    } else {
        Ok(buffer)
    }
}

pub trait Compress {
    fn zfp_type() -> zfp_type;

    fn max_precision() -> u32;

    fn supports_accuracy() -> bool {
        false
    }
}

impl Compress for i32 {
    fn zfp_type() -> zfp_type {
        zfp_type_zfp_type_int32
    }

    fn max_precision() -> u32 {
        Self::BITS
    }
}

impl Compress for i64 {
    fn zfp_type() -> zfp_type {
        zfp_type_zfp_type_int64
    }

    fn max_precision() -> u32 {
        Self::BITS
    }
}

impl Compress for f32 {
    fn zfp_type() -> zfp_type {
        zfp_type_zfp_type_float
    }

    fn max_precision() -> u32 {
        Self::MANTISSA_DIGITS
    }

    fn supports_accuracy() -> bool {
        true
    }
}

impl Compress for f64 {
    fn zfp_type() -> zfp_type {
        zfp_type_zfp_type_double
    }

    fn max_precision() -> u32 {
        Self::MANTISSA_DIGITS
    }

    fn supports_accuracy() -> bool {
        true
    }
}

impl<T: Compress> Compress for FloatOrd<T> {
    fn zfp_type() -> zfp_type {
        T::zfp_type()
    }

    fn max_precision() -> u32 {
        T::max_precision()
    }

    fn supports_accuracy() -> bool {
        T::supports_accuracy()
    }
}
