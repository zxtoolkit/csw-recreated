//! The DirectMode sample spool: captured audio goes to a temporary file, one
//! byte per sample, and the encoder reads it back. The recording length is
//! bounded by free space, which the "Max recording time" line reports.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::{Error, Result};

/// Bytes one spooled sample occupies.
pub const BYTES_PER_SAMPLE: u64 = 1;

pub use ::csw::source::CHUNK;

/// A temporary file that removes itself.
struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        clear_signal_cleanup();
    }
}

/// Unlink the spool when a signal ends the process, since `Drop` will not run.
///
/// Under `-t` with no terminal Ctrl-C is SIGINT. A handler may not allocate,
/// so the path is converted and **leaked** before the handler is armed; the
/// handler only `unlink`s, restores the default disposition and re-raises.
/// Panics are not covered: `panic = "abort"` skips `Drop` too.
#[cfg(unix)]
mod signal_cleanup {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::Once;
    use std::sync::atomic::{AtomicPtr, Ordering};

    static SPOOL: AtomicPtr<libc::c_char> = AtomicPtr::new(std::ptr::null_mut());
    static ARMED: Once = Once::new();

    extern "C" fn on_signal(sig: libc::c_int) {
        let path = SPOOL.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !path.is_null() {
            // SAFETY: the pointer came from a leaked CString and stays valid;
            // `unlink` is async-signal-safe.
            unsafe { libc::unlink(path) };
        }
        // SAFETY: restoring the default and re-raising is the documented way
        // to die of the signal we were handed.
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    pub fn register(path: &Path) {
        let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
            return;
        };
        // Leaked deliberately: a handler can run after everything else is gone.
        let old = SPOOL.swap(c.into_raw(), Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: ours, from an earlier `register`, and no handler can be
            // holding it -- the swap above took it out of reach first.
            drop(unsafe { CString::from_raw(old) });
        }
        ARMED.call_once(|| {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
                // SAFETY: `on_signal` is a plain extern "C" fn of the right
                // shape and touches nothing a handler may not.
                unsafe {
                    libc::signal(
                        sig,
                        on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
                    )
                };
            }
        });
    }

    pub fn clear() {
        let old = SPOOL.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: as above.
            drop(unsafe { CString::from_raw(old) });
        }
    }
}

#[cfg(unix)]
fn register_for_signal_cleanup(path: &Path) {
    signal_cleanup::register(path);
}

#[cfg(unix)]
fn clear_signal_cleanup() {
    signal_cleanup::clear();
}

/// Windows has no signal to catch: console Ctrl-C tears the process down
/// without a `Drop`, and a spool may be left behind (`.gitignore` names the
/// pattern).
#[cfg(not(unix))]
fn register_for_signal_cleanup(_path: &Path) {}

#[cfg(not(unix))]
fn clear_signal_cleanup() {}

/// The write side: the meter loop drains the capture buffer into this.
pub struct Spool {
    path: TempPath,
    file: BufWriter<File>,
    samples: u64,
}

impl Spool {
    /// Create the spool beside `dir`, so the free space that bounds the
    /// recording is the space on the volume the output lands on.
    pub fn create(dir: &Path) -> Result<Self> {
        // The counter keeps two spools in one process apart.
        static N: AtomicU32 = AtomicU32::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".csw-spool-{}-{id}.tmp", std::process::id()));
        // `create_new`: a name collision is a spool still in use, or one a
        // killed run left behind, and neither is truncated.
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| Error::Fatal(format!("Cannot create the recording spool: {e}").into()))?;
        register_for_signal_cleanup(&path);
        Ok(Spool {
            path: TempPath(path),
            file: BufWriter::with_capacity(1 << 16, file),
            samples: 0,
        })
    }

    /// Samples spooled so far.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Append one drained buffer. A write failure ends the recording.
    pub fn write(&mut self, samples: &[f64]) -> Result<()> {
        // The quantiser the keep-file and the encoder use.
        let bytes = ::csw::detect::to_byte_domain(samples, ::csw::detect::MIDPOINT);
        self.file
            .write_all(&bytes)
            .map_err(|e| Error::Fatal(format!("Cannot write the recording spool: {e}").into()))?;
        self.samples += samples.len() as u64;
        Ok(())
    }

    /// Close the write side and reopen for reading.
    ///
    /// A failed flush loses the buffered tail, not the take: the error is
    /// returned beside the reader, and the sample count is taken from the
    /// file, since `write` counted what it handed to the buffer.
    pub fn finish(mut self) -> Result<(SpoolReader, Option<String>)> {
        let failure = self
            .file
            .flush()
            .err()
            .map(|e| format!("Cannot write the recording spool: {e}"));
        let file = File::open(&self.path.0)
            .map_err(|e| Error::Fatal(format!("Cannot read the recording spool: {e}").into()))?;
        let on_disk = file
            .metadata()
            .map(|m| m.len() / BYTES_PER_SAMPLE)
            .unwrap_or(self.samples);
        Ok((
            SpoolReader {
                _path: self.path,
                file: BufReader::with_capacity(1 << 16, file),
                samples: on_disk.min(self.samples),
                read: 0,
                buf: Vec::new(),
            },
            failure,
        ))
    }
}

/// How many samples may be spooled into `free` bytes.
///
/// The spool is still on disk when the CSW is written beside it, and `-k`
/// adds a WAV of the spool's size, so the space is shared out: two parts
/// spool, one part CSW, two more for the keep-file when there is one.
pub fn sample_budget(free: u64, keep: bool) -> u64 {
    let parts = if keep { 5 } else { 3 };
    free / BYTES_PER_SAMPLE * 2 / parts
}

/// The read side: the encoder pulls chunks back out, and the file goes away
/// when this is dropped.
pub struct SpoolReader {
    _path: TempPath,
    file: BufReader<File>,
    samples: u64,
    read: u64,
    /// Reused across chunks; see [`crate::source::SampleSource`].
    buf: Vec<u8>,
}

impl SpoolReader {
    /// How many samples were captured.
    pub fn len(&self) -> u64 {
        self.samples
    }

    pub fn is_empty(&self) -> bool {
        self.samples == 0
    }

    /// Start over, so the samples can be walked a second time (`-k` writes the
    /// keep-file from one pass, the encoder reads the next).
    pub fn rewind(&mut self) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| Error::Fatal(format!("Cannot read the recording spool: {e}").into()))?;
        self.read = 0;
        Ok(())
    }

    /// Fill `out` with up to [`CHUNK`] samples; `false` at the end.
    pub fn next_chunk(&mut self, out: &mut Vec<f64>) -> Result<bool> {
        out.clear();
        let want = CHUNK.min((self.samples - self.read) as usize);
        if want == 0 {
            return Ok(false);
        }
        self.buf.resize(want * BYTES_PER_SAMPLE as usize, 0);
        self.file
            .read_exact(&mut self.buf)
            .map_err(|e| Error::Fatal(format!("Cannot read the recording spool: {e}").into()))?;
        out.extend(self.buf.iter().map(|&b| b as f64));
        self.read += want as u64;
        Ok(true)
    }
}

/// Free space on the volume holding `dir`, or `None` if it cannot be asked.
/// This is the bound behind the "Max recording time" line.
#[cfg(unix)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: `path` is a valid NUL-terminated C string for the duration of
    // the call, and `stat` is only read after statvfs reports success.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        // f_frsize is the fragment size the free-block count is in.
        let unit = if stat.f_frsize > 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        Some(stat.f_bavail as u64 * unit)
    }
}

#[cfg(windows)]
pub fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call,
    // and `available` is only read when the call reports success.
    unsafe {
        if GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            return None;
        }
    }
    Some(available)
}

#[cfg(not(any(unix, windows)))]
pub fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spool never gets the whole volume: the CSW is written while it is
    /// still there, and `-k` needs as much again.
    #[test]
    fn the_budget_leaves_room_for_what_is_written_beside_the_spool() {
        // Two parts spool, one CSW, two more for the keep-file.
        assert_eq!(sample_budget(3000, false), 2000);
        assert_eq!(sample_budget(5000, true), 2000);
        // Whatever is left over, the spool plus its outputs must fit.
        for free in [0u64, 1, 7, 1000, 1 << 30] {
            for keep in [false, true] {
                let n = sample_budget(free, keep);
                let needed = n + n / 2 + if keep { n } else { 0 };
                assert!(
                    needed <= free,
                    "{n} samples do not fit in {free} (keep={keep})"
                );
            }
        }
    }

    #[test]
    fn spool_round_trips_samples_in_chunks() {
        let dir = std::env::temp_dir();
        let mut spool = Spool::create(&dir).expect("create");
        let written: Vec<f64> = (0..CHUNK + 1000).map(|i| (i % 256) as f64).collect();
        for part in written.chunks(1000) {
            spool.write(part).expect("write");
        }
        assert_eq!(spool.samples(), written.len() as u64);

        let (mut reader, failure) = spool.finish().expect("finish");
        assert!(failure.is_none(), "clean spool: {failure:?}");
        assert_eq!(reader.len(), written.len() as u64);
        let mut got = Vec::new();
        let mut chunk = Vec::new();
        while reader.next_chunk(&mut chunk).expect("read") {
            assert!(chunk.len() <= CHUNK);
            got.extend_from_slice(&chunk);
        }
        assert_eq!(got, written);

        // A second pass sees the same samples.
        reader.rewind().expect("rewind");
        let mut again = Vec::new();
        while reader.next_chunk(&mut chunk).expect("read") {
            again.extend_from_slice(&chunk);
        }
        assert_eq!(again, written);
    }

    /// The spool rounds each sample to a byte; the fraction does not survive.
    #[test]
    fn the_spool_rounds_to_a_byte() {
        let dir = std::env::temp_dir();
        let mut spool = Spool::create(&dir).expect("create");
        spool.write(&[128.4, 129.6, 0.2, 254.8]).expect("write");
        let (mut reader, failure) = spool.finish().expect("finish");
        assert!(failure.is_none(), "clean spool: {failure:?}");
        let mut got = Vec::new();
        let mut chunk = Vec::new();
        while reader.next_chunk(&mut chunk).expect("read") {
            got.extend_from_slice(&chunk);
        }
        assert_eq!(got, vec![128.0, 130.0, 0.0, 255.0]);
    }

    #[test]
    fn the_spool_file_is_removed_with_the_reader() {
        let dir = std::env::temp_dir();
        let spool = Spool::create(&dir).expect("create");
        let path = spool.path.0.clone();
        assert!(path.exists());
        let (reader, failure) = spool.finish().expect("finish");
        assert!(failure.is_none(), "clean spool: {failure:?}");
        assert!(path.exists());
        drop(reader);
        assert!(!path.exists(), "spool file left behind: {path:?}");
    }

    #[test]
    fn free_space_is_reported_for_a_real_directory() {
        // Only that the platform call works where it is implemented; no value
        // is asserted.
        let got = free_bytes(&std::env::temp_dir());
        if cfg!(any(unix, windows)) {
            assert!(
                got.is_some_and(|b| b > 0),
                "no free space reported: {got:?}"
            );
        }
    }
}
