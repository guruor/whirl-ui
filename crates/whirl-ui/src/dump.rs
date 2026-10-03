//! The headless modes: what the app can answer with no display attached.
//!
//! A menu bar item and a settings window cannot be asserted by a test, so every
//! question they answer is also answerable from a terminal. The modes print the
//! daemon's own lines: its keys and its values, in its order, with no vocabulary
//! of the app's in between. A format invented here would be a second description
//! of the daemon's state, and the second description is the one that drifts.
//!
//! Exit codes are whirl's own (docs/architecture.md section 8 item 7), because a
//! caller that branches on them is the caller section 8 has in mind: 0 success,
//! 1 the daemon refused, 2 the daemon is unreachable, 3 the command line was
//! wrong.

use std::process::ExitCode;

use whirlui_client::protocol::{Request, Terminator, parse_plan_record};
use whirlui_client::{Client, ClientError};

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
    /// What the settings window will show.
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
    if let Err(error) = writeln!(stdout, "{text}") {
        if error.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("whirl-ui: {error}");
        }
    }
}

/// Run one mode against the daemon.
pub fn run(mode: Mode) -> ExitCode {
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => return report(&error),
    };
    let code = match mode {
        Mode::Status => one(&mut client, &Request::Status),
        Mode::Sources => one(&mut client, &Request::Sources),
        Mode::ConfigCheck => one(&mut client, &Request::ConfigCheck),
        Mode::Settings => settings(&mut client),
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

/// What the settings window will show, as three panes.
///
/// Read-only, and only the daemon's own answers: the `sources` records, the
/// rotation keys of the `plan:` line, and the daemon's own state from `status`
/// plus `config path` and `config check`. Nothing here is computed, so nothing
/// here can disagree with what the window will render.
fn settings(client: &mut Client) -> ExitCode {
    let status = match client.call(&Request::Status) {
        Ok(response) => response,
        Err(error) => return report(&error),
    };
    let sources = match client.call(&Request::Sources) {
        Ok(response) => response,
        Err(error) => return report(&error),
    };
    let config_path = match client.call(&Request::ConfigPath) {
        Ok(response) => response,
        Err(error) => return report(&error),
    };
    let config_check = match client.call(&Request::ConfigCheck) {
        Ok(response) => response,
        Err(error) => return report(&error),
    };

    print_line("Sources");
    for line in sources.lines() {
        print_line(&format!("  {line}"));
    }
    print_line("");
    print_line("Rotation");
    for line in config_check.lines() {
        if let Some(pairs) = parse_plan_record(line) {
            for (key, value) in pairs {
                if rotation_key(&key) {
                    print_line(&format!("  {key} {value}"));
                }
            }
        }
    }
    print_line("");
    print_line("App");
    for line in status.lines() {
        print_line(&format!("  {line}"));
    }
    for line in config_path.lines() {
        print_line(&format!("  {line}"));
    }
    for line in config_check.lines() {
        if parse_plan_record(line).is_none() {
            print_line(&format!("  {line}"));
        }
    }

    for response in [&status, &sources, &config_path, &config_check] {
        if let Terminator::Err { code, message } = response.terminator() {
            eprintln!("whirl-ui: {code} {message}");
            return ExitCode::from(EXIT_REFUSED);
        }
    }
    ExitCode::from(EXIT_OK)
}

/// The keys the Rotation pane shows: the schedule's interval, the display mode
/// and the startup behaviour, by their config key paths.
fn rotation_key(key: &str) -> bool {
    matches!(
        key,
        "schedule.interval_seconds"
            | "display.mode"
            | "display.mode_effective"
            | "startup.enabled"
            | "startup.mode"
            | "startup.respect_manual"
    )
}

/// Say why, on stderr, with the exit code section 8 item 7 gives that failure.
fn report(error: &ClientError) -> ExitCode {
    eprintln!("whirl-ui: {error}");
    if error.unreachable() {
        ExitCode::from(EXIT_UNREACHABLE)
    } else if error.refusal().is_some() {
        ExitCode::from(EXIT_REFUSED)
    } else {
        // A greeting this client cannot speak to, or a line the grammar
        // forbids: this client and that daemon do not agree, which is a command
        // line that cannot work rather than a failure of the daemon.
        ExitCode::from(EXIT_USAGE)
    }
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
    fn the_rotation_pane_shows_the_keys_the_settings_window_needs() {
        for key in [
            "schedule.interval_seconds",
            "display.mode",
            "startup.enabled",
            "startup.mode",
            "startup.respect_manual",
        ] {
            assert!(rotation_key(key), "{key}");
        }
        assert!(!rotation_key("cache.max_bytes"));
        assert!(!rotation_key("backend"));
    }
}
