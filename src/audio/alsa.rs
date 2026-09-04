//! The dependency-free DirectMode backend: the kernel's PCM ioctls on
//! `/dev/snd/pcmC<card>D<dev>c`, no `libasound`, so a static musl build is
//! possible. Exclusive open; `CSW_ALSA_DEVICE` (`hw:1,0`) picks the device.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use super::{Config, Format, Frames, Input, OpenSpec, SampleSink, Stream};
use crate::error::{Error, Result};

#[cfg(target_endian = "big")]
compile_error!(
    "the raw ALSA backend assumes a little-endian host: it asks the hardware \
     for `_LE` sample formats and reads the kernel's bitfields LSB-first"
);

/// Rates DirectMode will settle for, best first, when the one it asked for is
/// not on offer. The requested rate is tried ahead of all of them.
const RATES: [u32; 9] = [
    48_000, 44_100, 32_000, 96_000, 22_050, 16_000, 11_025, 8_000, 192_000,
];

/// Target period, in milliseconds. Short enough that the meter and the stop
/// key stay responsive, long enough not to wake the reader pointlessly.
const PERIOD_MS: u32 = 20;
/// Periods per buffer: how far behind the reader may fall before the kernel
/// overruns.
const PERIODS: u32 = 4;

// --- the kernel ABI ----------------------------------------------------------

mod sys {
    use std::mem::size_of;

    use libc::c_ulong;

    // asm-generic ioctl encoding, which every architecture Linux runs ALSA on
    // uses (the alpha/mips/parisc variant differs, and is not a target here).
    const NR_SHIFT: u32 = 0;
    const TYPE_SHIFT: u32 = 8;
    const SIZE_SHIFT: u32 = 16;
    const DIR_SHIFT: u32 = 30;
    const DIR_NONE: u32 = 0;
    const DIR_WRITE: u32 = 1;
    const DIR_READ: u32 = 2;
    /// `'A'`, the PCM ioctl family.
    const PCM: u32 = 0x41;

    const fn ioc(dir: u32, nr: u32, size: usize) -> u32 {
        (dir << DIR_SHIFT) | ((size as u32) << SIZE_SHIFT) | (PCM << TYPE_SHIFT) | (nr << NR_SHIFT)
    }

    /// `_IOR('A', 0x00, int)`
    pub const PVERSION: u32 = ioc(DIR_READ, 0x00, size_of::<i32>());
    /// `_IOR('A', 0x01, struct snd_pcm_info)`
    pub const INFO: u32 = ioc(DIR_READ, 0x01, size_of::<PcmInfo>());
    /// `_IOWR('A', 0x10, struct snd_pcm_hw_params)`
    pub const HW_REFINE: u32 = ioc(DIR_READ | DIR_WRITE, 0x10, size_of::<HwParams>());
    /// `_IOWR('A', 0x11, struct snd_pcm_hw_params)`
    pub const HW_PARAMS: u32 = ioc(DIR_READ | DIR_WRITE, 0x11, size_of::<HwParams>());
    /// `_IOWR('A', 0x13, struct snd_pcm_sw_params)`
    pub const SW_PARAMS: u32 = ioc(DIR_READ | DIR_WRITE, 0x13, size_of::<SwParams>());
    /// `_IO('A', 0x40)`
    pub const PREPARE: u32 = ioc(DIR_NONE, 0x40, 0);
    /// `_IO('A', 0x42)`
    pub const START: u32 = ioc(DIR_NONE, 0x42, 0);
    /// `_IO('A', 0x43)`
    pub const DROP: u32 = ioc(DIR_NONE, 0x43, 0);
    /// `_IOR('A', 0x51, struct snd_xferi)`
    pub const READI_FRAMES: u32 = ioc(DIR_READ, 0x51, size_of::<XferI>());

    /// `SNDRV_PCM_ACCESS_RW_INTERLEAVED`
    pub const ACCESS_RW_INTERLEAVED: u32 = 3;

    /// `SNDRV_PCM_FORMAT_*`, only the ones this backend can convert.
    pub const FORMAT_S8: u32 = 0;
    pub const FORMAT_U8: u32 = 1;
    pub const FORMAT_S16_LE: u32 = 2;
    pub const FORMAT_U16_LE: u32 = 4;
    pub const FORMAT_S32_LE: u32 = 10;
    pub const FORMAT_FLOAT_LE: u32 = 14;
    pub const FORMAT_FLOAT64_LE: u32 = 16;

    /// Mask parameters, indexed from `SNDRV_PCM_HW_PARAM_FIRST_MASK` (0).
    pub const P_ACCESS: usize = 0;
    pub const P_FORMAT: usize = 1;
    /// Interval parameters, indexed from `SNDRV_PCM_HW_PARAM_FIRST_INTERVAL`
    /// (8) -- which is also what their bit position in `rmask` is offset by.
    pub const FIRST_INTERVAL: usize = 8;
    pub const P_CHANNELS: usize = 10 - FIRST_INTERVAL;
    pub const P_RATE: usize = 11 - FIRST_INTERVAL;
    pub const P_PERIOD_SIZE: usize = 13 - FIRST_INTERVAL;
    pub const P_PERIODS: usize = 15 - FIRST_INTERVAL;
    pub const P_BUFFER_SIZE: usize = 17 - FIRST_INTERVAL;

    /// `struct snd_mask`: a 256-bit set, one bit per enum value.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Mask {
        pub bits: [u32; 8],
    }

    impl Mask {
        pub const fn any() -> Self {
            Mask { bits: [!0; 8] }
        }
        pub fn test(&self, bit: u32) -> bool {
            self.bits[(bit / 32) as usize] & (1 << (bit % 32)) != 0
        }
        pub fn set_single(&mut self, bit: u32) {
            self.bits = [0; 8];
            self.bits[(bit / 32) as usize] = 1 << (bit % 32);
        }
    }

    /// `struct snd_interval`: an inclusive range plus four bitfield flags,
    /// which on a little-endian host pack from the least significant bit.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Interval {
        pub min: u32,
        pub max: u32,
        pub flags: u32,
    }

    pub const INTERVAL_OPENMIN: u32 = 1 << 0;
    pub const INTERVAL_OPENMAX: u32 = 1 << 1;
    pub const INTERVAL_INTEGER: u32 = 1 << 2;

    impl Interval {
        pub const fn any() -> Self {
            Interval {
                min: 0,
                max: !0,
                flags: 0,
            }
        }
        /// Pin the interval to one value, as `snd_interval_set_value` does.
        pub fn set_single(&mut self, value: u32) {
            self.min = value;
            self.max = value;
            self.flags = INTERVAL_INTEGER;
        }
        /// The value a refined interval has settled on, if it has settled.
        pub fn settled(&self) -> u32 {
            self.min
        }
        pub fn contains(&self, value: u32) -> bool {
            let low = self
                .min
                .saturating_add(u32::from(self.flags & INTERVAL_OPENMIN != 0));
            let high = self
                .max
                .saturating_sub(u32::from(self.flags & INTERVAL_OPENMAX != 0));
            (low..=high).contains(&value)
        }
    }

    /// `struct snd_pcm_hw_params`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct HwParams {
        pub flags: u32,
        pub masks: [Mask; 3],
        pub mres: [Mask; 5],
        pub intervals: [Interval; 12],
        pub ires: [Interval; 9],
        pub rmask: u32,
        pub cmask: u32,
        pub info: u32,
        pub msbits: u32,
        pub rate_num: u32,
        pub rate_den: u32,
        pub fifo_size: c_ulong,
        pub reserved: [u8; 64],
    }

    impl HwParams {
        /// The equivalent of `snd_pcm_hw_params_any`: every parameter open,
        /// every one of them up for refinement.
        pub fn any() -> Self {
            HwParams {
                flags: 0,
                masks: [Mask::any(); 3],
                mres: [Mask::any(); 5],
                intervals: [Interval::any(); 12],
                ires: [Interval::any(); 9],
                rmask: !0,
                cmask: 0,
                info: !0,
                msbits: 0,
                rate_num: 0,
                rate_den: 0,
                fifo_size: 0,
                reserved: [0; 64],
            }
        }
    }

    /// `struct snd_pcm_sw_params`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SwParams {
        pub tstamp_mode: i32,
        pub period_step: u32,
        pub sleep_min: u32,
        pub avail_min: c_ulong,
        pub xfer_align: c_ulong,
        pub start_threshold: c_ulong,
        pub stop_threshold: c_ulong,
        pub silence_threshold: c_ulong,
        pub silence_size: c_ulong,
        pub boundary: c_ulong,
        pub proto: u32,
        pub tstamp_type: u32,
        pub reserved: [u8; 56],
    }

    /// `struct snd_pcm_info`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PcmInfo {
        pub device: u32,
        pub subdevice: u32,
        pub stream: i32,
        pub card: i32,
        pub id: [u8; 64],
        pub name: [u8; 80],
        pub subname: [u8; 32],
        pub dev_class: i32,
        pub dev_subclass: i32,
        pub subdevices_count: u32,
        pub subdevices_avail: u32,
        pub sync: [u8; 16],
        pub reserved: [u8; 64],
    }

    /// `struct snd_xferi`, the interleaved transfer request.
    #[repr(C)]
    pub struct XferI {
        /// Frames actually transferred; the kernel writes it back.
        pub result: isize,
        pub buf: *mut libc::c_void,
        pub frames: c_ulong,
    }

    // If any of these is wrong the ioctl number is wrong with it, and the
    // kernel answers ENOTTY; a wrong size fails the build instead.
    #[cfg(target_pointer_width = "64")]
    const _: () = {
        assert!(size_of::<HwParams>() == 608);
        assert!(size_of::<SwParams>() == 136);
        assert!(size_of::<PcmInfo>() == 288);
        assert!(size_of::<XferI>() == 24);
    };
    #[cfg(target_pointer_width = "32")]
    const _: () = {
        assert!(size_of::<HwParams>() == 604);
        assert!(size_of::<SwParams>() == 104);
        assert!(size_of::<PcmInfo>() == 288);
        assert!(size_of::<XferI>() == 12);
    };
    const _: () = {
        assert!(size_of::<Mask>() == 32);
        assert!(size_of::<Interval>() == 12);
    };
}

use sys::{HwParams, Interval, PcmInfo, SwParams, XferI};

// --- file descriptor ---------------------------------------------------------

/// An owned `/dev/snd` descriptor. Closing it is what releases the card.
struct Fd(libc::c_int);

impl Fd {
    fn open(path: &Path) -> std::io::Result<Fd> {
        let c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        // Capture is read-only, and the descriptor must not survive an exec.
        //
        // `O_NONBLOCK` makes a card that is already taken an `EBUSY`: without
        // it the kernel's PCM open sleeps until the holder lets go, and a card
        // held by PipeWire hangs the program in silence.
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // Wrapped before the next call can fail, so the card is released on
        // any path out of here.
        let fd = Fd(fd);
        fd.set_blocking()?;
        Ok(fd)
    }

    /// Drop `O_NONBLOCK` once the card is ours: the capture that follows is a
    /// blocking `READI_FRAMES` that must wait for its period, not return
    /// `EAGAIN`.
    fn set_blocking(&self) -> std::io::Result<()> {
        let flags = unsafe { libc::fcntl(self.0, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(self.0, libc::F_SETFL, flags & !libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    /// Raw `ioctl`, with the request widened to whatever this libc declares
    /// (`c_ulong` on glibc, `c_int` on musl) -- the bit pattern is the same.
    fn ioctl<T>(&self, request: u32, arg: *mut T) -> std::io::Result<()> {
        let r = unsafe { libc::ioctl(self.0, request as _, arg) };
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn ioctl_none(&self, request: u32) -> std::io::Result<()> {
        let r = unsafe { libc::ioctl(self.0, request as _) };
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

// --- opening -----------------------------------------------------------------

/// Open a capture device and negotiate a configuration for it.
pub fn open(spec: &OpenSpec) -> Result<Box<dyn Input>> {
    let candidates = candidates()?;
    if candidates.is_empty() {
        return Err(Error::Fatal(
            "Unable to initialize soundcard: no ALSA capture device under /dev/snd".into(),
        ));
    }

    // The first device that both opens and negotiates wins (a card held by
    // PipeWire or PulseAudio fails at `open` with EBUSY); if every one fails,
    // the first failure is the one reported.
    let mut first_error = None;
    for path in &candidates {
        match try_open(path, spec) {
            Ok(input) => return Ok(Box::new(input)),
            Err(e) => first_error.get_or_insert(e),
        };
    }
    Err(first_error.unwrap_or_else(|| {
        Error::Fatal("Unable to initialize soundcard: no usable ALSA capture device".into())
    }))
}

fn try_open(path: &Path, spec: &OpenSpec) -> Result<AlsaInput> {
    let fd = Fd::open(path).map_err(|e| open_error(path, &e))?;
    let proto = protocol_version(&fd);
    let name = device_name(&fd, path);
    let (config, period, buffer) = negotiate(&fd, spec).map_err(|e| open_error(path, &e))?;
    set_sw_params(&fd, proto, period, buffer);
    Ok(AlsaInput {
        fd,
        name,
        config,
        period,
    })
}

/// What went wrong, said in terms of what to do about it. `EBUSY` gets its
/// own sentence: this backend opens the card exclusively, and a running sound
/// server holds it.
fn open_error(path: &Path, e: &std::io::Error) -> Error {
    if e.raw_os_error() == Some(libc::EBUSY) {
        return Error::Fatal(
            format!(
                "Unable to initialize soundcard: {} is in use by another program. \
             This build records from the card directly and needs it to itself: \
             stop the sound server holding it (PipeWire or PulseAudio), name a \
             card it is not holding with CSW_ALSA_DEVICE=hw:<card>,<device>, or \
             use a build with the default 'record' feature -- the -glibc release \
             binary -- which records through the sound server instead.",
                path.display()
            )
            .into(),
        );
    }
    Error::Fatal(format!("Unable to initialize soundcard: {} ({e})", path.display()).into())
}

/// The capture devices to try, in order: `CSW_ALSA_DEVICE` alone if it is
/// set, otherwise everything under `/dev/snd` sorted by card then device.
fn candidates() -> Result<Vec<PathBuf>> {
    if let Some(spec) = std::env::var_os("CSW_ALSA_DEVICE") {
        let spec = spec.to_string_lossy().into_owned();
        let path = parse_device(&spec).ok_or_else(|| {
            Error::Fatal(format!(
                "CSW_ALSA_DEVICE='{spec}' is not a device: expected hw:CARD,DEV, CARD,DEV, CARD, \
                 or a /dev/snd path"
            ).into())
        })?;
        return Ok(vec![path]);
    }
    let mut found: Vec<(u32, u32, PathBuf)> = Vec::new();
    let dir = match std::fs::read_dir("/dev/snd") {
        Ok(d) => d,
        // No /dev/snd at all is reported as no capture device.
        Err(_) => return Ok(Vec::new()),
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        if let Some((card, device)) = parse_pcm_name(&name.to_string_lossy()) {
            found.push((card, device, entry.path()));
        }
    }
    found.sort();
    Ok(found.into_iter().map(|(_, _, p)| p).collect())
}

/// `pcmC<card>D<device>c` -- the trailing `c` is capture, `p` is playback.
fn parse_pcm_name(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_prefix("pcmC")?;
    let (card, rest) = rest.split_once('D')?;
    let device = rest.strip_suffix('c')?;
    Some((card.parse().ok()?, device.parse().ok()?))
}

/// `CSW_ALSA_DEVICE` in any of the spellings a user is likely to reach for.
fn parse_device(spec: &str) -> Option<PathBuf> {
    let spec = spec.trim();
    if spec.starts_with('/') {
        return Some(PathBuf::from(spec));
    }
    let rest = spec.strip_prefix("hw:").unwrap_or(spec);
    let (card, device) = match rest.split_once(',') {
        Some((c, d)) => (c.trim(), d.trim()),
        None => (rest, "0"),
    };
    let card: u32 = card.parse().ok()?;
    let device: u32 = device.parse().ok()?;
    Some(PathBuf::from(format!("/dev/snd/pcmC{card}D{device}c")))
}

/// The PCM protocol version the kernel speaks, which `SW_PARAMS` wants echoed
/// back at it. Zero if the kernel will not say, which is not fatal.
fn protocol_version(fd: &Fd) -> u32 {
    let mut version: i32 = 0;
    match fd.ioctl(sys::PVERSION, &mut version) {
        Ok(()) => version as u32,
        Err(_) => 0,
    }
}

/// The device's own name, with its `hw:` address appended so that what the
/// status line prints is also what `CSW_ALSA_DEVICE` accepts.
fn device_name(fd: &Fd, path: &Path) -> String {
    let mut info: PcmInfo = unsafe { std::mem::zeroed() };
    let address = match parse_pcm_name(&path.file_name().unwrap_or_default().to_string_lossy()) {
        Some((card, device)) => format!("hw:{card},{device}"),
        None => path.display().to_string(),
    };
    match fd.ioctl(sys::INFO, &mut info) {
        Ok(()) => {
            let name = c_str(&info.name);
            if name.is_empty() {
                address
            } else {
                format!("{name} ({address})")
            }
        }
        Err(_) => address,
    }
}

/// A NUL-terminated fixed-width kernel string.
fn c_str(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

// --- negotiation -------------------------------------------------------------

/// Fix a configuration on the device, returning it with the period and buffer
/// sizes the kernel settled on, both in frames.
fn negotiate(fd: &Fd, spec: &OpenSpec) -> std::io::Result<(Config, usize, usize)> {
    // What the hardware can do at all, once interleaved read/write is the
    // only access method on the table.
    let mut params = HwParams::any();
    params.masks[sys::P_ACCESS].set_single(sys::ACCESS_RW_INTERLEAVED);
    refine(fd, &mut params)?;

    // Format first: narrowing it can only widen what rates remain available.
    let format = best_format(&params).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "the device offers no sample format this build can convert",
        )
    })?;
    params.masks[sys::P_FORMAT].set_single(alsa_format(format));
    refine(fd, &mut params)?;

    // Mono if the card offers it, else the fewest channels, which the sink
    // folds down.
    let channels = if params.intervals[sys::P_CHANNELS].contains(1) {
        1
    } else {
        params.intervals[sys::P_CHANNELS].min.max(1)
    };
    params.intervals[sys::P_CHANNELS].set_single(channels);
    refine(fd, &mut params)?;

    // Rate: the one asked for if the hardware has it, else the best of the
    // ordinary ones it does have. `contains` only rules out the plainly
    // impossible -- an interval can still hide gaps -- so each candidate is
    // put to `refine` and the first the kernel accepts is the one.
    let wanted = rate_candidates(spec, &params.intervals[sys::P_RATE]);
    let (rate, mut params) = wanted
        .iter()
        .find_map(|&rate| {
            let mut attempt = params;
            attempt.intervals[sys::P_RATE].set_single(rate);
            refine(fd, &mut attempt).ok().map(|()| (rate, attempt))
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the device offers none of the capture rates DirectMode can use",
            )
        })?;

    // Period and buffer are a preference, not a requirement: if the hardware
    // will not take them, whatever it picks for itself still records.
    let target = (rate * PERIOD_MS / 1000).max(1);
    let mut sized = params;
    sized.intervals[sys::P_PERIOD_SIZE].set_single(target);
    sized.intervals[sys::P_PERIODS].set_single(PERIODS);
    if refine(fd, &mut sized).is_ok() {
        params = sized;
    }

    hw_params(fd, &mut params)?;

    let period = params.intervals[sys::P_PERIOD_SIZE].settled().max(1) as usize;
    let buffer = params.intervals[sys::P_BUFFER_SIZE]
        .settled()
        .max(period as u32) as usize;
    Ok((
        Config {
            rate,
            channels: channels as u16,
            format,
        },
        period,
        buffer,
    ))
}

/// Rates to try, best first.
fn rate_candidates(spec: &OpenSpec, available: &Interval) -> Vec<u32> {
    let mut out = Vec::with_capacity(RATES.len() + 1);
    if !spec.device_compat {
        out.push(spec.rate);
    }
    // `-c` differs only in not asking for DirectMode's rate first: a raw
    // device has no "own configuration" to defer to.
    out.extend_from_slice(&RATES);
    out.retain(|&r| available.contains(r));
    out.dedup();
    out
}

/// The best format the device offers, by the ranking both backends share.
fn best_format(params: &HwParams) -> Option<Format> {
    let mask = &params.masks[sys::P_FORMAT];
    let mut best: Option<Format> = None;
    for format in [
        Format::F32,
        Format::I16,
        Format::I32,
        Format::F64,
        Format::I8,
        Format::U16,
        Format::U8,
    ] {
        if mask.test(alsa_format(format))
            && best.is_none_or(|current| format.rank() < current.rank())
        {
            best = Some(format);
        }
    }
    best
}

/// This crate's formats as ALSA numbers them. Little-endian throughout, which
/// the module-level `compile_error!` guarantees is the host's order too.
fn alsa_format(format: Format) -> u32 {
    match format {
        Format::I8 => sys::FORMAT_S8,
        Format::U8 => sys::FORMAT_U8,
        Format::I16 => sys::FORMAT_S16_LE,
        Format::U16 => sys::FORMAT_U16_LE,
        Format::I32 => sys::FORMAT_S32_LE,
        Format::F32 => sys::FORMAT_FLOAT_LE,
        Format::F64 => sys::FORMAT_FLOAT64_LE,
    }
}

fn refine(fd: &Fd, params: &mut HwParams) -> std::io::Result<()> {
    params.rmask = !0;
    fd.ioctl(sys::HW_REFINE, params)
}

fn hw_params(fd: &Fd, params: &mut HwParams) -> std::io::Result<()> {
    params.rmask = !0;
    fd.ioctl(sys::HW_PARAMS, params)
}

/// Ask for the software parameters we want: an improvement on the kernel's
/// defaults, and if refused, recording still works.
fn set_sw_params(fd: &Fd, proto: u32, period: usize, buffer: usize) {
    // `boundary` is the buffer size doubled until just short of overflowing a
    // signed long, as alsa-lib computes it; the `max(1)` keeps a zero buffer
    // out of that loop.
    let buffer = (buffer as libc::c_ulong).max(1);
    let long_max = libc::c_ulong::MAX / 2;
    let mut boundary = buffer;
    while boundary.saturating_mul(2) <= long_max.saturating_sub(buffer) {
        boundary *= 2;
    }
    let mut params = SwParams {
        tstamp_mode: 0,
        period_step: 1,
        sleep_min: 0,
        // Wake the reader once a period is in, which is the cadence the
        // buffer was sized for.
        avail_min: period as libc::c_ulong,
        xfer_align: 1,
        // Capture starts on the explicit START below, not on the first read.
        start_threshold: 1,
        stop_threshold: buffer,
        silence_threshold: 0,
        silence_size: 0,
        boundary,
        proto,
        tstamp_type: 0,
        reserved: [0; 56],
    };
    let _ = fd.ioctl(sys::SW_PARAMS, &mut params);
}

// --- the opened device -------------------------------------------------------

struct AlsaInput {
    fd: Fd,
    name: String,
    config: Config,
    /// Frames per read, which is the period the kernel agreed to.
    period: usize,
}

impl Input for AlsaInput {
    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> Config {
        self.config
    }

    fn start(self: Box<Self>, sink: Arc<dyn SampleSink>) -> Result<Box<dyn Stream>> {
        let this = *self;
        this.fd
            .ioctl_none(sys::PREPARE)
            .and_then(|()| this.fd.ioctl_none(sys::START))
            .map_err(|e| Error::Fatal(format!("Cannot start recording: {e}").into()))?;

        let stop = Arc::new(AtomicBool::new(false));
        let reader = Reader {
            fd: this.fd,
            sink,
            stop: Arc::clone(&stop),
            config: this.config,
            period: this.period,
        };
        let handle = std::thread::Builder::new()
            .name("csw-alsa-capture".into())
            .spawn(move || reader.run())
            .map_err(|e| Error::Fatal(format!("Cannot start recording: {e}").into()))?;
        Ok(Box::new(AlsaStream {
            stop,
            handle: Some(handle),
        }))
    }
}

/// Stops the reader and waits for it, which is what guarantees the sink is
/// idle by the time DirectMode reads the spool.
struct AlsaStream {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Stream for AlsaStream {}

impl Drop for AlsaStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // The reader is inside a blocking read at worst, so this waits up to
        // one period. It owns the descriptor, so joining is what closes the
        // card.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

// --- the reader --------------------------------------------------------------

struct Reader {
    fd: Fd,
    sink: Arc<dyn SampleSink>,
    stop: Arc<AtomicBool>,
    config: Config,
    period: usize,
}

impl Reader {
    fn run(self) {
        let channels = self.config.channels as usize;
        let mut buf = Buffer::new(self.config.format, self.period * channels);
        while !self.stop.load(Ordering::Relaxed) {
            let mut xfer = XferI {
                result: 0,
                buf: buf.as_mut_ptr(),
                frames: self.period as libc::c_ulong,
            };
            match self.fd.ioctl(sys::READI_FRAMES, &mut xfer) {
                Ok(()) => {
                    let frames = xfer.result.max(0) as usize;
                    if frames > 0 {
                        self.sink.frames(buf.frames(frames * channels), channels);
                    }
                }
                Err(e) => {
                    if !self.recover(e, channels) {
                        self.sink.failed();
                        return;
                    }
                }
            }
        }
        // Leave the card stopped; a failure here changes nothing observable.
        let _ = self.fd.ioctl_none(sys::DROP);
    }

    /// Deal with a failed read. `true` to carry on, `false` to give up.
    fn recover(&self, e: std::io::Error, channels: usize) -> bool {
        match e.raw_os_error() {
            // A signal, not a fault.
            Some(libc::EINTR) => true,
            // The kernel's ring overran: it dropped audio while the reader
            // was elsewhere. Count the period we were in the middle of -- the
            // true figure is at least that -- and restart the stream.
            Some(libc::EPIPE) => {
                self.sink.dropped((self.period * channels) as u64);
                self.fd
                    .ioctl_none(sys::PREPARE)
                    .and_then(|()| self.fd.ioctl_none(sys::START))
                    .is_ok()
            }
            _ => false,
        }
    }
}

/// One period's worth of samples, in the type the device delivers, so that
/// the kernel writes into a correctly aligned buffer and the sink reads it
/// without a copy.
enum Buffer {
    U8(Vec<u8>),
    I8(Vec<i8>),
    I16(Vec<i16>),
    U16(Vec<u16>),
    I32(Vec<i32>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl Buffer {
    fn new(format: Format, samples: usize) -> Buffer {
        match format {
            Format::U8 => Buffer::U8(vec![0; samples]),
            Format::I8 => Buffer::I8(vec![0; samples]),
            Format::I16 => Buffer::I16(vec![0; samples]),
            Format::U16 => Buffer::U16(vec![0; samples]),
            Format::I32 => Buffer::I32(vec![0; samples]),
            Format::F32 => Buffer::F32(vec![0.0; samples]),
            Format::F64 => Buffer::F64(vec![0.0; samples]),
        }
    }

    fn as_mut_ptr(&mut self) -> *mut libc::c_void {
        match self {
            Buffer::U8(v) => v.as_mut_ptr().cast(),
            Buffer::I8(v) => v.as_mut_ptr().cast(),
            Buffer::I16(v) => v.as_mut_ptr().cast(),
            Buffer::U16(v) => v.as_mut_ptr().cast(),
            Buffer::I32(v) => v.as_mut_ptr().cast(),
            Buffer::F32(v) => v.as_mut_ptr().cast(),
            Buffer::F64(v) => v.as_mut_ptr().cast(),
        }
    }

    /// The first `samples` of the buffer, which is all the kernel filled.
    fn frames(&self, samples: usize) -> Frames<'_> {
        match self {
            Buffer::U8(v) => Frames::U8(&v[..samples.min(v.len())]),
            Buffer::I8(v) => Frames::I8(&v[..samples.min(v.len())]),
            Buffer::I16(v) => Frames::I16(&v[..samples.min(v.len())]),
            Buffer::U16(v) => Frames::U16(&v[..samples.min(v.len())]),
            Buffer::I32(v) => Frames::I32(&v[..samples.min(v.len())]),
            Buffer::F32(v) => Frames::F32(&v[..samples.min(v.len())]),
            Buffer::F64(v) => Frames::F64(&v[..samples.min(v.len())]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ioctl numbers as `sound/asound.h` yields them on a 64-bit kernel:
    /// a function of the struct sizes, so this checks the `size_of`
    /// assertions from the other end.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        assert_eq!(sys::PVERSION, 0x8004_4100);
        assert_eq!(sys::INFO, 0x8120_4101);
        assert_eq!(sys::HW_REFINE, 0xC260_4110);
        assert_eq!(sys::HW_PARAMS, 0xC260_4111);
        assert_eq!(sys::SW_PARAMS, 0xC088_4113);
        assert_eq!(sys::PREPARE, 0x0000_4140);
        assert_eq!(sys::START, 0x0000_4142);
        assert_eq!(sys::DROP, 0x0000_4143);
        assert_eq!(sys::READI_FRAMES, 0x8018_4151);
    }

    #[test]
    fn a_held_card_says_what_to_do_about_it() {
        let busy = open_error(
            Path::new("/dev/snd/pcmC0D0c"),
            &std::io::Error::from_raw_os_error(libc::EBUSY),
        );
        let msg = busy.to_string();
        assert!(msg.contains("/dev/snd/pcmC0D0c"), "{msg}");
        // The three ways out, all named.
        assert!(msg.contains("PipeWire"), "{msg}");
        assert!(msg.contains("CSW_ALSA_DEVICE"), "{msg}");
        assert!(msg.contains("-glibc"), "{msg}");
        // Anything else keeps the errno, which is all there is to say.
        let other = open_error(
            Path::new("/dev/snd/pcmC0D0c"),
            &std::io::Error::from_raw_os_error(libc::ENOENT),
        )
        .to_string();
        assert!(other.contains("/dev/snd/pcmC0D0c"), "{other}");
        assert!(!other.contains("PipeWire"), "{other}");
    }

    #[test]
    fn pcm_names_are_read_as_card_and_device() {
        assert_eq!(parse_pcm_name("pcmC0D0c"), Some((0, 0)));
        assert_eq!(parse_pcm_name("pcmC12D3c"), Some((12, 3)));
        // Playback devices, and everything else in /dev/snd, are not ours.
        assert_eq!(parse_pcm_name("pcmC0D0p"), None);
        assert_eq!(parse_pcm_name("controlC0"), None);
        assert_eq!(parse_pcm_name("timer"), None);
        assert_eq!(parse_pcm_name("pcmC0D0"), None);
    }

    #[test]
    fn device_overrides_are_accepted_in_every_spelling() {
        let want = PathBuf::from("/dev/snd/pcmC1D0c");
        assert_eq!(parse_device("hw:1,0").as_ref(), Some(&want));
        assert_eq!(parse_device("1,0").as_ref(), Some(&want));
        assert_eq!(parse_device("1").as_ref(), Some(&want));
        assert_eq!(parse_device(" hw:1, 0 ").as_ref(), Some(&want));
        assert_eq!(
            parse_device("/dev/snd/pcmC9D9c"),
            Some(PathBuf::from("/dev/snd/pcmC9D9c"))
        );
        assert_eq!(parse_device("hw:CARD=PCH,DEV=0"), None);
        assert_eq!(parse_device("default"), None);
    }

    #[test]
    fn open_bounds_narrow_an_interval() {
        let closed = Interval {
            min: 8_000,
            max: 48_000,
            flags: 0,
        };
        assert!(closed.contains(8_000));
        assert!(closed.contains(48_000));
        assert!(!closed.contains(7_999));
        assert!(!closed.contains(48_001));

        let open = Interval {
            min: 8_000,
            max: 48_000,
            flags: sys::INTERVAL_OPENMIN | sys::INTERVAL_OPENMAX,
        };
        assert!(!open.contains(8_000));
        assert!(!open.contains(48_000));
        assert!(open.contains(8_001));
        assert!(open.contains(47_999));
    }

    #[test]
    fn a_single_valued_mask_holds_only_that_value() {
        let mut mask = sys::Mask::any();
        mask.set_single(sys::FORMAT_S32_LE);
        assert!(mask.test(sys::FORMAT_S32_LE));
        assert!(!mask.test(sys::FORMAT_S16_LE));
        // Formats past 32 land in the second word.
        let mut wide = sys::Mask::any();
        wide.set_single(40);
        assert!(wide.test(40));
        assert!(!wide.test(8));
    }

    /// Build hw_params advertising exactly `formats`.
    fn params_offering(formats: &[u32]) -> HwParams {
        let mut params = HwParams::any();
        params.masks[sys::P_FORMAT].bits = [0; 8];
        for &f in formats {
            params.masks[sys::P_FORMAT].bits[(f / 32) as usize] |= 1 << (f % 32);
        }
        params
    }

    #[test]
    fn the_best_format_on_offer_wins() {
        // The shared ranking puts f32 first, then i16, then i32.
        assert_eq!(
            best_format(&params_offering(&[
                sys::FORMAT_U8,
                sys::FORMAT_S16_LE,
                sys::FORMAT_FLOAT_LE
            ])),
            Some(Format::F32)
        );
        assert_eq!(
            best_format(&params_offering(&[sys::FORMAT_S32_LE, sys::FORMAT_S16_LE])),
            Some(Format::I16)
        );
        assert_eq!(
            best_format(&params_offering(&[sys::FORMAT_S32_LE])),
            Some(Format::I32)
        );
        assert_eq!(best_format(&params_offering(&[])), None);
        // A device offering only formats this backend cannot convert (S24_LE
        // here) is one it has to decline.
        assert_eq!(best_format(&params_offering(&[6, 7])), None);
    }

    #[test]
    fn the_requested_rate_is_tried_first_and_impossible_ones_never() {
        let spec = OpenSpec {
            rate: 32_258,
            device_compat: false,
        };
        let wide = Interval {
            min: 8_000,
            max: 192_000,
            flags: 0,
        };
        let candidates = rate_candidates(&spec, &wide);
        assert_eq!(candidates[0], 32_258);
        assert_eq!(candidates[1], 48_000);

        // A card fixed at 48 kHz leaves exactly one candidate, and
        // DirectMode's own rate is not among them.
        let fixed = Interval {
            min: 48_000,
            max: 48_000,
            flags: sys::INTERVAL_INTEGER,
        };
        assert_eq!(rate_candidates(&spec, &fixed), vec![48_000]);
    }

    #[test]
    fn compat_does_not_ask_for_directmodes_rate() {
        let spec = OpenSpec {
            rate: 32_258,
            device_compat: true,
        };
        let wide = Interval {
            min: 8_000,
            max: 192_000,
            flags: 0,
        };
        let candidates = rate_candidates(&spec, &wide);
        assert_eq!(candidates[0], 48_000);
        assert!(!candidates.contains(&32_258));
    }

    #[test]
    fn a_short_read_is_reported_as_the_frames_that_arrived() {
        let mut buffer = Buffer::new(Format::I16, 8);
        if let Buffer::I16(v) = &mut buffer {
            v.copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        }
        match buffer.frames(4) {
            Frames::I16(d) => assert_eq!(d, &[1, 2, 3, 4]),
            _ => panic!("wrong buffer type"),
        }
        // A kernel claiming more than it was given never reads past the end.
        match buffer.frames(999) {
            Frames::I16(d) => assert_eq!(d.len(), 8),
            _ => panic!("wrong buffer type"),
        }
    }
}
