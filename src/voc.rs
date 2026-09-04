//! Creative Voice File (VOC) read/write: reads 8-bit mono PCM (block types
//! 1/2/3/8/9), writes a **v1.10** file in block-1 chunks with the rate as a
//! block-1 time constant.

use crate::error::{Error, Result};
use crate::signal::PulseSource;
#[cfg(test)]
use crate::signal::Pulses;
use std::io::{Read, Seek, Write};
use std::rc::Rc;

/// The refusal for a VOC whose block-9 codec word is not PCM, printed bare.
/// A block 1 or 8 with a pack byte raises `VOC_COMPRESSED` below, not this.
const VOC_ONLY_8_BIT: &str = "Sorry, only 8-bit mono PCM VOC files are supported.";
/// The pack byte / codec of a block 1 or 8: its own line, with no full stop.
const VOC_COMPRESSED: &str = "Sorry, compressed blocks are not yet supported";
/// A block **9** declaring more than one channel. Block 8 has its own
/// spelling, without the full stop.
const VOC_STEREO: &str = "Sorry, stereo VOC files are not yet supported.";
/// The block-8 spelling of it.
const VOC_STEREO_EXT: &str = "Sorry, stereo VOC files are not yet supported";
/// A block 9 whose width is not 8. The text says 16 whatever the width is.
const VOC_16_BIT: &str = "Sorry, 16 bit samples are not yet supported.";

use crate::source::{self, Encoding, Layout, Segment};
use crate::wav;
#[cfg(test)]
use crate::wav::MonoWav;

/// The signature the input's reader is chosen by, in `main`, and the one
/// this reader turns a file down for not having.
pub const MAGIC: &[u8; 20] = b"Creative Voice File\x1A";
const VERSION: u16 = 0x010A; // 1.10: block-1 files, the widely-played form
const MIDPOINT: u8 = 0x80; // 8-bit unsigned silence / threshold level

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailRead {
    Terminator,
    Sample(u8, bool),
    /// A read past the end of the 256 KiB buffer, of memory that is not the
    /// file's. The real run faults on it; a probe pass answers and goes on.
    Heap,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Block {
    pub claimed: u32,
    pub header: u64,
    pub length: u32,
    pub sound: bool,
    pub counted: bool,
    pub rate: Option<u32>,
    pub extended: bool,
}

pub(crate) fn parse_block(
    btype: u8,
    read: &mut dyn FnMut(usize) -> Result<Vec<u8>>,
) -> Result<Block> {
    debug_assert!(btype != 0);
    match btype {
        // Repeat blocks (6, 7) are refused by name, the type printed as a raw
        // byte; a type the walk does not know is a corrupt file; both before
        // any length is read.
        6 | 7 => {
            return Err(Error::Refused(format!(
                "Sorry, block type {} of VOC files is not yet supported.",
                btype as char
            )));
        }
        10.. => {
            return Err(Error::Fatal(
                "Input file is corrupted or in a wrong format".into(),
            ));
        }
        _ => {}
    }
    let len = read(3)?;
    let claimed = u32::from_le_bytes([len[0], len[1], len[2], 0]);
    let mut block = Block {
        claimed,
        header: 0,
        length: claimed,
        sound: true,
        counted: false,
        rate: None,
        extended: false,
    };
    match btype {
        1 => {
            // [TC][pack][samples...]
            let h = read(2)?;
            if h[1] != 0 {
                return Err(Error::Refused(VOC_COMPRESSED.into()));
            }
            block.header = 2;
            block.length = claimed.wrapping_sub(2);
            block.counted = true;
            block.rate = Some(tc_to_rate(h[0]));
        }
        // continuation: raw samples
        2 => block.counted = true,
        3 => {
            // Silence: [WORD length-1][TC]. Read and dropped: no samples are
            // emitted, and its time constant does not set the rate, so a
            // file whose only block is a silence has none.
            read(3)?;
            block.header = 3;
            block.sound = false;
        }
        // A marker (4) or text (5) block's payload is converted as sound
        // wherever it sits (seven bytes of annotation become a seven-sample
        // pulse); the declared sample count covers sound blocks only.
        4 | 5 => {}
        8 => {
            // extended: [WORD TC][BYTE pack][BYTE mode] before a block 1.
            // This pack byte is the codec that counts, superseding the block
            // 1's; a nonzero one is refused as a malformed block, before the
            // mode byte is looked at.
            let h = read(4)?;
            if h[2] != 0 {
                return Err(Error::Fatal("Extended block contains nonsense data".into()));
            }
            if h[3] != 0 {
                return Err(Error::Refused(VOC_STEREO_EXT.into()));
            }
            // A block 8 carries a *16-bit* time constant: the rate is
            // `256000000 / (65536 - tc)`. The rate this carries wins over any
            // found already, and a second block 8 wins over the first -- with
            // or without a block 1 behind it to spend it on.
            let tc16 = u16::from_le_bytes([h[0], h[1]]) as u32;
            block.header = 4;
            block.sound = false;
            block.rate = Some(256_000_000 / (65536 - tc16));
            block.extended = true;
        }
        9 => {
            // new format: [DWORD rate][BYTE bits][BYTE ch][WORD fmt][4 rsvd][data]
            let h = read(12)?;
            let bits = h[4];
            let channels = h[5];
            // The format word: 0 is unsigned 8-bit PCM; 4-bit ADPCM (1),
            // a-law (6) and u-law (7) share the width and are refused.
            let fmt = u16::from_le_bytes([h[6], h[7]]);
            // The width is checked *before* the channel count, so a
            // block that is both stereo and 16-bit is reported as
            // 16-bit. (The WAV reader checks them the other way round.)
            if bits != 8 {
                return Err(Error::Refused(VOC_16_BIT.into()));
            }
            // "More than one", not "not one": a block 9 declaring
            // **zero** channels is converted.
            if channels > 1 {
                return Err(Error::Refused(VOC_STEREO.into()));
            }
            // Codec 1 (ADPCM) and 6 (a-law) declare 8 bits and one
            // channel, so the width alone does not catch them.
            if fmt != 0 {
                return Err(Error::Refused(VOC_ONLY_8_BIT.into()));
            }
            block.header = 12;
            block.length = claimed.wrapping_sub(12);
            block.counted = true;
            block.rate = Some(u32::from_le_bytes([h[0], h[1], h[2], h[3]]));
            block.extended = true;
        }
        _ => unreachable!(),
    }
    Ok(block)
}

const MARK: u64 = 26;
const BUFFER: u64 = 0x40000;

#[derive(Debug, Clone)]
pub struct Tail {
    pos: u64,
    file_len: u64,
    btype: u8,
    length: u64,
    consumed: u64,
    phantom: Option<Rc<Vec<u8>>>,
    window: Vec<u8>,
    window_at: u64,
}

impl Tail {
    fn byte<R: Read + Seek>(&mut self, reader: &mut R) -> Result<u8> {
        let b = if self.pos < self.file_len {
            let in_window =
                self.pos >= self.window_at && self.pos < self.window_at + self.window.len() as u64;
            if !in_window {
                let want = (self.file_len - self.pos).min(source::CHUNK as u64) as usize;
                self.window = source::read_at(reader, self.pos, want)?;
                self.window_at = self.pos;
            }
            self.window[(self.pos - self.window_at) as usize]
        } else {
            if self.phantom.is_none() {
                self.phantom = Some(Rc::new(phantom(reader, self.file_len)?));
            }
            let i = (self.pos - self.file_len) as usize;
            self.phantom
                .as_deref()
                .and_then(|p| p.get(i))
                .copied()
                .unwrap_or(0)
        };
        self.pos += 1;
        Ok(b)
    }

    pub fn reset(&mut self, fresh: &Tail) {
        let phantom = self.phantom.take();
        *self = fresh.clone();
        self.phantom = phantom;
    }

    pub fn next<R: Read + Seek>(&mut self, reader: &mut R) -> Result<TailRead> {
        // The buffer is 256 KiB; past it a read is of memory that is not the
        // file's: the walk reports `Heap` and reads nothing, and the pass
        // decides (`read_on`). A header cut by the buffer's end has its
        // fields read by `byte`, which answers 0 there, and is refused.
        if self.pos >= self.file_len + BUFFER {
            return Ok(TailRead::Heap);
        }
        if self.consumed == self.length {
            self.consumed = 0;
            loop {
                let t = self.byte(reader)?;
                self.btype = t;
                if t == 0 {
                    return Ok(TailRead::Terminator);
                }
                let block = parse_block(t, &mut |k| (0..k).map(|_| self.byte(reader)).collect())?;
                self.length = u64::from(block.length);
                if block.sound {
                    break;
                }
            }
        }
        self.consumed += 1;
        let b = self.byte(reader)?;
        Ok(TailRead::Sample(b, self.btype != 0))
    }
}

fn phantom<R: Read + Seek>(reader: &mut R, file_len: u64) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; BUFFER as usize];
    refill(reader, &mut buf, 0, file_len)?;
    refill(reader, &mut buf, MARK, file_len)?;
    Ok(buf)
}

fn refill<R: Read + Seek>(reader: &mut R, buf: &mut [u8], from: u64, len: u64) -> Result<()> {
    let reads = (len - from).div_ceil(BUFFER);
    for k in reads.saturating_sub(2)..reads {
        let at = from + k * BUFFER;
        let want = (len - at).min(BUFFER) as usize;
        buf[..want].copy_from_slice(&source::read_at(reader, at, want)?);
    }
    Ok(())
}

/// Scan an 8-bit mono PCM VOC file.
pub fn scan<R: Read + Seek>(reader: &mut R) -> Result<Layout> {
    let file_len = source::len_of(reader)?;
    // A file without the 20-byte magic (an empty one included) is "Wrong
    // file type"; one that stops before the six header bytes after it, or
    // holds no block byte after them, is "corrupted". The version checksum
    // is never checked.
    if file_len < 20 {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let magic = source::read_at(reader, 0, 20)?;
    if magic[..] != *MAGIC {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    if file_len < 27 {
        return Err(Error::Fatal(
            "Input file is corrupted or in a wrong format".into(),
        ));
    }
    let head = source::read_at(reader, 0, 26)?;
    // The data-block offset is checked, not followed: anything but 26 is
    // refused, with the same line an OUT trace of a bad size gets.
    let data_off = u16::from_le_bytes([head[20], head[21]]) as u64;
    if data_off != 26 {
        return Err(Error::Fatal(
            "Input file is corrupted or in a wrong format".into(),
        ));
    }
    let version = u16::from_le_bytes([head[22], head[23]]);

    let mut pos = data_off;
    let mut rate: Option<u32> = None;
    let mut segments: Vec<Segment> = Vec::new();
    let mut empty_block = false;
    // The sample count is a 32-bit sum of the lengths of the sound blocks
    // (1, 2 and 9) as their headers leave them, less the bytes by which any
    // block runs past the end of the file: a marker or text block that does
    // takes it below zero, and it prints unsigned.
    let mut count = 0u32;
    let mut extended = false; // "EXT" if a block 8/9 is present, else "STD"

    // The description line reports the bytes *consumed*, not the file size:
    // a terminator between two sound blocks stops the walk and the count.
    let mut consumed = file_len;
    let mut terminated = false;
    let mut overran = false;
    let mut node_len = 0u64;
    let mut last_btype = 0u8;
    let mut last_consumed = 0u64;
    let mut tail: Option<Tail> = None;
    while pos < file_len {
        let btype = source::read_at(reader, pos, 1)?[0];
        if btype == 0 {
            consumed = pos + 1; // the terminator itself is consumed
            terminated = true;
            tail = Some(Tail {
                pos,
                file_len,
                btype,
                length: node_len,
                consumed: node_len,
                phantom: None,
                window: Vec::new(),
                window_at: 0,
            });
            break;
        }
        if btype == 3 && file_len - pos < 7 {
            overran = true;
            count = count.wrapping_sub((pos + 7 - file_len) as u32);
            break;
        }
        // Only each block's header is read, whatever the block claims for a
        // length: a block 1 of one byte takes its pack byte from the block
        // behind it, and past the end of the file the bytes come from the
        // buffer, which holds the file from its start (byte f past the end
        // is the file's byte f - len).
        let block = {
            let mut at = pos + 1;
            parse_block(btype, &mut |k: usize| -> Result<Vec<u8>> {
                let have = file_len.saturating_sub(at).min(k as u64) as usize;
                let mut v = source::read_at(reader, at, have)?;
                if have < k {
                    v.extend(source::read_at(
                        reader,
                        at + have as u64 - file_len,
                        k - have,
                    )?);
                }
                at += k as u64;
                Ok(v)
            })?
        };
        let body = pos + 4;
        last_btype = btype;
        node_len = u64::from(block.length);
        extended |= block.extended;
        if let Some(r) = block.rate {
            if block.extended || !extended {
                rate = Some(r);
            }
        }
        if block.sound {
            // A block claiming more than the file holds is clamped to what is
            // there; nothing can follow it, so the walk stops after it.
            let claimed = u64::from(block.claimed);
            let len = claimed.min(file_len.saturating_sub(body));
            let present = len.saturating_sub(block.header);
            if block.counted {
                count = count.wrapping_add(block.length);
                empty_block |= (block.length as i32) <= 0;
            }
            segments.push(Segment {
                offset: body + block.header,
                bytes: present,
            });
            if body + claimed > file_len {
                overran = true;
                count = count.wrapping_sub((body + claimed - file_len) as u32);
            }
            last_consumed = present;
            pos = body + len;
        } else {
            last_consumed = node_len;
            pos = body + block.header;
        }
    }

    // A file whose blocks run into the end of the file with **no terminator**
    // is read twice: the reader takes its next block from its buffer, which
    // holds the block area again, and the console carries "***WARNING*** -
    // Unexpected end of file!" under the sampling-rate line. The declared
    // sample count is the single pass's.
    let unterminated = !terminated;
    if unterminated {
        tail = Some(Tail {
            pos,
            file_len,
            btype: last_btype,
            length: node_len,
            consumed: last_consumed,
            phantom: None,
            window: Vec::new(),
            window_at: 0,
        });
    }

    // A file whose blocks carried no rate is converted all the same: the
    // rate line reports the indefinite integer -- which makes the playing
    // time 00:00.000 -- and a rate of zero is written into the file.
    let shown_rate = rate.map_or(i64::from(i32::MIN), |r| i64::from(r as i32));
    let design_rate = rate.map_or(f64::INFINITY, f64::from);
    let rate = rate.unwrap_or(0);
    let desc = format!(
        // `%i.%i`: 0x010A prints as v1.10 and 0x0200 as v2.0.
        "Creative Voice File (VOC) v{}.{} {}, {} bytes ({} samples)",
        version >> 8,
        version & 0xff,
        if extended { "EXT" } else { "STD" },
        consumed,
        count
    );
    Ok(Layout {
        rate,
        shown_rate,
        design_rate,
        midpoint: MIDPOINT as f64,
        desc,
        integrity_check: true,
        encoding: Encoding::U8,
        channels: 1,
        segments,
        unterminated: unterminated && !overran,
        empty_block,
        tail,
        total_samples: u64::from(count),
    })
}

/// Whole-file read straight to pulses, for the tests.
#[cfg(test)]
pub(crate) fn read(raw: &[u8]) -> Result<Pulses> {
    let m = read_mono(raw)?;
    Ok(crate::detect::samples_to_pulses(
        m.rate, &m.samples, m.midpoint,
    ))
}

/// Read a whole VOC file into memory, for the tests.
#[cfg(test)]
pub(crate) fn read_mono(raw: &[u8]) -> Result<MonoWav> {
    let mut cursor = std::io::Cursor::new(raw);
    let layout = scan(&mut cursor)?;
    let (rate, midpoint) = (layout.rate, layout.midpoint);
    let samples = crate::source::SampleSource::new(layout, cursor).collect()?;
    Ok(MonoWav {
        rate,
        samples,
        midpoint,
    })
}

/// The `-dv` writer's buffer, in samples: 256 KiB.
///
/// A block is flushed when the *next* pulse would not fit (`n + L >= 0x40000`,
/// an empty block included). A pulse longer than the buffer is written in
/// chunks of the buffer's size, each a block, and the fill count is then left
/// at the last chunk's size: the pulses that follow are appended behind that
/// many stale bytes of the long pulse's level.
const BUFFER_SAMPLES: u64 = 0x40000;

/// The block-1 time constant for `rate`: `256 - 1e6/rate`, the half added
/// *before* truncation toward zero, taken as a byte with no range check.
///
/// Adding the half first is not rounding the division: at 16000 Hz and
/// 80000 Hz the two differ by one. Below 3906 Hz the constant wraps into the
/// byte, so 1000 Hz gives 25 and 1 Hz gives 193. A reader recovers
/// `1_000_000/(256-tc)`, so 44100 Hz comes back as 43478.
///
/// The rate divides in as a **signed** int, so a CSW declaring a rate of
/// 0xFFFFFFFF is -1 here and the constant comes out 64, not 0.
fn time_constant(rate: u32) -> u8 {
    (256.0 - 1_000_000.0 / f64::from(rate as i32) + 0.5).trunc() as i64 as u8
}

/// Renders pulses into type-1 VOC blocks through a buffer of
/// [`BUFFER_SAMPLES`], flushed and chunked as that buffer dictates.
///
/// Each block is `01`, a 24-bit length (time-constant + pack byte + data), the
/// time constant, a pack byte of 0 (unpacked 8-bit PCM), then the data.
struct BlockWriter<'a, W: Write> {
    out: &'a mut W,
    tc: u8,
    /// The buffer, and how much of it is filled. What lies beyond `filled`
    /// is stale, and after a long pulse so is what lies before it.
    buf: Vec<u8>,
    filled: usize,
    /// File bytes written so far.
    written: u64,
}

impl<'a, W: Write> BlockWriter<'a, W> {
    fn new(out: &'a mut W, rate: u32) -> Self {
        BlockWriter {
            out,
            tc: time_constant(rate),
            buf: vec![0; BUFFER_SAMPLES as usize],
            filled: 0,
            written: 0,
        }
    }

    fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    /// One block: `01`, length (data + the two header bytes), the time
    /// constant, the pack byte, then `data` bytes from the front of the
    /// buffer -- zero of them included.
    fn block(&mut self, data: usize) -> Result<()> {
        self.raw(&[0x01])?;
        let len = data as u32 + 2; // + time constant + pack byte
        self.raw(&len.to_le_bytes()[0..3])?; // 24-bit LE length
        self.raw(&[self.tc, 0])?; // time constant, pack = 8-bit unpacked PCM
        self.out.write_all(&self.buf[..data])?;
        self.written += data as u64;
        Ok(())
    }

    /// One pulse of `len` samples at `level`, zero-length included.
    fn pulse(&mut self, len: u64, level: u8) -> Result<()> {
        let cap = BUFFER_SAMPLES;
        // Would not fit: flush what is buffered, empty or not.
        if self.filled as u64 + len >= cap {
            let n = self.filled;
            self.block(n)?;
            self.filled = 0;
        }
        if len > cap {
            // Longer than the buffer: chunks of the buffer's size, each its
            // own block, rendered from the front of the buffer -- and the
            // fill count is left at the last chunk's size.
            let mut left = len;
            let mut chunk = 0usize;
            while left > 0 {
                chunk = left.min(cap) as usize;
                self.buf[..chunk].fill(level);
                self.block(chunk)?;
                left -= chunk as u64;
            }
            self.filled = chunk;
        } else {
            let n = len as usize;
            self.buf[self.filled..self.filled + n].fill(level);
            self.filled += n;
        }
        Ok(())
    }

    /// The last block, if anything is buffered, and the terminator.
    fn finish(mut self) -> Result<u64> {
        if self.filled > 0 {
            let n = self.filled;
            self.block(n)?;
        }
        self.raw(&[0])?; // terminator
        Ok(self.written)
    }
}

/// Write pulses to `out` as a VOC v1.10 file (8-bit mono PCM in type-1
/// blocks), returning the file size. The waveform is rendered a pulse at a
/// time into the block buffer, never assembled.
pub fn write_to<W: Write, S: PulseSource>(out: &mut W, sig: &S) -> Result<u64> {
    // 26-byte file header
    let check = (!VERSION).wrapping_add(0x1234); // version checksum
    let mut head = Vec::with_capacity(26);
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&26u16.to_le_bytes()); // data block offset
    head.extend_from_slice(&VERSION.to_le_bytes());
    head.extend_from_slice(&check.to_le_bytes());
    out.write_all(&head)?;

    // Every pulse is rendered, a zero-length one included -- it flips the
    // level with nothing between -- so the blocks cover every byte written,
    // where a WAV header's count stops at the zero (see `signal::Survey`).
    let mut blocks = BlockWriter::new(out, sig.rate());
    let mut high = sig.initial_high();
    for length in sig.pulses()? {
        let level = if high { wav::HIGH_8 } else { wav::LOW_8 };
        blocks.pulse(u64::from(length?), level)?;
        high = !high;
    }
    Ok(head.len() as u64 + blocks.finish()?)
}

/// Serialise pulses to a VOC v1.10 file image.
#[cfg(test)]
pub(crate) fn write(sig: &Pulses) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_to(&mut out, sig)?;
    Ok(out)
}

/// VOC block-1 time constant -> sample rate (Hz).
fn tc_to_rate(tc: u8) -> u32 {
    1_000_000 / (256 - tc as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voc_roundtrip_block1() {
        let sig = Pulses::new(44100, vec![3, 5, 1, 4, 7, 2], true);
        let bytes = write(&sig).unwrap();
        assert_eq!(&bytes[0..20], MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[0x16], bytes[0x17]]), 0x010A); // v1.10
        assert_eq!(bytes[26], 1); // first block is type 1
        assert_eq!(*bytes.last().unwrap(), 0); // terminator

        let got = read(&bytes).unwrap();
        assert_eq!(got.pulses, sig.pulses);
        assert_eq!(got.initial_high, sig.initial_high);
    }

    /// The ordinary case: a rate a block-1 time constant can carry.
    #[test]
    fn block1_time_constant() {
        assert_eq!(time_constant(32258), 0xE1); // 225
        assert_eq!(time_constant(44100), 233);
    }

    #[test]
    fn a_rate_of_44100_comes_back_as_43478() {
        assert_eq!(tc_to_rate(time_constant(44100)), 43478);
    }

    #[test]
    fn adding_the_half_first_differs_from_rounding_the_division() {
        for rate in [16000u32, 80000] {
            let rounded_division = 256 - (1_000_000.0 / f64::from(rate) + 0.5).trunc() as u32;
            assert_eq!(u32::from(time_constant(rate)), rounded_division + 1);
        }
    }

    #[test]
    fn reads_classic_block1() {
        // hand-build a minimal block-1 VOC at ~11111 Hz (TC 256-90=166)
        let tc = 166u8;
        let pcm = [0xFFu8, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00];
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&26u16.to_le_bytes());
        v.extend_from_slice(&0x010Au16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.push(1);
        let len = (2 + pcm.len()) as u32;
        v.extend_from_slice(&len.to_le_bytes()[0..3]);
        v.push(tc);
        v.push(0); // pack = 8-bit PCM
        v.extend_from_slice(&pcm);
        v.push(0);
        let p = read(&v).unwrap();
        assert_eq!(p.rate, tc_to_rate(tc));
        assert_eq!(p.pulses, vec![2, 3, 1, 2]); // FF FF | 00 00 00 | FF | 00 00
        assert!(p.initial_high);
    }

    #[test]
    fn a_wrong_version_checksum_is_accepted() {
        let pcm = [0xFFu8, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00];
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&26u16.to_le_bytes());
        v.extend_from_slice(&0x010Au16.to_le_bytes());
        v.extend_from_slice(&0x1234u16.to_le_bytes());
        v.push(1);
        let len = (2 + pcm.len()) as u32;
        v.extend_from_slice(&len.to_le_bytes()[0..3]);
        v.push(166);
        v.push(0);
        v.extend_from_slice(&pcm);
        v.push(0);

        let layout = scan(&mut std::io::Cursor::new(v.clone())).unwrap();
        assert!(
            layout
                .desc
                .starts_with("Creative Voice File (VOC) v1.10 STD")
        );
        assert_eq!(read(&v).unwrap().pulses, vec![2, 3, 1, 2]);
    }

    #[test]
    fn rejects_stereo_block9() {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&26u16.to_le_bytes());
        v.extend_from_slice(&0x0114u16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.push(9);
        let mut block = Vec::new();
        block.extend_from_slice(&44100u32.to_le_bytes());
        block.push(8);
        block.push(2); // stereo
        block.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        block.push(0x80);
        v.extend_from_slice(&(block.len() as u32).to_le_bytes()[0..3]);
        v.extend_from_slice(&block);
        v.push(0);
        assert!(read(&v).is_err());
    }

    /// A file whose blocks run into the end of the file with no terminator is
    /// read twice, and the scan says so.
    #[test]
    fn an_unterminated_file_is_read_twice() {
        let samples: Vec<u8> = (0..40u16)
            .map(|i| if (i / 4) % 2 == 0 { 0xFF } else { 0x00 })
            .collect();
        let mut file = MAGIC.to_vec();
        file.extend_from_slice(&26u16.to_le_bytes());
        file.extend_from_slice(&0x0114u16.to_le_bytes());
        file.extend_from_slice(&(!0x0114u16).to_le_bytes());
        file.push(9);
        file.extend_from_slice(&((samples.len() + 12) as u32).to_le_bytes()[..3]);
        file.extend_from_slice(&22050u32.to_le_bytes());
        file.extend_from_slice(&[8, 1]);
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&[0, 0, 0, 0]);
        file.extend_from_slice(&samples);

        let mut c = std::io::Cursor::new(file.clone());
        let layout = scan(&mut c).unwrap();
        assert!(layout.unterminated);
        assert_eq!(layout.total_samples, samples.len() as u64);
        let mut tail = layout
            .tail
            .clone()
            .expect("an unterminated file has a tail");
        let read_back = crate::source::SampleSource::new(layout, c)
            .collect()
            .unwrap();
        assert_eq!(read_back.len(), samples.len());
        let mut c = std::io::Cursor::new(file.clone());
        let mut again = Vec::new();
        while again.len() < samples.len() {
            match tail.next(&mut c).unwrap() {
                TailRead::Sample(b, true) => again.push(b),
                other => panic!("unexpected read past the end: {other:?}"),
            }
        }
        assert_eq!(again, samples);

        // With a terminator, once.
        file.push(0);
        let mut c = std::io::Cursor::new(file);
        let layout = scan(&mut c).unwrap();
        assert!(!layout.unterminated);
        let read_back = crate::source::SampleSource::new(layout, c)
            .collect()
            .unwrap();
        assert_eq!(read_back.len(), samples.len());
    }

    fn voc_of(blocks: &[&[u8]]) -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&26u16.to_le_bytes());
        v.extend_from_slice(&0x010Au16.to_le_bytes());
        v.extend_from_slice(&0x1129u16.to_le_bytes());
        for b in blocks {
            v.extend_from_slice(b);
        }
        v
    }

    fn framed(btype: u8, payload: &[u8], claimed: Option<u32>) -> Vec<u8> {
        let mut v = vec![btype];
        v.extend_from_slice(&claimed.unwrap_or(payload.len() as u32).to_le_bytes()[..3]);
        v.extend_from_slice(payload);
        v
    }

    fn block1(tc: u8, n: usize) -> Vec<u8> {
        let mut p = vec![tc, 0];
        p.extend((0..n).map(|i| if (i / 20) % 2 == 0 { 0xE0 } else { 0x20 }));
        framed(1, &p, None)
    }

    #[test]
    fn a_later_block_1_sets_the_rate_while_the_ext_flag_is_clear() {
        let file = voc_of(&[&block1(211, 40), &block1(131, 40), &[0]]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert_eq!(layout.rate, tc_to_rate(131));
        assert_eq!(layout.total_samples, 80);

        let mut nine = 22050u32.to_le_bytes().to_vec();
        nine.extend_from_slice(&[8, 1, 0, 0, 0, 0, 0, 0, 0x80, 0x80]);
        let file = voc_of(&[&framed(9, &nine, None), &block1(131, 40), &[0]]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert_eq!(layout.rate, 22050);
    }

    #[test]
    fn an_overrunning_text_block_takes_the_count_below_zero() {
        let text = b"\xe0\xe0\xe0\xe0\xe0\xe0\x20\x20\x20\x20\x20\x20\0";
        let file = voc_of(&[&block1(0xa6, 40), &framed(5, text, Some(60))]);
        let mut c = std::io::Cursor::new(file);
        let layout = scan(&mut c).unwrap();
        assert_eq!(layout.total_samples, 0xFFFF_FFF9);
        assert!(!layout.unterminated);
        assert_eq!(layout.segments.len(), 2);
        assert_eq!(layout.segments[1].bytes, 13);
        let mut tail = layout.tail.clone().unwrap();
        for _ in 0..47 {
            assert!(matches!(
                tail.next(&mut c).unwrap(),
                TailRead::Sample(_, true)
            ));
        }
        match tail.next(&mut c) {
            Err(Error::Fatal(m)) => assert_eq!(m, "Input file is corrupted or in a wrong format"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_walk_reports_the_heap_at_the_end_of_the_buffer() {
        let mut p = vec![0xa6, 0];
        p.extend((0..100).map(|i| if (i / 20) % 2 == 0 { 0xE0 } else { 0x20 }));
        let file = voc_of(&[&framed(1, &p, Some(3_000_000))]);
        let mut c = std::io::Cursor::new(file);
        let layout = scan(&mut c).unwrap();
        let mut tail = layout.tail.clone().unwrap();
        let mut reads = 0u64;
        loop {
            match tail.next(&mut c).unwrap() {
                TailRead::Sample(_, true) => reads += 1,
                TailRead::Heap => break,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(reads, BUFFER);
        assert!(matches!(tail.next(&mut c).unwrap(), TailRead::Heap));
    }

    #[test]
    fn a_header_cut_by_the_end_of_a_file_larger_than_one_fill_reads_the_file_s_start() {
        let stereo9 = {
            let mut h = vec![0u8, 0];
            h.extend_from_slice(&44100u32.to_le_bytes());
            h.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0]);
            h
        };
        for (cut, fill, at_fill, line) in [
            (
                &[1u8, 0x40][..],
                0u8,
                &[0u8, 0, 0xa6, 0][..],
                VOC_COMPRESSED,
            ),
            (&[9, 0x40], 0x80, &stereo9, VOC_16_BIT),
        ] {
            let mut p = vec![0xa6, 0];
            p.resize(2 + 300_000, fill);
            let mut file = voc_of(&[&framed(1, &p, None)]);
            let b = BUFFER as usize;
            file[b..b + at_fill.len()].copy_from_slice(at_fill);
            file.extend_from_slice(cut);
            assert!(file.len() as u64 > BUFFER);
            match scan(&mut std::io::Cursor::new(file)) {
                Err(Error::Refused(m)) => assert_eq!(m, line),
                Err(e) => panic!("{e:?}"),
                Ok(_) => panic!("scanned, expected {line}"),
            }
        }
    }

    #[test]
    fn a_header_straddling_the_end_of_the_buffer_reads_zeros_past_it() {
        let sq = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|i| if (i / 20) % 2 == 0 { 0xE0 } else { 0x20 })
                .collect()
        };
        let mut last = sq(92);
        last[..4].copy_from_slice(&[2, 88, 0, 0]);
        let mut nine = 22050u32.to_le_bytes().to_vec();
        nine.extend_from_slice(&[8, 1, 0, 0, 0, 0, 0, 0]);
        nine.extend_from_slice(&last);
        let file = voc_of(&[
            &framed(2, &sq(96), None),
            &framed(2, &sq(262032), None),
            &framed(9, &nine, None),
        ]);
        let len = file.len() as u64;
        assert_eq!(len, 262270);
        assert_eq!(file[(MARK + BUFFER - 8) as usize], 9);
        let mut c = std::io::Cursor::new(file);
        let layout = scan(&mut c).unwrap();
        assert!(layout.unterminated);
        let mut tail = layout.tail.clone().unwrap();
        let err = loop {
            match tail.next(&mut c) {
                Ok(TailRead::Sample(..)) => continue,
                Ok(other) => panic!("{other:?}"),
                Err(e) => break e,
            }
        };
        assert!(matches!(err, Error::Refused(m) if m == VOC_16_BIT));
        assert_eq!(tail.pos, len + BUFFER + 8);
    }

    #[test]
    fn a_block_8_is_checked_pack_byte_first() {
        let ext = framed(8, &[0xd2, 0xff, 1, 1], None);
        let file = voc_of(&[&ext, &block1(211, 40), &[0]]);
        match scan(&mut std::io::Cursor::new(file)) {
            Err(Error::Fatal(m)) => assert_eq!(m, "Extended block contains nonsense data"),
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("converted"),
        }
        let (bytes, mut at) = (&ext[1..], 0usize);
        let mut read = |k: usize| {
            at += k;
            Ok(bytes[at - k..at].to_vec())
        };
        assert!(matches!(parse_block(8, &mut read), Err(Error::Fatal(_))));
    }

    #[test]
    fn a_type_refused_by_name_needs_no_length_field() {
        let cut = |t: u8| {
            scan(&mut std::io::Cursor::new(voc_of(&[
                &block1(0xa6, 40),
                &[t],
            ])))
        };
        assert!(matches!(cut(6), Err(Error::Refused(m)) if m.starts_with("Sorry, block type")));
        for t in [10, 11, 99, 255] {
            assert!(
                matches!(cut(t), Err(Error::Fatal(m)) if m.starts_with("Input file is corrupted"))
            );
        }
        assert!(matches!(cut(1), Err(Error::Refused(_))));
    }

    #[test]
    fn a_block_1_claiming_one_byte_counts_minus_one() {
        let file = voc_of(&[&block1(0xa6, 40), &[1, 1, 0, 0, 0xa6], &[0]]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert_eq!(layout.total_samples, 39);
        assert!(!layout.unterminated);
        assert!(layout.empty_block);
    }

    #[test]
    fn a_block_clamped_to_nothing_is_not_an_empty_one() {
        let file = voc_of(&[&framed(1, &[0xa6, 0], Some(64))]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert!(!layout.empty_block);
        assert_eq!(layout.total_samples, 0);
        assert_eq!(layout.segments[0].bytes, 0);
        assert!(!layout.unterminated);
        let mut nine = 22050u32.to_le_bytes().to_vec();
        nine.extend_from_slice(&[8, 1, 0, 0, 0, 0, 0, 0]);
        let file = voc_of(&[&framed(9, &nine, Some(64))]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert!(!layout.empty_block);
        assert_eq!(layout.total_samples, 0);
        assert_eq!(layout.rate, 22050);
        assert!(layout.desc.contains(" EXT, "));
    }

    #[test]
    fn a_node_length_of_zero_or_below_abandons_the_run() {
        let nine = |claim: u32| {
            let mut h = 22050u32.to_le_bytes().to_vec();
            h.extend_from_slice(&[8, 1, 0, 0, 0, 0, 0, 0]);
            framed(9, &h, Some(claim))
        };
        for (blocks, count, unterminated) in [
            (vec![block1(0xa6, 40), nine(12)], 40u32, true),
            (vec![nine(12)], 0, true),
            (vec![block1(0xa6, 40), vec![1, 0, 0, 0, 0, 0]], 38, false),
            (vec![block1(0xa6, 40), nine(11)], 39, false),
            (vec![block1(0xa6, 40), nine(8)], 36, false),
            (vec![vec![1, 1, 0, 0, 0xa6], vec![0]], 0xFFFF_FFFF, false),
        ] {
            let parts: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
            let layout = scan(&mut std::io::Cursor::new(voc_of(&parts))).unwrap();
            assert!(layout.empty_block);
            assert_eq!(layout.total_samples, u64::from(count));
            assert_eq!(layout.unterminated, unterminated);
        }
    }

    #[test]
    fn a_header_with_no_block_byte_is_corrupted() {
        let hdr = voc_of(&[]);
        assert_eq!(hdr.len(), 26);
        for n in [25, 26] {
            assert!(matches!(
                scan(&mut std::io::Cursor::new(&hdr[..n])),
                Err(Error::Fatal(m)) if m.starts_with("Input file is corrupted")
            ));
        }
        let layout = scan(&mut std::io::Cursor::new(voc_of(&[&[0]]))).unwrap();
        assert_eq!(layout.total_samples, 0);
        assert_eq!(layout.rate, 0);
        assert!(!layout.unterminated);
    }

    #[test]
    fn a_header_byte_past_the_end_is_the_files_own_from_its_start() {
        let file = voc_of(&[&block1(0xa6, 40), &[4]]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert_eq!(layout.total_samples, 4_288_318_946);
        assert!(!layout.unterminated);
        let cut = |tail: &[u8]| {
            scan(&mut std::io::Cursor::new(voc_of(&[
                &block1(0xa6, 40),
                tail,
            ])))
        };
        assert!(matches!(cut(&[1, 0x40, 0, 0]), Err(Error::Refused(m)) if m == VOC_COMPRESSED));
        assert!(
            matches!(cut(&[1, 0x40, 0, 0, 0xa6]), Err(Error::Refused(m)) if m == VOC_COMPRESSED)
        );
        assert!(matches!(cut(&[9, 0x40, 0, 0]), Err(Error::Refused(m)) if m == VOC_16_BIT));
    }

    #[test]
    fn a_silence_block_cut_by_the_end_of_the_file_is_clamped() {
        for (tail, count, terminator) in [
            (&[3][..], 34u32, false),
            (&[3, 0x40, 0], 36, false),
            (&[3, 0x40, 0, 0], 37, true),
            (&[3, 0x40, 0, 0, 0x10], 38, true),
            (&[3, 0x40, 0, 0, 0x10, 0], 39, false),
        ] {
            let file = voc_of(&[&block1(0xa6, 40), tail]);
            let mut c = std::io::Cursor::new(file);
            let layout = scan(&mut c).unwrap();
            assert_eq!(layout.total_samples, u64::from(count));
            assert!(!layout.unterminated);
            assert!(!layout.empty_block);
            assert_eq!(layout.segments.len(), 1);
            let mut walk = layout.tail.clone().unwrap();
            match walk.next(&mut c) {
                Ok(TailRead::Terminator) => assert!(terminator),
                Err(Error::Fatal(m)) => {
                    assert!(!terminator);
                    assert_eq!(m, "Input file is corrupted or in a wrong format");
                }
                other => panic!("{other:?}"),
            }
        }
        let file = voc_of(&[&block1(0xa6, 40), &[3, 0x40, 0, 0, 0x10, 0, 0xa6]]);
        let layout = scan(&mut std::io::Cursor::new(file)).unwrap();
        assert_eq!(layout.total_samples, 40);
        assert!(layout.unterminated);
    }
}
