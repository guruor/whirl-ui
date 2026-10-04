//! The write path, run the way the window runs it, with no display.
//!
//! `whirl-ui --set-rotation <value> <minutes|hours>` calls the same
//! `save_interval` the settings window's `Every [n] [unit]` control's `Save`
//! button calls, from the same two fields. This file asserts the round trip M2
//! criterion 1 names: the value lands in the file the daemon reads, a value the
//! parser refuses does not, and the file's mode and its other keys survive
//! either way.
//!
//! No daemon is started here and none is needed. `WHIRL_SOCKET` is pointed at a
//! path with nothing behind it on purpose: the one thing these tests must not do
//! is let a developer's running daemon name a config file, so the write goes to
//! the `WHIRL_CONFIG` this test wrote and to nothing else. That is also the proof
//! that the write path sends no daemon verb: it completes with no daemon to send
//! one to.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A config the parser accepts, with a comment key and an unknown key to lose if
/// the writer ever rebuilds the document instead of editing it.
const CONFIG: &str = r#"{
  "_comment_1": "the parser ignores this and the writer must not",
  "config_schema": 1,
  "schedule": {"interval_seconds": 1800, "_comment_interval_seconds": "the interval"},
  "a_key_no_build_has": ["one", "two"],
  "sources": []
}"#;

/// A scratch directory with a config in it and nothing else.
fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("whirlui-set-rotation-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let config = directory.join("config.json");
    std::fs::write(&config, CONFIG).expect("a config file");
    (directory, config)
}

/// Run the app's write mode with a socket path that has nothing behind it.
fn set_rotation(config: &Path, socket: &Path, value: &str, unit: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .args(["--set-rotation", value, unit])
        .env("WHIRL_CONFIG", config)
        .env("WHIRL_SOCKET", socket)
        .output()
        .expect("the app runs")
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_rotation_lands_in_the_file_the_daemon_reads() {
    let (directory, config) = scratch("lands");
    let socket = directory.join("whirl.sock");
    let output = set_rotation(&config, &socket, "15", "minutes");

    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    // The app names the file it wrote, and the line the window's Rotation
    // section carries after the same save: the control's own words, and when the
    // change applies.
    assert!(
        stdout.contains(&format!("config: {}", config.display())),
        "{stdout}"
    );
    assert!(stdout.contains("Every 15 minutes: saved"), "{stdout}");
    assert!(
        stdout.contains("does not change until whirl next reads the file"),
        "{stdout}"
    );

    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"interval_seconds\": 900"), "{landed}");
    // Everything that was not the interval is still there.
    assert!(landed.contains("\"_comment_1\""), "{landed}");
    assert!(landed.contains("\"a_key_no_build_has\""), "{landed}");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&config)
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
}

#[test]
fn hours_are_written_as_the_seconds_the_schema_stores() {
    let (directory, config) = scratch("hours");
    let socket = directory.join("whirl.sock");
    let output = set_rotation(&config, &socket, "6", "hours");

    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert!(
        stdout_of(&output).contains("Every 6 hours: saved"),
        "{}",
        stdout_of(&output)
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"interval_seconds\": 21600"), "{landed}");
}

#[test]
fn a_refused_value_changes_nothing_and_says_why() {
    let (directory, config) = scratch("refused");
    let socket = directory.join("whirl.sock");
    let before = std::fs::read(&config).expect("the file");
    let output = set_rotation(&config, &socket, "0.5", "minutes");

    assert_eq!(output.status.code(), Some(1), "{}", stdout_of(&output));
    let stderr = stderr_of(&output);
    // The message is whirl-core's own, name and number, so it reads the same as
    // `config check` would about the same file.
    assert!(stderr.contains("schedule.interval_seconds"), "{stderr}");
    assert!(stderr.contains("less than 60"), "{stderr}");
    assert_eq!(std::fs::read(&config).expect("the file"), before);

    // No temporary file is left behind by a refusal.
    let leftovers: Vec<String> = std::fs::read_dir(&directory)
        .expect("the directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("whirl-ui"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_write_mode_needs_no_daemon_to_write() {
    // `WHIRL_SOCKET` names a path with nothing behind it, so no verb can be
    // sent: the write completes anyway. That is the write path's whole shape,
    // and it is why the write added no state-changing verb to the app.
    let (directory, config) = scratch("no-daemon");
    let socket = directory.join("absent.sock");
    assert!(!socket.exists());
    let output = set_rotation(&config, &socket, "20", "minutes");
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert!(
        std::fs::read_to_string(&config)
            .expect("the file")
            .contains("\"interval_seconds\": 1200")
    );
}

#[test]
fn a_value_that_is_not_a_number_is_a_command_line_error() {
    let (directory, config) = scratch("not-a-number");
    let socket = directory.join("whirl.sock");
    let before = std::fs::read(&config).expect("the file");
    let output = set_rotation(&config, &socket, "half an hour", "minutes");
    assert_eq!(output.status.code(), Some(3), "{}", stdout_of(&output));
    assert!(stderr_of(&output).contains("is not a number"));
    assert_eq!(std::fs::read(&config).expect("the file"), before);
}

#[test]
fn a_unit_the_control_does_not_offer_is_a_command_line_error() {
    let (directory, config) = scratch("bad-unit");
    let socket = directory.join("whirl.sock");
    let before = std::fs::read(&config).expect("the file");
    let output = set_rotation(&config, &socket, "30", "seconds");
    assert_eq!(output.status.code(), Some(3), "{}", stdout_of(&output));
    assert!(
        stderr_of(&output).contains("minutes or hours"),
        "{}",
        stderr_of(&output)
    );
    assert_eq!(std::fs::read(&config).expect("the file"), before);
}
