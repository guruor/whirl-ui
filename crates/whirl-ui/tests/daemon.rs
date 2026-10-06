//! The daemon's lifecycle as the app asks for it, with a stand-in `whirl` on
//! `PATH` that records its arguments.
//!
//! These are the headless forms of the two things the app does about the daemon:
//! the launch offer (`--launch`) and the quit question (`--quit`), plus the raw
//! `--daemon <verb>` pass-through. `--daemon` runs the daemon's own command and
//! prints what it said, so it works anywhere; `--launch` and `--quit` are the
//! macOS paths and carry the app's own preference store, so they are macOS-only.
//!
//! The stand-in makes the app's half testable with no daemon and no supervisor:
//! it answers each verb with the exit code and the words the test gave it, and
//! records every call in a log. The log is the evidence for the acceptance
//! question "what did the app actually spawn": every line in it is one call, and
//! the only program that can write to it is the stand-in named `whirl`.

#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

/// The stand-in: records `daemon <verb>`, answers with words and an exit code.
///
/// The answer per verb is a default that a test overrides with the environment,
/// so one script can be a daemon that is absent, one that refuses, or one that
/// was done, without being rewritten between runs.
const STAND_IN: &str = r#"#!/bin/sh
# A stand-in for the daemon's own command. It is the single program the app is
# allowed to start, so it records every call in one place and answers each verb.
log=${WHIRL_STUB_LOG:?}
printf '%s\n' "$*" >> "$log"
# This binary records that it was the one the app ran, beside itself, so a test
# with two copies can see which copy the app chose. `${0%/*}` rather than
# `dirname`, because a test may run with almost nothing on `PATH`.
: > "${0%/*}/ran"
# The app identifies a binary that answered the wrong way by asking for its
# static version. A stand-in answers only when the test gave it one, so a test
# that does not is the build from before the flag exists.
case "$1" in
  --version)
    if [ -n "${WHIRL_STUB_VERSION:-}" ]; then
      printf '%s\n' "$WHIRL_STUB_VERSION"
      exit 0
    fi
    exit 3
    ;;
esac
case "$2" in
  status) code=${WHIRL_STUB_STATUS_CODE:-0}; words=${WHIRL_STUB_STATUS_WORDS:-status: com.guruor.whirl running};;
  start)  code=${WHIRL_STUB_START_CODE:-0};  words=${WHIRL_STUB_START_WORDS:-started: com.guruor.whirl};;
  stop)   code=${WHIRL_STUB_STOP_CODE:-0};   words=${WHIRL_STUB_STOP_WORDS:-stopped: com.guruor.whirl};;
  *)      code=3; words="whirl: daemon is not a command, or it has the wrong number of arguments";;
esac
if [ "$code" = 0 ]; then
  printf '%s\n' "$words"
else
  # The real CLI prints a refusal, an unreachable daemon and a usage error on
  # standard error, and the app's stream split is what these tests check.
  printf '%s\n' "$words" >&2
fi
exit "$code"
"#;

/// A scratch directory with the stand-in in it, a scratch home, and the file
/// the stand-in logs to.
struct Stub {
    directory: PathBuf,
    /// A home directory of its own, so the app's search for the daemon cannot
    /// see the machine the tests run on: no `~/.local/bin/whirl`, no receipt at
    /// `~/Library/Application Support/whirl-ui/install.receipt`.
    home: PathBuf,
    log: PathBuf,
}

impl Stub {
    fn new(tag: &str) -> Stub {
        let directory =
            std::env::temp_dir().join(format!("whirlui-daemon-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let command = directory.join("whirl");
        std::fs::write(&command, STAND_IN).expect("the stand-in");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&command)
                .expect("the stand-in")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&command, permissions).expect("an executable stand-in");
        }
        let home = directory.join("home");
        std::fs::create_dir_all(&home).expect("a scratch home");
        Stub {
            log: directory.join("calls.log"),
            directory,
            home,
        }
    }

    /// The absolute path of the stand-in, which is also the path the app must
    /// name when it reports which binary it ran.
    fn command(&self) -> PathBuf {
        self.directory.join("whirl")
    }

    /// Every call the app made, one `daemon <verb>` or `--version` per line.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Whether this copy was the one the app ran. The mark is beside the binary
    /// rather than in the shared call log, so two copies in one run are told
    /// apart.
    fn ran(&self) -> bool {
        self.directory.join("ran").exists()
    }

    /// A `PATH` of the stand-in and nothing else the app could need.
    fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", self.directory.display())
    }

    /// Write an install receipt under the stub's own home, as `install.sh`
    /// does: the lines are written verbatim, and only the `binary` line for
    /// `whirl` is the command the app must find.
    fn write_receipt(&self, lines: &[&str]) {
        let receipt = self
            .home
            .join("Library/Application Support/whirl-ui/install.receipt");
        std::fs::create_dir_all(receipt.parent().expect("the receipt's directory"))
            .expect("the receipt's directory");
        std::fs::write(&receipt, lines.join("\n") + "\n").expect("the receipt");
    }
}

/// Run the app with the stand-in on `PATH`.
///
/// The search's inputs are all controlled here, so no part of the machine leaks
/// in: `HOME` is the stub's own, `WHIRL_PREFIX` and `WHIRL_UI_RECEIPT` are
/// removed unless the test passes one, and `PATH` is the stub's by default.
/// A test overrides any of them by name. `WHIRL_UI_KEYCHAIN` names no store, so
/// the app this spawns asks no keychain.
fn app(stub: &Stub, env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_whirl-ui"));
    command.args(args);
    command.env("WHIRL_UI_KEYCHAIN", "none");
    command.env("PATH", stub.path());
    command.env("HOME", &stub.home);
    command.env("WHIRL_STUB_LOG", &stub.log);
    command.env_remove("WHIRL_PREFIX");
    command.env_remove("WHIRL_UI_RECEIPT");
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("the app runs")
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The suite name a prefs test uses, unique per test and per process.
///
/// `NSUserDefaults` writes this suite to `~/Library/Preferences/<name>.plist`;
/// there is no way to point it anywhere else, so the name carries this process's
/// id and the test removes the file it made (`forget_suite`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the prefs tests, which need the macOS store.
fn suite(tag: &str) -> String {
    format!("com.guruor.whirl-ui.tests.{tag}.{}", std::process::id())
}

/// Forget a suite once a test is done with it, so no scratch domain is left in
/// the real preference store.
///
/// The file is removed rather than `defaults delete`d: the suite is registered
/// with `cfprefsd` by the app that wrote it and not by the `defaults` tool, so
/// `defaults` reports the domain as not found while the file is right there. The
/// file is the whole of what this test created, and removing it is the whole of
/// the cleanup.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the prefs tests, which need the macOS store.
fn forget_suite(name: &str) {
    if let Some(home) = std::env::var_os("HOME") {
        let plist = PathBuf::from(home)
            .join("Library/Preferences")
            .join(format!("{name}.plist"));
        let _ = std::fs::remove_file(plist);
    }
}

#[test]
fn the_verb_mode_runs_the_daemons_own_command_and_quotes_it() {
    let stub = Stub::new("verb");

    let status = app(&stub, &[], &["--daemon", "status"]);
    assert_eq!(status.status.code(), Some(0), "{}", stderr_of(&status));
    assert_eq!(
        stdout_of(&status).trim(),
        "status: com.guruor.whirl running"
    );

    let start = app(&stub, &[], &["--daemon", "start"]);
    assert_eq!(start.status.code(), Some(0), "{}", stderr_of(&start));
    assert_eq!(stdout_of(&start).trim(), "started: com.guruor.whirl");

    // The two calls the app made, in order, and nothing else.
    assert_eq!(stub.calls(), vec!["daemon status", "daemon start"]);
}

#[test]
fn a_refusal_is_shown_as_the_daemon_said_it() {
    // Exit 1 is the daemon's own refusal, and the words are the daemon's: the
    // app shows them rather than substituting a route of its own.
    let stub = Stub::new("refusal");
    let words = "whirl: no unit at /Users/someone/Library/LaunchAgents/com.guruor.whirl.plist; run `whirl daemon install` first";
    let refused = app(
        &stub,
        &[
            ("WHIRL_STUB_START_CODE", "1"),
            ("WHIRL_STUB_START_WORDS", words),
        ],
        &["--daemon", "start"],
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stdout_of(&refused));
    assert_eq!(stderr_of(&refused).trim(), words);
    // The app's exit code is the daemon's meaning, not a code of its own.
    assert_eq!(stub.calls(), vec!["daemon start"]);
}

#[test]
fn a_binary_that_does_not_know_daemon_is_named_with_its_own_version() {
    // An older whirl is found, run, and answers with its usage text (exit 3).
    // The message must name the binary that answered and the version it
    // reports, and must not claim the command is missing.
    let stub = Stub::new("older");
    let usage = "whirl: daemon is not a command, or it has the wrong number of arguments";
    let older = app(
        &stub,
        &[
            ("WHIRL_STUB_STATUS_CODE", "3"),
            ("WHIRL_STUB_STATUS_WORDS", usage),
            ("WHIRL_STUB_VERSION", "whirl 0.1.0"),
        ],
        &["--daemon", "status"],
    );
    assert_eq!(older.status.code(), Some(3), "{}", stdout_of(&older));
    let message = stderr_of(&older);
    assert!(
        message.contains(&stub.command().display().to_string()),
        "the binary is named: {message}"
    );
    assert!(
        message.contains("whirl 0.1.0"),
        "the version is named: {message}"
    );
    assert!(
        message.contains("needs a daemon that has the `daemon` verb"),
        "{message}"
    );
    assert!(
        message.contains(usage),
        "the CLI's own words stay: {message}"
    );
    assert!(!message.contains("no whirl was found"), "{message}");
    assert_eq!(stub.calls(), vec!["daemon status", "--version"]);
}

#[test]
fn a_binary_that_cannot_name_a_version_is_still_named_and_not_called_missing() {
    // The build from before `--version` exists: the flag answers nothing, and
    // that silence is the evidence of an old binary rather than a missing one.
    let stub = Stub::new("no-version");
    let usage = "whirl: unrecognized subcommand 'daemon'";
    let older = app(
        &stub,
        &[
            ("WHIRL_STUB_STATUS_CODE", "3"),
            ("WHIRL_STUB_STATUS_WORDS", usage),
        ],
        &["--daemon", "status"],
    );
    assert_eq!(older.status.code(), Some(3), "{}", stdout_of(&older));
    let message = stderr_of(&older);
    assert!(
        message.contains(&stub.command().display().to_string()),
        "the binary is named: {message}"
    );
    assert!(message.contains("reports no version"), "{message}");
    assert!(
        message.contains("needs a daemon that has the `daemon` verb"),
        "{message}"
    );
    assert!(!message.contains("no whirl was found"), "{message}");
}

#[test]
fn a_daemon_that_cannot_be_reached_is_exit_two() {
    let stub = Stub::new("unreachable");
    let words = "whirl: no supervised daemon: com.guruor.whirl is not loaded in gui/501";
    let absent = app(
        &stub,
        &[
            ("WHIRL_STUB_STATUS_CODE", "2"),
            ("WHIRL_STUB_STATUS_WORDS", words),
        ],
        &["--daemon", "status"],
    );
    assert_eq!(absent.status.code(), Some(2), "{}", stdout_of(&absent));
    assert_eq!(stderr_of(&absent).trim(), words);
}

#[test]
fn the_receipt_binary_is_run_even_when_it_is_not_on_path() {
    // The bug this pins: `install.sh` puts the daemon in `~/.local/bin`, a GUI
    // inherits launchd's `PATH` without it, and the receipt is the installer's
    // own record of where the daemon went. The app must run that binary, and a
    // `PATH` that cannot see it must not matter.
    let stub = Stub::new("receipt");
    stub.write_receipt(&[&format!("binary {}", stub.command().display())]);
    let empty = stub.home.join("empty-path");
    std::fs::create_dir_all(&empty).expect("an empty PATH directory");
    let output = app(
        &stub,
        &[("PATH", empty.to_str().expect("a utf-8 path"))],
        &["--daemon", "status"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert_eq!(
        stdout_of(&output).trim(),
        "status: com.guruor.whirl running"
    );
    assert_eq!(stub.calls(), vec!["daemon status"]);
}

#[test]
fn the_receipts_whirl_wins_over_a_different_one_earlier_on_path() {
    // Two copies exist: one on `PATH`, and the one the installer recorded. The
    // receipt names the daemon the installer placed, and that is the one the
    // app runs, whatever `PATH` would have answered with.
    let earlier = Stub::new("earlier");
    let installed = Stub::new("installed");
    earlier.write_receipt(&[&format!("binary {}", installed.command().display())]);
    let output = app(&earlier, &[], &["--daemon", "status"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert!(
        installed.ran(),
        "the receipt's binary ran, and the one on PATH did not"
    );
    assert!(!earlier.ran(), "the copy earlier on PATH was not used");
}

#[test]
fn a_command_that_is_nowhere_the_app_looks_names_every_place_it_looked() {
    // A receipt whose binary was removed, no prefix, no /usr/local/bin and an
    // empty PATH: nothing to run, and the message must name every place that
    // was looked at and say how to get a daemon. It may not say "not on PATH",
    // which is the wrong diagnosis for a GUI app.
    let stub = Stub::new("missing");
    stub.write_receipt(&["binary /nonexistent/whirl"]);
    let empty = stub.home.join("empty-path");
    std::fs::create_dir_all(&empty).expect("an empty PATH directory");
    let output = app(
        &stub,
        &[("PATH", empty.to_str().expect("a utf-8 path"))],
        &["--daemon", "start"],
    );
    assert_eq!(output.status.code(), Some(3), "{}", stdout_of(&output));
    let message = stderr_of(&output);
    assert!(message.contains("no whirl was found"), "{message}");
    assert!(message.contains("the install receipt"), "{message}");
    assert!(
        message.contains("~/.local/bin/whirl"),
        "where install.sh puts it: {message}"
    );
    assert!(message.contains("/usr/local/bin/whirl"), "{message}");
    assert!(message.contains("whirl on PATH"), "{message}");
    assert!(
        message.contains("install.sh"),
        "how to install one: {message}"
    );
    // Nothing was started, because there was nothing to start.
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());
}

#[test]
fn the_verb_mode_refuses_a_call_it_cannot_make_a_command_line_from() {
    let stub = Stub::new("usage");
    for args in [
        vec!["--daemon"],
        vec!["--daemon", "install"],
        vec!["--daemon", "restart"],
        vec!["--daemon", "start", "extra"],
    ] {
        let output = app(&stub, &[], &args);
        assert_eq!(output.status.code(), Some(3), "{args:?}");
        assert!(
            stderr_of(&output).contains("--daemon takes a verb"),
            "{args:?}: {}",
            stderr_of(&output)
        );
    }
    // A command line the app refused is a command it never ran.
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());
}

#[cfg(target_os = "macos")]
#[test]
fn the_first_run_starts_an_absent_daemon_and_every_later_run_does_not() {
    // The launch offer. On the first run an absent daemon is started through the
    // daemon's own command; a run after that offers it and starts nothing. The
    // two runs share one preference suite, so the second sees the first's record.
    let stub = Stub::new("launch");
    let name = suite("launch");
    forget_suite(&name);
    let env = [
        ("WHIRL_UI_PREFS_SUITE", name.as_str()),
        ("WHIRL_STUB_STATUS_CODE", "2"),
        (
            "WHIRL_STUB_STATUS_WORDS",
            "whirl: no supervised daemon: com.guruor.whirl is not loaded in gui/501",
        ),
        ("WHIRL_STUB_START_WORDS", "started: com.guruor.whirl"),
    ];

    // First run: status says the daemon is absent, so it is started.
    let first = app(&stub, &env, &["--launch"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr_of(&first));
    let stdout = stdout_of(&first);
    assert!(stdout.contains("started: com.guruor.whirl"), "{stdout}");
    assert!(stdout.contains("the daemon was started"), "{stdout}");
    assert_eq!(stub.calls(), vec!["daemon status", "daemon start"]);

    // Second run: the first run is on record, so the daemon is offered and
    // nothing is started.
    std::fs::remove_file(&stub.log).expect("a fresh log");
    let second = app(&stub, &env, &["--launch"]);
    assert_eq!(second.status.code(), Some(2), "{}", stderr_of(&second));
    let stdout = stdout_of(&second);
    assert!(
        stdout.contains("the daemon is not running: start it from the menu"),
        "{stdout}"
    );
    assert_eq!(stub.calls(), vec!["daemon status"], "nothing was started");

    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_daemon_that_is_already_running_is_not_touched() {
    // The supervisor has it: the app asks and changes nothing. No start call.
    let stub = Stub::new("present");
    let name = suite("present");
    forget_suite(&name);
    let output = app(
        &stub,
        &[
            ("WHIRL_UI_PREFS_SUITE", name.as_str()),
            ("WHIRL_STUB_STATUS_CODE", "0"),
            (
                "WHIRL_STUB_STATUS_WORDS",
                "status: com.guruor.whirl running",
            ),
        ],
        &["--launch"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert!(
        stdout_of(&output).contains("nothing was started"),
        "{}",
        stdout_of(&output)
    );
    assert_eq!(stub.calls(), vec!["daemon status"]);
    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_quit_with_the_box_unchecked_leaves_the_daemon_alone() {
    // The default: the app closes and the daemon keeps running. The verb is
    // never called, so there is no stop in the log at all.
    let stub = Stub::new("quit-keep");
    let name = suite("quit-keep");
    forget_suite(&name);
    let output = app(
        &stub,
        &[("WHIRL_UI_PREFS_SUITE", name.as_str())],
        &["--quit", "--keep"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert!(
        stdout_of(&output).contains("the daemon keeps running"),
        "{}",
        stdout_of(&output)
    );
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());
    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_quit_with_the_box_checked_stops_it_through_the_verb_and_quotes_the_answer() {
    let stub = Stub::new("quit-stop");
    let name = suite("quit-stop");
    forget_suite(&name);
    let words = "stopped: com.guruor.whirl";
    let output = app(
        &stub,
        &[
            ("WHIRL_UI_PREFS_SUITE", name.as_str()),
            ("WHIRL_STUB_STOP_CODE", "0"),
            ("WHIRL_STUB_STOP_WORDS", words),
        ],
        &["--quit", "--stop"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output).trim(), words, "the daemon's own answer");
    assert_eq!(stub.calls(), vec!["daemon stop"]);
    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn dont_ask_again_remembers_the_answer_and_the_next_quit_obeys_it() {
    // The box that remembers. A quit that ticks it records the answer; the next
    // quit asks nothing, calls no verb for the answer it did not change, and
    // behaves exactly as remembered.
    let stub = Stub::new("remember");
    let name = suite("remember");
    forget_suite(&name);
    let env = [("WHIRL_UI_PREFS_SUITE", name.as_str())];

    // First quit: the box is unchecked and "don't ask again" is ticked, so the
    // answer is "keep running" for good.
    let first = app(&stub, &env, &["--quit", "--keep", "--dont-ask"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr_of(&first));
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());

    // The preference now says so.
    let remembered = app(&stub, &env, &["--quit-answer"]);
    assert_eq!(stdout_of(&remembered).trim(), "quit: keep");

    // The next quit passes no flags at all: it is answered from the preference,
    // and nothing stops the daemon.
    let next = app(&stub, &env, &["--quit"]);
    assert_eq!(next.status.code(), Some(0), "{}", stderr_of(&next));
    assert!(
        stdout_of(&next).contains("the daemon keeps running"),
        "{}",
        stdout_of(&next)
    );
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());

    // A remembered answer is the answer: the question is not asked, so a flag
    // for the other box is not an answer to anything and changes nothing.
    let flagged = app(&stub, &env, &["--quit", "--stop", "--dont-ask"]);
    assert_eq!(flagged.status.code(), Some(0), "{}", stderr_of(&flagged));
    assert!(
        stdout_of(&flagged).contains("the daemon keeps running"),
        "{}",
        stdout_of(&flagged)
    );
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());
    assert_eq!(
        stdout_of(&app(&stub, &env, &["--quit-answer"])).trim(),
        "quit: keep"
    );

    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_remembered_stop_stops_without_asking() {
    // The other remembered answer, in a suite of its own so nothing it does is
    // left behind for another test.
    let stub = Stub::new("remember-stop");
    let name = suite("remember-stop");
    forget_suite(&name);
    let env = [("WHIRL_UI_PREFS_SUITE", name.as_str())];

    let first = app(&stub, &env, &["--quit", "--stop", "--dont-ask"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr_of(&first));
    assert_eq!(
        stub.calls(),
        vec!["daemon stop"],
        "this quit stopped it once"
    );

    std::fs::remove_file(&stub.log).expect("a fresh log");
    let after = app(&stub, &env, &["--quit"]);
    assert_eq!(after.status.code(), Some(0), "{}", stderr_of(&after));
    assert_eq!(stub.calls(), vec!["daemon stop"], "and so does the next");
    assert_eq!(
        stdout_of(&app(&stub, &env, &["--quit-answer"])).trim(),
        "quit: stop"
    );

    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_quit_with_no_answer_anywhere_is_a_usage_error_rather_than_a_guess() {
    // A terminal has no dialog, so with nothing remembered and no flag there is
    // no answer to act on: the app says so instead of picking one. It also never
    // calls the verb.
    let stub = Stub::new("quit-usage");
    let name = suite("quit-usage");
    forget_suite(&name);
    let output = app(
        &stub,
        &[("WHIRL_UI_PREFS_SUITE", name.as_str())],
        &["--quit"],
    );
    assert_eq!(output.status.code(), Some(3), "{}", stdout_of(&output));
    assert!(
        stderr_of(&output).contains("--stop or --keep"),
        "{}",
        stderr_of(&output)
    );
    assert!(stub.calls().is_empty(), "{:?}", stub.calls());
    forget_suite(&name);
}

#[cfg(target_os = "macos")]
#[test]
fn a_stop_the_daemon_refuses_is_quoted_on_the_quit_path() {
    let stub = Stub::new("quit-refusal");
    let name = suite("quit-refusal");
    forget_suite(&name);
    let words = "whirl: no supervised daemon: com.guruor.whirl is not loaded in gui/501";
    let output = app(
        &stub,
        &[
            ("WHIRL_UI_PREFS_SUITE", name.as_str()),
            ("WHIRL_STUB_STOP_CODE", "2"),
            ("WHIRL_STUB_STOP_WORDS", words),
        ],
        &["--quit", "--stop"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    assert_eq!(stderr_of(&output).trim(), words);
    forget_suite(&name);
}
