use std::io;

use tsz::decode::Error;
use tsz::stream::{BufferedReader, BufferedWriter, Read, Write};
use tsz::Bit;

// END_MARKER relies on the fact that when we encode the delta of delta for a number that requires
// more than 12 bits we write four control bits 1111 followed by the 32 bits of the value. Since
// encoding assumes the value is greater than 12 bits, we can store the value 0 to signal the end
// of the stream

/// END_MARKER is a special bit sequence used to indicate the end of the stream
pub const END_MARKER: u64 = 0b1111_0000_0000_0000_0000_0000_0000_0000_0000;

pub fn compress_tsz(values: &[f64]) -> io::Result<Vec<u8>> {
    let mut encoder = TszF64Encoder::new(BufferedWriter::new());
    for value in values {
        encoder.encode(*value);
    }
    Ok(encoder.close().into_vec())
}

pub fn decompress_tsz(slice: &[u8]) -> io::Result<Vec<f64>> {
    let data = slice.to_vec().into_boxed_slice();
    let mut decoder = TszF64Decoder::new(BufferedReader::new(data));
    let mut values = Vec::new();
    while let Ok(value) = decoder.decode() {
        values.push(value);
    }
    Ok(values)
}

/// TszF64Encoder
///
/// TszF64Encoder is used to encode `DataPoint`s
#[derive(Debug)]
pub struct TszF64Encoder<T: Write> {
    value_bits: u64, // current float value as bits

    // store the number of leading and trailing zeroes in the current xor as u32 so we
    // don't have to do any conversions after calling `leading_zeros` and `trailing_zeros`
    leading_zeroes: u32,
    trailing_zeroes: u32,

    first: bool, // will next DataPoint be the first DataPoint encoded

    w: T,
}

impl<T> TszF64Encoder<T>
where
    T: Write,
{
    /// new creates a new TszF64Encoder whose starting timestamp is `start` and writes its encoded
    /// bytes to `w`
    pub fn new(w: T) -> Self {
        Self {
            value_bits: 0,
            leading_zeroes: 64,  // 64 is an initial sentinel value
            trailing_zeroes: 64, // 64 is an intitial sentinel value
            first: true,
            w,
        }
    }

    fn write_first(&mut self, value_bits: u64) {
        self.value_bits = value_bits;

        // write one control bit so we can distinguish a stream which contains only an initial
        // timestamp, this assumes the first bit of the END_MARKER is 1
        self.w.write_bit(Bit::Zero);

        // store the first value exactly
        self.w.write_bits(self.value_bits, 64);

        self.first = true
    }

    fn write_next_value(&mut self, value_bits: u64) {
        let xor = value_bits ^ self.value_bits;
        self.value_bits = value_bits;

        if xor == 0 {
            // if xor with previous value is zero just store single zero bit
            self.w.write_bit(Bit::Zero);
        } else {
            self.w.write_bit(Bit::One);

            let leading_zeroes = xor.leading_zeros();
            let trailing_zeroes = xor.trailing_zeros();

            if leading_zeroes >= self.leading_zeroes && trailing_zeroes >= self.trailing_zeroes {
                // if the number of leading and trailing zeroes in this xor are >= the leading and
                // trailing zeroes in the previous xor then we only need to store a control bit and
                // the significant digits of this xor
                self.w.write_bit(Bit::Zero);
                self.w.write_bits(
                    xor.wrapping_shr(self.trailing_zeroes),
                    64 - self.leading_zeroes - self.trailing_zeroes,
                );
            } else {
                // if the number of leading and trailing zeroes in this xor are not less than the
                // leading and trailing zeroes in the previous xor then we store a control bit and
                // use 6 bits to store the number of leading zeroes and 6 bits to store the number
                // of significant digits before storing the significant digits themselves

                self.w.write_bit(Bit::One);
                self.w.write_bits(u64::from(leading_zeroes), 6);

                // if significant_digits is 64 we cannot encode it using 6 bits, however since
                // significant_digits is guaranteed to be at least 1 we can subtract 1 to ensure
                // significant_digits can always be expressed with 6 bits or less
                let significant_digits = 64 - leading_zeroes - trailing_zeroes;
                self.w.write_bits(u64::from(significant_digits - 1), 6);
                self.w
                    .write_bits(xor.wrapping_shr(trailing_zeroes), significant_digits);

                // finally we need to update the number of leading and trailing zeroes
                self.leading_zeroes = leading_zeroes;
                self.trailing_zeroes = trailing_zeroes;
            }
        }
    }

    pub fn encode(&mut self, value: f64) {
        let value_bits = value.to_bits();

        if self.first {
            self.write_first(value_bits);
            self.first = false;
            return;
        }

        self.write_next_value(value_bits)
    }

    pub fn close(mut self) -> Box<[u8]> {
        self.w.write_bits(END_MARKER, 36);
        self.w.close()
    }
}

/// StdDecoder
///
/// StdDecoder is used to decode `DataPoint`s
#[derive(Debug)]
pub struct TszF64Decoder<T: Read> {
    value_bits: u64, // current float value as bits

    leading_zeroes: u32,  // leading zeroes
    trailing_zeroes: u32, // trailing zeroes

    first: bool, // will next DataPoint be the first DataPoint decoded
    done: bool,

    r: T,
}

impl<T> TszF64Decoder<T>
where
    T: Read,
{
    /// new creates a new StdDecoder which will read bytes from r
    pub fn new(r: T) -> Self {
        Self {
            value_bits: 0,
            leading_zeroes: 0,
            trailing_zeroes: 0,
            first: true,
            done: false,
            r,
        }
    }
    fn read_first_value(&mut self) -> Result<u64, Error> {
        self.r.read_bits(64).map_err(Error::Stream).map(|bits| {
            self.value_bits = bits;
            self.value_bits
        })
    }

    fn read_next_value(&mut self) -> Result<u64, Error> {
        let contol_bit = self.r.read_bit()?;

        if contol_bit == Bit::Zero {
            return Ok(self.value_bits);
        }

        let zeroes_bit = self.r.read_bit()?;

        if zeroes_bit == Bit::One {
            self.leading_zeroes = self.r.read_bits(6).map(|n| n as u32)?;
            let significant_digits = self.r.read_bits(6).map(|n| (n + 1) as u32)?;
            self.trailing_zeroes = 64 - self.leading_zeroes - significant_digits;
        }

        let size = 64 - self.leading_zeroes - self.trailing_zeroes;
        self.r.read_bits(size).map_err(Error::Stream).map(|bits| {
            self.value_bits ^= bits << self.trailing_zeroes;
            self.value_bits
        })
    }

    pub fn decode(&mut self) -> Result<f64, Error> {
        if self.done {
            return Err(Error::EndOfStream);
        }

        let value_bits = if self.first {
            self.first = false;
            self.read_first_value()?
        } else {
            self.read_next_value()?
        };

        let value = f64::from_bits(value_bits);
        Ok(value)
    }
}
