//! A square wave as a run-length list of pulse durations: `pulses[i]` is how
//! many samples the level holds before it flips, starting from `initial_high`.
//! [`PulseSource`] walks one without holding it.

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulses {
    /// Sample rate in Hz.
    pub rate: u32,
    /// Pulse lengths, in samples, alternating logical high/low.
    pub pulses: Vec<u32>,
    /// If true, the waveform starts at logical high.
    pub initial_high: bool,
    /// The pulse count the v2 header declares and the console reports:
    /// detector *calls*, which exceed `pulses.len()` when a call yields no
    /// pulse (see `detect::PulseDetector::pulse_count`).
    pub declared: u64,
}

/// A pulse stream a writer can walk, and walk **again**: a WAV header states
/// the length before the samples, so a writer measures on one pass and writes
/// on the next.
pub trait PulseSource {
    type Pulses<'a>: Iterator<Item = Result<u32>>
    where
        Self: 'a;

    /// A new walk from the first pulse.
    fn pulses(&self) -> Result<Self::Pulses<'_>>;
    fn rate(&self) -> u32;
    fn initial_high(&self) -> bool;
}

/// What one walk of a source found: enough to size a header with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Survey {
    /// Pulses in the stream, all of them.
    pub pulses: u64,
    /// Samples the file *declares*. The sum **stops at a zero-length pulse**;
    /// the rendering does not.
    pub total_samples: u64,
}

/// Walk a source once and measure it.
pub fn survey<S: PulseSource>(src: &S) -> Result<Survey> {
    let mut out = Survey {
        pulses: 0,
        total_samples: 0,
    };
    let mut summing = true;
    for pulse in src.pulses()? {
        let pulse = pulse?;
        out.pulses += 1;
        if pulse == 0 {
            summing = false;
        }
        if summing {
            out.total_samples += pulse as u64;
        }
    }
    Ok(out)
}

impl PulseSource for Pulses {
    type Pulses<'a> = std::iter::Map<std::slice::Iter<'a, u32>, fn(&u32) -> Result<u32>>;

    fn pulses(&self) -> Result<Self::Pulses<'_>> {
        fn ok(p: &u32) -> Result<u32> {
            Ok(*p)
        }
        Ok(self.pulses.iter().map(ok as fn(&u32) -> Result<u32>))
    }
    fn rate(&self) -> u32 {
        self.rate
    }
    fn initial_high(&self) -> bool {
        self.initial_high
    }
}

impl Pulses {
    pub fn new(rate: u32, pulses: Vec<u32>, initial_high: bool) -> Self {
        let declared = pulses.len() as u64;
        Pulses {
            rate,
            pulses,
            initial_high,
            declared,
        }
    }

    /// The total sample count, as [`survey`] measures it.
    pub fn total_samples(&self) -> u64 {
        survey(self)
            .expect("a walk of pulses in memory")
            .total_samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_count_stops_at_a_zero_and_the_walk_does_not() {
        let sig = Pulses::new(8000, vec![100, 0, 100, 100, 100], true);
        let s = survey(&sig).unwrap();
        assert_eq!(s.pulses, 5);
        assert_eq!(s.total_samples, 100);
        assert_eq!(sig.total_samples(), 100);
        let sig = Pulses::new(8000, vec![100, 100, 100, 100], true);
        assert_eq!(survey(&sig).unwrap().total_samples, 400);
    }
}
