//! The settings window's content: three panes, read-only.
//!
//! Every line in a pane is the daemon's own, taken from the answer to one of its
//! requests and printed as it arrived. Nothing here is computed, so nothing here
//! can disagree with what the daemon said: `sources` fills the Sources pane,
//! `status`, `config path` and `config check` fill the App pane, and the
//! Rotation pane reads the rotation keys out of the `plan:` line the daemon
//! adopted.
//!
//! The three panes are views of those answers rather than a partition of them, so
//! a line can appear in two places and does: `config check`'s per-source records
//! are in App because they are part of that answer, and in Sources beside the
//! source each one is about. Dropping a line for being shown elsewhere would leave
//! the window quietly missing a value the daemon reported.
//!
//! Editing is not built (M2 owns it, and whirl's
//! `docs/decisions/0002-frontends-write-config-own-no-daemon.md` is the contract
//! it lands under), so every control a pane would have is disabled and every
//! pane says so on one visible line. [`Settings::enabled_controls`] is the count
//! of controls that can edit, and it is zero; a test asserts it, because "there
//! is no write path anywhere in the code" is a claim the reviewer will read it
//! out of the source rather than take on trust.
//!
//! The window shows no secret. `status` carries paths and counts, never a key,
//! and the Sources pane prints the daemon's `reason=` sentence, which names where
//! a Wallhaven key was looked for and never what was found there. That is the
//! daemon's own wording: `no key at ...`, `checked env WHIRL_WALLHAVEN_API_KEY,
//! keychain label 'whirl-wallhaven'`.

use whirlui_client::protocol::{SourceRecord, parse_plan_record, parse_source_record};

/// The line every pane carries while the window edits nothing.
pub const EDITING_ARRIVES_IN_M2: &str = "editing arrives in M2";

/// One control the window draws.
///
/// In M1 every control exists only to be shown disabled: the shape of the edit
/// surface is visible, and nothing can be pressed. `enabled` is a field rather
/// than a constant so the claim "no control is enabled" is a value a test can
/// read, not a hope about the drawing code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    pub label: &'static str,
    pub enabled: bool,
}

/// The controls the Sources pane will own in M2.
const SOURCE_CONTROLS: [&str; 4] = ["Add source…", "Edit…", "Remove…", "Wallhaven key…"];

/// The controls the Rotation pane will own in M2.
const ROTATION_CONTROLS: [&str; 5] = [
    "Interval…",
    "Display mode",
    "Start at login",
    "Startup mode",
    "Respect manual",
];

/// The controls the App pane will own in M2. Reading the config file is the one
/// thing here that is not an edit, and it is still a button that does nothing
/// until the window has a file to open.
const APP_CONTROLS: [&str; 1] = ["Open config file…"];

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

/// One pane: a title, the daemon's lines, and the controls it will own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub title: &'static str,
    /// The daemon's own lines, in its own order, one per row.
    pub lines: Vec<String>,
    /// Every control the pane draws, each one disabled in M1.
    pub controls: Vec<Control>,
}

impl Pane {
    fn new(title: &'static str, lines: Vec<String>, labels: &[&'static str]) -> Pane {
        Pane {
            title,
            lines,
            controls: labels
                .iter()
                .copied()
                .map(|label| Control {
                    label,
                    enabled: false,
                })
                .collect(),
        }
    }

    /// Every line the window shows in this pane: the daemon's, then the one line
    /// that says why nothing can be pressed.
    pub fn render(&self) -> Vec<String> {
        let mut lines = self.lines.clone();
        lines.push(EDITING_ARRIVES_IN_M2.to_string());
        lines
    }

    /// How many of this pane's controls can edit. Zero in M1.
    ///
    /// This is the assertion the window's read-only claim rests on, so it is the
    /// tests' accessor rather than something the drawing code calls: the drawing
    /// reads [`Control::enabled`] per button.
    #[cfg(test)]
    pub fn enabled_controls(&self) -> usize {
        self.controls
            .iter()
            .filter(|control| control.enabled)
            .count()
    }
}

/// The settings window: three panes, and nothing that writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub sources: Pane,
    pub rotation: Pane,
    pub app: Pane,
}

impl Settings {
    /// The window when the daemon cannot be asked anything.
    ///
    /// Each pane carries the reason and nothing else: a pane that showed an empty
    /// list would say "there are no sources", which is a different and false
    /// statement. The reason is the client's own text, which names the socket
    /// file and never a directory (see `whirlui_client::Socket`). The controls are
    /// the same ones a live pane draws and they are disabled here too, so the
    /// shape of the window does not depend on whether a daemon happened to be
    /// running.
    pub fn unreachable(reason: &str) -> Settings {
        let line = || vec![reason.to_string()];
        Settings {
            sources: Pane::new("Sources", line(), &SOURCE_CONTROLS),
            rotation: Pane::new("Rotation", line(), &ROTATION_CONTROLS),
            app: Pane::new("App", line(), &APP_CONTROLS),
        }
    }

    /// Build the window from the daemon's four answers.
    pub fn from_answers(answers: &Answers) -> Settings {
        if !answers.connected {
            return Settings::unreachable(&answers.reason());
        }
        Settings {
            sources: sources_pane(answers),
            rotation: rotation_pane(answers),
            app: app_pane(answers),
        }
    }

    /// The panes, in the order the window draws them.
    pub fn panes(&self) -> [&Pane; 3] {
        [&self.sources, &self.rotation, &self.app]
    }

    /// How many controls in the whole window can edit. Zero, always, in M1.
    #[cfg(test)]
    pub fn enabled_controls(&self) -> usize {
        self.panes()
            .iter()
            .map(|pane| pane.enabled_controls())
            .sum()
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
    Pane::new("Sources", lines, &SOURCE_CONTROLS)
}

/// The rotation keys, from the `plan:` line the daemon adopted.
///
/// A key the daemon did not report is named rather than filled with `-`: `-` is
/// 2.6's "unset", and a key an older daemon never sent is a different fact (2.4:
/// clients tolerate unknown keys; the reverse needs to be visible too).
fn rotation_pane(answers: &Answers) -> Pane {
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
    Pane::new("Rotation", lines, &ROTATION_CONTROLS)
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
    Pane::new("App", lines, &APP_CONTROLS)
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
            vec![
                "config: /somewhere/config.json".to_string(),
            ],
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
    fn every_pane_says_editing_arrives_in_m2_and_no_control_is_enabled() {
        for settings in [
            Settings::from_answers(&answers()),
            Settings::unreachable(
                "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)",
            ),
        ] {
            assert_eq!(settings.enabled_controls(), 0);
            for pane in settings.panes() {
                assert!(!pane.controls.is_empty(), "{}", pane.title);
                assert_eq!(
                    pane.render().last().map(String::as_str),
                    Some(EDITING_ARRIVES_IN_M2),
                    "{}",
                    pane.title
                );
            }
        }
    }

    #[test]
    fn the_no_daemon_window_renders_the_reason_in_every_pane() {
        let reason = "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)";
        let settings = Settings::from_answers(&Answers::unreachable(reason));
        assert_eq!(settings, Settings::unreachable(reason));
        let text = settings.to_text();
        assert_eq!(text.matches(reason).count(), 3, "{text}");
        assert_eq!(text.matches(EDITING_ARRIVES_IN_M2).count(), 3, "{text}");
        // The reason the client prints names the socket file and no directory.
        assert!(!text.contains("/Users"), "{text}");
        assert!(!text.contains("reason="), "{text}");
    }

    #[test]
    fn the_text_of_the_window_is_the_three_panes_in_order() {
        let text = Settings::from_answers(&answers()).to_text();
        let order: Vec<usize> = ["Sources", "Rotation", "App"]
            .iter()
            .map(|title| text.find(title).unwrap_or_else(|| panic!("{title}")))
            .collect();
        assert!(order[0] < order[1] && order[1] < order[2], "{text}");
        assert_eq!(text.matches(EDITING_ARRIVES_IN_M2).count(), 3, "{text}");
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
}
