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

use crate::settings::{
    self, APP_DAEMON_NOTE, Daemon, KEY_LINE, Kind, NO_SOURCES, PICKER_TITLE, Pane, ROTATION_LINE,
    ROTATION_TITLE, SOURCES_LINE, SOURCES_TITLE, SUBTITLE, Settings, Unit, WINDOW_TITLE,
};
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

/// The version the footer carries: this app's, the one the build put in it.
const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

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
        panes(ui, settings);
    }
}

/// Draw the window: the app's mark and its three panes, on one rail.
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
/// the panes (see [`header`]). The three panes are the three blocks the window
/// has always drawn; naming them adds no pane, and the sidebar lists exactly them
/// rather than the reference's future sections.
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
                    Pane::App => app_pane(ui, settings),
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
    let connected = matches!(settings.daemon, Daemon::Running);
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
                RichText::new(VERSION)
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
    Remove(String),
    AddFolder,
    AddWallhaven,
    SaveKey,
    CancelKey,
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
                    row_card(ui, row, &mut clicks);
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

    for click in clicks {
        match click {
            Click::Toggle(id, enabled) => settings.set_source_enabled(&id, enabled),
            Click::Change(id) => settings.open_picker(Some(id)),
            Click::Key => settings.key.open = true,
            Click::Remove(id) => settings.remove_source(&id),
            Click::AddFolder => settings.open_picker(None),
            Click::AddWallhaven => settings.add_wallhaven(),
            Click::SaveKey => settings.save_key(),
            Click::CancelKey => settings.key.open = false,
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

/// One source row: its toggle, its line, and the controls that act on it.
fn row_card(ui: &mut egui::Ui, row: &crate::settings::Row, clicks: &mut Vec<Click>) {
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
                    if row.changeable_folder().is_some() && ui.button("Change…").clicked() {
                        clicks.push(Click::Change(row.id.clone()));
                    }
                });
            });
        });
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

/// The App pane: whether whirl answered, and where a change is written.
///
/// The daemon's own sentence is drawn in full from [`Daemon::line`], so this
/// pane and `--dump-settings` say the same thing; the reference's disconnected
/// state is what that sentence looks like when the socket did not answer, and it
/// is drawn as an error card rather than a status light.
fn app_pane(ui: &mut egui::Ui, settings: &Settings) {
    let connected = matches!(settings.daemon, Daemon::Running);
    let outline = if connected {
        theme::HAIRLINE
    } else {
        theme::STATE_BAD
    };
    surface(ui, outline, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(
                rect.center(),
                5.0,
                if connected {
                    theme::STATE_OK
                } else {
                    theme::STATE_BAD
                },
            );
            ui.label(
                RichText::new(if connected { CONNECTED } else { NOT_CONNECTED })
                    .size(theme::TEXT_SECTION)
                    .color(if connected {
                        theme::STATE_OK
                    } else {
                        theme::STATE_BAD
                    }),
            );
        });
        ui.add_space(theme::SPACE_SM);
        if !connected {
            banner(ui, settings.daemon.line().as_str(), theme::STATE_BAD);
            ui.add_space(theme::SPACE_SM);
        } else {
            note(ui, settings.daemon.line().as_str());
            ui.add_space(theme::SPACE_SM);
        }
        caption(ui, APP_DAEMON_NOTE);
        ui.add_space(theme::SPACE_XS);
        caption(ui, &format!("version {VERSION}"));
    });
}

/// The folder chooser: the folders inside one folder, and the two ways out.
///
/// A folder is chosen from a list rather than typed, because a path typed into a
/// window is a path the person has to know and spell, and the one thing this
/// control is for is that they do not have to.
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
            | Click::Remove(..)
            | Click::AddFolder
            | Click::AddWallhaven
            | Click::SaveKey
            | Click::CancelKey => {}
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
    fn the_three_panes_are_the_three_blocks_the_window_prints() {
        // Naming the panes groups what the window already said; it adds none.
        // Each pane's heading is a block of the text dump, so a pane that lost
        // its heading would be a pane the dump does not carry.
        let settings = window();
        let text = settings.to_text();
        let names: Vec<&str> = Pane::ALL.into_iter().map(Pane::name).collect();
        assert_eq!(names, vec!["Sources", "Rotation", "App"]);
        assert!(text.contains(SOURCES_TITLE), "{text}");
        assert!(text.contains(ROTATION_TITLE), "{text}");
        assert!(text.contains("whirl is not running"), "{text}");
        assert_eq!(Pane::default(), Pane::Sources);
    }
}
