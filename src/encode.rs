//! The encode pipeline: samples in, pulses out, filtered first if asked.

use crate::error::{Error, Result};
use crate::filter::{self, FilterSpec};
use crate::signal::Pulses;
use crate::{detect, source, voc};
use std::io;

/// A signal the encoder can read more than once: the detector reads from the
/// first sample three times (see `detect::PulseDetector`).
pub trait Rewindable {
    fn rewind(&mut self) -> Result<()>;
    fn next_chunk(&mut self, out: &mut Vec<f64>) -> Result<bool>;
    fn past_end(&mut self) -> Result<Option<voc::TailRead>> {
        Ok(None)
    }
}

impl<R: io::Read + io::Seek> Rewindable for source::SampleSource<R> {
    fn rewind(&mut self) -> Result<()> {
        source::SampleSource::rewind(self);
        Ok(())
    }
    fn next_chunk(&mut self, out: &mut Vec<f64>) -> Result<bool> {
        source::SampleSource::next_chunk(self, out)
    }
    fn past_end(&mut self) -> Result<Option<voc::TailRead>> {
        source::SampleSource::past_end(self)
    }
}

/// Binarise samples into pulses, running the digital filter first if asked.
/// Both stages are state machines, so the samples arrive in pieces and
/// nothing holds a second copy of the signal.
pub struct PulseEncoder {
    rate: u32,
    chop: bool,
    design_rate: f64,
    midpoint: f64,
    spec: Option<FilterSpec>,
    detector: detect::PulseDetector,
    pulses: Vec<u32>,
    /// Reused across chunks so filtering allocates once.
    work: Vec<f64>,
}

impl PulseEncoder {
    pub fn new(
        rate: u32,
        design_rate: f64,
        midpoint: f64,
        spec: Option<FilterSpec>,
        zrle: bool,
        per_sample_flag: bool,
    ) -> Self {
        PulseEncoder {
            rate,
            chop: per_sample_flag,
            design_rate,
            midpoint,
            spec,
            detector: detect::PulseDetector::new()
                .with_zrle_frame(zrle)
                .with_valid_per_sample(per_sample_flag),
            pulses: Vec::new(),
            work: Vec::new(),
        }
    }

    /// A filter for one pass: each pass starts at the first sample and needs
    /// a recursion that starts empty.
    fn applier(&self) -> Option<filter::Applier> {
        self.spec
            .as_ref()
            .map(|spec| filter::Applier::new(spec, self.design_rate))
    }

    /// One chunk of samples in the byte domain the detector reads.
    fn byte_domain(&mut self, applier: &mut Option<filter::Applier>, samples: &[f64]) -> Vec<u8> {
        match applier {
            Some(applier) => {
                self.work.clear();
                self.work.extend(samples.iter().map(|&s| s - self.midpoint));
                applier.process(&mut self.work);
                if self.chop {
                    self.work.iter().map(|&y| detect::to_byte_chop(y)).collect()
                } else {
                    detect::to_byte_domain(&self.work, 0.0)
                }
            }
            None => detect::to_byte_domain(samples, self.midpoint),
        }
    }

    /// Read `src` once for each polarity probe and once for the run that
    /// emits.
    pub fn run<S: Rewindable>(&mut self, src: &mut S) -> Result<()> {
        self.probe(src)?;
        self.real(src)
    }

    pub fn probe<S: Rewindable>(&mut self, src: &mut S) -> Result<()> {
        let mut chunk = Vec::new();
        while !self.detector.ready() {
            src.rewind()?;
            let mut applier = self.applier();
            let mut answered = false;
            while src.next_chunk(&mut chunk)? {
                let bytes = self.byte_domain(&mut applier, &chunk);
                if self.detector.probe(&bytes) {
                    answered = true;
                    break;
                }
            }
            if !answered {
                self.read_on(src, &mut applier, false)?;
            }
        }
        Ok(())
    }

    pub fn real<S: Rewindable>(&mut self, src: &mut S) -> Result<()> {
        let mut chunk = Vec::new();
        src.rewind()?;
        let mut applier = self.applier();
        while src.next_chunk(&mut chunk)? {
            let bytes = self.byte_domain(&mut applier, &chunk);
            self.detector.push(&bytes, &mut self.pulses);
        }
        self.read_on(src, &mut applier, true)
    }

    pub fn partial(&self) -> Pulses {
        let mut sig = Pulses::new(self.rate, self.pulses.clone(), self.detector.initial_high());
        sig.declared = 0;
        sig
    }

    fn read_on<S: Rewindable>(
        &mut self,
        src: &mut S,
        applier: &mut Option<filter::Applier>,
        real: bool,
    ) -> Result<()> {
        loop {
            if real && self.detector.is_done() {
                return Ok(());
            }
            let (sample, valid) = match src.past_end()? {
                Some(voc::TailRead::Terminator) => (0, false),
                Some(voc::TailRead::Sample(b, valid)) => {
                    (self.byte_domain(applier, &[f64::from(b)])[0], valid)
                }
                Some(voc::TailRead::Heap) => {
                    if real {
                        return Err(Error::Silent(
                            "VOC read past the reader's 256 KiB buffer".into(),
                        ));
                    }
                    self.detector.probe_eof();
                    return Ok(());
                }
                None => {
                    if real {
                        self.detector.finish(&mut self.pulses);
                    } else {
                        self.detector.probe_eof();
                    }
                    return Ok(());
                }
            };
            if real {
                self.detector.push_read(sample, valid, &mut self.pulses);
            } else if self.detector.probe_read(sample, valid) {
                return Ok(());
            }
        }
    }

    pub fn finish(mut self) -> Pulses {
        self.detector.finish(&mut self.pulses);
        let mut sig = Pulses::new(self.rate, self.pulses, self.detector.initial_high());
        sig.declared = self.detector.pulse_count();
        sig
    }
}
