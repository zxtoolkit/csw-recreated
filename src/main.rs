//! csw -- a converter between sampled tape audio (WAV/VOC/IFF/OUT) and the
//! CSW (Compressed Square Wave) tape-image format.

#[cfg(feature = "directmode")]
mod audio;
mod cli;
#[cfg(feature = "directmode")]
mod record;
#[cfg(feature = "directmode")]
mod spool;
#[cfg(feature = "directmode")]
mod term;
mod ui;

use std::io::{self, IsTerminal, Write};

use ::csw::convert::{self, DecodeTarget, Report, Settings};
#[cfg(feature = "directmode")]
use ::csw::detect;
#[cfg(feature = "directmode")]
use ::csw::encode::PulseEncoder;
#[cfg(feature = "directmode")]
use ::csw::encode::Rewindable;
#[cfg(feature = "directmode")]
use ::csw::wav;
use ::csw::{container, error, filter};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use cli::Direction;
use error::{Error, Result};
use filter::FilterSpec;
use ui::Console;

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let ui = Console::new(io::stdout().is_terminal());
    let stdout = io::stdout();
    let mut w = stdout.lock();
    let _ = ui.banner(&mut w);

    let code = match run(&args, &ui, &mut w) {
        Ok(code) => code,
        Err(ref e @ Error::Rejected(ref msg, _)) => {
            // The refusal lands on the "Checking input file..." line, which
            // is already open; this completes it in place of "ok!".
            let _ = ui.checking_completion(&mut w, msg);
            ExitCode::from(exit_code(e))
        }
        // The checking line's completion, then a fatal line under it.
        Err(ref e @ Error::CheckedFatal(ref completion, ref msg)) => {
            let _ = ui.checking_completion(&mut w, completion);
            let _ = ui.fatal(&mut w, msg.as_bytes());
            ExitCode::from(exit_code(e))
        }
        // Several lines, printed flat: the useless-OUT message.
        Err(ref e @ Error::FatalBlock(ref msg)) => {
            let _ = ui.fatal_block(&mut w, msg.as_bytes());
            ExitCode::from(exit_code(e))
        }
        Err(ref e @ Error::Refused(ref msg)) => {
            // A bare line, no prefix: how a VOC that cannot be read is
            // turned down, there being no checking line open to complete.
            let _ = ui.refused(&mut w, msg);
            ExitCode::from(exit_code(e))
        }
        // Nothing is printed for these: banner, then the run is abandoned
        // and the output file left empty. The exit code carries the failure.
        Err(ref e @ Error::Silent(_)) => ExitCode::from(exit_code(e)),
        Err(e) => {
            // Errors go to stdout with everything else; the exit code says a
            // run failed.
            let msg = fatal_message(&e);
            let _ = if is_input(&e) {
                ui.error(&mut w, &msg)
            } else {
                ui.fatal(&mut w, &msg)
            };
            ExitCode::from(exit_code(&e))
        }
    };
    let _ = w.flush();
    code
}

/// Run one command line, returning the process exit code: 0 on success, and
/// **1** for the help screen however it was reached (`-?`, no arguments, or
/// more than five).
fn run(args: &[std::ffi::OsString], ui: &Console, w: &mut impl Write) -> Result<ExitCode> {
    // More than five arguments shows the help screen, whatever they are, and
    // before anything is parsed: `csw in out -fp2 -fo4 -fr3 -z` is help with
    // the built-in defaults in its brackets.
    if args.len() > 5 {
        ui.help(w, &FilterSpec::default())?;
        return Ok(ExitCode::from(1));
    }
    let cli = cli::parse(args)?;
    if cli.help {
        // The bracketed filter defaults track the command line: `csw -fo4
        // -fh300` shows order 4 and 300 Hz.
        ui.help(w, &cli.filter.unwrap_or_default())?;
        return Ok(ExitCode::from(1));
    }
    match cli.direction {
        Direction::Decode => decode(&cli, ui, w),
        Direction::Encode => encode(&cli, ui, w),
    }?;
    Ok(ExitCode::SUCCESS)
}

/// The name actually opened for an encode: the name as given when it carries
/// a `.`, or the first of `<name>.voc`, `.wav`, `.iff`, `.out` that exists
/// when it does not. The extensions are tried lower case only.
///
/// A dotless name is therefore never opened as it stands, so the open fails
/// with "Could not open input file" -- even where a file of exactly the
/// dotless name is sitting there.
fn resolve_input_name(name: &[u8]) -> Vec<u8> {
    if name.contains(&b'.') {
        return name.to_vec();
    }
    for ext in [b"voc", b"wav", b"iff", b"out"] {
        let candidate = joined(name, b".", ext);
        if as_path(&candidate).is_file() {
            return candidate;
        }
    }
    // Nothing found: a name that cannot open.
    joined(name, b".", b"out")
}

/// A name as argv spelled it, as a path. On Unix the bytes go straight
/// through, this host's file names being bytes too; elsewhere they are not,
/// and the name is read as text.
#[cfg(unix)]
fn as_path(name: &[u8]) -> std::borrow::Cow<'_, Path> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(Path::new(std::ffi::OsStr::from_bytes(name)))
}

#[cfg(not(unix))]
fn as_path(name: &[u8]) -> std::borrow::Cow<'_, Path> {
    std::borrow::Cow::Owned(std::path::PathBuf::from(
        String::from_utf8_lossy(name).into_owned(),
    ))
}

/// Three pieces of a name into one.
fn joined(a: &[u8], b: &[u8], c: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len() + b.len() + c.len());
    out.extend_from_slice(a);
    out.extend_from_slice(b);
    out.extend_from_slice(c);
    out
}

/// Open an input for streaming. The failure names no file: after the
/// extension search above, the name that failed to open is not necessarily
/// the name the user typed.
fn open_input(path: &[u8]) -> Result<std::io::BufReader<std::fs::File>> {
    let cannot_open = || Error::Fatal("Could not open input file".into());
    let file = std::fs::File::open(as_path(path)).map_err(|_| cannot_open())?;
    // A directory named as the input gets the missing-file line. Unix opens a
    // directory and fails at the first read, so it is checked here, before
    // the "Checking input file... " line.
    if file.metadata().map(|m| m.is_dir()).unwrap_or(false) {
        return Err(cannot_open());
    }
    Ok(std::io::BufReader::with_capacity(1 << 16, file))
}

/// Create (and so truncate) an output file, returning its handle.
fn create_output(path: &[u8]) -> Result<std::fs::File> {
    std::fs::File::create(as_path(path))
        .map_err(|_| Error::Fatal("Could not create output file".into()))
}

fn write_output(path: &[u8], bytes: &[u8]) -> Result<()> {
    std::fs::write(as_path(path), bytes)
        .map_err(|_| Error::Fatal("Could not create output file".into()))
}

const DIRECT_DECODE_INPUT: &[u8] = b"csw00000.raw";

fn keep_file_name(out: &[u8]) -> Vec<u8> {
    let stem = if out.len() >= 4 {
        &out[..out.len() - 4]
    } else {
        out
    };
    joined(stem, b".wav", b"")
}

/// Create the `-k` keep-file, empty. It is created through the *input* file
/// record, so a keep-file that cannot be created is reported as an input that
/// could not be opened.
fn create_keep_file(path: &[u8]) -> Result<()> {
    std::fs::write(as_path(path), b"").map_err(|_| Error::Fatal("Could not open input file".into()))
}

fn encode(cli: &cli::Cli, ui: &Console, w: &mut impl Write) -> Result<()> {
    if cli.direct {
        return encode_direct(cli, ui, w);
    }
    // A name with no extension is never opened as it stands -- see
    // `resolve_input_name`, which appends each known extension in turn.
    let input = resolve_input_name(&cli.input);
    let mut file = open_input(&input)?;
    // The output file is created -- and so truncated -- as soon as the input
    // opens, before a byte of it is parsed: a refused input leaves a
    // pre-existing output file empty, an input that cannot be opened leaves it
    // untouched, and `csw file file` truncates the input under the reader's
    // own handle and fails as "Wrong file type".
    let out = output_name(cli.output.as_deref(), &cli.input, "csw");
    let mut out_file = create_output(&out)?;
    // `-k` on a file conversion is the DirectMode keep-file reached by a
    // command line that does no recording: the file is created here, empty,
    // and then read *as* the input. An empty file carries no signature, so the
    // conversion ends before the "Checking input file" line is opened,
    // whatever the input held -- and `csw tape.wav tape.csw -k` empties
    // tape.wav, the keep-file's name being the input's.
    if cli.keep_samples {
        create_keep_file(&keep_file_name(&out))?;
        return Err(Error::Fatal("Wrong file type".into()));
    }
    let mut lines = Lines::new(ui, w);
    convert::encode(
        &mut file,
        &input,
        &cli.input,
        &settings(cli),
        &mut lines,
        &mut out_file,
    )
}

/// The conversion's switches, from the command line.
fn settings(cli: &cli::Cli) -> Settings {
    Settings {
        filter: cli.filter,
        csw_version: cli.csw_version,
        old_compression: cli.old_compression,
    }
}

/// The console lines of a conversion, written by [`Console`] to `w`.
struct Lines<'a, W: Write> {
    ui: &'a Console,
    w: &'a mut W,
    started: Instant,
}

impl<'a, W: Write> Lines<'a, W> {
    fn new(ui: &'a Console, w: &'a mut W) -> Self {
        Lines {
            ui,
            w,
            started: Instant::now(),
        }
    }
}

impl<W: Write> Report for Lines<'_, W> {
    fn checking_start(&mut self, integrity: bool) -> Result<()> {
        Ok(self.ui.checking_start(self.w, integrity)?)
    }
    fn checking_completion(&mut self, completion: &str) -> Result<()> {
        Ok(self.ui.checking_completion(self.w, completion)?)
    }
    fn describe(&mut self, desc: &str) -> Result<()> {
        Ok(self.ui.describe(self.w, desc)?)
    }
    fn sampling_rate(&mut self, rate: i64, samples: u64) -> Result<()> {
        Ok(self.ui.sampling_rate(self.w, rate, samples)?)
    }
    fn warn_block(&mut self, msg: &str) -> Result<()> {
        Ok(self.ui.warn_block(self.w, msg)?)
    }
    fn digital_filter(&mut self, spec: &FilterSpec) -> Result<()> {
        Ok(self.ui.digital_filter(self.w, spec)?)
    }
    fn writing_old_compression(&mut self, version: u8) -> Result<()> {
        Ok(self.ui.writing_old_compression(self.w, version)?)
    }
    fn working(&mut self) -> Result<()> {
        Ok(self.ui.working(self.w)?)
    }
    fn packed(&mut self, packed_bytes: u64, pulses: usize, orig_bytes: u64) -> Result<()> {
        Ok(self.ui.packed(self.w, packed_bytes, pulses, orig_bytes)?)
    }
    fn csw_header(&mut self, major: u8, minor: u8, rate: u32, file: &[u8]) -> Result<()> {
        Ok(self.ui.csw_header(self.w, major, minor, rate, file)?)
    }
    fn csw_app(&mut self, app: &[u8], comp: u8) -> Result<()> {
        Ok(self.ui.csw_app(self.w, app, comp)?)
    }
    fn total_samples(&mut self, samples: u64, rate: u32) -> Result<()> {
        Ok(self.ui.total_samples(self.w, samples, rate)?)
    }
    fn conversion_starts(&mut self) -> Result<()> {
        self.started = Instant::now();
        Ok(())
    }
    fn writing(&mut self, kind: &str, file: &[u8]) -> Result<()> {
        Ok(self.ui.writing(self.w, kind, file)?)
    }
    /// The wall-clock conversion-rate line.
    fn completed(&mut self, written: u64) -> Result<()> {
        let elapsed = self.started.elapsed().as_secs_f64().max(1e-6);
        Ok(self
            .ui
            .completed(self.w, written as f64 / 1000.0 / elapsed)?)
    }
}

/// The "FATAL ERROR: " / "ERROR: " message body, as the bytes the console
/// writes.
fn fatal_message(e: &Error) -> Vec<u8> {
    match e {
        Error::Io(e) => e.to_string().into_bytes(),
        Error::Format(m)
        | Error::Unsupported(m)
        | Error::Refused(m)
        | Error::Silent(m)
        | Error::CheckedFatal(_, m)
        | Error::Rejected(m, _) => m.clone().into_bytes(),
        Error::Fatal(m) | Error::FatalBlock(m) | Error::Input(m, _) => m.as_bytes().to_vec(),
    }
}

/// Whether this renders under the "ERROR: " prefix, not "FATAL ERROR: ".
fn is_input(e: &Error) -> bool {
    matches!(e, Error::Input(_, _))
}

/// The process exit code for a failure.
///
/// A refusal from a reader carries an error number `n` of its own and
/// exits `256 - n`:
///
/// | failure | n | exit |
/// |---|---|---|
/// | a truncated CSW, abandoned in silence | 1 | 255 |
/// | a CSW header declaring a sampling rate of 0 | 1 | 255 |
/// | an input that is not a CSW at all | 2 | 254 |
/// | a CSW version past 2.99 | 4 | 252 |
/// | a CSW compression type that is not known | 5 | 251 |
/// | compressed WAV or IFF sample data | 10 | 246 |
/// | stereo WAV | 11 | 245 |
/// | a WAV bit depth other than 8 | 12 | 244 |
///
/// Every other failure, and the help screen, exits 1.
fn exit_code(e: &Error) -> u8 {
    match e {
        Error::Silent(_) => 255,
        Error::Input(_, code) | Error::Rejected(_, code) => *code,
        _ => 1,
    }
}

#[cfg(feature = "directmode")]
fn direct_names(cli: &cli::Cli) -> (Vec<u8>, Vec<u8>, std::path::PathBuf) {
    let out = output_name(cli.output.as_deref(), &cli.input, "csw");
    let keep = keep_file_name(&out);
    let dir = as_path(&out)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| std::path::PathBuf::from("."), Path::to_path_buf);
    (out, keep, dir)
}

/// DirectMode (`-r`): record from the soundcard, then encode what was
/// captured through the same filter/binarise/write path a file takes.
#[cfg(feature = "directmode")]
fn encode_direct(cli: &cli::Cli, ui: &Console, w: &mut impl Write) -> Result<()> {
    let spec = record::DirectSpec {
        rate: cli.rate,
        seconds: cli.record_secs,
        device_compat: cli.device_compat,
        keep_samples: cli.keep_samples,
    };
    let (out, keep, dir) = direct_names(cli);

    if let Some(filter) = &cli.filter {
        ui.digital_filter(w, filter)?;
    }
    let mut capture = record::capture(&spec, ui, w, &dir)?;
    if capture.samples.is_empty() {
        return Err(Error::Fatal("Nothing was recorded".into()));
    }

    // `-k`: keep the raw samples too.
    if cli.keep_samples {
        write_keep_wav(&keep, capture.rate, &mut capture.samples)?;
        ui.keeping(w, &keep)?;
    }

    // No original file: the ratio line divides by nothing (see `packed`).
    let orig_bytes = 0;
    // Straight from the spool into the encoder: the recording is never held in
    // memory, at any length, and the spool is what the detector re-reads.
    let settings = settings(cli);
    let mut enc = PulseEncoder::new(
        capture.rate,
        f64::from(capture.rate),
        record::MIDPOINT,
        cli.filter,
        convert::zrle_output(&settings),
        false,
    );
    enc.run(&mut capture.samples)?;
    let sig = enc.finish();
    let mut lines = Lines::new(ui, w);
    convert::writing_and_working(&settings, &mut lines)?;
    let mut out_file = create_output(&out)?;
    convert::write_csw(&sig, orig_bytes, &settings, &mut lines, &mut out_file)
}

#[cfg(not(feature = "directmode"))]
fn encode_direct(_cli: &cli::Cli, _ui: &Console, _w: &mut impl Write) -> Result<()> {
    Err(Error::Unsupported(
        "this build has DirectMode disabled (built without the 'record' or 'record-alsa' feature)"
            .into(),
    ))
}

/// Write the `-k` keep-file straight from the spool: header first, then the
/// samples quantised to 8 bits a chunk at a time, so a long recording is never
/// held in memory to be written.
#[cfg(feature = "directmode")]
fn write_keep_wav(path: &[u8], rate: u32, samples: &mut spool::SpoolReader) -> Result<()> {
    use std::io::Write as _;

    let file = std::fs::File::create(as_path(path))
        .map_err(|_| Error::Fatal("Could not create output file".into()))?;
    let mut out = std::io::BufWriter::new(file);
    let write = |out: &mut std::io::BufWriter<std::fs::File>, bytes: &[u8]| {
        out.write_all(bytes)
            .map_err(|_| Error::Fatal("Could not create output file".into()))
    };
    let data_len = wav::wav_data_len(samples.len())?;
    write(&mut out, &wav::wav_header(rate, 1, 8, data_len))?;
    let mut chunk = Vec::new();
    while samples.next_chunk(&mut chunk)? {
        write(&mut out, &detect::to_byte_domain(&chunk, record::MIDPOINT))?;
    }
    out.flush()
        .map_err(|_| Error::Fatal("Could not create output file".into()))
}

/// DirectMode's source is the spool.
#[cfg(feature = "directmode")]
impl Rewindable for spool::SpoolReader {
    fn rewind(&mut self) -> Result<()> {
        spool::SpoolReader::rewind(self)
    }
    fn next_chunk(&mut self, out: &mut Vec<f64>) -> Result<bool> {
        spool::SpoolReader::next_chunk(self, out)
    }
}

/// The name a decode opens first: a dotless name with ".csw" appended, and
/// any other name as typed. The bare file behind a dotless name is never
/// tried -- `csw -d X` with a CSW named plain `X` is "Could not open input
/// file" -- as on the encode side (`resolve_input_name`).
fn first_csw_candidate(name: &[u8]) -> Vec<u8> {
    if name.contains(&b'.') {
        name.to_vec()
    } else {
        joined(name, b".csw", b"")
    }
}

/// The not-found message formats the *mutated* buffer: `csw -d out.wav`
/// reports "out.wav.csw". That line is for a file that is not a CSW at all;
/// one carrying the signature reports what is wrong with it.
fn read_csw_input(name: &[u8]) -> Result<(Vec<u8>, container::CswInfo, Vec<u8>)> {
    let first = first_csw_candidate(name);
    // A file carrying the signature is a CSW with something wrong inside,
    // and that fault is reported ("CSW compression type #9 not supported");
    // only something that is not a CSW at all falls through to the
    // not-found line.
    let mut malformed: Option<Error> = None;
    let mut fault = |raw: &[u8], e: Error| {
        if raw.starts_with(container::SIGNATURE) {
            malformed.get_or_insert(e);
        }
    };
    if let Ok(raw) = std::fs::read(as_path(&first)) {
        match container::read_header(&raw) {
            Ok(info) => return Ok((raw, info, first)),
            Err(e) => fault(&raw, e),
        }
    }
    let second = joined(&first, b".csw", b"");
    if let Ok(raw) = std::fs::read(as_path(&second)) {
        match container::read_header(&raw) {
            Ok(info) => return Ok((raw, info, second)),
            Err(e) => fault(&raw, e),
        }
    }
    match malformed {
        Some(e) => Err(e),
        // Not a CSW: exit code 254 (see `exit_code`).
        None => Err(Error::Input(
            joined(
                b"Input file '",
                &second,
                b"' not found or invalid file type.",
            )
            .into(),
            254,
        )),
    }
}

/// Where a decode writes: the name given, with the target's extension forced
/// onto it, or the input's name *as typed* with its extension swapped. The
/// name is derived before the `.csw` retry runs, so `csw -d t.abc` reading
/// `t.abc.csw` writes `t.wav`.
fn decode_output_name(cli: &cli::Cli, input: &[u8]) -> Vec<u8> {
    let ext = match cli.decode_target {
        DecodeTarget::Wav => "wav",
        DecodeTarget::Voc => "voc",
    };
    output_name(cli.output.as_deref(), input, ext)
}

fn decode(cli: &cli::Cli, ui: &Console, w: &mut impl Write) -> Result<()> {
    // The output is created -- and so truncated -- as soon as the input
    // opens, before the input is parsed, on this path as on the encode path:
    // a CSW that is turned down still leaves a pre-existing output file
    // empty, while an input that cannot be opened leaves it untouched.
    let mut input = cli.input.clone();
    let out = decode_output_name(cli, &cli.input);
    let mut out_file = if cli.direct {
        let out_file = create_output(&out)?;
        write_output(DIRECT_DECODE_INPUT, b"")?;
        input = if cli.keep_samples {
            let keep = keep_file_name(&out);
            create_keep_file(&keep)?;
            keep
        } else {
            DIRECT_DECODE_INPUT.to_vec()
        };
        out_file
    } else {
        open_input(&first_csw_candidate(&cli.input))?;
        let out_file = create_output(&out)?;
        // `-k` points the reader at the keep-file it has just created
        // empty (see `keep_file_name`), so what a decode reports missing
        // is that name and never the input's: `csw -k -d in.csw out.wav`
        // answers "Input file 'out.wav.csw' not found".
        if cli.keep_samples {
            input = keep_file_name(&out);
            create_keep_file(&input)?;
        }
        out_file
    };
    let (raw, info, path) = read_csw_input(&input)?;
    let mut lines = Lines::new(ui, w);
    convert::decode(
        &raw,
        &info,
        &path,
        cli.decode_target,
        &out,
        &mut lines,
        &mut out_file,
    )?;
    Ok(())
}

/// Default output naming: what stands before the input's **first** dot, plus
/// `ext` in lower case.
///
/// The cut is over the whole path, so an input named a.b/w8.wav derives
/// `a.csw`, beside the directory. The stem keeps the input's case, the
/// extension is always lower-case, and the console shows both (`CV1.CSW` →
/// `Writing WAV file 'CV1.wav'`).
fn swap_ext(path: &[u8], ext: &str) -> Vec<u8> {
    let stem = path.split(|&b| b == b'.').next().unwrap_or(path);
    joined(stem, b".", ext.as_bytes())
}

fn ensure_ext(path: &[u8], ext: &str) -> Vec<u8> {
    if path.contains(&b'.') {
        path.to_vec()
    } else {
        joined(path, b".", ext.as_bytes())
    }
}

fn output_name(given: Option<&[u8]>, derive_from: &[u8], ext: &str) -> Vec<u8> {
    given.map_or_else(|| swap_ext(derive_from, ext), |o| ensure_ext(o, ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use container::Compression;

    fn decode_cli(output: Option<&str>, target: DecodeTarget) -> cli::Cli {
        cli::Cli {
            direction: cli::Direction::Decode,
            decode_target: target,
            output: output.map(|o| o.as_bytes().to_vec()),
            ..cli::Cli::default()
        }
    }

    /// A name read back as text, every name here being text.
    fn name(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).expect("the names here are text")
    }

    #[test]
    fn the_dotless_search_takes_the_first_twin_it_finds() {
        let dir = std::env::temp_dir().join(format!("csw-stem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let at = |n: &str| dir.join(n);
        let resolve = |stem: &str| {
            let full = at(stem);
            name(resolve_input_name(full.to_str().unwrap().as_bytes()))
        };
        let stem = |n: &str| name(at(n).to_str().unwrap().as_bytes().to_vec());

        assert_eq!(resolve("T"), stem("T.out"));

        std::fs::write(at("T.out"), b"x").unwrap();
        assert_eq!(resolve("T"), stem("T.out"));
        std::fs::write(at("T.iff"), b"x").unwrap();
        assert_eq!(resolve("T"), stem("T.iff"));
        std::fs::write(at("T.wav"), b"x").unwrap();
        assert_eq!(resolve("T"), stem("T.wav"));
        std::fs::write(at("T.voc"), b"x").unwrap();
        assert_eq!(resolve("T"), stem("T.voc"));

        std::fs::write(at("UP.VOC"), b"x").unwrap();
        let found = resolve("UP");
        assert!(
            found == stem("UP.voc") || found == stem("UP.out"),
            "upper-case twin came back as {found}"
        );

        assert_eq!(resolve("T.dat"), stem("T.dat"));
        std::fs::write(at("N.csw"), b"x").unwrap();
        assert_eq!(resolve("N"), stem("N.out"));

        let dotted = at("A.B");
        std::fs::create_dir_all(&dotted).unwrap();
        std::fs::write(dotted.join("S.voc"), b"x").unwrap();
        let under = dotted.join("S");
        let under = under.to_str().unwrap();
        assert_eq!(name(resolve_input_name(under.as_bytes())), under);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_keep_file_is_the_output_name_less_four_bytes_and_wav() {
        assert_eq!(name(keep_file_name(b"tape.csw")), "tape.wav");
        assert_eq!(name(keep_file_name(b"t.cs")), ".wav");
        assert_eq!(name(keep_file_name(b"QQ.Z")), ".wav");
        assert_eq!(name(keep_file_name(b"O4.WA")), "O.wav");
        assert_eq!(name(keep_file_name(b"Q.Z")), "Q.Z.wav");
        assert_eq!(name(keep_file_name(b"Q.")), "Q..wav");
        assert_eq!(name(keep_file_name(b".Z")), ".Z.wav");
    }

    #[test]
    fn a_decode_output_name_comes_from_the_name_as_typed() {
        let wav = decode_cli(None, DecodeTarget::Wav);
        assert_eq!(name(decode_output_name(&wav, b"t.abc")), "t.wav");
        assert_eq!(name(decode_output_name(&wav, b"CV1.CSW")), "CV1.wav");
        assert_eq!(name(decode_output_name(&wav, b"bare")), "bare.wav");
        assert_eq!(name(decode_output_name(&wav, b"A.B/CV1.CSW")), "A.wav");

        let voc = decode_cli(None, DecodeTarget::Voc);
        assert_eq!(name(decode_output_name(&voc, b"CV1.CSW")), "CV1.voc");

        let named = decode_cli(Some("out"), DecodeTarget::Wav);
        assert_eq!(name(decode_output_name(&named, b"t.abc")), "out.wav");
        let named = decode_cli(Some("out.txt"), DecodeTarget::Wav);
        assert_eq!(name(decode_output_name(&named, b"t.abc")), "out.txt");
        let named = decode_cli(Some("out"), DecodeTarget::Voc);
        assert_eq!(name(decode_output_name(&named, b"t.abc")), "out.voc");
    }

    #[test]
    fn a_given_output_name_gains_an_extension_only_when_it_has_no_dot() {
        assert_eq!(name(output_name(Some(b"out"), b"in.wav", "csw")), "out.csw");
        assert_eq!(
            name(output_name(Some(b"out.txt"), b"in.wav", "csw")),
            "out.txt"
        );
        assert_eq!(
            name(output_name(Some(b"SUB/OUT"), b"in.wav", "csw")),
            "SUB/OUT.csw"
        );
        assert_eq!(
            name(output_name(Some(b"A.B/OUT"), b"in.wav", "csw")),
            "A.B/OUT"
        );
        assert_eq!(name(output_name(None, b"A.B/W8.WAV", "csw")), "A.csw");
    }

    #[cfg(feature = "directmode")]
    #[test]
    fn a_direct_run_derives_its_names_the_way_a_conversion_does() {
        let direct = |args: &[&str]| {
            let argv: Vec<std::ffi::OsString> = std::iter::once("-r")
                .chain(args.iter().copied())
                .map(Into::into)
                .collect();
            let cli = cli::parse(&argv).expect("parse");
            assert!(cli.direct && !cli.help);
            let (out, keep, dir) = direct_names(&cli);
            (name(out), name(keep), dir.to_string_lossy().into_owned())
        };
        assert_eq!(
            direct(&["o5"]),
            ("o5.csw".into(), "o5.wav".into(), ".".into())
        );
        assert_eq!(
            direct(&["tape.csw"]),
            ("tape.csw".into(), "tape.wav".into(), ".".into())
        );
        assert_eq!(
            direct(&["rec.dat"]),
            ("rec.csw".into(), "rec.wav".into(), ".".into())
        );
        assert_eq!(
            direct(&["A.B/REC"]),
            ("A.csw".into(), "A.wav".into(), ".".into())
        );
        assert_eq!(
            direct(&["ignored.wav", "my.tapes/rec.csw"]),
            (
                "my.tapes/rec.csw".into(),
                "my.tapes/rec.wav".into(),
                "my.tapes".into()
            )
        );
        assert_eq!(
            direct(&["ignored", "O4.WA"]),
            ("O4.WA".into(), "O.wav".into(), ".".into())
        );
    }

    /// A name is bytes: one this host cannot read as text keeps every byte
    /// through the derivation, and the keep-file's chop of four is four
    /// bytes.
    #[test]
    fn a_name_that_is_not_text_keeps_its_bytes() {
        let wav = decode_cli(None, DecodeTarget::Wav);
        assert_eq!(
            decode_output_name(&wav, b"t\xe1.csw").as_slice(),
            &b"t\xe1.wav"[..]
        );
        assert_eq!(
            output_name(None, b"re\xffc.dat", "csw").as_slice(),
            &b"re\xffc.csw"[..]
        );
        assert_eq!(
            output_name(Some(b"o\x8e"), b"in.wav", "csw").as_slice(),
            &b"o\x8e.csw"[..]
        );
        assert_eq!(keep_file_name(b"ta\xe1.csw").as_slice(), &b"ta\xe1.wav"[..]);
    }

    #[test]
    fn a_v1_file_is_plain_rle_and_a_v2_file_is_z_rle_unless_asked_not_to() {
        for (version, old_compression, plain) in [
            (1u8, false, true),
            (1, true, true),
            (2, false, false),
            (2, true, true),
        ] {
            let cli = cli::Cli {
                csw_version: version,
                old_compression,
                ..cli::Cli::default()
            };
            let settings = settings(&cli);
            assert_eq!(convert::writes_plain_rle(&settings), plain);
            assert_eq!(
                convert::output_compression(&settings),
                if plain {
                    Compression::Rle
                } else {
                    Compression::ZRle
                }
            );
        }
    }
}
