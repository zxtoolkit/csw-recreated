//! Sampled input as a stream: a container scan yields a [`Layout`] (rate,
//! sample encoding, where the sample bytes are), and [`SampleSource`] decodes
//! those runs a chunk at a time.

use std::io::{Read, Seek, SeekFrom};

use crate::error::{Error, Result};

/// Samples decoded per chunk, per channel.
pub const CHUNK: usize = 65_536;

/// How the sample bytes are encoded, and how they map onto the 0..=255
/// domain the detector works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// 8-bit unsigned, as WAV and VOC store it.
    U8,
    /// 8-bit signed, as IFF/8SVX stores it.
    S8,
}

impl Encoding {
    /// Bytes one sample of one channel occupies.
    fn width(self) -> usize {
        match self {
            Encoding::U8 | Encoding::S8 => 1,
        }
    }

    fn decode(self, bytes: &[u8]) -> f64 {
        match self {
            Encoding::U8 => bytes[0] as f64,
            Encoding::S8 => bytes[0] as i8 as f64,
        }
    }
}

/// A run of sample bytes in the file.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub offset: u64,
    pub bytes: u64,
}

/// What a container scan yields: everything about the signal except the
/// samples themselves.
pub struct Layout {
    pub rate: u32,
    /// The rate the console reports, which is `rate` for every container but
    /// a VOC whose blocks carried none.
    pub shown_rate: i64,
    pub design_rate: f64,
    /// Level separating low from high, in the decoded domain.
    pub midpoint: f64,
    /// The status line describing the format.
    pub desc: String,
    /// Print the checking line as `Checking input file integrity...` (VOC).
    pub integrity_check: bool,
    pub encoding: Encoding,
    pub channels: usize,
    /// Where the sample bytes are, in file order.
    pub segments: Vec<Segment>,
    /// A VOC whose blocks ran into the end of the file with no
    /// terminator: the console carries a warning and the samples are
    /// read twice.
    pub unterminated: bool,
    /// A VOC sound block whose length, read signed, is not positive: no
    /// samples, or a header longer than its claim. It abandons the run.
    pub empty_block: bool,
    pub tail: Option<crate::voc::Tail>,
    /// Mono samples the container *declares*: reported as such, even when a
    /// truncated file's segments hold fewer. Only the samples present are
    /// converted.
    pub total_samples: u64,
}

impl Layout {
    /// Samples a run of `bytes` decodes to, channels folded.
    pub fn samples_in(&self, bytes: u64) -> u64 {
        let frame = self.encoding.width() as u64 * self.channels as u64;
        bytes.checked_div(frame).unwrap_or(0)
    }
}

/// A sampled input part-way through being read.
pub struct SampleSource<R> {
    pub layout: Layout,
    reader: R,
    /// Segment being read, and how far into it.
    segment: usize,
    at: u64,
    positioned: bool,
    /// Reused across chunks.
    buf: Vec<u8>,
    tail: Option<crate::voc::Tail>,
}

impl<R: Read + Seek> SampleSource<R> {
    pub fn new(layout: Layout, reader: R) -> Self {
        SampleSource {
            tail: layout.tail.clone(),
            layout,
            reader,
            segment: 0,
            at: 0,
            positioned: false,
            buf: Vec::new(),
        }
    }

    /// Back to the first sample. The detector reads the signal three times
    /// from the start: two polarity probes, then the real run.
    pub fn rewind(&mut self) {
        self.segment = 0;
        self.at = 0;
        self.positioned = false;
        match (&mut self.tail, &self.layout.tail) {
            (Some(tail), Some(fresh)) => tail.reset(fresh),
            _ => self.tail = self.layout.tail.clone(),
        }
    }

    pub fn past_end(&mut self) -> Result<Option<crate::voc::TailRead>> {
        let Some(tail) = self.tail.as_mut() else {
            return Ok(None);
        };
        self.positioned = false;
        tail.next(&mut self.reader).map(Some)
    }

    /// Fill `out` with up to [`CHUNK`] mono samples; `false` at the end.
    ///
    /// Channels are folded per frame and frames never straddle a chunk, so the
    /// result does not depend on where the chunks fall.
    pub fn next_chunk(&mut self, out: &mut Vec<f64>) -> Result<bool> {
        out.clear();
        let width = self.layout.encoding.width();
        let frame = width * self.layout.channels;
        if frame == 0 {
            return Ok(false);
        }
        while self.segment < self.layout.segments.len() {
            let seg = self.layout.segments[self.segment];
            // Whole frames only: a segment's trailing partial frame is dropped.
            let usable = seg.bytes - seg.bytes % frame as u64;
            if self.at >= usable {
                self.segment += 1;
                self.at = 0;
                self.positioned = false;
                continue;
            }
            if !self.positioned {
                self.reader
                    .seek(SeekFrom::Start(seg.offset + self.at))
                    .map_err(|e| Error::Fatal(format!("Cannot read the input file: {e}").into()))?;
                self.positioned = true;
            }
            let want = ((CHUNK * frame) as u64).min(usable - self.at) as usize;
            self.buf.resize(want, 0);
            self.reader
                .read_exact(&mut self.buf)
                .map_err(|_| Error::Format("input file ends mid-sample".into()))?;
            self.at += want as u64;
            out.reserve(want / frame);
            for f in self.buf.chunks_exact(frame) {
                let sum: f64 = f
                    .chunks_exact(width)
                    .map(|s| self.layout.encoding.decode(s))
                    .sum();
                out.push(sum / self.layout.channels as f64);
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Every sample at once, for tests that assert on whole signals.
    #[cfg(test)]
    pub fn collect(mut self) -> Result<Vec<f64>> {
        let mut all = Vec::new();
        let mut chunk = Vec::new();
        while self.next_chunk(&mut chunk)? {
            all.extend_from_slice(&chunk);
        }
        Ok(all)
    }
}

/// Read exactly `n` bytes at `offset`, for scanning container headers.
pub fn read_at<R: Read + Seek>(reader: &mut R, offset: u64, n: usize) -> Result<Vec<u8>> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| Error::Fatal(format!("Cannot read the input file: {e}").into()))?;
    let mut buf = vec![0u8; n];
    reader
        .read_exact(&mut buf)
        .map_err(|e| Error::Fatal(format!("Cannot read the input file: {e}").into()))?;
    Ok(buf)
}

/// The file's length, which several containers' scans and status lines need.
pub fn len_of<R: Seek>(reader: &mut R) -> Result<u64> {
    reader
        .seek(SeekFrom::End(0))
        .map_err(|e| Error::Fatal(format!("Cannot read the input file: {e}").into()))
}
