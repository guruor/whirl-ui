//! The daemon's lifecycle, through the daemon's own command.
//!
//! The frontend contract forbids a frontend from spawning a daemon
//! (`whirl`'s `docs/architecture.md` section 8, "must never" 3, as amended): a
//! frontend that starts one of its own gets a second daemon, or a stale socket.
//! The permitted route is different, and this module is the whole of it: the app
//! may **ask the supervisor, through the daemon's own command, and never spawn a
//! process**. So the one process this module starts is `whirl daemon …`, and the
//! daemon decides what that means. `whirl daemon start` does not spawn a
//! `whirld` either; it asks the OS supervisor to run the job it already owns.
//!
//! Every claim here is the CLI's own contract (`whirl`'s `docs/architecture.md`
//! 2.5.1 and section 8 item 8): exit 0 the step was done, 1 whirl refused, 2
//! there is no supervised daemon to reach (or the supervisor could not be
//! asked), 3 the command line was wrong. [`Outcome`] is that contract as a
//! value, and the words beside it are exactly what the command printed, so a
//! refusal is shown rather than replaced with a route of the app's own.
//!
//! Nothing here writes a unit file, unlinks a socket, or signals the daemon, and
//! this module never reads or opens the control socket: the daemon's lifecycle
//! is the daemon's. The one process it may end is its own child, when that child
//! does not answer within `DEADLINE`: a wait that never returns is the one
//! failure this app must never have, and ending the child the app started is not
//! a control of the daemon. Watching whether the socket file is *there* is a
//! different thing, and it is not here: it is the tray's one observation, a
//! `stat` that takes the app's view offline, and it changes nothing about the
//! daemon.

use std::env;
use std::ffi::OsStr;
use std::io::{Error, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// The command's own name: the basename every candidate shares, and the name
/// looked up on `PATH` last.
///
/// The command is a name here and a path everywhere else: [`resolve`] finds the
/// absolute path of the copy to run, so the app can report which one it used
/// instead of describing a layout it does not own. `PATH` alone is not enough,
/// and that is the bug this constant used to hide: a GUI launched from Finder
/// inherits launchd's `PATH`, which never holds `~/.local/bin`, where
/// `install.sh` puts the daemon.
pub const PROGRAM: &str = "whirl";

/// Where the daemon's own command is, in the order its installer makes it
/// findable, named by an absolute path.
///
/// The search is the one `install.sh` writes down, not a guess:
///
/// 1. the `binary <path>` line for `whirl` in the install receipt, whatever
///    prefix the install used;
/// 2. `$WHIRL_PREFIX/whirl` when the installer was told a prefix, else
///    `~/.local/bin/whirl`, the default `install.sh:152` writes to;
/// 3. `/usr/local/bin/whirl`, where a `[sudo] make install` would put it;
/// 4. `whirl` on `PATH`, as the app always did.
///
/// `None` means no copy was found anywhere this app looks. That is the only
/// case the command may be called missing: a `whirl` on `PATH` is the last
/// resort, not the first.
pub fn resolve() -> Option<PathBuf> {
    if let Some(receipt) = receipt_path().and_then(|receipt| std::fs::read_to_string(receipt).ok())
        && let Some(program) = receipt_binary(&receipt).filter(|program| program.is_file())
    {
        return Some(program);
    }
    if let Some(program) = prefix_candidate().filter(|program| program.is_file()) {
        return Some(program);
    }
    let system = Path::new("/usr/local/bin").join(PROGRAM);
    if system.is_file() {
        return Some(system);
    }
    on_path()
}

/// The `binary <path>` line the installer wrote for the command, if any.
///
/// The receipt is `install.sh`'s own record: one `binary <absolute path>` line
/// per binary it placed, beside the `dir`, `app` and `unit` lines this search
/// ignores. The path is returned whether or not it exists; the caller decides
/// what a path that no longer exists means (nothing, and it keeps looking).
pub fn receipt_binary(receipt: &str) -> Option<PathBuf> {
    receipt.lines().find_map(|line| {
        let path = Path::new(line.strip_prefix("binary ")?);
        (path.file_name() == Some(OsStr::new(PROGRAM))).then(|| path.to_path_buf())
    })
}

/// The receipt `install.sh` writes, and this app reads.
///
/// `WHIRL_UI_RECEIPT` is the installer's own name for the path, so a receipt
/// written elsewhere is still found; the default is
/// `~/Library/Application Support/whirl-ui/install.receipt`.
fn receipt_path() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("WHIRL_UI_RECEIPT") {
        return Some(PathBuf::from(explicit));
    }
    home().map(|home| home.join("Library/Application Support/whirl-ui/install.receipt"))
}

/// `$WHIRL_PREFIX/whirl` when a prefix was given, else `~/.local/bin/whirl`.
fn prefix_candidate() -> Option<PathBuf> {
    match env::var_os("WHIRL_PREFIX") {
        Some(prefix) => Some(PathBuf::from(prefix).join(PROGRAM)),
        None => home().map(|home| home.join(".local/bin").join(PROGRAM)),
    }
}

/// The home directory the process inherited, when it has a non-empty one.
fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| !home.as_os_str().is_empty())
}

/// The first `whirl` on `PATH`, named by its absolute path.
fn on_path() -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(PROGRAM))
        .find(|program| program.is_file())
}

/// The places the search looked, as the missing message names them.
///
/// The receipt is named by its path so a user can see which file was read, and
/// the install prefix is written `~`-relative when it is under the home
/// directory, which is the form the installer's own output uses.
fn searched_places() -> Vec<String> {
    let mut places = Vec::new();
    match receipt_path() {
        Some(receipt) => places.push(format!("the install receipt {}", tilde(&receipt))),
        None => places.push("the install receipt".to_string()),
    }
    match prefix_candidate() {
        Some(prefix) => places.push(tilde(&prefix)),
        None => places.push("~/.local/bin/whirl".to_string()),
    }
    places.push("/usr/local/bin/whirl".to_string());
    places.push(format!("{PROGRAM} on PATH"));
    places
}

/// A path under the home directory, spelled the way the installer spells it.
fn tilde(path: &Path) -> String {
    match home().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The three lifecycle steps the app is allowed to ask for. `install` and
/// `uninstall` are deliberately absent: writing the login unit is the daemon's
/// own onboarding, not a frontend's, and section 8 item 8 lists the five the
/// daemon has while this app asks for three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Ask the supervisor to run the job it already owns.
    Start,
    /// Ask the supervisor to unload the job.
    Stop,
    /// Ask the supervisor what it says about the job.
    Status,
}

impl Verb {
    /// Every verb, in the order this module names them.
    pub const ALL: [Verb; 3] = [Verb::Start, Verb::Stop, Verb::Status];

    /// The subcommand word, exactly as the CLI's usage spells it.
    pub fn word(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Status => "status",
        }
    }

    /// The verb a word names, or `None` for anything else.
    pub fn parse(word: &str) -> Option<Verb> {
        Verb::ALL.into_iter().find(|verb| verb.word() == word)
    }
}

/// What one `whirl daemon …` call did, as the CLI's exit codes tell it.
///
/// Each variant carries the command's own words: the line the CLI printed on
/// the stream that code uses. The app quotes them and never paraphrases them, so
/// a refusal from an older daemon or a missing command reads the same on screen
/// as it does in a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Exit 0: the step was done.
    Done(String),
    /// Exit 1: whirl refused, and said why.
    Refused(String),
    /// Exit 2: there is no supervised daemon to reach, or the supervisor could
    /// not be asked.
    Unreachable(String),
    /// Exit 3: the command line was wrong, which is what an older daemon that
    /// does not know the verb answers with.
    Usage(String),
    /// The command is not on `PATH` at all, so there are no words of its own.
    Missing(String),
    /// The command could not be run, or ended without an exit code.
    Failed(String),
    /// The command did not answer within the app's deadline and was ended, so
    /// there are no words of the daemon's at all. The sentence is the app's own,
    /// and it deliberately does not read as a refusal: the daemon never
    /// answered.
    Unanswered(String),
}

impl Outcome {
    /// The command's own words, verbatim.
    pub fn words(&self) -> &str {
        match self {
            Outcome::Done(words)
            | Outcome::Refused(words)
            | Outcome::Unreachable(words)
            | Outcome::Usage(words)
            | Outcome::Missing(words)
            | Outcome::Failed(words)
            | Outcome::Unanswered(words) => words,
        }
    }

    /// Whether the step was done.
    pub fn done(&self) -> bool {
        matches!(self, Outcome::Done(_))
    }

    /// The exit code section 8 item 7 gives this outcome, which is also the
    /// code the app's own headless mode exits with.
    pub fn exit_code(&self) -> u8 {
        match self {
            Outcome::Done(_) => 0,
            Outcome::Refused(_) => 1,
            Outcome::Unreachable(_) => 2,
            // A wait that ran out is the same family as unreachable: the
            // supervisor could not be asked, so there is no daemon to report.
            // Deliberately not 1, which would read as the daemon refusing.
            Outcome::Unanswered(_) => 2,
            // The command line cannot work: the command is not there, it could
            // not be run, or it does not know the verb.
            Outcome::Usage(_) | Outcome::Missing(_) | Outcome::Failed(_) => 3,
        }
    }
}

/// How long the app waits for the daemon's own command to answer.
///
/// The wait this bounds is a supervisor call, not a rotation: the installed
/// `whirl daemon status` answered in 60-80 ms on the development machine
/// (measured 2026-10-06, twenty runs), and `install` and `uninstall` add one
/// supervisor call each on top of that. Thirty seconds is roughly four hundred
/// times the measured status, so a step that is merely slow still finishes, and a
/// child that never answers is ended rather than waited on, which is the one
/// failure this app must never have.
const DEADLINE: Duration = Duration::from_secs(30);

/// `WHIRL_UI_DAEMON_DEADLINE_MS` names the deadline in milliseconds.
///
/// The override is a test's input rather than something the app offers a user:
/// this app's own tests drive a stand-in `whirl` that sleeps past the bound, and
/// waiting out the real bound twice would only make the suite slow. It is the
/// same kind of seam as `WHIRL_UI_RECEIPT`. A value that is missing or not a
/// number means [`DEADLINE`].
const DEADLINE_MS: &str = "WHIRL_UI_DAEMON_DEADLINE_MS";

/// How often a wait looks at its child between checks.
const POLL: Duration = Duration::from_millis(10);

/// The bound for one wait: [`DEADLINE`], or the one
/// `WHIRL_UI_DAEMON_DEADLINE_MS` names.
fn deadline() -> Duration {
    match env::var(DEADLINE_MS)
        .ok()
        .and_then(|milliseconds| milliseconds.parse::<u64>().ok())
    {
        Some(milliseconds) => Duration::from_millis(milliseconds),
        None => DEADLINE,
    }
}

/// What one bounded wait for a child produced.
enum Answer {
    /// The child exited: the code and both streams are what it left.
    Exited(ExitStatus, Vec<u8>, Vec<u8>),
    /// The child did not exit within the deadline and was ended, so it gave no
    /// answer at all. The duration is the bound that ran out.
    Expired(Duration),
    /// The child could not be started, or the wait itself failed.
    Failed(Error),
}

/// Run one command and wait at most its deadline for an answer.
///
/// This is `Command::output()` with a bound. The child's two streams are read on
/// threads of their own, so a child that fills a pipe cannot deadlock the wait,
/// and this thread polls for the exit instead of blocking on it. On expiry the
/// child is killed and reaped, so no zombie and no wedged process is left behind,
/// and the caller learns the wait ran out rather than that something refused.
fn ask(program: &Path, arguments: &[&str], deadline: Duration) -> Answer {
    let mut child = match Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return Answer::Failed(error),
    };
    let stdout = child.stdout.take().map(drained);
    let stderr = child.stderr.take().map(drained);
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout.map(joined).unwrap_or_default();
                let stderr = stderr.map(joined).unwrap_or_default();
                return Answer::Exited(status, stdout, stderr);
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    // Ended and reaped here, so nothing is left behind.
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout.map(joined);
                    let _ = stderr.map(joined);
                    return Answer::Expired(deadline);
                }
                thread::sleep(POLL);
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Answer::Failed(error);
            }
        }
    }
}

/// Read one of the child's streams to its end on a thread of its own.
///
/// The stream has to be drained while the wait runs: a child that fills a pipe
/// blocks on the write and would never exit, and the deadline would then fire on
/// a step that was working.
fn drained(pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut pipe = pipe;
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

/// What a draining thread read, or nothing when it panicked.
fn joined(handle: thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}

/// A deadline as the app's own sentences spell it: seconds from a second up,
/// milliseconds below it, so a test's short bound reads as what it is.
fn bound(waited: Duration) -> String {
    if waited.as_secs() >= 1 {
        format!("{} s", waited.as_secs())
    } else {
        format!("{} ms", waited.as_millis())
    }
}

/// The app's own sentence for a command it ended because it did not answer.
///
/// It names the command and the bound, and it says plainly that the daemon did
/// not refuse: a wait that ran out is not a refusal, and the two must never read
/// the same.
fn did_not_answer(command: &str, waited: Duration) -> String {
    format!(
        "`{command}` did not answer within {}, so the app ended it rather than wait. The daemon did not refuse: it never answered, so this step is neither done nor refused.",
        bound(waited)
    )
}

/// Ask `whirl daemon <verb>`, and answer with what it said.
///
/// **This is the one place in this app where a process is started for the
/// daemon's lifecycle.** It is `whirl daemon …` and nothing else: no `whirld`,
/// no unit file, no socket, and nothing of the daemon's is signalled. The
/// command is found by [`resolve`],
/// which follows the installer's own record rather than a bare `PATH` lookup,
/// so the daemon the installer installed is the daemon the app runs. A stand-in
/// named `whirl` on `PATH` is still enough to test the whole path headlessly,
/// because `PATH` is the search's last resort.
///
/// One more process is started, and only to identify a binary that answered the
/// wrong way: `<path> --version`, when the command did not know the `daemon`
/// verb. It is not a lifecycle step, and it is never reached on the path that
/// runs a real daemon.
///
/// Neither wait is unbounded: each is given [`DEADLINE`], and a child that does
/// not answer within it is ended. Ending this app's own child is not a control of
/// the daemon: nothing is signalled but that child, nothing is written, and no
/// second route is taken.
pub fn run(verb: Verb) -> Outcome {
    let Some(program) = resolve() else {
        return Outcome::Missing(not_found(verb));
    };
    let arguments = ["daemon", verb.word()];
    match ask(&program, &arguments, deadline()) {
        Answer::Exited(status, stdout, stderr) => {
            match classify(status.code(), &text(&stdout), &text(&stderr)) {
                // Exit 3 is the CLI's usage error, which is what a binary older
                // than the `daemon` verb answers with. The app says which binary it
                // ran, and the version when that binary can name one, rather than
                // leaving the usage text to look like the app's own mistake.
                Outcome::Usage(usage) => {
                    Outcome::Usage(not_the_daemon_command(verb, &program, &usage))
                }
                outcome => outcome,
            }
        }
        // A child the app ended got no answer out of the daemon, so the sentence
        // is the app's own and says so. It is not a refusal.
        Answer::Expired(since) => Outcome::Unanswered(did_not_answer(
            &format!("{} daemon {}", program.display(), verb.word()),
            since,
        )),
        Answer::Failed(error) => Outcome::Failed(format!(
            "`{} daemon {}` could not be run: {error}",
            program.display(),
            verb.word()
        )),
    }
}

/// The message for a command that is nowhere the app looks: every place that
/// was searched, and how to install the daemon.
fn not_found(verb: Verb) -> String {
    format!(
        "`{PROGRAM} daemon {}` could not be run: no {PROGRAM} was found. Looked at {}. Install the daemon with install.sh, which puts it in ~/.local/bin.",
        verb.word(),
        searched_places().join(", ")
    )
}

/// The message for a `whirl` that answered as if `daemon` were not a verb:
/// which binary ran, its own version when it can name one, and what the app
/// needs.
///
/// `whirl version` is not the way to identify a binary without a daemon: it is
/// `Invocation::Ask(Request::Version)` (`whirl`'s `main.rs:143`), so it asks a
/// running daemon over the socket. `--version` is the static answer a later
/// build adds; when the binary has it the version is quoted, when it does not
/// the silence is itself the evidence, because the flag is part of the build the
/// app needs, and when the flag never answers the app says that instead, because
/// a command it ended is not the same evidence as a command that said nothing.
fn not_the_daemon_command(verb: Verb, program: &Path, usage: &str) -> String {
    let identity = match version_of(program) {
        Version::Named(version) => format!("`{}` (version {version})", program.display()),
        Version::Silent => format!(
            "`{}`, which reports no version (`--version` answers nothing, so it predates that flag)",
            program.display()
        ),
        Version::Unanswered(waited) => format!(
            "`{}` (whose `--version` was ended by the app: it did not answer within {})",
            program.display(),
            bound(waited)
        ),
    };
    format!(
        "`{PROGRAM} daemon {}` ran through {identity}, which does not know the `daemon` verb. This build of the app needs a daemon that has the `daemon` verb. It answered: {usage}",
        verb.word()
    )
}

/// What a binary's `--version` answered.
enum Version {
    /// The flag answered: this is its first line.
    Named(String),
    /// The flag answered nothing: it is not this binary's command, or the binary
    /// failed or printed nothing. The silence is the evidence of a build from
    /// before the flag.
    Silent,
    /// The flag did not answer within the deadline and was ended, so the silence
    /// is the app's own bound and not the old binary's evidence.
    Unanswered(Duration),
}

/// A binary's own `--version` line, when that flag answers.
///
/// Only a successful, non-empty answer counts: a binary from before the flag
/// exists exits non-zero or says nothing, and both mean there is no version to
/// report, which the caller says in words instead. This wait is bounded like the
/// wait for a verb: a `--version` that never returns is ended rather than waited
/// on, and the caller is told which of the two silences it got.
fn version_of(program: &Path) -> Version {
    match ask(program, &["--version"], deadline()) {
        Answer::Exited(status, stdout, _) if status.success() => {
            match text(&stdout).lines().next() {
                Some(line) => Version::Named(line.to_string()),
                None => Version::Silent,
            }
        }
        Answer::Exited(..) => Version::Silent,
        Answer::Expired(waited) => Version::Unanswered(waited),
        Answer::Failed(_) => Version::Silent,
    }
}

/// The CLI's exit code as an outcome, with the words the code's stream carries.
///
/// The split is the CLI's own: a step that was done prints its line on stdout,
/// and a refusal, an unreachable daemon or a usage error prints on stderr. When
/// one stream is empty the other is used, so no message is lost to a daemon that
/// printed its line on the other one.
fn classify(code: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    match code {
        Some(0) => Outcome::Done(pick(stdout, stderr)),
        Some(1) => Outcome::Refused(pick(stderr, stdout)),
        Some(2) => Outcome::Unreachable(pick(stderr, stdout)),
        Some(3) => Outcome::Usage(pick(stderr, stdout)),
        Some(other) => Outcome::Failed(format!(
            "`{PROGRAM} daemon …` exited {other}: {}",
            pick(stderr, stdout)
        )),
        None => Outcome::Failed(format!(
            "`{PROGRAM} daemon …` was ended by a signal: {}",
            pick(stderr, stdout)
        )),
    }
}

/// A stream as text, with the trailing newline removed.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end().to_string()
}

/// The first stream that said anything, so a message printed on the other one is
/// still the message.
fn pick(first: &str, second: &str) -> String {
    if first.is_empty() {
        second.to_string()
    } else {
        first.to_string()
    }
}

/// What the person answered in the quit question.
///
/// The question is the tray's, and the tray is macOS-only, so the two other
/// legs compile this type and the plan below and never reach them. The tests
/// here are the second caller on every platform that can run them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS quit question and by the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitAnswer {
    /// The box was left unchecked: the app closes and the daemon keeps running.
    KeepRunning,
    /// The box was checked: the daemon is stopped through the verb.
    StopWhirl,
}

/// What a quit will do, decided from the remembered answer and this quit's own.
///
/// The remembered answer wins: once "don't ask again" is ticked, the question is
/// not asked and the quit behaves as remembered. A quit that has no remembered
/// answer takes the checkbox as its answer, and remembers it only when the
/// suppression box was ticked too.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS quit question and by the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitPlan {
    /// Whether to ask `whirl daemon stop` before the app closes.
    pub stop: bool,
    /// The answer to remember, when the suppression box was ticked. `None` when
    /// the preference is left as it was.
    pub remember: Option<bool>,
}

/// The plan for a quit, from what was remembered and what this quit answered.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS quit question and by the tests.
pub fn quit_plan(remembered: Option<bool>, answer: QuitAnswer, remember: bool) -> QuitPlan {
    match remembered {
        Some(stop) => QuitPlan {
            stop,
            remember: None,
        },
        None => {
            let stop = answer == QuitAnswer::StopWhirl;
            QuitPlan {
                stop,
                remember: remember.then_some(stop),
            }
        }
    }
}

/// What the quit flow does about its question, decided before anything closes.
///
/// The question is the tray's, and the tray is macOS-only, so the two other legs
/// compile this type and the call below and never reach them. The tests here are
/// the second caller on every platform that can run them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS quit question and by the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitPrompt {
    /// No answer is remembered and the view says a daemon is answering: ask.
    Ask,
    /// An answer is remembered: do not ask again; the answer decides the plan.
    Remembered,
    /// No answer is remembered and the view says no daemon is answering: do not
    /// ask, because a question about a daemon that is not there is noise, and do
    /// not stop, because there is nothing to stop. The caller reports what the
    /// view saw so a quiet quit is never read as a hidden daemon stopped.
    NoDaemon,
}

/// Whether the quit flow asks its question, from the remembered answer and
/// whether the app's view says a daemon is answering.
///
/// The view is the tray's own: the one the mark is drawn from, which the app
/// re-reads after every verb and watches the socket for. The rule is the whole
/// of what makes the question mean something: a remembered answer is the answer
/// and is not asked again, and with no daemon answering there is nothing to ask
/// about.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS quit question and by the tests.
pub fn quit_prompt(remembered: Option<bool>, daemon_answering: bool) -> QuitPrompt {
    match remembered {
        Some(_) => QuitPrompt::Remembered,
        None if daemon_answering => QuitPrompt::Ask,
        None => QuitPrompt::NoDaemon,
    }
}

/// What the app did about the daemon when it started.
///
/// The first run is the only one that starts anything on its own. After that a
/// daemon that is absent is offered, not started: an app that restarted a daemon
/// somebody stopped on purpose would be overriding them.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launched {
    /// The supervisor already has the daemon running: nothing was asked of it
    /// and nothing was started. The words are `whirl daemon status`'s.
    Present(String),
    /// The app's first run, and the daemon was absent: this is what the start
    /// step said.
    Started(Outcome),
    /// Not the first run, and the daemon is absent: the control is offered and
    /// nothing is started. The words are `whirl daemon status`'s.
    Absent(String),
}

/// The launch offer: ask the supervisor, and start the daemon only on the first
/// run and only when it is absent.
///
/// `whirl daemon status` is the question, and it is a question the CLI is meant
/// to answer: exit 0 means the supervisor has the job running, and the daemon is
/// left alone. Anything else means absent, and the first run hands it to
/// `whirl daemon start`. The command is never asked to do more than that, and no
/// second route is substituted when it refuses.
#[cfg(target_os = "macos")]
pub fn launch() -> Launched {
    let status = run(Verb::Status);
    if status.done() {
        return Launched::Present(status.words().to_string());
    }
    if crate::prefs::first_run_done() {
        return Launched::Absent(status.words().to_string());
    }
    crate::prefs::mark_first_run_done();
    Launched::Started(run(Verb::Start))
}

/// Carry out a quit plan: remember the answer when asked, and stop the daemon
/// when the plan says so.
///
/// The stop's outcome is returned so the caller can quote it; `None` means the
/// plan kept the daemon running and the verb was never called.
#[cfg(target_os = "macos")]
pub fn finish_quit(plan: QuitPlan) -> Option<Outcome> {
    if let Some(remembered) = plan.remember {
        crate::prefs::remember_quit_answer(remembered);
    }
    plan.stop.then(|| run(Verb::Stop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_receipt_binary_line_is_the_one_whose_basename_is_the_command() {
        // The receipt has one line per installed artefact. Only the `binary`
        // line for `whirl` is the command; `whirld`, `whirl-worker`, the
        // comment and the `dir`/`app`/`unit` lines are not.
        let receipt = "\
# whirl-ui install receipt. Written by install.sh, read by uninstall.sh.
dir /home/someone/.local/bin
binary /home/someone/.local/bin/whirld
binary /home/someone/.local/bin/whirl
binary /home/someone/.local/bin/whirl-worker
app /Applications/Whirl.app
unit delegated
";
        assert_eq!(
            receipt_binary(receipt),
            Some(std::path::PathBuf::from("/home/someone/.local/bin/whirl"))
        );
        // Absent, empty and binary-less receipts name nothing.
        assert_eq!(receipt_binary(""), None);
        assert_eq!(receipt_binary("dir /x\napp /y\n"), None);
    }

    #[test]
    fn the_three_verbs_round_trip_through_their_words() {
        for verb in Verb::ALL {
            assert_eq!(Verb::parse(verb.word()), Some(verb), "{}", verb.word());
        }
        for word in ["", "install", "uninstall", "restart", "Start", "start "] {
            assert_eq!(Verb::parse(word), None, "{word:?}");
        }
    }

    #[test]
    fn every_exit_code_becomes_the_outcome_its_meaning_names() {
        assert_eq!(
            classify(Some(0), "started: job", ""),
            Outcome::Done("started: job".to_string())
        );
        assert_eq!(
            classify(Some(1), "", "whirl: refused"),
            Outcome::Refused("whirl: refused".to_string())
        );
        assert_eq!(
            classify(Some(2), "", "whirl: no daemon"),
            Outcome::Unreachable("whirl: no daemon".to_string())
        );
        assert_eq!(
            classify(Some(3), "", "whirl: usage"),
            Outcome::Usage("whirl: usage".to_string())
        );
    }

    #[test]
    fn the_words_are_the_commands_own_and_are_never_replaced() {
        // What a daemon says is shown as it said it: the line keeps its own
        // spelling, including the `whirl:` prefix on a refusal.
        let outcome = classify(
            Some(1),
            "",
            "whirl: no unit at /x; run `whirl daemon install` first",
        );
        assert_eq!(
            outcome.words(),
            "whirl: no unit at /x; run `whirl daemon install` first"
        );
        assert_eq!(outcome.exit_code(), 1);
        assert!(!outcome.done());
    }

    #[test]
    fn a_streams_trailing_newline_is_not_part_of_the_words() {
        // The daemon prints a line, so a newline is the stream's and not the
        // message's: the words are what is drawn, and a trailing blank in a menu
        // item would be a line nobody wrote.
        assert_eq!(
            text(b"started: com.guruor.whirl\n"),
            "started: com.guruor.whirl"
        );
        assert_eq!(text(b"no newline at all"), "no newline at all");
        assert_eq!(text(b""), "");
    }

    #[test]
    fn an_outcome_with_no_exit_code_is_a_failure_rather_than_a_guess() {
        let killed = classify(None, "", "whirl: killed");
        assert!(matches!(killed, Outcome::Failed(_)), "{killed:?}");
        assert_eq!(killed.exit_code(), 3);
    }

    #[test]
    fn a_remembered_answer_wins_over_the_dialog() {
        // Ticked once, the question is not asked again and the quit behaves as
        // remembered, whichever box this quit would have shown.
        assert_eq!(
            quit_plan(Some(true), QuitAnswer::KeepRunning, false),
            QuitPlan {
                stop: true,
                remember: None
            }
        );
        assert_eq!(
            quit_plan(Some(false), QuitAnswer::StopWhirl, true),
            QuitPlan {
                stop: false,
                remember: None
            }
        );
    }

    #[test]
    fn an_unticked_remember_box_leaves_the_preference_alone() {
        assert_eq!(
            quit_plan(None, QuitAnswer::StopWhirl, false),
            QuitPlan {
                stop: true,
                remember: None
            }
        );
        assert_eq!(
            quit_plan(None, QuitAnswer::KeepRunning, true),
            QuitPlan {
                stop: false,
                remember: Some(false)
            }
        );
        assert_eq!(
            quit_plan(None, QuitAnswer::StopWhirl, true),
            QuitPlan {
                stop: true,
                remember: Some(true)
            }
        );
    }

    #[test]
    fn the_question_is_asked_only_with_a_daemon_and_no_remembered_answer() {
        // The gate the whole card turns on. With no daemon answering the view,
        // the question is not asked and nothing is stopped, whatever this quit
        // would have answered; a remembered answer is the answer and is not
        // asked again either.
        assert_eq!(
            quit_prompt(None, true),
            QuitPrompt::Ask,
            "no answer remembered and a daemon answering: ask"
        );
        assert_eq!(
            quit_prompt(None, false),
            QuitPrompt::NoDaemon,
            "no daemon answering: nothing to ask about"
        );
        assert_eq!(
            quit_prompt(Some(true), true),
            QuitPrompt::Remembered,
            "a remembered stop is honoured, not asked"
        );
        assert_eq!(
            quit_prompt(Some(false), true),
            QuitPrompt::Remembered,
            "a remembered keep is honoured, not asked"
        );
        assert_eq!(
            quit_prompt(Some(true), false),
            QuitPrompt::Remembered,
            "a remembered answer is honoured even with no daemon"
        );
        assert_eq!(
            quit_prompt(Some(false), false),
            QuitPrompt::Remembered,
            "and so is a remembered keep"
        );
    }
}
