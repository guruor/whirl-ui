//! The whirl tray app.
//!
//! The tray is macOS-first. macOS-only code lands behind
//! `#[cfg(target_os = "macos")]`, so the Linux and Windows builds compile the
//! same workspace and stay green while those platforms are unfinished
//! (docs/milestones.md M4).
//!
//! Two things live here: the tray, whose menu is the product, the settings
//! window ([`app`]) behind its `Settings…` row, and the headless modes
//! ([`dump`]) that answer the same questions from a terminal. The modes exist
//! because neither window can be asserted by a test: every question they answer
//! is also answerable without a display.

mod app;
mod dump;
mod menu;
mod settings;
mod state;
#[cfg(target_os = "macos")]
mod tray;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use dump::{EXIT_OK, EXIT_USAGE, Mode, print_line};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui [mode]

With no mode the app starts its menu bar item (macOS). Its `Settings…` row opens
the settings window, read-only, on what the daemon reports; closing that window
does not quit the app, and the app never starts a daemon.

modes:
  --menu-dump           print the menu bar item's rows, in menu order
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window shows
  --screenshot <path>   run the window, write it to a PNG, and exit
  -h, --help            print this

The dump modes talk to a running daemon: exit 0 on success, 1 if the daemon
refused, 2 if it is not reachable, 3 if the command line cannot work.
`--menu-dump` is the exception: it exits 0 whether or not a daemon is running,
because a menu bar item that says the daemon is not running is a row list and
not a failure.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(first) = args.first().cloned() else {
        return without_a_mode();
    };

    if first == "-h" || first == "--help" {
        print_line(USAGE);
        return ExitCode::from(EXIT_OK);
    }

    if first == "--screenshot" {
        let Some(path) = args.get(1) else {
            eprintln!("whirl-ui: --screenshot takes the path to write");
            return ExitCode::from(EXIT_USAGE);
        };
        if args.len() > 2 {
            eprintln!("whirl-ui: --screenshot takes no other arguments");
            return ExitCode::from(EXIT_USAGE);
        }
        return window(Some(PathBuf::from(path)));
    }

    let Some(mode) = Mode::parse(&first) else {
        eprintln!("whirl-ui: unknown option {first:?}");
        eprintln!("{USAGE}");
        return ExitCode::from(EXIT_USAGE);
    };

    if args.len() > 1 {
        eprintln!("whirl-ui: {} takes no arguments", mode.flag());
        return ExitCode::from(EXIT_USAGE);
    }

    dump::run(mode)
}

/// Open the settings window on what the daemon reports, and run the app.
///
/// The panes are read before the window exists, so opening it starts nothing. The
/// read is the same four requests `--dump-settings` makes, and with no daemon the
/// window opens on the reason rather than starting one: section 8's "must never"
/// list has no exception for a window.
fn window(capture: Option<PathBuf>) -> ExitCode {
    let (answers, _code) = dump::settings_answers();
    let settings = settings::Settings::from_answers(&answers);
    match app::run(settings, capture) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(error) => {
            // A machine with no display, or no graphics context: the command line
            // is fine and the daemon is fine, and there is no window to draw.
            eprintln!("whirl-ui: cannot open the settings window: {error}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// No mode: the menu bar item itself.
#[cfg(target_os = "macos")]
fn without_a_mode() -> ExitCode {
    tray::run()
}

/// No mode, on a platform the tray has not landed on yet.
///
/// The other two CI legs build this arm, which is the point: the workspace stays
/// buildable everywhere while the tray is macOS-only (docs/milestones.md M4).
#[cfg(not(target_os = "macos"))]
fn without_a_mode() -> ExitCode {
    eprintln!("whirl-ui: the menu bar item is macOS-only for now; try --menu-dump");
    ExitCode::from(EXIT_USAGE)
}
