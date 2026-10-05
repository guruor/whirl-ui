//! The app's own user preferences, in the platform's own store.
//!
//! `whirl`'s `docs/architecture.md` section 8 item 9 puts an app's own answers
//! ("don't ask again", a window position) in the platform's preference store,
//! and not in the daemon's config file or its state directory: a preference of
//! the app's own is a file the daemon never reads and never writes.
//!
//! macOS keeps those in `NSUserDefaults`, which is `~/Library/Preferences` for
//! the app's bundle identifier (`com.guruor.whirl-ui`). This module is that
//! store and nothing else; it is compiled only on macOS, and a headless run may
//! point it at a scratch suite so its tests never touch a real person's
//! preferences.

use std::env;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSUserDefaults};

/// The environment variable that opens the store on a named suite.
///
/// The app's own preferences live in the app's bundle domain. A headless run
/// that has to exercise them without touching that domain names a scratch suite
/// here; the app itself never sets it.
pub const SUITE_ENV: &str = "WHIRL_UI_PREFS_SUITE";

/// The remembered answer to "Also stop whirl": `Some(true)` stop, `Some(false)`
/// keep running, `None` means the question is still asked.
const QUIT_KEY: &str = "quit-stop-daemon";
/// Whether the app's first run has already been through the launch offer.
const FIRST_RUN_KEY: &str = "first-run-done";

/// The two words the answer is stored as, so "unset" is a missing key and never
/// a value that reads as `false`.
const STOP: &str = "stop";
const KEEP: &str = "keep";

/// The store this run reads and writes.
fn store() -> Retained<NSUserDefaults> {
    if let Ok(suite) = env::var(SUITE_ENV)
        && !suite.is_empty()
        && let Some(defaults) = NSUserDefaults::initWithSuiteName(
            NSUserDefaults::alloc(),
            Some(&NSString::from_str(&suite)),
        )
    {
        return defaults;
    }
    NSUserDefaults::standardUserDefaults()
}

/// The remembered answer to the quit question, or `None` while it is asked.
pub fn quit_answer() -> Option<bool> {
    let defaults = store();
    let key = NSString::from_str(QUIT_KEY);
    // Presence is the question, because `boolForKey` answers `false` for a key
    // that was never set: an unset preference and a remembered "keep running"
    // are different states, and only presence tells them apart.
    defaults.objectForKey(&key)?;
    match defaults.boolForKey(&key) {
        true => Some(true),
        false => Some(false),
    }
}

/// Remember the answer for every later quit.
pub fn remember_quit_answer(stop: bool) {
    if stop {
        store().setBool_forKey(true, &NSString::from_str(QUIT_KEY));
    } else {
        // A stored `false` is still a stored answer, and `boolForKey` reads it.
        store().setBool_forKey(false, &NSString::from_str(QUIT_KEY));
    }
}

/// Whether the app has already had its first run.
pub fn first_run_done() -> bool {
    store().boolForKey(&NSString::from_str(FIRST_RUN_KEY))
}

/// Record that the app has had its first run.
pub fn mark_first_run_done() {
    store().setBool_forKey(true, &NSString::from_str(FIRST_RUN_KEY));
}

/// The words the answer is stored as, for the `--quit-answer` report.
pub fn quit_answer_word(answer: Option<bool>) -> &'static str {
    match answer {
        Some(true) => STOP,
        Some(false) => KEEP,
        None => "ask",
    }
}

// STOP and KEEP are read back through the booleans above, so the words exist to
// keep the stored shape readable and to name the report; this asserts the two
// agree without a display.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_words_name_the_three_states() {
        assert_eq!(quit_answer_word(None), "ask");
        assert_eq!(quit_answer_word(Some(true)), "stop");
        assert_eq!(quit_answer_word(Some(false)), "keep");
    }
}
