//! The settings window, asserted with no display attached.
//!
//! A window cannot be opened in a test, so the window is made answerable from the
//! command line: `whirl-ui --dump-settings` prints the three panes it draws, and
//! this file asserts them. The two states the card names are both here: a real
//! daemon (which every assertion about values needs), and no daemon at all, where
//! the panes render the reason, the interval editor is still offered, and nothing
//! crashes.
//!
//! The window's two editors are asserted in `write.rs` and `sources.rs`, which
//! drive the same write paths the interval field and the Sources pane's buttons
//! call. What this file adds is the window itself: which pane carries what, and
//! which pane is still read-only.
//!
//! The daemon is not built or started here. `whirl-ui` must never start one
//! (whirl's docs/architecture.md section 8, "must never"), and neither does this
//! suite: the live-daemon test talks to whatever `WHIRL_SOCKET` and `WHIRL_CONFIG`
//! name and skips, loudly, when nothing answers.

use std::path::{Path, PathBuf};
use std::process::Command;

use whirlui_client::Client;

/// The line the Sources pane carries: what its controls write, which is the same
/// string the pane puts on screen.
const SOURCES_WRITES: &str = "an edit is written to the config file, validated by the daemon's own parser, and never sent to the daemon as a verb";

/// The app, run with a socket path that has nothing behind it.
fn run_with_socket(socket: &Path, mode: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .env("WHIRL_SOCKET", socket)
        .arg(mode)
        .output()
        .expect("the app runs")
}

/// A directory that holds no socket, named for the test that asked for it.
///
/// The path is deliberately deep, because the reason the client prints must name
/// the socket file and never the directory, and a shallow one would not tell the
/// two apart.
fn empty_directory(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("whirlui-window-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn with_no_daemon_the_panes_render_the_reason_and_the_interval_is_still_offered() {
    let scratch = empty_directory("absent");
    let socket = scratch.join("whirl.sock");
    let output = run_with_socket(&socket, "--dump-settings");
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);

    // No crash: the process ran to its own exit rather than panicking.
    assert!(!stderr.contains("panicked"), "{stderr}");

    // The three panes, and the reason in each of them.
    for title in ["Sources", "Rotation", "App"] {
        assert!(stdout.contains(title), "{stdout}");
    }
    assert_eq!(
        stdout.matches("the daemon is not reachable").count(),
        3,
        "every pane renders the reason: {stdout}"
    );

    // Sources edits: its line says what its controls write. The Rotation pane's
    // interval editor is offered too, with no daemon at all, which is the state
    // the write path exists for.
    assert_eq!(stdout.matches(SOURCES_WRITES).count(), 1, "{stdout}");
    assert_eq!(stdout.matches("interval: ").count(), 1, "{stdout}");
    assert!(stdout.contains("next rotation"), "{stdout}");

    // The reason names the socket file and not the directory it is missing from:
    // the client's own diagnostics are path-free, and so is the window.
    assert!(!stdout.contains("whirlui-window-absent"), "{stdout}");
    assert!(!stdout.contains("/Users"), "{stdout}");

    // A pane with no answer shows the reason and never an empty list, which would
    // say "there are no sources" instead of "the daemon was not asked".
    assert!(!stdout.contains("count:"), "{stdout}");
    assert!(!stdout.contains("reason="), "{stdout}");

    // Section 8 item 7's code for an unreachable daemon, printed after the panes.
    assert_eq!(output.status.code(), Some(2), "{stderr}");
}

#[test]
fn with_a_daemon_the_panes_show_what_the_daemon_reports() {
    let Ok(mut client) = Client::connect() else {
        eprintln!("skipping: no daemon is reachable; point WHIRL_SOCKET and WHIRL_CONFIG at one");
        return;
    };
    let status = client.status().expect("status");
    let sources = client.sources().expect("sources");
    let config_path = client.config_path().expect("config path");
    let check = client.config_check().expect("config check");

    let output = Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg("--dump-settings")
        .output()
        .expect("the app runs");
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));

    // The App pane: the socket, the daemon's version and protocol, and the config
    // file the daemon says it read.
    assert!(stdout.contains("socket: live"), "{stdout}");
    let version = status.get("daemon_version").expect("a daemon version");
    assert!(
        stdout.contains(&format!("daemon_version: {version}")),
        "status daemon_version: {version}\n{stdout}"
    );
    assert!(stdout.contains("protocol: 2"), "{stdout}");
    assert!(
        stdout.contains(&format!("config: {config_path}")),
        "whirl config path: {config_path}\n{stdout}"
    );

    // The Sources pane: one row per source `whirl sources` lists.
    assert!(
        stdout.contains(&format!("count: {}", sources.count)),
        "{stdout}"
    );
    for record in &sources.records {
        let row = format!("source: {} {}", record.id, record.kind);
        assert!(stdout.contains(&row), "whirl sources row: {row}\n{stdout}");
    }

    // The Rotation pane: the plan the daemon adopted, key for key, and the
    // interval editor seeded from it.
    for key in [
        "schedule.interval_seconds",
        "display.mode",
        "startup.enabled",
        "startup.mode",
        "startup.respect_manual",
    ] {
        let value = check
            .effective(key)
            .unwrap_or_else(|| panic!("whirl config check has no {key}"));
        assert!(
            stdout.contains(&format!("{key}: {value}")),
            "whirl config check {key}={value}\n{stdout}"
        );
    }
    let interval = check
        .effective("schedule.interval_seconds")
        .expect("an interval in the plan");
    assert!(
        stdout.contains(&format!(
            "interval: {interval} (from the daemon's config check)"
        )),
        "whirl config check interval={interval}\n{stdout}"
    );

    // And the Sources pane's line is the one about writing, while the interval
    // editor is the one field in the window.
    assert_eq!(stdout.matches(SOURCES_WRITES).count(), 1, "{stdout}");
    assert_eq!(stdout.matches("interval: ").count(), 1, "{stdout}");
}

#[test]
fn each_dump_mode_prints_the_daemons_own_lines() {
    // The settings mode was built beside the dump modes whose answers it reads,
    // so the three that ask one request each are checked here too: they are the
    // panes' inputs, and a change to one must not quietly change the others.
    if Client::connect().is_err() {
        eprintln!("skipping: no daemon is reachable");
        return;
    }
    for (mode, expected) in [
        ("--dump-status", "protocol: 2"),
        ("--dump-sources", "count: "),
        ("--dump-config-check", "plan: "),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
            .arg(mode)
            .output()
            .expect("the app runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{mode}: {}",
            stderr_of(&output)
        );
        let stdout = stdout_of(&output);
        assert!(
            stdout.lines().any(|line| line.starts_with(expected)),
            "{mode} printed no {expected} line: {stdout}"
        );
    }
}
