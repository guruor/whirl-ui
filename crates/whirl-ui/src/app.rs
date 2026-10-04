//! The settings window, and the app state that owns it.
//!
//! The window is a dialog over the app, and the difference between the two is the
//! whole of this module:
//!
//! - **Opening it starts nothing.** [`App::open_settings`] takes panes that were
//!   already read, so pressing the menu's `Settings…` row cannot start a daemon, a
//!   worker or a rotation. `whirl status`, `whirl sources`, `whirl config path`
//!   and `whirl config check` are reads, and they happen before the window exists.
//! - **Closing it does not quit the app.** The window's close button is answered
//!   with [`egui::ViewportCommand::CancelClose`] and the viewport is hidden; the
//!   app stays up, which is what the menu bar item needs: the window behind its
//!   `Settings…` row is this one, drawn by [`panes`] and closed by this rule.
//! - **The app works with it closed.** With no window the app runs no egui pass
//!   and draws nothing, and [`App::running`] is still true.
//!
//! The window's editors are the rotation interval and the Sources pane, and the
//! files they write are [`crate::config_file`]'s and [`crate::keychain`]'s: this
//! module draws the fields and the buttons, and a click calls one of the
//! [`Settings`] methods, which are the whole of the write paths. Nothing here
//! opens a socket to change a setting, so no part of an edit is a daemon verb,
//! and the other file this crate can write is still the screenshot a maintainer
//! asks for on the command line.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::settings::{Control, Pane, SOURCE_ROW_CONTROLS, Settings, SourceAction};

/// The window's title, and the app name eframe registers.
pub const WINDOW_TITLE: &str = "whirl settings";

/// How big the window opens. Fixed so that a screenshot of it has a size a
/// reader can check.
pub const WINDOW_SIZE: [f32; 2] = [980.0, 720.0];

/// The app: whatever it knows, and which windows are open.
pub struct App {
    /// The dialog's content while the dialog is open. `None` is the window
    /// closed, which is a state the app sits in rather than an exit.
    settings: Option<Settings>,
    /// Set by the menu bar item's `Quit` row, which is a later card. Nothing in
    /// this milestone ends the app.
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

    /// The app with its settings window open on these panes.
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

    /// Show the dialog on these panes.
    ///
    /// The panes were read before the call, so this only records that they are on
    /// screen. Opening starts nothing.
    ///
    /// The menu bar item's `Settings…` row calls this: it reads the panes on its
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

    /// The panes the dialog is showing, or `None` while it is closed. The tests
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

/// Draw the three panes: the whole body of the settings window.
///
/// The menu bar item's `Settings…` row and the standalone window are the same
/// window, and this is what makes them the same: one body, drawn from one
/// [`Settings`] value, so a pane cannot be in one window and missing from the
/// other. The tray owns the viewport and this owns what is in it.
///
/// Two panes edit and one does not, and the drawing says so structurally rather
/// than by leaving a button live that nothing answers: Sources is drawn by
/// [`sources_pane`], Rotation's [`Control::Interval`] as a text field with a
/// `Save` button, and the App pane draws all of its controls disabled.
pub(crate) fn panes(ui: &mut egui::Ui, settings: &mut Settings) {
    egui::Frame::central_panel(ui.style()).show(ui, |ui| {
        ui.heading(WINDOW_TITLE);
        ui.label(
            "the daemon's own answers; the rotation interval and the sources are what this window edits",
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            sources_pane(ui, settings);
            rotation_pane(ui, settings);
            read_only_pane(ui, &settings.app);
        });
    });
}

/// A pane whose controls are all disabled, with the one line that says so.
fn read_only_pane(ui: &mut egui::Ui, pane: &Pane) {
    ui.add_space(8.0);
    ui.separator();
    ui.heading(pane.title);
    for line in &pane.lines {
        ui.monospace(line);
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for control in &pane.controls {
            if let Control::Planned(label) = control {
                ui.add_enabled(false, egui::Button::new(*label));
            }
        }
    });
    ui.label(pane.footer.as_str());
}

/// One click on a Sources control, in the form the write path takes it.
enum SourceClick {
    AddFolder,
    AddWallhaven,
    StoreKey,
    Enable(String),
    Disable(String),
    Remove(String),
    MoveUp(String),
    MoveDown(String),
}

impl SourceAction {
    /// The click this action makes as a control above the rows, if it is one.
    fn on_pane(self) -> Option<SourceClick> {
        match self {
            SourceAction::AddFolder => Some(SourceClick::AddFolder),
            SourceAction::AddWallhaven => Some(SourceClick::AddWallhaven),
            SourceAction::StoreKey => Some(SourceClick::StoreKey),
            SourceAction::Enable
            | SourceAction::Disable
            | SourceAction::Remove
            | SourceAction::MoveUp
            | SourceAction::MoveDown => None,
        }
    }

    /// The click this action makes in one source's row, if it is a row action.
    fn on_source(self, id: String) -> Option<SourceClick> {
        match self {
            SourceAction::Enable => Some(SourceClick::Enable(id)),
            SourceAction::Disable => Some(SourceClick::Disable(id)),
            SourceAction::Remove => Some(SourceClick::Remove(id)),
            SourceAction::MoveUp => Some(SourceClick::MoveUp(id)),
            SourceAction::MoveDown => Some(SourceClick::MoveDown(id)),
            SourceAction::AddFolder | SourceAction::AddWallhaven | SourceAction::StoreKey => None,
        }
    }
}

/// The Sources pane: the daemon's rows, or the file's own after an edit, the
/// fields an added source is made of, the token field, and one set of buttons per
/// source.
///
/// The click is recorded and applied after the widgets are drawn, so a button
/// never has to hold a borrow of the state its action needs, and every button is
/// drawn live because every button has a method behind it.
fn sources_pane(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.add_space(8.0);
    ui.separator();
    ui.heading(settings.sources.title);
    for line in &settings.sources.lines {
        ui.monospace(line);
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("id");
        ui.add(egui::TextEdit::singleline(&mut settings.editor.id).desired_width(120.0));
        ui.label("folder");
        ui.add(egui::TextEdit::singleline(&mut settings.editor.folder).desired_width(220.0));
    });
    ui.horizontal(|ui| {
        ui.label("Wallhaven token");
        // Masked, and the value only ever goes to `store_wallhaven_token`, which
        // hands it to the platform's store. Nothing reads it back.
        ui.add(
            egui::TextEdit::singleline(&mut settings.editor.token)
                .password(true)
                .desired_width(220.0),
        );
    });

    let mut clicked = None;
    ui.horizontal(|ui| {
        for control in &settings.sources.controls {
            if let Control::Source(action) = control
                && ui.add(egui::Button::new(action.label())).clicked()
            {
                clicked = action.on_pane();
            }
        }
    });
    for id in settings.source_ids() {
        ui.horizontal(|ui| {
            ui.monospace(format!("id: {id}"));
            for action in SOURCE_ROW_CONTROLS {
                if ui.add(egui::Button::new(action.label())).clicked() {
                    clicked = action.on_source(id.clone());
                }
            }
        });
    }
    if let Some(click) = clicked {
        apply_source_click(settings, click);
    }
    ui.label(settings.sources.footer.as_str());
}

/// Run the one [`Settings`] method a click asked for.
///
/// Each arm is one call, so "which control writes what" is read here rather than
/// inferred from the drawing, and it is exhaustive: an action that gained no arm
/// would be a control the window draws and nothing answers.
fn apply_source_click(settings: &mut Settings, click: SourceClick) {
    match click {
        SourceClick::AddFolder => {
            settings.add_local_source();
        }
        SourceClick::AddWallhaven => {
            settings.add_wallhaven_source();
        }
        SourceClick::StoreKey => {
            settings.store_wallhaven_token();
        }
        SourceClick::Enable(id) => {
            settings.set_source_enabled(&id, true);
        }
        SourceClick::Disable(id) => {
            settings.set_source_enabled(&id, false);
        }
        SourceClick::Remove(id) => {
            settings.remove_source(&id);
        }
        SourceClick::MoveUp(id) => {
            settings.move_source(&id, crate::config_file::Direction::Up);
        }
        SourceClick::MoveDown(id) => {
            settings.move_source(&id, crate::config_file::Direction::Down);
        }
    }
}

/// The Rotation pane: the interval field, a `Save` button, the four controls that
/// are still the shape of the surface, and the editor's own line.
///
/// `Save` writes the config file through [`Settings::save_interval`]; nothing it
/// does reaches the daemon, which is why the pane says the change is pending
/// until the next rotation rather than applied.
fn rotation_pane(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.add_space(8.0);
    ui.separator();
    ui.heading(settings.rotation.title);
    for line in &settings.rotation.lines {
        ui.monospace(line);
    }
    ui.add_space(4.0);
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label(Control::Interval.label());
        ui.add(egui::TextEdit::singleline(&mut settings.interval.input).desired_width(96.0));
        save = ui.add(egui::Button::new("Save")).clicked();
    });
    ui.horizontal(|ui| {
        for control in &settings.rotation.controls {
            if let Control::Planned(label) = control {
                ui.add_enabled(false, egui::Button::new(*label));
            }
        }
    });
    if save {
        settings.save_interval();
    }
    ui.label(settings.rotation.footer.as_str());
}

/// Run the window until it is closed or the screenshot is written.
pub fn run(settings: Settings, capture: Option<PathBuf>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW_SIZE)
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

    fn panes() -> Settings {
        Settings::unreachable(
            "the daemon is not reachable: whirl.sock (absent): No such file or directory (os error 2)",
        )
    }

    #[test]
    fn closing_the_settings_window_does_not_quit_the_app() {
        let mut app = App::new(panes());
        assert!(app.running());
        assert!(app.settings().is_some());
        app.close_settings();
        assert!(app.running(), "the window closed and the app went on");
        assert!(app.settings().is_none());
    }

    #[test]
    fn the_app_works_with_the_window_closed_and_can_open_it_again() {
        let mut app = App::new(panes());
        app.close_settings();
        // The app is up with nothing on screen.
        assert!(app.running());
        app.open_settings(panes());
        assert!(app.settings().is_some());
        assert!(app.running());
    }

    #[test]
    fn opening_the_window_starts_nothing_and_the_controls_that_edit_are_the_two_editors() {
        // Opening takes panes that were read beforehand, so the call itself has
        // nothing to start. What it opens has four controls that edit: the
        // interval, whose write path is `config_file`, and the Sources pane's
        // three, whose write paths are `config_file` and `keychain`.
        let mut app = App::new(panes());
        app.close_settings();
        let before = app.running();
        app.open_settings(panes());
        assert_eq!(app.running(), before);
        assert_eq!(
            app.settings().expect("an open window").editable_controls(),
            vec![
                ("Sources", Control::Source(SourceAction::AddFolder)),
                ("Sources", Control::Source(SourceAction::AddWallhaven)),
                ("Sources", Control::Source(SourceAction::StoreKey)),
                ("Rotation", Control::Interval),
            ]
        );
    }

    #[test]
    fn the_menu_bar_item_starts_on_a_closed_window_and_opens_it() {
        let mut app = App::closed();
        assert!(app.settings().is_none(), "the window starts closed");
        assert!(app.running(), "a closed window is not a quit");
        app.open_settings(panes());
        assert_eq!(
            app.settings().expect("an open window").enabled_controls(),
            4
        );
        assert!(app.running());
        app.close_settings();
        assert!(app.settings().is_none());
        assert!(app.running(), "the window closed and the app went on");
    }

    #[test]
    fn the_close_button_is_the_windows_and_a_quit_and_a_capture_are_not() {
        let mut app = App::new(panes());
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

        let mut quitting = App::new(panes());
        quitting.quit();
        assert!(
            !quitting.window_close(true),
            "the Quit row's close ends the app instead of hiding the window"
        );

        let capturing = App::new(panes()).capturing(PathBuf::from("window.png"));
        assert!(
            !capturing.window_close(true),
            "a screenshot's close is the capture's own"
        );
    }

    #[test]
    fn only_a_quit_ends_the_app() {
        let mut app = App::new(panes());
        assert!(app.running());
        app.close_settings();
        assert!(app.running());
        app.quit();
        assert!(!app.running());
    }
}
