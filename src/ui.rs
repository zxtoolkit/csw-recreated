//! Console output, fixed byte for byte: CP437 status bullets (0xFE), errors
//! on stdout. A line's prefix, its ending and whether a blank line follows it
//! are all part of the contract.

#[cfg(feature = "directmode")]
use ::csw::convert::clock;
use std::io::{self, Write};

use crate::filter::{Band, Design, FilterSpec};

/// Status-line bullet: "■" in code page 437.
const BULLET_CP437: u8 = 0xFE;
/// The same glyph as [`BULLET_CP437`], encoded for a terminal that reads UTF-8.
const BULLET_CP437_AS_UTF8: &[u8] = "\u{25A0}".as_bytes();

/// The banner. Fixed text: it identifies the format version, not the build.
const BANNER: &[u8] = b"\n-=[ CSW v2.00 ]=-  Ramsoft's CSW converter, recreated (GPL v2.0+).\n\n";

/// The help screen. Fixed text, and complete: the switches it lists are the
/// switches there are, so anything else is an invalid switch.
const HELP_HEAD: &[u8] = b"\
Squares and compresses sample files to CSW format and vice versa.\n\
Supports VOC/WAV/IFF/OUT and realtime conversion from soundcard input.\n\
Syntax : CSW [options] inputfile [outputfile]\n\n\
Options:\t-d:  Decompress to WAV file. Use -dv to write a VOC file.\n\
\t\t     Can be omitted if the inputfile extension is .CSW\n\
\t\t-r:  Enable realtime soundcard input processing (DirectMode)\n\
\t\t-s<rate>: Set sampling rate for DirectMode [default: 32258 Hz]\n\
\t\t-t<secs>: Set recording time (seconds) for DirectMode.\n\
\t\t-k:  Save DirectMode sampling data as WAV file.\n\
\t\t-c:  Enable SoundBlaster compatibility mode (DirectMode)\n\
\t\t-f:  Enable digital filter\n";

/// The rest of the help screen, from the `3` sub-option on.
const HELP_TAIL: &[u8] = b"\
\t\t     3: disable 3DNow! acceleration\n\n\
For more info, please read the enclosed documentation and MakeTZX's manual.\n";

/// A volume-meter cell: the glyph CP437 0xFE draws. Only ever drawn to a
/// live terminal (see `record::run_session`).
#[cfg(feature = "directmode")]
const CELL: &[u8] = BULLET_CP437_AS_UTF8;
/// Cells across a meter: one per column of an 80-column terminal.
#[cfg(feature = "directmode")]
const METER_CELLS: usize = 80;
/// Lines the meter block occupies: the prompt, the two bars, and the recording
/// line.
#[cfg(feature = "directmode")]
const METER_LINES: usize = 4;
/// Columns the readouts need after a bar. Below this much slack the window
/// gets bars alone.
#[cfg(feature = "directmode")]
const LABEL_COLUMNS: usize = 13;
/// Cells per unit of RMS amplitude in the 8-bit domain -- 80/128, so a
/// full-scale square wave fills the row.
#[cfg(feature = "directmode")]
const CELLS_PER_UNIT: f32 = 0.625;
/// The meter's text attributes, as the colours the CGA palette gives them:
/// bright green for the level bar (attribute 0x0A), bright red for the
/// clipping bar (0x0C), blue for the unfilled remainder of both (0x01).
#[cfg(feature = "directmode")]
const LEVEL_COLOUR: &[u8] = b"\x1b[38;2;85;255;85m";
#[cfg(feature = "directmode")]
const CLIP_COLOUR: &[u8] = b"\x1b[38;2;255;85;85m";
#[cfg(feature = "directmode")]
const EMPTY_COLOUR: &[u8] = b"\x1b[38;2;0;0;170m";
#[cfg(feature = "directmode")]
const RESET_COLOUR: &[u8] = b"\x1b[0m";
/// Erase from the cursor to the end of the line.
#[cfg(feature = "directmode")]
const ERASE_LINE: &[u8] = b"\x1b[K";
/// Back to the first line of the meter block.
#[cfg(feature = "directmode")]
const CURSOR_TO_BLOCK_TOP: &[u8] = b"\x1b[3A";
#[cfg(feature = "directmode")]
const _: () = assert!(METER_LINES == 4);
/// Hide and show the cursor, which parks on the first cell of the block
/// between redraws.
#[cfg(feature = "directmode")]
const HIDE_CURSOR: &[u8] = b"\x1b[?25l";
#[cfg(feature = "directmode")]
pub const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
/// The meter-phase prompt and the pause notice, drawn as the first line of
/// the meter block.
#[cfg(feature = "directmode")]
pub const PROMPT_START: &str = "* Press any key to start conversion when done with volume meter...";
#[cfg(feature = "directmode")]
pub const PROMPT_PAUSED: &str = "* PAUSED, press any key to continue...";
#[cfg(feature = "directmode")]
pub const ERASE_START: &str = "\r\t\t\t\t\t\t\t\t\t\r";
#[cfg(feature = "directmode")]
pub const ERASE_PAUSED: &str = "\r\t\t\t\t\t\t\t\t\r";

/// The bracketed values on the help screen: the filter settings in force, so
/// `csw -fo4 -fh300` shows order 4 and a 300 Hz upper cutoff. Frequencies and
/// ripple are formatted as `%g` with six significant digits ([`fmt_ostream`]).
struct HelpValues {
    order: i32,
    /// The `t<n>` code: 3 low-pass, 4 band-pass, 5 high-pass -- or whatever
    /// number `-ft` was given, which is neither validated nor forgotten:
    /// `-ft-1` shows `[-1]`.
    band: i32,
    /// The `p<n>` code: 1 Butterworth, 2 Chebyshev.
    design: i32,
    high_hz: String,
    low_hz: String,
    ripple_db: String,
}

impl HelpValues {
    fn of(filter: &FilterSpec) -> Self {
        HelpValues {
            order: filter.order,
            band: match filter.band {
                Band::LowPass => 3,
                Band::BandPass => 4,
                Band::HighPass => 5,
                Band::Unknown(n) => n,
            },
            design: match filter.design {
                Design::Butterworth => 1,
                Design::Chebyshev => 2,
                Design::Other(n) => n,
            },
            high_hz: fmt_ostream(filter.high_hz),
            low_hz: fmt_ostream(filter.low_hz),
            ripple_db: fmt_ostream(filter.ripple_db),
        }
    }
}

/// Console writer. `terminal` says whether the destination is one; the output
/// is code page 437 (see [`Console::bullet`]), and a destination that is not
/// a terminal keeps every byte as written.
pub struct Console {
    terminal: bool,
}

impl Console {
    pub fn new(terminal: bool) -> Self {
        Console { terminal }
    }

    /// The status bullet: the CP437 byte 0xFE to a redirected stream, the
    /// glyph it stands for to a terminal.
    fn bullet(&self) -> &'static [u8] {
        if self.terminal {
            BULLET_CP437_AS_UTF8
        } else {
            &[BULLET_CP437]
        }
    }

    #[cfg(feature = "directmode")]
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    /// The banner, printed before anything else, the help screen included.
    pub fn banner(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(BANNER)
    }

    /// The help screen, with the filter settings in force in its brackets.
    pub fn help(&self, w: &mut impl Write, filter: &FilterSpec) -> io::Result<()> {
        let v = HelpValues::of(filter);
        w.write_all(HELP_HEAD)?;
        writeln!(w, "\t\t     o<n>: set filter order [{}]", v.order)?;
        writeln!(
            w,
            "\t\t     t<n>: set type (3=Low-Pass 4=Band-Pass 5=High-Pass) [{}]",
            v.band
        )?;
        writeln!(w, "\t\t     h<n>: set upper cutoff freq [{} Hz]", v.high_hz)?;
        writeln!(w, "\t\t     l<n>: set lower cutoff freq [{} Hz]", v.low_hz)?;
        writeln!(
            w,
            "\t\t     p<n>: prototype, 1=Butterworth 2=Chebyshev [{}]",
            v.design
        )?;
        writeln!(
            w,
            "\t\t     r<n>: set ripple (Chebyshev only) [{} dB]",
            v.ripple_db
        )?;
        w.write_all(HELP_TAIL)
    }

    /// A `"■ <msg>"` status line (msg without the leading space or line ending).
    fn status(&self, w: &mut impl Write, msg: &str) -> io::Result<()> {
        self.status_raw(w, msg.as_bytes())
    }

    /// The same line from bytes, for a message carrying bytes printed
    /// straight out of memory (`csw_app`).
    fn status_raw(&self, w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
        w.write_all(self.bullet())?;
        w.write_all(b" ")?;
        w.write_all(msg)?;
        w.write_all(b"\n")
    }

    /// The input-format description line.
    pub fn describe(&self, w: &mut impl Write, desc: &str) -> io::Result<()> {
        self.status(w, desc)
    }

    /// The checking line up to its completion, and no further. It is printed
    /// *before* the file is looked at, so a WAV that is turned down ("Wrong
    /// file type") leaves the line hanging and the FATAL line, which
    /// opens with a CR, overwrites it on a terminal.
    pub fn checking_start(&self, w: &mut impl Write, integrity: bool) -> io::Result<()> {
        let what = if integrity {
            "Checking input file integrity..."
        } else {
            "Checking input file..."
        };
        w.write_all(b"\r")?;
        w.write_all(self.bullet())?;
        write!(w, " {what} ")
    }

    /// How that line ends: "ok!", or the refusal in its place.
    pub fn checking_completion(&self, w: &mut impl Write, completion: &str) -> io::Result<()> {
        writeln!(w, "{completion}")
    }

    /// `"■ Sampling rate: <rate> Hz (playing time: MM:SS.mmm)"`
    pub fn sampling_rate(&self, w: &mut impl Write, rate: i64, samples: u64) -> io::Result<()> {
        self.status(
            w,
            &format!(
                "Sampling rate: {rate} Hz (playing time: {})",
                play_ms(samples, rate)
            ),
        )
    }

    /// `"■ Digital filter: <design> <band>, order <n>[, ripple <r>]"` --
    /// sampled input only; no line is printed for an OUT trace. The cutoffs
    /// are the ones the design uses, to two decimal places.
    pub fn digital_filter(&self, w: &mut impl Write, spec: &FilterSpec) -> io::Result<()> {
        let design = match spec.design {
            Design::Butterworth => "Butterworth",
            // Anything but 1 is named "Chebyshev"; only 2 gets the ripple.
            Design::Chebyshev | Design::Other(_) => "Chebyshev",
        };
        let band = match spec.band {
            Band::LowPass => format!("low-pass {} Hz", fmt_fixed(spec.high_hz, 2)),
            Band::HighPass => format!("high-pass {} Hz", fmt_fixed(spec.low_hz, 2)),
            Band::BandPass => format!(
                "band-pass {}-{} Hz",
                fmt_fixed(spec.low_hz, 0),
                fmt_fixed(spec.high_hz, 0)
            ),
            // A placeholder with a trailing space, so the line reads
            // "Butterworth ??? , order 2".
            Band::Unknown(_) => "??? ".to_string(),
        };
        let mut line = format!(
            "Digital filter: {design} {band}, order {}",
            spec.designed_order()
        );
        if spec.design == Design::Chebyshev {
            line.push_str(&format!(", ripple {}", fmt_fixed(spec.ripple_db, 2)));
        }
        self.status(w, &line)
    }

    /// Progress line, left **open**: the "Packed size" line opens with the
    /// carriage return that overwrites it, so a run abandoned here ends with
    /// no return at all.
    pub fn working(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(b"* Working...")
    }

    /// `"* Writing CSW v<...> with old compression method"` -- printed only
    /// when writing plain RLE, which `-z` and `-1` select. The default Z-RLE
    /// path prints no line of its own.
    pub fn writing_old_compression(&self, w: &mut impl Write, version: u8) -> io::Result<()> {
        // A v1 file names its exact revision, a v2 file only its major.
        let label = if version == 1 { "v1.01" } else { "v2" };
        writeln!(w, "* Writing CSW {label} with old compression method")
    }

    /// `"* Packed size: <bytes> bytes (<pulses> pulses), compression ratio: P% (R:1)"`
    pub fn packed(
        &self,
        w: &mut impl Write,
        packed_bytes: u64,
        pulses: usize,
        orig_bytes: u64,
    ) -> io::Result<()> {
        // A DirectMode recording has no original size: the division by
        // nothing prints as "-Inf% (0:1)".
        let (pct, ratio) = if orig_bytes > 0 {
            (
                format!(
                    "{:.2}",
                    (1.0 - packed_bytes as f64 / orig_bytes as f64) * 100.0
                ),
                orig_bytes as f64 / packed_bytes as f64,
            )
        } else {
            ("-Inf".to_string(), 0.0)
        };
        let line = format!(
            "* Packed size: {packed_bytes} bytes ({pulses} pulses), compression ratio: {pct}% ({ratio:.0}:1)"
        );
        write!(w, "\r{line}\n\n")
    }

    // --- decode side ---------------------------------------------------------

    pub fn csw_header(
        &self,
        w: &mut impl Write,
        major: u8,
        minor: u8,
        rate: u32,
        file: &[u8],
    ) -> io::Result<()> {
        let mut msg = format!(
            "Compressed Square Wave v{major}.{minor:02} at {} Hz (file '",
            rate as i32
        )
        .into_bytes();
        msg.extend_from_slice(file);
        msg.extend_from_slice(b"').");
        self.status_raw(w, &msg)
    }

    /// `app` is bytes, not text: an unterminated field prints on into the
    /// payload length (see `container::CswInfo::app`).
    pub fn csw_app(&self, w: &mut impl Write, app: &[u8], comp: u8) -> io::Result<()> {
        let mut msg = b"CSW created by application '".to_vec();
        msg.extend_from_slice(app);
        msg.extend_from_slice(format!("' using compression type {comp}").as_bytes());
        self.status_raw(w, &msg)
    }

    pub fn total_samples(&self, w: &mut impl Write, samples: u64, rate: u32) -> io::Result<()> {
        let secs = samples / u64::from(rate);
        self.status(
            w,
            &format!(
                "Total {samples} samples, playing time {}:{:02}.",
                secs / 60,
                secs % 60
            ),
        )
    }

    pub fn writing(&self, w: &mut impl Write, kind: &str, file: &[u8]) -> io::Result<()> {
        let mut msg = format!("Writing {kind} file '").into_bytes();
        msg.extend_from_slice(file);
        msg.extend_from_slice(b"'... done!");
        self.status_raw(w, &msg)
    }

    /// `"* Completed, conversion rate: <kbps> KBps."` -- the closing line of a
    /// decode.
    pub fn completed(&self, w: &mut impl Write, kbps: f64) -> io::Result<()> {
        writeln!(w, "* Completed, conversion rate: {kbps:.2} KBps.")
    }

    // --- errors --------------------------------------------------------------

    pub fn fatal(&self, w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
        w.write_all(b"\rFATAL ERROR: ")?;
        w.write_all(msg)?;
        w.write_all(b"\n\n")
    }

    /// A fatal message of several lines, printed flat: no leading CR and no
    /// blank line after it, unlike `fatal`. The useless-OUT message is the
    /// one that takes this shape.
    pub fn fatal_block(&self, w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
        w.write_all(b"FATAL ERROR: ")?;
        w.write_all(msg)?;
        w.write_all(b"\n")
    }

    /// `"ERROR: <msg>"` -- the input-file report.
    pub fn error(&self, w: &mut impl Write, msg: &[u8]) -> io::Result<()> {
        w.write_all(b"ERROR: ")?;
        w.write_all(msg)?;
        w.write_all(b"\n")
    }

    /// A refusal with no prefix at all, on a line of its own: the form used
    /// for a VOC whose codec or block type is not taken.
    pub fn refused(&self, w: &mut impl Write, msg: &str) -> io::Result<()> {
        write!(w, "\n{msg}\n\n")
    }

    /// A `***WARNING***` line between the status lines and the writer's.
    pub fn warn_block(&self, w: &mut impl Write, msg: &str) -> io::Result<()> {
        write!(w, "\n{msg}\n")
    }
}

/// DirectMode's console output, compiled in with the `directmode` feature.
#[cfg(feature = "directmode")]
impl Console {
    // --- DirectMode (-r) -----------------------------------------------------

    /// `"■ Input device: '<name>' (<n> ch, <format>)."`
    pub fn input_device(
        &self,
        w: &mut impl Write,
        name: &str,
        channels: u16,
        format: &str,
    ) -> io::Result<()> {
        self.status(
            w,
            &format!("Input device: '{name}' ({channels} ch, {format})."),
        )
    }

    /// "■ Operating in compatibility mode." -- `-c`: take the device's own
    /// configuration, unnegotiated.
    pub fn device_compat_mode(&self, w: &mut impl Write) -> io::Result<()> {
        self.status(w, "Operating in compatibility mode.")
    }

    pub fn capture_rate(&self, w: &mut impl Write, rate: u32, asked: u32) -> io::Result<()> {
        self.status(
            w,
            &format!("Sampling rate: {rate} Hz (rounded from: {asked} Hz)."),
        )
    }

    /// `"■ Max recording time is MM:SS (<n> bytes)."` -- the limit is the `-t`
    /// time here, the free disk space otherwise.
    pub fn max_recording_time(&self, w: &mut impl Write, secs: f64, bytes: u64) -> io::Result<()> {
        self.status(
            w,
            &format!(
                "Max recording time is {} ({bytes} bytes).",
                clock(secs as i64)
            ),
        )
    }

    /// The live meters, redrawn in place: the RMS bar, the clipping bar,
    /// their readouts, and while recording the elapsed time and sample count.
    /// `levels` is `None` while recording, when the bars are blanked and the
    /// block keeps its height; the readouts are dropped when the window has
    /// no room beside the bars.
    pub fn meter(
        &self,
        w: &mut impl Write,
        note: &str,
        levels: Option<(f32, f32)>,
        progress: Option<(f64, u64)>,
    ) -> io::Result<()> {
        self.meter_at(w, note, levels, progress, meter_layout())
    }

    fn meter_at(
        &self,
        w: &mut impl Write,
        note: &str,
        levels: Option<(f32, f32)>,
        progress: Option<(f64, u64)>,
        (width, labelled): (usize, bool),
    ) -> io::Result<()> {
        let scale = width as f32 / METER_CELLS as f32;

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(HIDE_CURSOR);
        out.push(b'\r');
        out.extend_from_slice(note.as_bytes());
        out.extend_from_slice(ERASE_LINE);
        out.extend_from_slice(b"\n");
        match levels {
            Some((rms_units, clipped)) => {
                // Both lengths are truncated, not rounded.
                let level = (rms_units * CELLS_PER_UNIT)
                    .trunc()
                    .clamp(0.0, METER_CELLS as f32);
                let clip = (clipped.clamp(0.0, 1.0) * METER_CELLS as f32).trunc();
                bar(&mut out, (level * scale) as usize, width, LEVEL_COLOUR);
                if labelled {
                    out.extend_from_slice(format!("  {:>10}", rms_db_label(rms_units)).as_bytes());
                }
                out.extend_from_slice(ERASE_LINE);
                out.extend_from_slice(b"\n");
                bar(&mut out, (clip * scale) as usize, width, CLIP_COLOUR);
                if labelled {
                    out.extend_from_slice(
                        format!("  {:>10}", format!("{:.1}% clip", clipped * 100.0)).as_bytes(),
                    );
                }
                out.extend_from_slice(ERASE_LINE);
                out.extend_from_slice(b"\n\r");
            }
            None => {
                out.extend_from_slice(b"\r");
                out.extend_from_slice(ERASE_LINE);
                out.extend_from_slice(b"\n\r");
                out.extend_from_slice(ERASE_LINE);
                out.extend_from_slice(b"\n\r");
            }
        }
        if let Some((secs, samples)) = progress {
            out.extend_from_slice(
                format!(" {}  {samples} samples", clock(secs as u64 as i64)).as_bytes(),
            );
        }
        out.extend_from_slice(ERASE_LINE);
        out.extend_from_slice(CURSOR_TO_BLOCK_TOP);
        w.write_all(&out)?;
        w.flush()
    }

    pub fn transient(&self, w: &mut impl Write, text: &str) -> io::Result<()> {
        write!(w, "{text}")?;
        w.flush()
    }

    /// Erase the meter block before printing anything permanent, leaving the
    /// cursor where the block began.
    pub fn clear_transient(&self, w: &mut impl Write) -> io::Result<()> {
        for i in 0..METER_LINES {
            w.write_all(b"\r")?;
            w.write_all(ERASE_LINE)?;
            if i + 1 < METER_LINES {
                w.write_all(b"\n")?;
            }
        }
        w.write_all(b"\r")?;
        w.write_all(CURSOR_TO_BLOCK_TOP)?;
        w.write_all(SHOW_CURSOR)?;
        w.flush()
    }

    pub fn recorded(&self, w: &mut impl Write, samples: u64, secs: f64) -> io::Result<()> {
        self.status(
            w,
            &format!(
                "Recorded {samples} samples in {}",
                clock((secs + 0.5) as i64)
            ),
        )
    }

    /// `"■ Keeping samples in file \"<name>\""` -- `-k`.
    pub fn keeping(&self, w: &mut impl Write, file: &[u8]) -> io::Result<()> {
        let mut msg = b"Keeping samples in file \"".to_vec();
        msg.extend_from_slice(file);
        msg.push(b'"');
        self.status_raw(w, &msg)
    }

    /// Dropped-sample warning: the DMA overrun.
    pub fn overrun(&self, w: &mut impl Write, lost: u64) -> io::Result<()> {
        writeln!(w, "WARNING: DMA buffer OVERRUN!!! Lost {lost} samples")
    }

    /// "WARNING: ..." for a recording the spool ended before the user did (a
    /// full volume, most often). What was captured is kept and converted.
    pub fn recording_cut_short(&self, w: &mut impl Write, reason: &str) -> io::Result<()> {
        writeln!(
            w,
            "WARNING: the recording ended early: {reason}; keeping what was captured"
        )
    }
}

// --- volume meter ------------------------------------------------------------

/// One bar: `filled` cells in `colour`, the rest blue, every cell the same
/// glyph.
#[cfg(feature = "directmode")]
fn bar(out: &mut Vec<u8>, filled: usize, width: usize, colour: &[u8]) {
    let filled = filled.min(width);
    out.push(b'\r');
    out.extend_from_slice(colour);
    for _ in 0..filled {
        out.extend_from_slice(CELL);
    }
    out.extend_from_slice(EMPTY_COLOUR);
    for _ in filled..width {
        out.extend_from_slice(CELL);
    }
    out.extend_from_slice(RESET_COLOUR);
}

/// How wide the bars are, and whether the window has room for the readouts
/// beside them: 80 cells unless the window cannot hold them, the readouts
/// going first. A window reporting no width is taken as 80 columns.
#[cfg(feature = "directmode")]
fn meter_layout() -> (usize, bool) {
    layout_for(crate::term::width())
}

#[cfg(feature = "directmode")]
fn layout_for(cols: Option<u16>) -> (usize, bool) {
    let cols = cols
        .map(|cols| cols as usize)
        .filter(|&cols| cols > 0)
        .unwrap_or(METER_CELLS);
    if cols >= METER_CELLS + LABEL_COLUMNS {
        (METER_CELLS, true)
    } else {
        (cols.min(METER_CELLS), false)
    }
}

/// "-18.4 dB" for the level bar, relative to a full-scale square wave, or
/// "-inf dB" for digital silence.
#[cfg(feature = "directmode")]
fn rms_db_label(rms_units: f32) -> String {
    /// Full scale in the 8-bit domain the meter reads.
    const FULL_SCALE: f32 = 127.0;
    if rms_units <= 0.0 {
        return "-inf dB".into();
    }
    format!("{:.1} dB", 20.0 * (rms_units / FULL_SCALE).min(1.0).log10())
}

// --- number formatting -------------------------------------------------------

/// Format a `double` for the help screen: `printf`'s `%g` with six
/// significant digits, so 4100 prints as "4100", 300.5 as "300.5", and a
/// value needing more room falls back to exponent form ("1.23457e+06").
fn fmt_ostream(v: f64) -> String {
    const PRECISION: i32 = 6;
    if v == 0.0 {
        return "0".into();
    }
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf".into() } else { "inf".into() };
    }
    // Rust renders `{:e}` as "4.1e3", so the exponent after rounding to
    // `PRECISION` digits can be read straight off the scientific form.
    let sci = format!("{:.*e}", (PRECISION - 1) as usize, v);
    let (mantissa, exp) = sci.split_once('e').expect("scientific form has an e");
    let exp: i32 = exp.parse().expect("exponent is an integer");
    if !(-4..PRECISION).contains(&exp) {
        format!(
            "{}e{}{:02}",
            trim_fraction(mantissa),
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        )
    } else {
        trim_fraction(&format!("{:.*}", (PRECISION - 1 - exp).max(0) as usize, v))
    }
}

/// Drop a trailing fraction of zeros, and the point with it -- `%g` without
/// the `#` flag prints no trailing zeros.
fn trim_fraction(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

// --- time formatting ---------------------------------------------------------

/// A double-to-`i32` conversion that truncates toward zero and yields
/// `i32::MIN` for anything out of range, NaN and the infinities included: a
/// container declaring a rate of 0 prints the time `-14:-8.-2147483648`.
fn fistp_i32(v: f64) -> i32 {
    let t = v.trunc();
    if t >= i32::MIN as f64 && t <= i32::MAX as f64 {
        t as i32
    } else {
        i32::MIN
    }
}

pub(crate) fn play_ms(samples: u64, rate: i64) -> String {
    // The whole seconds and the milliseconds are two separate truncating
    // conversions, not one rounded total: 1.9996 s is 1 s and 999 ms.
    let secs = samples as f64 / rate as f64;
    let whole = fistp_i32(secs);
    let ms = fistp_i32((secs - whole as f64) * 1000.0);
    let (h, m, s) = (whole / 3600, (whole / 60) % 60, whole % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}.{ms:03}")
    } else {
        format!("{m:02}:{s:02}.{ms:03}")
    }
}

/// `printf("%.<prec>f")`, with the digits generated and the value rounded
/// **half up** on the decimal string: 600.5 prints as `601` where Rust's
/// `{:.0}` gives `600` (ties to even), and a negative value that rounds to
/// zero prints unsigned.
pub(crate) fn fmt_fixed(v: f64, prec: usize) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    let neg = v.is_sign_negative() && v != 0.0;
    let m = v.abs();
    let mut whole = m.trunc();
    let mut frac = m - whole;
    let mut digits = Vec::with_capacity(prec + 1);
    for _ in 0..=prec {
        frac *= 10.0;
        let d = frac.trunc();
        digits.push(d as u8);
        frac -= d;
    }
    // The digit past the precision decides the rounding.
    if digits[prec] >= 5 {
        let mut i = prec;
        loop {
            if i == 0 {
                whole += 1.0;
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    // No sign on a value that rounds to nothing: `-fh-0.3` prints `0`, not
    // `-0`; `-fh-0.7` prints `-1`.
    let mut out = String::new();
    if neg && (whole != 0.0 || digits[..prec].iter().any(|&d| d != 0)) {
        out.push('-');
    }
    out.push_str(&format!("{whole:.0}"));
    if prec > 0 {
        out.push('.');
        for &d in &digits[..prec] {
            out.push((b'0' + d) as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> Console {
        Console::new(false)
    }

    #[test]
    fn status_is_cp437() {
        let mut v = Vec::new();
        console().describe(&mut v, "x").unwrap();
        assert_eq!(v, [0xFE, b' ', b'x', b'\n']);
    }

    #[test]
    fn every_line_carries_its_own_framing() {
        let ui = console();
        let out = |f: &dyn Fn(&Console, &mut Vec<u8>)| {
            let mut v = Vec::new();
            f(&ui, &mut v);
            v
        };
        assert_eq!(
            out(&|u, v| u.checking_start(v, false).unwrap()),
            b"\r\xfe Checking input file... ".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.checking_completion(v, "ok!").unwrap()),
            b"ok!\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.writing_old_compression(v, 1).unwrap()),
            b"* Writing CSW v1.01 with old compression method\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.completed(v, 1.5).unwrap()),
            b"* Completed, conversion rate: 1.50 KBps.\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.fatal(v, b"Wrong file type").unwrap()),
            b"\rFATAL ERROR: Wrong file type\n\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.fatal_block(v, b"one\ntwo").unwrap()),
            b"FATAL ERROR: one\ntwo\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.error(v, b"nope").unwrap()),
            b"ERROR: nope\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.refused(v, "no").unwrap()),
            b"\nno\n\n".to_vec()
        );
        assert_eq!(
            out(&|u, v| u.warn_block(v, "hm").unwrap()),
            b"\nhm\n".to_vec()
        );
        assert_eq!(out(&|u, v| u.working(v).unwrap()), b"* Working...".to_vec());
        let packed = out(&|u, v| u.packed(v, 50, 2, 100).unwrap());
        assert!(
            packed.starts_with(
                b"\r* Packed size: 50 bytes (2 pulses), compression ratio: 50.00% (2:1)"
            )
        );
        assert!(packed.ends_with(b"\n\n"), "{packed:?}");
        // A recording has no original size: the ratio line reads -Inf% (0:1).
        let live = out(&|u, v| u.packed(v, 50, 2, 0).unwrap());
        assert!(
            live.starts_with(
                b"\r* Packed size: 50 bytes (2 pulses), compression ratio: -Inf% (0:1)"
            ),
            "{live:?}"
        );
        assert_eq!(
            out(&|u, v| u.describe(v, "x").unwrap()),
            b"\xfe x\n".to_vec()
        );
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn the_directmode_lines_carry_their_framing() {
        let ui = console();
        let mut v = Vec::new();
        ui.recording_cut_short(&mut v, "the disk filled").unwrap();
        assert_eq!(
            v,
            b"WARNING: the recording ended early: the disk filled; keeping what was captured\n"
                .to_vec()
        );
        v.clear();
        ui.transient(&mut v, PROMPT_START).unwrap();
        ui.transient(&mut v, ERASE_START).unwrap();
        assert_eq!(
            v,
            b"* Press any key to start conversion when done with volume meter...\
              \r\t\t\t\t\t\t\t\t\t\r"
                .to_vec()
        );
        v.clear();
        ui.transient(&mut v, ERASE_PAUSED).unwrap();
        ui.transient(&mut v, PROMPT_PAUSED).unwrap();
        ui.transient(&mut v, ERASE_PAUSED).unwrap();
        assert_eq!(
            v,
            b"\r\t\t\t\t\t\t\t\t\r* PAUSED, press any key to continue...\
              \r\t\t\t\t\t\t\t\t\r"
                .to_vec()
        );
        v.clear();
        ui.overrun(&mut v, 7).unwrap();
        assert_eq!(
            v,
            b"WARNING: DMA buffer OVERRUN!!! Lost 7 samples\n".to_vec()
        );
        v.clear();
        ui.capture_rate(&mut v, 45454, 44100).unwrap();
        assert_eq!(
            v,
            b"\xfe Sampling rate: 45454 Hz (rounded from: 44100 Hz).\n".to_vec()
        );
        v.clear();
        ui.capture_rate(&mut v, 8000, 8000).unwrap();
        assert_eq!(
            v,
            b"\xfe Sampling rate: 8000 Hz (rounded from: 8000 Hz).\n".to_vec()
        );
        v.clear();
        ui.recorded(&mut v, 64516, 2.0).unwrap();
        assert_eq!(v, b"\xfe Recorded 64516 samples in 00:02\n".to_vec());
        v.clear();
        ui.recorded(&mut v, 98304, 98304.0 / 32258.0).unwrap();
        assert_eq!(v, b"\xfe Recorded 98304 samples in 00:03\n".to_vec());
        v.clear();
        ui.recorded(&mut v, 116128800, 3600.0).unwrap();
        assert_eq!(v, b"\xfe Recorded 116128800 samples in 01:00:00\n".to_vec());
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn the_limit_truncates_where_the_closing_line_rounds() {
        let ui = console();
        let mut v = Vec::new();
        ui.max_recording_time(&mut v, 2.5, 80645).unwrap();
        assert_eq!(
            v,
            b"\xfe Max recording time is 00:02 (80645 bytes).\n".to_vec()
        );
        v.clear();
        ui.recorded(&mut v, 80645, 2.5).unwrap();
        assert_eq!(v, b"\xfe Recorded 80645 samples in 00:03\n".to_vec());
        v.clear();
        ui.max_recording_time(&mut v, 31.855, 1027605).unwrap();
        assert_eq!(
            v,
            b"\xfe Max recording time is 00:31 (1027605 bytes).\n".to_vec()
        );
    }

    #[test]
    fn banner_and_help_are_fixed_text() {
        let ui = console();
        let mut v = Vec::new();
        ui.banner(&mut v).unwrap();
        assert!(v.starts_with(b"\n-=[ CSW v2.00 ]=-"));
        v.clear();
        ui.help(&mut v, &FilterSpec::default()).unwrap();
        assert!(v.ends_with(b"MakeTZX's manual.\n"));
        assert!(!v.contains(&b'\xFE'));
        assert_eq!(
            BANNER,
            &b"\n-=[ CSW v2.00 ]=-  Ramsoft's CSW converter, recreated (GPL v2.0+).\n\n"[..]
        );
        assert!(HELP_HEAD.windows(2).any(|w| w == b"\n\n"));
        assert!(HELP_TAIL.ends_with(b"manual.\n"));
        let head = String::from_utf8_lossy(HELP_HEAD).into_owned();
        assert!(head.contains("Syntax : CSW [options] inputfile [outputfile]\n\n"));
        let tail = String::from_utf8_lossy(HELP_TAIL).into_owned();
        assert!(tail.contains("acceleration\n\nFor more info"));
    }

    /// The output is code page 437, which only a redirected stream can be
    /// asked to keep: a terminal reads UTF-8.
    #[test]
    fn bullet_follows_the_destination() {
        let mut redirected_out = Vec::new();
        console().describe(&mut redirected_out, "x").unwrap();
        assert_eq!(redirected_out[0], 0xFE);

        let mut terminal = Vec::new();
        Console::new(true).describe(&mut terminal, "x").unwrap();
        assert!(terminal.starts_with("\u{25A0}".as_bytes()), "{terminal:?}");
        assert!(!terminal.contains(&0xFE));
        // Only the bullet changes; the rest of the line is unchanged.
        assert!(terminal.ends_with(b" x\n"));
    }

    /// Full-scale amplitude in the 8-bit domain the meter is fed in.
    #[cfg(feature = "directmode")]
    const FULL_SCALE_UNITS: f32 = 127.0;

    /// The bracketed values in the help screen are the settings in force,
    /// not fixed text.
    #[test]
    fn help_shows_the_filter_settings_in_force() {
        let spec = FilterSpec {
            order: 4,
            high_hz: 300.0,
            ..FilterSpec::default()
        };
        let mut v = Vec::new();
        console().help(&mut v, &spec).unwrap();
        let s = String::from_utf8(v).unwrap();
        assert!(s.contains("order [4]"), "{s}");
        assert!(s.contains("[300"), "{s}");
        assert!(!s.contains("[4100"), "{s}");

        let unknown = FilterSpec {
            band: Band::Unknown(-1),
            ..FilterSpec::default()
        };
        let mut v = Vec::new();
        console().help(&mut v, &unknown).unwrap();
        let s = String::from_utf8(v).unwrap();
        assert!(s.contains("High-Pass) [-1]"), "{s}");
    }

    #[test]
    fn fixed_point_rounds_the_way_printf_does() {
        assert_eq!(fmt_fixed(600.5, 0), "601");
        assert_eq!(format!("{:.0}", 600.5), "600");
        assert_eq!(fmt_fixed(-0.3, 0), "0");
        assert_eq!(fmt_fixed(-0.7, 0), "-1");
        assert_eq!(fmt_fixed(600.125, 2), "600.13");
        assert_eq!(format!("{:.2}", 600.125), "600.12");
    }

    #[test]
    fn the_seconds_and_the_milliseconds_truncate_separately() {
        assert_eq!(play_ms(19996, 10000), "00:01.999");
    }

    #[test]
    fn a_rate_of_zero_prints_the_out_of_range_sentinel() {
        assert_eq!(play_ms(18180, 0), "-14:-8.-2147483648");
    }

    /// The help screen's numbers are `%g` with six significant digits.
    #[test]
    fn doubles_print_the_way_an_ostream_does() {
        assert_eq!(fmt_ostream(4100.0), "4100");
        assert_eq!(fmt_ostream(600.0), "600");
        assert_eq!(fmt_ostream(1.0), "1");
        assert_eq!(fmt_ostream(0.0), "0");
        assert_eq!(fmt_ostream(300.5), "300.5");
        assert_eq!(fmt_ostream(-2.5), "-2.5");
        assert_eq!(fmt_ostream(1234.5678), "1234.57");
        assert_eq!(fmt_ostream(1234567.0), "1.23457e+06");
        assert_eq!(fmt_ostream(0.00001), "1e-05");
        assert_eq!(fmt_ostream(0.0001), "0.0001");
    }

    /// Cells filled in `colour` on one meter line: the run between that colour
    /// and the blue the remainder is painted in.
    #[cfg(feature = "directmode")]
    fn filled(line: &str, colour: &[u8]) -> usize {
        let colour = std::str::from_utf8(colour).unwrap();
        line.split(colour)
            .nth(1)
            .and_then(|rest| rest.split('\u{1b}').next())
            .map_or(0, |run| run.chars().filter(|&c| c == '\u{25A0}').count())
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn the_meter_block_reanchors_after_every_newline() {
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        for (levels, layout) in [
            (Some((FULL_SCALE_UNITS / 2.0, 0.25)), (METER_CELLS, false)),
            (Some((FULL_SCALE_UNITS / 2.0, 0.25)), (METER_CELLS, true)),
            (None, (METER_CELLS, false)),
        ] {
            let mut v = Vec::new();
            console()
                .meter_at(&mut v, PROMPT_START, levels, Some((1.0, 48000)), layout)
                .unwrap();
            blocks.push(v);
        }
        let mut v = Vec::new();
        console().clear_transient(&mut v).unwrap();
        blocks.push(v);
        for bytes in blocks {
            let head = bytes.strip_prefix(HIDE_CURSOR).unwrap_or(&bytes);
            assert_eq!(
                head.first(),
                Some(&b'\r'),
                "block does not re-anchor before its first glyph"
            );
            for (i, pair) in bytes.windows(2).enumerate() {
                if pair[0] == b'\n' {
                    assert_eq!(pair[1], b'\r', "newline at {i} does not re-anchor");
                }
            }
        }
    }

    /// The meter is only ever drawn to a live terminal, so its bytes have to be
    /// printable there: raw CP437 cells arrive as replacement characters on a
    /// UTF-8 terminal.
    #[cfg(feature = "directmode")]
    #[test]
    fn meter_bars_are_printable_on_a_utf8_terminal() {
        let mut v = Vec::new();
        console()
            .meter_at(
                &mut v,
                "",
                Some((FULL_SCALE_UNITS / 2.0, 0.0)),
                None,
                (METER_CELLS, false),
            )
            .unwrap();
        let s = std::str::from_utf8(&v).expect("meter is UTF-8");
        assert!(s.contains('\u{25A0}'), "no cells: {s:?}");
        // Every cell is the same glyph; the colour is what distinguishes.
        let line = s.split('\n').nth(1).unwrap();
        assert!(filled(line, LEVEL_COLOUR) > 0, "no level cells: {line:?}");
        assert!(filled(line, EMPTY_COLOUR) > 0, "no empty cells: {line:?}");
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn the_layout_follows_the_window() {
        for (cols, expected) in [
            (None, (METER_CELLS, false)),
            (Some(0), (METER_CELLS, false)),
            (Some(1), (1, false)),
            (Some(79), (79, false)),
            (Some(80), (METER_CELLS, false)),
            (Some(92), (METER_CELLS, false)),
            (Some(93), (METER_CELLS, true)),
            (Some(200), (METER_CELLS, true)),
        ] {
            assert_eq!(layout_for(cols), expected, "at {cols:?} columns");
        }
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn a_wide_window_gets_the_readouts() {
        let mut v = Vec::new();
        console()
            .meter_at(
                &mut v,
                "",
                Some((FULL_SCALE_UNITS / 2.0, 0.25)),
                None,
                (METER_CELLS, true),
            )
            .unwrap();
        let wide = String::from_utf8(v).unwrap();
        assert!(wide.contains(" dB"), "no level readout: {wide:?}");
        assert!(wide.contains("% clip"), "no clipping readout: {wide:?}");
        let mut v = Vec::new();
        console()
            .meter_at(
                &mut v,
                "",
                Some((FULL_SCALE_UNITS / 2.0, 0.25)),
                None,
                (METER_CELLS, false),
            )
            .unwrap();
        let narrow = String::from_utf8(v).unwrap();
        assert!(!narrow.contains(" dB"), "readout on a narrow window");
    }

    /// The bar lengths: RMS times 0.625 cells, truncated, and the clipped
    /// share of 80 cells, also truncated.
    #[cfg(feature = "directmode")]
    #[test]
    fn bar_lengths_follow_the_meter_arithmetic() {
        let cells = |rms: f32, clipped: f32| {
            let mut v = Vec::new();
            console()
                .meter_at(&mut v, "", Some((rms, clipped)), None, (METER_CELLS, false))
                .unwrap();
            let s = String::from_utf8(v).unwrap();
            // line 1 is the prompt; the bars are lines 2 and 3.
            let mut lines = s.split('\n').skip(1);
            let level = filled(lines.next().unwrap_or(""), LEVEL_COLOUR);
            let clip = filled(lines.next().unwrap_or(""), CLIP_COLOUR);
            (level, clip)
        };
        // A full-scale square wave fills the row: 127 * 0.625 = 79 cells.
        assert_eq!(cells(FULL_SCALE_UNITS, 0.0).0, 79);
        // A full-scale sine is 127/sqrt(2) -> 56 cells.
        assert_eq!(cells(FULL_SCALE_UNITS / 2f32.sqrt(), 0.0).0, 56);
        assert_eq!(cells(0.0, 0.0), (0, 0));
        // Clipping: a quarter of the buffer railed is a quarter of the row.
        assert_eq!(cells(0.0, 0.25).1, 20);
        assert_eq!(cells(0.0, 1.0).1, METER_CELLS);
    }

    /// Blanked bars leave the block's height and its progress line alone;
    /// this is what is drawn while recording.
    #[cfg(feature = "directmode")]
    #[test]
    fn blank_bars_keep_the_block() {
        let mut v = Vec::new();
        console()
            .meter_at(&mut v, "", None, Some((1.0, 48000)), (METER_CELLS, false))
            .unwrap();
        let s = String::from_utf8(v).unwrap();
        assert!(!s.contains('\u{25A0}'), "bars drawn when blanked: {s:?}");
        // The block keeps its height, and still reports progress.
        assert_eq!(s.matches('\n').count(), METER_LINES - 1);
        assert!(s.contains("48000 samples"), "{s:?}");
    }

    /// The readout beside the level bar reads in dB against full scale.
    #[cfg(feature = "directmode")]
    #[test]
    fn level_readout_is_db_against_full_scale() {
        assert_eq!(rms_db_label(FULL_SCALE_UNITS), "0.0 dB");
        assert_eq!(rms_db_label(FULL_SCALE_UNITS / 2.0), "-6.0 dB");
        assert_eq!(rms_db_label(0.0), "-inf dB");
    }

    #[test]
    fn completed_line_closes_a_decode() {
        let mut v = Vec::new();
        console().completed(&mut v, 1.0).unwrap();
        assert!(v.starts_with(b"* Completed, conversion rate:"));
    }
}
