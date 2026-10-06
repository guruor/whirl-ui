//! The settings window, and the app state that owns it.
//!
//! The window is a dialog over the app, and the difference between the two is the
//! whole of this module:
//!
//! - **Opening it starts nothing.** [`App::open_settings`] takes a window that was
//!   already read, so pressing the menu's `Settings…` row cannot start a daemon, a
//!   worker or a rotation. The reads are `config path` and `config check` plus the
//!   config file itself, and they happen before the window exists.
//! - **Closing it does not quit the app.** The window's close button is answered
//!   with [`egui::ViewportCommand::CancelClose`] and the viewport is hidden; the
//!   app stays up, which is what the menu bar item needs: the window behind its
//!   `Settings…` row is this one, drawn by [`panes`] and closed by this rule.
//! - **The app works with it closed.** With no window the app runs no egui pass
//!   and draws nothing, and [`App::running`] is still true.
//!
//! What is drawn is [`crate::settings`]' two choices: where the wallpapers come
//! from and how often they change. This module holds no wording of its own that a
//! person reads twice: every heading, button, line and refusal comes from a
//! constant or a method on the value in `settings`, so the window on screen and
//! the text `--dump-settings` prints cannot drift apart. The window's *shape* is
//! [`crate::theme`]'s: the sidebar, the cards, the rows, the toggles and the
//! footer are drawn in the reference's palette and metrics,
//! and nothing here picks a colour or a size of its own.
//!
//! Nothing here opens a socket to change a setting, so no part of an edit is a
//! daemon verb, and the other file this crate can write is still the screenshot a
//! maintainer asks for on the command line.

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Stroke,
};

use crate::about;
use crate::settings::{
    self, ABOUT_LINE, ABOUT_TITLE, ADVANCED_LINES, ADVANCED_TITLE, CHECK_AGAIN, CHECK_LABEL,
    CHECK_LINE, CHECK_TITLE, COLLECTION_LINE, COLLECTION_TITLE, COLLECTION_TOKEN_NOTE,
    CONFIRM_REMOVE_HELPER, CONFIRM_REMOVE_WHIRL, CONTROL_PANEL_LINE, CONTROL_PANEL_TITLE, Checks,
    Daemon, HELPER_LINE, HELPER_TITLE, KEY_LINE, Kind, LAUNCH_LINE, LAUNCH_TITLE, LOGIN_ITEMS,
    NO_SOURCES, OPEN_AT_LOGIN, OPEN_AT_LOGIN_LINE, PATH_ROWS, PATHS_TITLE, PICKER_TITLE, Pane,
    REMOVE_HELPER, REMOVE_WHIRL, REMOVE_WHIRL_LINE, REMOVE_WHIRL_ROUTE, ROTATION_LINE,
    ROTATION_TITLE, SOURCES_LINE, SOURCES_TITLE, START_NOW, SUBTITLE, Settings, SystemPanel, Tone,
    Unit, WINDOW_TITLE,
};
use crate::state::View;
use crate::theme;

// The window's size and title are the settings module's, because the text dump
// carries the title too and the two may not drift. Whoever opens the window
// names them where they live, which is the settings module.

/// The footer's word for a daemon that answered, and for one that did not.
///
/// The pane's own line is the daemon's sentence, in full, and it is drawn there;
/// the footer is a status light and a word, which is what the reference's footer
/// is. These two words are the footer's and appear nowhere the text dump reads.
const CONNECTED: &str = "Connected";
const NOT_CONNECTED: &str = "Not connected";

/// The reason the window's line carries for a daemon that stopped answering
/// after the window was opened.
///
/// The reason the window was opened with is the client's own, and it is kept
/// while the connection is unchanged ([`App::set_daemon`]); this is only for the
/// change the view reports and the client never named to this window.
const DAEMON_LOST: &str = "the daemon stopped answering";

/// The app: whatever it knows, and which windows are open.
pub struct App {
    /// The dialog's content while the dialog is open. `None` is the window
    /// closed, which is a state the app sits in rather than an exit.
    settings: Option<Settings>,
    /// Set by the menu bar item's `Quit` row. Nothing else ends the app.
    quit: bool,
    /// Where a screenshot of the window goes, when one was asked for.
    capture: Option<PathBuf>,
    /// Whether the screenshot has been asked for yet.
    requested: bool,
    /// The pane the last pass drew, so the pane that opens this pass is a change
    /// the two switches can be read on.
    shown: Option<Pane>,
    /// Whether the window was focused on the last pass, so a focus arriving is a
    /// change too.
    focused: bool,
}

impl App {
    /// The app with its settings window closed, which is where the menu bar item
    /// starts: its `Settings…` row is what opens the window.
    ///
    /// Closed is a state rather than an exit, so this is a whole app: a menu bar
    /// item with its dialog put away.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // The tray starts here; the tray is macOS-only.
    pub fn closed() -> App {
        App {
            settings: None,
            quit: false,
            capture: None,
            requested: false,
            shown: None,
            focused: false,
        }
    }

    /// The app with its settings window open on this state.
    pub fn new(settings: Settings) -> App {
        App {
            settings: Some(settings),
            ..App::closed()
        }
    }

    /// Also write the window to `path`, once, and then exit.
    ///
    /// A window is the one thing a test cannot open, so the screenshot is how the
    /// window becomes evidence. It is the window's own pixels rather than the
    /// screen's: a screen capture would carry whatever else the machine was
    /// showing.
    pub fn capturing(mut self, path: PathBuf) -> App {
        self.capture = Some(path);
        self
    }

    /// Show the dialog on this state.
    ///
    /// The state was read before the call, so this only records that it is on
    /// screen. Opening starts nothing.
    ///
    /// The menu bar item's `Settings…` row calls this: it reads the state on its
    /// command thread and opens the window from the UI thread, so the reading is
    /// not done here and cannot block a draw. On the other two CI legs the tray
    /// is not compiled, which is what the `allow` covers.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn open_settings(&mut self, settings: Settings) {
        self.settings = Some(settings);
    }

    /// Close the dialog. The app keeps running.
    pub fn close_settings(&mut self) {
        self.settings = None;
    }

    /// Point the window's own status line at the app's live view.
    ///
    /// The window is a dialog opened on a snapshot of the daemon's state, and
    /// nothing here re-reads it: the tray is what follows the daemon after that,
    /// and this is how its one view reaches the window. Only a change of what
    /// the line says moves it, so the reason the window was opened with is not
    /// replaced by a plainer one on the next pass, and a view that says what the
    /// window already says changes nothing: a daemon that answered and was
    /// paused moves the line to the paused form, exactly as one that stopped
    /// answering moves it to the not-running form.
    ///
    /// The tray is the only caller and it is macOS-only, which is what the
    /// `allow` covers.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn set_daemon(&mut self, view: &View) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        if view.reachable() {
            // The daemon answered: the line follows whether its schedule is
            // paused, which is the same bit the mark reads.
            let answered = if view.paused() {
                Daemon::Paused
            } else {
                Daemon::Running
            };
            if settings.daemon != answered {
                settings.daemon = answered;
            }
        } else if settings.daemon.answered() {
            // The reason the window was opened with stays while the daemon is
            // already not answering; only a change replaces it with the one this
            // window names itself.
            settings.daemon = Daemon::NotRunning(DAEMON_LOST.to_string());
        }
    }

    /// Whether the dialog is on screen. Closing it is a state the app sits in
    /// rather than an exit, so this is how a caller tells the two apart, and the
    /// tray is one: the window is on screen exactly while this is true
    /// (`crate::tray`, `App::state_window`).
    #[allow(dead_code)] // The tray is the only caller, and the tray is macOS-only.
    pub fn open(&self) -> bool {
        self.settings.is_some()
    }

    /// The window the dialog is showing, or `None` while it is closed. The tests
    /// read it; the window draws the same value through
    /// [`App::ui`](eframe::App::ui).
    #[cfg(test)]
    pub fn settings(&self) -> Option<&Settings> {
        self.settings.as_ref()
    }

    /// End the app. The menu bar item's `Quit` row calls this, and the close it
    /// sends next is the app's rather than the window's: `logic` below does not
    /// answer a close as the window's once `quit` is set.
    #[allow(dead_code)] // Callers on macOS, tests elsewhere.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// Whether the app is up. It is up with its window closed, which is the point.
    #[allow(dead_code)] // The tests read it; the tray's Quit is what makes it false.
    pub fn running(&self) -> bool {
        !self.quit
    }

    /// Whether a requested close is the window's close rather than the app's.
    ///
    /// A value rather than a condition inside `logic`, because this is the whole
    /// of the rule that keeps the close button from quitting the app, and the
    /// whole of what keeps a `Quit` from being answered as the window's.
    ///
    /// Three closes are not the window's:
    ///
    /// - one taken while a screenshot is being written: the capture asks for its
    ///   own close once the pixels are on disk, and cancelling that would leave
    ///   the app hidden with no window and no way back;
    /// - one that follows [`App::quit`], which is the menu bar item's `Quit` row
    ///   ending the app: answering it as the window's would leave the app up with
    ///   its dialog shut, the opposite of what was clicked;
    /// - one with no window open, which is not a close at all.
    fn window_close(&self, close_requested: bool) -> bool {
        self.capture.is_none() && !self.quit && self.settings.is_some() && close_requested
    }

    /// Ask for the window's pixels, and write them once they arrive.
    fn capture_window(&mut self, ctx: &egui::Context) {
        let Some(path) = self.capture.clone() else {
            return;
        };
        if !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            return;
        }
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else {
            // The reply lands a frame or two after the request.
            ctx.request_repaint();
            return;
        };
        match write_png(&path, &image) {
            Ok(()) => println!("whirl-ui: wrote the settings window to {}", path.display()),
            Err(error) => eprintln!("whirl-ui: cannot write {}: {error}", path.display()),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The window's close button closes the window. It is a dialog over the
        // app, so the app is not what it closes: cancel the close and hide the
        // viewport. With no window the app runs on, which is the state the menu
        // bar item needs to be useful in. Which closes are the window's is the
        // whole of [`App::window_close`].
        if self.window_close(ctx.input(|input| input.viewport().close_requested())) {
            self.close_settings();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.capture_window(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let Some(settings) = self.settings.as_mut() else {
            return;
        };
        // The two switches are states and never a cache: another client can change
        // the unit at any moment, so the Background Helper's pane reads
        // `whirl daemon status` when it is shown and again whenever the window
        // gains focus, and the Control Panel's pane reads macOS's login item with
        // the same rule. The pane the last pass drew is what makes "shown" a
        // change rather than every frame, and the read happens before this pass
        // draws the pane it belongs to.
        let focused = ui.ctx().input(|input| input.focused);
        let opened = self.shown != Some(settings.pane);
        if opened || (focused && !self.focused) {
            match settings.pane {
                Pane::BackgroundHelper => settings.read_helper(),
                Pane::ControlPanel => settings.read_login(),
                Pane::Sources | Pane::Rotation | Pane::About => {}
            }
        }
        self.shown = Some(settings.pane);
        self.focused = focused;
        panes(ui, settings);
    }
}

/// Draw the window: the app's mark and its five panes, on one rail.
///
/// The menu bar item's `Settings…` row and the standalone window are the same
/// window, and this is what makes them the same: one body, drawn from one
/// [`Settings`] value, so a control cannot be in one window and missing from the
/// other. The tray owns the viewport and this owns what is in it.
///
/// The shape is the reference's: a sidebar carrying the mark, the wordmark, the
/// pane rows and a status footer, and a centre panel carrying the pane's title
/// and the pane itself. The pane list is the sidebar's alone: the centre panel's
/// top strip is reserved for a pane's own sub-views and carries no control over
/// the panes (see [`header`]). The five panes are the five blocks [`Settings`]
/// has, and the sidebar lists exactly them rather than the reference's future
/// sections.
///
/// While the folder chooser is open it is the whole centre panel: the choice it
/// is making is the only thing on screen, and a half-drawn list of folders under
/// a half-drawn list of sources is how a person picks the wrong one.
pub(crate) fn panes(ui: &mut egui::Ui, settings: &mut Settings) {
    sidebar(ui, settings);
    egui::CentralPanel::no_frame()
        .frame(
            egui::Frame::NONE
                .fill(theme::CANVAS)
                .inner_margin(Margin::symmetric(28, 22)),
        )
        .show(ui, |ui| {
            if settings.picker.is_some() {
                picker(ui, settings);
                return;
            }
            header(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match settings.pane {
                    Pane::Sources => sources(ui, settings),
                    Pane::Rotation => rotation(ui, settings),
                    Pane::BackgroundHelper => background_helper(ui, settings),
                    Pane::ControlPanel => control_panel(ui, settings),
                    Pane::About => about_pane(ui, settings),
                });
        });
}

/// The sidebar: the mark, the app's name, one row per pane, and the footer.
///
/// The footer is pinned to the bottom so the status light sits where the
/// reference's does whether the window is tall or short.
fn sidebar(ui: &mut egui::Ui, settings: &mut Settings) {
    let frame = egui::Frame::NONE
        .fill(theme::CANVAS)
        .inner_margin(Margin::symmetric(14, 18));
    egui::Panel::left("whirl-panes")
        .exact_size(theme::SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::mark(ui, theme::MARK, theme::ACCENT);
                ui.add_space(theme::SPACE_SM);
                ui.label(
                    RichText::new("whirl")
                        .size(theme::TEXT_WORDMARK)
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );
            });
            ui.add_space(theme::SPACE_LG);

            let mut wanted = None;
            for pane in Pane::ALL {
                if nav_row(ui, pane, pane == settings.pane) {
                    wanted = Some(pane);
                }
            }
            if let Some(pane) = wanted {
                settings.pane = pane;
            }

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                footer(ui, settings);
            });
        });
}

/// One row of the sidebar: the pane's name, lifted when it is the one on screen.
fn nav_row(ui: &mut egui::Ui, pane: Pane, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme::CONTROL_HEIGHT + 6.0),
        egui::Sense::click(),
    );
    let radius = CornerRadius::same(theme::RADIUS_ROW);
    if active {
        ui.painter().rect_filled(rect, radius, theme::ACCENT);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, radius, theme::PANEL);
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        pane.name(),
        FontId::proportional(theme::TEXT_BODY),
        if active {
            Color32::WHITE
        } else {
            theme::TEXT_SECONDARY
        },
    );
    response.clicked()
}

/// The footer: a status light, the connection state in a word, and the version.
fn footer(ui: &mut egui::Ui, settings: &Settings) {
    let connected = settings.daemon.answered();
    ui.add_space(theme::SPACE_MD);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(
            rect.center(),
            4.5,
            if connected {
                theme::STATE_OK
            } else {
                theme::STATE_BAD
            },
        );
        ui.label(
            RichText::new(if connected { CONNECTED } else { NOT_CONNECTED })
                .size(theme::TEXT_CAPTION)
                .color(theme::TEXT_SECONDARY),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("v{}", settings.running_version()))
                    .size(theme::TEXT_CAPTION)
                    .color(theme::TEXT_MUTED),
            );
        });
    });
}

/// The pane's title, its subtitle, and the strip below them that is reserved
/// for a pane's own sub-views.
///
/// **The top strip is for a pane's sub-views, never for the pane list.** A
/// segmented control over [`Pane`] was drawn here once, beside the sidebar's
/// rows, and the two controls did the same job in two languages; the sidebar is
/// the pane list and this strip is for what a pane grows *inside* itself. A pane
/// that gains sub-views (tabs within Sources, modes within Rotation) puts its
/// control here, and it moves between that pane's views and never between panes.
/// Adding a pane is a row in [`sidebar`], not a second control here.
fn header(ui: &mut egui::Ui) {
    ui.label(
        RichText::new(WINDOW_TITLE)
            .size(theme::TEXT_TITLE)
            .strong()
            .color(theme::TEXT_PRIMARY),
    );
    ui.label(
        RichText::new(SUBTITLE)
            .size(theme::TEXT_BODY)
            .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(theme::SPACE_MD);
}

/// One raised surface in the palette: the card everything else is drawn in.
fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    surface(ui, theme::HAIRLINE, add)
}

/// A card whose outline is a colour rather than the hairline, for a state.
fn surface<R>(ui: &mut egui::Ui, outline: Color32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .stroke(Stroke::new(1.0, outline))
        .inner_margin(theme::CARD_MARGIN)
        .show(ui, add)
        .inner
}

/// A card's heading.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .size(theme::TEXT_SECTION)
            .strong()
            .color(theme::TEXT_PRIMARY),
    );
}

/// A card's note, in the muted text the reference uses for one.
fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(theme::TEXT_CAPTION)
            .color(theme::TEXT_MUTED),
    );
}

/// A line the window is reporting: a save, a refusal, the rotation in use.
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(theme::TEXT_BODY)
            .color(theme::TEXT_SECONDARY),
    );
}

/// A state the person needs to read as a state: empty, or refused.
fn banner(ui: &mut egui::Ui, text: &str, colour: Color32) {
    egui::Frame::NONE
        .fill(colour.gamma_multiply(0.22))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(theme::TEXT_BODY)
                    .color(theme::TEXT_PRIMARY),
            );
        });
}

/// The primary button: the accent fill the reference gives the one action that
/// moves the window on.
fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(theme::TEXT_BODY)
                .color(Color32::WHITE),
        )
        .fill(theme::ACCENT)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM)),
    )
}

/// A toggle switch: a track and a knob, on when `on` is set.
///
/// Drawn rather than taken from egui's checkbox because the reference's control
/// is a switch, and a checkbox here would be a different control wearing the
/// same value. It reports the change the way a click does, so the caller runs
/// the same method it ran for the checkbox.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(theme::TOGGLE_WIDTH, theme::TOGGLE_HEIGHT),
        egui::Sense::click(),
    );
    let radius = theme::TOGGLE_HEIGHT / 2.0;
    let corner = CornerRadius::same(radius as u8);
    ui.painter().rect_filled(
        rect,
        corner,
        if *on { theme::ACCENT } else { theme::HAIRLINE },
    );
    let knob = if *on {
        rect.right() - radius
    } else {
        rect.left() + radius
    };
    ui.painter().circle_filled(
        egui::pos2(knob, rect.center().y),
        radius - 2.0,
        Color32::WHITE,
    );
    if response.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

/// One click, recorded while the widgets are drawn and applied afterwards.
///
/// Recording rather than acting inside the drawing is what lets a button sit
/// inside the loop over the rows it acts on, and it is why every button on screen
/// has a method behind it: the `match` at the end of [`sources`] is exhaustive.
enum Click {
    Toggle(String, bool),
    Change(String),
    Key,
    ChangeUrl(String),
    Remove(String),
    AddFolder,
    AddWallhaven,
    SaveKey,
    CancelKey,
    SaveCollection,
    CancelCollection,
    PickerUp,
    PickerInto(PathBuf),
    PickerChoose,
    PickerCancel,
}

/// The Sources pane: one card of rows, and the two ways to add one.
fn sources(ui: &mut egui::Ui, settings: &mut Settings) {
    let mut clicks: Vec<Click> = Vec::new();

    card(ui, |ui| {
        section(ui, SOURCES_TITLE);
        ui.add_space(theme::SPACE_SM);

        match &settings.sources.problem {
            // The file itself could not be read: an error state, drawn as one.
            Some(reason) => {
                banner(ui, reason.as_str(), theme::STATE_BAD);
                ui.add_space(theme::SPACE_SM);
            }
            // A file with no sources yet: the reference's empty state.
            None if settings.sources.rows.is_empty() => {
                empty_state(ui, NO_SOURCES);
                ui.add_space(theme::SPACE_SM);
            }
            None => {
                for row in &settings.sources.rows {
                    row_card(ui, row, &settings.sources.checks, &mut clicks);
                    ui.add_space(theme::SPACE_SM);
                }
            }
        }

        ui.horizontal(|ui| {
            if primary_button(ui, "Add a folder…").clicked() {
                clicks.push(Click::AddFolder);
            }
            if ui.button("Add Wallhaven").clicked() {
                clicks.push(Click::AddWallhaven);
            }
        });
        ui.add_space(theme::SPACE_XS);
        caption(ui, SOURCES_LINE);
        if let Some(outcome) = &settings.sources.outcome {
            ui.add_space(theme::SPACE_XS);
            note(ui, outcome.line().as_str());
        }
    });

    if settings.key.open {
        ui.add_space(theme::SPACE_MD);
        card(ui, |ui| {
            section(ui, "Wallhaven key");
            ui.add_space(theme::SPACE_SM);
            ui.horizontal(|ui| {
                // Masked, and the value only ever goes to `save_key`, which hands
                // it to the platform's store. Nothing reads it back, and nothing
                // draws it.
                ui.add(
                    egui::TextEdit::singleline(&mut settings.key.token)
                        .password(true)
                        .desired_width(260.0),
                );
                if primary_button(ui, "Save key").clicked() {
                    clicks.push(Click::SaveKey);
                }
                if ui.button("Cancel").clicked() {
                    clicks.push(Click::CancelKey);
                }
            });
            ui.add_space(theme::SPACE_XS);
            caption(ui, KEY_LINE);
        });
    }

    if settings.collection.open {
        ui.add_space(theme::SPACE_MD);
        card(ui, |ui| {
            section(ui, COLLECTION_TITLE);
            ui.add_space(theme::SPACE_SM);
            ui.horizontal(|ui| {
                // A collection's address is not a secret, so it is drawn as
                // typed: it is what the person pasted, and seeing it is how they
                // check it before saving.
                ui.add(
                    egui::TextEdit::singleline(&mut settings.collection.url).desired_width(360.0),
                );
                if primary_button(ui, "Save").clicked() {
                    clicks.push(Click::SaveCollection);
                }
                if ui.button("Cancel").clicked() {
                    clicks.push(Click::CancelCollection);
                }
            });
            ui.add_space(theme::SPACE_XS);
            caption(ui, COLLECTION_LINE);
            caption(ui, COLLECTION_TOKEN_NOTE);
            if let Some(problem) = &settings.collection.problem {
                ui.add_space(theme::SPACE_XS);
                banner(ui, problem.as_str(), theme::STATE_BAD);
            }
        });
    }

    for click in clicks {
        match click {
            Click::Toggle(id, enabled) => settings.set_source_enabled(&id, enabled),
            Click::Change(id) => settings.choose_folder(Some(id), &SystemPanel),
            Click::Key => settings.key.open = true,
            Click::ChangeUrl(id) => settings.edit_collection(&id),
            Click::Remove(id) => settings.remove_source(&id),
            Click::AddFolder => settings.choose_folder(None, &SystemPanel),
            Click::AddWallhaven => settings.ask_for_collection(),
            Click::SaveKey => settings.save_key(),
            Click::CancelKey => settings.key.open = false,
            Click::SaveCollection => settings.save_collection(),
            Click::CancelCollection => settings.cancel_collection(),
            // The chooser's clicks are recorded by `picker`, which is the only
            // place they can be made.
            Click::PickerUp | Click::PickerInto(_) | Click::PickerChoose | Click::PickerCancel => {}
        }
    }
}

/// The empty Sources pane: the mark, and the one line that says nothing is here.
fn empty_state(ui: &mut egui::Ui, text: &str) {
    ui.vertical_centered(|ui| {
        theme::mark(ui, 44.0, theme::HAIRLINE);
        ui.add_space(theme::SPACE_SM);
        ui.label(
            RichText::new(text)
                .size(theme::TEXT_BODY)
                .color(theme::TEXT_SECONDARY),
        );
    });
}

/// One source row: its toggle, its line, whether whirl can use it, and the
/// controls that act on it.
fn row_card(
    ui: &mut egui::Ui,
    row: &crate::settings::Row,
    checks: &Checks,
    clicks: &mut Vec<Click>,
) {
    egui::Frame::NONE
        .fill(theme::CANVAS)
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // The switch is the one per-row control: a source can be left in
                // the file and out of the rotation, and a window that hid that
                // would be describing a rotation the daemon is not running.
                let mut enabled = row.enabled;
                if toggle(ui, &mut enabled) {
                    clicks.push(Click::Toggle(row.id.clone(), enabled));
                }
                ui.add_space(theme::SPACE_SM);
                ui.label(
                    RichText::new(row.line())
                        .size(theme::TEXT_BODY)
                        .color(theme::TEXT_PRIMARY),
                );
                // This side is laid out right to left, so the widgets are added
                // in reverse: the one written here last is drawn leftmost, which
                // is where `--dump-settings` prints `[Change…] [Remove]` too. The
                // two orders agree, so a person comparing the window with the
                // dump is reading the same row.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Remove").clicked() {
                        clicks.push(Click::Remove(row.id.clone()));
                    }
                    if matches!(row.kind, Kind::Wallhaven { .. })
                        && ui.button("Enter key…").clicked()
                    {
                        clicks.push(Click::Key);
                    }
                    if matches!(row.kind, Kind::Wallhaven { .. })
                        && ui.button("Change URL…").clicked()
                    {
                        clicks.push(Click::ChangeUrl(row.id.clone()));
                    }
                    if row.changeable_folder().is_some() && ui.button("Change…").clicked() {
                        clicks.push(Click::Change(row.id.clone()));
                    }
                });
            });
            // The row's own answer, under the controls: whether whirl can use
            // this source, and what to do when it cannot. It is indented to
            // where the row's own line starts, so the pair reads as one row
            // rather than as two.
            if let Some(state) = row.state_line(checks) {
                ui.add_space(theme::SPACE_XS);
                ui.horizontal(|ui| {
                    ui.add_space(
                        theme::TOGGLE_WIDTH + ui.spacing().item_spacing.x + theme::SPACE_SM,
                    );
                    ui.label(
                        RichText::new(state.phrase)
                            .size(theme::TEXT_CAPTION)
                            .color(tone_colour(state.tone)),
                    );
                });
            }
        });
}

/// The colour a row's answer is drawn in: the window's own colours for a state.
///
/// The tone belongs to the answer, not to the widget, so the mapping lives here
/// with the rest of the drawing rather than in the module that builds the words.
fn tone_colour(tone: Tone) -> Color32 {
    match tone {
        Tone::Good => theme::STATE_OK,
        Tone::Bad => theme::STATE_BAD,
        Tone::Unknown => theme::TEXT_MUTED,
    }
}

/// The Rotation pane: how long each wallpaper stays, as a number and a unit.
fn rotation(ui: &mut egui::Ui, settings: &mut Settings) {
    let mut save = false;
    card(ui, |ui| {
        section(ui, ROTATION_TITLE);
        ui.add_space(theme::SPACE_SM);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Every")
                    .size(theme::TEXT_BODY)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add(egui::TextEdit::singleline(&mut settings.interval.value).desired_width(72.0));
            egui::ComboBox::from_id_salt("rotation-unit")
                .selected_text(settings.interval.unit.name())
                .show_ui(ui, |ui| {
                    for unit in Unit::ALL {
                        ui.selectable_value(&mut settings.interval.unit, unit, unit.name());
                    }
                });
            save = primary_button(ui, "Save").clicked();
        });
        ui.add_space(theme::SPACE_SM);
        caption(ui, ROTATION_LINE);
        if let Some(outcome) = &settings.interval.outcome {
            ui.add_space(theme::SPACE_XS);
            note(ui, outcome.line().as_str());
        }
        if let Some(now) = settings.interval.in_use_phrase() {
            // Its own paragraph: the note above it already ends in `does not change
            // yet`, and a reader who takes the two for one sentence reads the window
            // as contradicting itself.
            ui.add_space(theme::SPACE_XS);
            note(ui, now.as_str());
        }
    });
    if save {
        settings.save_interval();
    }
}

/// The Background Helper pane: the part of Whirl that keeps running after this
/// window closes, and the one switch that decides whether it comes back at login.
///
/// The switch is drawn from [`Settings::helper`], which is a state read from
/// Whirl's own `status`, and never from what a click asked for: a click that was
/// refused leaves it where the unit actually is, because every control here calls
/// a method that re-reads afterwards. The read itself belongs to [`App::ui`] and
/// happens when the pane opens and when the window is focused.
///
/// The two groups are collapsed, and which of them is open is [`Settings`]'s
/// value rather than egui's, so the window and `--dump-settings` cannot disagree
/// about what is on screen.
fn background_helper(ui: &mut egui::Ui, settings: &mut Settings) {
    let position = settings.launch_position();
    let readable = position.is_some();
    let mut on = position.unwrap_or(false);
    let state_line = settings.launch_state_line();
    let version = settings.helper_version_line();
    let action = settings.helper_action.clone();
    let start_now = settings
        .helper
        .as_ref()
        .is_some_and(settings::Helper::start_now);
    let confirming = settings.confirming == Some(settings::Removal::Helper);
    let mut launch = None;
    let mut start = false;
    let mut again = false;
    let mut remove = false;

    surface(ui, theme::HAIRLINE, |ui| {
        section(ui, HELPER_TITLE);
        ui.add_space(theme::SPACE_XS);
        note(ui, HELPER_LINE);
        ui.add_space(theme::SPACE_MD);
        ui.horizontal(|ui| {
            // A state that could not be read has no position to offer, so the
            // control is drawn disabled rather than at a guessed side.
            ui.add_enabled_ui(readable, |ui| {
                if toggle(ui, &mut on) {
                    launch = Some(on);
                }
            });
            ui.add_space(theme::SPACE_SM);
            ui.label(
                RichText::new(LAUNCH_TITLE)
                    .size(theme::TEXT_BODY)
                    .color(theme::TEXT_PRIMARY),
            );
        });
        ui.add_space(theme::SPACE_XS);
        caption(ui, LAUNCH_LINE);
        ui.add_space(theme::SPACE_SM);
        note(ui, state_line.as_str());
        if start_now {
            // The middle state alone, and beside the status line it belongs to: a
            // stopped unit is not a state a switch can show.
            ui.add_space(theme::SPACE_SM);
            start = primary_button(ui, START_NOW).clicked();
        }
        if !readable {
            ui.add_space(theme::SPACE_SM);
            again = primary_button(ui, CHECK_AGAIN).clicked();
        }
        if let Some(line) = &action {
            ui.add_space(theme::SPACE_XS);
            note(ui, line);
        }
        ui.add_space(theme::SPACE_SM);
        caption(ui, version.as_str());
    });

    ui.add_space(theme::SPACE_MD);

    let mut paths_open = settings.paths_open;
    group(ui, PATHS_TITLE, &mut paths_open, |ui| {
        for row in PATH_ROWS {
            caption(ui, row);
        }
        ui.add_space(theme::SPACE_XS);
        if primary_button(ui, LOGIN_ITEMS).clicked() {
            open_login_items();
        }
    });
    settings.paths_open = paths_open;

    ui.add_space(theme::SPACE_SM);

    let mut advanced_open = settings.advanced_open;
    group(ui, ADVANCED_TITLE, &mut advanced_open, |ui| {
        for line in ADVANCED_LINES {
            caption(ui, line);
        }
    });
    settings.advanced_open = advanced_open;

    ui.add_space(theme::SPACE_MD);

    card(ui, |ui| {
        if confirming {
            banner(ui, CONFIRM_REMOVE_HELPER, theme::STATE_BAD);
            ui.add_space(theme::SPACE_SM);
        }
        remove = primary_button(ui, REMOVE_HELPER).clicked();
    });

    // Every one of these is a method, and the three that ask Whirl for something
    // re-read afterwards: the switch never moves because a click said so.
    if let Some(on) = launch {
        settings.set_launch(on);
    }
    if start {
        settings.start_now();
    }
    if again {
        settings.read_helper();
    }
    if remove {
        // The first press asks and the second is the removal, so a stray click
        // cannot take the unit away.
        if confirming {
            settings.confirming = None;
            settings.remove_helper();
        } else {
            settings.confirming = Some(settings::Removal::Helper);
        }
    }
}

/// The Control Panel pane: this window's own login item, and nothing else.
///
/// The row is [`crate::login_item`]'s and unchanged: what is drawn beside it is
/// macOS's own answer, read when the pane opens and when the window is focused
/// exactly as the Background Helper's switch is. The sub-line says the trade-off
/// in plain words, so the two login rows cannot be read as one setting in two
/// places.
///
/// There is no row for the menu bar icon: the icon exists exactly while this
/// window runs, so the login flags are the only switch and a second control would
/// say the same thing twice.
fn control_panel(ui: &mut egui::Ui, settings: &mut Settings) {
    let position = settings.login_position();
    let readable = position.is_some();
    let mut on = position.unwrap_or(false);
    let state_line = settings.login_state_line();
    let action = settings.login_action.clone();
    let mut asked = None;
    let mut again = false;

    surface(ui, theme::HAIRLINE, |ui| {
        section(ui, CONTROL_PANEL_TITLE);
        ui.add_space(theme::SPACE_XS);
        note(ui, CONTROL_PANEL_LINE);
        ui.add_space(theme::SPACE_MD);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(readable, |ui| {
                if toggle(ui, &mut on) {
                    asked = Some(on);
                }
            });
            ui.add_space(theme::SPACE_SM);
            ui.label(
                RichText::new(OPEN_AT_LOGIN)
                    .size(theme::TEXT_BODY)
                    .color(theme::TEXT_PRIMARY),
            );
        });
        ui.add_space(theme::SPACE_XS);
        caption(ui, OPEN_AT_LOGIN_LINE);
        ui.add_space(theme::SPACE_SM);
        note(ui, state_line.as_str());
        if !readable {
            ui.add_space(theme::SPACE_SM);
            again = primary_button(ui, CHECK_AGAIN).clicked();
        }
        if let Some(line) = &action {
            ui.add_space(theme::SPACE_XS);
            note(ui, line);
        }
    });

    if let Some(on) = asked {
        settings.set_login(on);
    }
    if again {
        settings.read_login();
    }
}

/// A collapsed group: its heading is the control and its body is drawn only while
/// it is open.
///
/// The open state is the caller's rather than egui's, so the window, the text
/// dump and a test all read the same value.
fn group(ui: &mut egui::Ui, title: &str, open: &mut bool, add: impl FnOnce(&mut egui::Ui)) {
    let arrow = if *open { "-" } else { "+" };
    let response = ui.add(
        egui::Button::new(
            RichText::new(format!("{arrow} {title}"))
                .size(theme::TEXT_CAPTION)
                .color(theme::TEXT_SECONDARY),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE),
    );
    if response.clicked() {
        *open = !*open;
    }
    if *open {
        ui.add_space(theme::SPACE_XS);
        add(ui);
    }
}

/// Open the Login Items pane of System Settings, where a person finishes a
/// registration macOS kept for them.
///
/// macOS's own route, opened with macOS's own `open`; a platform this app ships no
/// bundle for has nothing to open, so nothing happens there.
fn open_login_items() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.LoginItems-Settings.extension")
            .spawn();
    }
}

/// The About pane: what this app is, which build is running, where its source
/// is, a newer release, and the one removal row.
///
/// The release check is a button and not a timer: pressing it is the whole of
/// the trigger, and the answer is drawn under it. A check that could not be made
/// is drawn as a failure rather than as silence, because silence is what "up to
/// date" looks like.
///
/// The source is a selectable label so it can be copied into a report rather
/// than retyped from a screenshot.
///
/// The removal row does the Background Helper's removal and then says what it
/// cannot do: the app bundle stays where it is, because removing what is in the
/// installed location is a permission this window cannot ask for. The sentence
/// saying so is the same on every platform; a platform with its own route adds
/// one sentence and nothing more.
fn about_pane(ui: &mut egui::Ui, settings: &mut Settings) {
    let version_lines = settings.version_lines();
    let check = settings.check.clone();
    let confirming = settings.confirming == Some(settings::Removal::Whirl);
    let action = settings.remove_action.clone();
    let mut remove = false;

    surface(ui, theme::HAIRLINE, |ui| {
        section(ui, ABOUT_TITLE);
        ui.add_space(theme::SPACE_XS);
        note(ui, ABOUT_LINE);
        ui.add_space(theme::SPACE_SM);
        for line in &version_lines {
            caption(ui, line);
        }
        ui.add_space(theme::SPACE_SM);
        caption(ui, "source:");
        ui.add(
            egui::Label::new(
                RichText::new(about::SOURCE_URL)
                    .size(theme::TEXT_BODY)
                    .color(theme::ACCENT_HIGHLIGHT),
            )
            .selectable(true),
        );
    });

    ui.add_space(theme::SPACE_MD);

    card(ui, |ui| {
        section(ui, CHECK_TITLE);
        ui.add_space(theme::SPACE_SM);
        let pressed = primary_button(ui, CHECK_LABEL).clicked();
        ui.add_space(theme::SPACE_XS);
        caption(ui, CHECK_LINE);
        if let Some(check) = &check {
            ui.add_space(theme::SPACE_SM);
            match check {
                about::Check::CouldNot { .. } => {
                    banner(ui, check.line().as_str(), theme::STATE_BAD);
                }
                _ => note(ui, check.line().as_str()),
            }
        }
        // The whole of the button: one on-demand call, made where the person
        // pressed it. Nothing here is on a timer and nothing runs it twice.
        if pressed {
            settings.check_release();
        }
    });

    ui.add_space(theme::SPACE_MD);

    card(ui, |ui| {
        if confirming {
            banner(ui, CONFIRM_REMOVE_WHIRL, theme::STATE_BAD);
            ui.add_space(theme::SPACE_SM);
        }
        remove = primary_button(ui, REMOVE_WHIRL).clicked();
        ui.add_space(theme::SPACE_XS);
        caption(ui, REMOVE_WHIRL_LINE);
        if let Some(route) = REMOVE_WHIRL_ROUTE {
            caption(ui, route);
        }
        if let Some(line) = &action {
            ui.add_space(theme::SPACE_XS);
            note(ui, line);
        }
    });

    if remove {
        // The first press asks and the second is the removal, so a stray click
        // cannot take the Background Helper away.
        if confirming {
            settings.confirming = None;
            settings.remove_whirl();
        } else {
            settings.confirming = Some(settings::Removal::Whirl);
        }
    }
}

/// The drawn folder browser: the folders inside one folder, and the two ways out.
///
/// A folder is chosen from a list rather than typed, because a path typed into a
/// window is a path the person has to know and spell, and the one thing this
/// control is for is that they do not have to.
///
/// It is the fallback for the platform's own panel, not the primary control:
/// `Add a folder…` and `Change…` ask `settings::SystemPanel` first, and this is
/// what a run with no panel to show falls back to (see
/// [`Settings::choose_folder`]). Its own clicks are the [`Click`] arms recorded
/// below, and its wording and palette are unchanged.
fn picker(ui: &mut egui::Ui, settings: &mut Settings) {
    let Some(showing) = settings.picker.clone() else {
        return;
    };
    let mut clicks: Vec<Click> = Vec::new();
    card(ui, |ui| {
        section(ui, PICKER_TITLE);
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(showing.directory.display().to_string())
                .size(theme::TEXT_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_SM);
        ui.horizontal(|ui| {
            if ui.button("Up").clicked() {
                clicks.push(Click::PickerUp);
            }
            if primary_button(ui, "Use this folder").clicked() {
                clicks.push(Click::PickerChoose);
            }
            if ui.button("Cancel").clicked() {
                clicks.push(Click::PickerCancel);
            }
        });
        ui.add_space(theme::SPACE_SM);
        match &showing.problem {
            Some(reason) => {
                banner(ui, reason.as_str(), theme::STATE_BAD);
            }
            None if showing.entries.is_empty() => {
                caption(ui, "(no folders inside it)");
            }
            None => {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for entry in &showing.entries {
                            let name = entry
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            if ui.button(name).clicked() {
                                clicks.push(Click::PickerInto(entry.clone()));
                            }
                        }
                    });
            }
        }
    });

    for click in clicks {
        match click {
            Click::PickerUp => settings.picker_up(),
            Click::PickerInto(directory) => settings.picker_into(directory),
            Click::PickerChoose => settings.picker_choose(),
            Click::PickerCancel => settings.picker_cancel(),
            // The sections' clicks are recorded by `sources`, which is not on
            // screen while the chooser is.
            Click::Toggle(..)
            | Click::Change(..)
            | Click::Key
            | Click::ChangeUrl(..)
            | Click::Remove(..)
            | Click::AddFolder
            | Click::AddWallhaven
            | Click::SaveKey
            | Click::CancelKey
            | Click::SaveCollection
            | Click::CancelCollection => {}
        }
    }
}

/// Run the window until it is closed or the screenshot is written.
pub fn run(settings: Settings, capture: Option<PathBuf>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(settings::WINDOW_SIZE)
            .with_title(WINDOW_TITLE),
        ..Default::default()
    };
    let mut app = App::new(settings);
    if let Some(path) = capture {
        app = app.capturing(path);
    }
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|creation| {
            // The palette is installed before the first frame is drawn, on the
            // dark theme only; the light one is left exactly as egui ships it.
            theme::apply(&creation.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

/// Write one egui frame as a PNG: 8-bit RGBA, the window's own pixels.
fn write_png(path: &Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let [width, height] = image.size;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&pixel.to_array());
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        u32::try_from(width).unwrap_or(u32::MAX),
        u32::try_from(height).unwrap_or(u32::MAX),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(codec_error)?;
    writer.write_image_data(&bytes).map_err(codec_error)
}

/// The PNG encoder's failure, as an I/O error, so one type covers writing here.
fn codec_error(error: png::EncodingError) -> std::io::Error {
    std::io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Outcome;

    fn window() -> Settings {
        Settings::unreachable(
            "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)",
        )
    }

    /// Which verdict is drawn in which colour.
    ///
    /// This repeats a mapping the drawing owns, and it repeats it on purpose: a
    /// change to it is a change to what a person sees, and this is the check that
    /// notices. A verdict that needs acting on may not read like one that is
    /// fine.
    #[test]
    fn the_row_answer_is_drawn_in_the_colour_its_verdict_asks_for() {
        assert_eq!(tone_colour(Tone::Good), theme::STATE_OK);
        assert_eq!(tone_colour(Tone::Bad), theme::STATE_BAD);
        assert_eq!(tone_colour(Tone::Unknown), theme::TEXT_MUTED);
    }

    /// A live view, for the window's status line: the keys 2.10 prints, in its
    /// shape.
    fn running() -> View {
        View::live(whirlui_client::Status::from_lines(&[
            "daemon_version: whirl 0.1.0".to_string(),
            "protocol: 2".to_string(),
            "seq: 7".to_string(),
            "paused: 0".to_string(),
            "favorites_degraded: 0".to_string(),
            "last_origin_key: pictures:8d9600e8".to_string(),
            "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
        ]))
    }

    /// The same snapshot with the schedule suspended: what `View` holds after a
    /// `pause` the tray re-read.
    fn paused() -> View {
        View::live(whirlui_client::Status::from_lines(&[
            "daemon_version: whirl 0.1.0".to_string(),
            "protocol: 2".to_string(),
            "seq: 7".to_string(),
            "paused: 1".to_string(),
            "favorites_degraded: 0".to_string(),
            "last_origin_key: pictures:8d9600e8".to_string(),
            "source: pictures local weight=1 enabled=1 last=- reason=-".to_string(),
        ]))
    }

    #[test]
    fn a_pause_moves_the_windows_line_the_way_it_moves_the_mark() {
        // The tray re-reads the daemon on the connection that performed
        // `pause`/`resume`, so the view reaching the window is the paused one.
        // The line has to follow it: a window that keeps saying `running` while
        // the daemon is paused is the status lagging the daemon.
        let mut app = App::new(window());
        app.set_daemon(&running());
        assert_eq!(
            app.settings().expect("an open window").daemon.line(),
            "whirl is running"
        );

        app.set_daemon(&paused());
        assert_eq!(
            app.settings().expect("an open window").daemon.line(),
            "whirl is paused"
        );

        // The resume is the same move back, and a daemon that stops answering
        // still moves the line to the not-running form.
        app.set_daemon(&running());
        assert_eq!(
            app.settings().expect("an open window").daemon.line(),
            "whirl is running"
        );
        app.set_daemon(&View::offline());
        assert_eq!(
            app.settings().expect("an open window").daemon.line(),
            format!("whirl is not running: {DAEMON_LOST}")
        );
    }

    #[test]
    fn the_windows_status_line_follows_the_apps_view() {
        // The window is opened on a snapshot of the daemon's state, and the tray
        // is what follows the daemon after that. This is how a daemon that
        // answers, or goes away, moves the line: a window opened on `not
        // running` cannot keep saying so about a daemon that is up, and the
        // reverse.
        let mut app = App::new(window());
        assert!(
            matches!(
                app.settings().expect("an open window").daemon,
                Daemon::NotRunning(_)
            ),
            "the window opens on the snapshot it was read with"
        );

        app.set_daemon(&running());
        assert_eq!(
            app.settings().expect("an open window").daemon,
            Daemon::Running
        );

        app.set_daemon(&View::offline());
        assert_eq!(
            app.settings().expect("an open window").daemon.line(),
            format!("whirl is not running: {DAEMON_LOST}")
        );

        // A view that says what the window already says leaves the reason the
        // window was opened with alone, and a window with no dialog is not
        // touched at all.
        let mut quiet = App::new(window());
        let before = quiet.settings().expect("an open window").daemon.clone();
        quiet.set_daemon(&View::offline());
        assert_eq!(quiet.settings().expect("an open window").daemon, before);

        let mut closed = App::closed();
        closed.set_daemon(&View::offline());
        assert!(closed.settings().is_none());
    }

    #[test]
    fn closing_the_settings_window_does_not_quit_the_app() {
        let mut app = App::new(window());
        assert!(app.running());
        assert!(app.settings().is_some());
        app.close_settings();
        assert!(app.running(), "the window closed and the app went on");
        assert!(app.settings().is_none());
    }

    #[test]
    fn the_app_works_with_the_window_closed_and_can_open_it_again() {
        let mut app = App::new(window());
        app.close_settings();
        // The app is up with nothing on screen.
        assert!(app.running());
        app.open_settings(window());
        assert!(app.settings().is_some());
        assert!(app.running());
    }

    #[test]
    fn opening_the_window_starts_nothing() {
        // Opening takes a window that was read beforehand, so the call itself has
        // nothing to start.
        let mut app = App::closed();
        assert!(app.settings().is_none(), "the window starts closed");
        let before = app.running();
        app.open_settings(window());
        assert_eq!(app.running(), before);
        assert!(app.settings().is_some());
    }

    #[test]
    fn the_window_the_menu_bar_item_opens_carries_the_two_choices() {
        let mut app = App::closed();
        app.open_settings(window());
        let text = app.settings().expect("an open window").to_text();
        assert!(text.contains(SOURCES_TITLE), "{text}");
        assert!(text.contains(ROTATION_TITLE), "{text}");
        // A window with no daemon still says so, and still offers the controls.
        assert!(text.contains("whirl is not running"), "{text}");
        assert!(text.contains(ROTATION_LINE), "{text}");
    }

    #[test]
    fn the_close_button_is_the_windows_and_a_quit_and_a_capture_are_not() {
        let mut app = App::new(window());
        assert!(
            app.window_close(true),
            "an open window's close is the window's"
        );
        assert!(!app.window_close(false), "no close was requested");
        app.close_settings();
        assert!(
            !app.window_close(true),
            "a hidden window has no close button"
        );

        let mut quitting = App::new(window());
        quitting.quit();
        assert!(
            !quitting.window_close(true),
            "the Quit row's close ends the app instead of hiding the window"
        );

        let capturing = App::new(window()).capturing(PathBuf::from("window.png"));
        assert!(
            !capturing.window_close(true),
            "a screenshot's close is the capture's own"
        );
    }

    #[test]
    fn only_a_quit_ends_the_app() {
        let mut app = App::new(window());
        assert!(app.running());
        app.close_settings();
        assert!(app.running());
        app.quit();
        assert!(!app.running());
    }

    #[test]
    fn one_click_runs_one_method_and_the_sections_can_be_read_from_their_lines() {
        // The drawing records a click and the `match` runs one method per arm;
        // what a test can assert without a display is that the value the click
        // produces is the value the lines report. The folder chooser is the one
        // click whose method this reaches without a file on disk.
        let mut settings = window();
        settings.target = None;
        settings.open_picker(None);
        let text = settings.to_text();
        assert!(text.contains(PICKER_TITLE), "{text}");
        settings.picker_cancel();
        assert!(!settings.to_text().contains(PICKER_TITLE));
        // A save with no file to write is refused beside the control rather than
        // panicking or silently doing nothing.
        settings.interval.value = "30".to_string();
        settings.save_interval();
        assert!(
            matches!(settings.interval.outcome, Some(Outcome::Refused { .. })),
            "{:?}",
            settings.interval.outcome
        );
    }

    #[test]
    fn the_five_panes_are_the_five_blocks_the_window_prints() {
        // Naming the panes groups what the window already said; it adds none.
        // Each pane's heading is a block of the text dump, so a pane that lost
        // its heading would be a pane the dump does not carry.
        let settings = window();
        let text = settings.to_text();
        let names: Vec<&str> = Pane::ALL.into_iter().map(Pane::name).collect();
        assert_eq!(
            names,
            vec![
                "Sources",
                "Rotation",
                "Background Helper",
                "Control Panel",
                "About"
            ]
        );
        assert!(text.contains(SOURCES_TITLE), "{text}");
        assert!(text.contains(ROTATION_TITLE), "{text}");
        assert!(text.contains(HELPER_TITLE), "{text}");
        assert!(text.contains(CONTROL_PANEL_TITLE), "{text}");
        assert!(text.contains("whirl is not running"), "{text}");
        // The About pane is the block the check lives in.
        assert!(text.contains(ABOUT_TITLE), "{text}");
        assert!(text.contains(CHECK_LABEL), "{text}");
        assert_eq!(Pane::default(), Pane::Sources);
    }

    /// A directory of this test's own, removed by the test that made it.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("whirl-ui-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    #[test]
    fn a_frame_is_written_as_a_png_of_the_frames_own_size() {
        // `--screenshot` is the one deliverable the app can produce without a
        // display: it writes its own pixels rather than photographing whatever
        // else the screen was showing. This is the whole of the writer: the
        // size, the eight-bit RGBA the encoder is told, and the bytes of each
        // pixel in order.
        let dir = scratch("png");
        let path = dir.join("frame.png");
        let mut image = egui::ColorImage::filled([3, 2], egui::Color32::from_rgb(9, 8, 7));
        image.pixels[5] = egui::Color32::from_rgb(200, 100, 50);
        write_png(&path, &image).expect("the frame is written");

        let bytes = std::fs::read(&path).expect("the file");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "not a PNG");
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = decoder.read_info().expect("a PNG this build can read");
        assert_eq!((reader.info().width, reader.info().height), (3, 2));
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buffer).expect("one frame");
        assert_eq!((frame.width, frame.height), (3, 2), "the frame's own size");
        assert_eq!(&buffer[..4], &[9, 8, 7, 255], "the first pixel, as drawn");
        assert_eq!(
            &buffer[20..24],
            &[200, 100, 50, 255],
            "the pixel the frame was given"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_asks_once_and_writes_the_reply_it_is_given() {
        // The capture's decisions, all reachable without a display. An app that
        // was not asked for a capture asks the context for nothing; the pass that
        // was asked for one asks for the frame and writes nothing yet; a pass
        // whose input carries anything but the reply asks again rather than
        // writing an empty file or closing on nothing; and the pass that carries
        // the screenshot writes it once and closes the window, which is the close
        // `window_close` says belongs to the capture rather than to the window.
        let dir = scratch("capture");
        let path = dir.join("window.png");
        let ctx = egui::Context::default();

        // One pass, with this input. `begin_pass`/`end_pass` are the public way
        // to run a frame against a context with no window behind it, and the
        // frame's own output is dropped, so its texture deltas are marked
        // handled first: egui panics on a delta nobody looked at.
        let pass = |app: &mut App, input: egui::RawInput| {
            ctx.begin_pass(input);
            app.capture_window(&ctx);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
            output
        };
        // What the pass asked the window to do, which is the decision a test can
        // read without a display.
        fn asked(output: &egui::FullOutput) -> Vec<egui::ViewportCommand> {
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.commands.clone())
                .unwrap_or_default()
        }
        fn asks_for_the_frame(commands: &[egui::ViewportCommand]) -> bool {
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Screenshot(_)))
        }
        fn closes(commands: &[egui::ViewportCommand]) -> bool {
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        }
        fn wants_another_pass(output: &egui::FullOutput) -> bool {
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|viewport| viewport.repaint_delay.is_zero())
        }

        let mut unasked = App::closed();
        let output = pass(&mut unasked, egui::RawInput::default());
        assert!(
            !asks_for_the_frame(&asked(&output)),
            "an app that was not asked for a capture asks for no frame"
        );

        let mut app = App::closed().capturing(path.clone());
        let output = pass(&mut app, egui::RawInput::default());
        assert!(
            asks_for_the_frame(&asked(&output)),
            "the first pass asks for the frame"
        );
        assert!(!path.exists(), "the reply lands a frame or two later");

        let output = pass(
            &mut app,
            egui::RawInput {
                events: vec![egui::Event::PointerGone],
                ..egui::RawInput::default()
            },
        );
        assert!(
            !asks_for_the_frame(&asked(&output)),
            "it asked once: a second pass waits for the reply"
        );
        assert!(
            wants_another_pass(&output),
            "and it keeps the loop alive until the reply lands"
        );
        assert!(
            !path.exists(),
            "an event that is not the reply is not the frame"
        );

        let image = egui::ColorImage::filled([2, 3], egui::Color32::from_rgb(4, 5, 6));
        let reply = egui::RawInput {
            events: vec![egui::Event::Screenshot {
                viewport_id: egui::ViewportId::ROOT,
                user_data: egui::UserData::default(),
                image: std::sync::Arc::new(image),
            }],
            ..egui::RawInput::default()
        };
        let output = pass(&mut app, reply);
        assert!(path.exists(), "the reply is the file");
        assert!(
            closes(&asked(&output)),
            "the pass that wrote the frame closes the window"
        );
        assert!(
            !app.window_close(true),
            "the close that follows the write is the capture's own"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_that_cannot_be_written_leaves_no_file_and_does_not_hang() {
        // A path whose folder is not there is the one capture failure a test can
        // reach without a display. It is an answer, not a stall: nothing is
        // written and the window still closes rather than sitting on a file it
        // cannot produce.
        let dir = scratch("unwritable");
        let path = dir.join("no-such-folder").join("frame.png");
        let ctx = egui::Context::default();
        let mut app = App::closed().capturing(path.clone());

        let pass = |app: &mut App, input: egui::RawInput| {
            ctx.begin_pass(input);
            app.capture_window(&ctx);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
        };

        pass(&mut app, egui::RawInput::default());
        let image = egui::ColorImage::filled([1, 1], egui::Color32::from_rgb(1, 2, 3));
        pass(
            &mut app,
            egui::RawInput {
                events: vec![egui::Event::Screenshot {
                    viewport_id: egui::ViewportId::ROOT,
                    user_data: egui::UserData::default(),
                    image: std::sync::Arc::new(image),
                }],
                ..egui::RawInput::default()
            },
        );
        assert!(
            !path.exists(),
            "a path that cannot be written holds nothing"
        );
        assert!(
            !app.window_close(true),
            "the window still closes: the failure is the capture's"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
