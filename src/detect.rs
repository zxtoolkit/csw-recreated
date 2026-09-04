//! The pulse detector: samples in the 0..=255 byte domain to pulse lengths.

use crate::signal::Pulses;

/// The three passes a caller with a re-readable signal makes, made here over
/// one buffer: probe, probe, run. [`crate::encode::PulseEncoder::run`] is the
/// same loop against a file.
pub fn run_detector(bytes: &[u8]) -> (Vec<u32>, PulseDetector) {
    let mut det = PulseDetector::new();
    while !det.ready() {
        if !det.probe(bytes) {
            det.probe_eof();
        }
    }
    let mut out = Vec::new();
    det.push(bytes, &mut out);
    det.finish(&mut out);
    (out, det)
}

pub fn samples_to_pulses(rate: u32, samples: &[f64], midpoint: f64) -> Pulses {
    let (pulses, det) = run_detector(&to_byte_domain(samples, midpoint));
    let mut sig = Pulses::new(rate, pulses, det.initial_high());
    sig.declared = det.pulse_count();
    sig
}

/// The byte domain's midpoint: what a signal of no level converts to.
pub const MIDPOINT: f64 = 128.0;

/// Map a mono signal onto the 0..=255 byte domain the detector works in,
/// `y + 128` truncated toward zero: a slightly negative `y` is 127.
pub fn to_byte_chop(y: f64) -> u8 {
    if y >= 128.0 {
        255
    } else if y.is_nan() || y <= -128.0 {
        0
    } else {
        (y + 128.0).trunc() as u8
    }
}

pub fn to_byte_domain(samples: &[f64], midpoint: f64) -> Vec<u8> {
    samples
        .iter()
        .map(|&s| to_byte(s - midpoint + 128.0))
        .collect()
}

/// One filtered sample to a byte: nearest, ties to even, then 0 below 0 and
/// 255 above 255. A value outside the range of a 32-bit integer -- NaN, an
/// infinity, or a magnitude of 2^31 or more, all of which an unstable filter
/// produces -- converts to **0**, whichever way it overflowed.
/// That decides the polarity flag of a filter degenerate enough to explode.
pub(crate) fn to_byte(v: f64) -> u8 {
    let r = v.round_ties_even();
    if !(-2_147_483_648.0..2_147_483_648.0).contains(&r) {
        return 0;
    }
    r.clamp(0.0, 255.0) as u8
}

/// Detect pulses over a whole buffer. [`PulseDetector`] is the streaming
/// form and the one the conversion path uses; this exists for tests.
#[cfg(test)]
pub(crate) fn detect_pulses(s: &[u8]) -> Vec<u32> {
    run_detector(s).0
}

/// A pulse length as the format can carry it: a CSW pulse is a DWORD, and a
/// run past it -- four gigasamples of one level -- is clamped to it.
fn clamp_pulse(run: i64) -> u32 {
    run.clamp(0, u32::MAX as i64) as u32
}

/// The part of the detector's state rebuilt fresh for each of the three runs
/// the encoder makes, so every run starts from the same pristine state; only
/// the seeded direction differs (see [`PulseDetector`]). Everything that must
/// survive a run lives in [`Globals`] instead.
#[derive(Clone, Debug)]
struct Node {
    /// Current direction, 0 or 1, flipped on every emit.
    dir: u8,
    /// Level the current pulse starts from; 128 seeds it.
    last: u8,
    /// Samples of a reversal seen but not yet believed. This and `run` count
    /// samples and are 64-bit: a flat input holds one pulse open for as long
    /// as the input lasts.
    pending: i64,
    /// The start-up counter, seeded -2: a call returns nothing until three
    /// emits have happened, and from then on the routine is two pulses
    /// behind the signal.
    counter: i32,
    /// The last five emits' rise reference, run length and amplitude, newest
    /// last. What a call returns is entry 2 -- the pulse from two emits ago.
    refv_hist: [u8; 5],
    run_hist: [i64; 5],
    amp_hist: [i64; 5],
}

impl Node {
    fn fresh(dir: u8) -> Self {
        Node {
            dir,
            last: 0x80,
            pending: 0,
            counter: -2,
            refv_hist: [0; 5],
            run_hist: [0; 5],
            amp_hist: [0; 5],
        }
    }
}

/// The state a rewind does **not** restore: the second and third runs inherit
/// what the previous run left here.
#[derive(Clone, Debug)]
struct Globals {
    /// The "sample is valid" flag. Set by the format opener before the first
    /// run, cleared by the reader at end of input, and driven by the detector's
    /// own start-up machine. While clear, every sample read is an emit.
    valid: bool,
    /// The sample at the last emit; breaks the polarity tie.
    last_emit: u8,
    /// Level on entering the current rise, from which the deadband derives.
    /// Reverted to the value of two emits ago at every return.
    refv: i32,
    /// Level at the previous turning point; 128 seeds it.
    prev_turn: u8,
    /// The candidate turning point. It persists across calls, starting at
    /// 0: two paths emit without setting it -- a jump over 127, and an
    /// invalid sample -- and use what the previous call left. Do not reset
    /// it per call.
    cand: u8,
}

impl Globals {
    fn fresh() -> Self {
        Globals {
            valid: true,
            last_emit: 0,
            refv: 0,
            prev_turn: 128,
            cand: 0,
        }
    }
}

/// One run of the detector routine, as a state machine fed a sample at a time.
///
/// The routine is a loop that pulls samples and returns one pulse per call,
/// turned inside out: [`Run::feed`] is one read and everything up to the next
/// read or return; [`Run::pump`] a call that needs no read (the drain, once
/// the counter passes 1). Keep the split: the drain emits without reading.
#[derive(Debug)]
struct Run {
    node: Node,
    /// Samples in the pulse being measured.
    run: i64,
    /// Total movement within it, for the 13% test.
    totvar: i64,
    /// Amplitude of the previous pulse, the yardstick for the 13% test.
    amp: i64,
    /// "Emit after this sample": a local of the call that survives from one
    /// sample to the next, and is only set on some paths.
    emit_flag: bool,
    /// The sample last read.
    cur: u8,
}

impl Run {
    fn new(dir: u8) -> Self {
        let mut r = Run {
            node: Node::fresh(dir),
            run: 0,
            totvar: 0,
            amp: 0,
            emit_flag: false,
            cur: 0,
        };
        r.begin_call();
        r
    }

    /// What the routine does on entry: the locals for one call.
    fn begin_call(&mut self) {
        self.totvar = 0;
        self.amp = self.node.amp_hist[2];
        self.run = self.node.pending;
        self.emit_flag = false;
        self.node.pending = 0;
        self.cur = self.node.last;
    }

    /// The next call would emit without reading.
    fn needs_no_input(&self) -> bool {
        self.node.counter > 1
    }

    /// One read -- `Some(sample)`, or `None` at the end of input -- and what
    /// follows it. A value the call returns is pushed to `out` (a zero
    /// included; the driver decides what a zero means).
    fn feed(&mut self, g: &mut Globals, input: Option<u8>, out: &mut Vec<i64>) {
        let prev = self.cur;
        let cur = match input {
            Some(s) => s,
            None => {
                g.valid = false;
                0
            }
        };
        self.cur = cur;
        if g.valid {
            self.process(g, prev, cur);
            if !self.emit_flag {
                return;
            }
        }
        self.settle(g, out);
    }

    /// A call that emits without reading: taken while the counter is past 1.
    fn pump(&mut self, g: &mut Globals, out: &mut Vec<i64>) {
        self.settle(g, out);
    }

    /// The hysteresis proper, for one valid sample. Sets `emit_flag` on the
    /// paths that decide -- and leaves it alone on the ones that do not.
    fn process(&mut self, g: &mut Globals, prev: u8, cur: u8) {
        let delta = i32::from(cur) - i32::from(prev);
        self.totvar += i64::from(delta.abs());
        if delta.abs() > 127 {
            // A jump that large is an edge on its own.
            self.run += self.node.pending;
            self.node.pending = 1;
            self.emit_flag = true;
            return;
        }
        if cur == prev {
            self.run += 1;
            self.emit_flag = false;
            return;
        }
        let rising = u8::from(delta > 0);
        if rising == 1 && self.node.dir == 0 {
            g.refv = i32::from(prev);
        }
        let d = ((128 - g.refv).abs() / 6).min(4);
        let m: i64 = if (g.refv - 128).abs() > 6 { 2 } else { 1 };
        if rising != self.node.dir {
            g.cand = cur;
            self.node.pending += 1;
            if self.node.pending <= m {
                // reversal not held long enough to be believed yet
            } else if self.totvar as f64 > self.amp as f64 * 0.13 {
                self.emit_flag = true;
            } else {
                // movement too small relative to the last real pulse
                self.run += 1;
                self.node.pending -= 1;
            }
        } else {
            let pend = self.node.pending;
            self.run += pend + 1;
            if pend != 0 {
                self.node.pending = 0;
            }
            let ad = delta.abs();
            if ad >= 128 - 4 * d && ad > d {
                if cur == prev {
                    self.emit_flag = false;
                } else {
                    g.cand = cur;
                    self.emit_flag = true;
                }
            } else {
                self.emit_flag = false;
            }
        }
    }

    /// An emit: record the pulse, then run the start-up/drain counter, which
    /// decides whether the call reads on or returns. A following call that
    /// would emit without reading is [`Run::pump`]'s, one at a time, so the
    /// driver acts on each value between.
    fn settle(&mut self, g: &mut Globals, out: &mut Vec<i64>) {
        let cur = self.cur;
        self.node.dir ^= 1;
        self.node.last = cur;
        g.last_emit = cur;
        self.amp = (i64::from(g.prev_turn) - i64::from(cur)).abs();
        g.prev_turn = g.cand;
        let n = &mut self.node;
        for i in 1..5 {
            n.run_hist[i - 1] = n.run_hist[i];
            n.refv_hist[i - 1] = n.refv_hist[i];
            n.amp_hist[i - 1] = n.amp_hist[i];
        }
        n.run_hist[4] = self.run;
        n.refv_hist[4] = g.refv as u8;
        self.run = if g.valid { n.pending } else { 0 };
        n.amp_hist[4] = self.amp;
        let c = n.counter;
        if c == 1 && g.valid {
            // steady state: one pulse per call
        } else if n.pending != 0 && c > 0 {
            g.valid = true;
        } else {
            let c = c + 1;
            n.counter = c;
            if c <= 1 {
                // still priming: keep reading within this call
            } else if c > 3 {
                g.valid = false;
                n.counter = 4;
            } else {
                g.valid = true;
            }
        }
        if n.counter <= 0 {
            // loop again inside the same call
            n.pending = 0;
            return;
        }
        g.refv = i32::from(n.refv_hist[2]);
        out.push(n.run_hist[2]);
        self.begin_call();
    }
}

/// Which of the three runs is in progress.
#[derive(Debug, PartialEq, Eq)]
enum Stage {
    /// The two polarity probes: the first with direction 0, the second with
    /// direction 1, each stopped at its first returned value.
    Probe(u8),
    /// The run whose pulses are written.
    Real,
}

/// Recover pulses from a sampled signal.
///
/// Three runs: seeded 0, then after a rewind seeded 1, each giving its first
/// pulse length. Those decide the header's polarity: high if the first is
/// longer, low if shorter, on a tie high iff the sample at the last emit was
/// below 128. A third run, seeded to the *opposite* of that, is the one
/// written. The node is restored for each run; `Globals` is not.
///
/// The routine returns the pulse from two emits ago: a counter seeded -2
/// primes it, a clear "sample is valid" flag makes every sample an emit, and
/// the end of input drains it. Fewer than three emits leaves the flag clear
/// for the real run, which then writes nothing.
///
/// A jump over 127 is an edge; any other reversal must hold `m` samples and
/// move more than 13% of the previous pulse's amplitude, `m` and the deadband
/// `d` scaled from the level the rise started at.
///
/// The caller reads the signal three times from the start; this holds none
/// of it.
#[derive(Debug)]
pub struct PulseDetector {
    g: Globals,
    run: Run,
    stage: Stage,
    /// The first probe's result, awaiting the second's.
    p1: i64,
    initial_high: Option<bool>,
    /// Calls the real run has returned from: the pulse count printed on the
    /// console and stamped in the header, which is not always the number of
    /// pulses written (see [`pulse_count`]).
    ///
    /// [`pulse_count`]: PulseDetector::pulse_count
    calls: u64,
    /// The detector loop has ended: a call returned with the flag clear.
    done: bool,
    /// Values returned by the routine, awaiting interpretation.
    raw: Vec<i64>,
    /// A VOC's reader sets the valid-sample flag per sample where a WAV's and
    /// an IFF's opener sets it once. It shows only on a signal too short for
    /// the start-up: a VOC writes one pulse where the same samples in a WAV
    /// write none.
    valid_per_sample: bool,
    /// Zero the stale candidate turning point after every pulse written.
    /// Set for Z-RLE output and clear for plain RLE, so the two paths can
    /// return different pulse counts for one input.
    zero_stale_cand: bool,
}

impl Default for PulseDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PulseDetector {
    pub fn new() -> Self {
        PulseDetector {
            g: Globals::fresh(),
            run: Run::new(0),
            stage: Stage::Probe(1),
            p1: 0,
            initial_high: None,
            calls: 0,
            done: false,
            raw: Vec::new(),
            valid_per_sample: false,
            zero_stale_cand: false,
        }
    }

    /// Model the frame the Z-RLE writer leaves under the detector between
    /// calls (see `zero_stale_cand`). Off for plain-RLE and v1 output.
    pub fn with_zrle_frame(mut self, zrle: bool) -> Self {
        self.zero_stale_cand = zrle;
        self
    }

    /// Read the valid-sample flag from every sample, as a VOC's reader does
    /// (see `valid_per_sample`). Off for WAV and IFF.
    pub fn with_valid_per_sample(mut self, on: bool) -> Self {
        self.valid_per_sample = on;
        self
    }

    /// The header's initial polarity: decided by the two probes, so known
    /// once the real run has started, and always after [`finish`].
    ///
    /// [`finish`]: PulseDetector::finish
    pub fn initial_high(&self) -> bool {
        self.initial_high.unwrap_or(false)
    }

    /// The pulse count reported on the console and stamped in the header.
    ///
    /// It counts *calls* to the detector, not pulses written, and the two
    /// differ when a call returns 0 -- so an input too short to yield any
    /// pulse is still reported as "1 pulses". Meaningful after [`finish`].
    ///
    /// [`finish`]: PulseDetector::finish
    pub fn pulse_count(&self) -> u64 {
        self.calls.max(1)
    }

    /// True once both probes have answered and [`push`] is what to call.
    ///
    /// [`push`]: PulseDetector::push
    pub fn ready(&self) -> bool {
        matches!(self.stage, Stage::Real)
    }

    /// Feed samples to whichever probe is running.
    ///
    /// `true` means that probe has answered and wants no more of this pass:
    /// rewind to the first sample and call again -- for the second probe, or,
    /// once [`ready`], to feed [`push`] the whole signal from the start.
    ///
    /// [`ready`]: PulseDetector::ready
    /// [`push`]: PulseDetector::push
    pub fn probe(&mut self, samples: &[u8]) -> bool {
        for &s in samples {
            if self.feed_probe(Some(s), None) {
                return true;
            }
        }
        false
    }

    /// The pass ran out of samples before the probe answered. Reading
    /// continues past the end, and each read at the end is an emit, so this
    /// always answers.
    pub fn probe_eof(&mut self) {
        // Only the probe that ran out of samples reads on past the end; the
        // next starts from the first sample after the caller rewinds. Stop as
        // soon as *this* probe has answered: end-of-input fed to the second
        // probe as well decides the tie-break wrongly on a signal with fewer
        // than three edges.
        let Stage::Probe(which) = self.stage else {
            return;
        };
        let mut guard = 0;
        while matches!(self.stage, Stage::Probe(w) if w == which) && guard < 64 {
            self.feed_probe(None, None);
            guard += 1;
        }
        debug_assert!(
            !matches!(self.stage, Stage::Probe(w) if w == which),
            "a probe did not answer at end of input"
        );
    }

    /// Feed samples to the real run, appending every pulse that completes
    /// within them. Call only once [`ready`], and from the first sample.
    ///
    /// [`ready`]: PulseDetector::ready
    pub fn push(&mut self, samples: &[u8], out: &mut Vec<u32>) {
        if self.done || !self.ready() {
            return;
        }
        for &s in samples {
            self.step(Some(s), None, out);
            if self.done {
                break;
            }
        }
    }

    pub fn probe_read(&mut self, sample: u8, valid: bool) -> bool {
        self.feed_probe(Some(sample), Some(valid))
    }

    pub fn push_read(&mut self, sample: u8, valid: bool, out: &mut Vec<u32>) {
        if self.done || !self.ready() {
            return;
        }
        self.step(Some(sample), Some(valid), out);
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    fn set_valid(&mut self, input: Option<u8>, valid: Option<bool>) {
        match valid {
            Some(v) => self.g.valid = v,
            None => {
                if self.valid_per_sample && input.is_some() {
                    self.g.valid = true;
                }
            }
        }
    }

    /// End of signal: complete the probes if the input was short, then drain
    /// the real run's two-pulse backlog.
    pub fn finish(&mut self, out: &mut Vec<u32>) {
        if self.done {
            return;
        }
        // Each read at end of input is an emit, and the counter machine
        // bounds how many it takes before a zero comes back with the flag
        // clear; the guard is only against looping forever.
        let mut guard = 0;
        while !self.done && guard < 64 {
            self.step(None, None, out);
            guard += 1;
        }
        debug_assert!(self.done, "end-of-input drain did not terminate");
        self.done = true;
    }

    /// One sample into the running probe. `true` when that probe has just
    /// answered, which is the caller's cue to rewind.
    ///
    /// The globals carry across the probes and into the real run: a rewind
    /// is of the *signal*, not of this state.
    fn feed_probe(&mut self, input: Option<u8>, valid: Option<bool>) -> bool {
        let Stage::Probe(which) = self.stage else {
            return false;
        };
        self.set_valid(input, valid);
        self.run.feed(&mut self.g, input, &mut self.raw);
        if self.raw.is_empty() {
            return false;
        }
        // A probe makes exactly one call: its first value is the answer.
        let v = self.raw[0];
        self.raw.clear();
        if which == 1 {
            self.p1 = v;
            self.stage = Stage::Probe(2);
            self.run = Run::new(1);
            return true;
        }
        let high = if self.p1 > v {
            true
        } else if self.p1 < v {
            false
        } else {
            self.g.last_emit < 128
        };
        self.initial_high = Some(high);
        self.run = Run::new(u8::from(!high));
        self.stage = Stage::Real;
        true
    }

    /// One read into the real run, then every call it makes without reading,
    /// interpreting each returned value as it comes.
    fn step(&mut self, input: Option<u8>, valid: Option<bool>, out: &mut Vec<u32>) {
        self.set_valid(input, valid);
        self.run.feed(&mut self.g, input, &mut self.raw);
        self.take(out);
        while !self.done && self.run.needs_no_input() {
            self.run.pump(&mut self.g, &mut self.raw);
            self.take(out);
        }
    }

    /// A returned value: a pulse if nonzero, skipped if zero. The loop then
    /// goes on only while the flag is set, so a value returned by a call that
    /// met the end of input is the last one taken.
    fn take(&mut self, out: &mut Vec<u32>) {
        for v in self.raw.drain(..) {
            self.calls += 1;
            if v != 0 {
                out.push(clamp_pulse(v));
                if self.zero_stale_cand {
                    self.g.cand = 0;
                }
            }
            if !self.g.valid {
                self.done = true;
                break;
            }
        }
        self.raw.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value the 32-bit integer conversion cannot hold -- an infinity, a
    /// NaN, a magnitude of 2^31 -- converts to 0, whichever way it overflowed;
    /// only a finite value out of range saturates.
    #[test]
    fn to_byte_sends_the_indefinite_to_zero() {
        assert_eq!(to_byte(f64::INFINITY), 0);
        assert_eq!(to_byte(f64::NEG_INFINITY), 0);
        assert_eq!(to_byte(f64::NAN), 0);
        assert_eq!(to_byte(2_147_483_648.0), 0);
        assert_eq!(to_byte(-2_147_483_649.0), 0);
        assert_eq!(to_byte(300.0), 255);
        assert_eq!(to_byte(-5.0), 0);
        assert_eq!(to_byte(127.5), 128);
    }

    /// A tape-like signal: square-ish pulses of varying width with noise on
    /// top, from a fixed LCG so the test is deterministic.
    fn pseudo_tape(n: usize) -> Vec<u8> {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut out = Vec::with_capacity(n);
        let (mut high, mut left) = (true, 0usize);
        while out.len() < n {
            if left == 0 {
                left = 4 + (next() % 40) as usize;
                high = !high;
            }
            let base = if high { 200 } else { 56 };
            let noise = (next() % 17) as i32 - 8;
            out.push((base + noise).clamp(0, 255) as u8);
            left -= 1;
        }
        out
    }

    /// The driver's three passes, with every pass -- probes included -- fed
    /// in pieces of `size`. `None` walks ragged boundaries instead, since a
    /// streamed read is not a fixed size.
    fn detect_in_chunks(signal: &[u8], size: Option<usize>) -> Vec<u32> {
        // A fresh cut of the same signal for each pass: every pass restarts
        // at the first sample, and the ragged one has to restart its rhythm
        // with it.
        fn pieces(signal: &[u8], size: Option<usize>) -> Vec<&[u8]> {
            match size {
                Some(n) => signal.chunks(n).collect(),
                None => {
                    let (mut at, mut step, mut out) = (0usize, 1usize, Vec::new());
                    while at < signal.len() {
                        let end = (at + step).min(signal.len());
                        out.push(&signal[at..end]);
                        at = end;
                        step = step * 3 % 251 + 1;
                    }
                    out
                }
            }
        }
        let mut det = PulseDetector::new();
        while !det.ready() {
            let mut answered = false;
            for piece in pieces(signal, size) {
                if det.probe(piece) {
                    answered = true;
                    break;
                }
            }
            if !answered {
                det.probe_eof();
            }
        }
        let mut got = Vec::new();
        for piece in pieces(signal, size) {
            det.push(piece, &mut got);
        }
        det.finish(&mut got);
        got
    }

    /// The detector carries its state across buffer boundaries, so feeding a
    /// signal in chunks -- the probe passes included -- emits exactly what
    /// feeding it whole does.
    #[test]
    fn chunked_detection_matches_the_whole_buffer() {
        let signal = pseudo_tape(20_000);
        let whole = detect_pulses(&signal);
        assert!(whole.len() > 100, "test signal has no pulses");
        for size in [1usize, 2, 3, 7, 64, 997, 8192, 32_768] {
            assert_eq!(
                detect_in_chunks(&signal, Some(size)),
                whole,
                "chunk size {size}"
            );
        }
        assert_eq!(detect_in_chunks(&signal, None), whole, "ragged chunks");
    }

    /// Samples are quantised through this on their way to an 8-bit file, so
    /// out-of-range samples have to clamp, not wrap.
    #[test]
    fn byte_domain_quantises_and_clamps() {
        let got = to_byte_domain(&[-5.0, 0.0, 127.6, 128.0, 300.0], 128.0);
        assert_eq!(got, vec![0, 0, 128, 128, 255]);
    }

    #[test]
    fn dither_around_the_midpoint_makes_no_edges() {
        // 8-bit silence flips between 127 and 128 every sample. A reversal
        // must hold, and a one-sample flip never does.
        let mut s = vec![127u8, 128, 127, 128, 127, 128, 127, 128];
        s.extend([200; 6]); // one real excursion
        s.extend([128, 127, 128, 127]);
        let pulses = detect_pulses(&s);
        assert!(
            pulses.len() <= 4,
            "dither must not become a pulse train, got {pulses:?}"
        );
        assert_eq!(pulses.iter().sum::<u32>() as usize, s.len());
        // The exact pulses these bytes produce, priming and two-pulse
        // delay included.
        assert_eq!(pulses, vec![14, 2, 1, 1]);
    }

    /// Three edges are needed before anything is written: on a shorter
    /// input the two polarity probes read to the end and leave the
    /// valid-sample flag clear, so the real run emits nothing -- while still
    /// reporting one pulse.
    #[test]
    fn fewer_than_three_edges_yields_nothing() {
        for s in [
            vec![255u8; 10],
            [[255u8; 10].as_slice(), &[0; 10]].concat(),
            [[255u8; 10].as_slice(), &[0; 10], &[255; 10]].concat(),
            Vec::new(),
        ] {
            let (out, det) = run_detector(&s);
            assert_eq!(out, Vec::<u32>::new(), "input {s:?}");
            assert_eq!(det.pulse_count(), 1);
            assert!(det.initial_high());
        }
        // Four edges are enough, and every pulse comes out.
        let s = [[255u8; 10].as_slice(), &[0; 10], &[255; 10], &[0; 10]].concat();
        assert_eq!(detect_pulses(&s), vec![10, 10, 10, 10]);
    }

    #[test]
    fn full_scale_square_is_recovered_exactly() {
        // A jump over 127 is an edge on its own, so a rendered square wave
        // round-trips to the run lengths that produced it.
        let runs = [7u32, 5, 11, 3, 9];
        let mut s = Vec::new();
        let mut hi = true;
        for &r in &runs {
            s.extend(std::iter::repeat_n(if hi { 255u8 } else { 0 }, r as usize));
            hi = !hi;
        }
        assert_eq!(detect_pulses(&s), runs);
    }
}
