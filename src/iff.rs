//! Amiga IFF / 8SVX read: big-endian FORM, VHDR (rate at offset 12,
//! `sCompression` at 15, which must be 0) and BODY, whose samples are 8-bit
//! *signed*, so the midpoint is 0.

use crate::error::{Error, Result};
use std::io::{Read, Seek};

#[cfg(test)]
use crate::wav::MonoWav;

use crate::source::{self, Encoding, Layout, Segment};

const MIDPOINT: f64 = 0.0;

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Scan an 8SVX IFF container: locate VHDR and BODY, and describe the signal
/// without reading a sample.
pub fn scan<R: Read + Seek>(reader: &mut R) -> Result<Layout> {
    let file_len = source::len_of(reader)?;
    if file_len < 12 {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let head = source::read_at(reader, 0, 12)?;
    if &head[0..4] != b"FORM" || &head[8..12] != b"8SVX" {
        return Err(Error::Fatal("Wrong file type".into()));
    }
    // The FORM's own size is read and not used: the walk runs to the end of
    // the file, so a FORM declaring 0 is walked all the same.
    let end = file_len;

    // The VHDR must be among the file's first five chunks and the BODY among
    // the five after it, else "Wrong file type". No pad byte is skipped
    // after an odd-sized chunk, so an ANNO of 3 bytes before the BODY is
    // refused.
    let mut pos = 12u64; // past "FORM" size "8SVX"
    let mut budget = 5;
    let (rate, compression) = loop {
        if pos + 8 > end {
            return Err(Error::Fatal("Wrong file type".into()));
        }
        let header = source::read_at(reader, pos, 8)?;
        let claimed = be_u32(&header[4..8]) as u64;
        let body = pos + 8;
        if &header[0..4] == b"VHDR" {
            // Twenty bytes of chunk whatever the chunk's length says, the
            // bytes it does not have reading as zero: a VHDR declaring 0 is a
            // rate of 0 and a compression byte of 0, not a refusal. A second
            // VHDR is never read -- the search for the BODY starts here.
            let mut chunk =
                source::read_at(reader, body, claimed.min(20).min(end - body) as usize)?;
            chunk.resize(20, 0);
            pos = body + claimed;
            break (u16::from_be_bytes([chunk[12], chunk[13]]) as u32, chunk[15]);
        }
        // NAME, ANNO, CHAN, ... are skipped wherever they sit. 8SVX has no
        // channel count in its VHDR, so a stereo file's CHAN is skipped too.
        budget -= 1;
        pos = body + claimed;
        if budget == 0 || pos >= end {
            return Err(Error::Fatal("Wrong file type".into()));
        }
    };
    if compression != 0 {
        // The non-PCM WAV refusal: it lands on the "Checking input file..."
        // line and exits 246.
        return Err(Error::Rejected(
            "Sorry, compressed IFF data not yet supported.".into(),
            246,
        ));
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut bytes = 0u64;
    let mut budget = 5;
    loop {
        if pos + 8 > end {
            return Err(Error::Fatal("Wrong file type".into()));
        }
        let header = source::read_at(reader, pos, 8)?;
        let claimed = be_u32(&header[4..8]) as u64;
        let body = pos + 8;
        if &header[0..4] == b"BODY" {
            // From the BODY's offset to the end of the **file**, not of the
            // chunk or of the FORM: whatever follows the first BODY is
            // converted as sound, chunk headers included.
            segments.push(Segment {
                offset: body,
                bytes: file_len - body,
            });
            // The *declared* length: a BODY claiming more samples than the
            // file holds is reported as it stands.
            bytes += claimed;
            break;
        }
        budget -= 1;
        pos = body + claimed;
        if budget == 0 || pos >= end {
            return Err(Error::Fatal("Wrong file type".into()));
        }
    }

    // The BODY has to be *reached*, not to hold anything: one declaring zero
    // bytes converts to an empty CSW.
    Ok(Layout {
        rate,
        shown_rate: i64::from(rate),
        design_rate: f64::from(rate),
        midpoint: MIDPOINT,
        desc: format!("IFF/8SVX (IFF), {bytes} samples."),
        integrity_check: false,
        encoding: Encoding::S8,
        channels: 1,
        segments,
        unterminated: false,
        empty_block: false,
        tail: None,
        total_samples: bytes,
    })
}

/// Whole-file read straight to pulses, for the tests.
#[cfg(test)]
pub(crate) fn read(raw: &[u8]) -> Result<crate::signal::Pulses> {
    let m = read_mono(raw)?;
    Ok(crate::detect::samples_to_pulses(
        m.rate, &m.samples, m.midpoint,
    ))
}

/// Read a whole IFF/8SVX file into memory, for the tests.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn be(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    #[test]
    fn reads_8svx() {
        // Signed 8-bit body at full scale: +,+,-,-,-,+,-,- -> pulses
        // 2,3,1,2, starting high.
        let pcm: [i8; 8] = [127, 127, -128, -128, -128, 127, -128, -128];
        let mut vhdr = Vec::new();
        vhdr.extend_from_slice(&be(0)); // oneShotHiSamples
        vhdr.extend_from_slice(&be(0)); // repeatHiSamples
        vhdr.extend_from_slice(&be(0)); // samplesPerHiCycle
        vhdr.extend_from_slice(&11025u16.to_be_bytes()); // samplesPerSec
        vhdr.push(1); // ctOctave
        vhdr.push(0); // sCompression = none
        vhdr.extend_from_slice(&be(0x10000)); // volume

        let body: Vec<u8> = pcm.iter().map(|&s| s as u8).collect();

        let mut inner = Vec::new();
        inner.extend_from_slice(b"8SVX");
        inner.extend_from_slice(b"VHDR");
        inner.extend_from_slice(&be(vhdr.len() as u32));
        inner.extend_from_slice(&vhdr);
        inner.extend_from_slice(b"BODY");
        inner.extend_from_slice(&be(body.len() as u32));
        inner.extend_from_slice(&body);

        let mut file = Vec::new();
        file.extend_from_slice(b"FORM");
        file.extend_from_slice(&be(inner.len() as u32));
        file.extend_from_slice(&inner);

        let p = read(&file).unwrap();
        assert_eq!(p.rate, 11025);
        assert!(p.initial_high);
        assert_eq!(p.pulses, vec![2, 3, 1, 2]);
    }

    #[test]
    fn rejects_compressed() {
        let mut vhdr = vec![0u8; 20];
        vhdr[15] = 1; // sCompression != 0
        let mut inner = Vec::new();
        inner.extend_from_slice(b"8SVX");
        inner.extend_from_slice(b"VHDR");
        inner.extend_from_slice(&be(vhdr.len() as u32));
        inner.extend_from_slice(&vhdr);
        let mut file = Vec::new();
        file.extend_from_slice(b"FORM");
        file.extend_from_slice(&be(inner.len() as u32));
        file.extend_from_slice(&inner);
        assert!(read(&file).is_err());
    }

    /// Each search gives up after five chunk headers, and the first VHDR wins.
    #[test]
    fn the_walk_keeps_two_budgets_of_five() {
        let vhdr = |rate: u16| {
            let mut v = Vec::new();
            v.extend_from_slice(&be(0));
            v.extend_from_slice(&be(0));
            v.extend_from_slice(&be(0));
            v.extend_from_slice(&rate.to_be_bytes());
            v.push(1);
            v.push(0);
            v.extend_from_slice(&be(0x10000));
            v
        };
        let build = |chunks: Vec<(&[u8; 4], Vec<u8>)>| {
            let mut inner = b"8SVX".to_vec();
            for (id, payload) in chunks {
                inner.extend_from_slice(id);
                inner.extend_from_slice(&be(payload.len() as u32));
                inner.extend_from_slice(&payload);
            }
            let mut file = b"FORM".to_vec();
            file.extend_from_slice(&be(inner.len() as u32));
            file.extend_from_slice(&inner);
            file
        };
        let body: Vec<u8> = vec![127, 127, 128u8.wrapping_neg(), 128u8.wrapping_neg()];
        let junk = (b"ANNO", vec![0u8; 4]);
        let mut four: Vec<(&[u8; 4], Vec<u8>)> = vec![junk.clone(); 4];
        four.push((b"VHDR", vhdr(11025)));
        four.push((b"BODY", body.clone()));
        let mut c = std::io::Cursor::new(build(four.clone()));
        assert_eq!(scan(&mut c).unwrap().rate, 11025);

        let mut five: Vec<(&[u8; 4], Vec<u8>)> = vec![junk.clone(); 5];
        five.push((b"VHDR", vhdr(11025)));
        five.push((b"BODY", body.clone()));
        let mut c = std::io::Cursor::new(build(five));
        assert!(scan(&mut c).is_err());

        // A second VHDR is skipped like any other chunk.
        let two = vec![
            (b"VHDR", vhdr(11025)),
            (b"VHDR", vhdr(22050)),
            (b"BODY", body.clone()),
        ];
        let mut c = std::io::Cursor::new(build(two));
        assert_eq!(scan(&mut c).unwrap().rate, 11025);
        let swapped = vec![
            (b"VHDR", vhdr(22050)),
            (b"VHDR", vhdr(11025)),
            (b"BODY", body.clone()),
        ];
        let mut c = std::io::Cursor::new(build(swapped));
        assert_eq!(scan(&mut c).unwrap().rate, 22050);
    }
}
