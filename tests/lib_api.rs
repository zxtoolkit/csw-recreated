//! The library without the binary: a buffer in, a buffer out, the console
//! lines collected.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

use csw::container;
use csw::convert::{self, DecodeTarget, Report, Settings};
use csw::error::Result;
use csw::filter::FilterSpec;

const BIN: &str = env!("CARGO_BIN_EXE_csw");
const TONE_WAV: &[u8] = include_bytes!("../fixtures/tone-8bit.wav");
const TONE_CSW: &[u8] = include_bytes!("../fixtures/tone-8bit.csw");

/// A VOC of one 60-sample block 1 and no terminator: the real run reads on
/// past the end of the file and is refused, after the Working line.
const UNTERMINATED_VOC: [u8; 92] = [
    0x43, 0x72, 0x65, 0x61, 0x74, 0x69, 0x76, 0x65, 0x20, 0x56, 0x6f, 0x69, 0x63, 0x65, 0x20, 0x46,
    0x69, 0x6c, 0x65, 0x1a, 0x1a, 0x00, 0x0a, 0x01, 0xf5, 0xfe, 0x01, 0x3e, 0x00, 0x00, 0xd3, 0x00,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Every console line as the name of the method that would print it.
struct Log(Vec<String>);

impl Report for Log {
    fn checking_start(&mut self, _integrity: bool) -> Result<()> {
        self.0.push("checking_start".into());
        Ok(())
    }
    fn checking_completion(&mut self, completion: &str) -> Result<()> {
        self.0.push(format!("checking_completion {completion}"));
        Ok(())
    }
    fn describe(&mut self, _desc: &str) -> Result<()> {
        self.0.push("describe".into());
        Ok(())
    }
    fn sampling_rate(&mut self, _rate: i64, _samples: u64) -> Result<()> {
        self.0.push("sampling_rate".into());
        Ok(())
    }
    fn warn_block(&mut self, _msg: &str) -> Result<()> {
        self.0.push("warn_block".into());
        Ok(())
    }
    fn digital_filter(&mut self, _spec: &FilterSpec) -> Result<()> {
        self.0.push("digital_filter".into());
        Ok(())
    }
    fn writing_old_compression(&mut self, _version: u8) -> Result<()> {
        self.0.push("writing_old_compression".into());
        Ok(())
    }
    fn working(&mut self) -> Result<()> {
        self.0.push("working".into());
        Ok(())
    }
    fn packed(&mut self, _packed: u64, _pulses: usize, _orig: u64) -> Result<()> {
        self.0.push("packed".into());
        Ok(())
    }
    fn csw_header(&mut self, _major: u8, _minor: u8, _rate: u32, _file: &[u8]) -> Result<()> {
        self.0.push("csw_header".into());
        Ok(())
    }
    fn csw_app(&mut self, _app: &[u8], _comp: u8) -> Result<()> {
        self.0.push("csw_app".into());
        Ok(())
    }
    fn total_samples(&mut self, _samples: u64, _rate: u32) -> Result<()> {
        self.0.push("total_samples".into());
        Ok(())
    }
    fn conversion_starts(&mut self) -> Result<()> {
        self.0.push("conversion_starts".into());
        Ok(())
    }
    fn writing(&mut self, kind: &str, _file: &[u8]) -> Result<()> {
        self.0.push(format!("writing {kind}"));
        Ok(())
    }
    fn completed(&mut self, _written: u64) -> Result<()> {
        self.0.push("completed".into());
        Ok(())
    }
}

fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("csw-lib-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn encodes_from_a_buffer_to_the_fixture_bytes() {
    let settings = Settings::default();
    assert!(settings.filter.is_none());
    assert_eq!(settings.csw_version, 2);
    assert!(!settings.old_compression);
    let mut input = Cursor::new(TONE_WAV);
    let mut out = Vec::new();
    let mut log = Log(Vec::new());
    convert::encode(
        &mut input,
        b"tone-8bit.wav",
        b"tone-8bit.wav",
        &settings,
        &mut log,
        &mut out,
    )
    .unwrap();
    assert_eq!(out, TONE_CSW);
    assert_eq!(
        log.0,
        [
            "checking_start",
            "checking_completion ok!",
            "describe",
            "sampling_rate",
            "working",
            "packed"
        ]
    );
}

#[test]
fn decodes_to_a_buffer_as_the_binary_writes_the_file() {
    let dir = work_dir();
    let csw = dir.join("tone.csw");
    std::fs::write(&csw, TONE_CSW).unwrap();
    let info = container::read_header(TONE_CSW).unwrap();
    for (target, switch, name, kind) in [
        (DecodeTarget::Wav, "-d", "tone.wav", "WAV"),
        (DecodeTarget::Voc, "-dv", "tone.voc", "VOC"),
    ] {
        let file = dir.join(name);
        let status = Command::new(BIN)
            .args([switch])
            .arg(&csw)
            .arg(&file)
            .status()
            .unwrap();
        assert!(status.success(), "{name}");
        let expected = std::fs::read(&file).unwrap();

        let mut out = Vec::new();
        let mut log = Log(Vec::new());
        let written = convert::decode(
            TONE_CSW,
            &info,
            b"tone.csw",
            target,
            name.as_bytes(),
            &mut log,
            &mut out,
        )
        .unwrap();
        assert_eq!(written as usize, out.len(), "{name}");
        assert_eq!(out, expected, "{name}");
        assert_eq!(
            log.0,
            [
                "csw_header",
                "csw_app",
                "total_samples",
                "conversion_starts",
                &format!("writing {kind}"),
                "completed"
            ]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A refusal met by the real run, after the Working line, leaves what the
/// run had written: the Z-RLE header alone, or the header and the pulses so
/// far under plain RLE.
#[cfg_attr(not(feature = "zlib-c"), ignore = "Z-RLE output needs the C zlib")]
#[test]
fn a_refusal_met_by_the_real_run_leaves_the_partial_file() {
    for (settings, len) in [
        (Settings::default(), 52),
        (
            Settings {
                old_compression: true,
                ..Settings::default()
            },
            65,
        ),
        (
            Settings {
                csw_version: 1,
                ..Settings::default()
            },
            45,
        ),
    ] {
        let mut input = Cursor::new(&UNTERMINATED_VOC[..]);
        let mut out = Vec::new();
        let mut log = Log(Vec::new());
        let result = convert::encode(
            &mut input,
            b"unterminated.voc",
            b"unterminated.voc",
            &settings,
            &mut log,
            &mut out,
        );
        assert!(
            result.is_err(),
            "v{} accepted the file",
            settings.csw_version
        );
        assert_eq!(out.len(), len, "v{}", settings.csw_version);
        assert_eq!(log.0.last().map(String::as_str), Some("working"));
    }
}
