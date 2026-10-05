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
//!
//! A write mode is the control's own code and not a second path beside it: it
//! builds the same [`Settings`] the window builds, puts its arguments in the same
//! fields, and calls the same method the button calls. What it adds is an exit
//! code and a place in the shell, which is what makes a change something a test
//! can run and a person can script.

mod about;
mod app;
mod config_file;
mod dump;
mod icon;
mod keychain;
mod login_item;
mod menu;
mod settings;
mod state;
mod theme;
#[cfg(target_os = "macos")]
mod tray;

use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use about::Check;
use config_file::Direction;
use dump::{EXIT_OK, EXIT_REFUSED, EXIT_UNREACHABLE, EXIT_USAGE, Mode, print_line};
use settings::{Outcome, Pane, Settings, Unit};

const USAGE: &str = "\
whirl-ui: a menu bar frontend for the whirl wallpaper daemon

usage: whirl-ui [mode]

With no mode the app starts its menu bar item (macOS). Its `Settings…` row opens
the settings window on the config file; the sources and the rotation there are
what it writes, and closing the window does not quit the app. The app never
starts a daemon.

modes:
  --menu-dump           print the menu bar item's rows, in menu order
  --dump-status         print the daemon's status, key by key
  --dump-sources        print the sources, with each one's enabled state and reason
  --dump-config-check   print the effective plan the daemon adopted
  --dump-settings       print what the settings window shows
  --set-rotation <value> <minutes|hours>
                        write the rotation to the config file, the same path the
                        window's `Every [n] [unit]` control writes
  --source <verb> ...   edit the sources, the same paths the window's Sources
                        section writes. Verbs:
                          add-folder <folder>       add a folder, deriving its id
                          add <id> <folder>         add a folder under a given id
                          set-folder <id> <folder>  point a source at a folder
                          add-wallhaven [<id>]      a source pointing at the key
                                                    label, with the id derived
                                                    when it is left out
                          remove <id>
                          enable <id>               weight 1
                          disable <id>              weight 0
                          move <id> <up|down>
                          store-token               read the token from stdin and
                                                    write it to the platform
                                                    store, once (never as an
                                                    argument)
                          status                    the store item's attributes,
                                                    never its value
  --login-item <verb>   the app's own login item, through macOS's own API
                        (macOS only, and only from inside Whirl.app):
                          status      what macOS reports about it now
                          register    ask macOS to start the app at login
                          unregister  remove the registration
  --screenshot <path> [state]
                        run the window, write it to a PNG, and exit. The state
                        names the pane to photograph and any control to open on
                        it. The panes are sources (the default), rotation, app
                        and about; the control states are chooser and key (on
                        the Sources pane), rejected and words (on Rotation), and
                        check-newer, check-newest and check-failed (the About
                        pane's release check, one outcome each), and each one is
                        put on through the same method the control it shows calls
  --check-update        make the release check the About pane's button makes,
                        print its one line, and exit. It needs no daemon.
  -h, --help            print this

The dump modes talk to a running daemon: exit 0 on success, 1 if the daemon
refused, 2 if it is not reachable, 3 if the command line cannot work.
`--menu-dump` is the exception: it exits 0 whether or not a daemon is running,
because a menu bar item that says the daemon is not running is a row list and
not a failure. `--set-rotation` exits 0 when the file is written, 1 when the
parser refused the value or the file could not be written, and 3 when the value
is not a number or the unit is not minutes or hours; it needs no daemon, because
the file is what it writes. `--source` exits 0 when the edit landed, 1 when the
parser or the store refused it, and 3 when the verb or its arguments do not make
a command line; it needs no daemon either, for the same reason. `--check-update`
makes the release check the About pane's button makes: exit 0 when the check was
made (a newer release is published, or this one is the newest), and 2 when it
could not be made, with the reason on the line it prints. It needs no daemon.
`--login-item` exits 0 when it has printed the status it was asked for (every
verb ends by printing it, so a before and an after are the same line), 1 when
macOS refused the change or the executable is not inside an app bundle, which is
the one thing a login item needs and a bare binary has not got, and 3 when the
verb is not one of the three.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(first) = args.first().cloned() else {
        return without_a_mode();
    };

    if first == "-h" || first == "--help" {
        print_line(USAGE);
        return ExitCode::from(EXIT_OK);
    }

    if first == "--set-rotation" {
        if args.len() != 3 {
            eprintln!("whirl-ui: --set-rotation takes a value and a unit (minutes or hours)");
            return ExitCode::from(EXIT_USAGE);
        }
        return set_rotation(&args[1], &args[2]);
    }

    if first == "--source" {
        return source_command(&args[1..]);
    }

    if first == "--check-update" {
        if args.len() != 1 {
            eprintln!("whirl-ui: --check-update takes no arguments");
            return ExitCode::from(EXIT_USAGE);
        }
        return check_update();
    }

    if first == "--login-item" {
        return login_item_command(&args[1..]);
    }

    if first == "--screenshot" {
        let Some(path) = args.get(1) else {
            eprintln!("whirl-ui: --screenshot takes the path to write");
            return ExitCode::from(EXIT_USAGE);
        };
        if args.len() > 3 {
            eprintln!("whirl-ui: --screenshot takes a path and an optional state");
            return ExitCode::from(EXIT_USAGE);
        }
        let snap = match args.get(2) {
            None => Snap::Sources,
            Some(name) => match Snap::parse(name) {
                Some(snap) => snap,
                None => {
                    eprintln!("whirl-ui: {name:?} is not a state; try {}", Snap::NAMES);
                    return ExitCode::from(EXIT_USAGE);
                }
            },
        };
        return window(Some(PathBuf::from(path)), snap);
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

/// A state `--screenshot` can photograph.
///
/// The window is the one deliverable a test cannot open, so a screenshot is how
/// it becomes evidence rather than a claim about it. These are the states worth a
/// photograph: the four panes, the controls that open on two of them, and the
/// About pane's release check in each of its three outcomes. Each one is put on
/// through the same method the control calls rather than by drawing something
/// that resembles it.
///
/// All of them leave the config file as they found it: the chooser and the key
/// field write nothing, and `rejected` writes nothing because the save it makes
/// is one the daemon's own parser refuses before the file is touched.
///
/// The three check states make no request either. A release is put in through
/// [`about::judge`], the same decision the live check uses, because this
/// repository has published no release and the two positive branches would
/// otherwise be unphotographable; the live path is `--check-update`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Snap {
    /// The Sources pane, as the window opens.
    Sources,
    /// The Rotation pane.
    Rotation,
    /// The App pane.
    App,
    /// The About pane, as the window opens: no check made yet.
    About,
    /// The folder chooser, open on where a new source would start.
    Chooser,
    /// The Wallhaven key field, open.
    Key,
    /// A rotation the parser refuses, and the line that says so.
    Rejected,
    /// A rotation that is words rather than a number: the window's own refusal,
    /// which no command line can reach, because a command line refuses the
    /// argument before the window's field ever holds it.
    Words,
    /// The About pane after a check found a newer release.
    CheckNewer,
    /// The About pane after a check found the running version is the newest.
    CheckNewest,
    /// The About pane after a check that could not be made.
    CheckFailed,
}

impl Snap {
    /// Every state, in the order the usage line lists them.
    const ALL: [Snap; 11] = [
        Snap::Sources,
        Snap::Rotation,
        Snap::App,
        Snap::About,
        Snap::Chooser,
        Snap::Key,
        Snap::Rejected,
        Snap::Words,
        Snap::CheckNewer,
        Snap::CheckNewest,
        Snap::CheckFailed,
    ];

    /// The states as the usage line spells them.
    const NAMES: &'static str = "sources, rotation, app, about, chooser, key, rejected, words, \
                                 check-newer, check-newest, check-failed";

    /// The word a command line uses for this state.
    fn name(self) -> &'static str {
        match self {
            Snap::Sources => "sources",
            Snap::Rotation => "rotation",
            Snap::App => "app",
            Snap::About => "about",
            Snap::Chooser => "chooser",
            Snap::Key => "key",
            Snap::Rejected => "rejected",
            Snap::Words => "words",
            Snap::CheckNewer => "check-newer",
            Snap::CheckNewest => "check-newest",
            Snap::CheckFailed => "check-failed",
        }
    }

    /// The state a command line asks for, when it asks for one.
    fn parse(name: &str) -> Option<Snap> {
        Snap::ALL.into_iter().find(|snap| snap.name() == name)
    }

    /// Put the window into this state, through the methods its controls call.
    fn apply(self, settings: &mut Settings) {
        match self {
            Snap::Sources => settings.pane = Pane::Sources,
            Snap::Rotation => settings.pane = Pane::Rotation,
            Snap::App => settings.pane = Pane::App,
            Snap::About => settings.pane = Pane::About,
            Snap::CheckNewer => {
                settings.pane = Pane::About;
                settings.check = Some(about::judge(
                    &settings.running_version(),
                    &about::Release {
                        version: "0.2.0".to_string(),
                        page: "https://github.com/guruor/whirl-ui/releases/tag/v0.2.0".to_string(),
                    },
                ));
            }
            Snap::CheckNewest => {
                settings.pane = Pane::About;
                let running = settings.running_version();
                settings.check = Some(about::judge(
                    &running,
                    &about::Release {
                        version: running.clone(),
                        page: "https://github.com/guruor/whirl-ui/releases/tag/v0.1.0".to_string(),
                    },
                ));
            }
            Snap::CheckFailed => {
                settings.pane = Pane::About;
                // The wording curl uses for a host it cannot resolve, which is
                // the reason a check on a machine with no network carries.
                settings.check = Some(Check::CouldNot {
                    reason: "Could not resolve host: api.github.com".to_string(),
                });
            }
            Snap::Chooser => {
                settings.pane = Pane::Sources;
                settings.open_picker(None);
            }
            Snap::Key => {
                settings.pane = Pane::Sources;
                settings.key.open = true;
            }
            Snap::Rejected => {
                settings.pane = Pane::Rotation;
                // Half a minute is below the floor whirl-core enforces, so the
                // save is refused and the file is not written.
                settings.interval.value = "0.5".to_string();
                settings.interval.unit = Unit::Minutes;
                settings.save_interval();
            }
            Snap::Words => {
                settings.pane = Pane::Rotation;
                settings.interval.value = "half an hour".to_string();
                settings.interval.unit = Unit::Minutes;
                settings.save_interval();
            }
        }
    }
}

/// Make the release check, the same call the About pane's button makes.
///
/// It needs no daemon and no window: the check is one request to this app's own
/// published releases and nothing else, so this is the button's behaviour with a
/// place in the shell. Exit 0 when the check was made, and 2 when it could not
/// be: a check that could not be made is not a success, and it says so on the
/// line it prints.
fn check_update() -> ExitCode {
    let check = about::check();
    print_line(&check.line());
    match check {
        Check::CouldNot { .. } => ExitCode::from(EXIT_UNREACHABLE),
        Check::Newer(_) | Check::Newest(_) => ExitCode::from(EXIT_OK),
    }
}

/// Write the rotation to the config file, without a window.
///
/// This is the window's write path rather than a second one beside it: it builds
/// the same [`Settings`] the window builds, puts the arguments in the same two
/// fields the `Every [n] [unit]` control holds, and calls the same
/// `save_interval` the `Save` button calls.
///
/// It needs no daemon: the config file is what it writes, and with no daemon the
/// window falls back to the platform's own path, exactly as it does when a user
/// opens the window before starting one.
fn set_rotation(value: &str, unit: &str) -> ExitCode {
    // Whether the value is a number is a question about the command line, so it
    // is classified here and answered with the usage code. Whether the number is
    // *acceptable* is the parser's question, and that is answered inside
    // `save_interval`, with the parser's own message.
    if value.parse::<f64>().is_err() {
        eprintln!("whirl-ui: {value:?} is not a number");
        return ExitCode::from(EXIT_USAGE);
    }
    let Some(unit) = unit_of(unit) else {
        eprintln!("whirl-ui: the unit is minutes or hours, not {unit:?}");
        return ExitCode::from(EXIT_USAGE);
    };
    let mut settings = window_state();
    settings.interval.value = value.to_string();
    settings.interval.unit = unit;
    settings.save_interval();
    print_rotation_edit(&settings)
}

/// The unit a person typed, as the one the control holds.
fn unit_of(argument: &str) -> Option<Unit> {
    Unit::ALL.into_iter().find(|unit| unit.name() == argument)
}

/// What the window's Rotation section would show after an edit, as exit codes.
fn print_rotation_edit(settings: &Settings) -> ExitCode {
    let Some(outcome) = &settings.interval.outcome else {
        return ExitCode::from(EXIT_OK);
    };
    match outcome {
        Outcome::Saved { .. } => {
            if let Some(target) = settings.target.as_ref() {
                print_line(&format!("config: {}", target.path.display()));
            }
            // The section's own line, so what a terminal prints here is what a
            // person would read in the window after the same save. The value that
            // landed is in the file the line names, which is where a script reads
            // it from.
            print_line(outcome.line().as_str());
            ExitCode::from(EXIT_OK)
        }
        Outcome::Refused { .. } => {
            // The section's own line, so what a terminal prints here is what a
            // person would read in the window after the same save.
            eprintln!("whirl-ui: {}", outcome.line());
            ExitCode::from(EXIT_REFUSED)
        }
    }
}

/// The window's state with no window: the same [`Settings`] the window builds
/// from the same two requests, so a mode here drives the button's own code.
///
/// It needs no daemon: with none the window falls back to the platform's config
/// path, exactly as it does when a user opens the window before starting one.
fn window_state() -> Settings {
    let (answers, _code) = dump::settings_answers();
    Settings::from_answers(&answers)
}

/// The source edits, driven through the same [`Settings`] methods the window's
/// Sources section buttons call.
///
/// The verbs are the controls: `add-folder` and `add-wallhaven` are the two add
/// buttons, `set-folder` is the `Change…` button, `remove`, `enable`, `disable`
/// and `move` are the per-row controls, and `store-token` is `Save key`.
/// `add` and the id form of `add-wallhaven` are the same writes with the id named
/// by the caller, which a command line needs and a person clicking does not.
/// `status` is the one verb that writes nothing: it prints the store item's
/// attributes, which is what a person can see about the item without asking for
/// its value.
fn source_command(args: &[String]) -> ExitCode {
    let Some(verb) = args.first() else {
        return usage(VERBS);
    };
    let rest = &args[1..];
    let mut settings = window_state();
    match verb.as_str() {
        "add-folder" if rest.len() == 1 => settings.add_folder(&rest[0]),
        "add" if rest.len() == 2 => settings.add_folder_as(&rest[0], &rest[1]),
        "set-folder" if rest.len() == 2 => settings.set_source_folder(&rest[0], &rest[1]),
        "add-wallhaven" if rest.is_empty() => settings.add_wallhaven(),
        "add-wallhaven" if rest.len() == 1 => settings.add_wallhaven_as(&rest[0]),
        "remove" if rest.len() == 1 => settings.remove_source(&rest[0]),
        "enable" if rest.len() == 1 => settings.set_source_enabled(&rest[0], true),
        "disable" if rest.len() == 1 => settings.set_source_enabled(&rest[0], false),
        "move" if rest.len() == 2 => {
            let Some(direction) = direction_of(&rest[1]) else {
                return usage("--source move takes up or down");
            };
            settings.move_source(&rest[0], direction);
        }
        "store-token" if rest.is_empty() => {
            // The token is read from standard input and never from an argument:
            // an argument is in the process table and in the shell's history.
            let Some(token) = read_secret() else {
                return usage("--source store-token reads the token from standard input");
            };
            // The field is the one the window's key box fills; `save_key` is the
            // button behind it.
            settings.key.token = token;
            settings.save_key();
        }
        "status" if rest.is_empty() => return keychain_status(),
        _ => return usage(VERBS),
    }
    print_sources_edit(&settings)
}

/// The verbs, as the usage line spells them.
const VERBS: &str = "--source takes a verb: add-folder <folder>, add <id> <folder>, \
                     set-folder <id> <folder>, add-wallhaven [<id>], remove <id>, \
                     enable <id>, disable <id>, move <id> <up|down>, store-token, status";

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

/// What the window's Sources section would show after an edit, as exit codes.
///
/// The lines are the section's own, so what a terminal prints here is what a
/// person would read in the window after the same click: the rows, in the file's
/// order, and the one line under the controls.
fn print_sources_edit(settings: &Settings) -> ExitCode {
    let Some(outcome) = &settings.sources.outcome else {
        return ExitCode::from(EXIT_OK);
    };
    match outcome {
        Outcome::Saved { .. } => {
            if let Some(target) = settings.target.as_ref() {
                print_line(&format!("config: {}", target.path.display()));
            }
            for row in &settings.sources.rows {
                print_line(&row.text_line());
            }
            print_line(outcome.line().as_str());
            ExitCode::from(EXIT_OK)
        }
        Outcome::Refused { .. } => {
            // The section's own line, so what a terminal prints here is what a
            // person would read in the window after the same click.
            eprintln!("whirl-ui: {}", outcome.line());
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

/// The app's own login item, from a terminal.
///
/// The three verbs are Apple's two calls and its one reader: `register` and
/// `unregister` change the login item, and `status` is what macOS reports about
/// it. Every verb ends by printing the status, so the line before a change and
/// the line after it are the same line read twice, which is what M3 criterion 1
/// asks a reader to compare.
///
/// The bundle check comes first, and it is why the mode looks like this: a login
/// item is a registration of an *app bundle*, so an executable that is not inside
/// one has nothing to register. It is told so, with the path it looked at, rather
/// than silently registering whichever directory the binary happens to sit in.
fn login_item_command(args: &[String]) -> ExitCode {
    let Some(verb @ ("status" | "register" | "unregister")) = args.first().map(String::as_str)
    else {
        return usage(LOGIN_ITEM_VERBS);
    };
    if args.len() > 1 {
        return usage(LOGIN_ITEM_VERBS);
    }
    let Some(bundle) = login_item::bundle() else {
        let looked_at = env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| String::from("<this executable>"));
        eprintln!(
            "whirl-ui: {looked_at} is not inside an app bundle, so it has no login item of its \
             own; a login item is a registration of the app's bundle, and Whirl.app is the bundle \
             this app ships as (scripts/make-bundle.sh writes one)"
        );
        return ExitCode::from(EXIT_REFUSED);
    };
    match verb {
        "register" => {
            if let Err(error) = login_item::register() {
                eprintln!("whirl-ui: registering the login item failed: {error}");
                return ExitCode::from(EXIT_REFUSED);
            }
        }
        "unregister" => {
            if let Err(error) = login_item::unregister() {
                eprintln!("whirl-ui: unregistering the login item failed: {error}");
                return ExitCode::from(EXIT_REFUSED);
            }
        }
        // `status` changes nothing, which is the point of it.
        _ => {}
    }
    match login_item::status() {
        Ok(status) => {
            print_line(&status.line());
            print_line(&bundle.line());
            ExitCode::from(EXIT_OK)
        }
        Err(error) => {
            eprintln!("whirl-ui: {error}");
            ExitCode::from(EXIT_REFUSED)
        }
    }
}

/// The `--login-item` verbs, as the usage line spells them.
const LOGIN_ITEM_VERBS: &str = "--login-item takes a verb: status, register or unregister";

/// A command line a mode cannot work, with the reason and the usage.
fn usage(message: &str) -> ExitCode {
    eprintln!("whirl-ui: {message}");
    eprintln!("{USAGE}");
    ExitCode::from(EXIT_USAGE)
}

/// Open the settings window on the config file, and run the app.
///
/// The window's state is read before the window exists, so opening it starts
/// nothing. The read is the same two requests `--dump-settings` makes, plus the
/// file itself, and with no daemon the window opens on the reason rather than
/// starting one: section 8's "must never" list has no exception for a window. The
/// controls still work there, pointed at the platform's config file, which is the
/// state the ADR decides the write path exists for.
fn window(capture: Option<PathBuf>, snap: Snap) -> ExitCode {
    let mut settings = window_state();
    snap.apply(&mut settings);
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

// The test module is last on purpose: the two `without_a_mode` arms above are
// cfg'd one or the other out, so an item written after this point would be after
// a test module on one platform and not on another.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_snapshot_state_round_trips_through_its_name() {
        for snap in Snap::ALL {
            assert_eq!(Snap::parse(snap.name()), Some(snap), "{}", snap.name());
        }
        assert_eq!(Snap::parse("sideways"), None);
        assert_eq!(Snap::parse(""), None);
    }

    /// The app is a menu bar item with a dialog, not a windowed application: it
    /// asks macOS for an accessory activation policy and never for a regular one,
    /// so it takes no Dock tile (M1 criterion 5). A Dock tile is a different
    /// product, and this is the assertion that a restyle did not introduce one.
    ///
    /// It reads the crate's own sources because the property has no runtime
    /// surface a headless test can reach: the policy is decided once, by a winit
    /// builder, before a window exists. The needle is assembled rather than
    /// written out, so the assertion is not satisfied by its own text.
    #[test]
    fn the_app_still_asks_for_no_dock_tile() {
        let regular = ["ActivationPolicy", "::", "Regular"].concat();
        let tray = include_str!("tray.rs");
        assert!(
            tray.contains("ActivationPolicy::Accessory"),
            "the app asks macOS for the accessory policy"
        );
        for (name, source) in [
            ("main.rs", include_str!("main.rs")),
            ("app.rs", include_str!("app.rs")),
            ("tray.rs", tray),
        ] {
            assert!(
                !source.contains(&regular),
                "{name} asks for a regular activation policy, which is what gives the app a Dock tile"
            );
        }
    }
}
