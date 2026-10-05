//! The macOS menu bar item.
//!
//! A status item whose menu is the product, and a window that is a dialog behind
//! one row of it. The dialog is the settings window [`app`] draws, so the
//! `Settings…` row opens the one settings window this repository has, and closing
//! it leaves the app up, which is the rule the standalone window already carried.
//! Everything that reaches the socket does it through `whirlui-client`, so
//! section 8's "must never" list holds by construction: this module writes no
//! state file, calls no platform setter, and never starts, stops or restarts the
//! daemon. It also never polls: one subscription is the only source of change,
//! and the two threads below block on it and on a channel.
//!
//! The app is an agent, and only while its window is shut. It is built with the
//! accessory activation policy and the bundle says `LSUIElement`, so it takes no
//! Dock tile and nothing in the switcher -- and, because that is what an agent
//! policy means, its windows are not managed by the window manager either. That
//! last part is the defect the policy half of [`crate::window`] answers: the
//! settings window was on screen, unfindable behind whatever was over it, and
//! unreachable through Cmd+Tab. So the policy follows the window out of
//! [`App::state_window`], and a `Settings…` click with the window already up
//! raises the window it has instead of activating the process ([`App::logic`]).
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
//!
//! The item's picture comes from [`crate::icon`], and it is the one thing here
//! that changes without a menu rebuild: the two marks are template images, so
//! macOS draws them from their alpha alone and inverts them for the menu bar's
//! appearance, and the state decides which of the two is set. The mark is the
//! item's whole label, which is why the item draws no title: what it is called
//! is a tooltip and an accessible name (`ITEM_NAME`), and neither of those
//! changes the item's width.

use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use tray_icon::TrayIcon;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};

use whirlui_client::{Client, ClientError, Event, Subscription, Update};

use crate::app;
use crate::dump::{self, EXIT_OK, EXIT_USAGE};
use crate::icon;
use crate::menu::{self, Action, Row, RowId};
use crate::settings::Settings;
use crate::settings::{WINDOW_SIZE, WINDOW_TITLE};
use crate::state::View;
use crate::window;

/// How long to wait before trying an unreachable daemon again. The subscription
/// already waits this long between reconnects; this is the same interval for the
/// one failure it cannot absorb, a daemon that is not there at all.
const RETRY: Duration = Duration::from_secs(1);

/// What a click has asked the UI thread for and it has not carried out yet.
///
/// A click lands on the main thread, but the work behind one does not belong
/// there: the panes a `Settings…` row shows are four reads of the daemon, and the
/// close that ends the app has to wait until the window is shut. So a click
/// leaves a request here and the UI thread's next pass performs it.
#[derive(Debug, Default)]
struct Requests {
    /// The panes a `Settings…` click read, for the pass that opens the window.
    window: Option<Settings>,
    /// Set by the `Quit` row.
    quit: bool,
}

/// The state the tray draws, shared by the two worker threads and the UI.
///
/// One mutex per value: every thread reads the view, writes the view, or asks the
/// UI to draw. Nothing here waits on anything but the subscription and the
/// command channel, so no thread polls.
#[derive(Debug)]
struct Shared {
    view: Mutex<View>,
    ui: Mutex<Option<egui::Context>>,
    requests: Mutex<Requests>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            view: Mutex::new(View::offline()),
            ui: Mutex::new(None),
            requests: Mutex::new(Requests::default()),
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

    /// Leave these panes for the pass that opens the settings window.
    ///
    /// The read happens on the command thread and showing a viewport is the UI
    /// thread's to do, so the panes wait here between the two.
    fn request_window(&self, panes: Settings) {
        self.requests.lock().expect("the requests").window = Some(panes);
        self.with_ui(egui::Context::request_repaint);
    }

    /// The panes a click left, once: the pass that shows the window takes them.
    fn take_window_request(&self) -> Option<Settings> {
        self.requests.lock().expect("the requests").window.take()
    }

    /// Ask for this app to end. Not the daemon: its lifetime is the OS
    /// supervisor's (section 8, "must never" 3).
    ///
    /// The close itself is the UI thread's to send, and it sends it after closing
    /// the window: a close that arrives while the window is open is answered as
    /// the window's own, and with that rule the app would stay up instead of
    /// going.
    fn request_quit(&self) {
        self.requests.lock().expect("the requests").quit = true;
        self.with_ui(egui::Context::request_repaint);
    }

    /// Whether a `Quit` click is waiting. The UI thread, once.
    fn take_quit_request(&self) -> bool {
        std::mem::take(&mut self.requests.lock().expect("the requests").quit)
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
/// connection of its own, and read the panes the `Settings…` row opens the window
/// on.
///
/// It is a separate connection precisely so that `next`, which blocks until the
/// daemon has an outcome (up to 300 s, 2.8), cannot stall the thread that draws.
/// The connection is opened on the first click and kept: a refusal is a legal
/// answer and does not close it, so only an unreachable or closed daemon makes
/// the next click reconnect. The window's read asks its own four questions on a
/// connection of its own and closes it, which is what `settings_answers` does.
fn commands(rx: Receiver<Action>, shared: Arc<Shared>) {
    let mut client: Option<Client> = None;
    while let Ok(action) = rx.recv() {
        // `Settings…` is this app's window rather than a daemon verb, and opening
        // it is four reads of the daemon. They happen here so a daemon that is
        // slow to answer cannot stall the menu. The exit code is dropped: it is
        // for a caller that branches on it, and the panes carry the reason
        // instead.
        if action == Action::Settings {
            let (answers, _code) = dump::settings_answers();
            shared.request_window(Settings::from_answers(&answers));
            continue;
        }
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
        // Neither is a daemon verb: `Settings…` is this app's window and `Quit`
        // is this process, and no clicked row reaches here as either. The arm
        // stays so that a verb added to `Action` cannot be forgotten here.
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
            // The window behind `Settings…` is the settings window `app` draws:
            // the same title, the same size, the same panes. It is one window
            // rather than a smaller one of the tray's own, which is what keeps
            // the row from opening something that drifts from `--dump-settings`.
            .with_title(WINDOW_TITLE)
            .with_inner_size(WINDOW_SIZE)
            // The menu is the product and the window is a dialog: it starts
            // hidden and only `Settings…` shows it.
            //
            // This is the ask, not the hiding. eframe shows a root window after
            // the first painted frame whatever `visible` the builder asked for
            // (`eframe` `native/epi_integration.rs`, `post_rendering`), so a
            // window that must start hidden has to be hidden again from the app:
            // `App::state_window`, which is also where the window comes off the
            // screen when the dialog closes.
            .with_visible(false),
        event_loop_builder: Some(Box::new(|builder| {
            // The one thing eframe does not surface (whirl's
            // docs/research/frontend-stack.md 3.4). This is the policy the app
            // launches under, and it is the accessory one: a menu bar item
            // starts with no Dock tile and nothing in the switcher, which is
            // what the bundle's `LSUIElement` says too (`docs/milestones.md` M1
            // criterion 5). It is not the policy for the app's whole life: this
            // is the only place winit takes one, and it takes it once, so the
            // window's own lifetime moves the app between the two from
            // `App::state_window`, through `crate::window`.
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

    match eframe::run_native(WINDOW_TITLE, options, creator) {
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

/// The app itself: the tray icon, the rows, and the settings window behind
/// `Settings…`.
struct App {
    shared: Arc<Shared>,
    tray: Option<TrayIcon>,
    /// The window behind `Settings…`. It is `app`'s, so the row opens the one
    /// settings window this repository has: the four panes `--dump-settings`
    /// prints, and a close button that hides the window rather than ending the
    /// app.
    dialog: app::App,
    /// The rows currently on the tray, so a repaint that changed nothing
    /// rebuilds nothing.
    rendered: Vec<Row>,
    /// The mark currently set, so a repaint that changed no state leaves the
    /// icon alone. The menu is rebuilt for the same reason.
    mark: icon::Mark,
    /// What the framework was last told about the settings window's visibility.
    /// `None` is before the first pass has told it anything, which that pass must
    /// treat as a change: see [`visibility`] and [`App::state_window`].
    stated: Option<bool>,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Shared>,
        actions: Sender<Action>,
    ) -> Result<App, Box<dyn std::error::Error + Send + Sync>> {
        shared.attach(&cc.egui_ctx);

        let rendered = menu::rows(&shared.view());
        let mark = icon::Mark::of(&shared.view());
        let tray = tray_icon::TrayIconBuilder::new()
            // The picture is the whole label: one mark, and no title beside it,
            // so the item is as wide as its artwork. Two readers need a word
            // that no pixel carries, and both are set below: the tooltip, for a
            // mouse, and the accessible name, for anything reading the
            // accessibility tree.
            .with_tooltip(TOOLTIP)
            .with_menu(Box::new(build_menu(&rendered)))
            // Templated, so AppKit draws the mark from its alpha alone and
            // recolours it for a light or a dark menu bar. `tray-icon` 0.26.0
            // deprecates the `set_icon_as_template`/`with_icon_as_template`
            // spelling, and this workspace lints with `-D warnings`:
            // `with_icon_templated` is the same request in one call, and
            // `set_mark` below is its swap-time twin.
            .with_icon_templated(artwork(mark)?)
            .build()?;
        set_accessible_name(&tray, ITEM_NAME);

        let (ui, clicks) = (Arc::clone(&shared), actions);
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            match click(event.id().as_ref()) {
                // The panes are four reads of the daemon, so they are read on the
                // command thread and the UI thread's next pass opens the window.
                Click::Show => {
                    let _ = clicks.send(Action::Settings);
                }
                // A quit waits for the same pass, which closes the window before
                // it closes the app (`Requests`).
                Click::Quit => ui.request_quit(),
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
            dialog: app::App::closed(),
            rendered,
            mark,
            stated: None,
        };
        // One pass is asked for from another thread, because asked for here it
        // would be discarded (see `Shared::attach`). It costs one thread start
        // and closes the window in which a change lands between the tray being
        // built from the view above and eframe being able to draw anything.
        first_pass(&cc.egui_ctx);
        Ok(app)
    }

    /// Put the settings window on screen when there is a dialog in it, and take it
    /// off the screen when there is not, at most once per change.
    ///
    /// This is the half of the window's rule the builder cannot state.
    /// `NativeOptions.viewport.with_visible(false)` in [`run`] is not honoured:
    /// eframe shows a root window after its first painted frame whatever the
    /// builder asked for (`eframe` `native/epi_integration.rs`, `post_rendering`),
    /// so an app that wants a window to stay hidden has to say so from a pass, and
    /// the pass that says it is the first one. Without this the app presented an
    /// empty window at launch, in the frame's own clear colour, because
    /// `app::App::ui` draws nothing while no dialog is open and eframe clears the
    /// frame with `App::clear_color`'s default (`eframe` `epi.rs`,
    /// `rgba(12, 12, 12, 180)`). Measured 2026-10-04, before this: the window is on
    /// screen 1.3 s to 2.3 s after launch in 10 of 10 runs.
    ///
    /// `stated` is what the framework was last told, so a pass that changes
    /// nothing sends nothing. The first pass is `None` and therefore always
    /// speaks: that is the pass whose paint eframe answers by showing the window.
    fn state_window(&mut self, ctx: &egui::Context) {
        let Some(visible) = visibility(self.dialog.open(), self.stated) else {
            return;
        };
        self.stated = Some(visible);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));

        // The policy follows the window, and this is the pass where it changed.
        // A regular app while the window is on screen: a Dock tile, a Cmd+Tab
        // entry, and a window the window manager manages, which is what the
        // raise below needs and what an agent app's window never had. Accessory
        // again when it goes, so no Dock tile outlives the window (M1 criterion
        // 5). The rule is `window::policy_for`, which is the one the test in
        // `main.rs` asserts.
        window::apply(window::policy_for(visible));

        // Showing is a raise as well as a visibility change, and the raise is
        // the window's rather than the app's: front, key and main, out of the
        // Dock if it was minimised, and only then the app forward. It replaces
        // the `Focus` viewport command this used to send, because that is one
        // request in winit's terms (`focus_window`) and does not activate the
        // process, which is the half that was missing. A `Settings…` click on a
        // window that is already up takes the same path from `App::logic`; this
        // is the click that opened it.
        let raised = visible.then(|| window::raise(&window::AppKit));

        // One line per change, and no line at all from a pass that changed
        // nothing, so the log says which of the two states the window was in and
        // when. A window on screen with no dialog in it is this defect, and a log
        // that shows a launch with no `shown` line is what tells it from a
        // launch that presented one. The steps the raise took ride on the `shown`
        // line, because they are what the app did and not what the desktop
        // looked like.
        eprintln!(
            "whirl-ui: {} the settings window is {}{}",
            unix_nanos(),
            if visible {
                "shown with its panes"
            } else {
                "hidden: no dialog is open"
            },
            raised.map_or_else(String::new, |steps| format!(", raised {steps:?}"))
        );
    }
}

/// Ask for one pass from a thread that outlives the app's creation.
fn first_pass(ctx: &egui::Context) {
    let ctx = ctx.clone();
    let _ = thread::Builder::new()
        .name("whirl-ui-first-pass".to_string())
        .spawn(move || ctx.request_repaint());
}

/// The mark as `tray-icon` takes it: raw RGBA.
///
/// The decoded file is already straight RGBA with the shape in its alpha, which
/// is what a template image is; whether AppKit *draws* it as one is the
/// caller's to say, and both callers below say yes.
fn artwork(mark: icon::Mark) -> Result<tray_icon::Icon, Box<dyn std::error::Error + Send + Sync>> {
    let art = mark.artwork()?;
    Ok(tray_icon::Icon::from_rgba(art.rgba, art.width, art.height)?)
}

/// Swap the item's picture, keeping it a template image.
///
/// `set_icon` alone would draw the mark in its own black, which is invisible on
/// a dark menu bar; `set_icon_templated` is the one call that sets the picture
/// and asks for the template rendering at the same time.
fn set_mark(
    tray: &TrayIcon,
    mark: icon::Mark,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tray.set_icon_templated(Some(artwork(mark)?))?;
    Ok(())
}

// What the item is called, in the two places nothing is drawn.
//
// The word beside the mark was redundant once the mark was there, so the item
// draws the mark alone. Two readers still need a name, and neither of them
// reads a pixel. A mouse reads these through `tray-icon`: the tooltip is set on
// the status item's button by the crate. The accessibility tree reads the
// button's own label, which a button with no title leaves empty and which
// `tray-icon` 0.26 has no call for; `set_accessible_name` below sets it.

/// The item's accessible name: the app alone, which is what the removed title
/// said.
const ITEM_NAME: &str = "whirl";

/// The item's tooltip: the app, and what the item is, for a mouse that has
/// already found it.
const TOOLTIP: &str = "whirl: the wallpaper daemon's menu";

/// Name the item for the accessibility tree, which a status button with no
/// title leaves nameless.
///
/// The name is set on the button the crate does expose
/// (`TrayIcon::ns_status_item`). `tray-icon` is built on `objc2`, so this is the
/// same bindings and the same versions rather than a second way in.
fn set_accessible_name(tray: &TrayIcon, name: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSAccessibility;
    use objc2_foundation::NSString;

    // The tray was built on this thread and a status item lives on no other:
    // `tray-icon` requires the main thread to create one.
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(button) = tray.ns_status_item().and_then(|item| item.button(mtm)) {
        button.setAccessibilityLabel(Some(&NSString::from_str(name)));
    }
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
    /// Ask for the settings window. The panes are read on the command thread and
    /// the window is opened by the UI thread, so a click on this row is a message
    /// rather than a verb: nothing it does reaches the daemon.
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
    /// The state half of a frame: what the menu shows, what a click asked for,
    /// and the window's own rule, with nothing drawn.
    ///
    /// eframe calls this before every `ui`, and also when the window is hidden
    /// and a repaint was requested (`NativeIntegration::update_logic_only`).
    /// That second case is the app's normal one: the menu bar item is always
    /// there and its window almost never is, so the tray has to be brought up to
    /// date in a call that does not require a window to draw into. It is also
    /// what makes a `Settings…` click work on a hidden window: the click requests
    /// a repaint, this runs, and the viewport is shown from here.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let rows = menu::rows(&self.shared.view());
        if rows != self.rendered {
            if let Some(tray) = self.tray.as_ref() {
                tray.set_menu(Some(Box::new(build_menu(&rows))));
            }
            self.rendered = rows;
        }

        // The picture, for the same reason and on the same pass as the rows: a
        // `Paused` event changes both, and the mark is what makes the state
        // readable with the menu shut.
        let mark = icon::Mark::of(&self.shared.view());
        if mark != self.mark {
            if let Some(tray) = self.tray.as_ref()
                && let Err(error) = set_mark(tray, mark)
            {
                // The files are compiled in, so this is a message about the
                // machine rather than about the state, and the mark is recorded
                // either way: a repaint that changes nothing retries nothing.
                eprintln!("whirl-ui: the menu bar item's mark could not be set: {error}");
            }
            self.mark = mark;
        }

        // The window a `Settings…` click asked for. Showing a viewport is this
        // thread's to do, which is why the command thread left the panes for this
        // pass instead of showing anything itself.
        //
        // A click with the window already up raises that window instead, and the
        // panes the command thread read are dropped rather than put on screen:
        // replacing the dialog's state would throw away whatever is half-edited
        // in it, and a second window is not something this app has. Which of the
        // two a click is, and what the raise does, is `window::settings_asked`.
        if let Some(panes) = self.shared.take_window_request() {
            match window::settings_asked(self.dialog.open(), &window::AppKit) {
                Some(steps) => eprintln!(
                    "whirl-ui: {} raised the settings window: {steps:?}",
                    unix_nanos()
                ),
                None => self.dialog.open_settings(panes),
            }
        }

        // The `Quit` row. `app` is told the app is going first, so the close sent
        // below is read as the app's: the window's own rule would answer it by
        // hiding the viewport, and the app would stay up.
        if self.shared.take_quit_request() {
            self.dialog.quit();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
            return;
        }

        // The window's own state: its close button hides the window and leaves the
        // app up. That is `app`'s rule, and running it here is what makes this the
        // settings window rather than a second window that resembles it.
        eframe::App::logic(&mut self.dialog, ctx, frame);

        // And the window itself, from that state. Last, so that a close button,
        // which `app`'s rule answers by hiding the window, is seen here in the
        // same pass.
        self.state_window(ctx);
    }

    /// The settings window's body, drawn by `app`.
    ///
    /// One window draws these panes behind the menu bar item's `Settings…` row,
    /// and one more standalone window draws them for `--screenshot`: the same
    /// body from the same value, which is the point of `app::panes`. With the
    /// window closed there is nothing to draw, and this is the same empty frame
    /// `app` draws.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.dialog, ui, frame);
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

/// Whether a pass must tell the framework the settings window is visible, given
/// whether a dialog is open and what the framework was last told.
///
/// `None` is "told nothing yet", which is the first pass, and it is why the app
/// speaks at launch even though [`run`] asked the builder for a hidden window:
/// eframe shows a root window after its first painted frame, so nothing the
/// builder was asked for is still true by then. Everything else is one change per
/// change: `Some(visible)` only when the window should be a different state from
/// the one the framework last heard about.
fn visibility(dialog_open: bool, stated: Option<bool>) -> Option<bool> {
    (stated != Some(dialog_open)).then_some(dialog_open)
}

/// Wall-clock nanoseconds, the clock the timing in `docs/milestones.md` M1
/// criterion 4 compares against.
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
    fn a_settings_click_leaves_its_panes_for_the_pass_that_opens_the_window() {
        // The panes are four reads of the daemon, made on the command thread, and
        // the viewport is the UI thread's to show: they wait here in between, and
        // taking them is what the pass that opens the window does.
        let shared = Shared::new();
        assert!(shared.take_window_request().is_none(), "nothing asked yet");
        let panes = Settings::unreachable("the daemon is not reachable");
        shared.request_window(panes.clone());
        assert_eq!(shared.take_window_request().as_ref(), Some(&panes));
        assert!(shared.take_window_request().is_none(), "taken once");
    }

    #[test]
    fn a_quit_click_waits_for_the_pass_that_closes_the_window_first() {
        // The close that ends the app is sent by the UI thread, after it has
        // closed the window: sent from the click it would reach the window's own
        // close rule and be cancelled, and the app would not go.
        let shared = Shared::new();
        assert!(!shared.take_quit_request(), "nothing asked yet");
        shared.request_quit();
        assert!(shared.take_quit_request());
        assert!(!shared.take_quit_request(), "taken once");
    }

    #[test]
    fn a_name_this_app_did_not_put_in_its_menu_does_nothing() {
        assert_eq!(click("sep"), Click::Nothing);
        assert_eq!(click(""), Click::Nothing);
        assert_eq!(click("--menu-dump"), Click::Nothing);
    }

    #[test]
    fn the_first_pass_hides_the_window_because_it_has_no_dialog_to_show() {
        // The regression this guard is for: the tray asked for a hidden window
        // with `with_visible(false)` and nothing else, eframe showed the window
        // after the first painted frame anyway (`epi_integration::post_rendering`,
        // eframe 0.36.2), and the app presented an empty window at launch (10 of
        // 10 measured, 2026-10-04).
        // `None` is "the framework has not been told anything", and a pass that
        // has no dialog must answer it with a hidden window rather than with
        // silence.
        assert_eq!(visibility(false, None), Some(false));
    }

    #[test]
    fn only_a_dialog_puts_the_window_on_screen() {
        // Showing is what a `Settings…` click does, and nothing else does it: the
        // panes are read before the window request, so the pass that shows the
        // window is a pass that has panes to draw.
        assert_eq!(visibility(true, Some(false)), Some(true));
        assert_eq!(visibility(true, None), Some(true));
        // A window that is already in the state its dialog asks for is left
        // alone, so a repaint that changed nothing sends nothing.
        assert_eq!(visibility(true, Some(true)), None);
        assert_eq!(visibility(false, Some(false)), None);
        // A dialog that closed takes the window off the screen again.
        assert_eq!(visibility(false, Some(true)), Some(false));
    }
}
