//! The source edits and the token mode, run the way the window runs them, with no
//! display.
//!
//! `whirl-ui --source …` calls the same `Settings` methods the Sources section's
//! controls call, so this file asserts the round trip end to end: a source the
//! window adds lands in the file the daemon reads and whirl-core's own parser
//! accepts the result, the row the terminal prints is the row the window draws,
//! an edit the parser refuses changes nothing, and the label a Wallhaven source
//! receives is a name and never a key.
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
///
/// `WHIRL_UI_KEYCHAIN` names no store, so the app this spawns asks no keychain
/// however the config it reads is shaped. The binary the tests run is built
/// without `cfg(test)`, so its build's default store is the real one, and this is
/// where the harness says otherwise.
fn source(config: &Path, socket: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_whirl-ui"))
        .arg("--source")
        .args(args)
        .env("WHIRL_CONFIG", config)
        .env("WHIRL_SOCKET", socket)
        .env("WHIRL_UI_KEYCHAIN", "none")
        .output()
        .expect("the app runs")
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The row lines a mode printed, in the order the file holds them: everything
/// that starts with the checkbox the window draws.
fn rows(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("[x] ") || line.starts_with("[ ] "))
        .collect()
}

/// The one row a mode printed, or a panic showing what it did print.
fn one_row(stdout: &str) -> &str {
    rows(stdout)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("no source row in: {stdout}"))
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
fn a_wallhaven_source_lands_the_collection_and_the_label_and_never_a_key() {
    let (directory, config) = scratch("wallhaven");
    let socket = directory.join("whirl.sock");
    let output = source(
        &config,
        &socket,
        &[
            "add-wallhaven",
            "https://wallhaven.cc/user/alice/favorites/12345",
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains(&format!("config: {}", config.display())),
        "{stdout}"
    );
    // The row the window draws: the parsed pair, so what landed is visible, and
    // what the store says about the key. Whether the store holds one is a fact
    // about this machine, so the row is asserted up to where the two differ.
    assert!(
        one_row(&stdout).starts_with("[x] Wallhaven, a remote collection: alice/12345 ("),
        "{stdout}"
    );
    assert!(stdout.contains("Wallhaven: saved"), "{stdout}");

    let landed = std::fs::read_to_string(&config).expect("the file");
    // The daemon's own field, holding the daemon's own pair rather than the URL
    // the person pasted.
    assert!(
        landed.contains("\"collection\": \"alice/12345\""),
        "{landed}"
    );
    assert!(!landed.contains("wallhaven.cc"), "{landed}");
    assert!(
        landed.contains(&format!("\"api_key_ref\": \"{LABEL}\"")),
        "{landed}"
    );
    assert!(landed.contains("\"id\": \"wallhaven\""), "{landed}");
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
fn every_accepted_address_form_lands_the_same_collection() {
    // The three forms a person has, each on its own scratch config so the
    // written document can be quoted for each one.
    for (tag, address) in [
        (
            "wallhaven-user",
            "https://wallhaven.cc/user/alice/favorites/12345",
        ),
        (
            "wallhaven-api",
            "https://wallhaven.cc/api/v1/collections/alice/12345",
        ),
        ("wallhaven-pair", "alice/12345"),
    ] {
        let (directory, config) = scratch(tag);
        let socket = directory.join("whirl.sock");
        let output = source(&config, &socket, &["add-wallhaven", address]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{address}: {}",
            stderr_of(&output)
        );
        let landed = std::fs::read_to_string(&config).expect("the file");
        assert!(
            landed.contains("\"kind\": \"wallhaven\""),
            "{address}: {landed}"
        );
        assert!(
            landed.contains("\"collection\": \"alice/12345\""),
            "{address}: {landed}"
        );
        assert!(
            one_row(&stdout_of(&output))
                .starts_with("[x] Wallhaven, a remote collection: alice/12345 ("),
            "{address}: {}",
            stdout_of(&output)
        );
    }
}

#[test]
fn an_address_that_is_not_a_collection_writes_nothing_and_says_which_are_taken() {
    let (directory, config) = scratch("wallhaven-bad");
    let socket = directory.join("whirl.sock");
    let before = std::fs::read(&config).expect("the file");
    let output = source(
        &config,
        &socket,
        &["add-wallhaven", "https://wallhaven.cc/search?q=nebula"],
    );
    assert_eq!(output.status.code(), Some(1), "{}", stdout_of(&output));
    let stderr = stderr_of(&output);
    // The one sentence names every accepted form.
    for form in [
        "https://wallhaven.cc/user/<username>/favorites/<id>",
        "https://wallhaven.cc/api/v1/collections/<username>/<id>",
        "<username>/<id>",
    ] {
        assert!(stderr.contains(form), "{stderr}");
    }
    assert_eq!(std::fs::read(&config).expect("the file"), before);
    assert!(
        leftovers(&directory).is_empty(),
        "{:?}",
        leftovers(&directory)
    );
}

#[test]
fn an_existing_collection_address_can_be_changed_and_the_key_survives() {
    let (directory, config) = scratch("wallhaven-change");
    let socket = directory.join("whirl.sock");
    let added = source(&config, &socket, &["add-wallhaven", "alice/12345"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr_of(&added));

    let changed = source(
        &config,
        &socket,
        &[
            "set-collection",
            "wallhaven",
            "https://wallhaven.cc/user/bob/favorites/9",
        ],
    );
    assert_eq!(changed.status.code(), Some(0), "{}", stderr_of(&changed));
    assert!(
        one_row(&stdout_of(&changed)).starts_with("[x] Wallhaven, a remote collection: bob/9 ("),
        "{}",
        stdout_of(&changed)
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"collection\": \"bob/9\""), "{landed}");
    assert!(!landed.contains("alice/12345"), "{landed}");
    // Every key the edit did not own is exactly as it was.
    assert!(
        landed.contains(&format!("\"api_key_ref\": \"{LABEL}\"")),
        "{landed}"
    );
    assert!(landed.contains("\"_comment_1\""), "{landed}");
    assert!(landed.contains("\"a_key_no_build_has\""), "{landed}");
    assert!(!looks_like_a_key(&landed), "{landed}");
}

#[test]
fn adding_wallhaven_with_no_id_derives_one_the_schema_accepts() {
    // The window's own add: a person clicks, pastes an address and chooses no
    // id, so the id comes from the source kind, and `wallhaven` is a legal
    // schema id.
    let (directory, config) = scratch("derived-wallhaven");
    let socket = directory.join("whirl.sock");
    let output = source(&config, &socket, &["add-wallhaven", "alice/12345"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"id\": \"wallhaven\""), "{landed}");

    // A second one is numbered rather than refused.
    let again = source(&config, &socket, &["add-wallhaven", "bob/678"]);
    assert_eq!(again.status.code(), Some(0), "{}", stderr_of(&again));
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"id\": \"wallhaven-2\""), "{landed}");
    assert!(landed.contains("\"collection\": \"bob/678\""), "{landed}");
}

#[test]
fn a_folder_source_lands_and_can_be_disabled_moved_and_removed() {
    let (directory, config) = scratch("folder");
    let socket = directory.join("whirl.sock");

    let added = source(&config, &socket, &["add", "pictures", "/tmp/walls"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr_of(&added));
    assert_eq!(
        one_row(&stdout_of(&added)),
        "[x] A folder on this Mac: /tmp/walls"
    );

    let second = source(&config, &socket, &["add", "holiday", "/tmp/holiday"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr_of(&second));

    // Disabling clears the row's box and writes weight 0, and the parser reads
    // the source back as disabled rather than deleting it.
    let disabled = source(&config, &socket, &["disable", "pictures"]);
    assert_eq!(disabled.status.code(), Some(0), "{}", stderr_of(&disabled));
    assert_eq!(
        one_row(&stdout_of(&disabled)),
        "[ ] A folder on this Mac: /tmp/walls"
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"weight\": 0"), "{landed}");
    assert!(landed.contains("\"holiday\""), "{landed}");

    // Enabling it again restores the schema's default weight.
    let enabled = source(&config, &socket, &["enable", "pictures"]);
    assert_eq!(
        one_row(&stdout_of(&enabled)),
        "[x] A folder on this Mac: /tmp/walls"
    );

    // Moving it to the back changes the file's order with it.
    let moved = source(&config, &socket, &["move", "pictures", "down"]);
    assert_eq!(moved.status.code(), Some(0), "{}", stderr_of(&moved));
    let moved_stdout = stdout_of(&moved);
    let order = rows(&moved_stdout);
    assert_eq!(order.len(), 2, "{order:?}");
    assert!(order[0].ends_with("/tmp/holiday"), "{order:?}");
    assert!(order[1].ends_with("/tmp/walls"), "{order:?}");

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
fn the_windows_own_add_derives_the_id_from_the_folder_name() {
    // `add-folder` is the button a person clicks in the chooser; the id it
    // derives is the file's word rather than the person's, and it has to be one
    // the schema accepts: no slashes, nothing above 64 bytes.
    let (directory, config) = scratch("derived-folder");
    let socket = directory.join("whirl.sock");
    let output = source(&config, &socket, &["add-folder", "/tmp/My Pictures"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_of(&output));
    assert_eq!(
        one_row(&stdout_of(&output)),
        "[x] A folder on this Mac: /tmp/My Pictures"
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("\"id\": \"My-Pictures\""), "{landed}");
}

#[test]
fn changing_a_folder_points_the_source_at_the_chosen_one() {
    // `set-folder` is the row's `Change…` control. A local source's `paths` is a
    // list; the window offers one folder, so the list becomes that folder.
    let (directory, config) = scratch("change-folder");
    let socket = directory.join("whirl.sock");
    let added = source(&config, &socket, &["add", "pictures", "/tmp/walls"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr_of(&added));

    let changed = source(
        &config,
        &socket,
        &["set-folder", "pictures", "/tmp/holiday"],
    );
    assert_eq!(changed.status.code(), Some(0), "{}", stderr_of(&changed));
    assert_eq!(
        one_row(&stdout_of(&changed)),
        "[x] A folder on this Mac: /tmp/holiday"
    );
    let landed = std::fs::read_to_string(&config).expect("the file");
    assert!(landed.contains("/tmp/holiday"), "{landed}");
    assert!(!landed.contains("/tmp/walls"), "{landed}");

    // Pointing a Wallhaven source at a folder is not an edit the window can
    // explain, so it is refused rather than writing a `paths` key into it.
    let wallhaven = source(&config, &socket, &["add-wallhaven", "alice/12345"]);
    assert_eq!(
        wallhaven.status.code(),
        Some(0),
        "{}",
        stderr_of(&wallhaven)
    );
    let before = std::fs::read(&config).expect("the file");
    let wrong_kind = source(
        &config,
        &socket,
        &["set-folder", "wallhaven", "/tmp/holiday"],
    );
    assert_eq!(
        wrong_kind.status.code(),
        Some(1),
        "{}",
        stdout_of(&wrong_kind)
    );
    assert!(
        stderr_of(&wrong_kind).contains("is not a folder source"),
        "{}",
        stderr_of(&wrong_kind)
    );
    assert_eq!(std::fs::read(&config).expect("the file"), before);
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
