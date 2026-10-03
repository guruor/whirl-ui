//! The macOS menu bar item.
//!
//! An accessory app: a status item whose menu is the product, and a window that
//! is a dialog behind one row of it. Everything that reaches the socket does it
//! through `whirlui-client`, so section 8's "must never" list holds by
//! construction: this module writes no state file, calls no platform setter, and
//! never starts, stops or restarts the daemon. It also never polls: one
//! subscription is the only source of change, and the two threads below block on
//! it and on a channel.
//!
//! The shape is three pieces:
//!
//! - the event thread sets the view (a `status` snapshot, then the deltas the
//!   events carry) and asks for a repaint;
//! - the command thread performs the five rows that are whirl's own verbs, on
//!   its own connection, so a rotation that blocks for seconds never blocks the
//!   menu;
//! - the UI thread draws the rows and owns the tray icon, which must be created
//!   once the event loop is running (`tray-icon`'s own macOS requirement).

use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use tray_icon::TrayIcon;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};

use whirlui_client::{Client, ClientError, Event, Subscription, Update};

use crate::dump::{EXIT_OK, EXIT_USAGE};
use crate::menu::{self, Action, Row, RowId};
use crate::state::View;

/// How long to wait before trying an unreachable daemon again. The subscription
/// already waits this long between reconnects; this is the same interval for the
/// one failure it cannot absorb, a daemon that is not there at all.
const RETRY: Duration = Duration::from_secs(1);

/// The state the tray draws, shared by the two worker threads and the UI.
///
/// One mutex, one value: every thread reads the view, writes the view, or asks
/// the UI to draw. Nothing here waits on anything but the subscription and the
/// command channel, so no thread polls.
#[derive(Debug)]
struct Shared {
    view: Mutex<View>,
    ui: Mutex<Option<egui::Context>>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            view: Mutex::new(View::offline()),
            ui: Mutex::new(None),
        }
    }

    /// The view as it stands.
    fn view(&self) -> View {
        self.view.lock().expect("the view").clone()
    }

    /// The egui context, once the UI has one.
    fn with_ui(&self, f: impl FnOnce(&egui::Context)) {
        if let Some(ctx) = self.ui.lock().expect("the ui slot").as_ref() {
            f(ctx);
        }
    }

    /// The UI's context, for the threads that change the view.
    ///
    /// Asking for a repaint here would be a no-op: eframe does not populate its
    /// window-to-viewport map until after this creator returns
    /// (`GlowWinitApp::window_id_from_viewport_id` reads `self.running`, and
    /// `running` is only set once the app exists), so a request made now resolves
    /// to no window and is discarded. `first_pass` is how one is asked for.
    fn attach(&self, ctx: &egui::Context) {
        *self.ui.lock().expect("the ui slot") = Some(ctx.clone());
    }

    /// Replace the view.
    fn replace(&self, view: View) {
        let line = {
            let mut slot = self.view.lock().expect("the view");
            if *slot == view {
                return;
            }
            let line = view.image_line();
            *slot = view;
            line
        };
        self.changed(&line);
    }

    /// Apply one event to the view.
    fn apply(&self, event: &Event) {
        let changed = {
            let mut slot = self.view.lock().expect("the view");
            let before = slot.clone();
            slot.apply(event);
            *slot != before
        };
        if changed {
            let line = self.view().image_line();
            self.changed(&line);
        }
    }

    /// One line per change of what the menu shows, and one ask to draw it.
    ///
    /// The line is written where the change happens rather than where the tray
    /// is updated, because the two are not the same moment: the tray can only be
    /// touched from the main thread, and the main thread only runs a pass when
    /// egui schedules one. A change that reaches no pass still happened, and the
    /// wall clock beside it is what the menu's latency is measured against.
    fn changed(&self, line: &str) {
        eprintln!("whirl-ui: {} now: {line}", unix_nanos());
        self.with_ui(egui::Context::request_repaint);
    }

    /// The daemon is not reachable, and this is why.
    fn unreachable(&self, error: &ClientError) {
        eprintln!("whirl-ui: {error}");
        self.replace(View::offline());
    }

    /// The daemon stopped answering, with no error of its own to report.
    ///
    /// The subscription reports this case without naming a socket, so there is
    /// nothing to name here: a report made up from what the app can guess would
    /// name a socket that is not the one in use. The attempt that follows makes
    /// the real one.
    fn offline(&self) {
        self.replace(View::offline());
    }

    /// The daemon refused a verb, which is a legal answer (2.7), not a state.
    fn refused(&self, error: &ClientError) {
        eprintln!("whirl-ui: {error}");
    }

    /// Show the settings window, or the placeholder standing in for it.
    fn show_window(&self) {
        self.with_ui(|ctx| {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        });
    }

    /// End this app. Not the daemon: its lifetime is the OS supervisor's
    /// (section 8, "must never" 3).
    fn quit(&self) {
        self.with_ui(|ctx| {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        });
    }
}

/// Follow the daemon and keep the view current.
///
/// This is the only thread that talks to a subscription, and it blocks on it:
/// `Subscription::next` returns a snapshot when the app starts, an event when
/// the daemon changes something, and a fresh snapshot after a gap, a restart or
/// a silence (section 8 items 3 and 4).
fn events(shared: Arc<Shared>) {
    // Whether the daemon's unavailability has already been reported. A daemon
    // that is down is a state the row carries, and the retry below runs once a
    // second: the same line every second is noise, not evidence. It is cleared
    // when a subscription opens, so an outage after a recovery is reported.
    let mut said = false;
    loop {
        let mut subscription = match Subscription::open() {
            Ok(subscription) => subscription,
            Err(error) => {
                if said {
                    shared.offline();
                } else {
                    shared.unreachable(&error);
                    said = true;
                }
                thread::sleep(RETRY);
                continue;
            }
        };
        said = false;
        loop {
            match subscription.next() {
                Update::Status(status) => shared.replace(View::live(status)),
                Update::Event(event) => shared.apply(&event),
                Update::Offline => {
                    // The subscription reports that it lost the daemon without
                    // an error of its own, so nothing here names a socket: the
                    // `open` on the next turn makes the report that can.
                    shared.offline();
                    break;
                }
            }
        }
    }
}

/// Perform the five rows that are whirl's own verbs, one at a time, on a
/// connection of its own.
///
/// It is a separate connection precisely so that `next`, which blocks until the
/// daemon has an outcome (up to 300 s, 2.8), cannot stall the thread that draws.
/// The connection is opened on the first click and kept: a refusal is a legal
/// answer and does not close it, so only an unreachable or closed daemon makes
/// the next click reconnect.
fn commands(rx: Receiver<Action>, shared: Arc<Shared>) {
    let mut client: Option<Client> = None;
    while let Ok(action) = rx.recv() {
        let mut connection = match client.take() {
            Some(connection) => connection,
            None => match Client::connect() {
                Ok(connection) => connection,
                Err(error) => {
                    // Every one of these is a click the user made, so every one
                    // of them gets its reason.
                    shared.unreachable(&error);
                    continue;
                }
            },
        };
        match perform(&mut connection, action) {
            Ok(()) => client = Some(connection),
            Err(error) => {
                shared.refused(&error);
                if error.refusal().and_then(|refusal| refusal.action())
                    == Some(whirlui_client::Action::ReReadStatus)
                {
                    // Section 8 item 5: `not_found` means the id this app holds
                    // is stale, so read the state again rather than keep
                    // drawing from it.
                    match connection.status() {
                        Ok(status) => shared.replace(View::live(status)),
                        Err(error) => shared.refused(&error),
                    }
                    client = Some(connection);
                } else if error.unreachable() {
                    shared.unreachable(&error);
                } else if !error.refusal().is_some_and(|refusal| refusal.closes()) {
                    // The daemon said no and kept the connection (2.7), so the
                    // next click can use it.
                    client = Some(connection);
                }
            }
        }
    }
}

/// Send one row's verb (2.5.1). No socket code lives above this function.
fn perform(client: &mut Client, action: Action) -> Result<(), ClientError> {
    match action {
        Action::Next => client.next().map(|_| ()),
        Action::Previous => client.prev().map(|_| ()),
        Action::Pause => client.pause(),
        Action::Resume => client.resume(),
        Action::Favourite => client.favorite(None).map(|_| ()),
        // Neither is a daemon verb: `Settings…` is the app's window and `Quit`
        // is this process. Both are handled where the click arrives.
        Action::Settings | Action::Quit => Ok(()),
    }
}

/// Run the menu bar item until the user quits it.
pub fn run() -> ExitCode {
    let (tx, rx) = mpsc::channel::<Action>();
    let shared = Arc::new(Shared::new());
    let (event_shared, command_shared) = (Arc::clone(&shared), Arc::clone(&shared));
    spawn("whirl-ui-events", move || events(event_shared));
    spawn("whirl-ui-commands", move || commands(rx, command_shared));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("whirl")
            // The menu is the product and the window is a dialog: it starts
            // hidden and only `Settings…` shows it.
            .with_visible(false)
            .with_inner_size([440.0, 190.0])
            .with_resizable(false),
        event_loop_builder: Some(Box::new(|builder| {
            // The one thing eframe does not surface (whirl's
            // docs/research/frontend-stack.md 3.4). Without it the app takes a
            // Dock tile, which the card forbids.
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory);
        })),
        ..Default::default()
    };

    let creator = {
        let shared = Arc::clone(&shared);
        Box::new(move |cc: &eframe::CreationContext<'_>| {
            App::new(cc, shared, tx).map(|app| Box::new(app) as Box<dyn eframe::App>)
        })
    };

    match eframe::run_native("whirl", options, creator) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(error) => {
            eprintln!("whirl-ui: the menu bar item could not start: {error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Start a named helper thread.
///
/// A thread that has panicked is a thread the app no longer has, and the
/// failure is reported where it happened; there is nothing to join here.
fn spawn(name: &str, body: impl FnOnce() + Send + 'static) {
    let _ = thread::Builder::new().name(name.to_string()).spawn(body);
}

/// The app itself: the tray icon, the rows, and the placeholder window.
struct App {
    shared: Arc<Shared>,
    tray: Option<TrayIcon>,
    /// The rows currently on the tray, so a repaint that changed nothing
    /// rebuilds nothing.
    rendered: Vec<Row>,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Shared>,
        actions: Sender<Action>,
    ) -> Result<App, Box<dyn std::error::Error + Send + Sync>> {
        shared.attach(&cc.egui_ctx);

        let rendered = menu::rows(&shared.view());
        let tray = tray_icon::TrayIconBuilder::new()
            // A text item: the app has no artwork yet and a made-up glyph would
            // be a picture nobody chose. The name is also what a screenshot of
            // the menu bar can be read against.
            .with_title("whirl")
            .with_tooltip("whirl: the wallpaper daemon's menu")
            .with_menu(Box::new(build_menu(&rendered)))
            .build()?;

        let (ui, clicks) = (Arc::clone(&shared), actions);
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            match click(event.id().as_ref()) {
                Click::Show => ui.show_window(),
                Click::Quit => ui.quit(),
                // The daemon's own verbs are performed off this thread: the
                // click handler runs while the menu is up, and a rotation can
                // take seconds.
                Click::Verb(action) => {
                    let _ = clicks.send(action);
                }
                Click::Nothing => {}
            }
        }));

        let app = App {
            shared: Arc::clone(&shared),
            tray: Some(tray),
            rendered,
        };
        // One pass is asked for from another thread, because asked for here it
        // would be discarded (see `Shared::attach`). It costs one thread start
        // and closes the window in which a change lands between the tray being
        // built from the view above and eframe being able to draw anything.
        first_pass(&cc.egui_ctx);
        Ok(app)
    }
}

/// Ask for one pass from a thread that outlives the app's creation.
fn first_pass(ctx: &egui::Context) {
    let ctx = ctx.clone();
    let _ = thread::Builder::new()
        .name("whirl-ui-first-pass".to_string())
        .spawn(move || ctx.request_repaint());
}

/// What a click asks for, before anything is done about it.
///
/// The click handler is the one piece of this module that a test cannot drive
/// end to end, because a menu event is something AppKit emits. So the decision is
/// a value here and the handler is three lines that act on it, and what the
/// handler can do is asserted directly.
///
/// A disabled row cannot reach here at all: AppKit does not send an event for an
/// item the menu has switched off, which is how the current-image line and the
/// rows of a daemon that is not running stay inert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    /// Send this verb to the daemon (2.5.1), through `whirlui-client`.
    Verb(Action),
    /// Show the settings window.
    Show,
    /// Quit this app, never the daemon.
    Quit,
    /// Nothing at all: the row is a line, or the name is not one this app put in
    /// its own menu.
    Nothing,
}

/// Resolve a clicked menu item's name.
fn click(id: &str) -> Click {
    match RowId::from_key(id).and_then(RowId::action) {
        Some(Action::Settings) => Click::Show,
        Some(Action::Quit) => Click::Quit,
        Some(action) => Click::Verb(action),
        None => Click::Nothing,
    }
}

impl eframe::App for App {
    /// The state half of a frame: what the menu shows, and nothing drawn.
    ///
    /// eframe calls this before every `ui`, and also when the window is hidden
    /// and a repaint was requested (`NativeIntegration::update_logic_only`).
    /// That second case is the app's normal one: the menu bar item is always
    /// there and its window almost never is, so the tray has to be brought up to
    /// date in a call that does not require a window to draw into.
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let rows = menu::rows(&self.shared.view());
        if rows != self.rendered {
            if let Some(tray) = self.tray.as_ref() {
                tray.set_menu(Some(Box::new(build_menu(&rows))));
            }
            self.rendered = rows;
        }
    }

    /// The placeholder the `Settings…` row shows, until that card lands.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let view = self.shared.view();
        ui.heading("whirl");
        ui.add_space(6.0);
        ui.label("The settings window has not landed yet.");
        ui.label(
            "When it does it will report the effective plan read-only, with every control \
             disabled and the reason it is disabled visible (docs/milestones.md, M1 criterion 6).",
        );
        ui.add_space(6.0);
        if view.reachable() {
            ui.label("The daemon is running.");
        } else {
            ui.label("The daemon is not reachable.");
        }
    }
}

/// The menu the tray shows: the rows, in order, one item or separator each.
fn build_menu(rows: &[Row]) -> Menu {
    let menu = Menu::new();
    for row in rows {
        match row.id {
            None => {
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
            Some(id) => {
                // A row with an id is clickable when it is enabled; the current
                // image is a line, so it is disabled and sends nothing.
                let item = MenuItem::with_id(id.key(), &row.label, row.enabled, None);
                let _ = menu.append(&item);
            }
        }
    }
    menu
}

/// Wall-clock nanoseconds, the clock the measurement in the card compares
/// against.
fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_resolves_to_what_the_handler_can_do() {
        for id in RowId::ALL {
            let resolved = click(id.key());
            match id.action() {
                None => assert_eq!(resolved, Click::Nothing, "{}", id.key()),
                Some(Action::Settings) => assert_eq!(resolved, Click::Show),
                Some(Action::Quit) => assert_eq!(resolved, Click::Quit),
                Some(action) => assert_eq!(resolved, Click::Verb(action), "{}", id.key()),
            }
        }
    }

    #[test]
    fn the_two_rows_that_are_not_daemon_verbs_never_become_one() {
        // The one bug this mapping can have is sending the daemon a verb it does
        // not have: `Settings…` and `Quit` belong to this process.
        assert_eq!(click(RowId::Settings.key()), Click::Show);
        assert_eq!(click(RowId::Quit.key()), Click::Quit);
        for id in RowId::ALL {
            let resolved = click(id.key());
            if id != RowId::Settings && id != RowId::Quit {
                assert!(!matches!(resolved, Click::Show | Click::Quit));
            }
        }
    }

    #[test]
    fn a_name_this_app_did_not_put_in_its_menu_does_nothing() {
        assert_eq!(click("sep"), Click::Nothing);
        assert_eq!(click(""), Click::Nothing);
        assert_eq!(click("--menu-dump"), Click::Nothing);
    }
}
