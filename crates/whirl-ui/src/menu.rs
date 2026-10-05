//! The menu bar item's rows, as data.
//!
//! A menu bar item cannot be asserted by a test: it needs a display, a
//! logged-in session and a click. So the rows are built here, as a value, and
//! `whirl-ui --menu-dump` prints them (`docs/milestones.md` M1 criterion 3).
//! The tray renders this list and nothing else, which is what makes the printed
//! rows evidence rather than a second description of the menu.
//!
//! The order is the one `--menu-dump` prints and `docs/milestones.md` M1
//! criterion 3 pins, and it is the order in the field: the current image on a
//! line of its own, the two movements, the one pause row whose label follows
//! `paused`, the pin, a separator, and the two rows that leave the rotation
//! alone.

use crate::state::View;

/// A row's stable name.
///
/// The tray carries this name as the menu item's id and the click comes back as
/// the same name, so a row and the action it sends cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowId {
    /// The current image. A line, not a control.
    Now,
    Next,
    Previous,
    /// `pause`, and the row's label while the schedule is live.
    Pause,
    /// `resume`, and the row's label while the schedule is suspended.
    Resume,
    Favourite,
    /// Ask the daemon's own command to start it, shown only when the daemon is
    /// not answering. Not a daemon protocol verb: it is `whirl daemon start`,
    /// which a frontend may ask for and never performs itself.
    StartDaemon,
    Settings,
    Quit,
    /// A line the app is reporting (the daemon command's own answer). A line,
    /// not a control.
    Report,
}

/// Resolving a click is the tray's job, and the tray is macOS-only for now, so
/// on the other two legs this mapping is reached by the tests below and by
/// nothing else. It stays in this module anyway, and stays tested there, because
/// the row ids are what the tray's menu is built from: the alternative is a
/// mapping that only the platform with no CI leg can exercise.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl RowId {
    /// Every row, in the order they appear on screen.
    pub const ALL: [RowId; 10] = [
        RowId::Now,
        RowId::Next,
        RowId::Previous,
        RowId::Pause,
        RowId::Resume,
        RowId::Favourite,
        RowId::StartDaemon,
        RowId::Settings,
        RowId::Quit,
        RowId::Report,
    ];

    /// The name the menu item carries.
    pub fn key(self) -> &'static str {
        match self {
            RowId::Now => "now",
            RowId::Next => "next",
            RowId::Previous => "previous",
            RowId::Pause => "pause",
            RowId::Resume => "resume",
            RowId::Favourite => "favourite",
            RowId::StartDaemon => "start-daemon",
            RowId::Settings => "settings",
            RowId::Quit => "quit",
            RowId::Report => "report",
        }
    }

    /// The row a clicked item's name came from.
    pub fn from_key(key: &str) -> Option<RowId> {
        RowId::ALL.into_iter().find(|row| row.key() == key)
    }

    /// What a click on this row asks for, or `None` for the rows that are lines
    /// rather than controls.
    pub fn action(self) -> Option<Action> {
        match self {
            RowId::Now | RowId::Report => None,
            RowId::Next => Some(Action::Next),
            RowId::Previous => Some(Action::Previous),
            RowId::Pause => Some(Action::Pause),
            RowId::Resume => Some(Action::Resume),
            RowId::Favourite => Some(Action::Favourite),
            RowId::StartDaemon => Some(Action::StartDaemon),
            RowId::Settings => Some(Action::Settings),
            RowId::Quit => Some(Action::Quit),
        }
    }
}

/// What a click asks the app to do.
///
/// Five of these are whirl's own verbs (2.5.1) and go to the daemon through
/// `whirlui-client`; two are the app's own business and never reach the socket.
/// `StartDaemon` is a third kind: it is the daemon's own command rather than a
/// protocol verb, and it asks the OS supervisor rather than spawning anything.
/// Only the tray builds one, so on the non-macOS legs it is the tests that do.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Next,
    Previous,
    Pause,
    Resume,
    Favourite,
    /// Ask `whirl daemon start` to start the daemon, through the supervisor.
    StartDaemon,
    /// Show the settings window: the real one, `app`'s, from the `Settings…` row.
    /// Every control is disabled and every pane says so.
    Settings,
    /// Quit this app. Never the daemon: its lifetime belongs to the OS
    /// supervisor (section 8, "must never" 3).
    Quit,
}

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `None` on a separator, which is the one row with no name.
    pub id: Option<RowId>,
    pub label: String,
    pub enabled: bool,
}

impl Row {
    /// A named row.
    pub fn item(id: RowId, label: impl Into<String>, enabled: bool) -> Row {
        Row {
            id: Some(id),
            label: label.into(),
            enabled,
        }
    }

    /// A separator.
    pub fn separator() -> Row {
        Row {
            id: None,
            label: String::new(),
            enabled: false,
        }
    }

    /// Whether this row is a separator.
    pub fn is_separator(&self) -> bool {
        self.id.is_none()
    }

    /// Whether this row is a control: something a click would have to ask the
    /// daemon for. The current image is the one row that is not, and it is
    /// inert for that reason rather than because anything is wrong.
    pub fn is_control(&self) -> bool {
        self.id.is_some_and(|id| id.action().is_some())
    }

    /// The line `--menu-dump` prints for this row: the label, and whether a
    /// control on it can be used.
    ///
    /// The suffix is for controls only. The image line is disabled on the tray
    /// so that clicking it does nothing, and printing `the daemon is not
    /// running (disabled)` would read as a broken control rather than as the
    /// sentence it is.
    pub fn line(&self) -> String {
        if self.is_separator() {
            "---".to_string()
        } else if self.enabled || !self.is_control() {
            self.label.clone()
        } else {
            format!("{} (disabled)", self.label)
        }
    }
}

/// The rows for the state the app is holding, in menu order.
///
/// Three rules are worth stating because they are decisions rather than
/// descriptions. `Favourite` disappears while `favorites_degraded` is set: the
/// daemon cannot honour a pin, and section 8 item 5 says to hide the affordance
/// rather than to offer one that fails. `Start whirl` appears exactly when the
/// daemon is not answering, and it is a control rather than an interruption: the
/// app offers the daemon's own command instead of a window that reads as broken
/// with no way forward. And a row that needs the daemon is disabled exactly when
/// there is no daemon, while `Settings…` and `Quit` stay live because neither is
/// a daemon verb and an app with no way to quit is a bug, not a state.
pub fn rows(view: &View) -> Vec<Row> {
    let reachable = view.reachable();
    let mut rows = vec![Row::item(RowId::Now, view.image_line(), false)];
    if !reachable {
        // The offer sits directly under the line that says the daemon is not
        // running, so the sentence and the way forward are read together.
        rows.push(Row::item(RowId::StartDaemon, "Start whirl", true));
    }
    rows.push(Row::item(RowId::Next, "Next", reachable));
    rows.push(Row::item(RowId::Previous, "Previous", reachable));
    rows.push(if view.paused() {
        Row::item(RowId::Resume, "Resume", reachable)
    } else {
        Row::item(RowId::Pause, "Pause", reachable)
    });
    if !view.favourites_degraded() {
        rows.push(Row::item(RowId::Favourite, "Favourite", reachable));
    }
    rows.push(Row::separator());
    rows.push(Row::item(RowId::Settings, "Settings…", true));
    rows.push(Row::item(RowId::Quit, "Quit", true));
    rows
}

/// The same rows, with one line the app is reporting added under the image line.
///
/// The line is the daemon command's own answer (a start that refused, a start
/// that was done), which is the app's own report and not a row of the daemon's
/// menu: `--menu-dump` prints the rows without it, and the tray draws it only
/// when there is something to say.
pub fn rows_reporting(view: &View, report: Option<&str>) -> Vec<Row> {
    let mut rows = rows(view);
    if let Some(text) = report {
        rows.insert(1, Row::item(RowId::Report, text, false));
    }
    rows
}

/// The rows as the lines `--menu-dump` prints, one per row.
pub fn lines(view: &View) -> Vec<String> {
    rows(view).iter().map(Row::line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use whirlui_client::Status;

    /// A `status` body, as the daemon prints the keys the menu reads.
    fn status(paused: bool, rotating: bool, degraded: bool, origin: &str) -> View {
        let lines: Vec<String> = [
            "daemon_version: whirl 0.1.0".to_string(),
            "protocol: 2".to_string(),
            "seq: 12".to_string(),
            format!("paused: {}", u8::from(paused)),
            format!("rotating: {}", u8::from(rotating)),
            format!("favorites_degraded: {}", u8::from(degraded)),
            "last_digest: d435840ce84fbb8d633f0d1f81ad0b620fff857597bca6ab238b51e164e1da9f"
                .to_string(),
            format!("last_origin_key: {origin}"),
            "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
        ]
        .to_vec();
        View::live(Status::from_lines(&lines))
    }

    /// The row ids and their enabled flags, which is what the three states are
    /// asserted on.
    fn shape(view: &View) -> Vec<(Option<RowId>, bool)> {
        rows(view).iter().map(|row| (row.id, row.enabled)).collect()
    }

    #[test]
    fn a_rotating_daemon_offers_every_row_and_a_rotation_does_not_disable_one() {
        let view = status(false, true, false, "pictures:8d9600e8");
        assert_eq!(
            shape(&view),
            vec![
                (Some(RowId::Now), false),
                (Some(RowId::Next), true),
                (Some(RowId::Previous), true),
                (Some(RowId::Pause), true),
                (Some(RowId::Favourite), true),
                (None, false),
                (Some(RowId::Settings), true),
                (Some(RowId::Quit), true),
            ]
        );
        // The line keeps the last known image while the daemon works: `busy` is
        // a retry (section 8 item 5), not a reason to blank the row.
        assert_eq!(view.image_line(), "pictures:8d9600e8");
        // With the daemon up, nothing is disabled except the line that is not a
        // control in the first place.
        assert!(!lines(&view).iter().any(|line| line.contains("disabled")));
    }

    #[test]
    fn the_disabled_suffix_only_ever_lands_on_a_control() {
        for view in [
            status(false, true, false, "pictures:8d9600e8"),
            status(true, false, false, "pictures:8d9600e8"),
            status(false, false, true, "space:ab12cd"),
            View::offline(),
        ] {
            for row in rows(&view) {
                assert_eq!(
                    row.line().ends_with(" (disabled)"),
                    row.is_control() && !row.enabled,
                    "{}",
                    row.line()
                );
            }
        }
    }

    #[test]
    fn a_paused_daemon_says_resume_and_everything_else_is_unchanged() {
        let view = status(true, false, false, "pictures:8d9600e8");
        assert_eq!(
            shape(&view),
            vec![
                (Some(RowId::Now), false),
                (Some(RowId::Next), true),
                (Some(RowId::Previous), true),
                (Some(RowId::Resume), true),
                (Some(RowId::Favourite), true),
                (None, false),
                (Some(RowId::Settings), true),
                (Some(RowId::Quit), true),
            ]
        );
        let labels: Vec<String> = rows(&view).iter().map(|row| row.label.clone()).collect();
        assert_eq!(
            labels,
            vec![
                "pictures:8d9600e8",
                "Next",
                "Previous",
                "Resume",
                "Favourite",
                "",
                "Settings…",
                "Quit"
            ]
        );
        assert!(!labels.contains(&"Pause".to_string()));
    }

    #[test]
    fn no_daemon_disables_the_rows_that_need_one_and_offers_to_start_it() {
        let view = View::offline();
        assert_eq!(
            shape(&view),
            vec![
                (Some(RowId::Now), false),
                (Some(RowId::StartDaemon), true),
                (Some(RowId::Next), false),
                (Some(RowId::Previous), false),
                (Some(RowId::Pause), false),
                (Some(RowId::Favourite), false),
                (None, false),
                (Some(RowId::Settings), true),
                (Some(RowId::Quit), true),
            ]
        );
        assert_eq!(
            lines(&view),
            vec![
                "the daemon is not running",
                "Start whirl",
                "Next (disabled)",
                "Previous (disabled)",
                "Pause (disabled)",
                "Favourite (disabled)",
                "---",
                "Settings…",
                "Quit",
            ]
        );
    }

    #[test]
    fn a_reachable_daemon_is_offered_no_start_row() {
        // The offer is for a daemon that is not answering: when it answers there
        // is nothing to start and no row to start it with.
        for view in [
            status(false, true, false, "pictures:8d9600e8"),
            status(true, false, false, "pictures:8d9600e8"),
        ] {
            let ids: Vec<Option<RowId>> = rows(&view).iter().map(|row| row.id).collect();
            assert!(!ids.contains(&Some(RowId::StartDaemon)), "{ids:?}");
        }
    }

    #[test]
    fn the_apps_own_report_is_a_line_under_the_image_line() {
        let view = View::offline();
        let reporting = rows_reporting(&view, Some("whirl: no unit at /x"));
        assert_eq!(reporting[1].id, Some(RowId::Report));
        assert_eq!(reporting[1].label, "whirl: no unit at /x");
        assert!(!reporting[1].is_control(), "the report is a line");
        assert_eq!(
            reporting[1].line(),
            "whirl: no unit at /x",
            "a line is never marked disabled"
        );
        // With nothing to report the rows are the menu's own.
        assert_eq!(rows_reporting(&view, None), rows(&view));
    }

    #[test]
    fn a_degraded_daemon_hides_the_pin_rather_than_offering_one_that_fails() {
        let view = status(false, false, true, "space:ab12cd");
        let ids: Vec<Option<RowId>> = rows(&view).iter().map(|row| row.id).collect();
        assert!(!ids.contains(&Some(RowId::Favourite)), "{ids:?}");
        // Everything else is where it was, so the row is hidden and not merely
        // re-ordered.
        assert_eq!(
            ids,
            vec![
                Some(RowId::Now),
                Some(RowId::Next),
                Some(RowId::Previous),
                Some(RowId::Pause),
                None,
                Some(RowId::Settings),
                Some(RowId::Quit),
            ]
        );
    }

    #[test]
    fn every_row_id_round_trips_through_its_key() {
        for id in RowId::ALL {
            assert_eq!(RowId::from_key(id.key()), Some(id), "{}", id.key());
        }
        assert_eq!(RowId::from_key("sep"), None);
        assert_eq!(RowId::from_key(""), None);
    }

    #[test]
    fn the_two_line_rows_are_the_only_ones_with_no_action() {
        for id in RowId::ALL {
            assert_eq!(
                id.action().is_none(),
                matches!(id, RowId::Now | RowId::Report),
                "{}",
                id.key()
            );
        }
    }

    #[test]
    fn the_dump_has_one_line_per_row() {
        let view = status(false, false, false, "space:ab12cd");
        assert_eq!(lines(&view).len(), rows(&view).len());
        assert_eq!(lines(&View::offline()).len(), rows(&View::offline()).len());
    }
}
