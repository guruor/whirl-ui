//! The Wallhaven token in the platform's own store, and the two calls that
//! never ask for it back.
//!
//! whirl's `docs/architecture.md` 6.3 puts the key in one of three places, and
//! this module owns exactly one of them on macOS: the Keychain item the
//! documented label names, `keychain:whirl-wallhaven`. A config file holds the
//! label and never the value, because a key in a config file is a key in every
//! backup of it.
//!
//! Three rules shape the API, and each one is visible in the signatures:
//!
//! - **There is one write.** [`store`] is the only call that carries the token,
//!   and it carries it on the child's standard input, never in an argument:
//!   `security add-generic-password` accepts its password from argv, and argv is
//!   readable by every process of the same user through `ps`. The token is
//!   hex-encoded and fed to `security -i`'s command line on stdin, which is the
//!   one form that keeps it out of the process table.
//! - **A read asks whether the item is there, never for its bytes.** [`exists`]
//!   runs `security find-generic-password -s whirl-wallhaven` with its output
//!   discarded and reads only the exit status. `-w` (the password alone) and `-g`
//!   (the password with the attributes) are the two flags that return the secret
//!   (`security(1)`), and no code path here passes either one, so a call that
//!   returns the value cannot be written against this API by accident.
//! - **The item's identity is the daemon's.** The service, the account and the
//!   label are whirl's own words ([`SERVICE`], [`ACCOUNT`], [`LABEL`]), so the
//!   item this app writes is the item `whirl config check` looks for. Nothing
//!   here invents a second name for the same secret.
//!
//! The platform is macOS-only today, and the other two CI legs still compile
//! this module: the `not(macos)` half answers every call with
//! [`KeychainError::Unsupported`] rather than failing to build, so a
//! cross-platform build of the workspace stays honest about what it can do.

use std::fmt;

/// The service (and, by macOS's default, the label) of the item the documented
/// label names: `security find-generic-password -s whirl-wallhaven`
/// (whirl's `docs/architecture.md` 6.3).
pub const SERVICE: &str = "whirl-wallhaven";

/// The account the item is created under. whirl looks the item up by service
/// alone, so this only has to stay stable between the write and the update.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub const ACCOUNT: &str = "whirl";

/// The label a config file carries in `sources[i].api_key_ref`, and the one
/// label this app manages. It is a name, never a value, and it is what
/// `looks_like_a_key` in whirl-core's parser accepts where a key would be
/// refused.
pub const LABEL: &str = "keychain:whirl-wallhaven";

/// The platform store could not be asked, or refused the write. The message is
/// the app's own; it never contains the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainError {
    /// This platform has no store wired yet (the two non-macOS CI legs).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    // Constructed only by the non-macOS half.
    Unsupported(String),
    /// The store answered, and the answer was a failure.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    // Constructed only by the macOS half.
    Failed(String),
}

impl KeychainError {
    /// The reason, in the words a pane shows.
    pub fn message(&self) -> &str {
        match self {
            KeychainError::Unsupported(message) | KeychainError::Failed(message) => message,
        }
    }
}

impl fmt::Display for KeychainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for KeychainError {}

/// Store the token, once. The one call in this crate that carries the secret.
pub fn store(token: &str) -> Result<(), KeychainError> {
    imp::store(token)
}

/// Whether an item exists for [`SERVICE`]. A `bool`, never the value: this is
/// the call the panes make, and the only question the app may ask after the
/// write.
pub fn exists() -> Result<bool, KeychainError> {
    imp::exists()
}

/// The item's attributes, for evidence a human reads. It runs the same query
/// [`exists`] runs and keeps the attribute lines; a line that carries password
/// data is dropped before it leaves this function, so even a future macOS that
/// printed one there could not make this a read.
pub fn metadata() -> Result<Vec<String>, KeychainError> {
    imp::metadata()
}

/// The command line `security -i` is fed on stdin to write the item.
///
/// `-U` updates an item that is already there, so re-entering a token replaces
/// it rather than failing. `-X` takes the password as a hexadecimal string,
/// which is what makes a token containing quotes, spaces or a backslash safe to
/// put on a line `security` splits into words. The hex is the token's bytes and
/// nothing here encodes it as a key: the value must be recoverable by the
/// daemon, so it cannot be anything but the bytes themselves.
///
/// It is a function of the hex rather than of the token so that a test can hold
/// the line without ever holding a token.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub fn add_line(hex_token: &str) -> String {
    format!("add-generic-password -U -a {ACCOUNT} -s {SERVICE} -X {hex_token}\n")
}

/// The arguments of the query, which is also the call a developer runs by hand:
/// `security find-generic-password -s whirl-wallhaven`.
///
/// Neither `-w` nor `-g` is here, and neither may be added: they are the two
/// flags that make `security` print the password, so a call built from these
/// arguments cannot return it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS calls and by the tests.
pub fn find_arguments() -> [&'static str; 3] {
    ["find-generic-password", "-s", SERVICE]
}

/// The token's bytes as lower-case hex, which is the form `-X` takes.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The attribute lines of a `security find-generic-password` answer.
///
/// The plain query prints the item's attributes and no password, because `-g`
/// and `-w` are the flags that ask for one. The filter is the belt to that
/// brace: any line that begins with `password` is dropped here, so the value
/// cannot leave this module even if a future `security` began printing it in the
/// attribute dump.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS read and by the tests.
pub fn attribute_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            !line
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("password")
        })
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use super::{ACCOUNT, KeychainError, SERVICE, add_line, attribute_lines, find_arguments, hex};

    /// The platform tool, by absolute path: the same binary `security(1)`
    /// documents, so nothing here depends on the caller's `PATH`.
    const SECURITY: &str = "/usr/bin/security";

    /// `errSecItemNotFound`, the status `security` exits with when the item is
    /// not there. It is the one non-zero status that is an answer rather than a
    /// failure.
    const ITEM_NOT_FOUND: i32 = 44;

    pub fn store(token: &str) -> Result<(), KeychainError> {
        let line = add_line(&hex(token.as_bytes()));
        let mut child = Command::new(SECURITY)
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| KeychainError::Failed(format!("cannot run {SECURITY}: {error}")))?;
        // The token goes in here and nowhere else: not an argument, not an
        // environment variable, not a temporary file.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(line.as_bytes()).map_err(|error| {
                KeychainError::Failed(format!("cannot write to {SECURITY}: {error}"))
            })?;
        }
        let status = child.wait().map_err(|error| {
            KeychainError::Failed(format!("{SECURITY} did not finish: {error}"))
        })?;
        if !status.success() {
            return Err(KeychainError::Failed(format!(
                "{SECURITY} add-generic-password -U -a {ACCOUNT} -s {SERVICE} exited {}",
                status
                    .code()
                    .map_or("on a signal".to_string(), |c| c.to_string())
            )));
        }
        // `security -i` reads commands until EOF and its status describes the
        // session, so the item's presence is confirmed by the one query that
        // asks about presence: `exists`. A store that did not land is a failure
        // here rather than a pane that says ready about nothing.
        if exists()? {
            Ok(())
        } else {
            Err(KeychainError::Failed(format!(
                "the store answered the write but holds no item for service {SERVICE}"
            )))
        }
    }

    pub fn exists() -> Result<bool, KeychainError> {
        // The child's output goes to /dev/null. The app reads the exit status
        // and not one byte of the item, which is the whole of "whether it is
        // there, never what it is".
        let status = Command::new(SECURITY)
            .args(find_arguments())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| KeychainError::Failed(format!("cannot run {SECURITY}: {error}")))?;
        match status.code() {
            Some(0) => Ok(true),
            Some(ITEM_NOT_FOUND) => Ok(false),
            Some(code) => Err(KeychainError::Failed(format!(
                "{SECURITY} find-generic-password -s {SERVICE} exited {code}"
            ))),
            None => Err(KeychainError::Failed(format!(
                "{SECURITY} find-generic-password -s {SERVICE} was killed by a signal"
            ))),
        }
    }

    pub fn metadata() -> Result<Vec<String>, KeychainError> {
        let output = Command::new(SECURITY)
            .args(find_arguments())
            .stdin(Stdio::null())
            .output()
            .map_err(|error| KeychainError::Failed(format!("cannot run {SECURITY}: {error}")))?;
        match output.status.code() {
            Some(0) => Ok(attribute_lines(&String::from_utf8_lossy(&output.stdout))),
            Some(ITEM_NOT_FOUND) => Ok(Vec::new()),
            Some(code) => Err(KeychainError::Failed(format!(
                "{SECURITY} find-generic-password -s {SERVICE} exited {code}"
            ))),
            None => Err(KeychainError::Failed(format!(
                "{SECURITY} find-generic-password -s {SERVICE} was killed by a signal"
            ))),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{KeychainError, SERVICE};

    fn unsupported() -> KeychainError {
        KeychainError::Unsupported(format!(
            "the platform store is not wired on this platform yet; the macOS item is {SERVICE}"
        ))
    }

    pub fn store(_token: &str) -> Result<(), KeychainError> {
        Err(unsupported())
    }

    pub fn exists() -> Result<bool, KeychainError> {
        Err(unsupported())
    }

    pub fn metadata() -> Result<Vec<String>, KeychainError> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_write_line_carries_the_token_only_as_hex_on_stdin() {
        // The fake token the tests are allowed to hold.
        let token = "fake-token-not-a-real-key";
        let line = add_line(&hex(token.as_bytes()));
        assert!(line.starts_with("add-generic-password -U "), "{line}");
        assert!(line.contains(&format!("-a {ACCOUNT}")), "{line}");
        assert!(line.contains(&format!("-s {SERVICE}")), "{line}");
        assert!(line.ends_with('\n'), "{line:?}");
        // The token itself is not in the line, and the line is one stdin line:
        // `security -i` reads one command per line.
        assert!(!line.contains(token), "{line}");
        assert_eq!(line.lines().count(), 1, "{line:?}");
        // The hex is the token's bytes, one way only.
        assert_eq!(hex(b"AB"), "4142");
        assert_eq!(hex(b""), "");
    }

    #[test]
    fn the_query_asks_for_neither_the_password_nor_the_attributes_with_it() {
        let arguments = find_arguments();
        assert_eq!(arguments[0], "find-generic-password");
        assert_eq!(arguments[1], "-s");
        assert_eq!(arguments[2], SERVICE);
        // `-w` prints the password alone and `-g` prints it beside the
        // attributes; a call that passes neither cannot print it.
        for forbidden in ["-w", "-g", "-X", "-a"] {
            assert!(
                !arguments.contains(&forbidden),
                "{forbidden} is a value-carrying flag: {arguments:?}"
            );
        }
    }

    #[test]
    fn an_attribute_dump_loses_any_line_that_carries_a_password() {
        let output = "keychain: \"/Users/example/Library/Keychains/login.keychain-db\"\n\
                      version: 512\n\
                      class: \"genp\"\n\
                      attributes:\n\
                      \x20   0x00000007 <blob>=\"whirl-wallhaven\"\n\
                      \x20   \"svce\"<blob>=\"whirl-wallhaven\"\n\
                      password: \"a-fake-token\"\n";
        let lines = attribute_lines(output);
        assert!(lines.iter().any(|line| line.contains(SERVICE)), "{lines:?}");
        assert!(
            !lines.iter().any(|line| line.contains("fake-token")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with("password")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_label_is_a_name_the_parser_accepts_where_a_key_would_be_refused() {
        // The label the module exports is the one the config carries, and it is
        // short, colon-prefixed and not key-shaped: whirl-core's
        // `looks_like_a_key` refuses a 32-character run of key characters.
        assert_eq!(LABEL, "keychain:whirl-wallhaven");
        assert!(LABEL.contains(':'));
        assert!(LABEL.len() < 32);
    }
}
