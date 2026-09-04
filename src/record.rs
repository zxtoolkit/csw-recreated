//! DirectMode (`-r`): live capture from a host audio input. A volume meter
//! runs until a key is pressed; recording then runs until `P` pauses it, any
//! other key stops it, or `-t` elapses. Audio is folded to mono and spooled.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::audio::{self, Frames, OpenSpec, Sample, SampleSink};
use crate::error::{Error, Result};
use crate::spool::{self, Spool, SpoolReader};
use crate::term;
use crate::ui::Console;

pub use ::csw::detect::MIDPOINT;
/// Full-scale amplitude around [`MIDPOINT`].
const FULL_SCALE: f64 = 127.0;
/// How often the meter is redrawn, and how long a key poll blocks for.
const TICK: Duration = Duration::from_millis(50);
pub const DEFAULT_RATE: u32 = 32258;

const MIN_RATE: u32 = 5000;
const MAX_RATE: u32 = 45454;

/// What the caller asked DirectMode to do.
#[derive(Debug, Clone, Copy)]
pub struct DirectSpec {
    /// Capture rate (`-s`); `None` asks for [`DEFAULT_RATE`].
    pub rate: Option<i64>,
    /// Recording time in seconds (`-t`); `None` records until a key is pressed.
    pub seconds: Option<f64>,
    /// `-c`: don't negotiate a configuration, take the device's own.
    pub device_compat: bool,
    /// `-k`: a WAV of the raw samples is written too, which needs as much
    /// room again as the spool -- see [`spool::sample_budget`].
    pub keep_samples: bool,
}

impl DirectSpec {
    fn rate_as_typed(&self) -> i64 {
        self.rate
            .filter(|&r| r != 0)
            .unwrap_or(i64::from(DEFAULT_RATE))
    }

    fn rate_asked(&self) -> u32 {
        self.rate_as_typed()
            .clamp(i64::from(MIN_RATE), i64::from(MAX_RATE)) as u32
    }

    fn stop_after(&self) -> Option<f64> {
        self.seconds.filter(|&secs| secs > 0.0)
    }

    fn time_budget(&self) -> Option<u64> {
        // The seconds are held as a 32-bit float and the product is truncated
        // straight from the register: `-t1.2` at 32258 Hz buys 38709 samples,
        // and `-t0.4969` 16028, where the double product would give 16029.
        let secs = f64::from(self.stop_after()? as f32);
        let samples = (secs * self.rate_as_typed() as f64).trunc();
        (samples > 0.0).then_some(samples as u64)
    }
}

#[derive(Debug, Clone, Copy)]
struct Limit {
    samples: u64,
    secs: f64,
}

/// A finished recording: the rate it was captured at, and the samples on disk.
pub struct Capture {
    /// Actual capture rate in Hz.
    pub rate: u32,
    /// Mono samples in the 8-bit unsigned domain, spooled to a temporary file
    /// that goes away when this is dropped.
    pub samples: SpoolReader,
}

/// State shared with the audio callback.
#[derive(Default)]
struct Shared {
    /// Captured samples, mono, 8-bit unsigned domain.
    samples: Mutex<Vec<f64>>,
    /// RMS of the most recent callback in the 8-bit domain (0..127), as `f32`
    /// bits -- the quantity the green bar shows.
    rms: AtomicU32,
    /// Share (0..1) of the most recent callback's frames that hit the rails,
    /// as `f32` bits -- the red bar.
    clipped: AtomicU32,
    /// Samples dropped because the buffer lock was held elsewhere.
    lost: AtomicU64,
    /// False while the meter runs: callbacks measure but discard.
    capturing: AtomicBool,
    /// Set by the stream error callback; recording stops.
    failed: AtomicBool,
}

impl Shared {
    /// The two meter readings: RMS in the 8-bit domain, and the clipped share.
    fn levels(&self) -> (f32, f32) {
        (
            f32::from_bits(self.rms.load(Ordering::Relaxed)),
            f32::from_bits(self.clipped.load(Ordering::Relaxed)),
        )
    }

    /// Fold one callback's interleaved frames into the mono buffer.
    fn ingest<T: Sample>(&self, data: &[T], channels: usize) {
        if channels == 0 {
            return;
        }
        // Both meter readings come from this buffer: the RMS of the deviation
        // from silence, and the share of frames at the rails (a source sample
        // at or past full scale; 0x00 or 0xFF after quantisation).
        let mut sum_sq = 0.0f64;
        let mut railed = 0usize;
        let frames_n = data.len() / channels;
        // Never block the audio thread: samples lost while the UI holds the
        // buffer are counted and reported. The lock is taken before the pass
        // so nothing allocates per callback.
        let capturing = self.capturing.load(Ordering::Relaxed);
        let mut sink = if capturing {
            self.samples.try_lock().ok()
        } else {
            None
        };
        if let Some(buf) = sink.as_mut() {
            buf.reserve(frames_n);
        }
        for frame in data.chunks_exact(channels) {
            let mut sum = 0.0f32;
            let mut rail = false;
            for &s in frame {
                let v = s.to_f32();
                sum += v;
                rail |= v.abs() >= 1.0;
            }
            let v = sum / channels as f32;
            let m = MIDPOINT + (v as f64).clamp(-1.0, 1.0) * FULL_SCALE;
            sum_sq += (m - MIDPOINT) * (m - MIDPOINT);
            railed += usize::from(rail);
            if let Some(buf) = sink.as_mut() {
                buf.push(m);
            }
        }
        let frames = frames_n.max(1) as f64;
        self.rms.store(
            ((sum_sq / frames).sqrt() as f32).to_bits(),
            Ordering::Relaxed,
        );
        self.clipped.store(
            ((railed as f64 / frames) as f32).to_bits(),
            Ordering::Relaxed,
        );
        match sink {
            // The samples are in the buffer; the recording loop counts them
            // when it drains, and the meter reads the count from the spool.
            Some(_) => {}
            // Not capturing yet, or the UI had the buffer.
            None if capturing => {
                self.lost.fetch_add(frames_n as u64, Ordering::Relaxed);
            }
            None => {}
        }
    }
}

impl SampleSink for Shared {
    fn frames(&self, data: Frames<'_>, channels: usize) {
        match data {
            Frames::U8(d) => self.ingest(d, channels),
            Frames::I8(d) => self.ingest(d, channels),
            Frames::I16(d) => self.ingest(d, channels),
            Frames::U16(d) => self.ingest(d, channels),
            Frames::I32(d) => self.ingest(d, channels),
            Frames::F32(d) => self.ingest(d, channels),
            Frames::F64(d) => self.ingest(d, channels),
        }
    }

    fn failed(&self) {
        self.failed.store(true, Ordering::Relaxed);
    }

    fn dropped(&self, samples: u64) {
        self.lost.fetch_add(samples, Ordering::Relaxed);
    }
}

/// Run a DirectMode session: meter, record, stop. Prints its own status lines.
pub fn capture(
    spec: &DirectSpec,
    ui: &Console,
    w: &mut impl Write,
    out_dir: &Path,
) -> Result<Capture> {
    let device = audio::open(&OpenSpec {
        rate: spec.rate_asked(),
        device_compat: spec.device_compat,
    })?;
    let name = device.name().to_string();
    let config = device.config();
    let rate = config.rate;
    let channels = config.channels;

    ui.input_device(w, &name, channels, &format!("{}", config.format))?;
    if spec.device_compat {
        ui.device_compat_mode(w)?;
    }
    ui.capture_rate(w, rate, spec.rate_asked())?;

    // Samples go to disk as they arrive; a recording is bounded by free space
    // (see `spool`).
    let spool = Spool::create(out_dir)?;

    // The limit: `-t` or the free space on the spool's volume, whichever runs
    // out first. Printed once the meter phase is over, enforced while
    // recording.
    let secs_of = |samples: u64| {
        if rate > 0 {
            samples as f64 / rate as f64
        } else {
            0.0
        }
    };
    let by_time = spec.time_budget();
    let by_space =
        spool::free_bytes(out_dir).map(|free| spool::sample_budget(free, spec.keep_samples));
    let limit = match (by_time, by_space) {
        (Some(t), Some(s)) => Some(t.min(s)),
        (t, s) => t.or(s),
    }
    .map(|samples| Limit {
        samples,
        secs: secs_of(samples),
    });

    let shared = Arc::new(Shared::default());
    // `Arc::clone` cannot name the trait object as its own type; the method
    // call lets the coercion happen at the binding.
    let sink: Arc<dyn SampleSink> = shared.clone();
    let stream = device.start(sink)?;

    let recorded = run_session(spec, ui, w, &shared, spool, limit, Keyboard::new())?;
    drop(stream);

    if shared.failed.load(Ordering::Relaxed) {
        // Nothing captured is a failed recording. Something captured is a
        // short one, kept and converted, with the reason said.
        if recorded.is_empty() {
            return Err(Error::Fatal(
                "Recording failed: the input device stopped".into(),
            ));
        }
        ui.recording_cut_short(w, "the input device stopped")?;
    }
    let lost = shared.lost.load(Ordering::Relaxed);
    if lost > 0 {
        ui.overrun(w, lost)?;
    }
    ui.recorded(w, recorded.len(), secs_of(recorded.len()))?;
    Ok(Capture {
        rate,
        samples: recorded,
    })
}

/// The interactive part: meter phase, then recording with pause/stop.
fn run_session(
    spec: &DirectSpec,
    ui: &Console,
    w: &mut impl Write,
    shared: &Arc<Shared>,
    mut spool: Spool,
    limit: Option<Limit>,
    keys: Keyboard,
) -> Result<SpoolReader> {
    let interactive = keys.is_some();
    let draw_meter = interactive && ui.is_terminal();
    let prompts_as_text = !draw_meter;
    if !interactive && spec.stop_after().is_none() {
        return Err(Error::Fatal(
            "DirectMode needs a terminal for its keyboard controls; \
             give a recording time with -t<secs> to record without one"
                .into(),
        ));
    }

    // --- meter phase: measure, discard, wait for a key ---------------------
    if prompts_as_text {
        ui.transient(w, crate::ui::PROMPT_START)?;
    }
    let mut aborted = false;
    if interactive {
        loop {
            if draw_meter {
                ui.meter(w, crate::ui::PROMPT_START, Some(shared.levels()), None)?;
            }
            match keys.pressed(TICK)? {
                // Nothing is recorded yet, so Ctrl-C aborts; any other key
                // starts.
                Some(Key::Abort) => {
                    aborted = true;
                    break;
                }
                Some(_) => break,
                None => {}
            }
            if shared.failed.load(Ordering::Relaxed) {
                break;
            }
        }
    }
    if draw_meter {
        ui.clear_transient(w)?;
    } else {
        ui.transient(w, crate::ui::ERASE_START)?;
    }
    if aborted {
        return Err(Error::Fatal("Aborted".into()));
    }

    // Without a limit there is no line; a limit of zero samples prints one.
    if let Some(limit) = limit {
        ui.max_recording_time(w, limit.secs, limit.samples * spool::BYTES_PER_SAMPLE)?;
    }

    // --- recording phase ---------------------------------------------------
    shared.samples.lock().unwrap().clear();
    shared.capturing.store(true, Ordering::Relaxed);
    // Recycled across ticks.
    let mut batch: Vec<f64> = Vec::new();
    let start = Instant::now();
    let mut paused = Duration::ZERO;
    // A spool write that fails ends the recording but keeps what is on disk:
    // returning through `?` would drop the `Spool`, which unlinks the file.
    // The error is kept and said at the end.
    let mut spool_failure: Option<String> = None;

    'recording: loop {
        if shared.failed.load(Ordering::Relaxed) {
            break;
        }
        let taken = match drain(shared, &mut spool, &mut batch, limit) {
            Ok(taken) => taken,
            Err(e) => {
                spool_failure = Some(e.to_string());
                break;
            }
        };
        let elapsed = start.elapsed().saturating_sub(paused);
        if limit.is_some_and(|max| spool.samples() >= max.samples) {
            break;
        }
        // The bars are blanked while recording. The levels are still taken
        // from every buffer, so the bars are correct the instant you pause.
        if draw_meter {
            ui.meter(w, "", None, Some((elapsed.as_secs_f64(), taken)))?;
        }

        match keys.pressed(TICK)? {
            None => {}
            Some(Key::Pause) => {
                // Pause: stop collecting, show the meter again, any key resumes.
                shared.capturing.store(false, Ordering::Relaxed);
                let began = Instant::now();
                if let Err(e) = drain(shared, &mut spool, &mut batch, limit) {
                    spool_failure = Some(e.to_string());
                    break 'recording;
                }
                if prompts_as_text {
                    ui.transient(w, crate::ui::ERASE_PAUSED)?;
                    ui.transient(w, crate::ui::PROMPT_PAUSED)?;
                }
                let mut aborted = false;
                loop {
                    if draw_meter {
                        ui.meter(w, crate::ui::PROMPT_PAUSED, Some(shared.levels()), None)?;
                    }
                    match keys.pressed(TICK)? {
                        // Ctrl-C while paused ends the recording; what is
                        // already spooled is kept.
                        Some(Key::Abort) => {
                            aborted = true;
                            break;
                        }
                        Some(_) => break,
                        None => {}
                    }
                    if shared.failed.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if prompts_as_text {
                    ui.transient(w, crate::ui::ERASE_PAUSED)?;
                }
                if aborted {
                    break 'recording;
                }
                paused += began.elapsed();
                shared.capturing.store(true, Ordering::Relaxed);
            }
            Some(Key::Stop) | Some(Key::Abort) => break,
        }
    }

    shared.capturing.store(false, Ordering::Relaxed);
    if spool_failure.is_none() {
        if let Err(e) = drain(shared, &mut spool, &mut batch, limit) {
            spool_failure = Some(e.to_string());
        }
    }
    drop(keys);
    if draw_meter {
        ui.clear_transient(w)?;
    }
    // The flush can fail for the same reason the writes did, and is handled
    // the same way: the tail is lost, the take is not.
    let (recorded, flush_failure) = spool.finish()?;
    // Say why, once, whichever of the two failures came first.
    if let Some(reason) = spool_failure.or(flush_failure) {
        ui.recording_cut_short(w, &reason)?;
    }
    Ok(recorded)
}

fn drain(
    shared: &Arc<Shared>,
    spool: &mut Spool,
    batch: &mut Vec<f64>,
    limit: Option<Limit>,
) -> Result<u64> {
    batch.clear();
    std::mem::swap(&mut *shared.samples.lock().unwrap(), batch);
    if let Some(max) = limit {
        let room = max.samples.saturating_sub(spool.samples());
        batch.truncate(usize::try_from(room).unwrap_or(usize::MAX));
    }
    if !batch.is_empty() {
        spool.write(batch)?;
    }
    Ok(spool.samples())
}

// --- keyboard ----------------------------------------------------------------

/// What a keypress means during a session.
enum Key {
    /// `P`: pause / resume.
    Pause,
    /// Anything else: stop.
    Stop,
    /// Ctrl-C. Raw mode swallows SIGINT, so it arrives as a keypress like any
    /// other -- but it is the one key that must never mean "go".
    Abort,
}

/// Raw-mode keyboard, restored on drop. Absent when stdin is not a terminal
/// (a pipe, a CI runner), in which case `-t` drives the session instead.
struct Keyboard {
    saved: Option<term::Saved>,
    #[cfg(test)]
    scripted: Option<std::cell::RefCell<std::collections::VecDeque<Option<Key>>>>,
    #[cfg(test)]
    feed: Option<(
        Arc<Shared>,
        std::cell::RefCell<std::collections::VecDeque<usize>>,
    )>,
}

impl Keyboard {
    /// Puts the terminal in raw mode; `is_some()` is false when there is none.
    fn new() -> Self {
        Keyboard {
            saved: term::enable_raw_mode(),
            #[cfg(test)]
            scripted: None,
            #[cfg(test)]
            feed: None,
        }
    }

    #[cfg(test)]
    fn scripted(keys: Vec<Option<Key>>) -> Self {
        Keyboard {
            saved: None,
            scripted: Some(std::cell::RefCell::new(keys.into())),
            feed: None,
        }
    }

    #[cfg(test)]
    fn scripted_feeding(shared: Arc<Shared>, entries: Vec<(usize, Option<Key>)>) -> Self {
        let (counts, keys): (Vec<usize>, Vec<Option<Key>>) = entries.into_iter().unzip();
        Keyboard {
            saved: None,
            scripted: Some(std::cell::RefCell::new(keys.into())),
            feed: Some((shared, std::cell::RefCell::new(counts.into()))),
        }
    }

    #[cfg(test)]
    fn none() -> Self {
        Keyboard {
            saved: None,
            scripted: None,
            feed: None,
        }
    }

    fn is_some(&self) -> bool {
        #[cfg(test)]
        if self.scripted.is_some() {
            return true;
        }
        self.saved.is_some()
    }

    /// Poll for up to `timeout`; `Ok(None)` when nothing was pressed.
    fn pressed(&self, timeout: Duration) -> Result<Option<Key>> {
        #[cfg(test)]
        if let Some((shared, counts)) = &self.feed {
            let n = counts
                .borrow_mut()
                .pop_front()
                .expect("the scripted keyboard ran out of sample counts");
            if shared.capturing.load(Ordering::Relaxed) {
                shared.samples.lock().unwrap().extend(vec![MIDPOINT; n]);
            }
        }
        #[cfg(test)]
        if let Some(keys) = &self.scripted {
            return Ok(keys
                .borrow_mut()
                .pop_front()
                .expect("the scripted keyboard ran out of keys"));
        }
        if self.saved.is_none() {
            std::thread::sleep(timeout);
            return Ok(None);
        }
        let Some(key) = term::poll_key(timeout).map_err(Error::Io)? else {
            return Ok(None);
        };
        if key.is_interrupt() {
            return Ok(Some(Key::Abort));
        }
        Ok(Some(if key.is('p') { Key::Pause } else { Key::Stop }))
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            term::disable_raw_mode(saved);
            // The meter hides the cursor; an error path must not leave it that
            // way, and this guard outlives every meter draw.
            let _ = std::io::stdout().write_all(crate::ui::SHOW_CURSOR);
            let _ = std::io::stdout().flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_with_no_meter_writes_the_prompt_and_the_run_that_blanks_it() {
        let mut v = Vec::new();
        session(&stream(), Keyboard::none(), AT_ONCE, &mut v).unwrap();
        assert_eq!(
            v,
            b"* Press any key to start conversion when done with volume meter...\
              \r\t\t\t\t\t\t\t\t\t\r\
              \xfe Max recording time is 00:00 (0 bytes).\n"
                .to_vec()
        );
    }

    fn session(ui: &Console, keys: Keyboard, limit: Option<Limit>, w: &mut Vec<u8>) -> Result<()> {
        let spec = DirectSpec {
            rate: None,
            seconds: Some(1.0),
            device_compat: false,
            keep_samples: false,
        };
        let shared = Arc::new(Shared::default());
        let spool = Spool::create(&std::env::temp_dir())?;
        run_session(&spec, ui, w, &shared, spool, limit, keys).map(|_| ())
    }

    #[test]
    fn a_drain_stops_at_the_limit() {
        let shared = Arc::new(Shared::default());
        let mut spool = Spool::create(&std::env::temp_dir()).unwrap();
        let mut batch = Vec::new();
        let limit = Some(Limit {
            samples: 4,
            secs: 0.0,
        });
        shared.samples.lock().unwrap().extend(vec![MIDPOINT; 3]);
        assert_eq!(drain(&shared, &mut spool, &mut batch, limit).unwrap(), 3);
        shared.samples.lock().unwrap().extend(vec![MIDPOINT; 10]);
        assert_eq!(drain(&shared, &mut spool, &mut batch, limit).unwrap(), 4);
        shared.samples.lock().unwrap().extend(vec![MIDPOINT; 10]);
        assert_eq!(drain(&shared, &mut spool, &mut batch, limit).unwrap(), 4);
        shared.samples.lock().unwrap().extend(vec![MIDPOINT; 3]);
        assert_eq!(drain(&shared, &mut spool, &mut batch, None).unwrap(), 7);
        assert_eq!(spool.samples(), 7);
    }

    #[test]
    fn every_drain_stops_at_the_limit() {
        let spec = DirectSpec {
            rate: None,
            seconds: Some(1.0),
            device_compat: false,
            keep_samples: false,
        };
        let run = |samples: u64, entries: Vec<(usize, Option<Key>)>| -> u64 {
            let shared = Arc::new(Shared::default());
            let spool = Spool::create(&std::env::temp_dir()).unwrap();
            let keys = Keyboard::scripted_feeding(shared.clone(), entries);
            let limit = Some(Limit { samples, secs: 0.0 });
            let mut w = Vec::new();
            run_session(&spec, &stream(), &mut w, &shared, spool, limit, keys)
                .unwrap()
                .len()
        };
        assert_eq!(
            run(
                4,
                vec![(0, Some(Key::Stop)), (3, None), (3, None), (3, None)]
            ),
            4
        );
        assert_eq!(
            run(
                7,
                vec![
                    (0, Some(Key::Stop)),
                    (3, None),
                    (3, None),
                    (3, Some(Key::Pause)),
                    (0, Some(Key::Stop)),
                ]
            ),
            7
        );
        assert_eq!(
            run(
                4,
                vec![(0, Some(Key::Stop)), (3, None), (3, Some(Key::Stop))]
            ),
            4
        );
    }

    #[test]
    fn samples_that_arrive_while_paused_are_dropped() {
        let spec = DirectSpec {
            rate: None,
            seconds: Some(1.0),
            device_compat: false,
            keep_samples: false,
        };
        let shared = Arc::new(Shared::default());
        let spool = Spool::create(&std::env::temp_dir()).unwrap();
        let keys = Keyboard::scripted_feeding(
            shared.clone(),
            vec![
                (0, Some(Key::Stop)),
                (3, Some(Key::Pause)),
                (5, None),
                (0, Some(Key::Stop)),
                (0, Some(Key::Stop)),
            ],
        );
        let mut w = Vec::new();
        let recorded = run_session(&spec, &stream(), &mut w, &shared, spool, None, keys).unwrap();
        assert_eq!(recorded.len(), 3);
    }

    const AT_ONCE: Option<Limit> = Some(Limit {
        samples: 0,
        secs: 0.0,
    });

    fn stream() -> Console {
        Console::new(false)
    }

    fn on_a_terminal(keys: Keyboard, w: &mut Vec<u8>) -> Result<()> {
        let _width = crate::term::fixed_width(Some(80));
        session(&Console::new(true), keys, None, w)
    }

    #[test]
    fn a_session_on_a_terminal_draws_the_meter_and_no_blanking_run() {
        let mut v = Vec::new();
        let err = on_a_terminal(Keyboard::scripted(vec![Some(Key::Abort)]), &mut v).unwrap_err();
        assert!(matches!(err, Error::Fatal(_)), "{err:?}");
        let s = String::from_utf8_lossy(&v);
        assert!(!s.contains('\t'), "{s:?}");
        assert!(s.contains(crate::ui::PROMPT_START), "{s:?}");
        assert!(s.contains('\u{25A0}'), "{s:?}");
        assert!(v.ends_with(crate::ui::SHOW_CURSOR), "{s:?}");
    }

    #[test]
    fn a_pause_on_a_terminal_stays_inside_the_meter_block() {
        let mut v = Vec::new();
        on_a_terminal(
            Keyboard::scripted(vec![
                Some(Key::Stop),
                Some(Key::Pause),
                Some(Key::Stop),
                Some(Key::Stop),
            ]),
            &mut v,
        )
        .unwrap();
        let s = String::from_utf8_lossy(&v);
        assert!(!s.contains('\t'), "{s:?}");
        assert!(s.contains(crate::ui::PROMPT_PAUSED), "{s:?}");
        assert!(s.contains(" samples"), "{s:?}");
        assert_eq!(s.matches("\u{1b}[3A").count(), 6, "{s:?}");
        assert!(v.ends_with(crate::ui::SHOW_CURSOR), "{s:?}");
    }

    #[test]
    fn a_pause_on_a_stream_is_blanked_on_both_sides() {
        let mut v = Vec::new();
        session(
            &stream(),
            Keyboard::scripted(vec![
                None,
                Some(Key::Stop),
                None,
                Some(Key::Pause),
                None,
                Some(Key::Stop),
                Some(Key::Stop),
            ]),
            None,
            &mut v,
        )
        .unwrap();
        assert_eq!(
            v,
            b"* Press any key to start conversion when done with volume meter...\
              \r\t\t\t\t\t\t\t\t\t\r\
              \r\t\t\t\t\t\t\t\t\r* PAUSED, press any key to continue...\
              \r\t\t\t\t\t\t\t\t\r"
                .to_vec()
        );
    }

    #[test]
    fn ctrl_c_in_the_meter_phase_leaves_through_the_blanking_run() {
        let mut v = Vec::new();
        let err = session(
            &stream(),
            Keyboard::scripted(vec![Some(Key::Abort)]),
            None,
            &mut v,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Fatal(_)), "{err:?}");
        assert_eq!(
            v,
            b"* Press any key to start conversion when done with volume meter...\
              \r\t\t\t\t\t\t\t\t\t\r"
                .to_vec()
        );
    }

    #[test]
    fn t_buys_samples_at_the_rate_that_was_asked_for() {
        let spec = |rate, seconds| DirectSpec {
            rate,
            seconds,
            device_compat: false,
            keep_samples: false,
        };
        assert_eq!(spec(Some(-1000), None).rate_asked(), 5000);
        assert_eq!(spec(Some(-1000), Some(1.0)).time_budget(), None);
        assert_eq!(spec(None, Some(0.0)).time_budget(), None);
        assert_eq!(spec(None, Some(-5.0)).time_budget(), None);
        assert_eq!(spec(None, Some(0.0)).stop_after(), None);
        assert_eq!(spec(Some(44100), Some(1.0)).time_budget(), Some(44100));
        assert_eq!(spec(Some(200000), Some(1.0)).time_budget(), Some(200000));
        assert_eq!(spec(None, Some(2.0)).time_budget(), Some(2 * 32258));
        // Truncated, and from a 32-bit float: 0.4969 as a float is a hair
        // under, so the product is 16028.99995; 0.5062 and 0.9938 go the
        // other way from a double.
        assert_eq!(spec(None, Some(1.2)).time_budget(), Some(38709));
        assert_eq!(spec(None, Some(0.4969)).time_budget(), Some(16028));
        assert_eq!(spec(None, Some(0.5062)).time_budget(), Some(16329));
        assert_eq!(spec(None, Some(0.9938)).time_budget(), Some(32057));
        assert_eq!(spec(Some(0), Some(1.0)).time_budget(), Some(32258));
        assert_eq!(spec(Some(8000), None).time_budget(), None);
        assert_eq!(spec(Some(200000), None).rate_asked(), 45454);
        assert_eq!(spec(Some(100), None).rate_asked(), 5000);
        assert_eq!(spec(Some(0), None).rate_asked(), 32258);
        assert_eq!(spec(Some(44100), None).rate_asked(), 44100);
    }

    #[test]
    fn ingest_averages_channels_into_the_8bit_domain() {
        let shared = Shared::default();
        shared.capturing.store(true, Ordering::Relaxed);
        // two stereo frames: (+1,-1) -> silence, (+1,+1) -> full scale
        shared.ingest(&[1.0f32, -1.0, 1.0, 1.0], 2);
        let got = shared.samples.lock().unwrap().clone();
        assert_eq!(got, vec![MIDPOINT, MIDPOINT + FULL_SCALE]);
        // one frame at silence, one at full scale: RMS = 127/sqrt(2).
        let (rms, clipped) = shared.levels();
        assert!(
            (rms - FULL_SCALE as f32 / 2f32.sqrt()).abs() < 0.01,
            "{rms}"
        );
        assert_eq!(clipped, 1.0);
    }

    #[test]
    fn meter_phase_discards_samples() {
        let shared = Shared::default();
        shared.ingest(&[0.5f32, 0.5], 1);
        assert!(shared.samples.lock().unwrap().is_empty());
        let (rms, clipped) = shared.levels();
        assert!((rms - 0.5 * FULL_SCALE as f32).abs() < 0.01, "{rms}");
        assert_eq!(clipped, 0.0);
    }

    #[test]
    fn integer_input_maps_silence_to_the_midpoint() {
        let shared = Shared::default();
        shared.capturing.store(true, Ordering::Relaxed);
        shared.ingest(&[0i16, i16::MAX], 1);
        let got = shared.samples.lock().unwrap().clone();
        assert_eq!(got[0], MIDPOINT);
        assert!((got[1] - (MIDPOINT + FULL_SCALE)).abs() < 0.02);
    }

    /// The whole DirectMode path minus the hardware: a captured square wave
    /// must binarise to the classic 2*f*t pulses.
    #[test]
    fn captured_square_wave_yields_two_pulses_per_cycle() {
        const RATE: u32 = 44100;
        const HZ: f64 = 1000.0;
        const SECS: f64 = 0.5;
        let shared = Shared::default();
        shared.capturing.store(true, Ordering::Relaxed);
        // A stereo device feeding the same square wave to both channels.
        let frames = (RATE as f64 * SECS) as usize;
        let mut interleaved = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let phase = (i as f64) * HZ / RATE as f64;
            let v = if phase.fract() < 0.5 { 0.8f32 } else { -0.8 };
            interleaved.push(v);
            interleaved.push(v);
        }
        shared.ingest(&interleaved, 2);

        let samples = shared.samples.lock().unwrap().clone();
        let sig = ::csw::detect::samples_to_pulses(RATE, &samples, MIDPOINT);
        let expected = (2.0 * HZ * SECS) as usize;
        assert!(
            sig.pulses.len().abs_diff(expected) <= 1,
            "{} pulses, expected about {expected}",
            sig.pulses.len()
        );
        assert_eq!(sig.total_samples(), samples.len() as u64);
    }
}
