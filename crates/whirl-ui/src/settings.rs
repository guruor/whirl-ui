//! The settings window's content: three panes, one of which edits.
//!
//! Every line in a pane that reports is the daemon's own, taken from the answer
//! to one of its requests and printed as it arrived. Nothing there is computed,
//! so nothing there can disagree with what the daemon said: `sources` fills the
//! Sources pane, `status`, `config path` and `config check` fill the App pane,
//! and the Rotation pane reads the rotation keys out of the `plan:` line of the
//! `config check` response.
//!
//! The one thing the window does that the daemon did not tell it is the rotation
//! interval ([`Interval`]). It is the only control this milestone wired, and it
//! is wired the way whirl's
//! `docs/decisions/0002-frontends-write-config-own-no-daemon.md` requires:
//!
//! - the value is written to the config file by [`crate::config_file`], through
//!   the daemon's own parser, atomically, and never sent to the daemon as a verb;
//! - after a write the pane says what the **file** now says and marks it
//!   **pending until the daemon re-reads it**, because nothing tells the client
//!   that the daemon has adopted it until `config_reloaded` arrives on the
//!   subscribe stream. The re-read is attributed to whirl's own architecture note
//!   ("the daemon re-reads the config on every rotation") rather than promised by
//!   this window, and the tray's Next is named as one such rotation;
//! - a value the parser refuses is refused with the parser's own message and the
//!   file is left as it was.
//!
//! Everything else stays read-only, and each pane says so on one visible line:
//! Sources is the next card, and the App pane's controls are not built at all.
//! [`Pane::enabled_controls`] counts the controls a pane can edit, and the
//! window's total is asserted in the tests, because "exactly one control edits,
//! and it is the interval" is a claim the reviewer can read out of the value
//! rather than take on trust.
//!
//! The window shows no secret. `status` carries paths and counts, never a key,
//! and the Sources pane prints the daemon's `reason=` sentence, which names where
//! a Wallhaven key was looked for and never what was found there. That is the
//! daemon's own wording: `no key at ...`, `checked env WHIRL_WALLHAVEN_API_KEY,
//! keychain label 'whirl-wallhaven'`.

use whirlui_client::protocol::{SourceRecord, parse_plan_record, parse_source_record};

use crate::config_file::{self, INTERVAL_KEY, Target};

/// The line the Sources pane carries while its controls are disabled. Editing
/// sources is the milestone's next card, not this one's.
pub const SOURCES_READ_ONLY: &str = "read-only: editing sources is a later card";

/// The line the App pane carries. None of its controls is built yet.
pub const APP_READ_ONLY: &str = "read-only";

/// One control the window draws.
///
/// In M1 every control existed only to be shown disabled: the shape of the edit
/// surface was visible and nothing could be pressed. M2 wired exactly one of
/// them, [`Control::Interval`], and left the rest as the shape it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// The rotation interval: the one control with a write path behind it.
    Interval,
    /// A control the window draws and cannot press, because no card owns it yet.
    Planned(&'static str),
}

impl Control {
    /// The label the button, or the field beside the edit control, carries.
    pub fn label(self) -> &'static str {
        match self {
            Control::Interval => "Rotation interval (seconds)",
            Control::Planned(label) => label,
        }
    }

    /// Whether this control can edit. Exactly one can, in the whole window.
    ///
    /// This is the value the window's "nothing else edits" claim rests on, so it
    /// is the tests' accessor rather than something the drawing code calls: the
    /// drawing reads the [`Control`] variant per widget.
    #[cfg(test)]
    pub fn enabled(self) -> bool {
        matches!(self, Control::Interval)
    }
}

/// The controls the Sources pane will own in a later card.
const SOURCE_CONTROLS: [Control; 4] = [
    Control::Planned("Add source…"),
    Control::Planned("Edit…"),
    Control::Planned("Remove…"),
    Control::Planned("Wallhaven key…"),
];

/// The controls the Rotation pane draws: the interval, which edits, and the four
/// that are still the shape of the surface.
const ROTATION_CONTROLS: [Control; 5] = [
    Control::Interval,
    Control::Planned("Display mode"),
    Control::Planned("Start at login"),
    Control::Planned("Startup mode"),
    Control::Planned("Respect manual"),
];

/// The controls the App pane will own in a later card. Reading the config file is
/// the one thing here that is not an edit, and it is still a button that does
/// nothing until the window has an action behind it.
const APP_CONTROLS: [Control; 1] = [Control::Planned("Open config file…")];

/// The `plan:` keys the Rotation pane shows: the schedule, the display mode and
/// the startup behaviour, by the config key paths the daemon reports them under
/// (docs/architecture.md 2.6).
const ROTATION_KEYS: [&str; 6] = [
    "schedule.interval_seconds",
    "display.mode",
    "display.mode_effective",
    "startup.enabled",
    "startup.mode",
    "startup.respect_manual",
];

/// One pane: a title, the daemon's lines, the controls it draws, and the one line
/// that says what its edit surface is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub title: &'static str,
    /// The daemon's own lines, in its own order, one per row.
    pub lines: Vec<String>,
    /// Every control the pane draws.
    pub controls: Vec<Control>,
    /// The one line under the controls. For a read-only pane it says so; for the
    /// Rotation pane it is the interval editor's own state ([`Interval::line`]),
    /// so it is refreshed whenever that state changes.
    pub footer: String,
}

impl Pane {
    fn new(
        title: &'static str,
        lines: Vec<String>,
        controls: &[Control],
        footer: impl Into<String>,
    ) -> Pane {
        Pane {
            title,
            lines,
            controls: controls.to_vec(),
            footer: footer.into(),
        }
    }

    /// Every line the window shows in this pane: the daemon's, then the one line
    /// that says what the pane's edit surface is doing.
    pub fn render(&self) -> Vec<String> {
        let mut lines = self.lines.clone();
        lines.push(self.footer.clone());
        lines
    }

    /// How many of this pane's controls can edit. Zero on every pane but Rotation.
    #[cfg(test)]
    pub fn enabled_controls(&self) -> usize {
        self.controls
            .iter()
            .filter(|control| control.enabled())
            .count()
    }
}

/// The rotation interval: the one setting this window writes.
///
/// `current` is what the daemon's plan reported, which is what the daemon
/// adopted and not necessarily what the file says. `input` is what the field
/// holds. `outcome` is the last write the window made, and it is what turns the
/// pane's line from "the daemon's plan" into "the file now says" or into the
/// parser's refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval {
    pub current: Option<u64>,
    pub input: String,
    pub outcome: Option<Outcome>,
}

/// What the last write attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The write landed, and the parser read this value back out of the file.
    Written {
        interval: u64,
        warnings: Vec<String>,
    },
    /// The write did not land, and this is why. The message is the parser's own
    /// when the parser was the one that refused.
    Refused { message: String },
}

impl Interval {
    /// The editor with the daemon's plan value in the field, when it had one.
    pub fn new(current: Option<u64>) -> Interval {
        Interval {
            current,
            input: current.map(|value| value.to_string()).unwrap_or_default(),
            outcome: None,
        }
    }

    /// The one line the Rotation pane carries under its controls.
    ///
    /// Before a write the value is named as the daemon's `config check` value,
    /// because that is what it is: the request parses the file at the time it is
    /// asked, and the file may say something else than the value the running
    /// daemon adopted. After a write the line names the file's value and calls it
    /// pending, because nothing tells the client the daemon has adopted it until
    /// `config_reloaded` arrives.
    ///
    /// The re-read is attributed to whirl's own architecture note rather than
    /// promised by this window, on purpose: the daemon's re-read is the daemon's
    /// to do, the pinned revision's code parses the config once at startup, and a
    /// window that said "applied" would be reporting a fact no client can observe.
    pub fn line(&self) -> String {
        match &self.outcome {
            Some(Outcome::Written { interval, warnings }) => {
                let mut line = format!(
                    "interval: {interval} (the config file now says this); pending until the daemon re-reads it, which its own architecture note puts at its next rotation, where the tray's Next is one"
                );
                if !warnings.is_empty() {
                    line.push_str(&format!(" [parser warnings: {}]", warnings.join("; ")));
                }
                line
            }
            Some(Outcome::Refused { message }) => {
                format!("interval: refused: {message} (the config file is unchanged)")
            }
            None => match self.current {
                Some(value) => format!(
                    "interval: {value} (from the daemon's config check); an edit is written to the config file and applies when the daemon re-reads it, which its own architecture note puts at its next rotation, where the tray's Next is one"
                ),
                None => "interval: (the daemon's config check reported none); an edit is written to the config file and applies when the daemon re-reads it, which its own architecture note puts at its next rotation, where the tray's Next is one".to_string(),
            },
        }
    }
}

/// The settings window: three panes, an interval editor, and the config file the
/// editor writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub sources: Pane,
    pub rotation: Pane,
    pub app: Pane,
    /// The config file the window writes, when one could be located.
    pub target: Option<Target>,
    /// The one editor in the window.
    pub interval: Interval,
}

impl Settings {
    /// The window when the daemon cannot be asked anything.
    ///
    /// Each pane carries the reason and nothing else: a pane that showed an empty
    /// list would say "there are no sources", which is a different and false
    /// statement. The reason is the client's own text, which names the socket
    /// file and never a directory (see `whirlui_client::Socket`).
    ///
    /// The interval editor is still here, and still points at a config file: the
    /// state a first-time user opens the window in is exactly the daemonless one,
    /// and the write path works there (the ADR's decision 1). Every other control
    /// is disabled exactly as it is on a live pane, so the shape of the window
    /// does not depend on whether a daemon happened to be running.
    pub fn unreachable(reason: &str) -> Settings {
        let interval = Interval::new(None);
        let line = || vec![reason.to_string()];
        Settings {
            sources: Pane::new("Sources", line(), &SOURCE_CONTROLS, SOURCES_READ_ONLY),
            rotation: Pane::new("Rotation", line(), &ROTATION_CONTROLS, interval.line()),
            app: Pane::new("App", line(), &APP_CONTROLS, APP_READ_ONLY),
            target: config_file::Target::default_path(),
            interval,
        }
    }

    /// Build the window from the daemon's four answers.
    pub fn from_answers(answers: &Answers) -> Settings {
        if !answers.connected {
            return Settings::unreachable(&answers.reason());
        }
        let interval = Interval::new(plan_interval(answers));
        Settings {
            sources: sources_pane(answers),
            rotation: rotation_pane(answers, &interval),
            app: app_pane(answers),
            target: target_of(answers),
            interval,
        }
    }

    /// The panes, in the order the window draws them.
    pub fn panes(&self) -> [&Pane; 3] {
        [&self.sources, &self.rotation, &self.app]
    }

    /// Write the field's value to the config file, and record what happened.
    ///
    /// This is the whole of the window's write path: parse the field, hand the
    /// value to [`config_file`], and put the outcome where the Rotation pane
    /// shows it. Nothing here opens a socket, so no part of a settings change is
    /// a daemon verb, and nothing here touches a state file or a platform setter.
    ///
    /// The outcome is returned as well as kept, so the headless mode that drives
    /// this same method can report it and set an exit code.
    pub fn save_interval(&mut self) -> &Outcome {
        let text = self.interval.input.trim().to_string();
        let outcome = match text.parse::<u64>() {
            Err(_) => Outcome::Refused {
                message: format!("{INTERVAL_KEY}: {text:?} is not a whole number of seconds"),
            },
            Ok(seconds) => match self.target.as_ref() {
                None => Outcome::Refused {
                    message: config_file::NO_PATH.to_string(),
                },
                Some(target) => match config_file::set_interval(&target.path, seconds) {
                    Ok(written) => Outcome::Written {
                        interval: written.interval,
                        warnings: written.warnings,
                    },
                    Err(error) => Outcome::Refused {
                        message: error.to_string(),
                    },
                },
            },
        };
        self.interval.outcome = Some(outcome);
        // The pane's line is the editor's state, so it moves with it.
        self.rotation.footer = self.interval.line();
        self.interval
            .outcome
            .as_ref()
            .expect("the outcome was just set")
    }

    /// How many controls in the whole window can edit. One, and it is the
    /// interval.
    #[cfg(test)]
    pub fn enabled_controls(&self) -> usize {
        self.panes()
            .iter()
            .map(|pane| pane.enabled_controls())
            .sum()
    }

    /// Every control that can edit, as its pane's title beside it.
    #[cfg(test)]
    pub fn editable_controls(&self) -> Vec<(&'static str, Control)> {
        self.panes()
            .iter()
            .flat_map(|pane| {
                pane.controls
                    .iter()
                    .filter(|control| control.enabled())
                    .map(|control| (pane.title, *control))
            })
            .collect()
    }

    /// The panes as text: what `whirl-ui --dump-settings` prints and what a test
    /// asserts, so the window is checkable with no display attached.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (index, pane) in self.panes().iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(pane.title);
            out.push('\n');
            for line in pane.render() {
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
        }
        out.trim_end_matches('\n').to_string()
    }
}

/// The daemon's answers, per request, or the reason there is no answer.
///
/// Each `Ok` is the response body exactly as the daemon sent it: the lines the
/// panes render are those lines, not a re-encoding of them. Each `Err` is the
/// reason in the client's own words, which is what a pane shows when a verb was
/// refused or the daemon is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answers {
    /// Whether a connection was made at all. This is the one fact the window
    /// states that is not a line of the daemon's own, and it is what lets the App
    /// pane say the socket is live.
    pub connected: bool,
    pub status: Result<Vec<String>, String>,
    pub sources: Result<Vec<String>, String>,
    pub config_path: Result<Vec<String>, String>,
    pub config_check: Result<Vec<String>, String>,
}

impl Answers {
    /// The four answers of a live conversation.
    pub fn live(
        status: Vec<String>,
        sources: Vec<String>,
        config_path: Vec<String>,
        config_check: Vec<String>,
    ) -> Answers {
        Answers {
            connected: true,
            status: Ok(status),
            sources: Ok(sources),
            config_path: Ok(config_path),
            config_check: Ok(config_check),
        }
    }

    /// Every answer missing, because the daemon could not be reached.
    pub fn unreachable(reason: &str) -> Answers {
        Answers {
            connected: false,
            status: Err(reason.to_string()),
            sources: Err(reason.to_string()),
            config_path: Err(reason.to_string()),
            config_check: Err(reason.to_string()),
        }
    }

    /// The first reason any request failed for. Every pane shows this when the
    /// connection itself failed.
    pub fn reason(&self) -> String {
        for answer in [
            &self.status,
            &self.sources,
            &self.config_path,
            &self.config_check,
        ] {
            if let Err(reason) = answer {
                return reason.clone();
            }
        }
        // Unreachable only when nothing failed, which cannot happen: the callers
        // that produce a disconnected `Answers` fill all four.
        "the daemon sent no answer".to_string()
    }
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

/// The effective rotation interval of the daemon's plan, when it reported one.
fn plan_interval(answers: &Answers) -> Option<u64> {
    let check = answers.config_check.as_ref().ok()?;
    let plan = check.iter().find_map(|line| parse_plan_record(line))?;
    plan.iter()
        .find(|(name, _)| *name == INTERVAL_KEY)
        .and_then(|(_, value)| value.parse().ok())
}

/// One row per source, from `sources`.
///
/// The row is the daemon's `source:` line, verbatim. When `config check` reports
/// the same source with a different effective `enabled` or `reason`, the daemon's
/// `config check` line for it is shown indented under the row, because that is
/// where a source refused at load carries why: a Wallhaven source whose key is
/// missing is enabled in `sources` and disabled in the plan the daemon adopted,
/// with a `reason=` naming where it looked. Both are the daemon's words; the
/// window shows both rather than choosing.
fn sources_pane(answers: &Answers) -> Pane {
    let mut lines = Vec::new();
    match &answers.sources {
        Err(reason) => lines.push(reason.clone()),
        Ok(raw) => {
            let checked = match &answers.config_check {
                Ok(check) => records_of(check),
                Err(_) => Vec::new(),
            };
            for line in raw {
                lines.push(line.clone());
                let Some(record) = parse_source_record(line) else {
                    continue;
                };
                if let Some((check_line, check)) =
                    checked.iter().find(|(_, check)| check.id == record.id)
                    && (check.enabled != record.enabled || check.reason != record.reason)
                {
                    lines.push(format!("    check: {check_line}"));
                }
            }
        }
    }
    Pane::new("Sources", lines, &SOURCE_CONTROLS, SOURCES_READ_ONLY)
}

/// The rotation keys, from the `plan:` line of the daemon's `config check`.
///
/// `config check` parses the file at request time, so these are the file's
/// effective values and not necessarily what the running daemon adopted: a write
/// this window made shows up here before the daemon has rotated, which is exactly
/// why the Rotation pane's own line calls the change pending.
///
/// A key the daemon did not report is named rather than filled with `-`: `-` is
/// 2.6's "unset", and a key an older daemon never sent is a different fact (2.4:
/// clients tolerate unknown keys; the reverse needs to be visible too).
fn rotation_pane(answers: &Answers, interval: &Interval) -> Pane {
    let mut lines = Vec::new();
    match &answers.config_check {
        Err(reason) => lines.push(reason.clone()),
        Ok(raw) => {
            let plan = raw.iter().find_map(|line| parse_plan_record(line));
            for key in ROTATION_KEYS {
                let value = plan.as_ref().and_then(|pairs| {
                    pairs
                        .iter()
                        .find(|(name, _)| *name == key)
                        .map(|(_, value)| value.as_str())
                });
                match value {
                    Some(value) => lines.push(format!("{key}: {value}")),
                    None => lines.push(format!("{key}: (not reported by this daemon)")),
                }
            }
        }
    }
    Pane::new("Rotation", lines, &ROTATION_CONTROLS, interval.line())
}

/// The daemon's own state: the socket, `status`, `config path` and `config check`,
/// each printed whole.
///
/// The panes are different views and not a partition: `config check`'s per-source
/// records appear here because they are part of that answer, and again in the
/// Sources pane because that is where the card puts a source's effective
/// `enabled` and `reason`. Nothing is dropped for being shown elsewhere, so a
/// value in this pane is a value `status` or `config check` printed.
fn app_pane(answers: &Answers) -> Pane {
    let mut lines = vec!["socket: live".to_string()];
    match &answers.status {
        Ok(raw) => lines.extend(raw.iter().cloned()),
        Err(reason) => lines.push(reason.clone()),
    }
    match &answers.config_path {
        Ok(raw) => lines.extend(raw.iter().cloned()),
        Err(reason) => lines.push(reason.clone()),
    }
    match &answers.config_check {
        Ok(raw) => lines.extend(raw.iter().cloned()),
        Err(reason) => lines.push(reason.clone()),
    }
    Pane::new("App", lines, &APP_CONTROLS, APP_READ_ONLY)
}

/// The parsed `source:` records of a response, each with the line it came from.
fn records_of(lines: &[String]) -> Vec<(String, SourceRecord)> {
    lines
        .iter()
        .filter_map(|line| parse_source_record(line).map(|record| (line.clone(), record)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// One `sources` answer and one `config check` answer, as a daemon sends
    /// them: a local source, and a Wallhaven source whose key is not there.
    fn answers() -> Answers {
        Answers::live(
            vec![
                "daemon_version: whirl 0.1.0".to_string(),
                "protocol: 2".to_string(),
                "seq: 4".to_string(),
                "sources: 2".to_string(),
            ],
            vec![
                "count: 2".to_string(),
                "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
                "source: space wallhaven weight=2 enabled=1 last=- reason=-".to_string(),
            ],
            vec!["config: /somewhere/config.json".to_string()],
            vec![
                "queued".to_string(),
                "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
                "source: space wallhaven weight=2 enabled=0 last=- reason=no key at keychain:whirl-wallhaven".to_string(),
                "plan: schedule.interval_seconds=1800 startup.enabled=1 startup.mode=last startup.respect_manual=1 display.mode=all display.mode_effective=all backend=noop".to_string(),
            ],
        )
    }

    #[test]
    fn the_sources_pane_has_one_row_per_source_and_the_refused_ones_reason() {
        let settings = Settings::from_answers(&answers());
        let rows: Vec<&String> = settings
            .sources
            .lines
            .iter()
            .filter(|line| line.starts_with("source: "))
            .collect();
        assert_eq!(rows.len(), 2, "{:?}", settings.sources.lines);
        assert!(settings.sources.lines[0].starts_with("count: 2"));
        // The Wallhaven source is enabled in `sources` and refused in the plan,
        // and the window shows the daemon's line for the refusal underneath.
        assert_eq!(
            settings.sources.lines.last().map(String::as_str),
            Some(
                "    check: source: space wallhaven weight=2 enabled=0 last=- reason=no key at keychain:whirl-wallhaven"
            )
        );
        // No value, no masked value, no length: the reason is the daemon's own
        // sentence and nothing here adds a key to it.
        let text = settings.sources.lines.join("\n");
        assert!(!text.contains("api_key"), "{text}");
    }

    #[test]
    fn the_rotation_pane_shows_the_keys_the_card_names() {
        let settings = Settings::from_answers(&answers());
        for (key, value) in [
            ("schedule.interval_seconds", "1800"),
            ("display.mode", "all"),
            ("startup.enabled", "1"),
            ("startup.mode", "last"),
            ("startup.respect_manual", "1"),
        ] {
            let wanted = format!("{key}: {value}");
            assert!(settings.rotation.lines.contains(&wanted), "{wanted}");
        }
    }

    #[test]
    fn the_app_pane_shows_the_socket_the_status_the_path_and_the_rest_of_the_check() {
        let settings = Settings::from_answers(&answers());
        assert_eq!(settings.app.lines[0], "socket: live");
        for line in [
            "daemon_version: whirl 0.1.0",
            "protocol: 2",
            "config: /somewhere/config.json",
            "queued",
        ] {
            assert!(settings.app.lines.iter().any(|own| own == line), "{line}");
        }
        // The plan line is shown whole, because the Rotation pane names only the
        // rotation keys and App is where the rest of it stays visible.
        assert!(
            settings
                .app
                .lines
                .iter()
                .any(|line| line.starts_with("plan: ") && line.contains("backend=noop")),
            "{:?}",
            settings.app.lines
        );
        // The panes are views rather than a partition: the check's per-source
        // records are part of that answer, so App carries them, and the Sources
        // pane carries the effective ones beside their rows.
        assert!(
            settings
                .app
                .lines
                .iter()
                .any(|line| line.contains("enabled=0") && line.starts_with("source: ")),
            "{:?}",
            settings.app.lines
        );
    }

    #[test]
    fn exactly_one_control_edits_and_it_is_the_rotation_interval() {
        for settings in [
            Settings::from_answers(&answers()),
            Settings::unreachable(
                "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)",
            ),
        ] {
            assert_eq!(settings.enabled_controls(), 1);
            assert_eq!(
                settings.editable_controls(),
                vec![("Rotation", Control::Interval)]
            );
        }
    }

    #[test]
    fn every_read_only_pane_says_so_and_offers_nothing_that_edits() {
        let settings = Settings::from_answers(&answers());
        assert_eq!(settings.sources.enabled_controls(), 0);
        assert_eq!(settings.app.enabled_controls(), 0);
        assert_eq!(
            settings.sources.render().last().map(String::as_str),
            Some(SOURCES_READ_ONLY)
        );
        assert_eq!(
            settings.app.render().last().map(String::as_str),
            Some(APP_READ_ONLY)
        );
        // The disabled controls are still drawn: the shape of the surface is the
        // same one M1 showed, so a reader can see what is coming.
        for pane in [&settings.sources, &settings.app, &settings.rotation] {
            assert!(!pane.controls.is_empty(), "{}", pane.title);
        }
    }

    #[test]
    fn the_interval_editor_starts_on_the_checks_value_and_says_when_it_applies() {
        let settings = Settings::from_answers(&answers());
        assert_eq!(settings.interval.current, Some(1800));
        assert_eq!(settings.interval.input, "1800");
        let line = settings.interval.line();
        assert!(line.contains("from the daemon's config check"), "{line}");
        assert!(line.contains("next rotation"), "{line}");
        assert!(line.contains("Next"), "{line}");
        // Before any write the pane names the request the value came from, never
        // the file: nothing here has read the file, and the daemon's own
        // `config check` is what parsed it.
        assert!(!line.contains("the config file now says"), "{line}");
    }

    /// A config file of our own, and a window pointed at it.
    fn window_over(tag: &str, text: &str) -> (Settings, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "whirlui-window-interval-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join("config.json");
        std::fs::write(&path, text).expect("a config file");
        let mut settings = Settings::from_answers(&answers());
        settings.target = Some(Target {
            path: path.clone(),
            named_by_daemon: false,
        });
        (settings, path)
    }

    /// A minimal config the parser accepts, with a comment key to preserve.
    const SCRATCH_CONFIG: &str = r#"{
  "_comment_1": "keep me",
  "config_schema": 1,
  "schedule": {"interval_seconds": 1800},
  "sources": []
}"#;

    #[test]
    fn a_written_interval_is_the_files_value_marked_pending_until_it_is_re_read() {
        let (mut settings, path) = window_over("written", SCRATCH_CONFIG);
        settings.interval.input = "900".to_string();
        settings.save_interval();

        assert_eq!(
            settings.interval.outcome,
            Some(Outcome::Written {
                interval: 900,
                warnings: Vec::new()
            })
        );
        let line = settings.rotation.footer.clone();
        assert!(line.contains("the config file now says this"), "{line}");
        assert!(
            line.contains("pending until the daemon re-reads it"),
            "{line}"
        );
        assert!(line.contains("next rotation"), "{line}");
        assert!(line.contains("Next"), "{line}");
        // The file, and the one line the pane now carries, both say 900.
        let landed = std::fs::read_to_string(&path).expect("the file");
        assert!(landed.contains("\"interval_seconds\": 900"), "{landed}");
        assert!(landed.contains("\"_comment_1\""), "{landed}");
        assert!(settings.to_text().contains(&line), "{}", settings.to_text());
    }

    #[test]
    fn a_refused_interval_says_why_in_the_pane_and_leaves_the_file_alone() {
        let (mut settings, path) = window_over("refused", SCRATCH_CONFIG);
        let before = std::fs::read(&path).expect("the file");
        // 30 is below the floor whirl-core enforces, so the parser refuses the
        // document and the pane carries the parser's own sentence.
        settings.interval.input = "30".to_string();
        settings.save_interval();

        let line = settings.rotation.footer.clone();
        assert!(line.contains("refused"), "{line}");
        assert!(line.contains("schedule.interval_seconds"), "{line}");
        assert!(line.contains("less than 60"), "{line}");
        assert!(line.contains("the config file is unchanged"), "{line}");
        assert_eq!(std::fs::read(&path).expect("the file"), before);
    }

    #[test]
    fn a_field_that_is_not_a_number_is_refused_before_the_file_is_touched() {
        let (mut settings, path) = window_over("not-a-number", SCRATCH_CONFIG);
        let before = std::fs::read(&path).expect("the file");
        settings.interval.input = "half an hour".to_string();
        settings.save_interval();
        let line = settings.rotation.footer.clone();
        assert!(line.contains("is not a whole number of seconds"), "{line}");
        assert_eq!(std::fs::read(&path).expect("the file"), before);
    }

    #[test]
    fn the_no_daemon_window_renders_the_reason_in_every_pane() {
        let reason = "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)";
        let settings = Settings::from_answers(&Answers::unreachable(reason));
        assert_eq!(settings, Settings::unreachable(reason));
        let text = settings.to_text();
        assert_eq!(text.matches(reason).count(), 3, "{text}");
        // The interval editor is the one pane line that is not the reason, and it
        // is still there: the daemonless state is where a first write happens.
        assert_eq!(text.matches("interval: ").count(), 1, "{text}");
        // The reason the client prints names the socket file and no directory.
        assert!(!text.contains("/Users"), "{text}");
        assert!(!text.contains("reason="), "{text}");
    }

    #[test]
    fn the_text_of_the_window_is_the_three_panes_in_order() {
        let settings = Settings::from_answers(&answers());
        let text = settings.to_text();
        let order: Vec<usize> = ["Sources", "Rotation", "App"]
            .iter()
            .map(|title| text.find(title).unwrap_or_else(|| panic!("{title}")))
            .collect();
        assert!(order[0] < order[1] && order[1] < order[2], "{text}");
        // Each read-only pane carries its own line, once.
        assert_eq!(
            settings.sources.render().last().map(String::as_str),
            Some(SOURCES_READ_ONLY)
        );
        assert_eq!(
            settings.app.render().last().map(String::as_str),
            Some(APP_READ_ONLY)
        );
        assert_eq!(text.matches(SOURCES_READ_ONLY).count(), 1, "{text}");
        assert_eq!(
            settings
                .app
                .render()
                .iter()
                .filter(|line| *line == APP_READ_ONLY)
                .count(),
            1,
            "{text}"
        );
    }

    #[test]
    fn a_key_this_daemon_did_not_report_is_named_rather_than_shown_as_unset() {
        let answers = Answers::live(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec!["plan: display.mode=all".to_string()],
        );
        let settings = Settings::from_answers(&answers);
        assert!(
            settings
                .rotation
                .lines
                .contains(&"display.mode: all".to_string())
        );
        assert!(
            settings
                .rotation
                .lines
                .contains(&"startup.mode: (not reported by this daemon)".to_string())
        );
        // A daemon whose plan names no interval leaves the field empty rather
        // than filling it with a value nobody reported.
        assert_eq!(settings.interval.current, None);
        assert_eq!(settings.interval.input, "");
    }

    #[test]
    fn a_refused_verb_is_shown_as_the_reason_of_its_pane_alone() {
        let mut answers = answers();
        answers.sources = Err("the daemon refused: busy: a rotation is running".to_string());
        let settings = Settings::from_answers(&answers);
        assert_eq!(
            settings.sources.lines,
            vec!["the daemon refused: busy: a rotation is running".to_string()]
        );
        // The panes that were answered keep their rows.
        assert!(!settings.rotation.lines.is_empty());
        assert_eq!(settings.app.lines[0], "socket: live");
    }

    #[test]
    fn the_target_is_the_config_path_the_daemon_named() {
        let settings = Settings::from_answers(&answers());
        let target = settings.target.expect("a target");
        assert_eq!(target.path, PathBuf::from("/somewhere/config.json"));
        assert!(target.named_by_daemon);
    }
}
