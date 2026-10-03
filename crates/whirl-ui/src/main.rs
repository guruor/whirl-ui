//! The whirl tray app.
//!
//! The tray is macOS-first. macOS-only code lands behind
//! `#[cfg(target_os = "macos")]`, so the Linux and Windows builds compile the
//! same workspace and stay green while those platforms are unfinished
//! (docs/milestones.md M4).
//!
//! With no arguments the app starts its menu bar item on macOS. Every question
//! the item answers is also answerable from a terminal ([`dump`]), so the app's
//! behaviour is testable without a display attached. The exit codes are whirl's
//! own (docs/architecture.md section 8 item 7): 0 success, 1 the daemon refused,
//! 2 the daemon is unreachable, 3 the command line cannot work.

mod dump;
mod menu;
mod state;
#[cfg(target_os = "macos")]
mod tray;

use std::env;
use std::process::ExitCode;

use dump::{EXIT_OK, EXIT_USAGE, Mode, print_line};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui <mode>

modes:
  --menu-dump           print the menu bar item's rows, in menu order
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window will show
  -h, --help            print this

With no arguments the app starts its menu bar item. The dump modes talk to a
running daemon: exit 0 on success, 1 if the daemon refused, 2 if it is not
reachable, 3 if the command line cannot work. `--menu-dump` is the exception:
it exits 0 whether or not a daemon is running, because a menu bar item that says
the daemon is not running is a row list and not a failure.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(first) = args.first() else {
        return without_a_mode();
    };

    if first == "-h" || first == "--help" {
        print_line(USAGE);
        return ExitCode::from(EXIT_OK);
    }

    let Some(mode) = Mode::parse(first) else {
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
