//! The write path: one setting, validated by whirl-core's own parser, written
//! atomically.
//!
//! The contract is whirl's
//! `docs/decisions/0002-frontends-write-config-own-no-daemon.md`: the frontend
//! writes the config file the daemon reads, and owns no part of the daemon. Four
//! rules of that contract are visible in this module and nowhere else in the
//! crate:
//!
//! - **The parser that judges a write is the daemon's own.** `Config::parse` is
//!   the function `whirld` runs on the same bytes, so a file this module writes
//!   is a file the daemon would have accepted. A value the parser refuses never
//!   lands, and the message the window shows is the parser's, verbatim.
//! - **Everything else in the file is preserved.** The document is re-encoded
//!   from its own values rather than rebuilt from `Config`, so a key this build
//!   does not know, every `_comment_*` key and the two legacy aliases survive a
//!   write. The daemon never rewrites this file, so nothing else would restore
//!   what a writer drops.
//! - **The write is atomic.** The new document goes to a temporary file in the
//!   same directory, is fsynced, and is renamed over the config. A rotation
//!   reading at that moment sees the old file or the new one and never half of
//!   either.
//! - **The file is read back after the write**, and the value reported is the one
//!   the parser found there, not the one the caller asked for.
//!
//! The one thing this module does *not* do is read the file as the effective
//! plan. The value it returns is what the file says; what the daemon makes of it
//! is `status`, `sources` and `config check`'s answer, and a rotation that has
//! not happened yet has made nothing of it at all.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use whirlui_client::whirl_core::config::{Config, ConfigWarning};

/// The config key this milestone's write path owns, by the dotted path
/// `config check` reports it under (whirl's `docs/architecture.md` 2.6).
pub const INTERVAL_KEY: &str = "schedule.interval_seconds";

/// The config file to write, and how its path was found.
///
/// The order is the ADR's: the daemon's `config path` answer when the daemon is
/// there, and the platform default otherwise ([`whirlui_client::config_path`],
/// which resolves `WHIRL_CONFIG` before the default the daemon's own paths
/// module names). `named_by_daemon` is the difference between "the daemon told
/// us where its config is" and "we assumed the default", and the window says
/// which one it has rather than presenting a guess as a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub named_by_daemon: bool,
}

impl Target {
    /// The platform default config file, resolved without asking a daemon:
    /// `WHIRL_CONFIG` if it is set, then the platform's own path, which is the
    /// daemon's own precedence (`whirlui_client::config_path`).
    ///
    /// This is what the window falls back to, and it is where a first-time user's
    /// write goes: no daemon is running when a settings window first opens, and
    /// every verb fails then, so the file write is the path that exists.
    pub fn default_path() -> Option<Target> {
        whirlui_client::config_path().map(|path| Target {
            path,
            named_by_daemon: false,
        })
    }

    /// The same target read off an already-asked `config path` answer.
    ///
    /// This is how the settings window gets its target without opening a second
    /// connection: the four reads it is built from already include `config path`.
    pub fn from_config_path_line(line: &str) -> Option<Target> {
        let path = line.strip_prefix("config: ")?;
        if path.is_empty() || path == "-" {
            return None;
        }
        Some(Target {
            path: PathBuf::from(path),
            named_by_daemon: true,
        })
    }
}

/// A write that landed, and what the file says afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// The interval the parser read out of the file after the rename. This is the
    /// file's value, which is the only value a writer can honestly report.
    pub interval: u64,
    /// The parser's warnings about the new file, in its own words. A warning is
    /// not a refusal: whirl deliberately starts on one (architecture.md 4.3).
    pub warnings: Vec<String>,
}

/// A write that did not land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The document, or the value in it, is one whirl's parser refuses. The
    /// string is the parser's own message, so the window and `config check` say
    /// the same thing about the same file.
    Refused(String),
    /// The file could not be read or written, or a compose step failed. The file
    /// is as it was.
    Io(String),
}

/// The reason a window gives when there is no config file to write at all: the
/// daemon named none and the platform has no default either.
///
/// It lives here because locating the file is this module's business, and it is
/// what a caller with an empty [`Target`] shows.
pub const NO_PATH: &str =
    "no config file could be located: neither the daemon nor the platform names one";

impl WriteError {
    /// The reason, in the words a pane shows.
    pub fn message(&self) -> &str {
        match self {
            WriteError::Refused(message) | WriteError::Io(message) => message,
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for WriteError {}

/// Set the rotation interval in the config file at `path`.
///
/// The order is the contract's: read the file, change one key, hand the result to
/// the daemon's parser, and only then replace the file. A refused parse returns
/// before anything has been written, so the old file is intact by construction
/// rather than by cleanup.
pub fn set_interval(path: &Path, seconds: u64) -> Result<Written, WriteError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| WriteError::Io(format!("{}: {error}", path.display())))?;

    let composed = compose(&text, seconds)?;
    // The daemon's own parser, on the exact bytes that are about to land.
    // `Config::parse` is what `whirld` calls on this file, so a document that
    // passes here is one the daemon accepts.
    Config::parse(&composed).map_err(|error| WriteError::Refused(error.to_string()))?;

    replace_atomically(path, &composed)?;

    // Read back the truth: the file, through the same parser. What the window
    // shows after a write is this, never the value the caller asked for.
    let landed = std::fs::read_to_string(path)
        .map_err(|error| WriteError::Io(format!("{}: {error}", path.display())))?;
    let loaded = Config::parse(&landed).map_err(|error| WriteError::Refused(error.to_string()))?;
    Ok(Written {
        path: path.to_path_buf(),
        interval: loaded.config.schedule.interval_seconds,
        warnings: warnings(&loaded.warnings),
    })
}

/// The document with one key changed, and nothing else.
///
/// The file is parsed as a JSON document rather than rebuilt from
/// [`whirl_core::config::Config`], because `Config` is the schema's *values*: a
/// round trip through it would drop every key the schema does not name, and the
/// writer's contract is that nothing but the keys the UI owns may change.
fn compose(text: &str, seconds: u64) -> Result<String, WriteError> {
    let object = json_object(text)?;
    let mut schedule = match object.get("schedule") {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(schedule)) => schedule.clone(),
        Some(_) => {
            return Err(WriteError::Refused(
                "schedule: schedule is not an object".to_string(),
            ));
        }
    };
    // `insert` keeps the position of a key that is already there, so the
    // annotated file's own key order is what lands, not one this app invented.
    schedule.insert(
        "interval_seconds".to_string(),
        serde_json::Value::from(seconds),
    );

    let mut root = object.clone();
    root.insert("schedule".to_string(), serde_json::Value::Object(schedule));
    let mut encoded = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .map_err(|error| WriteError::Io(error.to_string()))?;
    // The daemon's own annotated default ends with a newline, and a text file
    // without one is a diff waiting to happen.
    encoded.push('\n');
    Ok(encoded)
}

/// The document's top-level object, or the reason it is not one.
fn json_object(text: &str) -> Result<serde_json::Map<String, serde_json::Value>, WriteError> {
    let root: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| WriteError::Refused(format!("the config file is not JSON: {error}")))?;
    match root {
        serde_json::Value::Object(object) => Ok(object),
        _ => Err(WriteError::Refused(
            "the config file is not a JSON object".to_string(),
        )),
    }
}

/// Replace `path` with `text` through a temporary file in the same directory.
///
/// The temp file is created `0600` (whirl's `docs/spec/state-and-cache.md` 6.3
/// requires it of every file whirl writes, and the daemon leaves this one `0600`
/// itself), written, fsynced, and renamed over the target. The directory is
/// fsynced afterwards so the rename is durable, not merely visible.
fn replace_atomically(path: &Path, text: &str) -> Result<(), WriteError> {
    let directory = path
        .parent()
        .ok_or_else(|| WriteError::Io(format!("{}: the path has no directory", path.display())))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.json".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let temp = directory.join(format!(
        ".{name}.whirl-ui-{}.{nanos}.tmp",
        std::process::id()
    ));

    let write = || -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        // Created at the mode it will keep: a rename moves the inode, so the
        // mode of the temp file is the mode of the config afterwards.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    };
    if let Err(error) = write() {
        let _ = std::fs::remove_file(&temp);
        return Err(WriteError::Io(format!("{}: {error}", temp.display())));
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(WriteError::Io(format!("{}: {error}", path.display())));
    }
    // Best effort: the file is already in place, and a directory that cannot be
    // opened for a sync does not make the write a failure.
    #[cfg(unix)]
    if let Ok(handle) = std::fs::File::open(directory) {
        let _ = handle.sync_all();
    }
    Ok(())
}

/// The parser's warnings, in its own words.
fn warnings(warnings: &[ConfigWarning]) -> Vec<String> {
    warnings.iter().map(ConfigWarning::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config with everything a rewrite must not lose: an unknown key, comment
    /// keys at two levels, and a legacy alias (`cache_dir` for `cache.root`).
    fn annotated() -> String {
        r#"{
  "_comment_1": "a comment key is ignored by every whirl parser",
  "config_schema": 1,
  "schedule": {
    "interval_seconds": 1800,
    "_comment_interval_seconds": "the rotation interval"
  },
  "cache_dir": "/tmp/whirl-cache-alias",
  "an_unknown_key_this_build_does_not_have": {"nested": [1, 2, 3]},
  "sources": []
}"#
        .to_string()
    }

    /// A directory of our own, named for the test that asked for it.
    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("whirlui-write-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        path
    }

    fn config_at(tag: &str, text: &str) -> PathBuf {
        let path = scratch(tag).join("config.json");
        std::fs::write(&path, text).expect("a config file");
        path
    }

    #[test]
    fn a_write_lands_the_new_value_and_the_parser_reads_it_back() {
        let path = config_at("lands", &annotated());
        let written = set_interval(&path, 900).expect("the write lands");
        assert_eq!(written.interval, 900);
        let loaded = Config::parse(&std::fs::read_to_string(&path).expect("the file"))
            .expect("the written file parses");
        assert_eq!(loaded.config.schedule.interval_seconds, 900);
    }

    #[test]
    fn a_write_keeps_every_key_it_did_not_own() {
        let path = config_at("keeps", &annotated());
        set_interval(&path, 600).expect("the write lands");
        let landed = std::fs::read_to_string(&path).expect("the file");
        // The comment keys, the unknown key and the alias are still there, with
        // their values, and the interval is the only value that moved.
        for expected in [
            "\"_comment_1\"",
            "\"_comment_interval_seconds\"",
            "\"an_unknown_key_this_build_does_not_have\"",
            "\"nested\"",
            "\"cache_dir\"",
            "\"/tmp/whirl-cache-alias\"",
            "\"interval_seconds\": 600",
        ] {
            assert!(landed.contains(expected), "{expected} is gone:\n{landed}");
        }
        // And it is still the same document, not a two-key rewrite of it.
        let before = Config::parse(&annotated()).expect("the fixture parses");
        let after = Config::parse(&landed).expect("the written file parses");
        assert_eq!(before.config.cache.root, after.config.cache.root);
        assert_eq!(before.config.config_schema, after.config.config_schema);
    }

    #[test]
    fn a_refused_value_leaves_the_file_byte_for_byte_alone() {
        let path = config_at("refused", &annotated());
        let before = std::fs::read(&path).expect("the file");
        // 30 is below the floor whirl-core enforces, so the parser refuses the
        // document the writer composed and nothing is renamed.
        let error = set_interval(&path, 30).expect_err("30 is refused");
        assert!(
            matches!(&error, WriteError::Refused(message)
                if message.contains("schedule.interval_seconds") && message.contains("less than 60")),
            "{error}"
        );
        assert_eq!(std::fs::read(&path).expect("the file"), before);
        // No temporary file survives a refusal either.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().expect("a directory"))
            .expect("the directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("whirl-ui"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_file_whose_other_keys_the_parser_rejects_is_not_written() {
        // The value asked for is legal, but the document it would produce is not:
        // the writer must refuse rather than land a file the daemon would not
        // have accepted.
        let text = r#"{"config_schema": 1, "schedule": {"interval_seconds": 1800}, "dedupe": {"recent_entries": 0}}"#;
        let path = config_at("other-key", text);
        let before = std::fs::read(&path).expect("the file");
        let error = set_interval(&path, 900).expect_err("the document is refused");
        assert!(
            matches!(&error, WriteError::Refused(message) if message.contains("dedupe.recent_entries")),
            "{error}"
        );
        assert_eq!(std::fs::read(&path).expect("the file"), before);
    }

    #[test]
    fn the_written_file_is_mode_six_hundred_on_unix() {
        let path = config_at("mode", &annotated());
        set_interval(&path, 900).expect("the write lands");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
    }

    #[test]
    fn a_missing_schedule_object_is_created_rather_than_the_write_refused() {
        let path = config_at("no-schedule", r#"{"config_schema": 1, "sources": []}"#);
        let written = set_interval(&path, 900).expect("the write lands");
        assert_eq!(written.interval, 900);
        let loaded = Config::parse(&std::fs::read_to_string(&path).expect("the file"))
            .expect("the written file parses");
        assert_eq!(loaded.config.schedule.interval_seconds, 900);
    }

    #[test]
    fn a_file_that_is_not_json_is_refused_and_left_alone() {
        let path = config_at("not-json", "this is not a config\n");
        let error = set_interval(&path, 900).expect_err("a refusal");
        assert!(matches!(error, WriteError::Refused(_)), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file"),
            "this is not a config\n"
        );
    }

    #[test]
    fn the_config_path_line_of_an_answer_names_the_target() {
        let target =
            Target::from_config_path_line("config: /somewhere/config.json").expect("a config line");
        assert_eq!(target.path, PathBuf::from("/somewhere/config.json"));
        assert!(target.named_by_daemon);
        // `-` is 2.6's unset, one of the two things that is not a path.
        assert_eq!(Target::from_config_path_line("config: -"), None);
        assert_eq!(Target::from_config_path_line("interval_s: 900"), None);
    }
}
