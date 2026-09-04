//! End-to-end tests driving the built `csw` binary: one input of every
//! supported format, the switches, the output names, the refusals, and the
//! reference fixtures. The Z-RLE cases stand down without the C zlib.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_csw");

fn work_dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let id = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("csw_corpus_{}_{}", std::process::id(), id));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the binary; return (success, stdout).
fn run_in_dir(dir: &Path, args: &[&Path]) -> (bool, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn csw");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn run(args: &[&Path]) -> (bool, String) {
    let out = Command::new(BIN).args(args).output().expect("spawn csw");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The process exit code of one run.
fn exit_code(args: &[&Path]) -> i32 {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("spawn csw")
        .status
        .code()
        .unwrap()
}

/// Run the binary, returning (success, stdout + stderr). Diagnostics go to
/// stdout with everything else; stderr is included so a panic is not lost.
fn run_diag(args: &[&Path]) -> (bool, String) {
    let out = Command::new(BIN).args(args).output().expect("spawn csw");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// Run the binary inside `dir`, with relative names; returns (exit, stdout).
fn run_in(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn csw");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Extract the "<N> pulses" count the binary prints.
fn pulses_reported(stdout: &str) -> Option<u64> {
    let idx = stdout.find(" pulses")?;
    stdout[..idx]
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Run the full invariant battery on `input`; returns the reported pulse count.
fn check_invariants(input: &Path, extra_encode: &[&str]) -> u64 {
    let dir = work_dir();
    let csw1 = dir.join("a.csw");
    let wav = dir.join("a.wav");
    let csw2 = dir.join("b.csw");

    let name = input.display().to_string();
    let extra_paths: Vec<PathBuf> = extra_encode.iter().map(PathBuf::from).collect();

    // encode
    let mut enc: Vec<&Path> = vec![input, &csw1];
    for p in &extra_paths {
        enc.push(p);
    }
    let (ok, out) = run(&enc);
    assert!(ok, "encode failed for {name}: {out}");
    let pulses =
        pulses_reported(&out).unwrap_or_else(|| panic!("no pulse count for {name}: {out}"));

    // round-trip fixed point: CSW1 -> WAV -> CSW2, CSW1 == CSW2
    let (ok, out) = run(&[Path::new("-d"), &csw1, &wav]);
    assert!(ok, "decode failed for {name}: {out}");
    let mut enc2: Vec<&Path> = vec![&wav, &csw2];
    for p in &extra_paths {
        enc2.push(p);
    }
    let (ok, out) = run(&enc2);
    assert!(ok, "re-encode failed for {name}: {out}");
    assert_eq!(
        std::fs::read(&csw1).unwrap(),
        std::fs::read(&csw2).unwrap(),
        "round-trip not a fixed point for {name}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    pulses
}

// --- synthetic fixture builders -------------------------------------------

fn write_wav8(path: &Path, rate: u32, samples: &[u8]) {
    let mut v = Vec::new();
    let data_len = samples.len() as u32;
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes()); // byte rate (8-bit mono)
    v.extend_from_slice(&1u16.to_le_bytes()); // block align
    v.extend_from_slice(&8u16.to_le_bytes()); // bits
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v.extend_from_slice(samples);
    std::fs::write(path, v).unwrap();
}

/// A RIFF/WAVE container around raw PCM, for the format-refusal tests.
fn build_wav(rate: u32, channels: u16, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let ba = channels * (bits / 8);
    let dl = pcm.len() as u32;
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + dl).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * ba as u32).to_le_bytes());
    v.extend_from_slice(&ba.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&dl.to_le_bytes());
    v.extend_from_slice(pcm);
    v
}

fn write_iff8(path: &Path, rate: u16, samples: &[i8]) {
    let be = |x: u32| x.to_be_bytes();
    let mut vhdr = Vec::new();
    vhdr.extend_from_slice(&be(0));
    vhdr.extend_from_slice(&be(0));
    vhdr.extend_from_slice(&be(0));
    vhdr.extend_from_slice(&rate.to_be_bytes());
    vhdr.push(1); // ctOctave
    vhdr.push(0); // sCompression
    vhdr.extend_from_slice(&be(0x10000));
    let body: Vec<u8> = samples.iter().map(|&s| s as u8).collect();
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
    std::fs::write(path, file).unwrap();
}

/// An 8-bit mono VOC, written as a v1.20 block 9 so the rate is explicit.
fn write_voc8(path: &Path, rate: u32, samples: &[u8]) {
    let mut v = Vec::new();
    v.extend_from_slice(b"Creative Voice File\x1A");
    v.extend_from_slice(&26u16.to_le_bytes()); // data offset
    v.extend_from_slice(&0x0114u16.to_le_bytes()); // version 1.20
    v.extend_from_slice(&0x111Fu16.to_le_bytes()); // ~version + 0x1234

    let mut block = Vec::new();
    block.extend_from_slice(&rate.to_le_bytes());
    block.push(8); // bits per sample
    block.push(1); // channels
    block.extend_from_slice(&0u16.to_le_bytes()); // format: 0 = unsigned 8-bit PCM
    block.extend_from_slice(&[0; 4]); // reserved
    block.extend_from_slice(samples);
    v.push(9);
    v.extend_from_slice(&(block.len() as u32).to_le_bytes()[..3]);
    v.extend_from_slice(&block);

    v.push(0); // terminator
    std::fs::write(path, v).unwrap();
}

/// A Z80 emulator trace: one write to port 0xFE every `tstates`, so the
/// pulses come out uniform. The 16-bit clock is made to roll over partway
/// through, which puts the wrap-marker path under test as well.
fn write_out(path: &Path, pulses: usize, tstates: u16) {
    let mut v = Vec::new();
    let mut rec = |word_a: u16, word_b: u16| {
        v.extend_from_slice(&word_a.to_le_bytes());
        v.extend_from_slice(&word_b.to_le_bytes());
        v.push(0); // unused
    };

    let mut prev: u32 = 0; // where the last record sat in the current page
    for i in 0..pulses {
        let border = if i % 2 == 0 { 0x10 } else { 0x00 };
        let next = prev + tstates as u32;
        // 0xFFFF is the wrap marker itself, so a boundary can only be placed
        // below it; anything at or past it starts the next page.
        if next >= 0xFFFF {
            rec(0xFFFF, 0xFFFF); // clock wrapped at 0xFFFF, restart at 0
            prev = next - 0xFFFF; // and the rest of this pulse is in the new page
        } else {
            prev = next;
        }
        rec(prev as u16, 0xFE | (border << 8));
    }
    std::fs::write(path, v).unwrap();
}

/// A square wave of the given frequency as N samples at the given rate.
fn square(rate: u32, freq: u32, secs: f64) -> Vec<bool> {
    let n = (rate as f64 * secs) as usize;
    (0..n)
        .map(|i| (i as u64 * 2 * freq as u64 / rate as u64).is_multiple_of(2))
        .collect()
}

// --- tests -----------------------------------------------------------------

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn wav_and_iff_of_one_tone_give_the_same_pulses() {
    let dir = work_dir();
    let levels = square(22050, 1000, 0.2);

    let wav = dir.join("tone.wav");
    let w: Vec<u8> = levels
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    write_wav8(&wav, 22050, &w);
    let p_wav = check_invariants(&wav, &[]);

    let iff = dir.join("tone.iff");
    let s: Vec<i8> = levels.iter().map(|&h| if h { 100 } else { -100 }).collect();
    write_iff8(&iff, 22050, &s);
    let p_iff = check_invariants(&iff, &[]);

    // both encode the same square wave, so the pulse counts match
    assert_eq!(p_wav, p_iff, "WAV and IFF of the same tone disagree");
    assert!(
        p_wav > 300,
        "expected ~400 pulses for 1kHz/0.2s, got {p_wav}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn a_voc_binarises_like_the_wav_that_carries_it() {
    let dir = work_dir();
    let levels = square(22050, 1000, 0.2);

    // The same tone as the WAV and IFF cases above, so it must binarise to
    // the same pulses whatever container carries it.
    let samples: Vec<u8> = levels
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    let wav = dir.join("tone.wav");
    write_wav8(&wav, 22050, &samples);
    let voc = dir.join("tone.voc");
    write_voc8(&voc, 22050, &samples);

    assert_eq!(
        check_invariants(&voc, &[]),
        check_invariants(&wav, &[]),
        "VOC and WAV of the same tone disagree"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// An OUT trace is pulse-native -- it never reaches the detector -- and its
/// timing is in T-states, converted at the fixed `OUT_RATE`; `-s` is ignored
/// for one, there being no rate in the input to override.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn an_out_trace_gives_one_pulse_per_port_write_at_any_rate() {
    let dir = work_dir();
    let trace = dir.join("trace.out");
    // 500 pulses of 1000 T-states: about 140 ms of tape, and enough of the
    // 16-bit clock to roll it over seven times.
    write_out(&trace, 500, 1000);

    assert_eq!(
        check_invariants(&trace, &[]),
        500,
        "one pulse per port write at the default rate"
    );
    assert_eq!(
        check_invariants(&trace, &["-s22050"]),
        500,
        "one pulse per port write at 22050 Hz"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// DirectMode's argument handling, which needs no soundcard to exercise:
/// `-r` takes the place of the input file, and its companion switches mean
/// nothing without it.
#[test]
fn r_with_a_csw_name_decodes_and_reports_the_spool_missing() {
    let dir = work_dir();
    let out = dir.join("live.csw");

    let (ok, msg) = run(&[Path::new("-r")]);
    assert!(!ok, "-r without an output file should fail: {msg}");

    let (ok, msg) = run(&[Path::new("-k"), Path::new("in.wav")]);
    assert!(!ok, "-k on an input that is not there should fail: {msg}");

    let (ok, msg) = run_in_dir(&dir, &[Path::new("-r"), Path::new("-d"), &out]);
    assert!(!ok, "-r with -d decodes a spool that is not there: {msg}");

    let (_, log) = run_in_dir(&dir, &[Path::new("-r"), Path::new("-tzzz"), &out]);
    assert!(log.contains("csw00000.raw.csw"), "{log}");
    assert_eq!(log, msg);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn a_f_token_past_its_first_sub_option_is_dropped() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let packed = dir.join("packed.csw");
    let order_only = dir.join("order.csw");
    let separate = dir.join("separate.csw");

    let (ok, log) = run_diag(&[Path::new("-fo4h3000"), &wav, &packed]);
    assert!(ok, "{log}");
    assert!(log.contains("600-4100 Hz, order 4"), "{log}");
    let (ok, log) = run_diag(&[Path::new("-fo4"), &wav, &order_only]);
    assert!(ok, "{log}");
    assert!(log.contains("600-4100 Hz, order 4"), "{log}");
    let (ok, log) = run_diag(&[Path::new("-fo4"), Path::new("-fh3000"), &wav, &separate]);
    assert!(ok, "{log}");
    assert!(log.contains("600-3000 Hz, order 4"), "{log}");

    let packed = std::fs::read(&packed).unwrap();
    assert_eq!(packed, std::fs::read(&order_only).unwrap());
    assert_ne!(packed, std::fs::read(&separate).unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}

/// Switches and combinations that do nothing are still accepted: only a
/// genuinely unknown switch is an error, so a command line that carries an
/// inert switch still converts. `-k` is the one that is not inert: it has
/// a keep-file of its own (`k_on_a_file_conversion_reads_the_file_it_just_emptied`).
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn accepts_inert_switches_and_combinations() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let csw = dir.join("tone.csw");
    let (ok, log) = run_diag(&[&wav, &csw]);
    assert!(ok, "{log}");
    let out = dir.join("out");

    // Sub-option keys fold like the switch letter before them, so -FO4,
    // -fO4 and -fo4 all design the same filter.
    let folded = dir.join("folded.csw");
    let plain = dir.join("plain.csw");
    let (ok, log) = run_diag(&[Path::new("-fo4h3000"), &wav, &plain]);
    assert!(ok, "{log}");
    for sw in ["-FO4H3000", "-fO4H3000", "-Fo4h3000"] {
        let (ok, log) = run_diag(&[Path::new(sw), &wav, &folded]);
        assert!(ok, "{sw} should work: {log}");
        assert_eq!(
            std::fs::read(&folded).unwrap(),
            std::fs::read(&plain).unwrap(),
            "{sw} should design the same filter as -fo4h3000"
        );
    }

    // Anything outside the documented set is an unknown switch -- `-h`,
    // `--help` and `--compat` included, familiar as they are elsewhere.
    // Help is `-?` and nothing else.
    for sw in ["-n", "-h", "--help", "--compat"] {
        let (ok, log) = run_diag(&[Path::new(sw), &wav, &out]);
        assert!(!ok, "{sw} should be refused: {log}");
        assert!(log.contains("is an invalid switch"), "{sw} -> {log}");
    }

    // `-k` writes no CSW: it reads its own keep-file, which is empty.
    let (ok, log) = run_diag(&[Path::new("-k"), &wav, &out]);
    assert!(!ok, "-k should not convert: {log}");

    // The inert cases -- an ignored decode-side switch, a decode target with
    // junk after it, an encode switch, a DirectMode switch with no -r -- run,
    // and each writes a real file.
    for args in [
        vec!["-d"],
        vec!["-dv"],
        vec!["-dV"],
        vec!["-dx"],           // reads the char after d, ignores the rest -> WAV
        vec!["-dwav"],         // likewise
        vec!["-d", "-s44100"], // -s ignored on decode
        vec!["-d", "-z"],      // -z ignored on decode
        vec!["-d", "-1"],      // -1 ignored on decode
    ] {
        let mut argv: Vec<&Path> = args.iter().map(Path::new).collect();
        argv.push(&csw);
        argv.push(&out);
        let (ok, log) = run_diag(&argv);
        assert!(ok, "{args:?} should work: {log}");
    }
    for sw in ["-s22050", "-1", "-z", "-i5", "-Z", "-t5", "-c"] {
        let (ok, log) = run_diag(&[Path::new(sw), &wav, &out]);
        assert!(ok, "{sw} should work: {log}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// 16-bit and stereo WAV are refused on the "Checking input file..." line,
/// the channel count checked before the bit depth, and write no output.
#[test]
fn refuses_16bit_and_stereo_wav() {
    let dir = work_dir();
    let out = dir.join("out.csw");
    let levels = square(22050, 1000, 0.05);

    // 16-bit mono
    let mut pcm = Vec::new();
    for &h in &levels {
        pcm.extend_from_slice(&(if h { 20000i16 } else { -20000 }).to_le_bytes());
    }
    let w16 = dir.join("w16.wav");
    std::fs::write(&w16, build_wav(22050, 1, 16, &pcm)).unwrap();
    let (ok, log) = run_diag(&[&w16, &out]);
    assert!(!ok, "16-bit WAV was accepted: {log}");
    assert!(
        log.contains("Checking input file... Sorry, 16-bits WAV samples not yet supported."),
        "{log}"
    );
    assert_eq!(exit_code(&[&w16, &out]), 244);
    // The output file, if it was created, is empty: creation happens as soon
    // as the input opens and before the input is parsed (see `encode` in
    // main.rs).
    assert!(
        !out.exists() || std::fs::metadata(&out).unwrap().len() == 0,
        "16-bit refusal still wrote a CSW"
    );

    // 8-bit stereo
    let stereo: Vec<u8> = levels
        .iter()
        .flat_map(|&h| [if h { 0xFF } else { 0x00 }, if h { 0xFF } else { 0x00 }])
        .collect();
    let wst = dir.join("wst.wav");
    std::fs::write(&wst, build_wav(22050, 2, 8, &stereo)).unwrap();
    let (ok, log) = run_diag(&[&wst, &out]);
    assert!(!ok, "stereo WAV was accepted: {log}");
    assert!(
        log.contains("Checking input file... Sorry, stereo WAV samples not yet supported."),
        "{log}"
    );
    assert_eq!(exit_code(&[&wst, &out]), 245);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Each refusal exits with its own error number (the table above `exit_code`
/// in main.rs): 246 for compressed WAV or IFF sample data, 252 for a CSW past
/// v2.99, 251 for a compression type not known, 255 for a CSW declaring a
/// rate of 0. v2.99 itself decodes.
#[test]
fn a_refusal_exits_with_its_error_number() {
    let dir = work_dir();
    let out = dir.join("out.csw");
    let levels = square(22050, 1000, 0.05);
    let pcm: Vec<u8> = levels
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();

    let mut tag2 = build_wav(22050, 1, 8, &pcm);
    tag2[20] = 2; // the format tag: ADPCM, not PCM
    let wav = dir.join("tag2.wav");
    std::fs::write(&wav, tag2).unwrap();
    assert_eq!(exit_code(&[&wav, &out]), 246);

    let iff = dir.join("cmp.iff");
    let signed: Vec<i8> = levels.iter().map(|&h| if h { 100 } else { -100 }).collect();
    write_iff8(&iff, 22050, &signed);
    let mut bytes = std::fs::read(&iff).unwrap();
    bytes[35] = 1; // VHDR sCompression: Fibonacci-delta
    std::fs::write(&iff, bytes).unwrap();
    assert_eq!(exit_code(&[&iff, &out]), 246);

    let pulses = [100u8, 100, 100, 100];
    let write = |name: &str, mut header: Vec<u8>| {
        header.extend_from_slice(&pulses);
        std::fs::write(dir.join(name), header).unwrap();
    };
    let mut v300 = csw2_header(4, 1);
    v300[0x17] = 3;
    write("v300.csw", v300);
    let mut v299 = csw2_header(4, 1);
    v299[0x18] = 99;
    write("v299.csw", v299);
    write("comp9.csw", csw2_header(4, 9));
    let mut rate0 = csw2_header(4, 1);
    rate0[0x19..0x1D].fill(0);
    write("rate0.csw", rate0);
    for (name, code) in [
        ("v300.csw", 252),
        ("v299.csw", 0),
        ("comp9.csw", 251),
        ("rate0.csw", 255),
    ] {
        let (got, log) = run_in(&dir, &["-d", name, "o.wav"]);
        assert_eq!(got, code, "{name}: {log}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A VOC block 9 is 8-bit at several different codecs, only one of which is
/// PCM. The others are refused; reading them as PCM gives noise.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn refuses_a_voc_that_is_eight_bit_but_not_pcm() {
    let dir = work_dir();
    let out = dir.join("out.csw");
    let samples: Vec<u8> = (0..800)
        .map(|i| if (i / 40) % 2 == 0 { 0 } else { 255 })
        .collect();

    let pcm = dir.join("pcm.voc");
    write_voc8(&pcm, 22050, &samples);
    let (ok, log) = run_diag(&[&pcm, &out]);
    assert!(ok, "codec 0 is the PCM one: {log}");

    for codec in [1u16, 4, 6, 7] {
        let path = dir.join(format!("c{codec}.voc"));
        let mut v = std::fs::read(&pcm).unwrap();
        // The codec word sits 6 bytes into the block 9 body, which starts at
        // 26 (file header) + 4 (block type and length).
        v[30 + 6..30 + 8].copy_from_slice(&codec.to_le_bytes());
        std::fs::write(&path, v).unwrap();
        let (ok, log) = run_diag(&[&path, &out]);
        assert!(!ok, "codec {codec} should be refused: {log}");
        assert!(
            log.contains("only 8-bit mono PCM VOC files are supported"),
            "codec {codec} -> {log}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A signal with no edges in it converts without the detector holding it:
/// the polarity probes read to the end and re-read the file.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn an_edge_free_signal_is_not_held_in_memory() {
    let dir = work_dir();
    let flat = dir.join("flat.wav");
    let out = dir.join("flat.csw");
    let samples = vec![128u8; 4 << 20];

    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&44100u32.to_le_bytes());
    v.extend_from_slice(&44100u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&8u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    v.extend_from_slice(&samples);
    std::fs::write(&flat, &v).unwrap();

    let (ok, log) = run_diag(&[&flat, &out]);
    assert!(ok, "{log}");
    // The answer for a signal with fewer than three edges in it: no
    // pulses written, one reported.
    assert!(log.contains("1 pulses"), "{log}");
    let written = std::fs::read(&out).unwrap();
    assert_eq!(
        u32::from_le_bytes(written[0x1D..0x21].try_into().unwrap()),
        1
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_voc_walk_past_its_read_buffer_ends_in_silence_with_an_empty_file() {
    let dir = work_dir();
    let flat = dir.join("flat.voc");
    let mut v = b"Creative Voice File\x1a".to_vec();
    v.extend_from_slice(&26u16.to_le_bytes());
    v.extend_from_slice(&0x010Au16.to_le_bytes());
    v.extend_from_slice(&0x1129u16.to_le_bytes());
    v.push(1);
    v.extend_from_slice(&3_000_000u32.to_le_bytes()[..3]);
    v.extend_from_slice(&[0xa6, 0]);
    v.resize(v.len() + 300_000, 0x80);
    std::fs::write(&flat, &v).unwrap();
    for (args, tail) in [
        (
            vec!["flat.voc", "z.csw", "-z"],
            "* Writing CSW v2 with old compression method\n* Working...",
        ),
        (vec!["flat.voc", "d.csw"], "* Working..."),
    ] {
        let (code, log) = run_in(&dir, &args);
        assert_eq!(code, 255, "{log}");
        assert!(log.ends_with(tail), "{log}");
        assert_eq!(std::fs::metadata(dir.join(args[1])).unwrap().len(), 0);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A container whose sample data is declared past the end of the file is
/// clamped to what is there and converted, in every format that has a length
/// to overrun -- not only WAV.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn clamps_every_container_whose_data_runs_past_the_end() {
    let dir = work_dir();
    let out = dir.join("out.csw");
    let samples: Vec<u8> = (0..800)
        .map(|i| if (i / 40) % 2 == 0 { 0 } else { 255 })
        .collect();
    let n = samples.len();

    // WAV: the `data` chunk claims four times what follows it.
    let wav = dir.join("over.wav");
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&22050u32.to_le_bytes());
    v.extend_from_slice(&22050u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&8u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&((n * 4) as u32).to_le_bytes());
    v.extend_from_slice(&samples);
    std::fs::write(&wav, &v).unwrap();

    // IFF: the BODY chunk does the same.
    let iff = dir.join("over.iff");
    let mut body = Vec::new();
    body.extend_from_slice(b"VHDR");
    body.extend_from_slice(&20u32.to_be_bytes());
    body.extend_from_slice(&(n as u32).to_be_bytes());
    body.extend_from_slice(&[0; 8]);
    body.extend_from_slice(&22050u16.to_be_bytes());
    body.push(1);
    body.push(0);
    body.extend_from_slice(&0x1_0000u32.to_be_bytes());
    body.extend_from_slice(b"BODY");
    body.extend_from_slice(&((n * 4) as u32).to_be_bytes());
    body.extend_from_slice(&samples);
    let mut v = Vec::from(*b"FORM");
    v.extend_from_slice(&((body.len() + 4) as u32).to_be_bytes());
    v.extend_from_slice(b"8SVX");
    v.extend_from_slice(&body);
    std::fs::write(&iff, &v).unwrap();

    // VOC: the block-1 length does the same.
    let voc = dir.join("over.voc");
    let mut v = Vec::from(*b"Creative Voice File\x1A");
    v.extend_from_slice(&26u16.to_le_bytes());
    v.extend_from_slice(&0x010Au16.to_le_bytes());
    v.extend_from_slice(&(!0x010Au16).wrapping_add(0x1234).to_le_bytes());
    v.push(1);
    v.extend_from_slice(&(((n + 2) * 4) as u32).to_le_bytes()[..3]);
    v.push(233);
    v.push(0);
    v.extend_from_slice(&samples);
    std::fs::write(&voc, &v).unwrap();

    // All three convert, and none of them says anything about it. A WAV
    // even reports the *declared* sample count -- four times what the file
    // holds here -- with the playing time to match: the console reports what
    // the file says about itself, and converts what is there.
    for path in [&wav, &iff, &voc] {
        let (ok, log) = run_diag(&[path.as_path(), &out]);
        assert!(ok, "{} should convert: {log}", path.display());
        assert!(
            !log.contains("runs past the end"),
            "{} should say nothing about the overrun: {log}",
            path.display()
        );
    }
    let (_, log) = run_diag(&[wav.as_path(), &out]);
    assert!(
        log.contains(&format!("{} samples", n * 4)),
        "the WAV should report its declared sample count: {log}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The same codec check, on the block that actually carries it for an
/// extended VOC. A block 8 supersedes the block 1 behind it, so a v1.10 ADPCM
/// file names the coding in the block 8 and leaves the block 1's own pack byte
/// at 0 -- and reading only the block 1 lets it through as PCM.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn refuses_a_voc_whose_codec_is_named_by_the_extended_block() {
    let dir = work_dir();
    let out = dir.join("out.csw");
    let samples: Vec<u8> = (0..800)
        .map(|i| if (i / 40) % 2 == 0 { 0 } else { 255 })
        .collect();

    // v1.10: [block 8: WORD tc, BYTE pack, BYTE mode][block 1: BYTE tc, BYTE
    // pack, samples]. Only the block 8's pack byte varies below.
    let build = |pack: u8| {
        let mut v = Vec::new();
        v.extend_from_slice(b"Creative Voice File\x1A");
        v.extend_from_slice(&26u16.to_le_bytes());
        v.extend_from_slice(&0x010Au16.to_le_bytes());
        v.extend_from_slice(&(!0x010Au16).wrapping_add(0x1234).to_le_bytes());
        v.push(8);
        v.extend_from_slice(&4u32.to_le_bytes()[..3]);
        v.extend_from_slice(&0xF800u16.to_le_bytes()); // 16-bit time constant
        v.push(pack);
        v.push(0); // mono
        v.push(1);
        v.extend_from_slice(&((samples.len() + 2) as u32).to_le_bytes()[..3]);
        v.push(233); // superseded by the block 8's constant
        v.push(0); // the block 1's own pack byte: PCM, as the spec directs
        v.extend_from_slice(&samples);
        v.push(0); // terminator
        v
    };

    let pcm = dir.join("ext-pcm.voc");
    std::fs::write(&pcm, build(0)).unwrap();
    let (ok, log) = run_diag(&[&pcm, &out]);
    assert!(ok, "pack 0 is the PCM one: {log}");

    for pack in [1u8, 2, 3, 4] {
        let path = dir.join(format!("ext-p{pack}.voc"));
        std::fs::write(&path, build(pack)).unwrap();
        let (ok, log) = run_diag(&[&path, &out]);
        assert!(!ok, "block-8 pack {pack} should be refused: {log}");
        // A block 8's pack byte is refused as a malformed block, not a
        // compressed one: block 1 has its own wording for that.
        assert!(
            log.contains("FATAL ERROR: Extended block contains nonsense data"),
            "pack {pack} -> {log}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file carrying the signature but not a whole header is abandoned in
/// silence -- banner, nothing else, no output -- while one that carries
/// neither gets the not-found line. The two are different failures and the
/// exit codes differ with them.
#[test]
fn tells_a_truncated_csw_from_a_file_that_is_not_one() {
    let dir = work_dir();
    let out = dir.join("out.wav");

    let short = dir.join("short.csw");
    std::fs::write(&short, b"Compressed Square Wave\x1a\x02\x00").unwrap();
    let (ok, log) = run_diag(&[Path::new("-d"), &short, &out]);
    assert!(!ok, "{log}");
    assert!(
        !log.contains("ERROR"),
        "a truncated CSW is abandoned in silence: {log}"
    );
    // The output file, if it was created, is empty: creation happens as soon
    // as the input opens and before the input is parsed (see `decode` in
    // main.rs).
    assert!(
        !out.exists() || std::fs::metadata(&out).unwrap().len() == 0,
        "no output file is written: {log}"
    );

    let alien = dir.join("alien.csw");
    std::fs::write(&alien, vec![0x42u8; 25]).unwrap();
    let (ok, log) = run_diag(&[Path::new("-d"), &alien, &out]);
    assert!(!ok, "{log}");
    assert!(log.contains("not found or invalid file type"), "{log}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The `-f` status line: its wording, field order and precisions are fixed,
/// and the cutoffs printed are the ones the design actually uses.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn reports_the_filter_it_designed() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let out = dir.join("tone.csw");

    let line = |args: &[&str]| -> String {
        let mut argv: Vec<&Path> = args.iter().map(Path::new).collect();
        argv.push(&wav);
        argv.push(&out);
        let (ok, log) = run_diag(&argv);
        assert!(ok, "conversion failed: {log}");
        log.lines()
            .find(|l| l.contains("Digital filter:"))
            .unwrap_or_else(|| panic!("no filter line for {args:?}: {log}"))
            // The bullet is CP437 0xFE when redirected, which the lossy UTF-8
            // read renders as U+FFFD; on a terminal it is U+25A0.
            .trim_start_matches(['\u{fffd}', '\u{25a0}', ' '])
            .to_string()
    };

    assert_eq!(
        line(&["-fo4"]),
        "Digital filter: Butterworth band-pass 600-4100 Hz, order 4"
    );
    // An odd band-pass order is reported as the even one actually designed.
    assert_eq!(
        line(&["-fo3"]),
        "Digital filter: Butterworth band-pass 600-4100 Hz, order 4"
    );
    // Ripple only for Chebyshev; the one-sided bands name a single cutoff and
    // print it to two decimals.
    assert_eq!(
        line(&["-fp2", "-fo16", "-fr3"]),
        "Digital filter: Chebyshev band-pass 600-4100 Hz, order 16, ripple 3.00"
    );
    assert_eq!(
        line(&["-ft3", "-fh5000"]),
        "Digital filter: Butterworth low-pass 5000.00 Hz, order 2"
    );
    assert_eq!(
        line(&["-ft5"]),
        "Digital filter: Butterworth high-pass 600.00 Hz, order 2"
    );

    // No filter asked for, no line.
    let (ok, log) = run_diag(&[&wav, &out]);
    assert!(ok, "{log}");
    assert!(!log.contains("Digital filter:"), "unasked-for line: {log}");

    // An OUT trace is already clean digital edges, so the filter does not
    // apply to one and no line is printed for it.
    let trace = dir.join("trace.out");
    write_out(&trace, 100, 1000);
    let (ok, log) = run_diag(&[Path::new("-fo4"), &trace, &out]);
    assert!(ok, "{log}");
    assert!(
        !log.contains("Digital filter:"),
        "line for OUT input: {log}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The help screen reports the filter settings in force, and `-f` sub-options
/// accumulate across switches.
#[test]
fn help_reports_the_filter_settings_from_the_command_line() {
    // The help screen exits 1, so `ok` is false here and the screen itself
    // is what this asserts on.
    let (ok, out) = run(&[Path::new("-fo4"), Path::new("-fh300")]);
    assert!(!ok, "the help screen exits 1: {out}");
    assert!(
        out.contains("order [4]"),
        "order not taken from -fo4: {out}"
    );
    assert!(
        out.contains("[300"),
        "upper cutoff not taken from -fh300: {out}"
    );
    assert!(!out.contains("[4100"), "stale default upper cutoff: {out}");

    // Defaults still read as defaults.
    let (ok, out) = run(&[Path::new("-?")]);
    assert!(!ok, "the help screen exits 1: {out}");
    assert!(out.contains("order [2]") && out.contains("[4100"), "{out}");
}

// --- behaviours a cleanup would remove ---------------------------------------
//
// Each of these reproduces a behaviour that reads as a mistake, with the
// numbers as they stand, so that removing it reports what moved. The inputs
// are built here: nothing in the fixtures reaches them.

/// A square wave under hum and hiss, the shape that reaches the detector's
/// stale candidate: `seed` drives a linear congruential generator.
fn noisy_square(rate: u32, secs: f64, sq: f64, hum: f64, hiss: u32, seed: u32) -> Vec<u8> {
    let n = (rate as f64 * secs) as usize;
    let mut state = seed;
    (0..n)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let square = if ((t * 4000.0) as u64).is_multiple_of(2) {
                sq
            } else {
                -sq
            };
            let hum = hum * (2.0 * std::f64::consts::PI * 60.0 * t).sin();
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345) & 0x7FFF_FFFF;
            let hiss = (state % (2 * hiss + 1)) as f64 - hiss as f64;
            (128.0 + square + hum + hiss)
                .round_ties_even()
                .clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// The pulse count a written CSW v2 declares.
fn declared_pulses(csw: &[u8]) -> u32 {
    u32::from_le_bytes(csw[0x1D..0x21].try_into().unwrap())
}

/// The data lengths of a VOC's blocks, in order.
fn voc_block_lengths(voc: &[u8]) -> Vec<(u8, u32)> {
    let mut at = 26;
    let mut out = Vec::new();
    while at < voc.len() && voc[at] != 0 {
        let len = u32::from_le_bytes([voc[at + 1], voc[at + 2], voc[at + 3], 0]);
        out.push((voc[at], len));
        at += 4 + len as usize;
    }
    out
}

/// The detector's stale candidate is zeroed between calls on the Z-RLE path
/// and not on the plain-RLE path, so noise converts to a different pulse
/// count under `-z`. Make the candidate ordinary state and the two agree.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn noise_converts_to_different_pulse_counts_by_writer_path() {
    let dir = work_dir();
    let wav = dir.join("noise44.wav");
    write_wav8(&wav, 44100, &noisy_square(44100, 0.7, 60.0, 25.0, 12, 11));
    let zrle = dir.join("z.csw");
    let rle = dir.join("r.csw");
    let (ok, log) = run(&[&wav, &zrle]);
    assert!(ok, "{log}");
    let (ok, log) = run(&[&wav, &rle, Path::new("-z")]);
    assert!(ok, "{log}");
    let (z, r) = (
        declared_pulses(&std::fs::read(&zrle).unwrap()),
        declared_pulses(&std::fs::read(&rle).unwrap()),
    );
    assert_eq!((z, r), (2630, 2632));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The `-dv` writer flushes a pulse longer than its 256 KiB buffer in
/// buffer-sized blocks and leaves its fill at the last chunk's size, so the
/// pulses after it are appended behind that many stale bytes: `[10, 500000,
/// 10, 20]` comes out as blocks of 10, 262144, 237856 and 237886 samples.
#[test]
fn a_long_pulse_leaves_the_voc_writers_fill_where_it_stood() {
    let dir = work_dir();
    let csw = dir.join("long.csw");
    let voc = dir.join("long.voc");
    let mut file = csw2_header(4, 0x01);
    file.push(10);
    file.push(0);
    file.extend_from_slice(&500_000u32.to_le_bytes());
    file.extend_from_slice(&[10, 20]);
    std::fs::write(&csw, file).unwrap();
    let (ok, log) = run(&[Path::new("-dv"), &csw, &voc]);
    assert!(ok, "{log}");
    let blocks = voc_block_lengths(&std::fs::read(&voc).unwrap());
    let lengths: Vec<u32> = blocks.iter().map(|&(_, len)| len - 2).collect();
    assert_eq!(lengths, [10, 262_144, 237_856, 237_886]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The VOC time constant is `256 - 1e6/rate` with the rate divided in as a
/// signed int and the result taken as a byte with no range check: 1000 Hz
/// gives 25, 1 Hz gives 193, and a rate of 0xFFFFFFFF is -1 and gives 64.
#[test]
fn the_voc_time_constant_wraps_and_divides_signed() {
    let dir = work_dir();
    for (rate, tc) in [(1000u32, 25u8), (1, 193), (0xFFFF_FFFF, 64), (22050, 211)] {
        let csw = dir.join(format!("r{rate}.csw"));
        let voc = dir.join(format!("r{rate}.voc"));
        let mut file = csw2_header(2, 0x01);
        file[0x19..0x1D].copy_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&[50, 50]);
        std::fs::write(&csw, file).unwrap();
        let (ok, log) = run(&[Path::new("-dv"), &csw, &voc]);
        assert!(ok, "rate {rate}: {log}");
        let out = std::fs::read(&voc).unwrap();
        assert_eq!(out[26], 1, "rate {rate}: first block is type 1");
        assert_eq!(out[30], tc, "rate {rate}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A trace whose writes all share one timestamp has no gap to convert, and
/// the CSW it writes still declares one pulse: the count is floored at 1.
#[test]
fn a_trace_of_one_timestamp_declares_one_pulse() {
    let dir = work_dir();
    let trace = dir.join("same.out");
    let mut v = Vec::new();
    for _ in 0..3 {
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&0x10FEu16.to_le_bytes());
        v.push(0);
    }
    std::fs::write(&trace, v).unwrap();
    let csw = dir.join("same.csw");
    let (ok, log) = run(&[&trace, &csw, Path::new("-z")]);
    assert!(ok, "{log}");
    let out = std::fs::read(&csw).unwrap();
    assert_eq!(declared_pulses(&out), 1);
    assert_eq!(out.len(), 0x34, "no pulse data follows the header");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A WAV whose `fmt ` is followed by a `fact` chunk of `fact` bytes, padded
/// to an even length or not, then the `data` chunk.
fn wav_with_fact(pcm: &[u8], fact: &[u8], pad: bool) -> Vec<u8> {
    let mut v = build_wav(22050, 1, 8, &[]);
    v.truncate(36); // through the fmt chunk
    v.extend_from_slice(b"fact");
    v.extend_from_slice(&(fact.len() as u32).to_le_bytes());
    v.extend_from_slice(fact);
    if pad && fact.len() % 2 == 1 {
        v.push(0);
    }
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    v.extend_from_slice(pcm);
    let riff = (v.len() - 8) as u32;
    v[4..8].copy_from_slice(&riff.to_le_bytes());
    v
}

/// A CSW of compression type 0 decodes to a WAV of nothing: 44 bytes of
/// header, no samples, exit 0, the type reported on the application line.
#[test]
fn compression_type_0_decodes_to_an_empty_wav() {
    let dir = work_dir();
    let csw = dir.join("c0.csw");
    let wav = dir.join("c0.wav");
    let mut file = csw2_header(4, 0);
    file.extend_from_slice(&[100; 4]);
    std::fs::write(&csw, file).unwrap();
    let (code, log) = run_in(&dir, &["-d", "c0.csw", "c0.wav"]);
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("using compression type 0"), "{log}");
    assert_eq!(std::fs::metadata(&wav).unwrap().len(), 44);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A major version other than 1 or 2 is read with the v1 layout, whose
/// compression byte is the third byte of a v2 rate: 0 for any ordinary rate,
/// so a v0.00 file decodes to nothing.
#[test]
fn an_unknown_major_version_is_read_with_the_v1_layout() {
    let dir = work_dir();
    let csw = dir.join("v0.csw");
    let wav = dir.join("v0.wav");
    let mut file = csw2_header(4, 1);
    file[0x17] = 0;
    file.extend_from_slice(&[100; 4]);
    std::fs::write(&csw, file).unwrap();
    let (code, log) = run_in(&dir, &["-d", "v0.csw", "v0.wav"]);
    assert_eq!(code, 0, "{log}");
    assert!(
        log.contains("Compressed Square Wave v0.00 at 44100 Hz"),
        "{log}"
    );
    assert!(log.contains("Total 0 samples"), "{log}");
    assert_eq!(std::fs::metadata(&wav).unwrap().len(), 44);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A VOC header's data-block offset is checked against 26, not followed: a
/// file whose first block really does sit at 27 is refused all the same.
#[test]
fn a_voc_data_offset_other_than_26_is_refused() {
    let dir = work_dir();
    let voc = dir.join("off27.voc");
    let samples: Vec<u8> = square(22050, 1000, 0.05)
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    write_voc8(&voc, 22050, &samples);
    let mut bytes = std::fs::read(&voc).unwrap();
    bytes[20..22].copy_from_slice(&27u16.to_le_bytes());
    bytes.insert(26, 0); // the block area now starts where the header says
    std::fs::write(&voc, bytes).unwrap();
    let (code, log) = run_in(&dir, &["off27.voc", "o.csw"]);
    assert_eq!(code, 1, "{log}");
    assert!(
        log.contains("Input file is corrupted or in a wrong format"),
        "{log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// No pad byte is skipped after an odd-sized `fact`: a correctly padded
/// 3-byte `fact` puts the pad where the `data` header is expected and the
/// file is refused, while the same chunk unpadded converts.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn an_odd_fact_chunk_is_read_without_its_pad_byte() {
    let dir = work_dir();
    let pcm: Vec<u8> = square(22050, 1000, 0.05)
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    std::fs::write(dir.join("padded.wav"), wav_with_fact(&pcm, b"abc", true)).unwrap();
    std::fs::write(dir.join("bare.wav"), wav_with_fact(&pcm, b"abc", false)).unwrap();
    let (code, log) = run_in(&dir, &["padded.wav", "p.csw"]);
    assert_eq!(code, 1, "{log}");
    assert!(log.contains("Checking input file..."), "{log}");
    assert!(log.contains("FATAL ERROR: Wrong file type"), "{log}");
    let (code, log) = run_in(&dir, &["bare.wav", "b.csw"]);
    assert_eq!(code, 0, "{log}");
    assert!(log.contains("RIFF Wave PCM (WAV), 1102 samples."), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The help screen answers a third file name, under `-r` as on a conversion,
/// and any command line of more than five arguments before it is parsed:
/// five that convert become help with a sixth switch added.
#[test]
fn a_third_name_and_a_sixth_argument_are_the_help_screen() {
    let dir = work_dir();
    let pcm: Vec<u8> = square(22050, 1000, 0.05)
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    write_wav8(&dir.join("sq.wav"), 22050, &pcm);
    let (code, log) = run_in(&dir, &["sq.wav", "five.csw", "-z", "-1", "-i0"]);
    assert_eq!(code, 0, "{log}");
    for args in [
        &["-r", "a", "b", "c"][..],
        &["a", "b", "c"][..],
        &["sq.wav", "six.csw", "-z", "-1", "-i0", "-s8000"][..],
    ] {
        let (code, log) = run_in(&dir, args);
        assert_eq!(code, 1, "{args:?}: {log}");
        assert!(
            log.contains("Syntax : CSW [options] inputfile [outputfile]"),
            "{args:?}: {log}"
        );
    }
    assert!(
        !dir.join("six.csw").exists(),
        "a sixth argument still converted"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `-s` takes a leading number or nothing, so `-s0` and `-sxyz` convert a
/// file as if the switch were absent; `-ft` takes any number, and one outside
/// 3, 4 and 5 designs a filter reported as `???` whose output is one pulse.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn s_and_ft_take_whatever_sscanf_reads() {
    let dir = work_dir();
    let wav = dir.join("sq.wav");
    let pcm: Vec<u8> = square(22050, 1000, 0.05)
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    write_wav8(&wav, 22050, &pcm);
    for switch in ["-s0", "-sxyz", "-s99999999"] {
        let (code, log) = run_in(&dir, &["sq.wav", "s.csw", switch]);
        assert_eq!(code, 0, "{switch}: {log}");
        assert!(log.contains("Sampling rate: 22050 Hz"), "{switch}: {log}");
        assert!(log.contains("(100 pulses)"), "{switch}: {log}");
    }
    let (code, log) = run_in(&dir, &["sq.wav", "t.csw", "-f", "-ft9"]);
    assert_eq!(code, 0, "{log}");
    assert!(
        log.contains("Digital filter: Butterworth ??? , order 2"),
        "{log}"
    );
    assert!(log.contains("(1 pulses)"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `-f` sub-option's number is stored into a 32-bit int, magnitude
/// wrapping at 2^32 and the sign applied after: the help brackets show it,
/// and `-fo4294967297` designs order 1, the same file as `-fo1`.
#[test]
fn sub_option_numbers_wrap_at_two_to_the_32() {
    for (switch, shown) in [
        ("-fo-1", "set filter order [-1]"),
        ("-fo2147483648", "set filter order [-2147483648]"),
        ("-fo4294967297", "set filter order [1]"),
        ("-ft4294967300", "[4]"),
    ] {
        let (ok, log) = run(&[Path::new(switch), Path::new("-?")]);
        assert!(!ok);
        assert!(log.contains(shown), "{switch}: {log}");
        assert!(!log.contains("4294967300"), "{switch}: {log}");
    }
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let (wrapped, one) = (dir.join("w.csw"), dir.join("one.csw"));
    let (ok, log) = run(&[&wav, &wrapped, Path::new("-fo4294967297"), Path::new("-z")]);
    assert!(ok, "{log}");
    assert!(
        log.contains("Digital filter: Butterworth band-pass 600-4100 Hz, order 2"),
        "{log}"
    );
    let (ok, _) = run(&[&wav, &one, Path::new("-fo1"), Path::new("-z")]);
    assert!(ok);
    assert_eq!(
        std::fs::read(&wrapped).unwrap(),
        std::fs::read(&one).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `-f3` is accepted and changes nothing: the default filter, the same file
/// as `-f`.
#[test]
fn f3_is_accepted_and_ignored() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let (three, plain) = (dir.join("three.csw"), dir.join("plain.csw"));
    let (ok, log) = run(&[&wav, &three, Path::new("-f3"), Path::new("-z")]);
    assert!(ok, "{log}");
    assert!(
        log.contains("Digital filter: Butterworth band-pass 600-4100 Hz, order 2"),
        "{log}"
    );
    let (ok, _) = run(&[&wav, &plain, Path::new("-f"), Path::new("-z")]);
    assert!(ok);
    assert_eq!(
        std::fs::read(&three).unwrap(),
        std::fs::read(&plain).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The `-dv` writer at its buffer's boundaries: a pulse that fills the buffer
/// exactly, one that starts a block at the buffer's size, one of two
/// buffers, and one a sample over.
#[test]
fn the_voc_writer_at_its_buffer_boundaries() {
    let dir = work_dir();
    for (name, pulses, blocks) in [
        ("fit", &[10u32, 262_134, 5][..], &[10u32, 262_139][..]),
        ("cap", &[262_144, 10][..], &[0, 262_144, 10][..]),
        ("two", &[524_288][..], &[0, 262_144, 262_144, 262_144][..]),
        ("over", &[262_145, 10][..], &[0, 262_144, 1, 11][..]),
    ] {
        let csw = dir.join(format!("{name}.csw"));
        let voc = dir.join(format!("{name}.voc"));
        let mut file = csw2_header(pulses.len() as u32, 0x01);
        for &p in pulses {
            if p < 256 {
                file.push(p as u8);
            } else {
                file.push(0);
                file.extend_from_slice(&p.to_le_bytes());
            }
        }
        std::fs::write(&csw, file).unwrap();
        let (ok, log) = run(&[Path::new("-dv"), &csw, &voc]);
        assert!(ok, "{name}: {log}");
        let got: Vec<u32> = voc_block_lengths(&std::fs::read(&voc).unwrap())
            .iter()
            .map(|&(_, len)| len - 2)
            .collect();
        assert_eq!(got, blocks, "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A v1 file through the binary: it decodes with no application line, a v1
/// rate of 0 is abandoned in silence, and a minor version over 99 is printed
/// as such and refused as past v2.99.
#[test]
fn v1_files_and_wide_minor_versions_through_the_binary() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    std::fs::write(&wav, TONE_WAV).unwrap();
    let (code, log) = run_in(&dir, &["tone.wav", "v1.csw", "-1"]);
    assert_eq!(code, 0, "{log}");
    let (code, log) = run_in(&dir, &["-d", "v1.csw", "v1.wav"]);
    assert_eq!(code, 0, "{log}");
    assert!(
        log.contains("Compressed Square Wave v1.01 at 32258 Hz"),
        "{log}"
    );
    assert!(!log.contains("created by application"), "{log}");

    let v1 = std::fs::read(dir.join("v1.csw")).unwrap();
    let mut rate0 = v1.clone();
    rate0[0x19..0x1B].fill(0);
    std::fs::write(dir.join("r0.csw"), rate0).unwrap();
    assert_eq!(run_in(&dir, &["-d", "r0.csw", "r0.wav"]).0, 255);

    let mut v1m = v1;
    v1m[0x18] = 200;
    std::fs::write(dir.join("v1m.csw"), v1m).unwrap();
    let mut v2m = csw2_header(4, 1);
    v2m[0x18] = 100;
    v2m.extend_from_slice(&[100; 4]);
    std::fs::write(dir.join("v2m.csw"), v2m).unwrap();
    for (name, shown) in [("v1m.csw", "v1.200"), ("v2m.csw", "v2.100")] {
        let (code, log) = run_in(&dir, &["-d", name, "m.wav"]);
        assert_eq!(code, 252, "{name}: {log}");
        assert!(
            log.contains(&format!("Compressed Square Wave {shown} at")),
            "{name}: {log}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// --- parity against the reference fixtures ---------------------------------
//
// `fixtures/` at the repo root holds matched pairs not produced by this
// repository: a WAV and the CSW its pulse stream must encode to. See the
// README there for what each pair covers.

/// Half-wave tone: exercises the adaptive deadband and confirmation length.
const TONE_WAV: &[u8] = include_bytes!("../fixtures/tone-8bit.wav");
const TONE_CSW: &[u8] = include_bytes!("../fixtures/tone-8bit.csw");
/// Full-scale square: exercises the large-jump path (|delta| > 127).
const SQUARE_WAV: &[u8] = include_bytes!("../fixtures/square-8bit.wav");
const SQUARE_CSW: &[u8] = include_bytes!("../fixtures/square-8bit.csw");

/// Assert our encoder reproduces the fixture's pulse stream exactly.
///
/// Both sides are written through our own container, and the deflate over a
/// given pulse stream is deterministic, so the comparison is byte-for-byte.
/// The fixture's side is obtained by decoding its CSW to a full-scale square
/// and re-encoding: that round-trip is a fixed point, so what comes back is
/// the fixture's own pulse stream in our container.
fn assert_pulse_parity(name: &str, wav_bytes: &[u8], csw_bytes: &[u8]) {
    let dir = work_dir();
    let wav = dir.join("in.wav");
    let ours = dir.join("ours.csw");
    let fixture = dir.join("fixture.csw");
    let fixture_wav = dir.join("fixture.wav");
    let theirs = dir.join("theirs.csw");
    std::fs::write(&wav, wav_bytes).unwrap();
    std::fs::write(&fixture, csw_bytes).unwrap();

    let (ok, out) = run(&[&wav, &ours]);
    assert!(ok, "{name}: encode failed: {out}");
    let (ok, out) = run(&[Path::new("-d"), &fixture, &fixture_wav]);
    assert!(ok, "{name}: decoding the fixture failed: {out}");
    let (ok, out) = run(&[&fixture_wav, &theirs]);
    assert!(ok, "{name}: re-encode failed: {out}");

    assert_eq!(
        std::fs::read(&ours).unwrap(),
        std::fs::read(&theirs).unwrap(),
        "{name}: pulse stream differs from the fixture's"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn matches_the_fixture_on_a_half_wave_tone() {
    assert_pulse_parity("tone", TONE_WAV, TONE_CSW);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn matches_the_fixture_on_a_full_scale_square() {
    assert_pulse_parity("square", SQUARE_WAV, SQUARE_CSW);
}

/// Both pairs go further than pulse parity: the whole file matches, the
/// deflate stream and the application stamp included. The tone is the more
/// sensitive -- its payload is long enough that a deflate strategy change
/// moves it, where the square's 24 bytes are not.
fn assert_byte_parity(name: &str, wav_bytes: &[u8], csw_bytes: &[u8]) {
    let dir = work_dir();
    let wav = dir.join(format!("{name}.wav"));
    std::fs::write(&wav, wav_bytes).unwrap();
    let out = dir.join(format!("{name}.csw"));
    let (ok, log) = run(&[&wav, &out]);
    assert!(ok, "encode failed: {log}");

    let got = std::fs::read(&out).unwrap();
    assert_eq!(
        got.len(),
        csw_bytes.len(),
        "{name}: file is {} bytes, the reference is {}",
        got.len(),
        csw_bytes.len()
    );
    let differs = got.iter().zip(csw_bytes).position(|(a, b)| a != b);
    assert!(
        differs.is_none(),
        "{name}: differs from the reference at byte {}",
        differs.unwrap()
    );
    assert_eq!(
        &got[0x24..0x2D],
        b"CSW v2.00",
        "{name}: wrong application stamp"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn reproduces_the_reference_tone_file_byte_for_byte() {
    assert_byte_parity("tone", TONE_WAV, TONE_CSW);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn reproduces_the_reference_square_file_byte_for_byte() {
    assert_byte_parity("square", SQUARE_WAV, SQUARE_CSW);
}

#[test]
fn decodes_a_reference_fixture() {
    let dir = work_dir();
    let csw = dir.join("fixture.csw");
    let wav = dir.join("fixture.wav");
    std::fs::write(&csw, TONE_CSW).unwrap();

    let (ok, out) = run(&[Path::new("-d"), &csw, &wav]);
    assert!(ok, "could not decode the fixture: {out}");
    // The fixture is stamped at 32258 Hz.
    assert!(
        out.contains("at 32258 Hz"),
        "sampling rate not reported: {out}"
    );

    // Its pulse lengths must expand back to exactly the frames it covered.
    let decoded = std::fs::read(&wav).unwrap();
    let data_len = u32::from_le_bytes(decoded[40..44].try_into().unwrap());
    assert_eq!(data_len, 65448, "decoded length disagrees with the fixture");

    // Every sample is 224 or 32 -- the +-96 around the midpoint, not full
    // scale.
    let bad: Vec<u8> = decoded[44..]
        .iter()
        .copied()
        .filter(|&b| b != 224 && b != 32)
        .take(4)
        .collect();
    assert!(bad.is_empty(), "decoded sample not 224/32: {bad:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

// --- CSW headers the reader accepts or refuses ------------------------------

/// A CSW v2 header claiming `count` pulses under `compression`.
fn csw2_header(count: u32, compression: u8) -> Vec<u8> {
    let mut h = Vec::new();
    h.extend_from_slice(b"Compressed Square Wave");
    h.push(0x1A);
    h.extend_from_slice(&[0x02, 0x00]);
    h.extend_from_slice(&44100u32.to_le_bytes());
    h.extend_from_slice(&count.to_le_bytes());
    h.push(compression);
    h.push(0x01); // initial polarity high
    h.push(0x00); // no header extension
    let app = b"CSW v2.00";
    h.extend_from_slice(app);
    h.extend_from_slice(&vec![0u8; 16 - app.len()]);
    h
}

/// A zero-length pulse (only the 4-byte escape can carry one) is decoded, and
/// the WAV that comes out contradicts its own header: every pulse is
/// rendered, the zero flipping the level with nothing between, while the
/// file is sized from a sum that stops at the zero. For `100 0 100 100 100`
/// that is 400 bytes of payload under a header declaring 100 samples.
#[test]
fn decodes_a_csw_carrying_a_zero_length_pulse() {
    let dir = work_dir();
    let csw = dir.join("zero.csw");
    let wav = dir.join("zero.wav");

    let mut file = csw2_header(5, 0x01);
    file.push(100);
    file.push(0); // escape ...
    file.extend_from_slice(&0u32.to_le_bytes()); // ... carrying a zero
    file.extend_from_slice(&[100, 100, 100]);
    std::fs::write(&csw, &file).unwrap();

    let (ok, out) = run_diag(&[Path::new("-d"), &csw, &wav]);
    assert!(ok, "this file decodes rather than being refused: {out}");
    let got = std::fs::read(&wav).expect("a WAV was written");
    assert_eq!(got.len(), 444, "file size");
    assert_eq!(
        u32::from_le_bytes(got[40..44].try_into().unwrap()),
        100,
        "the data chunk declares the sum that stopped at the zero"
    );
    assert_eq!(got.len() - 44, 400, "while the payload holds every pulse");
    // 100 high, the zero flipping with nothing between, then 100 high again --
    // so the first run is 200 samples long.
    assert_eq!(&got[44..244], &[224u8; 200][..]);
    assert_eq!(&got[244..344], &[32u8; 100][..]);
    assert_eq!(&got[344..444], &[224u8; 100][..]);

    // The same file to VOC: a block length has to cover what is written, so
    // the one block is sized from the 400 samples rendered (declared 402),
    // while the console reports the 100 the short sum gives.
    let voc = dir.join("zero.voc");
    let (ok, out) = run_diag(&[Path::new("-dv"), &csw, &voc]);
    assert!(ok, "it decodes to VOC too: {out}");
    assert!(
        out.contains("Total 100 samples"),
        "the console keeps the short sum: {out}"
    );
    let got = std::fs::read(&voc).expect("a VOC was written");
    assert_eq!(got.len(), 433, "file size");
    assert_eq!(got[26], 0x01, "one type-1 block");
    assert_eq!(
        u32::from_le_bytes([got[27], got[28], got[29], 0]),
        402,
        "sized from the 400 samples rendered, plus the two header bytes"
    );
    assert_eq!(&got[32..232], &[224u8; 200][..]);
    assert_eq!(&got[232..332], &[32u8; 100][..]);
    assert_eq!(&got[332..432], &[224u8; 100][..]);
    assert_eq!(got[432], 0x00, "terminator");

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that *is* a CSW but will not parse reports why; one that is not a
/// CSW keeps the not-found line, doubled extension and all.
#[test]
fn malformed_csw_reports_its_reason_but_a_non_csw_does_not() {
    let dir = work_dir();

    let bad = dir.join("badcomp.csw");
    let mut file = csw2_header(4, 0x09); // no such compression type
    file.extend_from_slice(&[100, 100, 100, 100]);
    std::fs::write(&bad, &file).unwrap();
    let (ok, out) = run_diag(&[Path::new("-d"), &bad, &dir.join("out.wav")]);
    assert!(!ok, "an unsupported compression type was accepted: {out}");
    assert!(
        out.contains("compression type"),
        "the reason was not reported: {out}"
    );

    let alien = dir.join("alien.csw");
    std::fs::write(&alien, b"Not A Square Wave!!!! and then some padding").unwrap();
    let (ok, out) = run_diag(&[Path::new("-d"), &alien, &dir.join("out2.wav")]);
    assert!(!ok, "a non-CSW was accepted: {out}");
    assert!(
        out.contains("not found or invalid file type"),
        "the not-found line for a non-CSW was lost: {out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The 0x16 terminator is written and never read back: only the 22 bytes of
/// the signature are compared, so a header carrying any byte there decodes.
#[test]
fn reads_a_header_whose_terminator_byte_is_wrong() {
    let dir = work_dir();
    let csw = dir.join("badterm.csw");
    let mut file = csw2_header(4, 0x01);
    file[0x16] = 0xFF;
    file.extend_from_slice(&[100, 100, 100, 100]);
    std::fs::write(&csw, &file).unwrap();

    let wav = dir.join("out.wav");
    let (ok, out) = run_diag(&[Path::new("-d"), &csw, &wav]);
    assert!(ok, "a wrong terminator byte was refused: {out}");
    assert!(wav.exists(), "nothing was written: {out}");

    // A wrong byte *inside* the signature is still not a CSW.
    let mut file = csw2_header(4, 0x01);
    file[0x15] = 0xFF;
    file.extend_from_slice(&[100, 100, 100, 100]);
    let bad = dir.join("badsig.csw");
    std::fs::write(&bad, &file).unwrap();
    let (ok, out) = run_diag(&[Path::new("-d"), &bad, &dir.join("no.wav")]);
    assert!(!ok, "a wrong signature was accepted: {out}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn decodes_the_body_whatever_the_count_says() {
    let dir = work_dir();
    let csw = dir.join("badcount.csw");
    let wav = dir.join("badcount.wav");
    let mut file = csw2_header(99, 0x01); // four pulses follow, not 99
    file.extend_from_slice(&[100, 100, 100, 100]);
    std::fs::write(&csw, &file).unwrap();

    let (ok, out) = run_diag(&[Path::new("-d"), &csw, &wav]);
    assert!(ok, "a count mismatch stopped the decode: {out}");
    assert!(
        out.contains("Total 400 samples"),
        "the total followed the header, not the body: {out}"
    );
    // 4 pulses x 100 samples, and the WAV says so, not 99.
    let decoded = std::fs::read(&wav).unwrap();
    let data_len = u32::from_le_bytes(decoded[40..44].try_into().unwrap());
    assert_eq!(
        data_len, 400,
        "decoded length followed the header, not the body"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- the names a run opens and writes ---------------------------------------

/// A minimal CSW v2 (plain RLE) carrying `pulses`.
fn write_min_csw(path: &Path, pulses: &[u8]) {
    let mut v = Vec::new();
    v.extend_from_slice(b"Compressed Square Wave\x1a");
    v.extend_from_slice(&[2, 0]);
    v.extend_from_slice(&22050u32.to_le_bytes());
    v.extend_from_slice(&(pulses.len() as u32).to_le_bytes());
    v.extend_from_slice(&[1, 0, 0]);
    v.extend_from_slice(&[0u8; 16]);
    v.extend_from_slice(pulses);
    std::fs::write(path, v).unwrap();
}

#[test]
fn a_refused_encode_empties_the_output_but_a_failed_open_does_not() {
    let dir = work_dir();
    std::fs::write(dir.join("junk.wav"), b"not a RIFF file at all").unwrap();
    std::fs::create_dir(dir.join("adir.wav")).unwrap();
    std::fs::write(dir.join("keep.csw"), b"IMPORTANT BYTES").unwrap();
    let (code, out) = run_in(&dir, &["junk.wav", "keep.csw"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Wrong file type"), "{out}");
    assert_eq!(std::fs::metadata(dir.join("keep.csw")).unwrap().len(), 0);

    for input in ["missing.wav", "adir.wav"] {
        std::fs::write(dir.join("kept.csw"), b"IMPORTANT BYTES").unwrap();
        let (code, out) = run_in(&dir, &[input, "kept.csw"]);
        assert_eq!(code, 1, "{out}");
        assert!(out.contains("Could not open input file"), "{out}");
        assert_eq!(std::fs::metadata(dir.join("kept.csw")).unwrap().len(), 15);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn an_encode_of_a_dotless_name_never_opens_the_bare_file() {
    let dir = work_dir();
    let tone: Vec<u8> = square(22050, 1000, 0.1)
        .iter()
        .map(|&h| if h { 0xFF } else { 0x00 })
        .collect();
    write_wav8(&dir.join("bare"), 22050, &tone);
    let (code, out) = run_in(&dir, &["bare", "o.csw"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Could not open input file"), "{out}");
    assert!(
        !dir.join("o.csw").exists(),
        "an output for a name that never opened"
    );

    write_wav8(&dir.join("bare.wav"), 22050, &tone);
    let (code, out) = run_in(&dir, &["bare", "o.csw"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("RIFF Wave PCM (WAV)"), "{out}");
    assert!(dir.join("o.csw").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_keep_file_that_cannot_be_created_is_an_input_that_could_not_be_opened() {
    let dir = work_dir();
    write_wav8(&dir.join("in.wav"), 22050, &[128; 400]);
    std::fs::create_dir(dir.join("tape.wav")).unwrap();
    let (code, out) = run_in(&dir, &["in.wav", "tape.csw", "-k"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Could not open input file"), "{out}");
    assert_eq!(std::fs::metadata(dir.join("tape.csw")).unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_trailing_dot_is_part_of_the_name_on_this_host() {
    let dir = work_dir();
    write_wav8(&dir.join("W8.WAV"), 22050, &[128; 400]);
    let (code, out) = run_in(&dir, &["W8.WAV", "OD2.", "-z"]);
    assert_eq!(code, 0, "{out}");
    assert!(dir.join("OD2.").is_file());
    assert!(!dir.join("OD2").exists());
    write_min_csw(&dir.join("CVDOT"), &[100, 100, 100, 100]);
    let (code, out) = run_in(&dir, &["-d", "CVDOT."]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Could not open input file"), "{out}");
    assert!(!dir.join("CVDOT.wav").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A dotted name that cannot be opened is refused without a `.csw` retry --
/// and leaves no output file, under either name it might have derived.
#[test]
fn a_dotted_name_is_not_retried_with_csw_appended() {
    let dir = work_dir();
    write_min_csw(&dir.join("t.abc.csw"), &[100, 100, 100, 100]);
    let (code, out) = run_in(&dir, &["-d", "t.abc"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Could not open input file"), "{out}");
    assert!(!dir.join("t.wav").exists(), "stray t.wav");
    assert!(!dir.join("t.abc.wav").exists(), "stray t.abc.wav");
}

/// A dotless name opens `name.csw` and nothing else: the bare file is never
/// tried, and the output takes the typed name's stem.
#[test]
fn a_dotless_name_opens_only_its_csw_twin() {
    let dir = work_dir();
    write_min_csw(&dir.join("bare"), &[100, 100, 100, 100]);
    let (code, out) = run_in(&dir, &["-d", "bare"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Could not open input file"), "{out}");
    assert!(!dir.join("bare.wav").exists(), "stray bare.wav");

    write_min_csw(&dir.join("twin.csw"), &[100, 100, 100, 100]);
    let (code, out) = run_in(&dir, &["-d", "twin"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("(file 'twin.csw')"), "{out}");
    assert!(out.contains("Writing WAV file 'twin.wav'"), "{out}");
    assert!(dir.join("twin.wav").exists());
}

/// The retry proper: a file that opens and is not a CSW is followed by
/// `name.csw`, and the output is derived from the name as typed.
#[test]
fn a_non_csw_that_opens_is_retried_and_the_output_keeps_the_typed_name() {
    let dir = work_dir();
    std::fs::write(dir.join("x.dat"), b"not a csw at all").unwrap();
    write_min_csw(&dir.join("x.dat.csw"), &[100, 100, 100, 100]);
    let (code, out) = run_in(&dir, &["-d", "x.dat"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("(file 'x.dat.csw')"), "{out}");
    assert!(out.contains("Writing WAV file 'x.wav'"), "{out}");
    assert!(dir.join("x.wav").exists());
    assert!(!dir.join("x.dat.wav").exists(), "stray x.dat.wav");

    std::fs::remove_file(dir.join("x.dat.csw")).unwrap();
    let (code, out) = run_in(&dir, &["-d", "x.dat"]);
    assert_eq!(code, 254, "{out}");
    assert!(out.contains("Input file 'x.dat.csw' not found"), "{out}");
}

/// `-k` on a file conversion is the DirectMode keep-file reached without any
/// recording: its name comes from the **output** -- the last four characters
/// dropped, `.wav` put on -- and it is created empty and then read as the
/// input. So an encode ends as "Wrong file type" having emptied that file,
/// and a decode reports it missing under its own name.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn k_on_a_file_conversion_reads_the_file_it_just_emptied() {
    let dir = work_dir();
    let wav = dir.join("tone.wav");
    write_wav8(
        &wav,
        22050,
        &square(22050, 1000, 0.2)
            .iter()
            .map(|&h| if h { 0xFF } else { 0x00 })
            .collect::<Vec<u8>>(),
    );
    let csw = dir.join("tone.csw");
    let (ok, log) = run_diag(&[&wav, &csw]);
    assert!(ok, "{log}");
    let encoded = std::fs::read(&csw).unwrap();
    assert!(!encoded.is_empty());

    // `tone.csw` gives `tone.wav`, which here is the input: the conversion
    // empties the file it was asked to read.
    let (ok, log) = run_diag(&[Path::new("-k"), &wav, &csw]);
    assert!(!ok, "-k should not convert: {log}");
    assert!(log.contains("Wrong file type"), "{log}");
    assert_eq!(
        std::fs::metadata(&wav).unwrap().len(),
        0,
        "-k empties its own input"
    );
    assert_eq!(std::fs::metadata(&csw).unwrap().len(), 0);

    // Four characters, not the extension: an output named `o4.wa` keeps
    // `o.wav`, and that is the name the decode reports.
    std::fs::write(&csw, &encoded).unwrap();
    let (ok, log) = run_diag(&[Path::new("-k"), Path::new("-d"), &csw, &dir.join("o4.wa")]);
    assert!(!ok, "-k should not decode: {log}");
    assert!(log.contains("o.wav.csw' not found"), "{log}");
    assert_eq!(std::fs::metadata(dir.join("o.wav")).unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn the_seven_volatile_fold_keys_are_no_switch_and_no_filter_key() {
    use std::os::unix::ffi::OsStrExt;
    for key in [0xcfu8, 0xe3, 0xe4, 0xe5, 0xf3, 0xf4, 0xf5] {
        for token in [vec![b'-', b'f', key], vec![b'-', key]] {
            let sw = std::ffi::OsStr::from_bytes(&token);
            let (ok, log) = run(&[Path::new(sw), Path::new("in.wav"), Path::new("out.csw")]);
            let shown = String::from_utf8_lossy(&token).into_owned();
            assert!(!ok, "{shown} should be refused: {log}");
            assert!(log.contains("is an invalid switch"), "{shown} -> {log}");
        }
    }
}

/// The line an unknown switch raises quotes the token as argv carried it: the
/// folded letter, then the rest of the token byte for byte.
#[cfg(unix)]
#[test]
fn an_unknown_switch_is_quoted_in_the_bytes_argv_carried() {
    use std::os::unix::ffi::OsStrExt;
    for (token, quoted) in [
        (&b"-\x8e\xa0"[..], &b"-D\xa0 is an invalid switch"[..]),
        (&b"-f\x8e\xa0"[..], &b"-fD\xa0 is an invalid switch"[..]),
        (&b"-fq4\xa0"[..], &b"-fq4\xa0 is an invalid switch"[..]),
        // A letter folding to a NUL ends the token where it stands.
        (&b"-\xff\xa0"[..], &b"- is an invalid switch"[..]),
    ] {
        let out = Command::new(BIN)
            .arg(std::ffi::OsStr::from_bytes(token))
            .arg("in.wav")
            .arg("out.csw")
            .output()
            .expect("spawn csw");
        assert!(
            out.stdout.windows(quoted.len()).any(|w| w == quoted),
            "{:?} -> {:?}",
            String::from_utf8_lossy(token),
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
