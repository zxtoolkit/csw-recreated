//! CSW v1.01 and v2.00 container read/write: signature, 0x1A, major, minor,
//! then rate (WORD in v1, DWORD in v2), v2's pulse count, compression (1 =
//! RLE, 2 = Z-RLE), flags (bit 0 = initial high), and v2's extension and app.

#[cfg(test)]
use std::io::Read;

use crate::error::{Error, Result};
use crate::rle;
use crate::signal::{PulseSource, Pulses};
use flate2::read::ZlibDecoder;

pub const SIGNATURE: &[u8; 22] = b"Compressed Square Wave";
pub const TERMINATOR: u8 = 0x1A;

/// The RLE stream's framing: plain, or deflated ("Z-RLE"). Z-RLE is the
/// default; `-z` selects plain RLE (the "old compression method"), and `-1`
/// writes a v1 file, which has only plain RLE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Rle,
    ZRle,
}

/// Header metadata surfaced for the decode status messages.
pub struct CswInfo {
    pub major: u8,
    pub minor: u8,
    pub compression: u8,
    /// The encoding-application field (v2 only; empty for v1): the bytes
    /// before the first NUL of the sixteen, or all sixteen followed by the
    /// little-endian bytes of the payload length up to its first zero byte,
    /// an unterminated field running on into the dword behind it. Bytes, not
    /// text.
    pub app: Vec<u8>,
    /// The pulse count the v2 header declares at 0x1D (`None` for v1, which
    /// has no such field). Parsed and stored, read by nothing: a decode walks
    /// the pulse data whatever the count says, so a file declaring 99 pulses
    /// and carrying four converts, in silence.
    pub declared_pulses: Option<u32>,
    /// Sample rate from the header.
    pub rate: u32,
    /// Initial polarity (flags bit 0).
    pub initial_high: bool,
    /// Where the pulse data starts, past the header and its extension.
    pub data_offset: usize,
}

/// Parse a CSW v1 or v2 file into pulses (see `read_info` for the metadata).
#[cfg(test)]
pub(crate) fn read(raw: &[u8]) -> Result<Pulses> {
    Ok(read_info(raw)?.0)
}

/// Parse a CSW file, also returning its header metadata.
#[cfg(test)]
pub(crate) fn read_info(raw: &[u8]) -> Result<(Pulses, CswInfo)> {
    let info = read_header(raw)?;
    let pulses = decode_pulses(&info, &raw[info.data_offset..])?;
    Ok((pulses, info))
}

/// Parse the header alone, leaving the pulse data undecoded: the "Compressed
/// Square Wave v2.00 at 44100 Hz" line is printed before the compression type
/// is looked at, and an unsupported type is reported under it.
pub fn read_header(raw: &[u8]) -> Result<CswInfo> {
    if raw.len() < SIGNATURE.len() || &raw[..SIGNATURE.len()] != SIGNATURE {
        return Err(Error::Format("not a CSW file (bad signature)".into()));
    }
    // A file carrying the signature but not a whole header is abandoned in
    // silence: nothing after the banner, an empty output file, and only the
    // exit code reports it.
    if raw.len() < 0x20 {
        return Err(Error::Silent(format!(
            "truncated CSW file: {} bytes, the header needs 32",
            raw.len()
        )));
    }
    // The 0x1A at 0x16 is written but never read: only the 22 bytes of the
    // signature are compared, so a file whose terminator byte is 0x00 decodes
    // like any other. Keep writing it.
    let major = raw[0x17];
    let minor = raw[0x18];
    let mut declared_pulses = None;
    let (rate, comp, flags, app, data_offset): (u32, u8, u8, Vec<u8>, usize) = match major {
        1 => {
            let rate = u16::from_le_bytes([raw[0x19], raw[0x1A]]) as u32;
            (rate, raw[0x1B], raw[0x1C], Vec::new(), 0x20)
        }
        2 => {
            if raw.len() < 0x34 {
                return Err(Error::Silent("truncated CSW-2 header".into()));
            }
            let rate = u32::from_le_bytes([raw[0x19], raw[0x1A], raw[0x1B], raw[0x1C]]);
            declared_pulses = Some(u32::from_le_bytes([
                raw[0x1D], raw[0x1E], raw[0x1F], raw[0x20],
            ]));
            // The extension length is a **signed** byte, the same reading the
            // application name's fallback below takes: 255 is -1, so the pulse
            // data starts one byte *inside* the header and the last byte of the
            // application field is read as a pulse. A length whose offset lands
            // before the file is read from outside it, which this cannot follow.
            let start = 0x34i64 + i64::from(raw[0x23] as i8);
            let payload = raw.len() as i64 - start;
            if start < 0 || payload < -7 {
                return Err(Error::Silent("truncated CSW-2 header".into()));
            }
            let start = if payload < 0 {
                raw.len()
            } else {
                start as usize
            };
            let app_bytes = &raw[0x24..0x34];
            let app = match app_bytes.iter().position(|&b| b == 0) {
                Some(end) => app_bytes[..end].to_vec(),
                None => {
                    // No terminator: the print runs on into the payload
                    // length, `file length - 0x34 - extension` with the
                    // extension byte signed, until a zero byte stops it. What
                    // lies past those four bytes is not modelled.
                    let tail = (payload as u32).to_le_bytes();
                    let run = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
                    [app_bytes, &tail[..run]].concat()
                }
            };
            (rate, raw[0x21], raw[0x22], app, start)
        }
        // Any other major is read with the **v1** layout; the version gate
        // is in the decode path, under the header line. A v2-shaped file
        // with another major thus takes its compression byte from 0x1B, the
        // third byte of its rate -- 0 for any ordinary rate, so it decodes
        // to silence.
        _ => {
            let rate = u16::from_le_bytes([raw[0x19], raw[0x1A]]) as u32;
            (rate, raw[0x1B], raw[0x1C], Vec::new(), 0x20)
        }
    };
    // A header declaring no sampling rate is abandoned in silence, like a
    // truncated one: every downstream length is a division by it.
    if rate == 0 {
        return Err(Error::Silent(
            "CSW header declares a sampling rate of 0".into(),
        ));
    }

    Ok(CswInfo {
        major,
        minor,
        compression: comp,
        app,
        declared_pulses,
        rate,
        initial_high: flags & 0x01 != 0,
        data_offset,
    })
}

fn unsupported_compression(other: u8) -> Error {
    Error::Input(
        format!("CSW compression type #{other} not supported, please upgrade this tool.").into(),
        251,
    )
}

/// The pulse data of a CSW file, walked in memory bounded by the file: each
/// walk inflates the payload afresh and feeds the RLE decoder.
pub struct CswPulses<'a> {
    data: &'a [u8],
    compression: u8,
    rate: u32,
    initial_high: bool,
}

/// Either compression's pulse stream, behind one type so `PulseSource` has one
/// iterator to name.
pub enum PulseReader<'a> {
    Rle(rle::Decoder<&'a [u8]>),
    ZRle(Box<rle::Decoder<ZlibDecoder<&'a [u8]>>>),
    /// Compression type 0: read as an empty stream.
    Empty,
}

impl Iterator for PulseReader<'_> {
    type Item = Result<u32>;
    fn next(&mut self) -> Option<Result<u32>> {
        match self {
            PulseReader::Rle(d) => d.next(),
            PulseReader::ZRle(d) => d.next(),
            PulseReader::Empty => None,
        }
    }
}

impl PulseSource for CswPulses<'_> {
    type Pulses<'s>
        = PulseReader<'s>
    where
        Self: 's;

    fn pulses(&self) -> Result<PulseReader<'_>> {
        match self.compression {
            1 => Ok(PulseReader::Rle(rle::Decoder::new(self.data))),
            2 => Ok(PulseReader::ZRle(Box::new(rle::Decoder::new(
                ZlibDecoder::new(self.data),
            )))),
            // Type 0 is neither of the two the format defines and reads as
            // an empty stream (a 44-byte WAV of nothing); a type *above* the
            // known two is an error.
            0 => Ok(PulseReader::Empty),
            other => Err(unsupported_compression(other)),
        }
    }
    fn rate(&self) -> u32 {
        self.rate
    }
    fn initial_high(&self) -> bool {
        self.initial_high
    }
}

/// The pulse data a [`read_header`] left, as something to walk. The
/// compression type is checked by the first walk, after the header line.
pub fn pulse_source<'a>(info: &CswInfo, data: &'a [u8]) -> CswPulses<'a> {
    CswPulses {
        data,
        compression: info.compression,
        rate: info.rate,
        initial_high: info.initial_high,
    }
}

/// Decompress and run-length-decode the pulse data into a `Vec`, for callers
/// that want the whole thing (the tests, `read_info`). The decode path walks
/// [`pulse_source`].
#[cfg(test)]
pub(crate) fn decode_pulses(info: &CswInfo, data: &[u8]) -> Result<Pulses> {
    let stream = match info.compression {
        1 => data.to_vec(),
        // Type 0 carries no pulses; see `pulse_source`.
        0 => Vec::new(),
        2 => zlib_inflate(data)?,
        other => return Err(unsupported_compression(other)),
    };

    let pulses = rle::decode(&stream);
    Ok(Pulses::new(info.rate, pulses, info.initial_high))
}

pub fn write(sig: &Pulses, version: u8, compression: Compression, app: &str) -> Result<Vec<u8>> {
    // v1 has plain RLE only; a caller asking for v1 Z-RLE is a bug.
    debug_assert!(!(version == 1 && compression == Compression::ZRle));
    let flags = if sig.initial_high { 0x01 } else { 0x00 };
    let rle_stream = rle::encode(&sig.pulses)?;
    let comp_byte = match compression {
        Compression::Rle => 0x01u8,
        Compression::ZRle => 0x02u8,
    };
    let payload = match compression {
        Compression::Rle => rle_stream,
        Compression::ZRle => zlib_deflate(&rle_stream)?,
    };

    let mut out = Vec::new();
    out.extend_from_slice(SIGNATURE);
    out.push(TERMINATOR);
    match version {
        1 => {
            // Major, minor; then the rate, a **word** in v1: the low word is
            // stamped, where the console reports the rate in full.
            out.extend_from_slice(&[0x01, 0x01]);
            out.extend_from_slice(&(sig.rate as u16).to_le_bytes());
            out.push(comp_byte);
            out.push(flags);
            out.extend_from_slice(&[0, 0, 0]); // reserved
        }
        _ => {
            out.extend_from_slice(&[0x02, 0x00]); // major, minor
            out.extend_from_slice(&sig.rate.to_le_bytes());
            out.extend_from_slice(&(sig.declared as u32).to_le_bytes());
            out.push(comp_byte);
            out.push(flags);
            out.push(0x00); // header-extension length
            let mut name = [0u8; 16]; // ASCIIZ[16]
            for (dst, b) in name.iter_mut().zip(app.bytes()).take(15) {
                *dst = b;
            }
            out.extend_from_slice(&name);
        }
    }
    out.extend_from_slice(&payload);
    Ok(out)
}

#[cfg(feature = "zlib-c")]
const ZALLOC_HEADER: usize = 16;

#[cfg(all(test, feature = "zlib-c"))]
static ZALLOC_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(feature = "zlib-c")]
unsafe extern "C" fn zalloc(
    _opaque: *mut std::os::raw::c_void,
    items: std::os::raw::c_uint,
    size: std::os::raw::c_uint,
) -> *mut std::os::raw::c_void {
    #[cfg(test)]
    ZALLOC_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let Some(total) = (items as usize)
        .checked_mul(size as usize)
        .and_then(|n| n.checked_add(ZALLOC_HEADER))
    else {
        return std::ptr::null_mut();
    };
    let Ok(layout) = std::alloc::Layout::from_size_align(total, ZALLOC_HEADER) else {
        return std::ptr::null_mut();
    };
    unsafe {
        let base = std::alloc::alloc(layout);
        if base.is_null() {
            return std::ptr::null_mut();
        }
        base.cast::<usize>().write(total);
        base.add(ZALLOC_HEADER).cast()
    }
}

#[cfg(feature = "zlib-c")]
unsafe extern "C" fn zfree(_opaque: *mut std::os::raw::c_void, address: *mut std::os::raw::c_void) {
    if address.is_null() {
        return;
    }
    unsafe {
        let base = address.cast::<u8>().sub(ZALLOC_HEADER);
        let total = base.cast::<usize>().read();
        let layout = std::alloc::Layout::from_size_align_unchecked(total, ZALLOC_HEADER);
        std::alloc::dealloc(base, layout);
    }
}

/// Compress with zlib: level 9, `Z_DEFLATED`, 15 window bits, **memory level
/// 9** and the default strategy.
///
/// Memory level 9 is not zlib's default: `lit_bufsize` is `1 << (memLevel +
/// 6)`, so 8 and 9 pack identically until 16384 literals have gone by;
/// `memory_level_9_shows_past_16384_literals` pins a 40000-literal stream.
#[cfg(feature = "zlib-c")]
fn zlib_deflate(data: &[u8]) -> Result<Vec<u8>> {
    zlib_deflate_pieces(data, u32::MAX as usize)
}

/// [`zlib_deflate`] with the input handed over `piece` bytes at a time. The
/// stream's bytes do not depend on the piece size, only on the parameters.
#[cfg(feature = "zlib-c")]
fn zlib_deflate_pieces(data: &[u8], piece: usize) -> Result<Vec<u8>> {
    // One output block at a time.
    const CHUNK: usize = 1 << 16;
    let failed = || Error::Fatal("Could not create output file".into());
    let mut out: Vec<u8> = Vec::with_capacity(data.len() / 2 + 64);
    // SAFETY: the stream is zeroed as zlib requires and is only ever touched
    // through the pointer -- no `z_stream` value is ever materialised, which
    // matters because its allocator fields are non-null function pointers
    // that a zeroed one would violate. The input and output pointers are
    // re-derived from live slices immediately before each call and never held
    // across one, and `deflateEnd` runs on every exit path.
    unsafe {
        let mut stream = std::mem::MaybeUninit::<libz_sys::z_stream>::zeroed();
        let strm = stream.as_mut_ptr();
        (*strm).zalloc = zalloc;
        (*strm).zfree = zfree;
        if libz_sys::deflateInit2_(
            strm,
            9,
            libz_sys::Z_DEFLATED,
            15,
            9,
            libz_sys::Z_DEFAULT_STRATEGY,
            libz_sys::zlibVersion(),
            std::mem::size_of::<libz_sys::z_stream>() as std::os::raw::c_int,
        ) != libz_sys::Z_OK
        {
            return Err(failed());
        }
        let mut fed = 0usize;
        loop {
            // `avail_in` is a 32-bit count, so a stream past 4 GiB is handed
            // over in pieces. Only the last piece asks for Z_FINISH.
            let take = (data.len() - fed).min(piece);
            (*strm).next_in = data[fed..].as_ptr() as *mut u8;
            (*strm).avail_in = take as u32;
            let flush = if fed + take == data.len() {
                libz_sys::Z_FINISH
            } else {
                libz_sys::Z_NO_FLUSH
            };
            let done = loop {
                let filled = out.len();
                out.resize(filled + CHUNK, 0);
                (*strm).next_out = out[filled..].as_mut_ptr();
                (*strm).avail_out = CHUNK as u32;
                let rc = libz_sys::deflate(strm, flush);
                let produced = CHUNK - (*strm).avail_out as usize;
                out.truncate(filled + produced);
                if rc == libz_sys::Z_STREAM_END {
                    break true;
                }
                if rc != libz_sys::Z_OK {
                    libz_sys::deflateEnd(strm);
                    return Err(failed());
                }
                // The piece is consumed. Before the last one, stop even when
                // the block was filled to the byte: another call with nothing
                // to hand over is answered Z_BUF_ERROR, not Z_OK.
                if (*strm).avail_in == 0 && (produced < CHUNK || flush != libz_sys::Z_FINISH) {
                    break false;
                }
            };
            fed += take - (*strm).avail_in as usize;
            if done {
                break;
            }
        }
        libz_sys::deflateEnd(strm);
    }
    Ok(out)
}

#[cfg(not(feature = "zlib-c"))]
fn zlib_deflate(_data: &[u8]) -> Result<Vec<u8>> {
    Err(Error::Unsupported(
        "this build cannot write Z-RLE output (built without the 'zlib-c' feature); \
         -z writes CSW v2 with plain RLE, -1 writes CSW v1"
            .into(),
    ))
}

/// Inflate a zlib stream. Any conforming deflate decodes to the same bytes,
/// so Z-RLE data from any CSW encoder is read interchangeably.
#[cfg(test)]
fn zlib_inflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|e| Error::Format(format!("bad Z-RLE stream: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "zlib-c")]
    #[test]
    fn zlib_allocates_through_the_crates_allocator() {
        unsafe {
            let block = zalloc(std::ptr::null_mut(), 300, 8);
            assert!(!block.is_null());
            assert_eq!(block as usize % 16, 0);
            std::ptr::write_bytes(block.cast::<u8>(), 0xa5, 2400);
            let header = block.cast::<u8>().sub(ZALLOC_HEADER).cast::<usize>().read();
            assert_eq!(header, 2416);
            zfree(std::ptr::null_mut(), block);
            zfree(std::ptr::null_mut(), std::ptr::null_mut());
            assert!(zalloc(std::ptr::null_mut(), u32::MAX, u32::MAX).is_null());
        }
        let before = ZALLOC_CALLS.load(std::sync::atomic::Ordering::Relaxed);
        zlib_deflate(&[0u8; 4096]).unwrap();
        assert!(ZALLOC_CALLS.load(std::sync::atomic::Ordering::Relaxed) > before);
    }

    /// Memory level 9 packs a stream of more than 16384 literals differently
    /// from level 8: a fixed pseudo-random stream pins the level.
    #[cfg(feature = "zlib-c")]
    #[test]
    fn memory_level_9_shows_past_16384_literals() {
        let mut x: u32 = 0x1234_5678;
        let data: Vec<u8> = (0..40000)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect();
        let out = zlib_deflate(&data).unwrap();
        let sum = out
            .iter()
            .fold(0u64, |h, &b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
        assert_eq!((out.len(), sum), (40016, 3_926_221_656_866_596_599));
    }

    /// A stream handed to zlib in pieces packs to the same bytes as one
    /// handed over whole -- the path a stream past 4 GiB takes, including a
    /// piece that ends on a full output block.
    #[cfg(feature = "zlib-c")]
    #[test]
    fn deflate_in_pieces_matches_one_piece() {
        // Noise with runs in it: several output blocks, some of them matches.
        let mut x = 0x2545_f491u32;
        let data: Vec<u8> = (0..300_000u32)
            .map(|i| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                if (i / 1000) % 4 == 0 {
                    (i % 5) as u8
                } else {
                    (x >> 24) as u8
                }
            })
            .collect();
        let whole = zlib_deflate(&data).unwrap();
        assert!(
            whole.len() > 1 << 16,
            "the stream must span several output blocks"
        );
        for piece in [97usize, 1 << 16, 100_003, 299_999] {
            assert_eq!(
                zlib_deflate_pieces(&data, piece).unwrap(),
                whole,
                "piece {piece}"
            );
        }
    }

    #[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
    #[test]
    fn the_walk_is_not_proportional_to_the_pulses() {
        let info = CswInfo {
            major: 2,
            minor: 0,
            compression: 2,
            app: Vec::new(),
            declared_pulses: Some(4_000_000),
            rate: 44100,
            initial_high: true,
            data_offset: 0,
        };
        // Four million one-sample pulses from a payload of a few kilobytes:
        // collecting them is 16 MB of `Vec<u32>`, walking them is nothing.
        let payload = zlib_deflate(&vec![1u8; 4_000_000]).unwrap();
        assert!(payload.len() < 64 << 10, "payload: {}", payload.len());

        let src = pulse_source(&info, &payload);
        let measured = crate::signal::survey(&src).unwrap();
        assert_eq!(measured.pulses, 4_000_000);
        assert_eq!(measured.total_samples, 4_000_000);

        // Walkable again, and to the same answer -- which is what lets the
        // header be sized on one pass and written on the next.
        assert_eq!(crate::signal::survey(&src).unwrap(), measured);

        // An unsupported compression type surfaces on the walk, since the
        // header line is printed before anything looks at it.
        let bad = CswInfo {
            compression: 9,
            ..info
        };
        let e = crate::signal::survey(&pulse_source(&bad, &payload)).unwrap_err();
        assert!(format!("{e}").contains("compression type #9"), "{e}");
    }

    /// The two decoders are one codec, so a stream read a byte at a time and
    /// the same stream read whole must agree -- escapes, a zero-length pulse,
    /// and a truncated escape at the end included.
    #[test]
    fn the_streaming_and_collecting_decoders_agree() {
        let cases: [&[u8]; 5] = [
            &[],
            &[1, 2, 255],
            &[0, 0xE9, 0xCD, 0, 0, 7], // an escaped 0xCDE9, then 7
            &[5, 0, 0, 0, 0, 0, 9],    // an escaped zero in the middle
            &[3, 0, 1, 2],             // an escape with too few bytes
        ];
        for raw in cases {
            let whole = rle::decode(raw);
            let walked: Vec<u32> = rle::Decoder::new(raw).collect::<Result<Vec<_>>>().unwrap();
            assert_eq!(whole, walked, "{raw:?}");
        }
    }

    #[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
    #[test]
    fn v2_zrle_header_and_roundtrip() {
        let p = Pulses::new(44100, vec![3, 5, 1, 300, 7], true);
        let bytes = write(&p, 2, Compression::ZRle, "csw").unwrap();
        assert_eq!(&bytes[0..22], SIGNATURE);
        assert_eq!(bytes[0x16], 0x1A);
        assert_eq!(bytes[0x17], 0x02);
        assert_eq!(
            u32::from_le_bytes(bytes[0x19..0x1D].try_into().unwrap()),
            44100
        );
        assert_eq!(u32::from_le_bytes(bytes[0x1D..0x21].try_into().unwrap()), 5);
        assert_eq!(bytes[0x21], 0x02); // compression type = Z-RLE
        assert_eq!(bytes[0x22], 0x01);
        assert_eq!(read(&bytes).unwrap(), p);
    }

    /// The three forms that can be written round-trip, and each stamps the
    /// version and compression byte its combination calls for.
    #[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
    #[test]
    fn every_written_form_roundtrips() {
        let p = Pulses::new(22050, vec![10, 20, 300, 1, 7], true);
        // (version, compression, header-version byte, compression byte)
        let cases = [
            (2, Compression::ZRle, 0x02u8, 0x02u8),
            (2, Compression::Rle, 0x02, 0x01),
            (1, Compression::Rle, 0x01, 0x01),
        ];
        for (ver, comp, vbyte, cbyte) in cases {
            let bytes = write(&p, ver, comp, "csw").unwrap();
            assert_eq!(bytes[0x17], vbyte, "version byte for v{ver}");
            let cpos = if ver == 1 { 0x1B } else { 0x21 };
            assert_eq!(bytes[cpos], cbyte, "compression byte for v{ver} {comp:?}");
            assert_eq!(read(&bytes).unwrap(), p, "roundtrip v{ver} {comp:?}");
        }
    }

    #[cfg(not(feature = "zlib-c"))]
    #[test]
    fn zrle_output_is_refused_without_the_c_zlib() {
        let p = Pulses::new(44100, (1..500u32).collect(), true);
        assert!(matches!(
            write(&p, 2, Compression::ZRle, "csw"),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn truncated_v2_header_is_an_error() {
        // A v2 signature cut short inside the header must error, not panic.
        let p = Pulses::new(44100, vec![1, 2, 3], false);
        let bytes = write(&p, 2, Compression::Rle, "csw").unwrap();
        for len in 0x20..0x34 {
            assert!(read(&bytes[..len]).is_err(), "no error at length {len}");
        }
    }

    /// The header extension length is signed: 255 is -1, so the pulse data
    /// starts one byte inside the header.
    #[test]
    fn the_header_extension_length_is_signed() {
        let mut header = Vec::new();
        header.extend_from_slice(SIGNATURE);
        header.push(TERMINATOR);
        header.extend_from_slice(&[2, 0]);
        header.extend_from_slice(&22050u32.to_le_bytes());
        header.extend_from_slice(&2u32.to_le_bytes());
        header.extend_from_slice(&[1, 1, 0xFF]);
        // The application field's last byte is the first pulse.
        let mut app = [0u8; 16];
        app[..6].copy_from_slice(b"parity");
        app[15] = 5;
        header.extend_from_slice(&app);
        let mut file = header.clone();
        file.extend_from_slice(&[7, 9]);

        let info = read_header(&file).unwrap();
        assert_eq!(info.data_offset, 0x33);

        // ...and a positive length skips forward as it says.
        file[0x23] = 2;
        let info = read_header(&file).unwrap();
        assert_eq!(info.data_offset, 0x36);
    }

    fn v2_with_app(len: usize, comp: u8, ext: u8, app: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(SIGNATURE);
        file.push(TERMINATOR);
        file.extend_from_slice(&[2, 0]);
        file.extend_from_slice(&22050u32.to_le_bytes());
        file.extend_from_slice(&4u32.to_le_bytes());
        file.extend_from_slice(&[comp, 1, ext]);
        file.extend_from_slice(app);
        file.extend((0..len - 0x34).map(|i| [10, 20, 30, 40][i % 4]));
        assert_eq!(file.len(), len);
        file
    }

    #[test]
    fn an_unterminated_application_field_runs_into_the_payload_length() {
        let unterminated = b"AAAAAAAAAAAAAAAA";
        for (len, ext, length) in [
            (56usize, 5u8, &[0xFF, 0xFF, 0xFF, 0xFF][..]),
            (56, 11, &[0xF9, 0xFF, 0xFF, 0xFF]),
            (56, 0, &[0x04]),
            (52, 0, &[]),
            (0x34 + 257, 0, &[0x01, 0x01]),
            (0x34 + 65793, 0, &[0x01, 0x01, 0x01]),
        ] {
            let info = read_header(&v2_with_app(len, 1, ext, unterminated)).unwrap();
            assert_eq!(
                info.app,
                [&unterminated[..], length].concat(),
                "{len} bytes, extension {ext}"
            );
        }
        let terminated = b"CSW v2.00\0\0\0\0\0\0\0";
        let info = read_header(&v2_with_app(56, 1, 5, terminated)).unwrap();
        assert_eq!(info.app, b"CSW v2.00");
    }

    #[test]
    fn an_extension_past_the_end_is_empty_down_to_minus_seven() {
        let app = b"CSW v2.00\0\0\0\0\0\0\0";
        for len in [56usize, 52] {
            let inside = (len - 0x34) as u8;
            let file = v2_with_app(len, 1, inside, app);
            assert_eq!(read_header(&file).unwrap().data_offset, len);
            for comp in [1u8, 2] {
                for past in 1..=7u8 {
                    let file = v2_with_app(len, comp, inside + past, app);
                    let (pulses, info) = read_info(&file).unwrap();
                    let at = format!("{len} bytes, {past} past, compression {comp}");
                    assert_eq!(info.data_offset, len, "{at}");
                    assert!(pulses.pulses.is_empty(), "{at}");
                }
            }
            for ext in [inside + 8, 100, 127, 0x80, 0xCB] {
                let file = v2_with_app(len, 1, ext, app);
                assert!(
                    matches!(read_header(&file), Err(Error::Silent(_))),
                    "{len} bytes, extension {ext}"
                );
            }
        }
        let (pulses, info) = read_info(&v2_with_app(56, 1, 0xCC, app)).unwrap();
        assert_eq!(info.data_offset, 0);
        assert_eq!(pulses.pulses.len(), 40);
    }
}
