//! The headless modes: what the app can answer with no display attached.
//!
//! A menu bar item and a settings window cannot be asserted by a test, so every
//! question they answer is also answerable from a terminal. The dump modes print
//! the daemon's own lines: its keys and its values, in its order, with no
//! vocabulary of the app's in between. A format invented here would be a second
//! description of the daemon's state, and the second description is the one that
//! drifts.
//!
//! `--dump-settings` is the same idea applied to the whole window: it prints the
//! three panes [`Settings`] holds, which is exactly what the window draws. What a
//! test asserts from that text is a fact about the window, and it asserts it with
//! no display, which is the only way this repository can check a window at all.
//!
//! Exit codes are whirl's own (docs/architecture.md section 8 item 7), because a
//! caller that branches on them is the caller section 8 has in mind: 0 success,
//! 1 the daemon refused, 2 the daemon is unreachable, 3 the command line was
//! wrong. `--dump-settings` still prints its panes when the daemon is not there
//! (the window shows the reason, so the dump does too) and still exits 2: the
//! code says what happened, not whether anything was printed.
//!
//! The exit codes are the whole of this module's own vocabulary, and the write
//! path is the whole of what it does not have: nothing here writes a file, starts
//! a process or edits a setting. The one thing a dump mode does with the daemon is
//! ask it a question.

use std::process::ExitCode;

use whirlui_client::protocol::{Request, Terminator};
use whirlui_client::{Client, ClientError, Refusal};

use crate::settings::{Answers, Settings};

/// The app exited successfully.
pub const EXIT_OK: u8 = 0;
/// The daemon answered `ERR`; the code on the line says why.
pub const EXIT_REFUSED: u8 = 1;
/// The daemon is not reachable.
pub const EXIT_UNREACHABLE: u8 = 2;
/// The command line, or this client and the daemon, do not agree.
pub const EXIT_USAGE: u8 = 3;

/// One headless mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `status`, the complete snapshot.
    Status,
    /// `sources`, with each source's effective `enabled` and `reason`.
    Sources,
    /// `config check`, the effective plan.
    ConfigCheck,
    /// What the settings window shows, as three panes.
    Settings,
}

impl Mode {
    /// The flag that selects this mode.
    pub fn flag(self) -> &'static str {
        match self {
            Mode::Status => "--dump-status",
            Mode::Sources => "--dump-sources",
            Mode::ConfigCheck => "--dump-config-check",
            Mode::Settings => "--dump-settings",
        }
    }

    /// Every mode, in the order the help text lists them.
    pub const ALL: [Mode; 4] = [
        Mode::Status,
        Mode::Sources,
        Mode::ConfigCheck,
        Mode::Settings,
    ];

    /// The mode a command line asks for, when it asks for one.
    pub fn parse(flag: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.flag() == flag)
    }
}

/// Print one line.
///
/// A closed pipe is the reader's decision, not a failure: `whirl-ui
/// --dump-status | head -1` stops printing early, and this exits 0 rather than
/// panicking, which would print a rustc path and take an exit code outside
/// section 8 item 7's set.
pub(crate) fn print_line(text: &str) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if let Err(error) = writeln!(stdout, "{text}")
        && error.kind() != std::io::ErrorKind::BrokenPipe
    {
        eprintln!("whirl-ui: {error}");
    }
}

/// Print a whole block, one line at a time, so a closed pipe is handled the same
/// way it is for a single line.
pub(crate) fn print_block(text: &str) {
    for line in text.lines() {
        print_line(line);
    }
}

/// Run one mode against the daemon.
pub fn run(mode: Mode) -> ExitCode {
    if mode == Mode::Settings {
        // The window is the one mode that answers without a daemon: with none it
        // shows the reason, so it is built from four possibly-failed answers
        // rather than from a connection.
        let (answers, code) = settings_answers();
        print_block(&Settings::from_answers(&answers).to_text());
        return ExitCode::from(code);
    }

    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => return report(&error),
    };
    let code = match mode {
        Mode::Status => one(&mut client, &Request::Status),
        Mode::Sources => one(&mut client, &Request::Sources),
        Mode::ConfigCheck => one(&mut client, &Request::ConfigCheck),
        // Handled above; a mode added here without a branch is a compile error.
        Mode::Settings => unreachable!(),
    };
    // `close` is how a client says it is finished rather than how it is allowed
    // to end; a failure to close is not worth reporting over the answer.
    let _ = client.close();
    code
}

/// One request, its data lines printed as the daemon wrote them.
fn one(client: &mut Client, request: &Request) -> ExitCode {
    match client.call(request) {
        Ok(response) => match response.terminator() {
            Terminator::Ok => {
                for line in response.lines() {
                    print_line(line);
                }
                ExitCode::from(EXIT_OK)
            }
            Terminator::Err { code, message } => {
                eprintln!("whirl-ui: {} {code} {message}", request.verb());
                ExitCode::from(EXIT_REFUSED)
            }
        },
        Err(error) => report(&error),
    }
}

/// The four answers the settings window is built from, and the exit code of the
/// first request that failed.
///
/// Every request is made even after one fails: one verb can be refused while the
/// others answer, and the panes that were answered keep their rows.
pub(crate) fn settings_answers() -> (Answers, u8) {
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => return (Answers::unreachable(&error.to_string()), exit_code(&error)),
    };
    let (status, status_code) = ask(&mut client, &Request::Status);
    let (sources, sources_code) = ask(&mut client, &Request::Sources);
    let (config_path, path_code) = ask(&mut client, &Request::ConfigPath);
    let (config_check, check_code) = ask(&mut client, &Request::ConfigCheck);
    let _ = client.close();

    let code = [status_code, sources_code, path_code, check_code]
        .into_iter()
        .find(|code| *code != EXIT_OK)
        .unwrap_or(EXIT_OK);
    let answers = match (status, sources, config_path, config_check) {
        (Ok(status), Ok(sources), Ok(config_path), Ok(config_check)) => {
            Answers::live(status, sources, config_path, config_check)
        }
        // The socket answered, so it is live, and the panes that were refused say
        // so in the daemon's own words.
        (status, sources, config_path, config_check) => Answers {
            connected: true,
            status,
            sources,
            config_path,
            config_check,
        },
    };
    (answers, code)
}

/// One request's answer: its data lines as the daemon wrote them, or the reason
/// there are none, with the exit code that reason carries.
fn ask(client: &mut Client, request: &Request) -> (Result<Vec<String>, String>, u8) {
    match client.call(request) {
        Ok(response) => match response.terminator() {
            Terminator::Ok => (Ok(response.lines().to_vec()), EXIT_OK),
            // A refusal is a legal answer (2.7). The reason is worded exactly as
            // `ClientError` words it, so one failure reads the same wherever it
            // surfaces.
            Terminator::Err { code, message } => (
                Err(ClientError::Refused(Refusal::new(*code, message.clone())).to_string()),
                EXIT_REFUSED,
            ),
        },
        Err(error) => (Err(error.to_string()), exit_code(&error)),
    }
}

/// The exit code section 8 item 7 gives this failure.
fn exit_code(error: &ClientError) -> u8 {
    if error.unreachable() {
        EXIT_UNREACHABLE
    } else if error.refusal().is_some() {
        EXIT_REFUSED
    } else {
        // A greeting this client cannot speak to, or a line the grammar
        // forbids: this client and that daemon do not agree, which is a command
        // line that cannot work rather than a failure of the daemon.
        EXIT_USAGE
    }
}

/// Say why, on stderr, with the exit code section 8 item 7 gives that failure.
fn report(error: &ClientError) -> ExitCode {
    eprintln!("whirl-ui: {error}");
    ExitCode::from(exit_code(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_round_trips_through_its_flag() {
        for mode in Mode::ALL {
            assert_eq!(Mode::parse(mode.flag()), Some(mode));
        }
        assert_eq!(Mode::parse("--menu-dump"), None);
        assert_eq!(Mode::parse("status"), None);
    }

    #[test]
    fn the_settings_mode_is_offered_beside_the_dumps_it_is_built_from() {
        assert_eq!(Mode::parse("--dump-settings"), Some(Mode::Settings));
        assert!(Mode::ALL.contains(&Mode::Settings));
    }
}
