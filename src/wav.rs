//! PCM WAV read/write, and pulses rendered back to samples. Input is 8-bit
//! unsigned mono; a decoded pulse takes the levels 224 and 32.

use std::io::{Read, Seek, Write};

use crate::error::{Error, Result};
use crate::signal::PulseSource;
#[cfg(test)]
use crate::signal::Pulses;

use crate::source::{self, Encoding, Layout, Segment};

/// The two sample levels a decoded pulse takes.
pub(crate) const HIGH_8: u8 = 0xE0;
pub(crate) const LOW_8: u8 = 0x20;

/// A decoded mono signal plus the level threshold separating low from high.
#[cfg(test)]
pub struct MonoWav {
    pub rate: u32,
    pub samples: Vec<f64>,
    pub midpoint: f64,
}

/// Read a whole PCM WAV into memory, for tests that assert on a complete
/// signal.
#[cfg(test)]
pub(crate) fn read_mono(raw: &[u8]) -> Result<MonoWav> {
    let mut cursor = std::io::Cursor::new(raw);
    let layout = scan(&mut cursor)?;
    let (rate, midpoint) = (layout.rate, layout.midpoint);
    let samples = source::SampleSource::new(layout, cursor).collect()?;
    Ok(MonoWav {
        rate,
        samples,
        midpoint,
    })
}

/// Scan a RIFF/WAVE container: locate `fmt ` and `data`, and describe the
/// signal without reading a single sample.
pub fn scan<R: Read + Seek>(reader: &mut R) -> Result<Layout> {
    let end = source::len_of(reader)?;
    if end < 20 {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let head = source::read_at(reader, 0, 12)?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(Error::Fatal("Wrong file type".into()));
    }

    // `fmt ` is read at a fixed offset, so it has to be the file's first
    // chunk: a `fact` or a `LIST` ahead of it is "Wrong file type".
    let header = source::read_at(reader, 12, 8)?;
    if &header[0..4] != b"fmt " {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let fmt_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
    // Sixteen bytes whatever the chunk's length says: a shorter one reads
    // its remaining fields out of whatever follows it.
    if end < 36 {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let fmt = source::read_at(reader, 20, 16)?;
    if fmt[0] != 1 {
        // One line for every non-PCM format tag, IEEE float included.
        return Err(Error::Rejected(
            "Sorry, compressed WAV data not yet supported.".into(),
            246,
        ));
    }
    // "More than one", not "not one": a `fmt ` chunk declaring **zero**
    // channels is converted as mono. The channel count is checked before the
    // bit depth, so 16-bit stereo is reported as stereo.
    let channels = fmt[2] as usize;
    if channels > 1 {
        return Err(Error::Rejected(
            "Sorry, stereo WAV samples not yet supported.".into(),
            245,
        ));
    }
    let bits = u16::from(fmt[14]);
    if bits != 8 {
        return Err(Error::Rejected(
            format!("Sorry, {bits}-bits WAV samples not yet supported."),
            244,
        ));
    }
    // The low **word** of the four-byte field, console, playing time and
    // header alike.
    let rate = u32::from(u16::from_le_bytes([fmt[4], fmt[5]]));
    // What the chunk holds past those sixteen bytes is skipped; a length under
    // 16, or past the end of the file, abandons the run in silence: the
    // checking line is left open, nothing is written, exit 255.
    if fmt_len < 16 || 20 + fmt_len > end {
        return Err(Error::Silent(
            "WAV fmt chunk runs past the end of the file".into(),
        ));
    }

    // One `fact` chunk is tolerated between `fmt ` and `data`, nothing else:
    // the sample window below sits at a fixed offset. No pad byte is skipped
    // after an odd-sized `fact`.
    let mut intervening = 0u64;
    let mut pos = 20 + fmt_len;
    if pos + 8 > end {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let mut header = source::read_at(reader, pos, 8)?;
    if &header[0..4] == b"fact" {
        intervening = 1;
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
        if pos + 8 + size > end {
            return Err(Error::Silent(
                "WAV fact chunk runs past the end of the file".into(),
            ));
        }
        pos += 8 + size;
        if pos + 8 > end {
            return Err(Error::Fatal("Wrong file type".into()));
        }
        header = source::read_at(reader, pos, 8)?;
    }
    if &header[0..4] != b"data" {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    // A `data` chunk claiming more than the file holds is not refused: the
    // declared count is reported, and what the file holds is converted.
    // Trailing chunks are never read.
    let declared = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
    // The samples are **not** read from where `data` starts: the canonical
    // 44-byte header is assumed, and a `fact` chunk moves the window by
    // exactly 8 bytes -- its header, never its payload -- so four bytes of
    // the `data` header are converted as sound and the last bytes of the real
    // data fall off the end. Keep it so.
    let start = 44 + 8 * intervening;
    let data = Segment {
        offset: start,
        bytes: declared.min(end.saturating_sub(start)),
    };
    let mut layout = Layout {
        rate,
        shown_rate: i64::from(rate),
        design_rate: f64::from(rate),
        midpoint: 128.0,
        desc: String::new(),
        integrity_check: false,
        encoding: Encoding::U8,
        channels: channels.max(1),
        segments: vec![data],
        unterminated: false,
        empty_block: false,
        tail: None,
        total_samples: 0,
    };
    layout.total_samples = layout.samples_in(declared);
    layout.desc = format!("RIFF Wave PCM (WAV), {} samples.", layout.total_samples);
    Ok(layout)
}

/// The largest PCM payload a RIFF/WAVE file can describe. Both size fields are
/// 32-bit and the RIFF one counts 36 bytes of header on top of the data.
pub const MAX_WAV_DATA: u64 = u32::MAX as u64 - 36;

/// Check a sample count against [`MAX_WAV_DATA`] before anything is rendered:
/// a CSW can describe more audio than a WAV can hold (a few 0xFFFFFFFF
/// pulses), and such a decode is refused up front.
pub fn wav_data_len(samples: u64) -> Result<u32> {
    if samples > MAX_WAV_DATA {
        return Err(Error::Unsupported(format!(
            "{samples} samples exceed the {MAX_WAV_DATA}-byte WAV limit -- write a VOC with -dv"
        )));
    }
    Ok(samples as u32)
}

/// Feed the pulse train to `emit` as 8-bit unsigned PCM in bounded pieces.
/// The waveform is never assembled: one buffer per level, sliced per run.
pub fn stream_pcm8<S: PulseSource>(
    sig: &S,
    mut emit: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    const CHUNK: usize = 1 << 16;
    let levels = [vec![LOW_8; CHUNK], vec![HIGH_8; CHUNK]];
    let mut high = sig.initial_high();
    // Every pulse is rendered, a zero-length one included -- it flips the
    // level with nothing between. The header was sized by a sum that stops at
    // that zero (see `Survey`), so on such a file the two disagree.
    for length in sig.pulses()? {
        let buf = &levels[usize::from(high)];
        let mut left = length? as usize;
        while left > 0 {
            let n = left.min(CHUNK);
            emit(&buf[..n])?;
            left -= n;
        }
        high = !high;
    }
    Ok(())
}

/// Write pulses to `out` as an 8-bit unsigned mono WAV, returning the file
/// size. `total_samples` is what a prior `signal::survey` of the same source
/// found, since a RIFF header states its length up front.
pub fn write_to<W: Write, S: PulseSource>(out: &mut W, sig: &S, total_samples: u64) -> Result<u64> {
    let data_len = wav_data_len(total_samples)?;
    out.write_all(&wav_header(sig.rate(), 1, 8, data_len))?;
    stream_pcm8(sig, |bytes| {
        out.write_all(bytes)?;
        Ok(())
    })?;
    Ok(44 + data_len as u64)
}

/// Expand pulses into an 8-bit unsigned mono PCM byte stream (32 / 224).
#[cfg(test)]
pub(crate) fn pulses_to_pcm8(sig: &Pulses) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(sig.total_samples() as usize);
    let mut high = sig.initial_high;
    for &length in &sig.pulses {
        let v = if high { HIGH_8 } else { LOW_8 };
        pcm.resize(pcm.len() + length as usize, v);
        high = !high;
    }
    pcm
}

/// Render pulses to an 8-bit unsigned mono WAV byte image.
#[cfg(test)]
pub(crate) fn pulses_to_wav(sig: &Pulses) -> Vec<u8> {
    build_wav(sig.rate, 1, 8, &pulses_to_pcm8(sig))
}

/// Wrap raw PCM bytes in a canonical RIFF/WAVE container.
#[cfg(test)]
pub(crate) fn build_wav(rate: u32, channels: u16, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let mut out = wav_header(rate, channels, bits, pcm.len() as u32);
    out.extend_from_slice(pcm);
    out
}

/// The 44-byte RIFF/WAVE header for `data_len` bytes of PCM.
pub fn wav_header(rate: u32, channels: u16, bits: u16, data_len: u32) -> Vec<u8> {
    let block_align = channels * (bits / 8);
    let byte_rate = rate * block_align as u32;
    let mut out = Vec::with_capacity(44);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::samples_to_pulses;

    #[test]
    fn wav_pulse_roundtrip() {
        let sig = Pulses::new(44100, vec![3, 5, 1, 4, 7], true);
        let wav = pulses_to_wav(&sig);
        let mono = read_mono(&wav).unwrap();
        let back = samples_to_pulses(mono.rate, &mono.samples, mono.midpoint);
        assert_eq!(back, sig);
    }

    #[test]
    fn a_declared_rate_is_read_as_its_low_word() {
        let pcm = [0xFFu8, 0xFF, 0x00, 0x00];
        let build = |rate: u32| {
            let mut fmt = Vec::new();
            fmt.extend_from_slice(&1u16.to_le_bytes());
            fmt.extend_from_slice(&1u16.to_le_bytes());
            fmt.extend_from_slice(&rate.to_le_bytes());
            fmt.extend_from_slice(&rate.to_le_bytes());
            fmt.extend_from_slice(&1u16.to_le_bytes());
            fmt.extend_from_slice(&8u16.to_le_bytes());
            let mut body = Vec::new();
            body.extend_from_slice(b"WAVE");
            body.extend_from_slice(b"fmt ");
            body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
            body.extend_from_slice(&fmt);
            body.extend_from_slice(b"data");
            body.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
            body.extend_from_slice(&pcm);
            let mut file = Vec::new();
            file.extend_from_slice(b"RIFF");
            file.extend_from_slice(&(body.len() as u32).to_le_bytes());
            file.extend_from_slice(&body);
            file
        };
        for (declared, read_as) in [(96000u32, 30464u32), (65536, 0)] {
            let mut c = std::io::Cursor::new(build(declared));
            assert_eq!(scan(&mut c).unwrap().rate, read_as, "{declared}");
        }
    }

    /// Every `fmt ` field but the rate is compared one byte wide, so the high
    /// byte of each is ignored: 257 is PCM, 512 channels is mono, and 264 bits
    /// is eight.
    #[test]
    fn fmt_fields_are_compared_one_byte_wide() {
        let pcm = [0xFFu8, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00];
        let build = |tag: u16, ch: u16, bits: u16| {
            let mut fmt = Vec::new();
            fmt.extend_from_slice(&tag.to_le_bytes());
            fmt.extend_from_slice(&ch.to_le_bytes());
            fmt.extend_from_slice(&22050u32.to_le_bytes());
            fmt.extend_from_slice(&22050u32.to_le_bytes());
            fmt.extend_from_slice(&1u16.to_le_bytes());
            fmt.extend_from_slice(&bits.to_le_bytes());
            let mut body = Vec::new();
            body.extend_from_slice(b"WAVE");
            body.extend_from_slice(b"fmt ");
            body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
            body.extend_from_slice(&fmt);
            body.extend_from_slice(b"data");
            body.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
            body.extend_from_slice(&pcm);
            let mut file = Vec::new();
            file.extend_from_slice(b"RIFF");
            file.extend_from_slice(&(body.len() as u32).to_le_bytes());
            file.extend_from_slice(&body);
            file
        };
        for (tag, ch, bits) in [(257u16, 1u16, 8u16), (1, 257, 8), (1, 512, 8), (1, 1, 264)] {
            let mut c = std::io::Cursor::new(build(tag, ch, bits));
            let layout = scan(&mut c).expect("read one byte wide");
            assert_eq!(layout.rate, 22050);
        }
        let mut c = std::io::Cursor::new(build(1, 2, 8));
        assert!(scan(&mut c).is_err());
    }
}
