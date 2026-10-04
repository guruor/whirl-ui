//! The source edits and the token mode, run the way the window runs them, with no
//! display.
//!
//! `whirl-ui --source …` calls the same `Settings` methods the Sources pane's
//! buttons call, so this file asserts the round trip end to end: a source the
//! window adds lands in the file the daemon reads and whirl-core's own parser
//! accepts the result, an edit the parser refuses changes nothing, and the label a
//! Wallhaven source receives is a name and never a key.
//!
//! The store is not written here. `--source store-token` is exercised only in the
//! ways it refuses before the store is reached, because a test that wrote a fake
//! token would replace a real one in the login Keychain; the write line itself is
//! asserted by the store module's own tests, which hold no item at all.
//!
//! No daemon is started and none is needed: `WHIRL_SOCKET` names a path with
//! nothing behind it on purpose, so a developer's running daemon can never name
//! the config file these tests write.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The label whirl's architecture documents, and the one the window writes into
/// `api_key_ref`.
const LABEL: &str = "keychain:whirl-wallhaven";

/// A config the parser accepts, with a comment key and an unknown key to lose if
/// the writer ever rebuilds the document instead of editing it.
const CONFIG: &str = r#"{
  "_comment_1": "the parser ignores this and the writer must not",
  "config_schema": 1,
  "schedule": {"interval_seconds": 1800},
  "a_key_no_build_has": ["one", "two"],
  "sources": []
}"#;

/// A scratch directory with a config in it and nothing else.
fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("whirlui-source-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let config = directory.join("config.json");
    std::fs::write(&config, CONFIG).expect("a config file");
    (directory, config)
}

/// Run the source mode with a socket path that has nothing behind it.
fn source(config: &Path, socket: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg("--source")
        .args(args)
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

/// The `source:` records a mode printed, in the order it printed them.
fn records(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("source: "))
        .collect()
}

/// The first record a mode printed, or a panic showing what it did print.
fn first_record(stdout: &str) -> &str {
    records(stdout)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("no source record in: {stdout}"))
}

/// Whether any run of 32 or more key characters appears in the text.
///
/// This is the shape whirl-core's parser refuses in `api_key_ref`, applied to the
/// whole file: a config holding a Wallhaven source must never match it.
fn looks_like_a_key(text: &str) -> bool {
    let mut run = 0usize;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            run += 1;
            if run >= 32 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// The names of the files a failed write left in the directory.
fn leftovers(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("whirl-ui"))
        .collect()
}

#[test]
fn a_wallhaven_source_lands_with_the_label_and_the_parser_reads_it_back() {
    let (directory, config) = scratch("wallhaven");
    let socket = directory.join("whirl.sock");
    let output = source(&config, &socket, &["add-wallhaven", "space"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains(&format!("config: {}", config.display())),
        "{stdout}"
    );
    // The pane's own record for the source it just added, read back out of the
    // file by the daemon's parser.
    assert!(
        records(&stdout)
            .first()
            .is_some_and(|record| record.starts_with("source: space wallhaven weight=1 enabled=1")),
        "{stdout}"
    );

    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(
        landed.contains(&format!("\"api_key_ref\": \"{LABEL}\"")),
        "{landed}"
    );
    // Everything that was not the sources array is still there.
    for keep in [
        "\"_comment_1\"",
        "\"a_key_no_build_has\"",
        "\"config_schema\": 1",
    ] {
        assert!(landed.contains(keep), "{keep} is gone:\n{landed}");
    }
    // The file carries the label and never a key.
    assert!(!looks_like_a_key(&landed), "{landed}");
    // The label is short and not key-shaped, which is what makes the parser
    // accept it where a key would be refused.
    assert!(LABEL.len() < 32, "{LABEL}");
}

#[test]
fn a_folder_source_lands_and_can_be_disabled_moved_and_removed() {
    let (directory, config) = scratch("folder");
    let socket = directory.join("whirl.sock");

    let added = source(&config, &socket, &["add", "pictures", "/tmp/walls"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr_of(&added));
    assert!(
        first_record(&stdout_of(&added)).starts_with("source: pictures local weight=1 enabled=1"),
        "{}",
        stdout_of(&added)
    );

    let second = source(&config, &socket, &["add", "holiday", "/tmp/holiday"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr_of(&second));

    // Disabling writes weight 0, and the parser reads the source back as disabled
    // rather than deleting it.
    let disabled = source(&config, &socket, &["disable", "pictures"]);
    assert_eq!(disabled.status.code(), Some(0), "{}", stderr_of(&disabled));
    assert!(
        first_record(&stdout_of(&disabled))
            .starts_with("source: pictures local weight=0 enabled=0"),
        "{}",
        stdout_of(&disabled)
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"weight\": 0"), "{landed}");
    assert!(landed.contains("\"holiday\""), "{landed}");

    // Enabling it again restores the schema's default weight.
    let enabled = source(&config, &socket, &["enable", "pictures"]);
    assert!(
        first_record(&stdout_of(&enabled)).starts_with("source: pictures local weight=1 enabled=1"),
        "{}",
        stdout_of(&enabled)
    );

    // Moving it to the back changes the file's order with it.
    let moved = source(&config, &socket, &["move", "pictures", "down"]);
    assert_eq!(moved.status.code(), Some(0), "{}", stderr_of(&moved));
    let moved_stdout = stdout_of(&moved);
    let order = records(&moved_stdout);
    assert_eq!(order.len(), 2, "{order:?}");
    assert!(order[0].starts_with("source: holiday"), "{order:?}");
    assert!(order[1].starts_with("source: pictures"), "{order:?}");

    // Removing it leaves the other source alone.
    let removed = source(&config, &socket, &["remove", "pictures"]);
    assert_eq!(removed.status.code(), Some(0), "{}", stderr_of(&removed));
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(!landed.contains("pictures"), "{landed}");
    assert!(landed.contains("\"holiday\""), "{landed}");

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
fn an_edit_the_parser_refuses_changes_nothing_and_says_why() {
    let (directory, config) = scratch("refused");
    let socket = directory.join("whirl.sock");
    let added = source(&config, &socket, &["add", "pictures", "/tmp/walls"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr_of(&added));
    let before = std::fs::read(&config).expect("the file");

    // A second source with the same id is refused by whirl-core's own parser, and
    // the message is the parser's.
    let duplicate = source(&config, &socket, &["add", "pictures", "/tmp/other"]);
    assert_eq!(
        duplicate.status.code(),
        Some(1),
        "{}",
        stdout_of(&duplicate)
    );
    let stderr = stderr_of(&duplicate);
    assert!(stderr.contains("duplicate source id"), "{stderr}");
    assert_eq!(std::fs::read(&config).expect("the file"), before);
    assert!(
        leftovers(&directory).is_empty(),
        "{:?}",
        leftovers(&directory)
    );

    // A source the file does not hold is refused by name, and nothing is written.
    let missing = source(&config, &socket, &["remove", "space"]);
    assert_eq!(missing.status.code(), Some(1), "{}", stdout_of(&missing));
    assert!(
        stderr_of(&missing).contains("no source named"),
        "{}",
        stderr_of(&missing)
    );
    assert_eq!(std::fs::read(&config).expect("the file"), before);

    // An empty folder is refused before the file is touched: the parser refuses an
    // empty `paths` entry.
    let empty = source(&config, &socket, &["add", "empty", ""]);
    assert_eq!(empty.status.code(), Some(1), "{}", stdout_of(&empty));
    assert_eq!(std::fs::read(&config).expect("the file"), before);
}

#[test]
fn a_token_is_read_from_stdin_and_never_from_an_argument() {
    // An argument is in the process table and in the shell's history, so the mode
    // takes none: a token given as one is a usage error and the store is never
    // reached. Nothing here writes the Keychain: standard input is closed, so the
    // mode cannot read a token even if it wanted one.
    let (directory, config) = scratch("token");
    let socket = directory.join("whirl.sock");

    let argument = source(
        &config,
        &socket,
        &["store-token", "fake-token-not-a-real-key"],
    );
    assert_eq!(argument.status.code(), Some(3), "{}", stdout_of(&argument));
    let stderr = stderr_of(&argument);
    assert!(stderr.contains("--source takes a verb"), "{stderr}");
    // Neither the token as typed nor any echo of it appears in what the mode
    // prints.
    assert!(!stderr.contains("fake-token"), "{stderr}");

    let empty = source(&config, &socket, &["store-token"]);
    assert_eq!(empty.status.code(), Some(3), "{}", stdout_of(&empty));
    assert!(
        stderr_of(&empty).contains("reads the token from standard input"),
        "{}",
        stderr_of(&empty)
    );

    // An unknown verb is a usage error rather than a guess at what was meant.
    let unknown = source(&config, &socket, &["frobnicate"]);
    assert_eq!(unknown.status.code(), Some(3), "{}", stdout_of(&unknown));
    assert!(
        stderr_of(&unknown).contains("--source takes a verb"),
        "{}",
        stderr_of(&unknown)
    );
}
