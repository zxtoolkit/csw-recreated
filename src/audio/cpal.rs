//! The portable DirectMode backend: `cpal` over CoreAudio, WASAPI or ALSA,
//! built everywhere unless `record-alsa` is selected on Linux.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};

use super::{Config, Format, Frames, Input, OpenSpec, SampleSink, Stream};
use crate::error::{Error, Result};

/// Open the host's default input device, configured as `spec` asks.
pub fn open(spec: &OpenSpec) -> Result<Box<dyn Input>> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        Error::Fatal("Unable to initialize soundcard: no audio input device".into())
    })?;
    let name = device.to_string();
    let config = choose_config(&device, spec)?;
    Ok(Box::new(CpalInput {
        device,
        name,
        config,
    }))
}

struct CpalInput {
    device: cpal::Device,
    name: String,
    config: SupportedStreamConfig,
}

impl Input for CpalInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> Config {
        Config {
            rate: self.config.sample_rate(),
            channels: self.config.channels(),
            // `choose_config` refuses a default whose format `format_of` does
            // not know, so this is never the fallback.
            format: format_of(self.config.sample_format()).unwrap_or(Format::F32),
        }
    }

    fn start(self: Box<Self>, sink: Arc<dyn SampleSink>) -> Result<Box<dyn Stream>> {
        let channels = self.config.channels() as usize;
        let stream_config: cpal::StreamConfig = self.config.into();

        macro_rules! input_stream {
            ($t:ty, $variant:ident) => {{
                let sink = Arc::clone(&sink);
                let errors = Arc::clone(&sink);
                self.device.build_input_stream::<$t, _, _>(
                    stream_config,
                    move |data: &[$t], _: &_| sink.frames(Frames::$variant(data), channels),
                    move |_| errors.failed(),
                    None,
                )
            }};
        }

        let stream = match self.config.sample_format() {
            SampleFormat::I8 => input_stream!(i8, I8),
            SampleFormat::I16 => input_stream!(i16, I16),
            SampleFormat::I32 => input_stream!(i32, I32),
            SampleFormat::U8 => input_stream!(u8, U8),
            SampleFormat::U16 => input_stream!(u16, U16),
            SampleFormat::F32 => input_stream!(f32, F32),
            SampleFormat::F64 => input_stream!(f64, F64),
            other => {
                return Err(Error::Unsupported(format!(
                    "soundcard sample format '{other}' is not supported"
                )));
            }
        }
        .map_err(|e| Error::Fatal(format!("Unable to initialize soundcard: {e}").into()))?;

        stream
            .play()
            .map_err(|e| Error::Fatal(format!("Cannot start recording: {e}").into()))?;
        Ok(Box::new(CpalStream(stream)))
    }
}

/// Dropping the `cpal::Stream` stops the device, which is all this has to do.
struct CpalStream(#[allow(dead_code)] cpal::Stream);

impl Stream for CpalStream {}

/// Pick a stream configuration: honour the requested rate when the device
/// supports it, otherwise fall back to the device's own preferred
/// configuration (which is all `-c` ever does).
fn choose_config(device: &cpal::Device, spec: &OpenSpec) -> Result<SupportedStreamConfig> {
    let default = || {
        let config = device
            .default_input_config()
            .map_err(|e| Error::Fatal(format!("Unable to initialize soundcard: {e}").into()))?;
        if format_of(config.sample_format()).is_none() {
            return Err(Error::Fatal(
                format!(
                    "Unable to initialize soundcard: sample format {} not supported",
                    config.sample_format()
                )
                .into(),
            ));
        }
        Ok(config)
    };
    if spec.device_compat {
        return default();
    }
    let ranges = match device.supported_input_configs() {
        Ok(r) => r,
        Err(_) => return default(),
    };
    // Among ranges covering the requested rate, prefer mono and a sample
    // format we can convert without loss of range.
    let rate = spec.rate;
    let best = ranges
        .filter(|r| r.min_sample_rate() <= rate && rate <= r.max_sample_rate())
        .filter_map(|r| format_of(r.sample_format()).map(|f| (r, f)))
        .min_by_key(|(r, f)| (r.channels() != 1, f.rank(), r.channels()))
        .map(|(r, _)| r);
    match best {
        Some(r) => Ok(r.with_sample_rate(rate)),
        None => default(),
    }
}

/// The formats the sink can convert, as this crate names them. `None` for the
/// ones `cpal` knows and DirectMode does not.
fn format_of(f: SampleFormat) -> Option<Format> {
    Some(match f {
        SampleFormat::I8 => Format::I8,
        SampleFormat::I16 => Format::I16,
        SampleFormat::I32 => Format::I32,
        SampleFormat::U8 => Format::U8,
        SampleFormat::U16 => Format::U16,
        SampleFormat::F32 => Format::F32,
        SampleFormat::F64 => Format::F64,
        _ => return None,
    })
}
