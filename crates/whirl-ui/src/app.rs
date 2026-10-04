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
//! What is drawn is [`crate::settings`]'s two choices: where the wallpapers come
//! from and how often they change. This module holds no wording of its own that a
//! person reads twice: every heading, button, line and refusal comes from a
//! constant or a method on the value in `settings`, so the window on screen and
//! the text `--dump-settings` prints cannot drift apart.
//!
//! Nothing here opens a socket to change a setting, so no part of an edit is a
//! daemon verb, and the other file this crate can write is still the screenshot a
//! maintainer asks for on the command line.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::settings::{
    self, APP_DAEMON_NOTE, KEY_LINE, Kind, NO_SOURCES, PICKER_TITLE, ROTATION_LINE, ROTATION_TITLE,
    SOURCES_LINE, SOURCES_TITLE, SUBTITLE, Settings, Unit,
};

/// The window's size and title are the settings module's, because the text dump
/// carries the title too and the two may not drift. The tray names them through
/// this module because this module is the window.
pub use crate::settings::{WINDOW_SIZE, WINDOW_TITLE};

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

/// Draw the window: the two choices, and one line about the daemon.
///
/// The menu bar item's `Settings…` row and the standalone window are the same
/// window, and this is what makes them the same: one body, drawn from one
/// [`Settings`] value, so a control cannot be in one window and missing from the
/// other. The tray owns the viewport and this owns what is in it.
///
/// While the folder chooser is open it is the whole body: the choice it is making
/// is the only thing on screen, and a half-drawn list of folders under a
/// half-drawn list of sources is how a person picks the wrong one.
pub(crate) fn panes(ui: &mut egui::Ui, settings: &mut Settings) {
    egui::Frame::central_panel(ui.style()).show(ui, |ui| {
        ui.heading(WINDOW_TITLE);
        ui.label(SUBTITLE);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if settings.picker.is_some() {
                picker(ui, settings);
                return;
            }
            sources(ui, settings);
            rotation(ui, settings);
            ui.add_space(10.0);
            ui.separator();
            ui.label(settings.daemon.line());
            ui.label(APP_DAEMON_NOTE);
        });
    });
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

/// The Sources section: one row per source, and the two ways to add one.
fn sources(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.add_space(10.0);
    ui.separator();
    ui.heading(SOURCES_TITLE);
    match &settings.sources.problem {
        Some(reason) => {
            ui.label(reason.as_str());
        }
        None if settings.sources.rows.is_empty() => {
            ui.label(NO_SOURCES);
        }
        None => {}
    }

    let mut clicks: Vec<Click> = Vec::new();
    for row in &settings.sources.rows {
        // The row's words give way to its buttons rather than pushing them off
        // the edge: a folder is a path and a path can be long, and a button half
        // outside the window is a button a person cannot press. The row's line
        // loses its tail to an ellipsis here and nowhere else: `--dump-settings`
        // prints the same line whole.
        let (toggle, click) = egui::Sides::new().shrink_left().truncate().show(
            ui,
            |ui| {
                // The checkbox is the one per-row control: a source can be left in
                // the file and out of the rotation, and a window that hid that
                // would be describing a rotation the daemon is not running.
                let mut enabled = row.enabled;
                if ui.checkbox(&mut enabled, "").changed() {
                    return Some(Click::Toggle(row.id.clone(), enabled));
                }
                ui.label(row.line());
                None
            },
            |ui| {
                // This side is laid out right to left, so the widgets are added
                // in reverse: the one written here last is drawn leftmost, which
                // is where `--dump-settings` prints `[Change…] [Remove]` too. The
                // two orders agree, so a person comparing the window with the
                // dump is reading the same row.
                if ui.button("Remove").clicked() {
                    return Some(Click::Remove(row.id.clone()));
                }
                if matches!(row.kind, Kind::Wallhaven { .. }) && ui.button("Enter key…").clicked()
                {
                    return Some(Click::Key);
                }
                if row.changeable_folder().is_some() && ui.button("Change…").clicked() {
                    return Some(Click::Change(row.id.clone()));
                }
                None
            },
        );
        for click in [toggle, click].into_iter().flatten() {
            clicks.push(click);
        }
    }
    ui.horizontal(|ui| {
        if ui.button("Add a folder…").clicked() {
            clicks.push(Click::AddFolder);
        }
        if ui.button("Add Wallhaven").clicked() {
            clicks.push(Click::AddWallhaven);
        }
    });
    ui.label(SOURCES_LINE);
    if let Some(outcome) = &settings.sources.outcome {
        ui.label(outcome.line());
    }

    if settings.key.open {
        ui.horizontal(|ui| {
            ui.label("Wallhaven key");
            // Masked, and the value only ever goes to `save_key`, which hands it
            // to the platform's store. Nothing reads it back, and nothing draws
            // it.
            ui.add(
                egui::TextEdit::singleline(&mut settings.key.token)
                    .password(true)
                    .desired_width(240.0),
            );
            if ui.button("Save key").clicked() {
                clicks.push(Click::SaveKey);
            }
            if ui.button("Cancel").clicked() {
                clicks.push(Click::CancelKey);
            }
        });
        ui.label(KEY_LINE);
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

/// The Rotation section: how long each wallpaper stays, as a number and a unit.
fn rotation(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.add_space(10.0);
    ui.separator();
    ui.heading(ROTATION_TITLE);
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label("Every");
        ui.add(egui::TextEdit::singleline(&mut settings.interval.value).desired_width(80.0));
        egui::ComboBox::from_id_salt("rotation-unit")
            .selected_text(settings.interval.unit.name())
            .show_ui(ui, |ui| {
                for unit in Unit::ALL {
                    ui.selectable_value(&mut settings.interval.unit, unit, unit.name());
                }
            });
        save = ui.button("Save").clicked();
    });
    ui.label(ROTATION_LINE);
    if let Some(outcome) = &settings.interval.outcome {
        ui.label(outcome.line());
    }
    if let Some(now) = settings.interval.in_use_phrase() {
        // Its own paragraph: the note above it already ends in `does not change
        // yet`, and a reader who takes the two for one sentence reads the window
        // as contradicting itself.
        ui.add_space(4.0);
        ui.label(now);
    }
    if save {
        settings.save_interval();
    }
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
    ui.add_space(10.0);
    ui.separator();
    ui.heading(PICKER_TITLE);
    ui.monospace(showing.directory.display().to_string());

    let mut clicks: Vec<Click> = Vec::new();
    ui.horizontal(|ui| {
        if ui.button("Up").clicked() {
            clicks.push(Click::PickerUp);
        }
        if ui.button("Use this folder").clicked() {
            clicks.push(Click::PickerChoose);
        }
        if ui.button("Cancel").clicked() {
            clicks.push(Click::PickerCancel);
        }
    });
    match &showing.problem {
        Some(reason) => {
            ui.label(reason.as_str());
        }
        None if showing.entries.is_empty() => {
            ui.label("(no folders inside it)");
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
        Box::new(|_creation| Ok(Box::new(app))),
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
    use crate::settings::{Outcome, ROTATION_LINE};

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
}
