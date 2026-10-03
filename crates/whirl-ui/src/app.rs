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
//!   app stays up, which is what the menu bar item it will grow needs.
//! - **The app works with it closed.** With no window the app runs no egui pass
//!   and draws nothing, and [`App::running`] is still true.
//!
//! There is no config write path in this module or anywhere in this crate: the
//! window renders [`Settings`], which is a value built from the daemon's answers,
//! and every control is drawn disabled. Keeping that out is not a matter of
//! discipline here, it is a matter of nothing to call: `whirl-ui` depends on
//! `whirlui-client` for talking to the daemon and reads no config file of its own,
//! so the only file it can write is the screenshot a maintainer asks for on the
//! command line.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::settings::{EDITING_ARRIVES_IN_M2, Settings};

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
    /// The app with its settings window open on these panes.
    pub fn new(settings: Settings) -> App {
        App {
            settings: Some(settings),
            quit: false,
            capture: None,
            requested: false,
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
    #[allow(dead_code)] // The menu bar item's `Settings…` row opens it; nothing here does yet.
    pub fn open_settings(&mut self, settings: Settings) {
        self.settings = Some(settings);
    }

    /// Close the dialog. The app keeps running.
    pub fn close_settings(&mut self) {
        self.settings = None;
    }

    /// The panes the dialog is showing, or `None` while it is closed.
    pub fn settings(&self) -> Option<&Settings> {
        self.settings.as_ref()
    }

    /// End the app. The menu bar item's `Quit` row will call this.
    #[allow(dead_code)] // As above: the tray owns the only way to end the app.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// Whether the app is up. It is up with its window closed, which is the point.
    #[allow(dead_code)] // The tests read it; the tray's Quit is what makes it false.
    pub fn running(&self) -> bool {
        !self.quit
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
        // bar item needs to be useful in.
        //
        // While a screenshot is being taken the close is not the dialog's: the
        // capture asks for one itself once the pixels have been written, and
        // cancelling that would leave the app hidden with no window and no way
        // back, which is not what `--screenshot` is for.
        if self.capture.is_none()
            && self.settings.is_some()
            && ctx.input(|input| input.viewport().close_requested())
        {
            self.close_settings();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.capture_window(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let Some(settings) = self.settings() else {
            return;
        };
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.heading(WINDOW_TITLE);
            ui.label("the daemon's own answers, and nothing here edits");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for pane in settings.panes() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.heading(pane.title);
                    for line in &pane.lines {
                        ui.monospace(line);
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for control in &pane.controls {
                            // The field, not a hard `false`: M2 flips the value the
                            // writer can act on, and the drawing follows it.
                            ui.add_enabled(control.enabled, egui::Button::new(control.label));
                        }
                    });
                    ui.label(EDITING_ARRIVES_IN_M2);
                }
            });
        });
    }
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
    fn opening_the_window_starts_nothing_and_can_edit_nothing() {
        // Opening takes panes that were read beforehand, so the call itself has
        // nothing to start. The panes it takes cannot edit: every control is
        // disabled, which is the value the drawing code reads.
        let mut app = App::new(panes());
        app.close_settings();
        let before = app.running();
        app.open_settings(panes());
        assert_eq!(app.running(), before);
        assert_eq!(
            app.settings().expect("an open window").enabled_controls(),
            0
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
