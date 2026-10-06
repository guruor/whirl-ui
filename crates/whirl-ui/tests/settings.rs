//! The settings window, asserted with no display attached.
//!
//! A window cannot be opened in a test, so the window is made answerable from the
//! command line: `whirl-ui --dump-settings` prints the panes it draws, in the
//! same words, and this file asserts them. The two states `README.md`
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
//! name and skips, loudly, when nothing answers. The Background Helper pane's
//! switch is the one part that needs a daemon's answer and no daemon, so it is
//! asserted against the stand-in in `support`, giving it each of the three codes
//! `whirl daemon status` writes.

use std::path::{Path, PathBuf};
use std::process::Command;

use whirlui_client::Client;

#[cfg(unix)]
mod support;
#[cfg(unix)]
use support::{Stub, app};

/// The app binary, with the store gate already set.
///
/// The binary this spawns is built without `cfg(test)`, so its build's default
/// store is the machine's own keychain; naming no store here is how the harness
/// keeps a test out of it. Every spawn in this file that does not drive the
/// stand-in goes through this helper, so a test added later cannot reach the
/// keychain by forgetting to opt out.
fn app_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_whirl-ui"));
    command.env("WHIRL_UI_KEYCHAIN", "none");
    command
}

/// The app, run with the socket and config file this test names.
fn run_with(socket: &Path, config: &Path, mode: &str) -> std::process::Output {
    app_command()
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
/// unreachable. The store is the one thing the harness does point elsewhere: the
/// live config can name a Wallhaven source, and this run is not the place to ask
/// the machine's own keychain about it.
fn run_inherited(mode: &str) -> std::process::Output {
    app_command().arg(mode).output().expect("the app runs")
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

    // The three panes of choices and state, and nothing else: no dump of the rest
    // of the file.
    for title in ["Wallpapers come from", "How often they change"] {
        assert!(stdout.contains(title), "{stdout}");
    }
    for gone in ["socket: live", "daemon_version", "protocol: 2", "count: "] {
        assert!(!stdout.contains(gone), "{gone} in:\n{stdout}");
    }

    // The About block is on screen with no daemon too, and says so: a tester
    // pastes the whole picture from one pane, and a check that could not be made
    // is a line the window prints rather than a silence.
    assert!(stdout.contains("About\n"), "{stdout}");
    assert!(
        stdout.contains("https://github.com/guruor/whirl-ui"),
        "{stdout}"
    );
    assert!(stdout.contains("Check for a newer release"), "{stdout}");
    // The one removal row, and it says what it cannot do.
    assert!(stdout.contains("Remove Whirl completely"), "{stdout}");
    let about = stdout.split("About\n").nth(1).expect("the About block");
    // The About block names no path on this machine: a version, a URL, and the
    // daemon in a word.
    if let Ok(home) = std::env::var("HOME") {
        assert!(
            !about.contains(&home),
            "the About block names the operator's home:\n{about}"
        );
    }
    // Nothing has been checked yet, so no outcome is on screen: the check's own
    // line is there, and none of the three verdicts is.
    assert!(!about.contains("could not be made"), "{about}");
    assert!(!about.contains("a newer version is published"), "{about}");
    assert!(
        !about.contains("this is the newest version published"),
        "{about}"
    );

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
fn the_window_shows_the_five_panes_and_the_two_switches() {
    let directory = scratch("panes");
    let socket = directory.join("whirl.sock");
    let config = directory.join("config.json");
    let output = run_with(&socket, &config, "--dump-settings");
    let stdout = stdout_of(&output);

    // The five panes, in the order the sidebar lists them.
    let mut at = 0;
    for title in [
        "Wallpapers come from",
        "How often they change",
        "Background Helper",
        "Control Panel",
        "About",
    ] {
        let found = stdout[at..]
            .find(title)
            .unwrap_or_else(|| panic!("{title} is not on screen:\n{stdout}"));
        at += found + title.len();
    }

    // The two switches are the two login flags and nothing else: the Background
    // Helper's unit, and this window's own login item. Each row is drawn once,
    // with a switch token on it, and neither row is a command.
    assert_eq!(stdout.matches("Launch at login").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("Open Whirl at login").count(), 1, "{stdout}");
    for label in ["Launch at login", "Open Whirl at login"] {
        let row = stdout
            .lines()
            .find(|line| line.contains(label))
            .unwrap_or_else(|| panic!("{label} is not on screen:\n{stdout}"));
        assert!(
            row.starts_with("  [x] ") || row.starts_with("  [ ] ") || row.starts_with("  [-] "),
            "{row}"
        );
    }

    // There is no control for the menu bar icon anywhere, because the icon shows
    // exactly while this window runs: the login flags are the only switch, so a
    // second control would say the same thing twice.
    for gone in [
        "menu bar",
        "Menu bar",
        "tray icon",
        "Show icon",
        "Hide icon",
    ] {
        assert!(!stdout.contains(gone), "{gone} in:\n{stdout}");
    }
    // And nothing on screen names a login flag, a store, or the API behind the
    // app's own login item: the flags are the switch, in the app's own words.
    for key in [
        "launch_at_login",
        "open_at_login",
        "login_item",
        "SMAppService",
        "LaunchAgent",
    ] {
        assert!(!stdout.contains(key), "{key} in:\n{stdout}");
    }
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
    let version = client.version().expect("the daemon's version");
    let daemon_version = version
        .iter()
        .find_map(|line| line.strip_prefix("daemon_version: "))
        .expect("a daemon_version line")
        .trim()
        .to_string();
    let (_config, document) = daemon_config(&config_line);

    let output = run_inherited("--dump-settings");
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));

    // The window's line, and the note that says where a change actually lands.
    assert!(stdout.contains("whirl is running"), "{stdout}");
    assert!(
        stdout.contains("used when whirl next reads the file"),
        "{stdout}"
    );

    // The Background Helper pane carries Whirl's own version, so one pane answers
    // which build this window is talking to, in Whirl's words rather than in this
    // window's.
    assert!(
        stdout.contains(&format!("the Background Helper is {daemon_version}")),
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
        let output = app_command().arg(mode).output().expect("the app runs");
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
    let output = app_command()
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
    let missing_path = app_command()
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

#[test]
fn the_update_check_that_could_not_be_made_says_so() {
    // The check is one request, so a machine that cannot make it gets the reason
    // and an exit code that is not success. A proxy that does not resolve is
    // that machine, and it costs no network: `.invalid` is reserved.
    let output = app_command()
        .env("https_proxy", "http://no-such-host.invalid:9")
        .env("HTTPS_PROXY", "http://no-such-host.invalid:9")
        .arg("--check-update")
        .output()
        .expect("the app runs");
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    assert!(stdout.contains("the check could not be made:"), "{stdout}");
    // The one outcome this path must never fake: a check that failed is not the
    // newest release.
    assert!(
        !stdout.contains("this is the newest version published"),
        "{stdout}"
    );
    assert!(!stdout.contains("a newer version is published"), "{stdout}");
}

#[test]
fn the_update_check_takes_no_arguments() {
    // The check is a mode of its own, not a verb with a subject: there is nothing
    // to point it at, and nothing to configure.
    let output = app_command()
        .arg("--check-update")
        .arg("--now")
        .output()
        .expect("the app runs");
    assert_eq!(output.status.code(), Some(3));
    assert!(
        stderr_of(&output).contains("--check-update takes no arguments"),
        "{}",
        stderr_of(&output)
    );
}

/// The Background Helper pane's switch is the state `whirl daemon status`
/// reported, and nothing else.
///
/// `--dump-settings` calls the same `read_helper` the pane calls when it is
/// shown, and with the stand-in on `PATH` the switch's position is the stand-in's
/// own answer. The three codes are the daemon's status contract, and the middle
/// one is the only state that offers a step beside it. The call log is the
/// evidence that the read is `whirl daemon status` and nothing else, which is
/// what makes the switch a state rather than a command.
#[cfg(unix)]
#[test]
fn the_switch_is_the_state_whirls_own_status_reports() {
    let stub = Stub::new("window-switch");
    stub.warm();

    let dump = |env: &[(&str, &str)]| stdout_of(&app(&stub, env, &["--dump-settings"]));

    // Exit 0, running: on, and there is nothing to start.
    let running = dump(&[]);
    assert!(running.contains("[x] Launch at login"), "{running}");
    assert!(running.contains("it is running now"), "{running}");
    assert!(!running.contains("[Start now]"), "{running}");

    // Exit 1, loaded and stopped: on, with the one step out of it beside the
    // status line, because a stopped unit is not a state a switch can show.
    let stopped = dump(&[
        ("WHIRL_STUB_STATUS_CODE", "1"),
        (
            "WHIRL_STUB_STATUS_WORDS",
            "status: com.guruor.whirl loaded, not running",
        ),
    ]);
    assert!(stopped.contains("[x] Launch at login"), "{stopped}");
    assert!(stopped.contains("it is loaded and stopped"), "{stopped}");
    assert!(stopped.contains("[Start now]"), "{stopped}");

    // Exit 2, no job: off, and nothing to start.
    let absent = dump(&[
        ("WHIRL_STUB_STATUS_CODE", "2"),
        ("WHIRL_STUB_STATUS_WORDS", "no job for com.guruor.whirl"),
    ]);
    assert!(absent.contains("[ ] Launch at login"), "{absent}");
    assert!(absent.contains("it is not installed"), "{absent}");
    assert!(!absent.contains("[Start now]"), "{absent}");

    // One read per window, of the daemon's own command, and no verb that would
    // change anything: nothing here writes a unit or starts a job.
    assert_eq!(
        stub.calls(),
        vec!["daemon status", "daemon status", "daemon status"],
        "the pane asked for something other than the state"
    );
}
