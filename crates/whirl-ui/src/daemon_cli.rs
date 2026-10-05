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
//! Nothing here writes a unit file, unlinks a socket, or kills a process, and
//! nothing here knows a socket path: the daemon's lifecycle is the daemon's.

use std::io::ErrorKind;
use std::process::Command;

/// The command the daemon's lifecycle is asked through, found on `PATH`.
///
/// It is a name rather than a path on purpose: the daemon is installed beside
/// its own command, and a frontend that guessed at an install location would be
/// describing a layout it does not own.
pub const PROGRAM: &str = "whirl";

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
            | Outcome::Failed(words) => words,
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
            // The command line cannot work: the command is not there, it could
            // not be run, or it does not know the verb.
            Outcome::Usage(_) | Outcome::Missing(_) | Outcome::Failed(_) => 3,
        }
    }
}

/// Ask `whirl daemon <verb>`, and answer with what it said.
///
/// **This is the one place in this app where a process is started for the
/// daemon's lifecycle.** It is `whirl daemon …` and nothing else: no `whirld`,
/// no unit file, no socket, no `kill`. The command is looked up on `PATH`, which
/// is what makes the app's half testable headlessly with a stand-in on `PATH`.
pub fn run(verb: Verb) -> Outcome {
    let arguments = ["daemon", verb.word()];
    match Command::new(PROGRAM).args(arguments).output() {
        Ok(output) => classify(
            output.status.code(),
            &text(&output.stdout),
            &text(&output.stderr),
        ),
        // The command is not installed, or not on this app's PATH. There is
        // nothing of the daemon's to print, so the app says exactly that rather
        // than reaching for another route.
        Err(error) if error.kind() == ErrorKind::NotFound => Outcome::Missing(format!(
            "`{PROGRAM} daemon {}` could not be run: {PROGRAM} is not on PATH",
            verb.word()
        )),
        Err(error) => Outcome::Failed(format!(
            "`{PROGRAM} daemon {}` could not be run: {error}",
            verb.word()
        )),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitPlan {
    /// Whether to ask `whirl daemon stop` before the app closes.
    pub stop: bool,
    /// The answer to remember, when the suppression box was ticked. `None` when
    /// the preference is left as it was.
    pub remember: Option<bool>,
}

/// The plan for a quit, from what was remembered and what this quit answered.
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
}
