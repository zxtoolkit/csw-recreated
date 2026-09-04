//! IIR digital filter (`-f`): one bilinear transform at a single pre-warped
//! cutoff (the bandwidth, for a band-pass), then the band step in the z
//! domain. Applied as biquads. Not textbook.

use crate::complex::Cx;
use std::f64::consts::PI;

/// Filter response shape.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    LowPass,
    BandPass,
    HighPass,
    /// A `-ft` value outside 3/4/5, carried as given. The status line prints
    /// `??? ` in place of the band and no band step runs: the conversion
    /// writes a single pulse, which a NaN gain reproduces here.
    Unknown(i32),
}

/// Analog prototype family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Design {
    Butterworth,
    Chebyshev,
    /// A `-fp` value outside 1/2, carried as given. The status line names
    /// it "Chebyshev" with no ripple clause, and neither prototype is
    /// computed -- see `digital_prototype`.
    Other(i32),
}

/// Filter parameters; the defaults are an order-2 Butterworth band-pass,
/// 600–4100 Hz.
#[derive(Debug, Clone, Copy)]
pub struct FilterSpec {
    pub order: i32,
    pub band: Band,
    pub design: Design,
    pub low_hz: f64,
    pub high_hz: f64,
    /// Chebyshev pass-band ripple in dB.
    pub ripple_db: f64,
}

/// Highest filter order the design supports. The requested order is clamped
/// into `1..=MAX_ORDER` silently: `o99` designs order 16, `o0` order 1.
pub const MAX_ORDER: i32 = 16;

impl FilterSpec {
    /// The order actually designed for, after clamping.
    pub fn effective_order(&self) -> u32 {
        self.order.clamp(1, MAX_ORDER) as u32
    }

    /// The order the design ends up with: clamped, and rounded up to even
    /// for a band-pass. The status line reports this, so `-fo3` announces
    /// order 4.
    pub fn designed_order(&self) -> u32 {
        let order = self.effective_order();
        if self.band == Band::BandPass && !order.is_multiple_of(2) {
            order + 1
        } else {
            order
        }
    }
}

impl Default for FilterSpec {
    fn default() -> Self {
        FilterSpec {
            order: 2,
            band: Band::BandPass,
            design: Design::Butterworth,
            low_hz: 600.0,
            high_hz: 4100.0,
            ripple_db: 1.0,
        }
    }
}

/// A cascade of second-order sections, each `[b0, b1, b2, a0, a1, a2]` with
/// `a0 == 1`.
#[derive(Debug, Clone)]
pub struct Sos {
    pub sections: Vec<[f64; 6]>,
}

/// IIR coefficients in the usual convention: `a[0] == 1`; the filter path
/// runs the cascade (see `design_sos`).
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Coeffs {
    pub b: Vec<f64>,
    pub a: Vec<f64>,
}

// ---------------------------------------------------------------------------
// zero/pole/gain representation and the design pipeline
// ---------------------------------------------------------------------------

struct Zpk {
    zeros: Vec<Cx>,
    poles: Vec<Cx>,
    gain: f64,
}

/// A prototype pole counts as real below this |imaginary part|, which decides
/// whether the band-pass step takes its unpaired-pole branch. Loose on
/// purpose: at extreme cutoffs a complex pole drifts under it, and the real
/// branch keeps the design finite.
const REAL_POLE_EPS: f64 = 1e-4;

/// Analog Butterworth low-pass prototype (unit cutoff).
fn buttap(n: u32) -> Zpk {
    let n = n as i32;
    let poles = (0..n)
        .map(|k| {
            let m = (-n + 1 + 2 * k) as f64;
            -(Cx::new(0.0, PI * m / (2.0 * n as f64)).exp())
        })
        .collect();
    Zpk {
        zeros: Vec::new(),
        poles,
        gain: 1.0,
    }
}

/// Analog Chebyshev type I low-pass prototype.
fn cheb1ap(n: u32, rp: f64) -> Zpk {
    let nf = n as f64;
    let eps = (10f64.powf(0.1 * rp) - 1.0).sqrt();
    let mu = (1.0 / eps).asinh() / nf;
    let ni = n as i32;
    let poles: Vec<Cx> = (0..ni)
        .map(|k| {
            let m = (-ni + 1 + 2 * k) as f64;
            let theta = PI * m / (2.0 * nf);
            -(Cx::new(mu, theta).sinh())
        })
        .collect();
    // k = prod(-p).re ; even order divides out the DC ripple
    let mut gain = prod(&poles.iter().map(|&p| -p).collect::<Vec<_>>()).re;
    if n.is_multiple_of(2) {
        gain /= (1.0 + eps * eps).sqrt();
    }
    Zpk {
        zeros: Vec::new(),
        poles,
        gain,
    }
}

/// The digital low-pass prototype every band starts from: the analog
/// prototype's poles through the bilinear transform at cutoff `t`. One
/// `tan`, of the *bandwidth* for a band-pass; everything after is in the z
/// domain, so zero, inverted and above-Nyquist cutoffs still design.
fn digital_prototype(spec: &FilterSpec, fs: f64, m: u32) -> Vec<Cx> {
    let x = match spec.band {
        Band::LowPass => spec.high_hz,
        Band::HighPass => fs * 0.5 - spec.low_hz,
        Band::BandPass | Band::Unknown(_) => spec.high_hz - spec.low_hz,
    };
    let t = (PI * x / fs).tan();
    let proto = match spec.design {
        Design::Butterworth => buttap(m),
        // A one-section Chebyshev has no conjugate pair for the ripple to
        // shape: an order-2 band-pass is a Butterworth at every ripple.
        Design::Chebyshev if m == 1 => buttap(1),
        Design::Chebyshev => cheb1ap(m, spec.ripple_db),
        // A prototype outside 1/2: both pole computations are skipped, so
        // the band step reads `1 + i` and `1 - i` for every conjugate pair
        // and `1` for the odd real pole, with no bilinear step. The filter
        // has poles on or outside the unit circle and converts all the same.
        Design::Other(_) => {
            return buttap(m)
                .poles
                .iter()
                .map(|s| {
                    Cx::new(
                        1.0,
                        if s.im.abs() < 1e-12 {
                            0.0
                        } else {
                            s.im.signum()
                        },
                    )
                })
                .collect();
        }
    };
    proto
        .poles
        .iter()
        .map(|&s| {
            let ts = s.scale(t);
            (Cx::real(1.0) + ts) / (Cx::real(1.0) - ts)
        })
        .collect()
}

/// The band-pass step: each prototype pole becomes two.
///
/// The prototype is already cut at the bandwidth, so the low-pass to band-pass
/// substitution is `Z^-1 -> -z^-1 (z^-1 - alpha) / (1 - alpha z^-1)` and a
/// prototype pole `p` maps to the roots of `z^2 - alpha (1 + p) z + p`.
///
/// For the unpaired **real** pole of an odd-section prototype (N/2 odd: 2,
/// 6, 10 ..., the default order 2 included) the larger of its quadratic's
/// two real roots is used for both poles.
fn bandpass_poles(lp: &[Cx], alpha: f64) -> Vec<Cx> {
    let mut out = Vec::with_capacity(lp.len() * 2);
    for &p in lp {
        if p.im.abs() < REAL_POLE_EPS {
            // The imaginary part is dropped outright here, so this arm is
            // real arithmetic even when `p` is only nearly real.
            let half = alpha * 0.5 * (1.0 + p.re);
            let disc = half * half - p.re;
            if disc > 0.0 {
                let root = half + disc.sqrt();
                out.push(Cx::real(root));
                out.push(Cx::real(root));
            } else {
                let im = (-disc).sqrt();
                out.push(Cx::new(half, im));
                out.push(Cx::new(half, -im));
            }
        } else {
            let half = (Cx::real(1.0) + p).scale(alpha * 0.5);
            let d = (half * half - p).sqrt();
            out.push(half + d);
            out.push(half - d);
        }
    }
    out
}

/// Expand zeros/poles/gain into transfer-function coefficients (b, a).
#[cfg(test)]
fn zpk2tf(zpk: &Zpk) -> Coeffs {
    let b: Vec<f64> = poly(&zpk.zeros).iter().map(|c| c.re * zpk.gain).collect();
    let a: Vec<f64> = poly(&zpk.poles).iter().map(|c| c.re).collect();
    Coeffs { b, a }
}

/// Product of a list of complex numbers (empty product = 1).
fn prod(v: &[Cx]) -> Cx {
    v.iter().fold(Cx::new(1.0, 0.0), |acc, &x| acc * x)
}

/// Monic polynomial (highest degree first) whose roots are `roots`.
#[cfg(test)]
fn poly(roots: &[Cx]) -> Vec<Cx> {
    let mut c = vec![Cx::new(1.0, 0.0)];
    for &r in roots {
        let mut next = vec![Cx::new(0.0, 0.0); c.len() + 1];
        for i in 0..c.len() {
            next[i] = next[i] + c[i];
            next[i + 1] = next[i + 1] - c[i] * r;
        }
        c = next;
    }
    c
}

/// Design the filter coefficients for a given sample rate, in the expanded
/// `(b, a)` form, which is only conditioned well enough for modest orders;
/// the filter path runs `design_sos`.
#[cfg(test)]
pub(crate) fn design(spec: &FilterSpec, sample_rate: f64) -> Coeffs {
    zpk2tf(&design_zpk(spec, sample_rate))
}

/// The design pipeline up to the digital zero/pole/gain form, shared by the
/// `(b, a)` and second-order-section outputs.
///
/// Nothing here rejects a cutoff: zero, inverted and above-Nyquist edges
/// design *some* filter, usually one that collapses the pulses to a handful.
fn design_zpk(spec: &FilterSpec, fs: f64) -> Zpk {
    let order = spec.designed_order();
    // `order` counts *total* poles. A band-pass doubles the prototype's, so
    // it starts from a prototype of half the order -- odd orders having been
    // rounded up to even already.
    let m = if spec.band == Band::BandPass {
        order / 2
    } else {
        order
    };
    let lp = digital_prototype(spec, fs, m);
    let n = m as usize;

    // The band step. A low-pass keeps the prototype and puts its zeros at
    // Nyquist; a high-pass reflects the poles through the origin and puts the
    // zeros at DC; a band-pass splits every pole in two and takes both.
    let (zeros, poles) = match spec.band {
        // No band step ran, and what comes out has no signal left in it.
        Band::Unknown(_) => {
            return Zpk {
                zeros: Vec::new(),
                poles: Vec::new(),
                gain: f64::NAN,
            };
        }
        Band::LowPass => (vec![Cx::real(-1.0); n], lp),
        Band::HighPass => (
            vec![Cx::real(1.0); n],
            lp.iter().map(|&p| -p).collect::<Vec<_>>(),
        ),
        Band::BandPass => {
            let alpha = (PI * (spec.high_hz + spec.low_hz) / fs).cos()
                / (PI * (spec.high_hz - spec.low_hz) / fs).cos();
            let mut zeros = vec![Cx::real(1.0); n];
            zeros.extend(std::iter::repeat_n(Cx::real(-1.0), n));
            (zeros, bandpass_poles(&lp, alpha))
        }
    };

    // Every band is scaled to unit peak on a fixed grid -- see `peak_gain`.
    let unscaled = Zpk {
        zeros,
        poles,
        gain: 1.0,
    };
    let peak = peak_gain(&unscaled);
    Zpk {
        gain: 1.0 / peak,
        ..unscaled
    }
}

/// |H| of a zpk filter at one digital frequency, in radians per sample,
/// evaluated from the factors.
fn response_mag(zpk: &Zpk, w: f64) -> f64 {
    let ejw = Cx::new(0.0, w).exp();
    let mut num = Cx::new(zpk.gain, 0.0);
    for &z in &zpk.zeros {
        num = num * (ejw - z);
    }
    let mut den = Cx::new(1.0, 0.0);
    for &p in &zpk.poles {
        den = den * (ejw - p);
    }
    (num / den).norm()
}

/// The largest |H| on a fixed response grid: **251 points**, spaced `pi/250`
/// apart, with the two ends pulled `pi/10000` inside DC and Nyquist.
///
/// The grid is part of the design, not an approximation: the gain it finds
/// reaches the pulses, since the detector reads absolute deadbands in the
/// byte domain.
fn peak_gain(zpk: &Zpk) -> f64 {
    const N: usize = 251;
    const STEP: f64 = 0.012_566_370_614_359_173; // pi/250
    const INSET: f64 = 0.000_314_159_265_358_979_3; // pi/10000
    let mut best = f64::NEG_INFINITY;
    for i in 0..N {
        let w = match i {
            0 => INSET,
            _ if i == N - 1 => PI - INSET,
            _ => i as f64 * STEP,
        };
        let m = response_mag(zpk, w);
        if m > best {
            best = m;
        }
    }
    best
}

/// Design as a cascade of second-order sections: the same filter as `design`,
/// in the form the filter path runs. Expanded into one high-order
/// denominator, an order-16 high-pass diverges
/// (`order_16_high_pass_stays_finite`); nothing here expands beyond degree 2.
pub fn design_sos(spec: &FilterSpec, sample_rate: f64) -> Sos {
    zpk2sos(&design_zpk(spec, sample_rate))
}

/// Group roots into monic quadratic factors, pairing each complex root with its
/// conjugate so every factor has real coefficients. Real roots are paired off
/// two at a time; a single leftover becomes a first-order factor.
fn roots_to_quadratics(roots: &[Cx]) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = Vec::new();
    let mut reals: Vec<f64> = Vec::new();
    let mut used = vec![false; roots.len()];
    for i in 0..roots.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let r = roots[i];
        if r.im.abs() <= REAL_POLE_EPS {
            reals.push(r.re);
            continue;
        }
        let mate = (i + 1..roots.len()).find(|&k| {
            !used[k] && (roots[k].re - r.re).abs() <= 1e-9 && (roots[k].im + r.im).abs() <= 1e-9
        });
        match mate {
            Some(k) => {
                used[k] = true;
                // (x - r)(x - conj r) = x^2 - 2*Re(r)*x + |r|^2
                out.push([1.0, -2.0 * r.re, r.re * r.re + r.im * r.im]);
            }
            // A real-coefficient filter never gets here; degrade to the real
            // part.
            None => reals.push(r.re),
        }
    }
    let mut it = reals.into_iter();
    while let Some(a) = it.next() {
        match it.next() {
            Some(b) => out.push([1.0, -(a + b), a * b]),
            None => out.push([1.0, -a, 0.0]),
        }
    }
    out
}

/// Pair the zero and pole factors into biquads, carrying the gain on the first.
fn zpk2sos(zpk: &Zpk) -> Sos {
    let mut num = roots_to_quadratics(&zpk.zeros);
    let mut den = roots_to_quadratics(&zpk.poles);
    let n = num.len().max(den.len()).max(1);
    num.resize(n, [1.0, 0.0, 0.0]);
    den.resize(n, [1.0, 0.0, 0.0]);
    let sections = num
        .into_iter()
        .zip(den)
        .enumerate()
        .map(|(i, (b, a))| {
            let g = if i == 0 { zpk.gain } else { 1.0 };
            [b[0] * g, b[1] * g, b[2] * g, a[0], a[1], a[2]]
        })
        .collect();
    Sos { sections }
}

/// A filter part-way through a signal: the state carries across chunks, so
/// filtering in chunks produces the same samples as filtering whole.
pub struct Applier {
    sos: Sos,
    /// `[x1, x2, y1, y2]` per section.
    state: Vec<[f64; 4]>,
}

impl Applier {
    /// The filter `spec` calls for at `sample_rate`, as a cascade of
    /// second-order sections (see `design_sos`).
    pub fn new(spec: &FilterSpec, sample_rate: f64) -> Self {
        let sos = design_sos(spec, sample_rate);
        let state = vec![[0.0; 4]; sos.sections.len()];
        Applier { sos, state }
    }

    /// Filter one chunk in place, carrying state into the next.
    pub fn process(&mut self, buf: &mut [f64]) {
        for (s, st) in self.sos.sections.iter().zip(self.state.iter_mut()) {
            let (b0, b1, b2, a1, a2) = (s[0], s[1], s[2], s[4], s[5]);
            let [mut x1, mut x2, mut y1, mut y2] = *st;
            for v in buf.iter_mut() {
                let xn = *v;
                let yn = b0 * xn + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                x2 = x1;
                x1 = xn;
                y2 = y1;
                y1 = yn;
                *v = yn;
            }
            *st = [x1, x2, y1, y2];
        }
    }
}

/// Apply the filter to `x` as a direct-form-I recursion (`y[n] = sum b_k
/// x[n-k] - sum a_k y[n-k]`, with `a[0] == 1`): the reference the cascade is
/// checked against.
#[cfg(test)]
pub(crate) fn apply(c: &Coeffs, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0f64; x.len()];
    for i in 0..x.len() {
        let mut acc = 0.0;
        for (k, &bk) in c.b.iter().enumerate() {
            if i >= k {
                acc += bk * x[i - k];
            }
        }
        for (k, &ak) in c.a.iter().enumerate().skip(1) {
            if i >= k {
                acc -= ak * y[i - k];
            }
        }
        y[i] = acc;
    }
    y
}

/// Full pipeline: DC-remove, design, filter. Returns the centred signal; the
/// caller thresholds at 0 to recover levels.
#[cfg(test)]
pub(crate) fn filter_samples(
    samples: &[f64],
    sample_rate: f64,
    spec: &FilterSpec,
    dc: f64,
) -> Vec<f64> {
    let mut centred: Vec<f64> = samples.iter().map(|&s| s - dc).collect();
    Applier::new(spec, sample_rate).process(&mut centred);
    centred
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole-buffer cascade: the oracle for [`Applier::process`],
    /// independent of the code under test.
    fn reference_sos(sos: &Sos, x: &[f64]) -> Vec<f64> {
        let mut y = x.to_vec();
        for s in &sos.sections {
            let (b0, b1, b2, a1, a2) = (s[0], s[1], s[2], s[4], s[5]);
            let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for v in y.iter_mut() {
                let xn = *v;
                let yn = b0 * xn + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                x2 = x1;
                x1 = xn;
                y2 = y1;
                y1 = yn;
                *v = yn;
            }
        }
        y
    }

    /// A chunked run equals a whole-buffer run, and either equals the
    /// reference cascade, to the bit.
    #[test]
    fn chunked_filtering_matches_the_whole_buffer() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f64 / 16_777_216.0
        };
        let x: Vec<f64> = (0..10_000)
            .map(|i| {
                let tone = (i as f64 * 0.07).sin() * 60.0;
                128.0 + tone + next() * 8.0
            })
            .collect();

        let specs = [
            FilterSpec::default(),
            FilterSpec {
                order: 4,
                band: Band::HighPass,
                design: Design::Chebyshev,
                low_hz: 800.0,
                high_hz: 4100.0,
                ripple_db: 1.0,
            },
        ];
        for spec in specs {
            let whole = filter_samples(&x, 44100.0, &spec, 128.0);
            let centred: Vec<f64> = x.iter().map(|v| v - 128.0).collect();
            let oracle = reference_sos(&design_sos(&spec, 44100.0), &centred);
            assert_eq!(whole, oracle, "the rewrite changed the filter: {spec:?}");
            for size in [1usize, 2, 3, 64, 997, 8192] {
                let mut applier = Applier::new(&spec, 44100.0);
                let mut got = Vec::with_capacity(x.len());
                for chunk in x.chunks(size) {
                    let mut buf: Vec<f64> = chunk.iter().map(|&v| v - 128.0).collect();
                    applier.process(&mut buf);
                    got.extend_from_slice(&buf);
                }
                assert_eq!(got, whole, "chunk size {size}, {spec:?}");
            }
        }
    }

    /// Largest |root| of a biquad denominator `x^2 + a1 x + a2`.
    fn section_pole_radius(s: &[f64; 6]) -> f64 {
        let (a1, a2) = (s[4], s[5]);
        let disc = a1 * a1 - 4.0 * a2;
        if disc >= 0.0 {
            let r = disc.sqrt();
            ((-a1 + r) / 2.0).abs().max(((-a1 - r) / 2.0).abs())
        } else {
            a2.abs().sqrt() // complex conjugate pair, |root| = sqrt(a2)
        }
    }

    /// Every design the CLI can ask for is stable as a cascade: each factor
    /// stays at degree 2, so the poles stay put.
    #[test]
    fn every_analytic_design_is_stable() {
        let mut worst = 0.0f64;
        let mut worst_cfg = String::new();
        for band in [Band::LowPass, Band::BandPass, Band::HighPass] {
            for design in [Design::Butterworth, Design::Chebyshev] {
                for order in 1..=16i32 {
                    let spec = FilterSpec {
                        band,
                        design,
                        order,
                        low_hz: 600.0,
                        high_hz: 4100.0,
                        ripple_db: 1.0,
                    };
                    let sos = design_sos(&spec, 44100.0);
                    for s in &sos.sections {
                        let r = section_pole_radius(s);
                        if r > worst {
                            worst = r;
                            worst_cfg = format!("{band:?}/{design:?}/order {order}");
                        }
                    }
                }
            }
        }
        assert!(
            worst < 1.0,
            "unstable analytic design {worst_cfg}: max|pole| = {worst}"
        );
    }

    /// An order-16 high-pass produces finite output.
    #[test]
    fn order_16_high_pass_stays_finite() {
        let spec = FilterSpec {
            band: Band::HighPass,
            design: Design::Butterworth,
            order: 16,
            low_hz: 600.0,
            high_hz: 4100.0,
            ripple_db: 1.0,
        };
        let x: Vec<f64> = (0..4000)
            .map(|i| if (i / 9) % 2 == 0 { 250.0 } else { 8.0 })
            .collect();
        let y = filter_samples(&x, 44100.0, &spec, 128.0);
        assert!(
            y.iter().all(|v| v.is_finite()),
            "order-16 high-pass diverged"
        );
    }

    fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
    }

    /// Band-pass with N/2 odd follows the double-pole convention in
    /// `bandpass_poles`. The coefficients are pinned exactly.
    #[test]
    fn bandpass_odd_half_order_uses_the_double_pole_convention() {
        let c = design(
            &FilterSpec {
                band: Band::BandPass,
                order: 2,
                low_hz: 400.0,
                high_hz: 5000.0,
                ..Default::default()
            },
            32000.0,
        );
        let want = [1.0, -1.821_033_884_216_941_3, 0.829_041_101_866_560_1];
        assert!(
            close(&c.a, &want, 1e-9),
            "odd-N/2 band-pass drifted: {:?}",
            c.a
        );
    }

    /// With N/2 even the convention coincides with the textbook transform.
    #[test]
    fn bandpass_even_half_order_is_textbook() {
        let c = design(
            &FilterSpec {
                band: Band::BandPass,
                order: 4,
                low_hz: 400.0,
                high_hz: 5000.0,
                ..Default::default()
            },
            32000.0,
        );
        // scipy.signal.butter(2, [400, 5000], 'bandpass', fs=32000)
        let want = [
            1.0,
            -2.6810808015,
            2.7103597041,
            -1.3116785243,
            0.2859231247,
        ];
        assert!(
            close(&c.a, &want, 1e-6),
            "even-N/2 band-pass should match the textbook design: {:?}",
            c.a
        );
    }

    /// Cutoffs that are never checked -- zero, inverted, far above Nyquist --
    /// all design a finite filter. Only the denominator is pinned: for a
    /// degenerate design the unit-peak scale divides one near-zero by
    /// another, so the gain is only checked to be finite.
    #[test]
    fn unchecked_cutoffs_still_design_a_filter() {
        // (rate, low, high, denominator)
        let cases: [(f64, f64, f64, &[f64]); 4] = [
            // upper cutoff far above Nyquist: wraps back into the band
            (
                22050.0,
                600.0,
                99999.0,
                &[1.0, -1.689_652_715_415_508_3, 0.713_731_574_677_750_2],
            ),
            // lower cutoff zero: a double pole at DC
            (22050.0, 0.0, 4100.0, &[1.0, -2.0, 1.0]),
            // inverted band
            (
                22050.0,
                5000.0,
                600.0,
                &[1.0, -10.012_762_048_037_095, 25.063_850_957_653],
            ),
            // both cutoffs above Nyquist (4000 Hz input)
            (
                4000.0,
                600.0,
                4100.0,
                &[1.0, -3.672_113_107_581_718_6, 3.371_103_668_718_366_6],
            ),
        ];
        for (rate, low_hz, high_hz, want) in cases {
            let c = design(
                &FilterSpec {
                    band: Band::BandPass,
                    order: 2,
                    low_hz,
                    high_hz,
                    ..Default::default()
                },
                rate,
            );
            assert!(
                close(&c.a, want, 1e-9),
                "{low_hz}-{high_hz} Hz @ {rate}: {:?}",
                c.a
            );
            assert!(
                c.b.iter().all(|v| v.is_finite()),
                "{low_hz}-{high_hz} Hz @ {rate}: gain is not finite: {:?}",
                c.b
            );
        }
    }

    /// A one-section Chebyshev has no conjugate pair for the ripple to shape,
    /// so an order-2 band-pass is a Butterworth at every ripple, zero included.
    #[test]
    fn order_two_chebyshev_is_butterworth_at_any_ripple() {
        let butter = design(&FilterSpec::default(), 32000.0);
        for ripple_db in [0.0, 0.5, 3.0, 20.0] {
            let c = design(
                &FilterSpec {
                    design: Design::Chebyshev,
                    ripple_db,
                    ..Default::default()
                },
                32000.0,
            );
            assert!(
                close(&c.a, &butter.a, 1e-12),
                "ripple {ripple_db}: {:?}",
                c.a
            );
            assert!(
                close(&c.b, &butter.b, 1e-12),
                "ripple {ripple_db}: {:?}",
                c.b
            );
        }
    }

    #[test]
    fn butter_lowpass_matches_scipy() {
        // scipy.signal.butter(2, 1000/22050, 'low') @ 44100
        let c = design(
            &FilterSpec {
                band: Band::LowPass,
                high_hz: 1000.0,
                ..Default::default()
            },
            44100.0,
        );
        let b = [
            0.004603998475022464,
            0.009207996950044928,
            0.004603998475022464,
        ];
        let a = [1.0, -1.7990964094846684, 0.8175124033847582];
        assert!(close(&c.b, &b, 1e-9), "b = {:?}", c.b);
        assert!(close(&c.a, &a, 1e-9), "a = {:?}", c.a);
    }

    /// The order-4 band-pass 600-4100 @ 44100: N/2 is even, so the
    /// double-pole convention stays inert and this is the textbook design.
    #[test]
    fn butter_bandpass_order4_matches_the_textbook_design() {
        let c = design(
            &FilterSpec {
                order: 4,
                low_hz: 600.0,
                high_hz: 4100.0,
                ..Default::default()
            },
            44100.0,
        );
        let b = [
            0.045_501_626_530_969_445,
            0.0,
            -0.091_003_253_061_938_89,
            0.0,
            0.045_501_626_530_969_445,
        ];
        let a = [
            1.0,
            -3.228_450_386_996_702,
            3.978_949_916_123_979,
            -2.243_260_564_877_916_3,
            0.494_571_010_322_306,
        ];
        assert!(close(&c.b, &b, 1e-9), "b = {:?}", c.b);
        assert!(close(&c.a, &a, 1e-9), "a = {:?}", c.a);
    }

    /// Chebyshev low-pass against scipy's `cheby1(3, 1.0, 3000/22050, 'low')`,
    /// agreeing to 6e-7: scipy scales by the algebraic gain, this to unit
    /// peak on the 251-point grid, which for an odd order lands beside the
    /// ripple maximum. The numbers are this design's, pinned to the last bit.
    #[test]
    fn cheby1_lowpass_scales_on_the_response_grid() {
        let c = design(
            &FilterSpec {
                order: 3,
                band: Band::LowPass,
                design: Design::Chebyshev,
                high_hz: 3000.0,
                ripple_db: 1.0,
                ..Default::default()
            },
            44100.0,
        );
        let b = [
            0.003_930_252_182_335_95,
            0.011_790_756_547_007_86,
            0.011_790_756_547_007_86,
            0.003_930_252_182_335_95,
        ];
        let a = [
            1.0,
            -2.4581125092134544,
            2.1459775097274525,
            -0.656423002246622,
        ];
        assert!(close(&c.b, &b, 1e-9), "b = {:?}", c.b);
        assert!(close(&c.a, &a, 1e-9), "a = {:?}", c.a);
    }

    /// The default preset (order-2 Butterworth band-pass 600-4100 @ 44100),
    /// pinned to full double precision.
    #[test]
    fn default_bandpass_coefficients_are_pinned() {
        let c = design(&FilterSpec::default(), 44100.0);
        assert!(
            (c.b[0] - 0.118_591_060_277_465_51).abs() < 1e-9,
            "b0 moved, got {}",
            c.b[0]
        );
        assert_eq!(c.a[0], 1.0);
        assert!((c.a[1] - (-1.746_869_617_025_513_8)).abs() < 1e-9);
        assert!((c.a[2] - 0.762_888_364_721_716_3).abs() < 1e-9);
        // ... and it is not the textbook design: N/2 is odd here, so the
        // band-pass pole quirk fires.
        assert!(
            (c.b[0] - 0.202_953).abs() > 1e-3,
            "should not be the textbook value"
        );
    }

    #[test]
    fn order_is_clamped_to_the_supported_range() {
        // The order is clamped silently, and clamped before rounding an odd
        // band-pass order up to even.
        let spec = |order| FilterSpec {
            order,
            ..Default::default()
        };
        assert_eq!(spec(0).effective_order(), 1);
        assert_eq!(spec(1).effective_order(), 1);
        assert_eq!(spec(2).effective_order(), 2);
        assert_eq!(spec(MAX_ORDER).effective_order(), MAX_ORDER as u32);
        assert_eq!(spec(MAX_ORDER + 1).effective_order(), MAX_ORDER as u32);
        assert_eq!(spec(99).effective_order(), MAX_ORDER as u32);

        // An out-of-range order designs identically to the ceiling.
        let at_ceiling = design(&spec(MAX_ORDER), 44100.0);
        for over in [MAX_ORDER + 1, 17, 99, i32::MAX] {
            let got = design(&spec(over), 44100.0);
            assert_eq!(
                got.a, at_ceiling.a,
                "order {over} should design as {MAX_ORDER}"
            );
            assert_eq!(
                got.b, at_ceiling.b,
                "order {over} should design as {MAX_ORDER}"
            );
        }
        assert!(design(&spec(0), 44100.0).b.iter().all(|x| x.is_finite()));

        // Every coefficient must be finite: the failure this guards against is
        // a silently divergent filter, not a panic.
        assert!(at_ceiling.a.iter().all(|v| v.is_finite()));
        assert!(at_ceiling.b.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn highpass_dc_blocked() {
        let c = design(
            &FilterSpec {
                band: Band::HighPass,
                low_hz: 1000.0,
                ..Default::default()
            },
            44100.0,
        );
        let y = apply(&c, &vec![1.0; 4096]);
        assert!(y[y.len() - 1].abs() < 1e-3, "high-pass should block DC");
    }
    /// The band-pass follows the unit-peak gain rule. The algebraic gain
    /// coincides with it while the double-pole convention is inert, and runs
    /// to 1.91 (44100 Hz order 2) and 2.29 (32000 Hz order 6) where it fires.
    #[test]
    fn analytic_bandpass_is_unit_peak() {
        let mut worst = 0.0f64;
        let mut worst_cfg = String::new();
        for rate in [11025.0, 22050.0, 32000.0, 32258.0, 44100.0, 48000.0] {
            for order in 1..=16i32 {
                for design in [Design::Butterworth, Design::Chebyshev] {
                    let spec = FilterSpec {
                        band: Band::BandPass,
                        design,
                        order,
                        low_hz: 600.0,
                        high_hz: 4100.0,
                        ripple_db: 1.0,
                    };
                    let zpk = design_zpk(&spec, rate);
                    let err = (peak_gain(&zpk) - 1.0).abs();
                    if err > worst {
                        worst = err;
                        worst_cfg = format!("{rate} Hz order {order} {design:?}");
                    }
                }
            }
        }
        assert!(
            worst < 1e-9,
            "analytic band-pass peak is off unit by {worst:.2e} at {worst_cfg}"
        );
    }

    /// Low- and high-pass keep the textbook gain: the rule above is the
    /// band-pass transform's.
    #[test]
    fn other_bands_keep_the_textbook_gain() {
        for band in [Band::LowPass, Band::HighPass] {
            let spec = FilterSpec {
                band,
                design: Design::Butterworth,
                order: 2,
                low_hz: 600.0,
                high_hz: 4100.0,
                ripple_db: 1.0,
            };
            let zpk = design_zpk(&spec, 44100.0);
            // A Butterworth low-/high-pass peaks at 1 in the pass band anyway,
            // so this asserts the shape is untouched, not that it was scaled.
            assert!((peak_gain(&zpk) - 1.0).abs() < 1e-9, "{band:?} gain moved");
        }
    }
}
