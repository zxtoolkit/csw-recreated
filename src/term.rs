//! The three things DirectMode asks of the terminal: raw mode, one keypress,
//! and the window width. System calls on Unix, `crossterm` on Windows. No
//! escape sequence is parsed: an arrow key and a letter are the same answer.

use std::time::Duration;

/// A keypress, reduced to what DirectMode asks of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    /// The character typed, lowercased, or `None` for anything that is not one
    /// (a function key, an escape sequence, a control code other than the ones
    /// below).
    pub ch: Option<char>,
    /// Ctrl was held. Only Ctrl-C is acted on, and only to stop.
    pub ctrl: bool,
}

impl KeyPress {
    /// Ctrl-C, which raw mode would otherwise swallow.
    pub fn is_interrupt(&self) -> bool {
        self.ctrl && self.ch == Some('c')
    }
    /// The key that pauses, in either case.
    pub fn is(&self, want: char) -> bool {
        !self.ctrl && self.ch == Some(want)
    }
}

#[cfg(unix)]
mod imp {
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;
    use std::time::Duration;

    use super::KeyPress;

    /// Put the terminal in raw mode, returning the settings to restore.
    /// `None` when stdin is not a terminal, which is not an error: DirectMode
    /// runs off `-t` instead.
    pub fn enable_raw_mode() -> Option<Saved> {
        let fd = std::io::stdin().as_raw_fd();
        // SAFETY: `termios` is plain data, and `tcgetattr` fills it or fails.
        let mut saved = MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(fd, saved.as_mut_ptr()) } != 0 {
            return None;
        }
        let saved = unsafe { saved.assume_init() };

        // `cfmakeraw`: no canonical line editing, no echo, no signal from
        // Ctrl-C, so Ctrl-C has to be recognised as a keypress.
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        // Return from `read` as soon as one byte is there. `poll` has already
        // said there is something, so this only bounds the escape sequence
        // that came with it.
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return None;
        }
        Some(Saved(saved))
    }

    /// The terminal settings from before raw mode.
    pub struct Saved(libc::termios);

    pub fn disable_raw_mode(saved: &Saved) {
        let fd = std::io::stdin().as_raw_fd();
        // TCSANOW, not TCSAFLUSH: anything already typed belongs to whatever
        // runs next.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved.0) };
    }

    /// Wait up to `timeout` for a keypress.
    pub fn poll_key(timeout: Duration) -> std::io::Result<Option<KeyPress>> {
        let fd = std::io::stdin().as_raw_fd();
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // `poll` takes whole milliseconds and rounds down, so a sub-millisecond
        // timeout would spin. One millisecond is the floor.
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let ready = unsafe { libc::poll(&mut pfd, 1, ms.max(1)) };
        if ready < 0 {
            let e = std::io::Error::last_os_error();
            // A signal interrupting the wait is not a keypress and not a
            // failure; the caller's loop comes straight back here.
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(e);
        }
        if ready == 0 || pfd.revents & libc::POLLIN == 0 {
            return Ok(None);
        }

        // Take everything that arrived together, so an escape sequence is one
        // keypress. Read the descriptor `poll` reported on, not `Stdin`,
        // whose buffer `poll` cannot see.
        let mut buf = [0u8; 32];
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(e);
        }
        let n = n as usize;
        if n == 0 {
            // EOF on stdin: no more keys will ever come. Report it as one
            // press so a session waiting on a key ends.
            return Ok(Some(KeyPress {
                ch: None,
                ctrl: false,
            }));
        }
        Ok(Some(decode(&buf[..n])))
    }

    /// One keypress from the bytes raw mode delivered for it.
    fn decode(bytes: &[u8]) -> KeyPress {
        let first = bytes[0];
        // A control character is the letter it is typed with, minus 0x60.
        if (0x01..=0x1a).contains(&first) && bytes.len() == 1 {
            return KeyPress {
                ch: Some((first + 0x60) as char),
                ctrl: true,
            };
        }
        // Anything multi-byte is an escape sequence or a UTF-8 character; both
        // count as "a key was pressed" and neither is `p`.
        let ch = (bytes.len() == 1 && first.is_ascii_graphic())
            .then(|| first.to_ascii_lowercase() as char);
        KeyPress { ch, ctrl: false }
    }

    /// Terminal width in columns, `None` if it cannot be had.
    pub fn width() -> Option<u16> {
        let fd = std::io::stdout().as_raw_fd();
        let mut ws = MaybeUninit::<libc::winsize>::uninit();
        // SAFETY: TIOCGWINSZ fills a `winsize` or fails; nothing else is read.
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ as _, ws.as_mut_ptr()) } != 0 {
            return None;
        }
        Some(unsafe { ws.assume_init() }.ws_col)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_plain_letter_is_that_letter() {
            assert_eq!(decode(b"p").ch, Some('p'));
            assert_eq!(decode(b"P").ch, Some('p'));
            assert!(!decode(b"p").ctrl);
            assert!(decode(b"p").is('p'));
            assert!(decode(b"P").is('p'));
        }

        #[test]
        fn ctrl_c_is_recognised_because_raw_mode_swallows_the_signal() {
            let k = decode(&[0x03]);
            assert!(k.ctrl);
            assert_eq!(k.ch, Some('c'));
            assert!(k.is_interrupt());
            // Ctrl-P is not the pause key: `is` requires the modifier absent.
            assert!(!decode(&[0x10]).is('p'));
        }

        #[test]
        fn an_escape_sequence_is_one_press_and_is_not_a_letter() {
            // Up arrow. Three bytes, one keypress, stops the recording like
            // any other key that is not `p`.
            let k = decode(b"\x1b[A");
            assert_eq!(k.ch, None);
            assert!(!k.is('p'));
            assert!(!k.is_interrupt());
        }

        #[test]
        fn multibyte_input_is_not_mistaken_for_a_letter() {
            // A pasted 'é' is two bytes; it must not decode as a stray ASCII
            // one, and must not be an interrupt.
            let k = decode("é".as_bytes());
            assert_eq!(k.ch, None);
            assert!(!k.ctrl);
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::time::Duration;

    use super::KeyPress;

    /// Windows console input is an API, not a byte stream, so `crossterm`
    /// keeps this platform.
    pub struct Saved;

    pub fn enable_raw_mode() -> Option<Saved> {
        crossterm::terminal::enable_raw_mode().ok().map(|()| Saved)
    }

    pub fn disable_raw_mode(_saved: &Saved) {
        let _ = crossterm::terminal::disable_raw_mode();
    }

    pub fn poll_key(timeout: Duration) -> std::io::Result<Option<KeyPress>> {
        use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

        if !event::poll(timeout)? {
            return Ok(None);
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => Ok(Some(KeyPress {
                ch: match k.code {
                    KeyCode::Char(c) => Some(c.to_ascii_lowercase()),
                    _ => None,
                },
                ctrl: k.modifiers.contains(KeyModifiers::CONTROL),
            })),
            // Resizes and mouse events are not keypresses; keep going.
            _ => Ok(None),
        }
    }

    pub fn width() -> Option<u16> {
        crossterm::terminal::size().ok().map(|(cols, _)| cols)
    }
}

pub use imp::Saved;

pub fn width() -> Option<u16> {
    #[cfg(test)]
    match TEST_WIDTH.load(std::sync::atomic::Ordering::Relaxed) {
        UNSET => {}
        NO_TERMINAL => return None,
        cols => return Some(cols as u16),
    }
    imp::width()
}

#[cfg(test)]
const UNSET: i32 = -1;
#[cfg(test)]
const NO_TERMINAL: i32 = 0;
#[cfg(test)]
static TEST_WIDTH: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(UNSET);
#[cfg(test)]
static WIDTH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub struct FixedWidth(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl Drop for FixedWidth {
    fn drop(&mut self) {
        TEST_WIDTH.store(UNSET, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
pub fn fixed_width(cols: Option<u16>) -> FixedWidth {
    let held = WIDTH_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    TEST_WIDTH.store(
        cols.map_or(NO_TERMINAL, i32::from),
        std::sync::atomic::Ordering::Relaxed,
    );
    FixedWidth(held)
}

/// Put the terminal in raw mode. `None` when there is no terminal.
pub fn enable_raw_mode() -> Option<Saved> {
    imp::enable_raw_mode()
}

/// Restore what [`enable_raw_mode`] saved.
pub fn disable_raw_mode(saved: &Saved) {
    imp::disable_raw_mode(saved)
}

/// Wait up to `timeout` for a keypress.
pub fn poll_key(timeout: Duration) -> std::io::Result<Option<KeyPress>> {
    imp::poll_key(timeout)
}
