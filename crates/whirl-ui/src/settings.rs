//! The settings window's content: two things a person sets, in their own words.
//!
//! The window shows exactly two choices and one line of state:
//!
//! - **where the wallpapers come from** ([`Sources`]): one row per source, a
//!   folder of the person's own pictures or Wallhaven's remote collection,
//!   described in those words rather than in the schema's;
//! - **how often they change** ([`Interval`]): a number and a unit, never the
//!   raw interval the file stores and never a key name;
//! - **whether whirl answered** ([`Daemon`]), one line, so nothing on screen
//!   reads as live while nothing is listening.
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

use whirlui_client::protocol::parse_plan_record;

use crate::config_file::{self, FileSource, INTERVAL_KEY, Target};
use crate::keychain;

/// The window's title, and the app name eframe registers.
pub const WINDOW_TITLE: &str = "whirl settings";

/// How big the window opens. Fixed so that a screenshot of it has a size a
/// reader can check.
pub const WINDOW_SIZE: [f32; 2] = [860.0, 620.0];

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

/// The line under the daemon's: where a change is actually written.
///
/// The window edits the config file and never the running daemon, and a person
/// who expects the wallpaper to change the moment they press Save deserves the
/// one sentence that says it will not until whirl reads the file again.
pub const APP_DAEMON_NOTE: &str =
    "changes are written to the config file; whirl picks them up the next time it reads it";

/// The folder chooser's title.
pub const PICKER_TITLE: &str = "Choose a folder";

/// What a saved edit says about itself, for the edits that land in the file.
pub const SAVED_FILE: &str = "the config file now says it, and the wallpaper on screen does not change until whirl next reads the file";

/// What a saved key says about itself.
pub const SAVED_KEY: &str = "the key is in your system's password store and the config file only names it, so the key itself is in no file whirl reads";

/// The one source kind with a secret behind it, in whirl's own spelling.
const WALLHAVEN: &str = "wallhaven";

/// The most sub-folders the chooser lists at once.
const CHILD_LIMIT: usize = 200;

/// The window: where the wallpapers come from, how often they change, and the
/// config file both are written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The config file an edit writes, when one could be located.
    pub target: Option<Target>,
    /// Whether whirl answered.
    pub daemon: Daemon,
    /// Where the wallpapers come from.
    pub sources: Sources,
    /// How often they change.
    pub interval: Interval,
    /// The folder chooser, while one is open.
    pub picker: Option<Picker>,
    /// The Wallhaven key field, while it is open.
    pub key: KeyField,
}

/// Whether whirl answered, as the one line the window carries about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    /// The socket answered.
    Running,
    /// The socket did not answer, in the client's own words.
    NotRunning(String),
}

impl Daemon {
    /// The line the window carries.
    pub fn line(&self) -> String {
        match self {
            Daemon::Running => "whirl is running".to_string(),
            Daemon::NotRunning(reason) => format!("whirl is not running: {reason}"),
        }
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
            Kind::Wallhaven { key } => {
                format!("Wallhaven, a remote collection: {}", key.phrase())
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
}

impl Answers {
    /// The answers of a live conversation.
    pub fn live(config_path: Vec<String>, config_check: Vec<String>) -> Answers {
        Answers {
            connection: Ok(()),
            config_path: Ok(config_path),
            config_check: Ok(config_check),
        }
    }

    /// Every answer missing, because the daemon could not be reached.
    pub fn unreachable(reason: &str) -> Answers {
        Answers {
            connection: Err(reason.to_string()),
            config_path: Err(reason.to_string()),
            config_check: Err(reason.to_string()),
        }
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
    /// supplies the rotation it is using now, which the window cannot see in the
    /// file. With no readable file the fields are empty and the reason is on
    /// screen rather than being papered over with a default.
    pub fn from_answers(answers: &Answers) -> Settings {
        let target = target_of(answers);
        let read = target
            .as_ref()
            .map(|target| config_file::read_file(&target.path));
        let (rows, problem, stored) = match &read {
            None => (Vec::new(), Some(config_file::NO_PATH.to_string()), None),
            Some(Ok(state)) => (rows_of(&state.sources), None, Some(state.interval_seconds)),
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
            sources: Sources {
                rows,
                problem,
                outcome: None,
            },
            interval,
            picker: None,
            key: KeyField::default(),
        }
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
        self.record_sources("Add a folder", SAVED_FILE, result);
    }

    /// Add a Wallhaven source, and open the field for its key.
    ///
    /// The source lands first and the key second, so a store that refuses leaves
    /// the source on screen reading as one that needs a key rather than losing
    /// the person's place.
    pub fn add_wallhaven(&mut self) {
        let id = self.free_id(WALLHAVEN);
        self.add_wallhaven_as(&id);
    }

    /// Add a Wallhaven source under an id the caller names, and open the key
    /// field when it landed.
    pub fn add_wallhaven_as(&mut self, id: &str) {
        let document = config_file::wallhaven_source(id, keychain::LABEL);
        let result = self
            .writing_target()
            .and_then(|path| config_file::add_source(path, document));
        let saved = result.is_ok();
        self.record_sources("Wallhaven", SAVED_FILE, result);
        if saved {
            self.key.open = true;
        }
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
        self.record_sources("Change folder", SAVED_FILE, result);
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
        self.record_sources("Wallhaven key", SAVED_KEY, result);
        if saved {
            self.key.token.clear();
            self.key.open = false;
        }
    }

    /// Open the folder chooser, for one source or for a new one.
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
                self.sources.rows = rows_of(&written.sources);
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
                    let mut buttons = Vec::new();
                    if row.changeable_folder().is_some() {
                        buttons.push("Change…");
                    }
                    if matches!(row.kind, Kind::Wallhaven { .. }) {
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
        out
    }
}

/// The rows for a file's sources, each Wallhaven one beside what the store says.
///
/// The store is asked once, and only when a Wallhaven row is on screen, because
/// the question costs a process; a source that names a key this app does not
/// manage is not this app's to resolve and the store is not asked about it.
fn rows_of(sources: &[FileSource]) -> Vec<Row> {
    let mut store: Option<Result<bool, String>> = None;
    let mut rows = Vec::with_capacity(sources.len());
    for source in sources {
        let kind = if source.kind == WALLHAVEN {
            let exists = store
                .get_or_insert_with(|| keychain::exists().map_err(|error| error.to_string()))
                .clone();
            Kind::Wallhaven {
                key: key_state(source.key_ref.as_deref(), exists),
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
        let settings = Settings::from_answers(&settings_answers_for(&path));
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
    fn adding_wallhaven_opens_the_key_field_and_the_source_needs_a_key() {
        let (mut settings, path) = window_from("add-wallhaven", CONFIG);
        settings.add_wallhaven();
        assert!(
            settings.key.open,
            "the key field opens beside the new source"
        );
        let row = settings.sources.rows.last().expect("the new source");
        assert_eq!(row.id, "wallhaven");
        let landed = std::fs::read_to_string(&path).expect("the file");
        // The label, which is a name, and never a key.
        assert!(
            landed.contains(&format!("\"api_key_ref\": \"{}\"", keychain::LABEL)),
            "{landed}"
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

        let mut settings = Settings::from_answers(&Answers::live(
            vec![format!("config: {}", path.display())],
            Vec::new(),
        ));
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
        std::fs::write(
            &path,
            CONFIG.replace("/tmp/walls", folder.to_str().expect("a path")),
        )
        .expect("the fixture");

        let mut settings = Settings::from_answers(&Answers::live(
            vec![format!("config: {}", path.display())],
            Vec::new(),
        ));
        settings.open_picker(Some("pictures".to_string()));
        assert_eq!(
            settings
                .picker
                .as_ref()
                .map(|picker| picker.directory.clone()),
            Some(folder)
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
        let mut settings = Settings::unreachable(reason);
        // The state a window on a machine with neither a daemon nor a locatable
        // config file is in: nothing was read, so nothing is shown for it.
        settings.target = None;
        settings.sources = Sources {
            rows: Vec::new(),
            problem: Some(config_file::NO_PATH.to_string()),
            outcome: None,
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

    #[test]
    fn a_config_file_that_cannot_be_read_says_so_instead_of_showing_no_sources() {
        let (directory, _path) = scratch("missing", CONFIG);
        let missing = directory.join("absent.json");
        let settings = Settings::from_answers(&settings_answers_for(&missing));
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
        let mut settings = Settings::from_answers(&Answers::unreachable("no daemon"));
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
        // Two sections, in the order the window draws them, and the daemon's
        // line last. Nothing else is on screen.
        let order: Vec<usize> = [SOURCES_TITLE, ROTATION_TITLE]
            .iter()
            .map(|title| text.find(title).unwrap_or_else(|| panic!("{title}")))
            .collect();
        assert!(order[0] < order[1], "{text}");
        // The daemon's line is the last thing on screen, and says where a change
        // actually lands.
        assert!(text.trim_end().ends_with(APP_DAEMON_NOTE), "{text}");
        assert!(
            text.find("whirl is running").expect("the daemon's line")
                < text.find(APP_DAEMON_NOTE).expect("the note"),
            "{text}"
        );
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
        )
    }
}
