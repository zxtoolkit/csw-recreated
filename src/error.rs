//! Crate-wide error type.

use std::fmt;

/// A message carrying bytes from the command line: a switch token or a file
/// name as argv spelled it.
///
/// Argv is bytes, and the lines that quote it print those bytes back: a
/// switch `-\x8e\xa0` is reported as `-D\xa0`, whose tail is no character on
/// this host.
pub struct Msg(Vec<u8>);

impl Msg {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[cfg(test)]
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix.as_bytes())
    }
}

#[cfg(test)]
impl PartialEq<&str> for Msg {
    fn eq(&self, other: &&str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl From<String> for Msg {
    fn from(s: String) -> Self {
        Msg(s.into_bytes())
    }
}

impl From<&str> for Msg {
    fn from(s: &str) -> Self {
        Msg(s.as_bytes().to_vec())
    }
}

impl From<Vec<u8>> for Msg {
    fn from(v: Vec<u8>) -> Self {
        Msg(v)
    }
}

/// The lossy text form; the console writes [`Msg::as_bytes`], never this.
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Debug for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(&self.0))
    }
}

#[derive(Debug)]
pub enum Error {
    /// Underlying I/O failure.
    Io(std::io::Error),
    /// The input did not match the expected container/format.
    Format(String),
    /// A request this build or the container cannot carry out.
    Unsupported(String),
    /// A pre-formatted fatal message shown verbatim after "FATAL ERROR: ".
    Fatal(Msg),
    /// A message shown verbatim after "ERROR: " for a CSW whose header was
    /// already described on the console; the byte is the exit code.
    Input(Msg, u8),
    /// A refusal that lands on the open "Checking input file..." line. The
    /// string is the completion printed there in place of "ok!" (e.g.
    /// "Sorry, stereo WAV samples not yet supported."); the byte is the exit
    /// code.
    Rejected(String, u8),
    /// A refusal that lands on the "Checking input file..." line *and* is
    /// followed by a FATAL line: the first string completes the checking line
    /// in place of "ok!", the second is the fatal message under it.
    CheckedFatal(String, String),
    /// A fatal message of several lines, printed flat -- no leading CR, no
    /// blank line after. Used for the multi-line advice given when an OUT
    /// trace carries no port-0xFE writes to convert.
    FatalBlock(Msg),
    /// A refusal printed as a bare line, with no prefix and no "Checking
    /// input file..." line before it: how a VOC that cannot be read is turned
    /// down ("Sorry, only 8-bit mono PCM VOC files are supported.").
    Refused(String),
    /// A failure reported by saying nothing at all: the banner is printed,
    /// the run is abandoned, and the output file is left empty. The message
    /// is carried for tests and `fmt::Display`, never printed.
    Silent(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Format(m) => write!(f, "format error: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::Silent(m) | Error::Refused(m) => write!(f, "{m}"),
            Error::Fatal(m) | Error::FatalBlock(m) | Error::Input(m, _) => write!(f, "{m}"),
            Error::CheckedFatal(_, m) | Error::Rejected(m, _) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
