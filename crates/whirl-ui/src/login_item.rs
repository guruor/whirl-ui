//! The app's own login item: register it, unregister it, and read what macOS says
//! about it.
//!
//! M3's first criterion is that the app installs and removes its own login item.
//! The mechanism is macOS's own, `SMAppService.mainAppService`, and what it
//! produces is a Background Task Management record rather than a launchd service:
//! `launchctl print gui/$(id -u)/<label>` exits 113 for every label (measured on
//! macOS 26.7), so a criterion naming that command cannot be met by any mechanism
//! this repository is allowed to use. What can be read back without privilege is
//! the status this module reports (`--login-item status`) and the row `sfltool
//! dumpbtm` lists, which is what M3 criterion 1 now asks for.
//!
//! The app registers *its own bundle*, which is why the app has to be a bundle at
//! all: `SMAppService` speaks about the calling app's bundle, the Login Items pane
//! shows that bundle's `CFBundleName`, and an executable with no
//! `CFBundleIdentifier` has nothing to register. [`bundle`] answers that question
//! first, and every verb refuses with the path it looked at rather than
//! registering whatever directory the binary happens to sit in.
//!
//! Nothing here writes a file, and nothing here starts, stops or supervises the
//! daemon: this is the app's own login item, and the daemon's unit is whirl's
//! (docs/milestones.md M3 criterion 3).

/// What `SMAppService.status` reports about a login item.
///
/// The four values are Apple's, and they are kept apart here for the reason a
/// person reads them: "not registered" is the state an unregister leaves behind,
/// "requires approval" is a registration the user has still to allow in System
/// Settings, and "not found" is a question macOS could not answer at all. Folding
/// any two of them into one word would hide the one thing a reader needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the variants are only constructed through `State::of`, which reads macOS's `SMAppService` numbers; `Status` carries a `State` on every platform, so the type is compiled here and nothing but this module's test builds a value off macOS"
    )
)]
pub enum State {
    /// No registration, or one that was unregistered.
    NotRegistered,
    /// Registered, and eligible to run at login.
    Enabled,
    /// Registered, and waiting for the user in System Settings.
    RequiresApproval,
    /// macOS found no such service.
    NotFound,
}

impl State {
    /// The state a status number names, or `None` for a number this build does
    /// not know.
    ///
    /// The numbers are the ones in `SMAppServiceStatus`, and `None` rather than a
    /// guessed mapping: a status macOS grows in a future release is a fact this
    /// build has not been told about, and saying so is the only honest answer.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(
            dead_code,
            reason = "the numbers it maps are `SMAppService`'s, so only the macOS build calls it; its other caller is this module's own test, and use inside `#[cfg(test)]` does not count in a non-test build"
        )
    )]
    pub fn of(raw: isize) -> Option<State> {
        match raw {
            0 => Some(State::NotRegistered),
            1 => Some(State::Enabled),
            2 => Some(State::RequiresApproval),
            3 => Some(State::NotFound),
            _ => None,
        }
    }

    /// The word this state prints under.
    pub fn word(self) -> &'static str {
        match self {
            State::NotRegistered => "not registered",
            State::Enabled => "enabled",
            State::RequiresApproval => "requires approval",
            State::NotFound => "not found",
        }
    }

    /// One line saying what this state means, for the line after the status.
    pub fn means(self) -> &'static str {
        match self {
            State::NotRegistered => "the app is not registered to start at login",
            State::Enabled => "the app is registered and will start at login",
            State::RequiresApproval => {
                "the registration stands, and the user has still to allow it in Login Items"
            }
            State::NotFound => "macOS could not find the app's login item at all",
        }
    }
}

/// The status of the app's own login item, and the number behind it.
///
/// The number is carried beside the word because the word is this build's reading
/// of it: `--login-item status` prints both, so a reader comparing two runs sees
/// exactly what macOS said rather than what this app decided it said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// What the number names.
    pub state: State,
    /// The number `SMAppService.status` returned.
    pub raw: isize,
}

impl Status {
    /// The line a terminal prints for this status, before or after a change.
    pub fn line(&self) -> String {
        format!(
            "login item: {} (status {}): {}",
            self.state.word(),
            self.raw,
            self.state.means()
        )
    }
}

/// The bundle the running executable belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// `CFBundleIdentifier`, which is the name macOS registers.
    pub identifier: String,
    /// The bundle's directory, as `NSBundle` reports it.
    pub path: String,
}

impl Bundle {
    /// The line a terminal prints for the bundle a verb is about to act on.
    ///
    /// A registration is about one bundle and only that one, so naming it is part
    /// of the answer: `--login-item register` run from two different copies of the
    /// app registers two different items, and the line is what says which. It is
    /// also where the Login Items pane's row comes from, which is the second
    /// occurrence of `CFBundleName` in the bundle's `Info.plist`.
    pub fn line(&self) -> String {
        format!("bundle: {} at {}", self.identifier, self.path)
    }
}

/// The bundle the running executable belongs to, when there is one.
///
/// `None` for a plain binary: `NSBundle.mainBundle` still answers, with the
/// directory the executable is in and no identifier at all, and a login item
/// registered for that would be a record for a path and not for an app.
pub fn bundle() -> Option<Bundle> {
    platform::bundle()
}

/// What macOS reports about the app's own login item now.
pub fn status() -> Result<Status, String> {
    platform::status()
}

/// Ask macOS to start the app at login.
pub fn register() -> Result<(), String> {
    platform::register()
}

/// Remove the registration, so no login starts the app.
pub fn unregister() -> Result<(), String> {
    platform::unregister()
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2_foundation::{NSBundle, NSError};
    use objc2_service_management::SMAppService;

    use super::{Bundle, State, Status};

    pub(super) fn bundle() -> Option<Bundle> {
        let bundle = NSBundle::mainBundle();
        let identifier = bundle.bundleIdentifier()?.to_string();
        let path = bundle.bundlePath().to_string();
        Some(Bundle { identifier, path })
    }

    /// The status, with the number macOS reported kept beside it.
    ///
    /// `status` is the call M3 criterion 1's proof names: it is read before a
    /// change and after one, and the number is printed with the word so nothing
    /// here is a translation a reader has to take on trust.
    pub(super) fn status() -> Result<Status, String> {
        let service = unsafe { SMAppService::mainAppService() };
        let raw = unsafe { service.status() }.0;
        match State::of(raw) {
            Some(state) => Ok(Status { state, raw }),
            None => Err(format!(
                "macOS reported login item status {raw}, which this build does not know"
            )),
        }
    }

    /// Register the app's own bundle, and report macOS's refusal in its own words.
    pub(super) fn register() -> Result<(), String> {
        let service = unsafe { SMAppService::mainAppService() };
        unsafe { service.registerAndReturnError() }.map_err(describe)
    }

    /// Unregister it, with the same handling of the refusal.
    pub(super) fn unregister() -> Result<(), String> {
        let service = unsafe { SMAppService::mainAppService() };
        unsafe { service.unregisterAndReturnError() }.map_err(describe)
    }

    /// The error as macOS words it, with the domain and code a reader can look up.
    ///
    /// `localizedDescription` alone would lose the code, which is the part that
    /// says *why* (`kSMErrorAlreadyRegistered` and `kSMErrorJobNotFound` are both
    /// perfectly ordinary answers here, and they read alike until the code lands).
    fn describe(error: objc2::rc::Retained<NSError>) -> String {
        let description = error.localizedDescription().to_string();
        let domain = error.domain().to_string();
        let code = error.code();
        format!("{description} ({domain} error {code})")
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{Bundle, Status};

    /// No bundle on a platform this app does not ship a bundle for.
    pub(super) fn bundle() -> Option<Bundle> {
        None
    }

    /// The login item is macOS's: there is no other platform's to read.
    pub(super) fn status() -> Result<Status, String> {
        Err(elsewhere())
    }

    /// Nor one to register.
    pub(super) fn register() -> Result<(), String> {
        Err(elsewhere())
    }

    /// Nor one to remove.
    pub(super) fn unregister() -> Result<(), String> {
        Err(elsewhere())
    }

    /// The reason, worded once so the four arms above cannot drift apart.
    fn elsewhere() -> String {
        "the login item is macOS's; this build is not the macOS one and ships no bundle".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    /// The four numbers `SMAppServiceStatus` defines, in its order, and the words
    /// they print under. A number that moves between releases would silently
    /// rename a state, so the mapping is asserted rather than assumed.
    #[test]
    fn every_status_number_names_its_own_state() {
        let known = [
            (0, State::NotRegistered, "not registered"),
            (1, State::Enabled, "enabled"),
            (2, State::RequiresApproval, "requires approval"),
            (3, State::NotFound, "not found"),
        ];
        for (raw, state, word) in known {
            assert_eq!(State::of(raw), Some(state), "{raw}");
            assert_eq!(state.word(), word, "{raw}");
        }
        // And a number this build has not been told about is named as unknown
        // rather than folded into one of the four.
        assert_eq!(State::of(4), None);
        assert_eq!(State::of(-1), None);
    }

    /// The states are kept apart: no two of them share a word, which is what makes
    /// a before-and-after pair of lines readable on its own.
    #[test]
    fn no_two_states_read_alike() {
        let states = [
            State::NotRegistered,
            State::Enabled,
            State::RequiresApproval,
            State::NotFound,
        ];
        for (i, one) in states.iter().enumerate() {
            for other in &states[i + 1..] {
                assert_ne!(one.word(), other.word());
                assert_ne!(one.means(), other.means());
            }
        }
    }
}
