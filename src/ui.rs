//! Console output, fixed byte for byte: CP437 status bullets (0xFE), errors
//! on stdout. A line's prefix, its ending and whether a blank line follows it
//! are all part of the contract.

use std::io::{self, Write};

use crate::filter::{Band, Design, FilterSpec};

/// Status-line bullet: "■" in code page 437.
const BULLET_CP437: u8 = 0xFE;
/// The same glyph as [`BULLET_CP437`], encoded for a terminal that reads UTF-8.
const BULLET_CP437_AS_UTF8: &[u8] = "\u{25A0}".as_bytes();

/// The banner, byte for byte, copyright line included.
const BANNER: &[u8] = b"\n-=[ CSW v2.00 ]=-  (C) 1998-2003 Ramsoft, a ZX Spectrum demogroup.\n\n";

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
        // An input of no size divides by nothing, which prints as
        // "-Inf% (0:1)".
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
            &b"\n-=[ CSW v2.00 ]=-  (C) 1998-2003 Ramsoft, a ZX Spectrum demogroup.\n\n"[..]
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

    #[test]
    fn completed_line_closes_a_decode() {
        let mut v = Vec::new();
        console().completed(&mut v, 1.0).unwrap();
        assert!(v.starts_with(b"* Completed, conversion rate:"));
    }
}
