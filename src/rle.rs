//! CSW run-length codec, shared by v1 and v2: one byte per pulse length, a
//! length over 255 escaped as `0x00` followed by the length as a little-endian
//! u32 (`0xCDE9` -> `00 E9 CD 00 00`).

use std::io::Read;

use crate::error::{Error, Result};

/// Pulse lengths read one at a time from an RLE byte stream, in constant
/// memory. `decode` is the same codec collecting into a `Vec`.
pub struct Decoder<R: Read> {
    inner: R,
    buf: [u8; 5],
}

impl<R: Read> Decoder<R> {
    pub fn new(inner: R) -> Self {
        Decoder {
            inner,
            buf: [0u8; 5],
        }
    }

    /// Read as many bytes as are there, up to `n`. A short read is the end of
    /// the stream, which the caller reads as trailing padding.
    fn fill(&mut self, n: usize) -> Result<usize> {
        let mut got = 0;
        while got < n {
            match self.inner.read(&mut self.buf[got..n]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(Error::Format(format!("bad pulse stream: {e}"))),
            }
        }
        Ok(got)
    }
}

impl<R: Read> Iterator for Decoder<R> {
    type Item = Result<u32>;

    fn next(&mut self) -> Option<Result<u32>> {
        match self.fill(1) {
            Err(e) => return Some(Err(e)),
            Ok(0) => return None,
            Ok(_) => {}
        }
        let b = self.buf[0];
        if b != 0 {
            return Some(Ok(b as u32));
        }
        // The escape: a zero byte, then the length as a little-endian u32.
        // Fewer than four bytes behind it is padding or truncation, and ends
        // the stream.
        match self.fill(4) {
            Err(e) => Some(Err(e)),
            Ok(4) => Some(Ok(u32::from_le_bytes([
                self.buf[0],
                self.buf[1],
                self.buf[2],
                self.buf[3],
            ]))),
            Ok(_) => None,
        }
    }
}

/// Decode an RLE byte stream into pulse lengths.
///
/// A zero-length pulse, which only the four-byte form can express, is
/// accepted: it flips the level with no samples between, and the survey's
/// `total_samples` stops summing at it.
#[cfg(test)]
pub fn decode(data: &[u8]) -> Vec<u32> {
    let mut pulses = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if b != 0 {
            pulses.push(b as u32);
        } else {
            if i + 4 > data.len() {
                break; // trailing padding / truncation
            }
            let n = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            pulses.push(n);
            i += 4;
        }
    }
    pulses
}

/// Encode pulse lengths into an RLE byte stream.
///
/// A zero-length pulse has no representation and is rejected.
pub fn encode(pulses: &[u32]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(pulses.len());
    for (index, &length) in pulses.iter().enumerate() {
        if length == 0 {
            return Err(Error::Format(format!(
                "pulse {index} is 0 samples: a pulse lasts at least one sample"
            )));
        }
        if length <= 255 {
            out.push(length as u8);
        } else {
            out.push(0);
            out.extend_from_slice(&length.to_le_bytes());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_roundtrip() {
        let pulses = vec![3, 5, 1, 300, 7];
        let bytes = encode(&pulses).unwrap();
        assert_eq!(bytes, vec![3, 5, 1, 0, 0x2C, 0x01, 0x00, 0x00, 7]);
        assert_eq!(decode(&bytes), pulses);
    }

    #[test]
    fn plain_bytes() {
        let pulses = vec![255, 1, 128];
        assert_eq!(decode(&encode(&pulses).unwrap()), pulses);
    }

    #[test]
    fn encode_rejects_a_zero_length_pulse() {
        assert!(encode(&[3, 0, 5]).is_err());
        assert!(encode(&[0]).is_err());
        assert_eq!(encode(&[3, 5]).unwrap(), vec![3, 5]); // still fine
    }

    #[test]
    fn decode_takes_a_zero_length_escape() {
        // The four-byte form is the only way a zero can reach a pulse list:
        // 03 | 00 00000000 | 05.
        let stream = vec![3, 0, 0, 0, 0, 0, 5];
        assert_eq!(decode(&stream), vec![3, 0, 5]);
    }

    #[test]
    fn decode_still_takes_a_real_escape_and_a_truncated_tail() {
        assert_eq!(decode(&[3, 0, 0x2C, 0x01, 0x00, 0x00, 7]), vec![3, 300, 7]);
        // A short tail after the marker is padding, not a malformed pulse.
        assert_eq!(decode(&[3, 0, 1, 2]), vec![3]);
    }
}
