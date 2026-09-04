//! The host audio input DirectMode (`-r`) records from: `cpal` (feature
//! `record`, the default) or the kernel's ALSA ioctls (`record-alsa`, Linux,
//! static builds). A backend pushes frames at a [`SampleSink`] unconverted.

use std::fmt;
use std::sync::Arc;

use crate::error::Result;

// Backend selection: `record-alsa` wins where it can run; elsewhere it is
// inert and `cpal` answers, so `--all-features` builds everywhere.
#[cfg(all(feature = "record-alsa", target_os = "linux"))]
#[path = "audio/alsa.rs"]
mod backend;
#[cfg(all(
    feature = "record",
    not(all(feature = "record-alsa", target_os = "linux"))
))]
#[path = "audio/cpal.rs"]
mod backend;

#[cfg(not(any(all(feature = "record-alsa", target_os = "linux"), feature = "record")))]
compile_error!(
    "DirectMode needs an audio backend: enable `record` (cpal, on any platform) or \
     `record-alsa` (raw ALSA, Linux only)"
);

#[cfg(any(all(feature = "record-alsa", target_os = "linux"), feature = "record"))]
pub use backend::open;

/// What DirectMode wants from a device before it knows what it can have.
#[derive(Debug, Clone, Copy)]
pub struct OpenSpec {
    /// The capture rate to ask for (`-s`, or DirectMode's default).
    pub rate: u32,
    /// `-c`: don't negotiate, take the device's own configuration.
    pub device_compat: bool,
}

/// What a backend settled on once the device had its say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Capture rate in Hz.
    pub rate: u32,
    /// Interleaved channel count.
    pub channels: u16,
    /// The sample type frames will arrive in.
    pub format: Format,
}

/// An opened, configured input device that has not started running yet.
pub trait Input {
    /// The device's name, as the status line should show it.
    fn name(&self) -> &str;
    /// The configuration this device was opened with.
    fn config(&self) -> Config;
    /// Start capturing into `sink`. Capture stops when the returned value is
    /// dropped, which must not return until the sink can no longer be called.
    fn start(self: Box<Self>, sink: Arc<dyn SampleSink>) -> Result<Box<dyn Stream>>;
}

/// A running capture. Dropping it stops the device.
pub trait Stream {}

/// Where a backend delivers what it captured.
///
/// Both methods are called from the audio thread -- `cpal`'s callback, or the
/// ALSA backend's reader -- so neither may block on anything the UI holds.
pub trait SampleSink: Send + Sync + 'static {
    /// One buffer of interleaved frames.
    fn frames(&self, data: Frames<'_>, channels: usize);
    /// The stream has stopped and will not recover.
    fn failed(&self);
    /// Samples the device lost before they could be handed over (an ALSA
    /// overrun). A backend that cannot tell never calls it (`cpal`), hence
    /// the default.
    #[allow(dead_code)]
    fn dropped(&self, _samples: u64) {}
}

/// The sample types a backend may deliver, named and displayed as `cpal`
/// names them (`i16`, `f32`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    U8,
    I8,
    I16,
    U16,
    I32,
    F32,
    F64,
}

impl Format {
    /// Preference order when several formats would serve: the ones that
    /// survive conversion to `f32` without losing range come first. Both
    /// backends apply it.
    pub fn rank(self) -> u8 {
        match self {
            Format::F32 => 0,
            Format::I16 => 1,
            Format::I32 => 2,
            Format::F64 => 3,
            Format::I8 => 4,
            Format::U16 => 5,
            Format::U8 => 6,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::U8 => "u8",
            Format::I8 => "i8",
            Format::I16 => "i16",
            Format::U16 => "u16",
            Format::I32 => "i32",
            Format::F32 => "f32",
            Format::F64 => "f64",
        })
    }
}

/// One buffer of interleaved frames, in the type the device delivered, so the
/// sink can fold straight to the 8-bit domain in one pass.
#[derive(Debug, Clone, Copy)]
pub enum Frames<'a> {
    U8(&'a [u8]),
    I8(&'a [i8]),
    I16(&'a [i16]),
    U16(&'a [u16]),
    I32(&'a [i32]),
    F32(&'a [f32]),
    F64(&'a [f64]),
}

/// A sample type that can be normalised to `f32`, silence at `0.0` and full
/// scale at `±1.0`.
pub trait Sample: Copy {
    /// The conversion, which must agree with `cpal`'s to the bit.
    fn to_f32(self) -> f32;
}

// The divisors are `dasp_sample`'s: the negative full scale, so the positive
// rail lands just short of 1.0. The unsigned types are their signed
// counterparts biased by half their range.
impl Sample for i8 {
    fn to_f32(self) -> f32 {
        self as f32 / 128.0
    }
}
impl Sample for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / 32_768.0
    }
}
impl Sample for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / 2_147_483_648.0
    }
}
impl Sample for u8 {
    fn to_f32(self) -> f32 {
        self.wrapping_sub(128) as i8 as f32 / 128.0
    }
}
impl Sample for u16 {
    fn to_f32(self) -> f32 {
        self.wrapping_sub(32_768) as i16 as f32 / 32_768.0
    }
}
impl Sample for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}
impl Sample for f64 {
    fn to_f32(self) -> f32 {
        self as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_display_matches_rust_type_names() {
        assert_eq!(Format::I16.to_string(), "i16");
        assert_eq!(Format::F32.to_string(), "f32");
        assert_eq!(Format::U8.to_string(), "u8");
    }

    #[test]
    fn rank_prefers_lossless_conversions() {
        let mut all = [
            Format::U8,
            Format::I8,
            Format::I16,
            Format::U16,
            Format::I32,
            Format::F32,
            Format::F64,
        ];
        all.sort_by_key(|f| f.rank());
        assert_eq!(all[0], Format::F32);
        assert_eq!(all[1], Format::I16);
        assert_eq!(*all.last().unwrap(), Format::U8);
    }

    #[test]
    fn silence_converts_to_zero() {
        assert_eq!(0i16.to_f32(), 0.0);
        assert_eq!(128u8.to_f32(), 0.0);
        assert_eq!(32_768u16.to_f32(), 0.0);
        assert_eq!(0i32.to_f32(), 0.0);
    }

    #[test]
    fn rails_reach_full_scale() {
        assert_eq!(i16::MIN.to_f32(), -1.0);
        assert_eq!(0u8.to_f32(), -1.0);
        assert!((i16::MAX.to_f32() - 1.0).abs() < 1e-4);
        assert!((255u8.to_f32() - 1.0).abs() < 1e-2);
    }

    // The conversion must equal `cpal`'s, so where `cpal` is compiled in it
    // is the oracle: every 8- and 16-bit value, the wider ones sampled.
    #[cfg(feature = "record")]
    #[test]
    fn conversion_matches_cpal() {
        use cpal::Sample as _;

        for v in i8::MIN..=i8::MAX {
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "i8 {v}");
        }
        for v in u8::MIN..=u8::MAX {
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "u8 {v}");
        }
        for v in i16::MIN..=i16::MAX {
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "i16 {v}");
        }
        for v in u16::MIN..=u16::MAX {
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "u16 {v}");
        }
        for step in 0..=1024u32 {
            let v = (i32::MIN as i64 + (step as i64 * (u32::MAX as i64 + 1) / 1024)) as i32;
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "i32 {v}");
        }
        for v in [-1.0f64, -0.5, 0.0, 0.25, 1.0, 1.5] {
            assert_eq!(Sample::to_f32(v), v.to_sample::<f32>(), "f64 {v}");
        }
    }
}
