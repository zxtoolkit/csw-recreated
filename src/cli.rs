//! Command-line parsing (`csw [options] inputfile [outputfile]`).

use std::ffi::OsString;

use crate::error::{Error, Result};
use crate::filter::{Band, Design, FilterSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Encode an input sound file to CSW (default).
    Encode,
    /// Decode CSW back to a waveform (`-d`).
    Decode,
}

pub use ::csw::convert::DecodeTarget;

#[derive(Debug, Clone)]
pub struct Cli {
    /// The file names as argv spelled them: bytes, not text, opened and
    /// printed as they came (`as_path` in `main.rs` says what happens to a
    /// name this host cannot read as UTF-8).
    pub input: Vec<u8>,
    pub output: Option<Vec<u8>>,
    pub direction: Direction,
    pub decode_target: DecodeTarget,
    /// The DirectMode capture rate (`-s<n>`); a file input keeps its own.
    pub rate: Option<i64>,
    /// CSW container version to write: 2 (default) or 1 (`-1`).
    pub csw_version: u8,
    /// `-z`: plain RLE, the "old compression method", in place of Z-RLE. A
    /// v1 file (`-1`) is plain RLE regardless.
    pub old_compression: bool,
    /// `-i<n>`: a recording input selector (0 line, 1 aux, 2 mic), parsed and
    /// never read.
    pub input_source: Option<i32>,
    /// `Some(spec)` when `-f` was given.
    pub filter: Option<FilterSpec>,
    /// `-r`: record from the soundcard in place of an input file.
    pub direct: bool,
    /// `-t<secs>`: stop a DirectMode recording after this long.
    pub record_secs: Option<f64>,
    /// `-k`: also write the DirectMode samples to a WAV file.
    pub keep_samples: bool,
    /// `-c`: DirectMode, take the device's own configuration unchanged.
    pub device_compat: bool,
    pub help: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Cli {
            input: Vec::new(),
            output: None,
            direction: Direction::Encode,
            decode_target: DecodeTarget::Wav,
            rate: None,
            csw_version: 2,
            old_compression: false,
            input_source: None,
            filter: None,
            direct: false,
            record_secs: None,
            keep_samples: false,
            device_compat: false,
            help: false,
        }
    }
}

/// Parse argv (excluding the program name).
///
/// Argv is read as bytes, not as text: dispatch is on the byte after the `-`,
/// so a byte this host cannot read as a character is still a switch letter.
pub fn parse(args: &[OsString]) -> Result<Cli> {
    let mut cli = Cli::default();
    let mut positionals: Vec<Vec<u8>> = Vec::new();

    for arg in args {
        // A switch is `-` followed by a letter; `/` is not a switch prefix.
        if let Some(rest) = arg.as_encoded_bytes().strip_prefix(b"-") {
            parse_switch(rest, &mut cli)?;
        } else {
            positionals.push(arg.as_encoded_bytes().to_vec());
        }
        // `-?` ends the parse where it stands: `csw -? -fq` is the help
        // screen where `csw -fq -?` is "-fq is an invalid switch".
        if cli.help {
            break;
        }
    }

    if cli.help {
        return Ok(cli);
    }

    if cli.direct {
        match positionals.as_slice() {
            // The file-name count decides the help screen, DirectMode
            // included: `csw -r` prints help and records nothing.
            [] => cli.help = true,
            [name] => cli.input = name.clone(),
            // A second name takes the output's place, the first then naming
            // nothing at all; a third is the help screen, as ever.
            [inp, out] => {
                cli.input = inp.clone();
                cli.output = Some(out.clone());
            }
            [_, _, ..] => cli.help = true,
        }
        if cli.help {
            return Ok(cli);
        }
        return Ok(csw_input_implies_decode(cli));
    }
    match positionals.as_slice() {
        [] => cli.help = true,
        [inp] => cli.input = inp.clone(),
        [inp, out] => {
            cli.input = inp.clone();
            cli.output = Some(out.clone());
        }
        // A third filename shows the help screen.
        [inp, out, ..] => {
            cli.input = inp.clone();
            cli.output = Some(out.clone());
            cli.help = true;
        }
    }

    Ok(csw_input_implies_decode(cli))
}

fn csw_input_implies_decode(mut cli: Cli) -> Cli {
    if cli.input.to_ascii_lowercase().ends_with(b".csw") && cli.direction == Direction::Encode {
        cli.direction = Direction::Decode;
    }
    cli
}

/// The 128 bytes a switch letter with the high bit set is folded through.
///
/// A letter from `\x80` up indexes this table: `-\xd6` gives `r` and starts
/// DirectMode, `-\x85` gives `1` and writes a v1 file. 48 entries are NUL,
/// which ends a quoted token where it stands, so `-\xff` is reported as a
/// bare `-`. Seven -- `\xcf`, `\xe3`-`\xe5` and `\xf3`-`\xf5` -- fold to bytes
/// that are neither a switch letter nor a sub-option key, which a corpus test
/// pins.
const HIGH_FOLD: &[u8; 128] = b" 2.8.1 $\0@(#) DJGPP libc built Dec 24 2001 21:24:39 by gcc 2.8.1 \
$\0\0\0\0\0\0\0\0\0\0\0\0\0\x02\0\0\0 !proxy\0\x07\0\0\0\0\0\0\0\xf4\
\xd7\x11\0\0\0\0\0\0\0\0\0\0\0\0\0\x9c]\x03\0\0\0\0\0\0\0\0\0\0";

/// One switch or sub-option letter, folded: plain lower-casing below
/// `\x80`, and [`HIGH_FOLD`] at or above it.
fn fold(b: u8) -> u8 {
    match b.checked_sub(0x80) {
        Some(i) => HIGH_FOLD[i as usize],
        None => b.to_ascii_lowercase(),
    }
}

/// The token an unrecognised switch is quoted by: argv's own bytes with the
/// folded letter written back into them.
fn switch_token(head: &[u8], folded: u8, tail: &[u8]) -> Vec<u8> {
    let mut token = head.to_vec();
    if folded == 0 {
        return token;
    }
    token.push(folded);
    token.extend_from_slice(tail);
    token
}

fn invalid_switch(token: &[u8]) -> Error {
    let mut msg = token.to_vec();
    msg.extend_from_slice(b" is an invalid switch");
    Error::Fatal(msg.into())
}

fn parse_switch(rest: &[u8], cli: &mut Cli) -> Result<()> {
    // A bare `-` folds its own terminator, which is no switch: the same line
    // as any switch that is not recognised.
    let flag = fold(rest.first().copied().unwrap_or(0));
    let tail = rest.get(1..).unwrap_or_default();
    let arg = String::from_utf8_lossy(tail);
    // The invalid-switch line quotes the *whole* token, not just the letter
    // that failed to dispatch. Only the first letter is folded: `-Xyz` is
    // reported as `-xyz`.
    let token = switch_token(b"-", flag, tail);
    match flag {
        b'?' => cli.help = true,
        b'd' => {
            cli.direction = Direction::Decode;
            // One character after `d` selects the target, and anything
            // beyond it is ignored: `-dv` is VOC, while `-dx` and `-dwav`
            // are both WAV. An unknown target is not an error. That byte is
            // read where it lies and is never folded, so `-dV` is a WAV.
            cli.decode_target = match tail.first() {
                Some(b'v') => DecodeTarget::Voc,
                _ => DecodeTarget::Wav,
            };
        }
        b's' => {
            // `-s` sets the DirectMode capture rate only. Leading digits or
            // nothing, so `-s0` and `-sxyz` are accepted.
            if let Scan::Value(v) = scan_i(&arg) {
                cli.rate = Some(i64::from(v));
            }
        }
        b'r' => cli.direct = true,
        b'k' => cli.keep_samples = true,
        b'c' => cli.device_compat = true,
        b't' => {
            if let Scan::Value(v) = scan_lf(&arg) {
                cli.record_secs = Some(v);
            }
        }
        b'1' => cli.csw_version = 1,
        b'z' => cli.old_compression = true,
        b'i' => {
            if let Scan::Value(v) = scan_i(&arg) {
                cli.input_source = Some(v);
            }
        }
        b'f' => {
            // Sub-options accumulate across repeated switches: `-fo4 -fh300`
            // sets both. Within a single switch only the first is read.
            let mut spec = cli.filter.unwrap_or_default();
            // An unknown sub-option has no message of its own: it is reported
            // as an invalid switch, quoting the whole token, so `-fx` reads
            // `-fx is an invalid switch`.
            parse_filter_suboptions(tail, &mut spec)?;
            cli.filter = Some(spec);
        }
        _ => return Err(invalid_switch(&token)),
    }
    Ok(())
}

/// Parse one `-f` sub-option -- **the first only**: anything after its digits
/// is ignored, so `-fp2r3` is Chebyshev at the default ripple, and two
/// settings need two switches (`-fp2 -fr3`). The key is folded as the switch
/// letter is: `-FO4` is `-fo4`, and a key with the high bit set reads the
/// bytes in front of the fold table (see [`fold`]).
fn parse_filter_suboptions(tail: &[u8], spec: &mut FilterSpec) -> Result<()> {
    // `-f` alone, or a key folding to NUL, selects the filter and sets
    // nothing else.
    let key = fold(tail.first().copied().unwrap_or(0));
    if key == 0 {
        return Ok(());
    }
    let arg = tail.get(1..).unwrap_or_default();
    let token = switch_token(b"-f", key, arg);
    let token = token.as_slice();
    let owned = String::from_utf8_lossy(arg);
    let rest = owned.as_ref();
    // Each key hands the rest of the token to `sscanf` (`%i` or `%lf`): a
    // value is stored, an empty tail keeps the setting, and text that
    // converts to nothing is fatal. So `-fo0x4` is order 4 and `-fh-5` a
    // cutoff of -5 Hz; nothing is validated after that.
    match key {
        b'o' => match scan_i(rest) {
            Scan::Value(v) => spec.order = v,
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        b'h' => match scan_lf(rest) {
            Scan::Value(v) => spec.high_hz = v,
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        b'l' => match scan_lf(rest) {
            Scan::Value(v) => spec.low_hz = v,
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        b'r' => match scan_lf(rest) {
            Scan::Value(v) => spec.ripple_db = v,
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        // 3, 4 and 5 are the bands. Any other number is carried as given:
        // it prints as "??? " on the filter line and designs nothing.
        b't' => match scan_i(rest) {
            Scan::Value(3) => spec.band = Band::LowPass,
            Scan::Value(4) => spec.band = Band::BandPass,
            Scan::Value(5) => spec.band = Band::HighPass,
            Scan::Value(v) => spec.band = Band::Unknown(v),
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        // 1 is Butterworth and 2 Chebyshev. Anything else is carried as
        // given: the status line names it "Chebyshev" with no ripple clause,
        // and neither prototype is used (see `Design::Other`).
        b'p' => match scan_i(rest) {
            Scan::Value(1) => spec.design = Design::Butterworth,
            Scan::Value(2) => spec.design = Design::Chebyshev,
            Scan::Value(v) => spec.design = Design::Other(v),
            Scan::Eof => {}
            Scan::Fail => return Err(suboption_fail()),
        },
        // Accepted and ignored.
        b'3' => {}
        _ => return Err(invalid_switch(token)),
    }
    Ok(())
}

/// The fatal error a `-f` sub-option whose argument converts to nothing
/// raises: the line for a bad sampling rate, whichever sub-option it was --
/// `-fhxyz`, `-foxyz` and `-frxyz` all say "Invalid sample rate".
fn suboption_fail() -> Error {
    Error::Fatal("Invalid sample rate".into())
}

/// What `sscanf` makes of a sub-option's argument: nothing to read at all
/// (the tail is empty -- EOF), a value, or text it cannot convert.
enum Scan<T> {
    Eof,
    Value(T),
    Fail,
}

/// `sscanf("%i")`: optional whitespace and sign, then hexadecimal after
/// `0x`, octal after a leading `0`, decimal otherwise. Stops at the first
/// character that does not belong, so `-fo1.5` is order 1.
fn scan_i(tail: &str) -> Scan<i32> {
    let s = tail.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if s.is_empty() {
        return Scan::Eof;
    }
    let (neg, s) = match s.as_bytes()[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let b = s.as_bytes();
    let (radix, digits): (u32, &str) = if b.len() > 2
        && (b[0] == b'0')
        && (b[1] == b'x' || b[1] == b'X')
        && b[2].is_ascii_hexdigit()
    {
        (16, &s[2..])
    } else if b.first() == Some(&b'0') {
        (8, s)
    } else {
        (10, s)
    };
    let run: String = digits.chars().take_while(|c| c.is_digit(radix)).collect();
    if run.is_empty() {
        return Scan::Fail;
    }
    // Stored into an `int`: the magnitude wraps at 2^32 and the sign is
    // applied after, so `-fo4294967297` is order 1 and `-fo2147483648` is
    // -2147483648.
    let v = run.chars().fold(0u32, |acc, c| {
        acc.wrapping_mul(radix)
            .wrapping_add(c.to_digit(radix).unwrap())
    }) as i32;
    Scan::Value(if neg { v.wrapping_neg() } else { v })
}

/// `sscanf("%lf")`: optional whitespace and sign, digits with an optional
/// fraction and exponent. `-fh1e3` is 1000 Hz and `-fh.5` half a hertz;
/// `-fhinf` converts to nothing: `inf` is not accepted.
fn scan_lf(tail: &str) -> Scan<f64> {
    let s = tail.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if s.is_empty() {
        return Scan::Eof;
    }
    let (neg, s) = match s.as_bytes()[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let mag = {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let int_digits = i;
        let mut frac_digits = 0;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
                frac_digits += 1;
            }
        }
        if int_digits + frac_digits == 0 {
            return Scan::Fail;
        }
        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            let mut j = i + 1;
            if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_digit() {
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        match s[..i].parse::<f64>() {
            Ok(v) => v,
            Err(_) => return Scan::Fail,
        }
    };
    Scan::Value(if neg { -mag } else { mag })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Cli> {
        let owned: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse(&owned)
    }

    #[test]
    fn high_bit_switch_letters_fold_through_the_table() {
        assert_eq!(fold(0xd6), b'r');
        assert_eq!(fold(0x85), b'1');
        assert_eq!(fold(0xff), 0);
    }

    #[test]
    fn a_sub_option_argument_that_converts_to_nothing_is_an_invalid_rate() {
        for arg in ["-fhxyz", "-foxyz", "-frxyz", "-fhinf"] {
            let err = parse_args(&[arg, "x"]).unwrap_err();
            assert!(
                matches!(&err, Error::Fatal(m) if m.as_bytes() == b"Invalid sample rate"),
                "{arg}: {err:?}"
            );
        }
    }

    /// Each `%i` argument is stored into a 32-bit int: the magnitude wraps at
    /// 2^32 and the sign is applied after.
    #[test]
    fn sub_option_numbers_are_32_bit_ints() {
        let order = |a: &str| parse_args(&[a, "x"]).unwrap().filter.unwrap().order;
        assert_eq!(order("-fo-1"), -1);
        assert_eq!(order("-fo2147483648"), i32::MIN);
        assert_eq!(order("-fo4294967297"), 1);
        let band = |a: &str| parse_args(&[a, "x"]).unwrap().filter.unwrap().band;
        assert_eq!(band("-ft4294967300"), Band::BandPass);
        assert_eq!(band("-ft-4294967292"), Band::BandPass);
        assert_eq!(parse_args(&["-s4294967297", "x"]).unwrap().rate, Some(1));
    }

    #[test]
    fn t_reads_its_argument_as_sscanf_f_does() {
        let secs = |a: &str| parse_args(&["-r", a, "x"]).unwrap().record_secs;
        assert_eq!(secs("-t1e1"), Some(10.0));
        assert_eq!(secs("-t+2"), Some(2.0));
        assert_eq!(secs("-t1.2.3"), Some(1.2));
        assert_eq!(secs("-t-3"), Some(-3.0));
        assert_eq!(secs("-tzzz"), None);
        assert_eq!(secs("-t"), None);
    }

    #[test]
    fn help_ends_the_parse_where_it_stands() {
        let cli = parse_args(&["-?", "-fq"]).unwrap();
        assert!(cli.help);

        let cli = parse_args(&["-fo6", "-?"]).unwrap();
        assert!(cli.help);
        assert_eq!(cli.filter.unwrap().order, 6);

        let err = parse_args(&["-fq", "-?"]).unwrap_err();
        assert!(
            format!("{err}").contains("-fq is an invalid switch"),
            "{err}"
        );
    }

    #[test]
    fn the_decode_target_letter_is_not_folded() {
        let cli = parse_args(&["-dv", "in.csw", "out"]).unwrap();
        assert_eq!(cli.direction, Direction::Decode);
        assert_eq!(cli.decode_target, DecodeTarget::Voc);

        let cli = parse_args(&["-dV", "in.csw", "out"]).unwrap();
        assert_eq!(cli.direction, Direction::Decode);
        assert_eq!(cli.decode_target, DecodeTarget::Wav);

        let cli = parse_args(&["-Dv", "in.csw", "out"]).unwrap();
        assert_eq!(cli.decode_target, DecodeTarget::Voc);
    }
}
