//! Where the daemon is listening, and how one connection to it is made.
//!
//! The socket path resolves exactly as the daemon's own CLI resolves it, because
//! a client and a daemon that disagree about the path disagree about everything
//! (docs/architecture.md 2.1, `crates/whirl-cli/src/main.rs`):
//!
//! 1. `WHIRL_SOCKET`, if it is set;
//! 2. the `socket` value of the config file the daemon read (`WHIRL_CONFIG`, or
//!    the platform default);
//! 3. the platform default.
//!
//! Reading the config file here is not "reading the config file and treating it
//! as the effective plan" (section 8, "must never" 6). It is a path lookup: a
//! client must know where the socket is before it can ask the daemon anything,
//! and there is nothing else to ask. Nothing this module returns is a setting.
//!
//! A config that cannot be read or parsed falls through to the platform default,
//! which is what the daemon's CLI does: the client is not the program that
//! refuses a bad config, and it has no window in which to say so.

use std::env;
use std::path::{Path, PathBuf};

#[cfg(not(unix))]
use std::io;

use whirl_core::config::{Config, paths};

use crate::error::{ClientError, Socket};

/// The variable that names the socket directly, and wins over everything.
pub const SOCKET_VAR: &str = "WHIRL_SOCKET";
/// The variable that names the config file whose `socket` is the second choice.
pub const CONFIG_VAR: &str = "WHIRL_CONFIG";

/// The config file this client would read, in the daemon's own precedence:
/// `WHIRL_CONFIG`, then the platform default for this account.
pub fn config_path() -> Option<PathBuf> {
    env::var_os(CONFIG_VAR)
        .map(PathBuf::from)
        .or_else(paths::config_file)
}

/// The socket path, resolved in the order above.
///
/// `Err` when none of the three sources names one, which on a supported platform
/// means neither `WHIRL_SOCKET` nor a home directory is available.
pub fn socket_path() -> Result<PathBuf, ClientError> {
    if let Some(path) = env::var_os(SOCKET_VAR) {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = configured_socket() {
        return Ok(path);
    }
    paths::socket_file().ok_or_else(|| ClientError::Unreachable(Socket::unresolved()))
}

/// The `socket` value of the daemon's config file, when there is a readable one
/// that parses and sets it.
fn configured_socket() -> Option<PathBuf> {
    let path = config_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let loaded = Config::parse(&text).ok()?;
    loaded.config.socket
}

/// The connected socket type.
///
/// The transport of docs/architecture.md 2.1 is a unix domain socket on macOS and
/// Linux, and a named pipe on Windows. The pipe is a later card; until it lands
/// the Windows type exists, compiles and refuses, so that one `cfg` here is the
/// whole of the platform difference and the rest of the crate is portable.
#[cfg(unix)]
pub use std::os::unix::net::UnixStream as Stream;

/// The Windows transport, unimplemented (see the `cfg` note above).
///
/// It is a type rather than a compile error so that this crate, the app and the
/// three-platform CI matrix all build the same code; every method fails, and
/// [`connect`] never returns one.
#[cfg(not(unix))]
#[derive(Debug)]
pub struct Stream;

#[cfg(not(unix))]
impl io::Read for Stream {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(unsupported())
    }
}

#[cfg(not(unix))]
impl io::Write for Stream {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(unsupported())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(not(unix))]
impl Stream {
    pub fn try_clone(&self) -> io::Result<Stream> {
        Err(unsupported())
    }
    pub fn set_read_timeout(&self, _timeout: Option<std::time::Duration>) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "the Windows transport is a named pipe and is not implemented yet (docs/architecture.md 2.1)",
    )
}

/// One connection to the socket at `path`.
///
/// A failure here never unlinks anything: a socket file with nothing behind it is
/// a stale socket, the daemon's to remove and never a client's (section 8, "must
/// never" 5).
#[cfg(unix)]
pub fn connect(path: &Path) -> Result<Stream, ClientError> {
    Stream::connect(path).map_err(|error| ClientError::Unreachable(Socket::new(path, &error)))
}

#[cfg(not(unix))]
pub fn connect(path: &Path) -> Result<Stream, ClientError> {
    let error = io::Error::new(
        io::ErrorKind::Unsupported,
        "the Windows transport is a named pipe and is not implemented yet (docs/architecture.md 2.1)",
    );
    Err(ClientError::Unreachable(Socket::new(path, &error)))
}
