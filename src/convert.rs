//! The conversions, driven by a caller that owns the input, the output and
//! the console: a file or a buffer in, a file or a buffer out.

use std::io::{Read, Seek, Write};

use crate::container::{self, Compression, CswInfo};
use crate::encode::PulseEncoder;
use crate::error::{Error, Result};
use crate::filter::FilterSpec;
use crate::signal::{self, Pulses};
use crate::{iff, out, source, voc, wav};

/// The rate an OUT trace is rendered at: 65000 Hz is stamped in the CSW.
pub const OUT_RATE: u32 = 65000;
/// Encoding-application field, written into a CSW v2 header and nowhere else:
/// a `-1` file has no such field, and neither has a decode's WAV or VOC.
pub const APP_NAME: &str = "CSW v2.00";

/// What a decode writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeTarget {
    Wav,
    Voc,
}

/// The switches a conversion reads.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// `Some(spec)` when `-f` was given.
    pub filter: Option<FilterSpec>,
    /// CSW container version to write: 2 (default) or 1 (`-1`).
    pub csw_version: u8,
    /// `-z`: plain RLE, the "old compression method", in place of Z-RLE. A
    /// v1 file (`-1`) is plain RLE regardless.
    pub old_compression: bool,
}

impl Default for Settings {
    /// No filter, CSW v2, Z-RLE: the command line with no switches.
    fn default() -> Self {
        Settings {
            filter: None,
            csw_version: 2,
            old_compression: false,
        }
    }
}

/// Whether the written CSW uses the "old compression method" (plain RLE):
/// `-z` selects it, and `-1` (v1) has only plain RLE to write.
pub fn writes_plain_rle(settings: &Settings) -> bool {
    settings.old_compression || settings.csw_version == 1
}

/// Whether the pulses go down the Z-RLE path -- the default, and the one
/// whose writer frame zeroes the detector's stale candidate between calls
/// (`detect::PulseDetector::with_zrle_frame`). `-z` and `-1` take the other.
pub fn zrle_output(settings: &Settings) -> bool {
    !writes_plain_rle(settings)
}

pub fn output_compression(settings: &Settings) -> Compression {
    if writes_plain_rle(settings) {
        Compression::Rle
    } else {
        Compression::ZRle
    }
}

pub fn writing_and_working<P: Report + ?Sized>(settings: &Settings, report: &mut P) -> Result<()> {
    if writes_plain_rle(settings) {
        report.writing_old_compression(settings.csw_version)?;
    }
    report.working()
}

/// "MM:SS", or "HH:MM:SS" from the first hour.
pub fn clock(whole: i64) -> String {
    let (h, m, s) = (whole / 3600, (whole / 60) % 60, whole % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// The console lines a conversion prints, in the order it prints them. Each
/// method is one line; the caller renders it.
pub trait Report {
    /// Opens the "Checking input file..." line, left unterminated.
    fn checking_start(&mut self, integrity: bool) -> Result<()>;
    /// Completes the open checking line: "ok!", or a refusal in its place.
    fn checking_completion(&mut self, completion: &str) -> Result<()>;
    /// Completes the open checking line with "ok!".
    fn checking_ok(&mut self) -> Result<()> {
        self.checking_completion("ok!")
    }
    /// The whole checking line at once.
    fn checking(&mut self, integrity: bool) -> Result<()> {
        self.checking_start(integrity)?;
        self.checking_ok()
    }
    fn describe(&mut self, desc: &str) -> Result<()>;
    fn sampling_rate(&mut self, rate: i64, samples: u64) -> Result<()>;
    fn warn_block(&mut self, msg: &str) -> Result<()>;
    fn digital_filter(&mut self, spec: &FilterSpec) -> Result<()>;
    fn writing_old_compression(&mut self, version: u8) -> Result<()>;
    fn working(&mut self) -> Result<()>;
    fn packed(&mut self, packed_bytes: u64, pulses: usize, orig_bytes: u64) -> Result<()>;
    fn csw_header(&mut self, major: u8, minor: u8, rate: u32, file: &[u8]) -> Result<()>;
    fn csw_app(&mut self, app: &[u8], comp: u8) -> Result<()>;
    fn total_samples(&mut self, samples: u64, rate: u32) -> Result<()>;
    /// The decode is about to write; prints nothing. [`Report::completed`]
    /// reports the rate against the time from here.
    fn conversion_starts(&mut self) -> Result<()>;
    fn writing(&mut self, kind: &str, file: &[u8]) -> Result<()>;
    fn completed(&mut self, written: u64) -> Result<()>;
}

/// Encode an opened input into a CSW written to `out`, which the caller has
/// created before the call. `input_name` chooses the OUT reader by its
/// extension; `shown_name` is the input as the user typed it.
pub fn encode<R: Read + Seek, W: Write, P: Report + ?Sized>(
    file: &mut R,
    input_name: &[u8],
    shown_name: &[u8],
    settings: &Settings,
    report: &mut P,
    out: &mut W,
) -> Result<()> {
    let lower = input_name.to_ascii_lowercase();
    let orig_bytes = source::len_of(file)?;

    // Resolve the input into pulses, printing the per-format status lines.
    // OUT is timed in T-states, so it has no sampling-rate line; the playing
    // time on that line is the input's declared sample count, printed before
    // anything is converted.
    let sig = if lower.ends_with(b".out") {
        // `-s` is ignored here: the rate is fixed (see `OUT_RATE`).
        let rate = OUT_RATE;
        // Opened before the read, as for every other format, so a trace
        // whose length is not a whole number of records completes this line
        // with "bad size!" in place of "ok!".
        report.checking_start(false)?;
        let (sig, t_total, described) = out::read(file, rate)?;
        report.checking_ok()?;
        // The playing time comes from the trace's own T-state total, which is
        // 32-bit and wraps (see `out::read`), not from the pulses -- a
        // backwards step makes those two say different things.
        let secs = t_total as u64 / 3_500_000;
        report.describe(&format!(
            "Z80 emulation trace file (OUT), playing time: {} ({} pulses)",
            clock(secs as i64),
            described
        ))?;
        // Nothing to convert. The advice goes *after* the description line.
        if described == 0 {
            let mut msg = b"The specified .OUT file (\"".to_vec();
            msg.extend_from_slice(shown_name);
            msg.extend_from_slice(
                b"\") is useless.\n\
                  Please make sure that the emulator is correctly configured to log OUTs\n\
                  to port 0xFE (254 decimal) and then generate a new .OUT file.",
            );
            return Err(Error::FatalBlock(msg.into()));
        }
        writing_and_working(settings, report)?;
        sig
    } else {
        // Scanned first: the container's structure, not its samples. The
        // checking line opens *before* the scan for all but VOC, whose
        // refusals are bare lines with no checking line above them. The
        // reader is chosen by what the file opens with, not by its name:
        // `.out` above is the only extension that names one, a WAV called
        // `.voc` converts as a WAV, and a file with none of the three
        // signatures is "Wrong file type" before the checking line opens.
        let have = source::len_of(file)?.min(20) as usize;
        let signature = source::read_at(file, 0, have)?;
        let is_voc = signature.starts_with(voc::MAGIC);
        let is_iff = signature.starts_with(b"FORM");
        if !is_voc && !is_iff && !signature.starts_with(b"RIFF") {
            return Err(Error::Fatal("Wrong file type".into()));
        }
        if !is_voc {
            report.checking_start(false)?;
        }
        let layout = if is_voc {
            voc::scan(file)?
        } else if is_iff {
            iff::scan(file)?
        } else {
            wav::scan(file)?
        };
        // `-s` is ignored for sampled file input: the rate is the one the
        // container declares. The switch is for DirectMode.
        let rate = layout.rate;
        let shown_rate = layout.shown_rate;
        // Only VOC input gets the "integrity" checking line -- and only VOC
        // still has a line to print here, the rest having opened theirs above.
        if is_voc {
            report.checking(layout.integrity_check)?;
        } else {
            report.checking_ok()?;
        }
        report.describe(&layout.desc)?;
        // A VOC sound block whose length, read signed, is not positive
        // abandons the run in silence -- but only once the lines below have
        // been printed, so the flag travels and `empty_block` raises it. A
        // WAV `data` chunk of zero length converts.
        let empty_block = layout.empty_block;
        // A VOC that ran into the end of the file with no terminator: the
        // warning goes under the sampling-rate line, above the filter's.
        let unterminated = layout.unterminated;

        // Then the samples, a chunk at a time, straight into the encoder: a
        // long tape rip is never held in memory.
        let input_samples = layout.total_samples;
        let midpoint = layout.midpoint;
        let design_rate = layout.design_rate;
        report.sampling_rate(shown_rate, input_samples)?;
        if unterminated {
            report.warn_block("***WARNING*** - Unexpected end of file!")?;
        }
        if let Some(spec) = &settings.filter {
            report.digital_filter(spec)?;
        }
        let mut src = source::SampleSource::new(layout, file);
        let mut enc = PulseEncoder::new(
            rate,
            design_rate,
            midpoint,
            settings.filter,
            zrle_output(settings),
            is_voc,
        );
        if !empty_block {
            enc.probe(&mut src)?;
        }
        writing_and_working(settings, report)?;
        if empty_block {
            return Err(Error::Silent(
                "VOC sound block of no positive length".into(),
            ));
        }
        if let Err(e) = enc.real(&mut src) {
            // A refusal exits through the file close, which flushes what the
            // run has written so far; `Error::Silent` flushes nothing, leaving
            // the empty file the open created.
            if !matches!(e, Error::Silent(_)) {
                let compression = output_compression(settings);
                if let Ok(mut bytes) =
                    container::write(&enc.partial(), settings.csw_version, compression, APP_NAME)
                {
                    if compression == Compression::ZRle {
                        bytes.truncate(0x34);
                    }
                    let _ = out.write_all(&bytes);
                }
            }
            return Err(e);
        }
        enc.finish()
    };

    write_csw(&sig, orig_bytes, settings, report, out)
}

/// Write the CSW container to `out` and print the closing "Packed size" line.
pub fn write_csw<W: Write, P: Report + ?Sized>(
    sig: &Pulses,
    orig_bytes: u64,
    settings: &Settings,
    report: &mut P,
    out: &mut W,
) -> Result<()> {
    let compression = output_compression(settings);
    let bytes = container::write(sig, settings.csw_version, compression, APP_NAME)?;
    out.write_all(&bytes)
        .and_then(|()| out.flush())
        .map_err(|_| Error::Fatal("Could not create output file".into()))?;

    // "Packed size" is the whole CSW file, header included, and the
    // compression ratio is taken against that -- not against the
    // header-stripped payload.
    report.packed(bytes.len() as u64, sig.declared as usize, orig_bytes)
}

/// Decode a CSW held in `raw`, whose header is `info`, streaming the audio to
/// `out`; returns the bytes written. `shown_path` and `out_name` are what the
/// console prints for the two files.
pub fn decode<W: Write, P: Report + ?Sized>(
    raw: &[u8],
    info: &CswInfo,
    shown_path: &[u8],
    target: DecodeTarget,
    out_name: &[u8],
    report: &mut P,
    out: &mut W,
) -> Result<u64> {
    // The header line comes before the pulse data is touched, so an
    // unsupported compression type is reported under it.
    report.csw_header(info.major, info.minor, info.rate, shown_path)?;
    // The version gate, applied under the header line just printed:
    // `major*100 + minor` must be at most 299, so v2.99 is read and v3.00 is
    // not. Exit code 252.
    if info.major as u32 * 100 + info.minor as u32 > 299 {
        return Err(Error::Input(
            "CSW version not supported, please upgrade this tool.".into(),
            252,
        ));
    }
    // Walked twice, never collected: once to measure (which finds an
    // unsupported compression type, and sizes the count line and the WAV
    // header), once to write.
    let sig = container::pulse_source(info, &raw[info.data_offset..]);
    // A v2 header whose pulse count disagrees with the pulse data under it
    // is not remarked on: what is there gets decoded, in silence. The count
    // is a hint, and the pulses are the file.
    let measured = signal::survey(&sig)?;
    if info.major == 2 {
        report.csw_app(&info.app, info.compression)?;
    }
    report.total_samples(measured.total_samples, info.rate)?;

    let kind = match target {
        DecodeTarget::Wav => {
            // Refused before anything is written: a CSW header can describe
            // more audio than a RIFF container's 32-bit sizes can express.
            wav::wav_data_len(measured.total_samples)?;
            "WAV"
        }
        DecodeTarget::Voc => "VOC",
    };

    report.conversion_starts()?;
    // A failure *after* the file is open has a line of its own, and it is
    // not the create one.
    let cannot_write = || Error::Fatal("Could not write to output file".into());
    // The WAV header takes the declared total and is left disagreeing with
    // its payload; the VOC writer sizes each block from what it has placed
    // and takes nothing from the survey. See `signal::Survey`.
    let mut buf = std::io::BufWriter::with_capacity(1 << 16, out);
    let written = match target {
        DecodeTarget::Wav => wav::write_to(&mut buf, &sig, measured.total_samples),
        DecodeTarget::Voc => voc::write_to(&mut buf, &sig),
    }
    // An I/O failure part-way through is still a failure to write the file.
    .map_err(|e| match e {
        Error::Io(_) => cannot_write(),
        other => other,
    })?;
    buf.flush().map_err(|_| cannot_write())?;
    report.writing(kind, out_name)?;
    report.completed(written)?;
    Ok(written)
}
