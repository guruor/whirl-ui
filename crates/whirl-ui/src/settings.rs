//! The settings window's content: two things a person sets, in their own words.
//!
//! The window shows exactly two choices and one line of state:
//!
//! - **where the wallpapers come from** ([`Sources`]): one row per source, a
//!   folder of the person's own pictures or Wallhaven's remote collection,
//!   described in those words rather than in the schema's. A Wallhaven source
//!   is added by the address of the collection to fetch, which is asked for
//!   first and stays editable afterwards ([`CollectionField`], [`Row::line`]);
//! - **how often they change** ([`Interval`]): a number and a unit, never the
//!   raw interval the file stores and never a key name;
//! - **whether whirl answered, and whether it is paused** ([`Daemon`]), one
//!   line, so nothing on screen reads as live while nothing is listening and a
//!   paused daemon never reads as a running one.
//!
//! What the window reads and writes is the config file, and the daemon's own
//! parser judges every write (`crate::config_file`), so a value this window
//! lands is one the daemon would have accepted. The daemon is told nothing: no
//! part of a change is a verb, and the window has no way to send one.
//!
//! Four rules shape the words on screen, and each one is a value a test can
//! read rather than a claim:
//!
//! - **A person never sees a key name.** The file's own spelling stays in the
//!   file, in [`crate::config_file`] and in the daemon's answers; what the
//!   window draws is [`Row::line`] and [`Interval::phrase`]. The one exception
//!   is a refusal the daemon itself wrote: [`Outcome::line`] quotes it as the
//!   reason, and its reason sentence names the config key the parser found
//!   wrong. Quoting the daemon verbatim is the point of that line; the window's
//!   own words around it name no key.
//! - **Nothing that can only be set in the file is on screen.** The window shows
//!   the two things a person asked for and no dump of the rest of the file.
//! - **An edit says when it applies.** Every saved edit says the change is in
//!   the file and that the wallpaper on screen has not moved, because the daemon
//!   reads the file on its own schedule and nothing here can tell the window
//!   that it has.
//! - **The token is never shown.** It lives in [`KeyField::token`] only between a
//!   person typing it and [`Settings::save_key`] handing it to the platform's
//!   store; it is in no line of any pane and [`Settings::to_text`] has no way to
//!   print it.

use std::path::{Path, PathBuf};

use whirlui_client::protocol::{SourceRecord, parse_plan_record};

use crate::about;
use crate::config_file::{self, FileSource, INTERVAL_KEY, Target};
use crate::keychain;

/// The window's title, and the app name eframe registers.
pub const WINDOW_TITLE: &str = "whirl settings";

/// How big the window opens. Fixed so that a screenshot of it has a size a
/// reader can check.
pub const WINDOW_SIZE: [f32; 2] = [860.0, 620.0];

/// Which of the window's four panes is on screen.
///
/// The window draws one pane at a time, and this is which: the sidebar's rows
/// move it, and a screenshot names it on the command line. It is a
/// build-time-only grouping of what the window says: each pane is one block
/// [`Settings::to_text`] prints, so no new pane is added by naming them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    /// Where the wallpapers come from.
    #[default]
    Sources,
    /// How often they change.
    Rotation,
    /// Whether whirl answered, and what the window writes.
    App,
    /// What this app is, which build, where its source is, and a newer release.
    About,
}

impl Pane {
    /// Every pane, in the order the sidebar shows them.
    pub const ALL: [Pane; 4] = [Pane::Sources, Pane::Rotation, Pane::App, Pane::About];

    /// The pane's name on a control, and the word `--screenshot` takes.
    pub fn name(self) -> &'static str {
        match self {
            Pane::Sources => "Sources",
            Pane::Rotation => "Rotation",
            Pane::App => "App",
            Pane::About => "About",
        }
    }
}

/// The one line under the title: what the window is for.
pub const SUBTITLE: &str = "where your wallpapers come from, and how often they change";

/// The first section.
pub const SOURCES_TITLE: &str = "Wallpapers come from";

/// The second section.
pub const ROTATION_TITLE: &str = "How often they change";

/// The Sources section's own line: what its controls do and when it applies.
pub const SOURCES_LINE: &str = "adding a folder of your own pictures, or Wallhaven's collection, saves it to the config file straight away; whirl uses it when it next reads the file";

/// The Sources section with nothing in it yet.
pub const NO_SOURCES: &str =
    "no wallpapers yet: add a folder of your own, or Wallhaven's collection";

/// The Rotation section's own line: what the control does and when it applies.
pub const ROTATION_LINE: &str = "how long each wallpaper stays before whirl changes it; saved to the config file straight away and used when whirl next reads the file, so the wallpaper on screen does not change yet";

/// The key field's own line.
pub const KEY_LINE: &str =
    "the key is saved in your system's password store and is never written to the config file";

/// The collection field's heading, in a person's words rather than the schema's.
pub const COLLECTION_TITLE: &str = "Wallhaven collection address";

/// The collection field's own line: the three addresses a person can paste.
///
/// The forms are the ones a person actually holds, and they are the same three
/// [`crate::config_file::COLLECTION_FORMS`] names in the refusal, so the field
/// and the reason it gives agree word for word.
pub const COLLECTION_LINE: &str = "paste the collection's address: https://wallhaven.cc/user/<username>/favorites/<id>, https://wallhaven.cc/api/v1/collections/<username>/<id>, or just <username>/<id>";

/// The collection field's second line: when a key is needed, and when it is not.
pub const COLLECTION_TOKEN_NOTE: &str = "a public collection needs no key; a private one, or one of your own account's, needs a key, which you can add once this is saved";

/// The line under the daemon's: where a change is actually written.
///
/// The window edits the config file and never the running daemon, and a person
/// who expects the wallpaper to change the moment they press Save deserves the
/// one sentence that says it will not until whirl reads the file again.
pub const APP_DAEMON_NOTE: &str =
    "changes are written to the config file; whirl picks them up the next time it reads it";

/// The window's own control for a daemon that is not answering, beside the
/// daemon line in the App pane. The same request as the menu's `Start whirl`
/// row: it is a control, not an interruption.
pub const START_LABEL: &str = "Start whirl";

/// What the control does, in the app's own words.
///
/// The route is worth one sentence because it is the whole of the permission the
/// daemon's contract gives the app: the app asks the daemon's own command, and
/// that command asks the OS supervisor. No process of the app's own is started,
/// and a daemon that refuses is shown rather than worked around.
pub const START_LINE: &str = "asks the daemon's own command, `whirl daemon start`, to start it through the OS supervisor; the app starts no process of its own";

/// The About section's heading.
pub const ABOUT_TITLE: &str = "About";

/// What this app is, in the product's own words.
///
/// The last clause is the amended frontend contract: the app never spawns a
/// daemon of its own, and the one way it takes part in the daemon's lifecycle is
/// by asking the daemon's own command, which asks the OS supervisor.
pub const ABOUT_LINE: &str = "whirl-ui is a lightweight tray frontend for the whirl wallpaper daemon: it reads the daemon's status and edits the config file, and it takes part in the daemon's lifecycle only by asking the daemon's own command, never by spawning a daemon of its own";

/// The heading over the release check.
pub const CHECK_TITLE: &str = "Is there a newer one?";

/// The check's button.
pub const CHECK_LABEL: &str = "Check for a newer release";

/// The check's own line: what it does, and what it does not.
pub const CHECK_LINE: &str = "asks GitHub once for this app's newest published release; it downloads nothing, sends nothing but the request, and repeats nothing on its own";

/// The folder chooser's title.
pub const PICKER_TITLE: &str = "Choose a folder";

/// What a saved edit says about itself, for the edits that land in the file.
pub const SAVED_FILE: &str = "the config file now says it, and the wallpaper on screen does not change until whirl next reads the file";

/// What a saved key says about itself.
pub const SAVED_KEY: &str = "the key is in your system's password store and the config file only names it, so the key itself is in no file whirl reads";

/// The one source kind with a secret behind it, in whirl's own spelling.
const WALLHAVEN: &str = "wallhaven";

/// The counter a source's record carries the count of its candidates under.
///
/// The record's counter group is the daemon's own (`crates/whirl-worker`'s
/// `Counters::pairs`: `candidates`, `admitted`, then one per rejection), and this
/// is the one that answers "how many does it hold".
const CANDIDATES: &str = "candidates";

/// The most sub-folders the chooser lists at once.
const CHILD_LIMIT: usize = 200;

/// The window: where the wallpapers come from, how often they change, and the
/// config file both are written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The config file an edit writes, when one could be located.
    pub target: Option<Target>,
    /// Which of the four panes is on screen.
    pub pane: Pane,
    /// Whether whirl answered, and whether its schedule is suspended.
    pub daemon: Daemon,
    /// The version the running bundle declares, when the app is inside one. The
    /// About pane reads it beside the binary's own version.
    pub bundle_version: Option<String>,
    /// The daemon's own version, when it answered the `version` request.
    pub daemon_version: Option<String>,
    /// The daemon's own words about the last `Start whirl` this window asked
    /// for, kept so the pane can show a refusal beside the line it belongs to.
    /// `None` until the control is pressed.
    pub daemon_action: Option<String>,
    /// The last release check, and what it found. `None` until the button is
    /// pressed: the app never checks on its own.
    pub check: Option<about::Check>,
    /// The platform store's answer about the Wallhaven key, asked once when the
    /// window is built: `Ok(true)` when an item is there, `Ok(false)` when none
    /// is, and the reason when the store could not be asked.
    ///
    /// Kept here so an edit rebuilds a row without asking the platform a second
    /// time, and so a window a test opens is built from the store the test
    /// handed in rather than from the machine's own keychain.
    pub key_saved: Result<bool, String>,
    /// Where the wallpapers come from.
    pub sources: Sources,
    /// How often they change.
    pub interval: Interval,
    /// The folder chooser, while one is open.
    pub picker: Option<Picker>,
    /// The Wallhaven key field, while it is open.
    pub key: KeyField,
    /// The Wallhaven collection field, while it is open.
    pub collection: CollectionField,
}

/// Whether whirl answered, as the one line the window carries about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    /// The socket answered, and the schedule is running.
    Running,
    /// The socket answered, and the schedule is suspended (`paused: 1` in
    /// `status`, section 2.5). The daemon is up: nothing failed to answer.
    Paused,
    /// The socket did not answer, in the client's own words.
    NotRunning(String),
}

impl Daemon {
    /// The line the window carries.
    pub fn line(&self) -> String {
        match self {
            Daemon::Running => "whirl is running".to_string(),
            Daemon::Paused => "whirl is paused".to_string(),
            Daemon::NotRunning(reason) => format!("whirl is not running: {reason}"),
        }
    }

    /// Whether the socket answered at all.
    ///
    /// A paused daemon answered, so this is what a caller asking "is whirl
    /// there" wants; only [`Daemon::NotRunning`] is a daemon that did not. It
    /// is the value beside the line, not the line: the line still tells the two
    /// apart.
    pub fn answered(&self) -> bool {
        !matches!(self, Daemon::NotRunning(_))
    }
}

/// Where the wallpapers come from: one row per source in the file's own order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sources {
    /// One row per source, in the file's order.
    pub rows: Vec<Row>,
    /// Why there is nothing to show, when the file could not be read.
    pub problem: Option<String>,
    /// The last edit, and what the window says about it.
    pub outcome: Option<Outcome>,
    /// What the daemon said when it was asked to check these sources, so each
    /// row can say whether it is usable and why not.
    pub checks: Checks,
}

/// The daemon's `config check`, as the rows read it.
///
/// The check answers with one `source:` record per source, carrying that
/// source's counters and its own `reason`, or the reason the check could not be
/// made at all (whirl's `docs/architecture.md` 2.6). A row whose source the
/// check did not name says so rather than saying nothing: silence would read as
/// "usable", which is the one thing nobody has said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checks {
    /// The check landed, and these are the records the daemon sent, in its order.
    Read(Vec<SourceRecord>),
    /// No check: the daemon did not answer, and this is the client's own reason.
    Unanswered(String),
}

impl Default for Checks {
    /// A window with no check at all, which says every row has not been checked
    /// rather than inventing one.
    fn default() -> Self {
        Checks::Read(Vec::new())
    }
}

/// One source, as the window describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The id the file knows the source by. It is never shown; the window needs
    /// it to name the source in an edit, and derives it when it adds one.
    pub id: String,
    /// Whether the source is in the rotation.
    pub enabled: bool,
    /// What kind of source it is, in a person's words.
    pub kind: Kind,
}

/// The two kinds of source, each carrying what it takes to describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A folder of the person's own pictures.
    Folder {
        /// The folders the source reads. The window shows the first and says how
        /// many more there are.
        folders: Vec<String>,
    },
    /// Wallhaven's remote collection.
    Wallhaven {
        /// The collection's address as the file spells it: the daemon's own
        /// `<username>/<id>` pair, or `None` when the file names none. It is
        /// what the row shows, so what the window would write is visible first.
        collection: Option<String>,
        /// What is known about its key.
        key: KeyState,
    },
}

/// What is known about a Wallhaven source's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyState {
    /// An item is in the platform's store. The window has not read its value.
    Saved,
    /// No item is in the store: the source needs a key.
    NeedsKey,
    /// The store could not be asked, which is not the same fact as a missing key.
    CannotTell(String),
    /// The config file names a key that is not this app's to resolve.
    NotOurs,
}

impl KeyState {
    /// The phrase the row carries.
    pub fn phrase(&self) -> String {
        match self {
            KeyState::Saved => "a key is saved".to_string(),
            KeyState::NeedsKey => "needs a key".to_string(),
            KeyState::CannotTell(reason) => {
                format!("cannot tell whether a key is saved ({reason})")
            }
            KeyState::NotOurs => {
                "the config file names a key this window does not manage".to_string()
            }
        }
    }
}

/// One row's second line: what the daemon's check said about that source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// The sentence, in a person's words.
    pub phrase: String,
    /// How the sentence should read.
    pub tone: Tone,
}

/// Whether a row's state is something a person can use as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The source works.
    Good,
    /// The source cannot be used, and the phrase says what to do about it.
    Bad,
    /// Nothing is known yet: the check has not landed.
    Unknown,
}

impl State {
    fn good(phrase: String) -> State {
        State {
            phrase,
            tone: Tone::Good,
        }
    }

    fn bad(phrase: String) -> State {
        State {
            phrase,
            tone: Tone::Bad,
        }
    }

    fn unknown(phrase: String) -> State {
        State {
            phrase,
            tone: Tone::Unknown,
        }
    }
}

impl Row {
    /// The row, described in a person's words.
    pub fn line(&self) -> String {
        match &self.kind {
            Kind::Folder { folders } => {
                let folder = folders.first().map(String::as_str).unwrap_or("(no folder)");
                let more = folders.len().saturating_sub(1);
                if more == 0 {
                    format!("A folder on this Mac: {folder}")
                } else {
                    format!("A folder on this Mac: {folder} (and {more} more)")
                }
            }
            Kind::Wallhaven { collection, key } => {
                let address = collection.as_deref().unwrap_or("no URL yet");
                format!(
                    "Wallhaven, a remote collection: {address} ({})",
                    key.phrase()
                )
            }
        }
    }

    /// The row as one line of text: the checkbox a person sees, and the row.
    ///
    /// The checkbox is here rather than at each place a row is printed, so the
    /// window's `[x]` and the text dump's `[x]` are the same character.
    pub fn text_line(&self) -> String {
        format!("[{}] {}", if self.enabled { "x" } else { " " }, self.line())
    }

    /// The row's second line: whether the source can be used, and why not.
    ///
    /// The words are built from the daemon's own `config check` record for this
    /// source, so the window states what the daemon measured rather than a
    /// second opinion about the same folder or collection. Every phrase names
    /// what is wrong and what to do about it; a counter on its own is not an
    /// answer a person can act on.
    pub fn state_line(&self, checks: &Checks) -> Option<State> {
        let record = match checks {
            Checks::Unanswered(reason) => {
                return Some(State::unknown(no_answer(self.thing(), reason)));
            }
            Checks::Read(records) => records.iter().find(|record| record.id == self.id),
        };
        let Some(record) = record else {
            // The check landed before this source did. Saying nothing here would
            // read as "usable", which is the one thing nobody has said.
            return Some(State::unknown(format!(
                "whirl has not checked this {} yet: it was added or changed after the check, and \
                 the next one covers it",
                self.thing()
            )));
        };
        Some(match &record.reason {
            None => self.holds(record),
            Some(reason) => State::bad(format!(
                "whirl cannot use this {}: {}",
                self.thing(),
                self.refusal(reason)
            )),
        })
    }

    /// What the row is about, in a person's words.
    ///
    /// The two kinds of row hold different things, and a failure reads
    /// differently for each: a folder is a place on this Mac and a collection is
    /// something wallhaven serves, so the row names which it means.
    fn thing(&self) -> &'static str {
        match self.kind {
            Kind::Folder { .. } => "folder",
            Kind::Wallhaven { .. } => "collection",
        }
    }

    /// The words when the check ran: how much the source holds.
    fn holds(&self, record: &SourceRecord) -> State {
        let count = counter(record, CANDIDATES);
        let holding = match &self.kind {
            Kind::Folder { .. } => picture(count),
            Kind::Wallhaven { key, .. } => match key {
                KeyState::Saved => format!("{} with the saved key", wallpaper(count)),
                _ => format!("{} without a key, so it is public", wallpaper(count)),
            },
        };
        // A source that reads with nothing in it is not one whirl will ever
        // change the wallpaper from, which is worth the same tone as a refusal.
        let words = format!("whirl can read this {}, and it {holding}", self.thing());
        if count == 0 {
            State::bad(words)
        } else {
            State::good(words)
        }
    }

    /// The words when the check refused the source, from the daemon's own
    /// reason.
    ///
    /// The daemon's reason names an internal key path and, for a folder, the
    /// operating system's own words for the failure; both are turned into the
    /// sentence a person acts on. A reason this code does not know is quoted
    /// rather than replaced, so an unfamiliar failure still reaches the screen.
    fn refusal(&self, reason: &str) -> String {
        let message = without_field(reason);
        match &self.kind {
            Kind::Folder { .. } => folder_refusal(message),
            Kind::Wallhaven { key, .. } => collection_refusal(message, key),
        }
    }

    /// The folder the row's `Change…` control replaces, when the row is a folder
    /// source holding exactly one.
    ///
    /// A source holding several is one the window describes as several and does
    /// not offer to change: replacing a list with one folder is a silent loss,
    /// and the window has nowhere on screen to say so.
    pub fn changeable_folder(&self) -> Option<&str> {
        match &self.kind {
            Kind::Folder { folders } if folders.len() == 1 => folders.first().map(String::as_str),
            _ => None,
        }
    }

    /// The collection address a Wallhaven row already reads, when the file names
    /// one. This is what the row's `Change URL…` control opens the field on: the
    /// address is edited rather than replaced from memory.
    pub fn collection_url(&self) -> Option<&str> {
        match &self.kind {
            Kind::Wallhaven {
                collection: Some(collection),
                ..
            } => Some(collection),
            _ => None,
        }
    }
}

/// The rotation interval: a number and a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval {
    /// The number the field holds, as typed.
    pub value: String,
    /// The unit beside it.
    pub unit: Unit,
    /// The rotation whirl is using now, when it reported one. This is what the
    /// daemon adopted, which is not necessarily what the file says.
    pub in_use: Option<u64>,
    /// The last edit, and what the window says about it.
    pub outcome: Option<Outcome>,
}

/// The units a person measures a rotation in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Minutes,
    Hours,
}

impl Unit {
    /// Both units, in the order the chooser lists them.
    pub const ALL: [Unit; 2] = [Unit::Minutes, Unit::Hours];

    /// How many seconds one of these is.
    pub fn seconds(self) -> u64 {
        match self {
            Unit::Minutes => 60,
            Unit::Hours => 3600,
        }
    }

    /// The unit's name, as a person writes it.
    pub fn name(self) -> &'static str {
        match self {
            Unit::Minutes => "minutes",
            Unit::Hours => "hours",
        }
    }

    /// The unit a stored number of seconds reads whole in, and minutes when
    /// neither does.
    fn of(seconds: u64) -> Unit {
        if seconds != 0 && seconds.is_multiple_of(3600) {
            Unit::Hours
        } else {
            Unit::Minutes
        }
    }

    /// A stored number of seconds as the number this unit's field holds.
    fn count(seconds: u64) -> String {
        let value = seconds as f64 / Unit::of(seconds).seconds() as f64;
        if value.fract() == 0.0 {
            format!("{}", value as u64)
        } else {
            format!("{value:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    }
}

impl Interval {
    /// The editor as the window opens on it: what the file says, when it could
    /// be read.
    fn opening(seconds: Option<u64>) -> Interval {
        Interval {
            value: seconds.map(Unit::count).unwrap_or_default(),
            unit: seconds.map(Unit::of).unwrap_or(Unit::Minutes),
            in_use: None,
            outcome: None,
        }
    }

    /// The seconds the field asks for, or the reason it is not a length of time.
    ///
    /// Whether the number is *acceptable* is the daemon's parser's question and
    /// is answered by the write; this is only whether it is a length of time at
    /// all.
    pub fn seconds(&self) -> Result<u64, String> {
        let raw = self.value.trim();
        // The reason does not quote the text: the control's own words already
        // carry it, and the line reads `Every half an hour: nothing was saved,
        // that is not a number`.
        let number: f64 = raw
            .parse()
            .map_err(|_| "that is not a number".to_string())?;
        if !number.is_finite() || number <= 0.0 {
            return Err("that is not a length of time".to_string());
        }
        let seconds = (number * self.unit.seconds() as f64).round();
        if seconds < 1.0 {
            return Err("that is not a length of time".to_string());
        }
        Ok(seconds as u64)
    }

    /// The control's own words, and the prefix of its line: `Every 30 minutes`.
    ///
    /// A value that is not a number has no unit to its name, so the words are
    /// just `Every half an hour`: printing the unit beside text the unit cannot
    /// apply to would be the window inventing a reading the field does not have.
    pub fn phrase(&self) -> String {
        let value = self.value.trim();
        if value.parse::<f64>().is_ok() {
            format!("Every {value} {}", self.unit.name())
        } else {
            format!("Every {value}")
        }
    }

    /// What whirl is using now, in the same words as the control, when it
    /// reported an interval.
    ///
    /// This is the second half of the honesty rule: a change written while the
    /// daemon runs is in the file and not in the daemon, and the window says
    /// which rotation is actually turning rather than letting the field read as
    /// though it were live.
    pub fn in_use_phrase(&self) -> Option<String> {
        self.in_use.map(|seconds| {
            format!(
                "whirl is using every {} {} now",
                Unit::count(seconds),
                Unit::of(seconds).name()
            )
        })
    }
}

/// The Wallhaven key field, while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyField {
    /// Whether the field is on screen.
    pub open: bool,
    /// What has been typed. Never written to a file this window reads, never
    /// part of a line, and never printed by [`Settings::to_text`].
    pub token: String,
}

/// The Wallhaven collection field, while it is open.
///
/// It asks for the one thing a Wallhaven source cannot be added without: the
/// address of the collection to fetch. The address is not a secret and is shown
/// as typed; the key is the field beside it, and stays in [`KeyField`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionField {
    /// Whether the field is on screen.
    pub open: bool,
    /// The address as typed.
    pub url: String,
    /// The source this address is going to, or `None` when the field is adding a
    /// new one.
    pub for_source: Option<String>,
    /// Why the address was refused, in the one sentence that names the accepted
    /// forms, shown beside the field. `None` until a save was refused.
    pub problem: Option<String>,
}

/// The folder chooser, while one is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    /// The source the choice goes to, or `None` when the folder becomes a new
    /// source.
    pub for_source: Option<String>,
    /// The folder the chooser is showing.
    pub directory: PathBuf,
    /// The folders inside it, in name order.
    pub entries: Vec<PathBuf>,
    /// Why the folder could not be listed, when it could not.
    pub problem: Option<String>,
}

/// What the platform's folder panel answered.
///
/// The window's two folder controls go through [`Settings::choose_folder`], and
/// this is the panel's whole vocabulary: a folder, a dismissal, or no panel to
/// show. The drawn browser ([`Picker`]) is the fallback for the last of the
/// three, so a machine without AppKit, a run with no window to put a modal panel
/// in, or a panel that would not present all reach the same control the app
/// always had.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the two answers a panel can give are only built by the macOS `NSOpenPanel` call (`ask_the_panel`); off macOS there is no panel, so nothing outside this module's test constructs them, and use inside `#[cfg(test)]` does not count in a non-test build"
    )
)]
pub enum PanelAnswer {
    /// The person chose this folder.
    Chosen(PathBuf),
    /// The person dismissed the panel. Nothing is written.
    Cancelled,
    /// There was no panel to show: the platform has none, or none could be
    /// presented. The drawn browser is the fallback.
    Unavailable,
}

/// The platform's folder panel, asked for one folder.
///
/// One question, one answer. The AppKit panel ([`SystemPanel`]) is the only
/// implementation that ships; it is behind a trait so a test can answer without
/// opening anything and still drive the write a click drives
/// ([`Settings::choose_folder`]).
pub trait FolderPanel {
    /// Ask the person for a folder.
    fn ask(&self) -> PanelAnswer;
}

/// The panel this app ships: `NSOpenPanel` on macOS, and no panel elsewhere.
///
/// On a platform without AppKit the answer is [`PanelAnswer::Unavailable`], so a
/// click reaches the drawn browser exactly as it always did. The Linux and
/// Windows CI legs compile that arm rather than the AppKit one.
#[derive(Debug, Clone, Copy)]
pub struct SystemPanel;

impl FolderPanel for SystemPanel {
    fn ask(&self) -> PanelAnswer {
        ask_the_panel()
    }
}

/// Ask the platform for a folder.
///
/// The one AppKit call in this app: an `NSOpenPanel` that offers directories and
/// no files, lets one be created, takes one choice, and is worded for a folder of
/// pictures. It runs modally on the main thread, which is the thread the click is
/// on, and it answers with the folder the person picked, a dismissal, or
/// [`PanelAnswer::Unavailable`] when there is no main-thread marker to present a
/// panel with (or the run came back neither OK nor Cancel), which is where the
/// drawn browser takes over.
#[cfg(target_os = "macos")]
fn ask_the_panel() -> PanelAnswer {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseCancel, NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::NSString;

    // A marker off the main thread means this is not a run that can present a
    // modal panel, and the drawn browser is the way in.
    let Some(mtm) = MainThreadMarker::new() else {
        return PanelAnswer::Unavailable;
    };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setCanCreateDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setPrompt(Some(&NSString::from_str("Choose")));
    panel.setMessage(Some(&NSString::from_str("Choose a folder of pictures")));
    // `runModal` answers `NSModalResponseOK` on a choice, `NSModalResponseCancel`
    // on a dismissal, and `NSModalResponseAbort` when the panel could not be
    // displayed. The three are compared rather than matched: the two constants
    // are statics, and a static cannot stand as a match pattern.
    let answer = panel.runModal();
    if answer == NSModalResponseOK {
        match panel.URL().and_then(|url| url.path()) {
            Some(folder) => PanelAnswer::Chosen(PathBuf::from(folder.to_string())),
            // OK with no folder behind it is not a choice this app can write.
            None => PanelAnswer::Cancelled,
        }
    } else if answer == NSModalResponseCancel {
        PanelAnswer::Cancelled
    } else {
        // `NSModalResponseAbort`: the panel failed to display, which is the
        // fallback's case rather than a dismissal.
        PanelAnswer::Unavailable
    }
}

/// No AppKit here: the drawn browser is the way in.
#[cfg(not(target_os = "macos"))]
fn ask_the_panel() -> PanelAnswer {
    PanelAnswer::Unavailable
}

/// What the last edit did, in the words of the control that made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The edit landed.
    Saved {
        /// The control that did it, in its own words.
        control: String,
        /// What is true now.
        detail: String,
    },
    /// The edit did not land, and the file is as it was.
    Refused {
        /// The control that was refused, in its own words.
        control: String,
        /// Why, in the words of whatever refused it.
        reason: String,
    },
}

impl Outcome {
    /// The line the window carries under the control.
    pub fn line(&self) -> String {
        match self {
            Outcome::Saved { control, detail } => format!("{control}: saved, {detail}"),
            Outcome::Refused { control, reason } => {
                format!("{control}: nothing was saved, {reason}")
            }
        }
    }
}

/// The daemon's answers, as the window needs them, or the reason there is no
/// answer.
///
/// Each `Ok` is the response body exactly as the daemon sent it. The window uses
/// two of its answers: where the daemon's config file is, and the rotation the
/// daemon is using now. What the file says is read from the file, because the
/// file is what the window edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answers {
    /// `Ok(())` when the socket answered, `Err(reason)` when it did not.
    pub connection: Result<(), String>,
    pub config_path: Result<Vec<String>, String>,
    pub config_check: Result<Vec<String>, String>,
    /// The daemon's `version` answer: its own version, the protocol and the
    /// platform. The About pane reads the daemon's version out of it.
    pub version: Result<Vec<String>, String>,
}

impl Answers {
    /// The answers of a live conversation.
    pub fn live(
        config_path: Vec<String>,
        config_check: Vec<String>,
        version: Vec<String>,
    ) -> Answers {
        Answers {
            connection: Ok(()),
            config_path: Ok(config_path),
            config_check: Ok(config_check),
            version: Ok(version),
        }
    }

    /// Every answer missing, because the daemon could not be reached.
    pub fn unreachable(reason: &str) -> Answers {
        Answers {
            connection: Err(reason.to_string()),
            config_path: Err(reason.to_string()),
            config_check: Err(reason.to_string()),
            version: Err(reason.to_string()),
        }
    }
}

/// The store a window asks when no caller names one.
///
/// The app's own build asks the platform's keychain. A test build asks a store
/// that holds no item, and the choice is made here rather than at each call site
/// so that a test cannot reach the machine's own keychain by accident: every
/// window a test opens asks nothing and starts no process, and a new call site
/// inherits the choice instead of having to remember it.
#[cfg(not(test))]
fn default_store() -> &'static dyn Store {
    &Keychain
}

#[cfg(test)]
fn default_store() -> &'static dyn Store {
    &NO_KEY
}

/// The store a test build asks: no item, and no process.
#[cfg(test)]
static NO_KEY: NoKey = NoKey;

/// [`NO_KEY`]'s type. A named type rather than a closure, because [`Store`] is a
/// trait and the static has to be an implementation of it.
#[cfg(test)]
struct NoKey;

#[cfg(test)]
impl Store for NoKey {
    fn holds_key(&self) -> Result<bool, String> {
        Ok(false)
    }
}

impl Settings {
    /// The window when the daemon cannot be asked anything.
    ///
    /// Not a blank window: the reasons are on screen, the file's own sources are
    /// still read where they can be, and every control is still there. A window
    /// that went blank when the daemon stopped would be a window a person cannot
    /// use to fix the thing the daemon stopped over.
    ///
    /// The bin builds its window from `from_answers`, so this is reached by the
    /// two windows a test opens with no daemon: `app`'s and the tray's.
    #[allow(dead_code)]
    pub fn unreachable(reason: &str) -> Settings {
        Settings::from_answers(&Answers::unreachable(reason))
    }

    /// The window as it opens: what the file says, and whether whirl answered.
    ///
    /// The file under [`Settings::target`] is where the two settings come from,
    /// because the file is what the window edits; the daemon's `config check`
    /// supplies the rotation it is using now and what it says about each source,
    /// which the window cannot see in the file. With no readable file the fields
    /// are empty and the reason is on screen rather than being papered over with
    /// a default.
    pub fn from_answers(answers: &Answers) -> Settings {
        Settings::from_answers_with(answers, default_store())
    }

    /// The same window, built from the store the caller hands in.
    ///
    /// [`Settings::from_answers`] asks the store this build is for
    /// ([`default_store`]) and this asks whatever it is handed, so a test can
    /// reach a key state the machine's own keychain cannot be asked about
    /// without reaching that keychain.
    pub fn from_answers_with(answers: &Answers, store: &dyn Store) -> Settings {
        let target = target_of(answers);
        let read = target
            .as_ref()
            .map(|target| config_file::read_file(&target.path));
        // The store is asked once, and only when a Wallhaven row is on screen,
        // because the question costs a process: see `rows_of`.
        let key_saved = match read.as_ref().and_then(|read| read.as_ref().ok()) {
            Some(state) if state.sources.iter().any(|source| source.kind == WALLHAVEN) => {
                store.holds_key()
            }
            _ => Ok(false),
        };
        let (rows, problem, stored) = match &read {
            None => (Vec::new(), Some(config_file::NO_PATH.to_string()), None),
            Some(Ok(state)) => (
                rows_of(&state.sources, &key_saved),
                None,
                Some(state.interval_seconds),
            ),
            Some(Err(error)) => (Vec::new(), Some(error.to_string()), None),
        };
        let in_use = plan_interval(answers);
        let mut interval = Interval::opening(stored.or(in_use));
        interval.in_use = in_use;
        Settings {
            daemon: match &answers.connection {
                Ok(()) => Daemon::Running,
                Err(reason) => Daemon::NotRunning(reason.clone()),
            },
            target,
            pane: Pane::default(),
            bundle_version: about::bundle_version(),
            daemon_version: daemon_version_of(answers),
            // The window opens with no check made: it is the button's job, and
            // nothing here reaches the network.
            check: None,
            daemon_action: None,
            key_saved,
            sources: Sources {
                rows,
                problem,
                outcome: None,
                checks: checks_of(answers),
            },
            interval,
            picker: None,
            key: KeyField::default(),
            collection: CollectionField::default(),
        }
    }

    /// The version the app is running as: the bundle's when it is inside one,
    /// this binary's when it is not.
    pub fn running_version(&self) -> String {
        self.bundle_version
            .clone()
            .unwrap_or_else(|| about::binary_version().to_string())
    }

    /// The version, as the About pane states it.
    ///
    /// One line when the bundle and the binary agree, and two when they do not:
    /// the disagreement is the diagnostic, and a build installed over another is
    /// exactly that.
    pub fn version_lines(&self) -> Vec<String> {
        let binary = about::binary_version();
        match &self.bundle_version {
            Some(bundle) if bundle != binary => vec![
                format!("version {bundle} (the app bundle)"),
                format!("version {binary} (this binary)"),
            ],
            Some(bundle) => vec![format!("version {bundle}")],
            None => vec![format!(
                "version {binary} (this binary; not running from an app bundle)"
            )],
        }
    }

    /// The daemon's version and whether it answered, as the About pane states
    /// it: the version only when there is one, and the reason never here, since
    /// the App pane already carries it. A paused daemon answered, so its line
    /// says paused rather than connected, word for word with the App pane's.
    pub fn daemon_line(&self) -> String {
        match (&self.daemon, &self.daemon_version) {
            (Daemon::Running, Some(version)) => format!("the daemon: {version}, connected"),
            (Daemon::Running, None) => {
                "the daemon: connected, and it reported no version".to_string()
            }
            (Daemon::Paused, Some(version)) => format!("the daemon: {version}, paused"),
            (Daemon::Paused, None) => "the daemon: paused, and it reported no version".to_string(),
            (Daemon::NotRunning(_), _) => "the daemon: not connected".to_string(),
        }
    }

    /// Run the release check, on demand, and record what it found.
    ///
    /// This is the whole of the button: one call, made where the person pressed
    /// it. It is synchronous on purpose, so no thread, timer or interval is
    /// involved in asking, and nothing else in the app calls it.
    pub fn check_release(&mut self) {
        self.check = Some(about::check());
    }

    /// Write the rotation the field holds, and say what happened.
    ///
    /// The whole of the write: a length of time, then the file, then the
    /// daemon's parser. Nothing here opens a socket.
    pub fn save_interval(&mut self) {
        let control = self.interval.phrase();
        let outcome = match self.interval.seconds() {
            Err(reason) => Outcome::Refused { control, reason },
            Ok(seconds) => match self.writing_target() {
                Err(error) => Outcome::Refused {
                    control,
                    reason: error.to_string(),
                },
                Ok(path) => match config_file::set_interval(path, seconds) {
                    Ok(_) => Outcome::Saved {
                        control,
                        detail: SAVED_FILE.to_string(),
                    },
                    Err(error) => Outcome::Refused {
                        control,
                        reason: error.to_string(),
                    },
                },
            },
        };
        self.interval.outcome = Some(outcome);
    }

    /// Add `folder` as a new source at the end of the file.
    ///
    /// The id the schema requires is derived from the folder's name, because an
    /// id is the file's word rather than the person's and the window is the
    /// place the choice is made.
    pub fn add_folder(&mut self, folder: &str) {
        let id = self.free_id(&folder_name(folder));
        self.add_folder_as(&id, folder);
    }

    /// Add `folder` under an id the caller names.
    ///
    /// The window derives the id; a command line is the caller that has to name
    /// the source it is adding, and this is the same write with that one field
    /// decided further up.
    pub fn add_folder_as(&mut self, id: &str, folder: &str) {
        let document = config_file::local_source(id, folder);
        let result = self
            .writing_target()
            .and_then(|path| config_file::add_source(path, document));
        let landed = result.is_ok();
        self.record_sources("Add a folder", SAVED_FILE, result);
        if landed {
            // The check was made before this source existed.
            self.forget_check(id);
        }
    }

    /// Open the field that asks for a new Wallhaven source's collection address.
    ///
    /// Nothing is written yet: a Wallhaven source with no collection is a source
    /// that names nothing to fetch, which is the defect this field exists to
    /// answer. The source lands when the address does.
    pub fn ask_for_collection(&mut self) {
        self.collection = CollectionField {
            open: true,
            url: String::new(),
            for_source: None,
            problem: None,
        };
    }

    /// Open the field on the address an existing Wallhaven row already reads.
    ///
    /// The address is prefilled from the file, so the person changes it rather
    /// than retyping it from memory.
    pub fn edit_collection(&mut self, id: &str) {
        let url = self
            .sources
            .rows
            .iter()
            .find(|row| row.id == id)
            .and_then(|row| row.collection_url())
            .unwrap_or_default()
            .to_string();
        self.collection = CollectionField {
            open: true,
            url,
            for_source: Some(id.to_string()),
            problem: None,
        };
    }

    /// Land the address the field holds, and say what happened.
    ///
    /// The address is reduced to the daemon's own `<username>/<id>` pair before
    /// anything is written, so a form the daemon would refuse never reaches the
    /// file: the refusal is one sentence beside the field and the file is
    /// untouched. A new source lands with its collection, and the key field is
    /// offered next because the key is optional and the collection is not.
    pub fn save_collection(&mut self) {
        let adding = self.collection.for_source.is_none();
        let control = if adding {
            "Add Wallhaven"
        } else {
            "Change URL"
        };
        let pair = match config_file::collection_of(&self.collection.url) {
            Ok(pair) => pair,
            Err(reason) => {
                self.collection.problem = Some(reason.clone());
                self.sources.outcome = Some(Outcome::Refused {
                    control: control.to_string(),
                    reason,
                });
                return;
            }
        };
        let affected = self
            .collection
            .for_source
            .clone()
            .unwrap_or_else(|| self.free_id(WALLHAVEN));
        let result = match self.collection.for_source.clone() {
            None => {
                let document = config_file::wallhaven_source(&affected, keychain::LABEL, &pair);
                self.writing_target()
                    .and_then(|path| config_file::add_source(path, document))
            }
            Some(_) => self
                .writing_target()
                .and_then(|path| config_file::set_source_collection(path, &affected, &pair)),
        };
        let saved = result.is_ok();
        self.record_sources(control, SAVED_FILE, result);
        if saved {
            // What the check said was about the address this source used to name.
            self.forget_check(&affected);
            self.collection.problem = None;
            self.collection.open = false;
            if adding {
                // The key is the optional half: the source is on screen and in
                // the file, and the field that adds the key opens beside it.
                self.key.open = true;
            }
        }
    }

    /// Close the collection field and change nothing.
    pub fn cancel_collection(&mut self) {
        self.collection = CollectionField::default();
    }

    /// Move a source one place in the file's order, and nothing at the edge.
    pub fn move_source(&mut self, id: &str, direction: config_file::Direction) {
        let result = self
            .writing_target()
            .and_then(|path| config_file::move_source(path, id, direction));
        self.record_sources("Move", SAVED_FILE, result);
    }

    /// Point an existing folder source at `folder`.
    pub fn set_source_folder(&mut self, id: &str, folder: &str) {
        let result = self
            .writing_target()
            .and_then(|path| config_file::set_source_folder(path, id, folder));
        let landed = result.is_ok();
        self.record_sources("Change folder", SAVED_FILE, result);
        if landed {
            // What the check said was about the folder this source used to read.
            self.forget_check(id);
        }
    }

    /// Take a source out of the file.
    pub fn remove_source(&mut self, id: &str) {
        let result = self
            .writing_target()
            .and_then(|path| config_file::remove_source(path, id));
        self.record_sources("Remove", SAVED_FILE, result);
    }

    /// Put a source in the rotation, or take it out of it.
    pub fn set_source_enabled(&mut self, id: &str, enabled: bool) {
        let result = self
            .writing_target()
            .and_then(|path| config_file::set_source_enabled(path, id, enabled));
        self.record_sources("In the rotation", SAVED_FILE, result);
    }

    /// Save the typed key in the platform's store, and point the file at it.
    ///
    /// This is the only call in the window that touches the secret. Nothing
    /// reads the item back, the field empties once the store has it, and a store
    /// that refused leaves the field alone, because retyping a key is not how a
    /// failure should be reported.
    pub fn save_key(&mut self) {
        let token = self.key.token.clone();
        let result = match keychain::store(&token) {
            Err(error) => Err(config_file::WriteError::Io(error.to_string())),
            Ok(()) => self
                .writing_target()
                .and_then(|path| config_file::set_wallhaven_key_ref(path, keychain::LABEL)),
        };
        let saved = result.is_ok();
        if saved {
            // The store holds the item now, so the rows say a key is saved
            // without the platform being asked a second time.
            self.key_saved = Ok(true);
        }
        self.record_sources("Wallhaven key", SAVED_KEY, result);
        if saved {
            // What the check said about a collection was about the old key.
            self.forget_wallhaven_checks();
            self.key.token.clear();
            self.key.open = false;
        }
    }

    /// Ask for a folder through `panel`, and write what the answer says.
    ///
    /// The whole of the `Add a folder…` and `Change…` controls: the platform's
    /// panel is asked first, and its answer is a folder to write through the same
    /// path the commands use (`add_folder` for a new source, `set_source_folder`
    /// for an existing one), a dismissal that writes nothing, or no panel at all,
    /// which is where the drawn browser ([`Settings::open_picker`]) takes over.
    /// The AppKit panel is the only implementation that ships; a test hands its
    /// own answer in and drives the same write without opening anything.
    pub fn choose_folder(&mut self, for_source: Option<String>, panel: &dyn FolderPanel) {
        match panel.ask() {
            PanelAnswer::Chosen(folder) => {
                let folder = folder.display().to_string();
                match for_source {
                    Some(id) => self.set_source_folder(&id, &folder),
                    None => self.add_folder(&folder),
                }
            }
            PanelAnswer::Cancelled => {}
            PanelAnswer::Unavailable => self.open_picker(for_source),
        }
    }

    /// Open the drawn browser, for one source or for a new one.
    ///
    /// The fallback for [`Settings::choose_folder`]: the way in when there is no
    /// panel to ask (a machine without AppKit) or none could be presented. Its
    /// clicks are the picker methods below, and it stays the documented behaviour
    /// for those cases.
    pub fn open_picker(&mut self, for_source: Option<String>) {
        let start = for_source
            .as_ref()
            .and_then(|id| self.folder_of(id).map(PathBuf::from));
        let directory = start
            .or_else(home)
            .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR.to_string()));
        self.picker = Some(Picker {
            for_source,
            directory,
            entries: Vec::new(),
            problem: None,
        });
        self.refresh_picker();
    }

    /// Show the folders inside `directory`.
    pub fn picker_into(&mut self, directory: PathBuf) {
        if let Some(picker) = self.picker.as_mut() {
            picker.directory = directory;
        }
        self.refresh_picker();
    }

    /// Show the folder the chooser is in.
    pub fn picker_up(&mut self) {
        if let Some(picker) = self.picker.as_mut()
            && let Some(parent) = picker.directory.parent()
        {
            picker.directory = parent.to_path_buf();
        }
        self.refresh_picker();
    }

    /// Close the chooser and change nothing.
    pub fn picker_cancel(&mut self) {
        self.picker = None;
    }

    /// Use the folder the chooser is showing.
    pub fn picker_choose(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let folder = picker.directory.display().to_string();
        match picker.for_source {
            Some(id) => self.set_source_folder(&id, &folder),
            None => self.add_folder(&folder),
        }
    }

    /// The config file an edit writes, or the reason none could be located.
    fn writing_target(&self) -> Result<&Path, config_file::WriteError> {
        self.target
            .as_ref()
            .map(|target| target.path.as_path())
            .ok_or_else(|| config_file::WriteError::Io(config_file::NO_PATH.to_string()))
    }

    /// Record a source edit's result, and move the rows to what the file says.
    ///
    /// A refusal changes no row, because the file was not written: it changes
    /// only the line under the controls, which is where the reason belongs. The
    /// detail is the caller's, because the key is the one edit that does not land
    /// in the file.
    fn record_sources(
        &mut self,
        control: &str,
        detail: &str,
        result: Result<config_file::SourcesWritten, config_file::WriteError>,
    ) {
        let outcome = match result {
            Ok(written) => {
                self.sources.rows = rows_of(&written.sources, &self.key_saved);
                self.sources.problem = None;
                Outcome::Saved {
                    control: control.to_string(),
                    detail: detail.to_string(),
                }
            }
            Err(error) => Outcome::Refused {
                control: control.to_string(),
                reason: error.to_string(),
            },
        };
        self.sources.outcome = Some(outcome);
    }

    /// Drop one source's record, because it changed after the check was made.
    ///
    /// The row then says it has not been checked rather than quoting a check
    /// about the source as it was: a folder that moved is not the folder the
    /// daemon counted pictures in.
    fn forget_check(&mut self, id: &str) {
        if let Checks::Read(records) = &mut self.sources.checks {
            records.retain(|record| record.id != id);
        }
    }

    /// Drop every Wallhaven row's record, because the key they resolve changed.
    fn forget_wallhaven_checks(&mut self) {
        let wallhaven: Vec<String> = self
            .sources
            .rows
            .iter()
            .filter(|row| matches!(row.kind, Kind::Wallhaven { .. }))
            .map(|row| row.id.clone())
            .collect();
        for id in wallhaven {
            self.forget_check(&id);
        }
    }

    /// The folders inside the chooser's folder, or why it could not be listed.
    fn refresh_picker(&mut self) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match read_directories(&picker.directory) {
            Ok(entries) => {
                picker.entries = entries;
                picker.problem = None;
            }
            Err(reason) => {
                picker.entries.clear();
                picker.problem = Some(reason);
            }
        }
    }

    /// The folder the source named `id` reads, when it is a folder source.
    fn folder_of(&self, id: &str) -> Option<String> {
        self.sources
            .rows
            .iter()
            .find(|row| row.id == id)
            .and_then(|row| row.changeable_folder().map(str::to_string))
    }

    /// An id the file does not already use, derived from what the person chose.
    fn free_id(&self, name: &str) -> String {
        let base = sanitize_id(name);
        let taken = |candidate: &str| self.sources.rows.iter().any(|row| row.id == candidate);
        if !taken(&base) {
            return base;
        }
        for suffix in 2..1000 {
            let candidate = format!("{base}-{suffix}");
            if !taken(&candidate) {
                return candidate;
            }
        }
        base
    }

    /// The panes as text: what `whirl-ui --dump-settings` prints and what a test
    /// asserts, so the window is checkable with no display attached.
    ///
    /// The token has no place here: [`KeyField::token`] is not read, so no value
    /// a person typed can reach this text or a log of it.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(WINDOW_TITLE);
        out.push('\n');
        out.push_str("  ");
        out.push_str(SUBTITLE);
        out.push('\n');

        if let Some(picker) = &self.picker {
            out.push('\n');
            out.push_str(PICKER_TITLE);
            out.push('\n');
            out.push_str(&format!("  {}\n", picker.directory.display()));
            out.push_str("  [Up]\n");
            match &picker.problem {
                Some(reason) => out.push_str(&format!("    {reason}\n")),
                None if picker.entries.is_empty() => {
                    out.push_str("    (no folders inside it)\n");
                }
                None => {
                    for entry in &picker.entries {
                        out.push_str(&format!(
                            "    {}\n",
                            entry
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        ));
                    }
                }
            }
            out.push_str("  [Use this folder] [Cancel]\n");
            return out;
        }

        out.push('\n');
        out.push_str(SOURCES_TITLE);
        out.push('\n');
        match &self.sources.problem {
            Some(reason) => out.push_str(&format!("  {reason}\n")),
            None if self.sources.rows.is_empty() => {
                out.push_str(&format!("  {NO_SOURCES}\n"));
            }
            None => {
                for row in &self.sources.rows {
                    out.push_str(&format!("  {}\n", row.text_line()));
                    // The row's own line about whether it can be used, above the
                    // controls that change it.
                    if let Some(state) = row.state_line(&self.sources.checks) {
                        out.push_str(&format!("      {}\n", state.phrase));
                    }
                    let mut buttons = Vec::new();
                    if row.changeable_folder().is_some() {
                        buttons.push("Change…");
                    }
                    if matches!(row.kind, Kind::Wallhaven { .. }) {
                        buttons.push("Change URL…");
                        buttons.push("Enter key…");
                    }
                    buttons.push("Remove");
                    out.push_str(&format!(
                        "        {}\n",
                        buttons
                            .iter()
                            .map(|label| format!("[{label}]"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
            }
        }
        out.push_str("  [Add a folder…] [Add Wallhaven]\n");
        out.push_str(&format!("  {SOURCES_LINE}\n"));
        if let Some(outcome) = &self.sources.outcome {
            out.push_str(&format!("  {}\n", outcome.line()));
        }
        if self.key.open {
            out.push_str("  Wallhaven key [••••] [Save key] [Cancel]\n");
            out.push_str(&format!("  {KEY_LINE}\n"));
        }
        if self.collection.open {
            out.push_str(&format!(
                "  {COLLECTION_TITLE} [{}] [Save] [Cancel]\n",
                self.collection.url
            ));
            out.push_str(&format!("  {COLLECTION_LINE}\n"));
            out.push_str(&format!("  {COLLECTION_TOKEN_NOTE}\n"));
            if let Some(problem) = &self.collection.problem {
                out.push_str(&format!("  {problem}\n"));
            }
        }

        out.push('\n');
        out.push_str(ROTATION_TITLE);
        out.push('\n');
        out.push_str(&format!(
            "  Every [{}] [{}] [Save]\n",
            self.interval.value,
            self.interval.unit.name()
        ));
        out.push_str(&format!("  {ROTATION_LINE}\n"));
        if let Some(outcome) = &self.interval.outcome {
            out.push_str(&format!("  {}\n", outcome.line()));
        }
        if let Some(now) = self.interval.in_use_phrase() {
            out.push_str(&format!("  {now}\n"));
        }

        out.push('\n');
        out.push_str(&self.daemon.line());
        out.push('\n');
        out.push_str(APP_DAEMON_NOTE);

        out.push('\n');
        out.push('\n');
        out.push_str(ABOUT_TITLE);
        out.push('\n');
        out.push_str(&format!("  {ABOUT_LINE}\n"));
        for line in self.version_lines() {
            out.push_str(&format!("  {line}\n"));
        }
        // The source is named as a sentence rather than as a `source:` record,
        // so nothing here can be mistaken for one of the daemon's own lines.
        out.push_str(&format!("  the source is {}\n", about::SOURCE_URL));
        out.push_str(&format!("  {}\n", self.daemon_line()));
        out.push_str(&format!("  [{CHECK_LABEL}]\n"));
        out.push_str(&format!("  {CHECK_LINE}\n"));
        if let Some(check) = &self.check {
            out.push_str(&format!("  {}\n", check.line()));
        }
        out
    }
}

/// The store the window asks whether the Wallhaven key is saved.
///
/// A seam, not a wrapper: [`Settings::from_answers`] asks the store this build
/// is for ([`default_store`]) and [`Settings::from_answers_with`] asks whatever
/// it is handed, so every key state a row can show is reachable from a test that
/// never reads the machine's keychain. The question is whether an item is there,
/// never what it holds.
pub trait Store {
    /// `Ok(true)` when the item is there, `Ok(false)` when it is not, and the
    /// reason when the store could not be asked.
    fn holds_key(&self) -> Result<bool, String>;
}

/// The store this app ships: the platform's own keychain, through the one call
/// that asks for presence and never for bytes ([`crate::keychain::exists`]).
#[cfg_attr(test, allow(dead_code))] // A test build's windows ask the test store above, not this one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Keychain;

impl Store for Keychain {
    fn holds_key(&self) -> Result<bool, String> {
        keychain::exists().map_err(|error| error.to_string())
    }
}

/// The daemon's check, parsed from the answer it gave.
fn checks_of(answers: &Answers) -> Checks {
    match &answers.config_check {
        Ok(lines) => Checks::Read(whirlui_client::ConfigCheck::from_lines(lines).records),
        Err(reason) => Checks::Unanswered(reason.clone()),
    }
}

/// The rows for a file's sources, each Wallhaven one beside what the store said.
///
/// The store's answer is the caller's, asked once and only when a Wallhaven row
/// is on screen, because the question costs a process; see
/// [`Settings::from_answers_with`].
fn rows_of(sources: &[FileSource], exists: &Result<bool, String>) -> Vec<Row> {
    let mut rows = Vec::with_capacity(sources.len());
    for source in sources {
        let kind = if source.kind == WALLHAVEN {
            Kind::Wallhaven {
                collection: source.collection.clone(),
                key: key_state(source.key_ref.as_deref(), exists.clone()),
            }
        } else {
            Kind::Folder {
                folders: source.paths.clone(),
            }
        };
        rows.push(Row {
            id: source.id.clone(),
            enabled: source.enabled,
            kind,
        });
    }
    rows
}

/// The words for a check that did not land, from the client's own reason.
///
/// A read that ran out of time is the one case worth saying in the window's own
/// words: the client's timeout is the socket's, so its reason is the platform's
/// io error for a read that had none, which is not a sentence a person can act
/// on.
fn no_answer(thing: &str, reason: &str) -> String {
    if is_a_timeout(reason) {
        return format!(
            "whirl has not checked this {thing} yet: the daemon did not answer in time, and a \
             check can take minutes; open this window again in a moment"
        );
    }
    format!("whirl has not checked this {thing} yet: {reason}")
}

/// Whether the client's reason is a read that ran out of time.
///
/// The client's timeout is the socket's read timeout, equal to the daemon's own
/// worker deadline (whirlui-client's `COMMAND_TIMEOUT`), so the reason is the
/// platform's io error for a read with nothing to read: macOS and Linux both say
/// "Resource temporarily unavailable", and a timeout built in Rust says
/// "timed out".
fn is_a_timeout(reason: &str) -> bool {
    [
        "timed out",
        "Resource temporarily unavailable",
        "would block",
        "WouldBlock",
    ]
    .iter()
    .any(|words| reason.contains(words))
}

/// A record's counter, or zero when the record carries no group at all.
fn counter(record: &SourceRecord, name: &str) -> u64 {
    record
        .counters
        .iter()
        .find(|(counter, _)| counter == name)
        .map(|(_, value)| *value)
        .unwrap_or(0)
}

/// How many pictures a folder holds, in a person's words.
fn picture(count: u64) -> String {
    match count {
        0 => "holds no pictures".to_string(),
        1 => "holds one picture".to_string(),
        count => format!("holds {count} pictures"),
    }
}

/// How many wallpapers a collection holds, in a person's words.
fn wallpaper(count: u64) -> String {
    match count {
        0 => "holds no wallpapers".to_string(),
        1 => "holds one wallpaper".to_string(),
        count => format!("holds {count} wallpapers"),
    }
}

/// A reason without the daemon's own `sources[id=<id>].<field> (line N): ` prefix.
///
/// The prefix names an internal key path; what follows it is the sentence a
/// person acts on. A reason with no prefix is returned as it is.
fn without_field(reason: &str) -> &str {
    match reason.strip_prefix("sources[") {
        Some(rest) => rest.split_once(": ").map_or(reason, |(_, message)| message),
        None => reason,
    }
}

/// Why a folder source cannot be used, in the words a person acts on.
///
/// The daemon reports the operating system's own words for the failure, so the
/// mapping is over those words: a missing folder and an unreadable one are the
/// two the daemon can tell apart, and they need different actions from the
/// person. A failure this does not know is quoted rather than replaced.
fn folder_refusal(message: &str) -> String {
    if message.contains("no configured path can be read") {
        if message.contains("No such file or directory") || message.contains("cannot find the file")
        {
            return "the folder is not there; check the path, or remove this row".to_string();
        }
        if message.contains("Permission denied") {
            return "the folder is there and cannot be read: permission denied".to_string();
        }
        if message.contains("is a symlink") {
            return "the folder is a link to another one, and whirl does not follow those; name \
                    the folder itself"
                .to_string();
        }
        if message.contains("is not a directory") {
            return "that path is a file, not a folder".to_string();
        }
        return "whirl cannot read the folder".to_string();
    }
    if message.contains("requires an API key") {
        return "it needs a key, and none is saved".to_string();
    }
    message.to_string()
}

/// Why a Wallhaven source cannot be used, in the words a person acts on.
///
/// The daemon names the failure first (`not_found:`, `unauthorized:`, and the
/// rest of its own vocabulary) and the key the request carried is this window's
/// to know, so the two together say whether the collection is missing, private
/// and keyless, or holding a key the API refused.
fn collection_refusal(message: &str, key: &KeyState) -> String {
    if message.contains("requires an API key") {
        return "it needs a key, and none is saved: use Enter key… to add one".to_string();
    }
    match message.split_once(':') {
        Some(("not_found", detail)) => match key {
            KeyState::Saved => format!(
                "wallhaven answered 404 with the saved key, so the address names no collection: \
                 check it{}",
                tail(detail)
            ),
            _ => format!(
                "wallhaven answered 404 without a key: the address may name no collection, and a \
                 private one needs a key{}",
                tail(detail)
            ),
        },
        Some(("unauthorized", detail)) => match key {
            KeyState::Saved => format!(
                "the saved key was refused: wallhaven answered 401{}",
                tail(detail)
            ),
            _ => format!(
                "the collection needs a key and none is saved: wallhaven answered 401{}",
                tail(detail)
            ),
        },
        Some(("forbidden", detail)) => format!(
            "wallhaven refused the request: it answered 403{}",
            tail(detail)
        ),
        Some(("rate_limited", detail)) => format!(
            "wallhaven is rate-limiting this app: it answered 429, and the check works later{}",
            tail(detail)
        ),
        Some(("unavailable", detail)) => {
            format!("whirl could not reach wallhaven: {}", detail.trim_start())
        }
        Some(("malformed", detail)) => format!(
            "wallhaven answered something whirl could not read: {}",
            detail.trim_start()
        ),
        _ => message.to_string(),
    }
}

/// The API's own words after the daemon's status sentence, as a tail a phrase
/// can carry, or nothing when the API said nothing.
fn tail(detail: &str) -> String {
    let detail = detail.trim();
    match detail.is_empty() {
        true => String::new(),
        false => {
            // The daemon renders `answered <status>: <the API's error>`, so the
            // API's own words are what follows the status.
            match detail.split_once(": ") {
                Some((_, api)) => format!(" ({api})"),
                None => String::new(),
            }
        }
    }
}

/// What is known about a Wallhaven key, from the file's label and the store's
/// answer.
///
/// A file that names no label is the same case as one naming this app's label:
/// whirl looks the key up in the platform's own store, which is the item this
/// window writes and asks about. A file naming another label is not this app's
/// to resolve, and the store is not asked.
fn key_state(key_ref: Option<&str>, exists: Result<bool, String>) -> KeyState {
    match key_ref {
        Some(label) if label != keychain::LABEL => KeyState::NotOurs,
        _ => match exists {
            Ok(true) => KeyState::Saved,
            Ok(false) => KeyState::NeedsKey,
            Err(reason) => KeyState::CannotTell(reason),
        },
    }
}

/// The folders inside `directory`, in name order, hidden ones left out.
fn read_directories(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let listing = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut entries = Vec::new();
    for entry in listing.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            entries.push(path);
        }
    }
    entries.sort();
    entries.truncate(CHILD_LIMIT);
    Ok(entries)
}

/// The folder a chooser opens on when nothing better is known.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

/// A folder's own name, which is what an added source's id is derived from.
fn folder_name(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A name as an id the config schema accepts: `[A-Za-z0-9._:-]+`, at most 64
/// bytes, and never empty.
fn sanitize_id(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-') {
            out.push(character);
        } else {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    let out = if out.is_empty() {
        "folder".to_string()
    } else {
        out
    };
    out.chars().take(64).collect()
}

/// The config file the window would write: the one the daemon named in its
/// `config path` answer, and the platform default when it did not.
fn target_of(answers: &Answers) -> Option<Target> {
    answers
        .config_path
        .as_ref()
        .ok()
        .and_then(|lines| {
            lines
                .iter()
                .find_map(|line| Target::from_config_path_line(line))
        })
        .or_else(config_file::Target::default_path)
}

/// The daemon's own version, from the `daemon_version:` line of its `version`
/// answer, when it answered one.
///
/// The value is the daemon's, word for word, and the key name is not: the window
/// draws a version, never a config or protocol key.
fn daemon_version_of(answers: &Answers) -> Option<String> {
    let lines = answers.version.as_ref().ok()?;
    lines
        .iter()
        .find_map(|line| line.strip_prefix("daemon_version: "))
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "-")
        .map(str::to_string)
}

/// The rotation the daemon is using now, from the `plan:` line of its
/// `config check`, when it reported one.
///
/// This is what the daemon adopted, which is not necessarily what the file says:
/// a change written while the daemon runs is in the file and not in the daemon
/// until it reads the file again, and the two are shown as the two facts they
/// are.
fn plan_interval(answers: &Answers) -> Option<u64> {
    let check = answers.config_check.as_ref().ok()?;
    let plan = check.iter().find_map(|line| parse_plan_record(line))?;
    plan.iter()
        .find(|(name, _)| *name == INTERVAL_KEY)
        .and_then(|(_, value)| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store a test owns, so that a window built here never asks the machine's
    /// own keychain: it answers whatever the test says.
    struct Double(Result<bool, String>);

    impl Store for Double {
        fn holds_key(&self) -> Result<bool, String> {
            self.0.clone()
        }
    }

    /// A store holding no key.
    fn no_key() -> Double {
        Double(Ok(false))
    }

    /// A store holding a key, which is the state a collection request sends one
    /// in.
    fn a_key() -> Double {
        Double(Ok(true))
    }

    /// A store that records whether it was asked at all.
    #[derive(Default)]
    struct Recording {
        asked: std::cell::Cell<bool>,
        holds: bool,
    }

    impl Store for Recording {
        fn holds_key(&self) -> Result<bool, String> {
            self.asked.set(true);
            Ok(self.holds)
        }
    }

    /// A config the parser accepts, with a comment key to preserve.
    const CONFIG: &str = r#"{
  "_comment_1": "keep me",
  "config_schema": 1,
  "schedule": {"interval_seconds": 1800},
  "sources": [
    {"id": "pictures", "kind": "local", "paths": ["/tmp/walls"]},
    {"id": "space", "kind": "wallhaven", "api_key_ref": "keychain:whirl-wallhaven"}
  ]
}"#;

    /// A scratch directory of this test's own, with a config in it.
    fn scratch(tag: &str, text: &str) -> (PathBuf, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("whirlui-surface-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join("config.json");
        std::fs::write(&path, text).expect("a config file");
        (directory, path)
    }

    /// The window as it opens when the daemon answered, but on a file of our own.
    fn window_from_scratch(tag: &str, text: &str) -> (Settings, PathBuf) {
        let (_directory, path) = scratch(tag, text);
        let settings = Settings::from_answers_with(&settings_answers_for(&path), &no_key());
        (settings, path)
    }

    #[test]
    fn the_window_opens_on_the_files_own_sources_in_a_persons_words() {
        let (settings, _path) = window_from("opens", CONFIG);
        let lines: Vec<String> = settings.sources.rows.iter().map(Row::line).collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0], "A folder on this Mac: /tmp/walls");
        // The Wallhaven row says what the store answered and nothing else: a key
        // is saved, needs one, or cannot be asked about.
        assert!(
            lines[1].starts_with("Wallhaven, a remote collection: "),
            "{lines:?}"
        );
        // And no line of the window carries a key name or a file key.
        let text = settings.to_text();
        for forbidden in [
            "schedule",
            "interval_seconds",
            "api_key_ref",
            "weight=",
            "sources[",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} in:\n{text}");
        }
    }

    #[test]
    fn the_rotation_opens_on_the_files_value_in_a_unit_a_person_uses() {
        let (settings, _path) = window_from("rotation", CONFIG);
        assert_eq!(settings.interval.value, "30");
        assert_eq!(settings.interval.unit, Unit::Minutes);
        assert_eq!(settings.interval.in_use, Some(1800));
        let text = settings.to_text();
        assert!(text.contains("Every [30] [minutes] [Save]"), "{text}");
        // The one line the control carries, in the person's words.
        assert!(text.contains(ROTATION_LINE), "{text}");
        // Nothing on screen mentions the stored interval or its key.
        assert!(!text.contains("1800"), "{text}");
    }

    #[test]
    fn an_interval_the_file_stores_in_hours_reads_in_hours() {
        let minutes = Unit::count(1800);
        assert_eq!((minutes.as_str(), Unit::of(1800)), ("30", Unit::Minutes));
        assert_eq!(
            (Unit::count(21600).as_str(), Unit::of(21600)),
            ("6", Unit::Hours)
        );
        // A value that is not a whole number of minutes still reads as minutes
        // rather than being silently rounded into one.
        assert_eq!(Unit::count(90), "1.5");
        assert_eq!(Unit::count(61), "1.02");
    }

    #[test]
    fn a_change_to_the_rotation_lands_in_the_file_and_says_when_it_applies() {
        let (mut settings, path) = window_from("save", CONFIG);
        settings.interval.value = "15".to_string();
        settings.save_interval();

        assert_eq!(
            settings.interval.outcome,
            Some(Outcome::Saved {
                control: "Every 15 minutes".to_string(),
                detail: SAVED_FILE.to_string(),
            })
        );
        let line = settings
            .interval
            .outcome
            .as_ref()
            .expect("an outcome")
            .line();
        assert!(line.starts_with("Every 15 minutes: saved"), "{line}");
        assert!(
            line.contains("does not change until whirl next reads the file"),
            "{line}"
        );
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(landed.contains("\"interval_seconds\": 900"), "{landed}");
        assert!(landed.contains("\"_comment_1\""), "{landed}");
    }

    #[test]
    fn a_rotation_the_daemons_parser_refuses_says_so_beside_the_control() {
        let (mut settings, path) = window_from("refused", CONFIG);
        let before = std::fs::read(&path).expect("the file");
        // Half a minute is below the floor whirl-core enforces, so the parser
        // refuses the document and the control carries the parser's own words.
        settings.interval.value = "0.5".to_string();
        settings.save_interval();

        let line = settings
            .interval
            .outcome
            .as_ref()
            .expect("an outcome")
            .line();
        assert!(
            line.starts_with("Every 0.5 minutes: nothing was saved"),
            "{line}"
        );
        assert!(line.contains("less than 60"), "{line}");
        assert_eq!(std::fs::read(&path).expect("the file"), before);
        // The parse is whirl-core's, so its refusal is the sentence
        // `whirl config check` prints about the same document.
        assert!(line.contains("schedule.interval_seconds"), "{line}");
    }

    #[test]
    fn a_value_that_is_not_a_number_is_refused_before_the_file_is_touched() {
        let (mut settings, path) = window_from("not-a-number", CONFIG);
        let before = std::fs::read(&path).expect("the file");
        settings.interval.value = "half an hour".to_string();
        settings.save_interval();
        let line = settings
            .interval
            .outcome
            .as_ref()
            .expect("an outcome")
            .line();
        assert!(
            line.starts_with("Every half an hour: nothing was saved"),
            "{line}"
        );
        assert!(line.contains("that is not a number"), "{line}");
        // That refusal is the window's own, so it names no config key.
        assert!(!line.contains("schedule"), "{line}");
        assert_eq!(std::fs::read(&path).expect("the file"), before);
    }

    #[test]
    fn adding_a_folder_derives_an_id_and_puts_the_folder_in_the_file() {
        let (mut settings, path) = window_from("add-folder", CONFIG);
        settings.add_folder("/tmp/My Pictures");
        let rows = &settings.sources.rows;
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(
            rows[2].id, "My-Pictures",
            "the id came from the folder name"
        );
        assert_eq!(rows[2].line(), "A folder on this Mac: /tmp/My Pictures");
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(
            landed.contains("\"paths\": [") && landed.contains("My Pictures"),
            "{landed}"
        );
    }

    #[test]
    fn a_second_folder_of_the_same_name_gets_an_id_of_its_own() {
        let (mut settings, path) = window_from("unique-id", CONFIG);
        settings.add_folder("/somewhere/walls");
        // The derived id came from the folder's own name.
        assert_eq!(
            settings.sources.rows.last().map(|row| row.id.as_str()),
            Some("walls")
        );
        // A second folder with the same name is numbered rather than refused,
        // and it lands under an id of its own.
        settings.add_folder("/tmp/walls");
        assert_eq!(
            settings.sources.rows.last().map(|row| row.id.as_str()),
            Some("walls-2")
        );
        assert_eq!(
            settings
                .sources
                .rows
                .iter()
                .filter(|row| row.id == "pictures")
                .count(),
            1,
            "the file's own source is untouched"
        );
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(landed.contains("/somewhere/walls"), "{landed}");
        assert!(landed.contains("/tmp/walls"), "{landed}");
        assert!(
            settings
                .sources
                .outcome
                .as_ref()
                .is_some_and(|outcome| matches!(outcome, Outcome::Saved { .. })),
            "{:?}",
            settings.sources.outcome
        );
    }

    #[test]
    fn asking_for_a_wallhaven_collection_lands_the_address_and_then_offers_the_key() {
        let (mut settings, path) = window_from("add-wallhaven", CONFIG);
        // The button opens the field and writes nothing yet: a source with no
        // collection is what this field exists to prevent.
        settings.ask_for_collection();
        assert!(settings.collection.open, "the field opens on the button");
        assert!(settings.collection.for_source.is_none());
        assert!(
            !settings.key.open,
            "nothing is written until the address is"
        );

        settings.collection.url = "https://wallhaven.cc/user/alice/favorites/12345".to_string();
        settings.save_collection();

        assert!(
            !settings.collection.open,
            "the field closes once the address lands"
        );
        assert!(settings.key.open, "the optional key is offered next");
        let row = settings.sources.rows.last().expect("the new source");
        assert_eq!(row.id, "wallhaven", "the id is derived from the kind");
        assert_eq!(row.collection_url(), Some("alice/12345"));
        // The pair is the row's description, so what was written is visible.
        // Whether a key is saved is a fact about this machine, so the key half
        // is asserted only up to where the two answers differ.
        assert!(
            row.line()
                .starts_with("Wallhaven, a remote collection: alice/12345 ("),
            "{}",
            row.line()
        );
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(
            landed.contains(r#""collection": "alice/12345""#),
            "{landed}"
        );
        assert!(
            landed.contains(&format!("\"api_key_ref\": \"{}\"", keychain::LABEL)),
            "{landed}"
        );
        // The address that reached the file is the daemon's own value, not the
        // URL the person pasted.
        assert!(!landed.contains("wallhaven.cc"), "{landed}");
    }

    #[test]
    fn an_address_that_is_not_a_collection_is_refused_beside_the_field() {
        let (mut settings, path) = window_from("bad-collection", CONFIG);
        let before = std::fs::read(&path).expect("the file");
        settings.ask_for_collection();
        settings.collection.url = "https://wallhaven.cc/search?q=nebula".to_string();
        settings.save_collection();

        assert!(settings.collection.open, "the field stays open to be fixed");
        let problem = settings
            .collection
            .problem
            .clone()
            .expect("a reason beside the field");
        // The one sentence names every accepted form.
        for form in [
            "https://wallhaven.cc/user/<username>/favorites/<id>",
            "https://wallhaven.cc/api/v1/collections/<username>/<id>",
            "<username>/<id>",
        ] {
            assert!(problem.contains(form), "{problem}");
        }
        let line = settings
            .sources
            .outcome
            .as_ref()
            .expect("an outcome")
            .line();
        assert!(
            line.starts_with("Add Wallhaven: nothing was saved"),
            "{line}"
        );
        // Nothing was written: no source was added and the file is as it was.
        assert_eq!(
            settings.sources.rows.len(),
            2,
            "{:?}",
            settings.sources.rows
        );
        assert_eq!(std::fs::read(&path).expect("the file"), before);
        let text = settings.to_text();
        assert!(text.contains(COLLECTION_TOKEN_NOTE), "{text}");
    }

    #[test]
    fn an_existing_collection_address_can_be_changed_and_the_key_survives() {
        let (mut settings, path) = window_from("change-url", CONFIG);
        // `space` names a key and no collection yet: the field opens empty.
        settings.edit_collection("space");
        assert!(settings.collection.open);
        assert_eq!(settings.collection.for_source.as_deref(), Some("space"));
        assert_eq!(settings.collection.url, "");

        settings.collection.url = "alice/999".to_string();
        settings.save_collection();

        assert_eq!(settings.sources.rows[1].collection_url(), Some("alice/999"));
        assert!(
            settings.sources.rows[1]
                .line()
                .starts_with("Wallhaven, a remote collection: alice/999 ("),
            "{}",
            settings.sources.rows[1].line()
        );
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(landed.contains(r#""collection": "alice/999""#), "{landed}");
        // Every key the edit did not own is exactly as it was.
        assert!(
            landed.contains(&format!("\"api_key_ref\": \"{}\"", keychain::LABEL)),
            "{landed}"
        );
        assert!(landed.contains("\"_comment_1\""), "{landed}");
        assert!(landed.contains("\"/tmp/walls\""), "{landed}");

        // Editing again opens on the address the file now holds, so it is
        // changed rather than retyped from memory.
        settings.edit_collection("space");
        assert_eq!(settings.collection.url, "alice/999");
        settings.cancel_collection();
        assert!(!settings.collection.open);
        assert_eq!(
            std::fs::read_to_string(&path).expect("the file"),
            landed,
            "a cancel writes nothing"
        );
    }

    #[test]
    fn changing_a_folder_points_the_source_at_the_chosen_one() {
        let (mut settings, path) = window_from("change-folder", CONFIG);
        settings.set_source_folder("pictures", "/tmp/holiday");
        assert_eq!(
            settings.sources.rows[0].line(),
            "A folder on this Mac: /tmp/holiday"
        );
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(landed.contains("/tmp/holiday"), "{landed}");
        assert!(!landed.contains("/tmp/walls"), "{landed}");
    }

    #[test]
    fn a_refused_source_edit_keeps_the_rows_and_puts_the_reason_in_the_line() {
        let (mut settings, path) = window_from("source-refused", CONFIG);
        let rows = settings.sources.rows.clone();
        let before = std::fs::read(&path).expect("the file");
        // The id is taken and this path names it rather than deriving one, so
        // whirl-core's parser refuses the document.
        settings.add_folder_as("pictures", "/tmp/other");
        assert_eq!(settings.sources.rows, rows);
        let line = settings
            .sources
            .outcome
            .as_ref()
            .expect("an outcome")
            .line();
        assert!(
            line.starts_with("Add a folder: nothing was saved"),
            "{line}"
        );
        assert!(line.contains("duplicate source id"), "{line}");
        assert_eq!(std::fs::read(&path).expect("the file"), before);
    }

    #[test]
    fn the_chooser_lists_the_folders_inside_one_and_can_descend() {
        let (directory, path) = scratch("picker", CONFIG);
        let inside = directory.join("Pictures");
        std::fs::create_dir_all(inside.join("walls")).expect("a folder to choose");
        std::fs::create_dir_all(inside.join("holiday")).expect("another");
        std::fs::write(inside.join("not-a-folder.txt"), "x").expect("a file, not a folder");

        let mut settings = Settings::from_answers_with(
            &Answers::live(
                vec![format!("config: {}", path.display())],
                Vec::new(),
                Vec::new(),
            ),
            &no_key(),
        );
        settings.open_picker(None);
        settings.picker_into(inside.clone());

        let picker = settings.picker.clone().expect("an open chooser");
        assert_eq!(picker.directory, inside);
        let names: Vec<String> = picker
            .entries
            .iter()
            .map(|entry| entry.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // Folders only, in name order, and the file beside them is not one.
        assert_eq!(names, vec!["holiday", "walls"], "{names:?}");

        // Choosing the folder it is showing adds it as a new source.
        settings.picker_choose();
        assert!(settings.picker.is_none(), "the chooser closes on a choice");
        assert!(
            settings
                .sources
                .rows
                .iter()
                .any(|row| row.line().contains("Pictures")),
            "{:?}",
            settings.sources.rows
        );
    }

    #[test]
    fn the_chooser_opens_on_the_folder_a_source_already_reads() {
        let (directory, path) = scratch("picker-from", CONFIG);
        let folder = directory.join("walls");
        std::fs::create_dir_all(&folder).expect("the folder the source reads");
        // The folder goes into a JSON string, so it is written with forward
        // slashes: a backslash would have to be escaped, and a platform that
        // separates with one reads a forward slash too. The window hands the
        // path to the picker unchanged, which is what the comparison below is
        // about, so both sides are canonicalized: the platform, not this test,
        // decides how one directory is spelled.
        let written = folder.to_str().expect("a path").replace('\\', "/");
        std::fs::write(&path, CONFIG.replace("/tmp/walls", &written)).expect("the fixture");

        let mut settings = Settings::from_answers_with(
            &Answers::live(
                vec![format!("config: {}", path.display())],
                Vec::new(),
                Vec::new(),
            ),
            &no_key(),
        );
        settings.open_picker(Some("pictures".to_string()));
        let shown = settings
            .picker
            .as_ref()
            .map(|picker| picker.directory.canonicalize().expect("the folder"));
        assert_eq!(shown, Some(folder.canonicalize().expect("the folder")));
    }

    /// A panel that answers what a test tells it, without opening anything.
    ///
    /// The seam's whole point: the window asks a [`FolderPanel`], and a test can
    /// be one.
    struct Answering(PanelAnswer);

    impl FolderPanel for Answering {
        fn ask(&self) -> PanelAnswer {
            self.0.clone()
        }
    }

    #[test]
    fn a_chosen_folder_lands_the_write_the_button_makes() {
        // The panel is the way in now, and what it writes has to be the write the
        // control already made: the same folder chosen through the panel and
        // passed to `add_folder` must leave the same row and the same file.
        let folder = "/tmp/My Pictures";
        let (mut via_panel, panel_path) = window_from("choose-panel", CONFIG);
        let (mut via_button, button_path) = window_from("choose-button", CONFIG);

        via_panel.choose_folder(None, &Answering(PanelAnswer::Chosen(PathBuf::from(folder))));
        via_button.add_folder(folder);

        assert_eq!(
            via_panel.sources.rows, via_button.sources.rows,
            "the row the panel's choice produces"
        );
        assert!(
            via_panel
                .sources
                .rows
                .iter()
                .any(|row| row.line().contains(folder)),
            "{:?}",
            via_panel.sources.rows
        );
        assert_eq!(
            std::fs::read_to_string(&panel_path).expect("the file"),
            std::fs::read_to_string(&button_path).expect("the file"),
            "the file the panel's choice writes"
        );
    }

    #[test]
    fn a_cancelled_panel_writes_nothing() {
        let (mut settings, path) = window_from("choose-cancelled", CONFIG);
        let before = std::fs::read(&path).expect("the file");
        let rows = settings.sources.rows.clone();

        settings.choose_folder(None, &Answering(PanelAnswer::Cancelled));

        assert_eq!(
            std::fs::read(&path).expect("the file"),
            before,
            "a cancellation wrote the file"
        );
        assert_eq!(settings.sources.rows, rows, "a cancellation changed a row");
        assert!(
            settings.sources.outcome.is_none(),
            "a cancellation claimed an outcome: {:?}",
            settings.sources.outcome
        );
    }

    #[test]
    fn a_panel_that_cannot_be_shown_falls_back_to_the_drawn_browser() {
        // No AppKit, no GUI, or a panel that would not present: the click reaches
        // the drawn browser the app always had, and it stays the documented
        // behaviour for those runs (see `Settings::choose_folder`).
        let (mut settings, path) = window_from("choose-fallback", CONFIG);
        let before = std::fs::read(&path).expect("the file");

        settings.choose_folder(None, &Answering(PanelAnswer::Unavailable));

        assert!(settings.picker.is_some(), "the drawn browser did not open");
        assert!(
            settings.to_text().contains(PICKER_TITLE),
            "{}",
            settings.to_text()
        );
        assert_eq!(
            std::fs::read(&path).expect("the file"),
            before,
            "opening the fallback wrote the file"
        );
    }

    #[test]
    fn a_folder_of_several_is_described_as_several_and_is_not_offered_for_a_change() {
        let text = CONFIG.replace(
            "\"paths\": [\"/tmp/walls\"]",
            "\"paths\": [\"/tmp/walls\", \"/tmp/more\"]",
        );
        let (settings, _path) = window_from("several", &text);
        let row = &settings.sources.rows[0];
        assert_eq!(row.line(), "A folder on this Mac: /tmp/walls (and 1 more)");
        assert_eq!(row.changeable_folder(), None);
    }

    #[test]
    fn the_window_with_no_config_and_no_daemon_says_both_reasons() {
        let reason = "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)";
        let mut settings = Settings::from_answers_with(&Answers::unreachable(reason), &no_key());
        // The state a window on a machine with neither a daemon nor a locatable
        // config file is in: nothing was read, so nothing is shown for it.
        settings.target = None;
        settings.sources = Sources {
            rows: Vec::new(),
            problem: Some(config_file::NO_PATH.to_string()),
            outcome: None,
            checks: Checks::default(),
        };
        settings.interval = Interval::opening(None);

        let text = settings.to_text();
        assert!(
            text.contains(&format!("whirl is not running: {reason}")),
            "{text}"
        );
        // The reason names the socket file and no directory, and the window adds
        // no path of its own.
        assert!(!text.contains("/Users"), "{text}");
        assert!(!text.contains("reason="), "{text}");
        // A window with nothing to show still offers the controls, and says why
        // the list is empty rather than that there are no sources.
        assert!(text.contains(config_file::NO_PATH), "{text}");
        assert!(text.contains("[Add a folder…] [Add Wallhaven]"), "{text}");
        assert!(text.contains("Every [] [minutes] [Save]"), "{text}");
        // Nothing on screen came from the machine's own config file: the window
        // shows what it was given and no more.
        assert!(!text.contains("A folder on this Mac"), "{text}");
        assert!(!text.contains("Wallhaven, a remote collection"), "{text}");
    }

    /// The window's one daemon line carries the paused state, which is the same
    /// bit the menu bar item's mark reads.
    #[test]
    fn the_windows_line_carries_the_paused_state_and_still_says_not_running() {
        assert_eq!(Daemon::Running.line(), "whirl is running");
        assert_eq!(Daemon::Paused.line(), "whirl is paused");
        assert_eq!(
            Daemon::NotRunning("the daemon stopped answering".to_string()).line(),
            "whirl is not running: the daemon stopped answering"
        );
        // A paused daemon answered, so it is not the not-running form: only a
        // daemon that said nothing at all is.
        assert!(Daemon::Running.answered());
        assert!(Daemon::Paused.answered());
        assert!(!Daemon::NotRunning("gone".to_string()).answered());
    }

    /// The paused line reaches the text the window prints, and the About block
    /// says the same word rather than its `connected` form.
    #[test]
    fn the_window_text_carries_the_paused_line_and_the_about_block_agrees() {
        let (mut settings, _path) = window_from("paused-line", CONFIG);
        let running = settings.to_text();
        assert!(running.contains("whirl is running"), "{running}");
        assert!(
            running.contains("the daemon: whirl 0.1.0, connected"),
            "{running}"
        );

        settings.daemon = Daemon::Paused;
        let text = settings.to_text();
        assert!(text.contains("whirl is paused"), "{text}");
        assert!(!text.contains("whirl is running"), "{text}");
        assert!(text.contains("the daemon: whirl 0.1.0, paused"), "{text}");
        assert!(
            !text.contains("the daemon: whirl 0.1.0, connected"),
            "{text}"
        );
    }

    #[test]
    fn a_config_file_that_cannot_be_read_says_so_instead_of_showing_no_sources() {
        let (directory, _path) = scratch("missing", CONFIG);
        let missing = directory.join("absent.json");
        let settings = Settings::from_answers_with(&settings_answers_for(&missing), &no_key());
        let text = settings.to_text();
        assert!(settings.sources.rows.is_empty());
        assert!(text.contains("absent.json"), "{text}");
        assert!(
            !text.contains(NO_SOURCES),
            "a missing file is not an empty list: {text}"
        );
    }

    #[test]
    fn a_token_typed_into_the_window_reaches_no_line_and_not_the_text() {
        let fake = "fake-token-not-a-real-key";
        let mut settings =
            Settings::from_answers_with(&Answers::unreachable("no daemon"), &no_key());
        settings.key.open = true;
        settings.key.token = fake.to_string();
        assert!(
            !settings.to_text().contains("fake-token"),
            "{}",
            settings.to_text()
        );
        assert!(!settings.to_text().contains(fake), "{}", settings.to_text());
    }

    #[test]
    fn the_controls_that_edit_are_the_ones_a_person_asked_for() {
        let (settings, _path) = window_from("controls", CONFIG);
        let text = settings.to_text();
        // The two choices, the daemon's line, and the About block, in the order
        // the sidebar lists them. Nothing else is on screen.
        let order: Vec<usize> = [SOURCES_TITLE, ROTATION_TITLE, ABOUT_TITLE]
            .iter()
            .map(|title| text.find(title).unwrap_or_else(|| panic!("{title}")))
            .collect();
        assert!(order[0] < order[1], "{text}");
        assert!(order[1] < order[2], "{text}");
        // The daemon's line says where a change lands, and the About block is
        // the last thing on screen: the check has not been made yet, so the
        // check's own line is last.
        assert!(text.contains(APP_DAEMON_NOTE), "{text}");
        assert!(text.trim_end().ends_with(CHECK_LINE), "{text}");
        assert!(
            text.find("whirl is running").expect("the daemon's line")
                < text.find(ABOUT_TITLE).expect("the About block"),
            "{text}"
        );
        assert!(text.contains(CHECK_LABEL), "{text}");
        // No section dumps the rest of the file: the App pane's report is gone.
        for gone in [
            "socket: live",
            "daemon_version",
            "protocol: 2",
            "config check",
        ] {
            assert!(!text.contains(gone), "{gone} in:\n{text}");
        }
    }

    /// The About block carries what the card asks a report to be able to paste:
    /// the description, the version, the source, the daemon, and the check.
    #[test]
    fn the_about_block_carries_the_description_the_source_and_the_daemon() {
        let (settings, _path) = window_from("about", CONFIG);
        let text = settings.to_text();
        assert!(text.contains(ABOUT_LINE), "{text}");
        assert!(text.contains(about::SOURCE_URL), "{text}");
        // The daemon answered `version`, so its own version is on screen, and
        // the key name it arrived under is not.
        assert!(
            text.contains("the daemon: whirl 0.1.0, connected"),
            "{text}"
        );
        assert!(!text.contains("daemon_version"), "{text}");
        // The source is a sentence, not a `source:` record a reader could take
        // for one of the daemon's.
        assert!(!text.contains("source: "), "{text}");
        // No check has been run, so no outcome is on screen.
        assert!(settings.check.is_none(), "{:?}", settings.check);
    }

    /// The version is the bundle's, and the binary's own stands beside it only
    /// when the two disagree: a build installed over another is exactly that
    /// disagreement, and showing one of the two would hide it.
    #[test]
    fn the_version_is_the_bundles_and_the_binarys_own_when_they_differ() {
        let (mut settings, _path) = window_from("version", CONFIG);
        let binary = about::binary_version().to_string();

        settings.bundle_version = None;
        assert_eq!(
            settings.version_lines(),
            vec![format!(
                "version {binary} (this binary; not running from an app bundle)"
            )]
        );

        settings.bundle_version = Some(binary.clone());
        assert_eq!(settings.version_lines(), vec![format!("version {binary}")]);

        settings.bundle_version = Some("9.9.9".to_string());
        assert_eq!(
            settings.version_lines(),
            vec![
                "version 9.9.9 (the app bundle)".to_string(),
                format!("version {binary} (this binary)"),
            ]
        );
    }

    #[test]
    fn the_interval_the_daemon_is_using_is_named_beside_the_one_the_file_asks_for() {
        let (mut settings, _path) = window_from("in-use", CONFIG);
        settings.interval.in_use = Some(21600);
        let text = settings.to_text();
        assert!(text.contains("whirl is using every 6 hours now"), "{text}");
    }

    // A helper the tests above share: the window on a file of our own, built the
    // way the daemon's answer builds it.
    fn window_from(tag: &str, text: &str) -> (Settings, PathBuf) {
        window_from_scratch(tag, text)
    }

    /// The answers a daemon would give about `path`, so the window opens on the
    /// file of our own rather than on the machine's.
    fn settings_answers_for(path: &Path) -> Answers {
        Answers::live(
            vec![format!("config: {}", path.display())],
            vec!["plan: schedule.interval_seconds=1800 display.mode=all backend=noop".to_string()],
            vec![
                "daemon_version: whirl 0.1.0".to_string(),
                "protocol: 2".to_string(),
                "platform: macos".to_string(),
            ],
        )
    }

    // -- what the panel says about a source ---------------------------------
    //
    // Every collection state below is an `Answers` value built from the daemon's
    // own record lines, so no test here opens a socket: `Answers` is the double,
    // and the only code that can produce a live one is `crate::dump`, which no
    // test in this module calls. The key states come from a store a test hands
    // in, so the machine's keychain is never asked either.

    /// A file with one folder and one collection: the two kinds of row.
    const TWO: &str = r#"{
  "config_schema": 1,
  "sources": [
    {"id": "pictures", "kind": "local", "paths": ["/tmp/walls"]},
    {"id": "space", "kind": "wallhaven", "collection": "alice/12345", "api_key_ref": "keychain:whirl-wallhaven"}
  ]
}"#;

    /// A file with one folder and no collection, so the store is never asked.
    const ONE_FOLDER: &str = r#"{
  "config_schema": 1,
  "sources": [{"id": "pictures", "kind": "local", "paths": ["/tmp/walls"]}]
}"#;

    /// A `source:` record as the daemon sends one, in the two shapes 2.6 has: a
    /// source it could check (the counter group and `reason=-`) and one it could
    /// not (`enabled=0` with the reason and no group).
    ///
    /// Both shapes are the daemon's own: the first is pinned byte for byte by
    /// `crates/whirld/tests/control_socket.rs`'s `config_check_lines`, and the
    /// second is what `crates/whirl-worker/src/pipeline.rs`'s `disabled_record`
    /// writes.
    fn record(id: &str, kind: &str, candidates: u64, reason: Option<&str>) -> String {
        match reason {
            None => format!(
                "source: {id} {kind} weight=1 enabled=1 last=- candidates={candidates} \
                 admitted=0 rejected_resolution=0 rejected_ratio=0 rejected_size=0 \
                 rejected_type=0 rejected_dedupe=0 reason=-"
            ),
            Some(reason) => {
                format!("source: {id} {kind} weight=1 enabled=0 last=- reason={reason}")
            }
        }
    }

    /// The words the window shows for the source the file names `id`, with the
    /// daemon's answer being `check`.
    fn state_of(
        tag: &str,
        config: &str,
        check: Vec<String>,
        id: &str,
        store: &dyn Store,
    ) -> String {
        let (_directory, path) = scratch(tag, config);
        let answers = Answers::live(
            vec![format!("config: {}", path.display())],
            check,
            Vec::new(),
        );
        let settings = Settings::from_answers_with(&answers, store);
        settings
            .sources
            .rows
            .iter()
            .find(|row| row.id == id)
            .expect("the row of the source the file names")
            .state_line(&settings.sources.checks)
            .expect("every row says what is known about its source")
            .phrase
    }

    #[test]
    fn a_folder_that_holds_pictures_says_how_many() {
        let words = state_of(
            "holds",
            TWO,
            vec![record("pictures", "local", 412, None)],
            "pictures",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl can read this folder, and it holds 412 pictures"
        );
    }

    #[test]
    fn a_folder_that_holds_nothing_says_so() {
        let words = state_of(
            "empty",
            TWO,
            vec![record("pictures", "local", 0, None)],
            "pictures",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl can read this folder, and it holds no pictures"
        );
    }

    #[test]
    fn a_folder_that_is_not_there_says_so() {
        // `Local::validate`'s reason, word for word: the key it refuses, then the
        // operating system's own words for the failure.
        let reason = "sources[id=pictures].paths (line 4): no configured path can be read: \
                      /tmp/walls: No such file or directory (os error 2)";
        let words = state_of(
            "gone",
            TWO,
            vec![record("pictures", "local", 0, Some(reason))],
            "pictures",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this folder: the folder is not there; check the path, or remove this \
             row"
        );
    }

    #[test]
    fn a_folder_that_cannot_be_read_says_so_separately_from_one_that_is_missing() {
        let reason = "sources[id=pictures].paths (line 4): no configured path can be read: \
                      /tmp/walls: Permission denied (os error 13)";
        let words = state_of(
            "locked",
            TWO,
            vec![record("pictures", "local", 0, Some(reason))],
            "pictures",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this folder: the folder is there and cannot be read: permission \
             denied"
        );
    }

    #[test]
    fn a_collection_that_answers_without_a_key_is_public_and_says_how_many_it_holds() {
        let words = state_of(
            "public",
            TWO,
            vec![record("space", "wallhaven", 97, None)],
            "space",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl can read this collection, and it holds 97 wallpapers without a key, so it is \
             public"
        );
    }

    #[test]
    fn a_collection_that_answers_with_a_saved_key_says_the_key_is_in_use() {
        let words = state_of(
            "with-key",
            TWO,
            vec![record("space", "wallhaven", 97, None)],
            "space",
            &a_key(),
        );
        assert_eq!(
            words,
            "whirl can read this collection, and it holds 97 wallpapers with the saved key"
        );
    }

    #[test]
    fn a_collection_that_does_not_answer_says_it_could_not_be_reached() {
        // `http.rs`'s own words for a request that got nothing back.
        let reason = "unavailable: no response from \
                      https://wallhaven.cc/api/v1/collections/alice/12345?page=1: curl: (6) Could \
                      not resolve host: wallhaven.cc";
        let words = state_of(
            "offline",
            TWO,
            vec![record("space", "wallhaven", 0, Some(reason))],
            "space",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this collection: whirl could not reach wallhaven: no response from \
             https://wallhaven.cc/api/v1/collections/alice/12345?page=1: curl: (6) Could not \
             resolve host: wallhaven.cc"
        );
    }

    #[test]
    fn a_private_collection_with_no_key_says_a_key_is_what_it_needs() {
        let reason = "not_found: https://wallhaven.cc/api/v1/collections/alice/12345 answered \
                      404: Nothing here";
        let words = state_of(
            "private",
            TWO,
            vec![record("space", "wallhaven", 0, Some(reason))],
            "space",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this collection: wallhaven answered 404 without a key: the address \
             may name no collection, and a private one needs a key (Nothing here)"
        );
    }

    #[test]
    fn a_collection_whose_saved_key_was_refused_says_so() {
        let reason = "unauthorized: https://wallhaven.cc/api/v1/collections/alice/12345 answered \
                      401: Unauthorized";
        let words = state_of(
            "refused",
            TWO,
            vec![record("space", "wallhaven", 0, Some(reason))],
            "space",
            &a_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this collection: the saved key was refused: wallhaven answered 401 \
             (Unauthorized)"
        );
    }

    #[test]
    fn a_collection_whose_purity_needs_a_key_says_a_key_is_what_it_needs() {
        // `Wallhaven::refuse`'s reason for a purity only a key can ask for (2.4).
        let reason = "sources[id=space].purity (line 3): purity=111 requires an API key, none \
                      resolvable (checked env WHIRL_WALLHAVEN_API_KEY, keychain label \
                      'whirl-wallhaven')";
        let words = state_of(
            "sketchy",
            TWO,
            vec![record("space", "wallhaven", 0, Some(reason))],
            "space",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl cannot use this collection: it needs a key, and none is saved: use Enter key… \
             to add one"
        );
    }

    #[test]
    fn a_source_added_since_the_check_says_it_has_not_been_checked() {
        // The check landed and named the folder, and the collection was added
        // after it: the window must not read that as "the collection is fine".
        let words = state_of(
            "new",
            TWO,
            vec![record("pictures", "local", 412, None)],
            "space",
            &no_key(),
        );
        assert_eq!(
            words,
            "whirl has not checked this collection yet: it was added or changed after the check, \
             and the next one covers it"
        );
    }

    #[test]
    fn a_daemon_that_did_not_answer_leaves_every_row_saying_so() {
        let reason = "the daemon is not reachable: /tmp/whirl.sock (absent): No such file or \
                      directory (os error 2)";
        let (_directory, _path) = scratch("absent", TWO);
        let settings = Settings::from_answers_with(&Answers::unreachable(reason), &no_key());
        for row in &settings.sources.rows {
            let state = row
                .state_line(&settings.sources.checks)
                .expect("a row says what is known");
            // The row's own wrapper, and the client's own reason inside it.
            assert!(
                state.phrase.starts_with("whirl has not checked this ")
                    && state.phrase.ends_with(reason),
                "{:?}",
                state.phrase
            );
            assert_eq!(state.tone, Tone::Unknown);
        }
    }

    #[test]
    fn a_slow_daemon_leaves_the_row_saying_the_check_ran_out_of_time() {
        // The client's own reason for a socket read that ran out of time: the
        // timeout is the socket's, so the words are the platform's io error
        // (`crates/whirlui-client/src/error.rs`: a timeout is `ClientError::Io`
        // and prints as the io error does). Left as the io error, that sentence
        // is not one a person can act on, which is why the window says it in its
        // own words.
        let (_directory, path) = scratch("timeout", TWO);
        let answers = Answers {
            connection: Ok(()),
            config_path: Ok(vec![format!("config: {}", path.display())]),
            config_check: Err("Resource temporarily unavailable (os error 35)".to_string()),
            version: Ok(Vec::new()),
        };
        let settings = Settings::from_answers_with(&answers, &no_key());
        let state = settings.sources.rows[0]
            .state_line(&settings.sources.checks)
            .expect("a row says what is known");
        assert_eq!(
            state.phrase,
            "whirl has not checked this folder yet: the daemon did not answer in time, and a \
             check can take minutes; open this window again in a moment"
        );
        assert_eq!(state.tone, Tone::Unknown);
    }

    #[test]
    fn the_window_shows_the_answer_under_the_row_it_is_about() {
        // The panel's own arrangement, at the level the text dump can see: the
        // answer sits under its row and is indented to it, so a person reading
        // the window or the dump has the two together.
        let (_directory, path) = scratch("under", TWO);
        let answers = Answers::live(
            vec![format!("config: {}", path.display())],
            vec![
                record("pictures", "local", 7, None),
                record("space", "wallhaven", 3, None),
            ],
            Vec::new(),
        );
        let text = Settings::from_answers_with(&answers, &no_key()).to_text();
        let lines: Vec<&str> = text.lines().collect();
        let row = lines
            .iter()
            .position(|line| line.contains("A folder on this Mac: /tmp/walls"))
            .expect("the folder's row");
        let under = lines[row + 1];
        assert!(
            under
                .trim_start()
                .starts_with("whirl can read this folder, and it holds 7"),
            "{under:?}"
        );
        assert!(under.starts_with("      "), "under its row: {under:?}");
    }

    #[test]
    fn a_file_with_no_collection_never_reaches_the_store() {
        // The store costs a process, so it is asked only when a Wallhaven row is
        // on screen. The double here fails the test if it is asked at all, which
        // is also what keeps the machine's own keychain out of every test in this
        // module that has no collection in it.
        let store = Recording::default();
        let (_directory, path) = scratch("folder-only", ONE_FOLDER);
        let settings = Settings::from_answers_with(&settings_answers_for(&path), &store);
        assert!(
            !store.asked.get(),
            "the store is asked only for a Wallhaven row"
        );
        assert_eq!(settings.sources.rows.len(), 1);
    }
}
