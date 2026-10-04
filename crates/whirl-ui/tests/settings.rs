//! The settings window, asserted with no display attached.
//!
//! A window cannot be opened in a test, so the window is made answerable from the
//! command line: `whirl-ui --dump-settings` prints the two choices it draws, in
//! the same words, and this file asserts them. The two states `README.md`
//! describes are both here: a real daemon (which every assertion about values
//! needs), and no daemon at all, where the window renders the reason, still
//! offers its controls, and nothing crashes.
//!
//! The window's edits are asserted in `write.rs` and `sources.rs`, which drive
//! the same methods the controls call. What this file adds is the window itself:
//! which controls are on screen, what each one says, and that no key name or
//! secret reaches it.
//!
//! The daemon is not built or started here. `whirl-ui` must never start one
//! (whirl's docs/architecture.md section 8, "must never"), and neither does this
//! suite: the live-daemon test talks to whatever `WHIRL_SOCKET` and `WHIRL_CONFIG`
//! name and skips, loudly, when nothing answers.

use std::path::{Path, PathBuf};
use std::process::Command;

use whirlui_client::Client;

/// The app, run with the socket and config file this test names.
fn run_with(socket: &Path, config: &Path, mode: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .env("WHIRL_SOCKET", socket)
        .env("WHIRL_CONFIG", config)
        .arg(mode)
        .output()
        .expect("the app runs")
}

/// The app, run with the environment it was started in.
///
/// The live test must not point the app's own client anywhere else: the daemon it
/// talks to is whatever `WHIRL_SOCKET` names for this test too, and a socket
/// override here would make the app report a daemon that this test can reach as
/// unreachable.
fn run_inherited(mode: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg(mode)
        .output()
        .expect("the app runs")
}

/// A directory that holds no socket and no config, named for the test that asked
/// for it.
///
/// The path is deliberately deep, because the reason the client prints must name
/// the socket file and never the directory, and a shallow one would not tell the
/// two apart.
fn scratch(tag: &str) -> PathBuf {
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

/// The number and unit the window reads a stored interval as.
///
/// This repeats a rule the window owns, and it repeats it on purpose: a change to
/// how a stored interval reads is a change to what a person sees, and this is the
/// check that notices.
fn reads_as(seconds: u64) -> (String, &'static str) {
    let (unit, scale) = if seconds != 0 && seconds.is_multiple_of(3600) {
        ("hours", 3600)
    } else {
        ("minutes", 60)
    };
    let value = seconds as f64 / scale as f64;
    let number = if value.fract() == 0.0 {
        format!("{}", value as u64)
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    (number, unit)
}

/// The file the daemon named, as a JSON document, and the path it came from.
fn daemon_config(config_line: &str) -> (PathBuf, serde_json::Value) {
    let path = PathBuf::from(config_line.trim_start_matches("config: ").trim());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let document = serde_json::from_str(&text).expect("the config file is JSON");
    (path, document)
}

#[test]
fn with_no_daemon_the_window_shows_the_reason_and_still_offers_its_controls() {
    let directory = scratch("absent");
    let socket = directory.join("whirl.sock");
    let config = directory.join("config.json");
    let output = run_with(&socket, &config, "--dump-settings");
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);

    // No crash: the process ran to its own exit rather than panicking.
    assert!(!stderr.contains("panicked"), "{stderr}");

    // The two choices, and nothing else: no App pane, no dump of the rest of the
    // file.
    for title in ["Wallpapers come from", "How often they change"] {
        assert!(stdout.contains(title), "{stdout}");
    }
    for gone in ["socket: live", "daemon_version", "protocol: 2", "count: "] {
        assert!(!stdout.contains(gone), "{gone} in:\n{stdout}");
    }

    // The daemon's line carries the reason once, and the controls are all still
    // there: a window that went blank when the daemon stopped would be a window a
    // person cannot fix the daemon from.
    assert_eq!(
        stdout.matches("the daemon is not reachable").count(),
        1,
        "{stdout}"
    );
    assert!(stdout.contains("whirl is not running: "), "{stdout}");
    assert!(
        stdout.contains("[Add a folder…] [Add Wallhaven]"),
        "{stdout}"
    );
    assert!(stdout.contains("Every [] [minutes] [Save]"), "{stdout}");

    // The daemon's reason names the socket file and not the directory it is
    // missing from: the client's own diagnostics are path-free, and this line is
    // the client's words rather than the window's.
    let daemon_line = stdout
        .lines()
        .find(|line| line.starts_with("whirl is not running"))
        .expect("the daemon's line");
    assert!(
        !daemon_line.contains(&directory.to_string_lossy().into_owned()),
        "{daemon_line}"
    );
    // The window's own line about the file it edits does name that file, which is
    // the point of it: a person needs to know which config the window is pointed
    // at.
    assert!(
        stdout.contains(&config.to_string_lossy().into_owned()),
        "{stdout}"
    );

    // Nothing on screen names a config key, and no value of the file's is
    // reported: the window's own words only.
    for key in [
        "interval_seconds",
        "api_key_ref",
        "weight=",
        "config_schema",
    ] {
        assert!(!stdout.contains(key), "{key} in:\n{stdout}");
    }

    // Section 8 item 7's code for an unreachable daemon, printed after the window.
    assert_eq!(output.status.code(), Some(2), "{stderr}");
}

#[test]
fn with_a_daemon_the_window_shows_what_the_daemon_and_the_file_report() {
    let Ok(client) = Client::connect() else {
        eprintln!("skipping: no daemon is reachable; point WHIRL_SOCKET and WHIRL_CONFIG at one");
        return;
    };
    let mut client = client;
    let config_line = client.config_path().expect("config path");
    let check = client.config_check().expect("config check");
    let (_config, document) = daemon_config(&config_line);

    let output = run_inherited("--dump-settings");
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));

    // The daemon's line, and the note that says where a change actually lands.
    assert!(stdout.contains("whirl is running"), "{stdout}");
    assert!(
        stdout.contains("whirl picks them up the next time it reads it"),
        "{stdout}"
    );

    // The rotation the file asks for, as the control reads it, and the rotation
    // the daemon is using now, as the one line beside it.
    let stored = document["schedule"]["interval_seconds"]
        .as_u64()
        .expect("an interval in the file");
    let (number, unit) = reads_as(stored);
    assert!(
        stdout.contains(&format!("Every [{number}] [{unit}] [Save]")),
        "{stdout}"
    );
    let adopted: u64 = check
        .effective("schedule.interval_seconds")
        .expect("an interval in the plan")
        .parse()
        .expect("a number of seconds");
    let (in_use, in_use_unit) = reads_as(adopted);
    assert!(
        stdout.contains(&format!("whirl is using every {in_use} {in_use_unit} now")),
        "{stdout}"
    );

    // One row per source in the file, described in a person's words: a folder of
    // their own, or the remote collection. The daemon's own count is the same
    // number, because it read the same file.
    let sources = document["sources"].as_array().cloned().unwrap_or_default();
    let folders = stdout.matches("] A folder on this Mac: ").count();
    let remote = stdout.matches("] Wallhaven, a remote collection: ").count();
    assert_eq!(
        folders + remote,
        sources.len(),
        "the window shows every source in the file:\n{stdout}"
    );
    assert!(sources.len() >= folders + remote, "{stdout}");

    // Nothing on screen names a config key, and no keying is reported: the
    // file's own spelling stays in the file.
    for key in [
        "interval_seconds",
        "api_key_ref",
        "weight=",
        "config_schema",
        "source: ",
    ] {
        assert!(!stdout.contains(key), "{key} in:\n{stdout}");
    }
}

#[test]
fn each_dump_mode_prints_the_daemons_own_lines() {
    // The settings mode was built beside the dump modes whose answers it reads,
    // so the three that ask one request each are checked here too: they are the
    // window's inputs, and a change to one must not quietly change the others.
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

#[test]
fn a_screenshot_state_the_window_does_not_have_is_a_command_line_error() {
    // The state is read before the window opens, so a word that is not one of the
    // states is refused without a display and without a file.
    let directory = scratch("snap-state");
    let path = directory.join("never-written.png");
    let output = Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg("--screenshot")
        .arg(&path)
        .arg("sideways")
        .output()
        .expect("the app runs");
    assert_eq!(output.status.code(), Some(3), "{}", stdout_of(&output));
    assert!(
        stderr_of(&output).contains("is not a state"),
        "{}",
        stderr_of(&output)
    );
    assert!(!path.exists(), "no window ran, so no file was written");

    // And a screenshot with no path at all is the same kind of error.
    let missing_path = Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg("--screenshot")
        .output()
        .expect("the app runs");
    assert_eq!(missing_path.status.code(), Some(3));
    assert!(
        stderr_of(&missing_path).contains("takes the path to write"),
        "{}",
        stderr_of(&missing_path)
    );
}
