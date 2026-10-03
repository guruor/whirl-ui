//! The ways a conversation with the daemon ends badly, and what a caller does
//! about each.
//!
//! Two rules shape this module.
//!
//! The first is section 2.7: a failure is a typed code from a closed set, never a
//! sentence to be matched. [`Refusal`] carries the code the daemon sent, and
//! [`Action`] is section 8 item 5's list of the five a frontend can act on, by
//! name.
//!
//! The second is this crate's own: **no diagnostic names a path.** A socket path
//! is a home path on every platform the daemon supports, and a message that
//! carries one leaks the account it ran under into a log, a screenshot or a bug
//! report. [`Socket`] therefore reports the socket file's *name* and its mode,
//! which is what a diagnostic actually needs, and nothing here prints a
//! directory.

use std::fmt;
use std::io;
use std::path::Path;

use whirl_core::protocol::{ErrorCode, ProtocolError};

/// What a frontend does about a refusal, for the five codes section 8 item 5
/// names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `busy`: a rotation is in flight. Retry later, or leave the row enabled
    /// and let the user press it again.
    RetryLater,
    /// `not_found`: the id is stale. Re-read `status`.
    ReReadStatus,
    /// `timeout`: a rotation is probably still running. The next event settles
    /// it; do not report a failure yet.
    RotationMayBeRunning,
    /// `favorites_degraded`: pin state is unknown, so hide the pin affordance.
    HidePin,
    /// `bad_config`: point the user at `whirl config check`.
    ConfigCheck,
}

/// One `ERR` line: the code, which is a contract, and the message, which is for
/// a human and may name the offending value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: ErrorCode,
    pub message: String,
}

impl Refusal {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Refusal {
        Refusal {
            code,
            message: message.into(),
        }
    }

    /// The action section 8 item 5 names for this code, if it names one.
    pub fn action(&self) -> Option<Action> {
        match self.code {
            ErrorCode::Busy => Some(Action::RetryLater),
            ErrorCode::NotFound => Some(Action::ReReadStatus),
            ErrorCode::Timeout => Some(Action::RotationMayBeRunning),
            ErrorCode::FavoritesDegraded => Some(Action::HidePin),
            ErrorCode::BadConfig => Some(Action::ConfigCheck),
            _ => None,
        }
    }

    /// Whether the daemon closes the connection after this refusal
    /// (docs/architecture.md 2.7).
    pub fn closes(&self) -> bool {
        self.code.closes()
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

/// Where the socket was, without saying where it was.
///
/// A socket is `whirl.sock` in a directory the daemon owns, and the directory is
/// a home path. A diagnostic needs the leaf, the mode and what the platform said
/// when the connection failed; it does not need the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socket {
    /// The file's own name, e.g. `whirl.sock`.
    pub name: String,
    /// The file's permission bits, when the file exists at all.
    pub mode: Option<u32>,
    pub kind: io::ErrorKind,
    /// The platform's message, which carries an errno and no path.
    pub message: String,
}

impl Socket {
    /// Describe a socket file and the error a connection to it produced.
    pub fn new(path: &Path, error: &io::Error) -> Socket {
        // `whirl.sock` is the last component of the path the caller resolved, and
        // the only part of it that is not the user's own directory layout.
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "the control socket".to_string());
        let mode = std::fs::metadata(path)
            .ok()
            .map(|metadata| mode_of(&metadata));
        Socket {
            name,
            mode,
            kind: error.kind(),
            message: error.to_string(),
        }
    }

    /// A socket path could not be resolved at all: no `WHIRL_SOCKET`, no
    /// `socket` in the config, no platform default.
    pub fn unresolved() -> Socket {
        Socket {
            name: "whirl.sock".to_string(),
            mode: None,
            kind: io::ErrorKind::NotFound,
            message: "no socket path is configured and this platform has no default".to_string(),
        }
    }

    /// Whether the file is there but nothing is listening on it. This is the
    /// stale socket: the daemon died without unlinking it, and a client must
    /// never unlink it either (section 8, "must never" 5).
    pub fn stale(&self) -> bool {
        self.mode.is_some() && self.kind == io::ErrorKind::ConnectionRefused
    }
}

#[cfg(unix)]
fn mode_of(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn mode_of(_metadata: &std::fs::Metadata) -> u32 {
    0
}

impl fmt::Display for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.mode {
            Some(mode) => write!(f, "{} (mode {mode:04o})", self.name),
            None => write!(f, "{} (absent)", self.name),
        }?;
        if self.stale() {
            write!(f, " [stale socket: {kind}]", kind = kind_name(self.kind))?;
        }
        write!(f, ": {}", self.message)
    }
}

/// The platform's name for an `io::ErrorKind`, which is one word and never a
/// path.
fn kind_name(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "not found",
        io::ErrorKind::ConnectionRefused => "connection refused",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => "timed out",
        _ => "i/o error",
    }
}

/// Everything this client can fail at.
#[derive(Debug)]
pub enum ClientError {
    /// The socket could not be connected to: absent, stale, refused, or not
    /// permitted.
    Unreachable(Socket),
    /// The greeting announced a protocol version this client does not speak.
    /// Section 8 item 1 makes refusing to run the behaviour, not a warning.
    UnknownProtocol {
        server: String,
        supported: &'static [u32],
    },
    /// A line arrived where the greeting should have been, and it was not one.
    MalformedGreeting { line: String },
    /// The daemon refused the request (section 2.7). A legal answer, not a
    /// transport failure.
    Refused(Refusal),
    /// A line the grammar does not allow. The daemon and this client disagree.
    Protocol(ProtocolError),
    /// The connection itself failed.
    Io(io::Error),
}

impl ClientError {
    /// The refusal, when this is one.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            ClientError::Refused(refusal) => Some(refusal),
            _ => None,
        }
    }

    /// Whether the daemon is simply not there. A frontend draws its
    /// "not running" state from this and does not treat it as a bug.
    pub fn unreachable(&self) -> bool {
        matches!(self, ClientError::Unreachable(_))
    }

    /// Whether the read gave up waiting rather than failing.
    ///
    /// A command connection waits [`COMMAND_TIMEOUT`](crate::client::COMMAND_TIMEOUT)
    /// for an answer, and a subscription waits its liveness window (90 s) for a
    /// line at all; for the second, giving up *is* the signal that the daemon is
    /// gone (section 8 item 4), so the distinction has to survive this far.
    pub fn timed_out(&self) -> bool {
        match self {
            ClientError::Io(error) => matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            _ => false,
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Unreachable(socket) => write!(f, "the daemon is not reachable: {socket}"),
            ClientError::UnknownProtocol { server, supported } => write!(
                f,
                "the daemon speaks protocol {server} and this client speaks {}",
                supported
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ClientError::MalformedGreeting { line } => {
                write!(f, "the daemon's greeting is not a greeting: {line:?}")
            }
            ClientError::Refused(refusal) => write!(f, "the daemon refused: {refusal}"),
            ClientError::Protocol(error) => {
                write!(f, "the daemon sent what the grammar forbids: {error}")
            }
            ClientError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ClientError::Protocol(error) => Some(error),
            ClientError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ProtocolError> for ClientError {
    fn from(error: ProtocolError) -> ClientError {
        ClientError::Protocol(error)
    }
}

impl From<io::Error> for ClientError {
    fn from(error: io::Error) -> ClientError {
        ClientError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_diagnostic_names_the_socket_and_its_mode_and_no_directory() {
        let socket = Socket {
            name: "whirl.sock".to_string(),
            mode: Some(0o600),
            kind: io::ErrorKind::ConnectionRefused,
            message: "Connection refused (os error 61)".to_string(),
        };
        let text = socket.to_string();
        assert!(text.contains("whirl.sock"), "{text}");
        assert!(text.contains("0600"), "{text}");
        assert!(!text.contains('/'), "{text}");
        assert!(socket.stale());
    }

    #[test]
    fn a_socket_file_that_is_not_there_is_named_but_not_located() {
        // A path that is certainly absent, deep enough that a leaked directory
        // would be obvious in the text.
        let path =
            PathBuf::from("/very/deep/home/user/Library/Application Support/whirl/whirl.sock");
        let error = io::Error::new(io::ErrorKind::NotFound, "No such file or directory");
        let socket = Socket::new(&path, &error);
        assert_eq!(socket.name, "whirl.sock");
        assert_eq!(socket.mode, None);
        assert!(!socket.stale());
        let text = socket.to_string();
        assert!(!text.contains('/'), "{text}");
        assert!(!text.contains("very"), "{text}");
    }

    #[test]
    fn the_five_actionable_codes_are_handled_by_name() {
        let cases = [
            (ErrorCode::Busy, Action::RetryLater),
            (ErrorCode::NotFound, Action::ReReadStatus),
            (ErrorCode::Timeout, Action::RotationMayBeRunning),
            (ErrorCode::FavoritesDegraded, Action::HidePin),
            (ErrorCode::BadConfig, Action::ConfigCheck),
        ];
        for (code, action) in cases {
            assert_eq!(
                Refusal::new(code, "whatever").action(),
                Some(action),
                "{code}"
            );
        }
        assert_eq!(Refusal::new(ErrorCode::NoPrev, "x").action(), None);
    }

    #[test]
    fn an_unknown_protocol_names_both_sides() {
        let error = ClientError::UnknownProtocol {
            server: "3".to_string(),
            supported: &crate::SUPPORTED_PROTOCOLS,
        };
        let text = error.to_string();
        assert!(text.contains('3'), "{text}");
        assert!(text.contains('2'), "{text}");
    }
}
