//! The whirl tray app.
//!
//! The tray is macOS-first. macOS-only code lands behind
//! `#[cfg(target_os = "macos")]`, so the Linux and Windows builds compile the
//! same workspace and stay green while those platforms are unfinished.
//!
//! Until the menu bar item lands, every question the app can answer is answered
//! from a terminal ([`dump`]), so the app's behaviour is testable without a
//! display attached. The exit codes are whirl's own (docs/architecture.md
//! section 8 item 7): 0 success, 1 the daemon refused, 2 the daemon is
//! unreachable, 3 the command line cannot work.

mod dump;

use std::env;
use std::process::ExitCode;

use dump::{EXIT_OK, EXIT_USAGE, Mode};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui <mode>

modes:
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window will show
  -h, --help            print this

With no arguments the app starts its menu bar item. The dump modes talk to a
running daemon: exit 0 on success, 1 if the daemon refused, 2 if it is not
reachable, 3 if the command line cannot work.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(first) = args.first() else {
        // No mode: the tray itself. It is the next card; until it exists, saying
        // so beats exiting silently.
        eprintln!("whirl-ui: the menu bar item is not implemented yet; try --dump-status");
        return ExitCode::from(EXIT_USAGE);
    };

    if first == "-h" || first == "--help" {
        dump::print_line(USAGE);
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
