//! The whirl tray app.
//!
//! The tray is macOS-first. macOS-only code lands behind
//! `#[cfg(target_os = "macos")]`, so the Linux and Windows builds compile the
//! same workspace and stay green while those platforms are unfinished
//! (docs/milestones.md M4).
//!
//! Three things live here: the tray, whose menu is the product, the settings
//! window ([`app`]) behind its `Settings…` row, and the headless modes
//! ([`dump`], and the write modes beside it) that answer the same questions from
//! a terminal. The modes exist because neither window can be asserted by a test:
//! every question they answer, and every change the window can make, is also
//! reachable without a display.

mod app;
mod config_file;
mod dump;
mod icon;
mod keychain;
mod menu;
mod settings;
mod state;
#[cfg(target_os = "macos")]
mod tray;

use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use config_file::{Direction, INTERVAL_KEY};
use dump::{EXIT_OK, EXIT_REFUSED, EXIT_USAGE, Mode, print_line};
use settings::{Outcome, Settings, SourceOutcome};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui [mode]

With no mode the app starts its menu bar item (macOS). Its `Settings…` row opens
the settings window on what the daemon reports; the rotation interval and the
sources there are what it writes, and closing the window does not quit the app.
The app never starts a daemon.

modes:
  --menu-dump           print the menu bar item's rows, in menu order
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window shows
  --set-interval <n>    write the rotation interval to the config file, the same
                        path the window's interval field writes
  --source <verb> ...   edit the sources, the same paths the window's Sources
                        pane writes. Verbs:
                          add <id> <folder>       a local folder source
                          add-wallhaven <id>      a source pointing at the key label
                          remove <id>
                          enable <id>             weight 1
                          disable <id>            weight 0
                          move <id> <up|down>
                          store-token            read the token from stdin and
                                                 write it to the platform store,
                                                 once (never as an argument)
                          status                  the store item's attributes,
                                                 never its value
  --screenshot <path>   run the window, write it to a PNG, and exit
  -h, --help            print this

The dump modes talk to a running daemon: exit 0 on success, 1 if the daemon
refused, 2 if it is not reachable, 3 if the command line cannot work.
`--menu-dump` is the exception: it exits 0 whether or not a daemon is running,
because a menu bar item that says the daemon is not running is a row list and
not a failure. `--set-interval` exits 0 when the file is written, 1 when the
parser refused the value or the file could not be written, and 3 when the
argument is not a whole number of seconds; it needs no daemon, because the file
is what it writes. `--source` exits 0 when the edit landed, 1 when the parser or
the store refused it, and 3 when the verb or its arguments do not make a command
line; it needs no daemon either, for the same reason.";

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

    if first == "--source" {
        return source_command(&args[1..]);
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
    let mut settings = window_state();
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

/// The window's state with no window: the same [`Settings`] the window builds
/// from the same four requests, so a mode here drives the button's own code.
///
/// It needs no daemon: with none the window falls back to the platform's config
/// path, exactly as it does when a user opens the window before starting one.
fn window_state() -> Settings {
    let (answers, _code) = dump::settings_answers();
    Settings::from_answers(&answers)
}

/// The source edits, driven through the same [`Settings`] methods the window's
/// Sources pane buttons call.
///
/// The verbs are the buttons: `add` and `add-wallhaven` are the two add buttons,
/// `remove`, `enable`, `disable` and `move` are the per-row buttons, and
/// `store-token` is the token button. `status` is the one verb that writes
/// nothing: it prints the store item's attributes, which is what a person can see
/// about the item without asking for its value.
fn source_command(args: &[String]) -> ExitCode {
    /// The verbs, as the usage line spells them.
    const VERBS: &str = "--source takes a verb: add <id> <folder>, add-wallhaven <id>, \
                         remove <id>, enable <id>, disable <id>, move <id> <up|down>, \
                         store-token, status";
    let Some(verb) = args.first() else {
        return usage(VERBS);
    };
    let rest = &args[1..];
    let mut settings = window_state();
    let outcome = match verb.as_str() {
        "add" if rest.len() == 2 => {
            settings.editor.id = rest[0].clone();
            settings.editor.folder = rest[1].clone();
            settings.add_local_source().clone()
        }
        "add-wallhaven" if rest.len() == 1 => {
            settings.editor.id = rest[0].clone();
            settings.add_wallhaven_source().clone()
        }
        "remove" if rest.len() == 1 => settings.remove_source(&rest[0]).clone(),
        "enable" if rest.len() == 1 => settings.set_source_enabled(&rest[0], true).clone(),
        "disable" if rest.len() == 1 => settings.set_source_enabled(&rest[0], false).clone(),
        "move" if rest.len() == 2 => {
            let Some(direction) = direction_of(&rest[1]) else {
                return usage("--source move takes up or down");
            };
            settings.move_source(&rest[0], direction).clone()
        }
        "store-token" if rest.is_empty() => {
            // The token is read from standard input and never from an argument:
            // an argument is in the process table and in the shell's history.
            let Some(token) = read_secret() else {
                return usage("--source store-token reads the token from standard input");
            };
            settings.editor.token = token;
            settings.store_wallhaven_token().clone()
        }
        "status" if rest.is_empty() => return keychain_status(),
        _ => return usage(VERBS),
    };
    print_sources_edit(&settings, &outcome)
}

/// Which way a source moves, from the word a person typed.
fn direction_of(argument: &str) -> Option<Direction> {
    match argument {
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

/// One line from standard input, with the trailing newline removed.
///
/// `None` for empty input rather than an empty token: an empty token is not a
/// thing a person meant to store.
fn read_secret() -> Option<String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).ok()?;
    let line = text.trim_end_matches(['\n', '\r']).to_string();
    if line.is_empty() { None } else { Some(line) }
}

/// What the window's Sources pane would show after an edit, as exit codes.
///
/// The lines are the pane's own, so what a terminal prints here is what a person
/// would read in the window after the same click.
fn print_sources_edit(settings: &Settings, outcome: &SourceOutcome) -> ExitCode {
    match outcome {
        // The pane's own lines, and its own footer: the footer already carries
        // the parser's warnings, so they are not printed twice.
        SourceOutcome::Written { .. } => {
            if let Some(target) = settings.target.as_ref() {
                print_line(&format!("config: {}", target.path.display()));
            }
            for line in &settings.sources.lines {
                print_line(line);
            }
            print_line(settings.sources.footer.as_str());
            ExitCode::from(EXIT_OK)
        }
        SourceOutcome::Refused { message } => {
            eprintln!("whirl-ui: {message}");
            ExitCode::from(EXIT_REFUSED)
        }
    }
}

/// The store item's attributes, and never its value.
///
/// This runs the same query `exists` runs and keeps the attribute lines; a line
/// that carried password data was dropped before it left the store module, so
/// what a terminal prints here is the item's identity and not its secret.
fn keychain_status() -> ExitCode {
    match keychain::metadata() {
        Ok(lines) if lines.is_empty() => {
            print_line(&format!(
                "keychain: no item for service {}",
                keychain::SERVICE
            ));
            ExitCode::from(EXIT_REFUSED)
        }
        Ok(lines) => {
            print_line(&format!("keychain: item for service {}", keychain::SERVICE));
            for line in &lines {
                print_line(line);
            }
            ExitCode::from(EXIT_OK)
        }
        Err(error) => {
            eprintln!("whirl-ui: {error}");
            ExitCode::from(EXIT_REFUSED)
        }
    }
}

/// A command line a mode cannot work, with the reason and the usage.
fn usage(message: &str) -> ExitCode {
    eprintln!("whirl-ui: {message}");
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
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
    let settings = window_state();
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
