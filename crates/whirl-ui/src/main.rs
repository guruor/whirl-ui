//! The whirl tray app.
//!
//! The tray is macOS-first. macOS-only code lands behind
//! `#[cfg(target_os = "macos")]`, so the Linux and Windows builds compile the
//! same workspace and stay green while those platforms are unfinished
//! (docs/milestones.md M4).
//!
//! Three things live here: the tray, whose menu is the product, the settings
//! window ([`app`]) behind its `Settings…` row, and the headless modes
//! ([`dump`], and the one write mode beside them) that answer the same questions
//! from a terminal. The modes exist because neither window can be asserted by a
//! test: every question they answer, and the one change the window can make, is
//! also reachable without a display.

mod app;
mod config_file;
mod dump;
mod menu;
mod settings;
mod state;
#[cfg(target_os = "macos")]
mod tray;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use config_file::INTERVAL_KEY;
use dump::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, Mode, print_line};
use settings::{Outcome, Settings};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui [mode]

With no mode the app starts its menu bar item (macOS). Its `Settings…` row opens
the settings window on what the daemon reports; the rotation interval there is
the one setting it writes, and closing the window does not quit the app. The app
never starts a daemon.

modes:
  --menu-dump           print the menu bar item's rows, in menu order
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window shows
  --set-interval <n>    write the rotation interval to the config file, the same
                        path the window's interval field writes
  --screenshot <path>   run the window, write it to a PNG, and exit
  -h, --help            print this

The dump modes talk to a running daemon: exit 0 on success, 1 if the daemon
refused, 2 if it is not reachable, 3 if the command line cannot work.
`--menu-dump` is the exception: it exits 0 whether or not a daemon is running,
because a menu bar item that says the daemon is not running is a row list and
not a failure. `--set-interval` exits 0 when the file is written, 1 when the
parser refused the value or the file could not be written, and 3 when the
argument is not a whole number of seconds; it needs no daemon, because the file
is what it writes.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(first) = args.first().cloned() else {
        return without_a_mode();
    };

    if first == "-h" || first == "--help" {
        print_line(USAGE);
        return ExitCode::from(EXIT_OK);
    }

    if first == "--set-interval" {
        let Some(value) = args.get(1) else {
            eprintln!("whirl-ui: --set-interval takes the number of seconds to write");
            return ExitCode::from(EXIT_USAGE);
        };
        if args.len() > 2 {
            eprintln!("whirl-ui: --set-interval takes no other arguments");
            return ExitCode::from(EXIT_USAGE);
        }
        return set_interval(value);
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

/// Write the rotation interval to the config file, without a window.
///
/// This is the window's write path rather than a second one beside it: it builds
/// the same [`Settings`] the window builds, puts the argument in the same field,
/// and calls the same `save_interval` the `Save` button calls. What it adds is an
/// exit code, so the round trip can be shown with the daemon's own commands.
///
/// It needs no daemon: the config file is what it writes, and with no daemon the
/// window falls back to the platform's own path, exactly as it does when a user
/// opens the window before starting one.
fn set_interval(value: &str) -> ExitCode {
    // Whether the argument is a number is a question about the command line, so
    // it is classified here and answered with the usage code. Whether the number
    // is *acceptable* is the parser's question, and that is answered inside
    // `save_interval`, with the parser's own message.
    if value.parse::<u64>().is_err() {
        eprintln!("whirl-ui: {value:?} is not a whole number of seconds");
        return ExitCode::from(EXIT_USAGE);
    }
    let (answers, _code) = dump::settings_answers();
    let mut settings = Settings::from_answers(&answers);
    settings.interval.input = value.to_string();
    // The path is read before the write, because the write borrows the window.
    let target = settings.target.as_ref().map(|target| target.path.clone());
    match settings.save_interval() {
        Outcome::Written { interval, warnings } => {
            if let Some(path) = target.as_ref() {
                print_line(&format!("config: {}", path.display()));
            }
            print_line(&format!("{INTERVAL_KEY}: {interval}"));
            for warning in warnings {
                print_line(&format!("warning: {warning}"));
            }
            // The window's own line, so what a terminal prints here is what the
            // Rotation pane shows after the same write.
            print_line(settings.rotation.footer.as_str());
            ExitCode::from(EXIT_OK)
        }
        Outcome::Refused { message } => {
            eprintln!("whirl-ui: {message}");
            ExitCode::from(EXIT_REFUSED)
        }
    }
}

/// Open the settings window on what the daemon reports, and run the app.
///
/// The panes are read before the window exists, so opening it starts nothing. The
/// read is the same four requests `--dump-settings` makes, and with no daemon the
/// window opens on the reason rather than starting one: section 8's "must never"
/// list has no exception for a window. The interval editor still works there,
/// pointed at the platform's config file, which is the state the ADR decides the
/// write path exists for.
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
