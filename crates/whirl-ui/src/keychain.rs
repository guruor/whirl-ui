//! The Wallhaven token in the platform's own store, and the two calls that
//! never ask for it back.
//!
//! whirl's `docs/architecture.md` 6.3 puts the key in one of three places, and
//! this module owns exactly one of them on macOS: the Keychain item the
//! documented label names, `keychain:whirl-wallhaven`. A config file holds the
//! label and never the value, because a key in a config file is a key in every
//! backup of it.
//!
//! Three rules shape the API, and each one is visible in the signatures:
//!
//! - **There is one write.** [`store`] is the only call that carries the token,
//!   and it carries it on the child's standard input, never in an argument:
//!   `security add-generic-password` accepts its password from argv, and argv is
//!   readable by every process of the same user through `ps`. The token is
//!   hex-encoded and fed to `security -i`'s command line on stdin, which is the
//!   one form that keeps it out of the process table.
//! - **A read asks whether the item is there, never for its bytes.** [`exists`]
//!   runs `security find-generic-password -s whirl-wallhaven` with its output
//!   discarded and reads only the exit status. `-w` (the password alone) and `-g`
//!   (the password with the attributes) are the two flags that return the secret
//!   (`security(1)`), and no code path here passes either one, so a call that
//!   returns the value cannot be written against this API by accident.
//! - **The item's identity is the daemon's.** The service, the account and the
//!   label are whirl's own words ([`SERVICE`], [`ACCOUNT`], [`LABEL`]), so the
//!   item this app writes is the item `whirl config check` looks for. Nothing
//!   here invents a second name for the same secret.
//!
//! Every call that runs the tool goes through one place, `Proc::run`, and every
//! call consults [`STORE_ENV`] first: a run that names no store reaches nothing
//! by any path, and a run that reaches `Proc::run` at all is one that asked a
//! store. That makes this the one seam a check can instrument: make `Proc::run`
//! panic and a run that reaches a store says so rather than answering quietly.
//! What keeps a test out is naming no store at all, and that half of the rule is
//! the harness's: the suites that spawn the app binary, whose own build asks the
//! platform's store, name `none` in the one helper each of them spawns through.
//!
//! A zero from that check is a fact about the run it was taken in, and about no
//! wider run. Two tests here need a daemon and return early while none answers,
//! so a run with nothing behind `WHIRL_SOCKET` says nothing about the paths they
//! drive, and the count is worth quoting only from a run in which they ran.
//!
//! The platform is macOS-only today, and the other two CI legs still compile
//! this module: the `not(macos)` half answers every call with
//! [`KeychainError::Unsupported`] rather than failing to build, so a
//! cross-platform build of the workspace stays honest about what it can do.

use std::env;
use std::fmt;

/// The service (and, by macOS's default, the label) of the item the documented
/// label names: `security find-generic-password -s whirl-wallhaven`
/// (whirl's `docs/architecture.md` 6.3).
pub const SERVICE: &str = "whirl-wallhaven";

/// The account the item is created under. whirl looks the item up by service
/// alone, so this only has to stay stable between the write and the update.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub const ACCOUNT: &str = "whirl";

/// The label a config file carries in `sources[i].api_key_ref`, and the one
/// label this app manages. It is a name, never a value, and it is what
/// `looks_like_a_key` in whirl-core's parser accepts where a key would be
/// refused.
pub const LABEL: &str = "keychain:whirl-wallhaven";

/// The environment variable that names the store this run asks.
///
/// A run that must not reach a person's own keychain names `none` here, and then
/// no question is asked of it at all: every one is refused with
/// [`NO_STORE_REASON`] and no process is started. The app itself never sets it;
/// a run that has to be kept out of the machine's keychain -- a test above all --
/// is what needs it, and this is the one gate every call below consults.
pub const STORE_ENV: &str = "WHIRL_UI_KEYCHAIN";

/// The word that names the platform's own store, which an unset variable means.
const SYSTEM_STORE: &str = "system";

/// Why a question asked by a run that named no store is not an answer from one.
pub const NO_STORE_REASON: &str = "this run asks no store, so the item was not looked up";

/// Whether a run that named this word asks no store at all.
///
/// Unset, empty and `system` are the platform's own store; `none` is no store;
/// and every other word counts as no store too, because a run that meant to name
/// none and misspelled it must not reach the real one by accident. It is a
/// function of the word rather than of the environment, so the rule can be held
/// without a process-wide variable being set.
fn asks_no_store(named: Option<&str>) -> bool {
    match named {
        None | Some("") => false,
        Some(word) => word != SYSTEM_STORE,
    }
}

/// The refusal a run that named no store gets, or `None` when it asks the
/// platform's own.
fn refused_store() -> Option<KeychainError> {
    let named = env::var_os(STORE_ENV);
    asks_no_store(named.as_deref().and_then(|word| word.to_str()))
        .then(|| KeychainError::Failed(NO_STORE_REASON.to_string()))
}

/// The platform tool, by absolute path: the same binary `security(1)`
/// documents, so nothing here depends on the caller's `PATH`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS run and by the rules below.
const SECURITY: &str = "/usr/bin/security";

/// `errSecItemNotFound`, the status `security` exits with when the item is
/// not there. It is the one non-zero status that is an answer rather than a
/// failure.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by `presence` below.
const ITEM_NOT_FOUND: i32 = 44;

/// The platform store could not be asked, or refused the write. The message is
/// the app's own; it never contains the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainError {
    /// This platform has no store wired yet (the two non-macOS CI legs).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    // Constructed only by the non-macOS half.
    Unsupported(String),
    /// The store answered, and the answer was a failure, or this run named no
    /// store to ask at all.
    Failed(String),
}

impl KeychainError {
    /// The reason, in the words a pane shows.
    pub fn message(&self) -> &str {
        match self {
            KeychainError::Unsupported(message) | KeychainError::Failed(message) => message,
        }
    }
}

impl fmt::Display for KeychainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for KeychainError {}

/// One run of the platform tool, as this module reads it: the exit status, and
/// the bytes it printed when they were asked for.
///
/// It is a value rather than a live child, so that the four shapes a store can
/// answer with -- present, absent, refused, unreadable -- can be put in front of
/// the rules below without running `security` at all.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Built by the macOS run and by the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The exit status, or `None` when the child was killed by a signal.
    pub code: Option<i32>,
    /// What the child printed on standard output, empty when it was discarded.
    pub stdout: Vec<u8>,
}

/// The platform store, as this module asks it questions: one run of the tool.
///
/// A trait, so that [`store`], [`exists`] and [`metadata`] can be driven from a
/// double and never from the user's own keychain: no test in this file runs
/// `security`, so no test reads the item, writes it, or asks for access to it.
///
/// `keep_output` is the other half of that, and the reason it is a parameter
/// rather than a rule inside the caller: the presence query discards everything
/// the tool printed, so "is it there" cannot become "what is it" by accident.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Driven by the macOS calls and by the tests.
pub trait Security {
    /// Run the tool with `args`, feeding `input` on its standard input.
    ///
    /// `Err(reason)` is a run that did not happen -- the tool could not be
    /// started, its input could not be written, or it did not finish -- which is
    /// not an answer from the store and is kept apart from one.
    fn run(&self, args: &[&str], input: Option<&[u8]>, keep_output: bool)
    -> Result<Answer, String>;
}

/// Store the token, once. The one call in this crate that carries the secret.
pub fn store(token: &str) -> Result<(), KeychainError> {
    match refused_store() {
        Some(refusal) => Err(refusal),
        None => imp::store(token),
    }
}

/// Whether an item exists for [`SERVICE`]. A `bool`, never the value: this is
/// the call the panes make, and the only question the app may ask after the
/// write.
#[cfg_attr(test, allow(dead_code))] // A test build's windows ask the test store, so nothing in one asks this.
pub fn exists() -> Result<bool, KeychainError> {
    match refused_store() {
        Some(refusal) => Err(refusal),
        None => imp::exists(),
    }
}

/// The item's attributes, for evidence a human reads. It runs the same query
/// [`exists`] runs and keeps the attribute lines; a line that carries password
/// data is dropped before it leaves this function, so even a future macOS that
/// printed one there could not make this a read.
pub fn metadata() -> Result<Vec<String>, KeychainError> {
    match refused_store() {
        Some(refusal) => Err(refusal),
        None => imp::metadata(),
    }
}

/// Whether the store holds the item, from one query's answer.
///
/// The rule a `find-generic-password` answer means, in one place: exit `0` is
/// "it is there", [`ITEM_NOT_FOUND`] is "it is not", any other code is a refusal
/// that names the code, and a signal is a refusal with no code at all. An item
/// that is not there is an answer, and the two refusals stay apart from it:
/// "the store would not say" turning into "there is no item" is how a pane comes
/// to tell a person that a key they saved has gone.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS read and by the tests.
pub fn presence(tool: &dyn Security) -> Result<bool, KeychainError> {
    let answer = tool
        .run(&find_arguments(), None, false)
        .map_err(KeychainError::Failed)?;
    match answer.code {
        Some(0) => Ok(true),
        Some(ITEM_NOT_FOUND) => Ok(false),
        Some(code) => Err(KeychainError::Failed(format!(
            "{SECURITY} find-generic-password -s {SERVICE} exited {code}"
        ))),
        None => Err(KeychainError::Failed(format!(
            "{SECURITY} find-generic-password -s {SERVICE} was killed by a signal"
        ))),
    }
}

/// The item's attribute lines, from one query's answer.
///
/// The one call that keeps what the tool printed; [`attribute_lines`] is what
/// keeps a password line out of the result even so.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS read and by the tests.
pub fn attributes(tool: &dyn Security) -> Result<Vec<String>, KeychainError> {
    let answer = tool
        .run(&find_arguments(), None, true)
        .map_err(KeychainError::Failed)?;
    match answer.code {
        Some(0) => Ok(attribute_lines(&String::from_utf8_lossy(&answer.stdout))),
        Some(ITEM_NOT_FOUND) => Ok(Vec::new()),
        Some(code) => Err(KeychainError::Failed(format!(
            "{SECURITY} find-generic-password -s {SERVICE} exited {code}"
        ))),
        None => Err(KeychainError::Failed(format!(
            "{SECURITY} find-generic-password -s {SERVICE} was killed by a signal"
        ))),
    }
}

/// Write the item, and confirm it is there afterwards.
///
/// The token goes in on the child's standard input and nowhere else: not as an
/// argument, not in the environment, and not in a temporary file. `security -i`
/// reads commands until end of input, and its status describes the session
/// rather than the one line, so the item is confirmed by the query that asks
/// about presence. A store that did not take the write is a failure here rather
/// than a pane that says a key is saved when nothing landed.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub fn write(tool: &dyn Security, token: &str) -> Result<(), KeychainError> {
    let line = add_line(&hex(token.as_bytes()));
    let answer = tool
        .run(&["-i"], Some(line.as_bytes()), false)
        .map_err(KeychainError::Failed)?;
    if answer.code != Some(0) {
        return Err(KeychainError::Failed(format!(
            "{SECURITY} add-generic-password -U -a {ACCOUNT} -s {SERVICE} exited {}",
            answer
                .code
                .map_or("on a signal".to_string(), |code| code.to_string())
        )));
    }
    if presence(tool)? {
        Ok(())
    } else {
        Err(KeychainError::Failed(format!(
            "the store answered the write but holds no item for service {SERVICE}"
        )))
    }
}

/// The command line `security -i` is fed on stdin to write the item.
///
/// `-U` updates an item that is already there, so re-entering a token replaces
/// it rather than failing. `-X` takes the password as a hexadecimal string,
/// which is what makes a token containing quotes, spaces or a backslash safe to
/// put on a line `security` splits into words. The hex is the token's bytes and
/// nothing here encodes it as a key: the value must be recoverable by the
/// daemon, so it cannot be anything but the bytes themselves.
///
/// It is a function of the hex rather than of the token so that a test can hold
/// the line without ever holding a token.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub fn add_line(hex_token: &str) -> String {
    format!("add-generic-password -U -a {ACCOUNT} -s {SERVICE} -X {hex_token}\n")
}

/// The arguments of the query, which is also the call a developer runs by hand:
/// `security find-generic-password -s whirl-wallhaven`.
///
/// Neither `-w` nor `-g` is here, and neither may be added: they are the two
/// flags that make `security` print the password, so a call built from these
/// arguments cannot return it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS calls and by the tests.
pub fn find_arguments() -> [&'static str; 3] {
    ["find-generic-password", "-s", SERVICE]
}

/// The token's bytes as lower-case hex, which is the form `-X` takes.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS write and by the tests.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The attribute lines of a `security find-generic-password` answer.
///
/// The plain query prints the item's attributes and no password, because `-g`
/// and `-w` are the flags that ask for one. The filter is the belt to that
/// brace: any line that begins with `password` is dropped here, so the value
/// cannot leave this module even if a future `security` began printing it in the
/// attribute dump.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Used by the macOS read and by the tests.
pub fn attribute_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            !line
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("password")
        })
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use super::{Answer, KeychainError, SECURITY, Security, attributes, presence, write};

    /// The store this build ships: one run of `/usr/bin/security`.
    ///
    /// The whole of the tool's use is here, which is what makes the rest of the
    /// module a set of rules about answers rather than a set of calls: a test
    /// replaces this with a double and every rule below it is exercised without
    /// a store being touched.
    pub(super) struct Proc;

    impl Security for Proc {
        fn run(
            &self,
            args: &[&str],
            input: Option<&[u8]>,
            keep_output: bool,
        ) -> Result<Answer, String> {
            let mut child = Command::new(SECURITY)
                .args(args)
                .stdin(if input.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(if keep_output {
                    Stdio::piped()
                } else {
                    // The presence query's output goes to /dev/null: the app
                    // reads the exit status and not one byte of the item, which
                    // is the whole of "whether it is there, never what it is".
                    Stdio::null()
                })
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("cannot run {SECURITY}: {error}"))?;
            // The token goes in here and nowhere else: not an argument, not an
            // environment variable, not a temporary file.
            if let Some(input) = input
                && let Some(mut stdin) = child.stdin.take()
            {
                stdin
                    .write_all(input)
                    .map_err(|error| format!("cannot write to {SECURITY}: {error}"))?;
            }
            let answer = child
                .wait_with_output()
                .map_err(|error| format!("{SECURITY} did not finish: {error}"))?;
            Ok(Answer {
                code: answer.status.code(),
                stdout: answer.stdout,
            })
        }
    }

    pub(super) fn store(token: &str) -> Result<(), KeychainError> {
        write(&Proc, token)
    }

    pub(super) fn exists() -> Result<bool, KeychainError> {
        presence(&Proc)
    }

    pub(super) fn metadata() -> Result<Vec<String>, KeychainError> {
        attributes(&Proc)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{KeychainError, SERVICE};

    fn unsupported() -> KeychainError {
        KeychainError::Unsupported(format!(
            "the platform store is not wired on this platform yet; the macOS item is {SERVICE}"
        ))
    }

    pub fn store(_token: &str) -> Result<(), KeychainError> {
        Err(unsupported())
    }

    pub fn exists() -> Result<bool, KeychainError> {
        Err(unsupported())
    }

    pub fn metadata() -> Result<Vec<String>, KeychainError> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn the_write_line_carries_the_token_only_as_hex_on_stdin() {
        // The fake token the tests are allowed to hold.
        let token = "fake-token-not-a-real-key";
        let line = add_line(&hex(token.as_bytes()));
        assert!(line.starts_with("add-generic-password -U "), "{line}");
        assert!(line.contains(&format!("-a {ACCOUNT}")), "{line}");
        assert!(line.contains(&format!("-s {SERVICE}")), "{line}");
        assert!(line.ends_with('\n'), "{line:?}");
        // The token itself is not in the line, and the line is one stdin line:
        // `security -i` reads one command per line.
        assert!(!line.contains(token), "{line}");
        assert_eq!(line.lines().count(), 1, "{line:?}");
        // The hex is the token's bytes, one way only.
        assert_eq!(hex(b"AB"), "4142");
        assert_eq!(hex(b""), "");
    }

    #[test]
    fn the_query_asks_for_neither_the_password_nor_the_attributes_with_it() {
        let arguments = find_arguments();
        assert_eq!(arguments[0], "find-generic-password");
        assert_eq!(arguments[1], "-s");
        assert_eq!(arguments[2], SERVICE);
        // `-w` prints the password alone and `-g` prints it beside the
        // attributes; a call that passes neither cannot print it.
        for forbidden in ["-w", "-g", "-X", "-a"] {
            assert!(
                !arguments.contains(&forbidden),
                "{forbidden} is a value-carrying flag: {arguments:?}"
            );
        }
    }

    #[test]
    fn an_attribute_dump_loses_any_line_that_carries_a_password() {
        let output = "keychain: \"/Users/example/Library/Keychains/login.keychain-db\"\n\
                      version: 512\n\
                      class: \"genp\"\n\
                      attributes:\n\
                      \x20   0x00000007 <blob>=\"whirl-wallhaven\"\n\
                      \x20   \"svce\"<blob>=\"whirl-wallhaven\"\n\
                      password: \"a-fake-token\"\n";
        let lines = attribute_lines(output);
        assert!(lines.iter().any(|line| line.contains(SERVICE)), "{lines:?}");
        assert!(
            !lines.iter().any(|line| line.contains("fake-token")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with("password")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_label_is_a_name_the_parser_accepts_where_a_key_would_be_refused() {
        // The label the module exports is the one the config carries, and it is
        // short, colon-prefixed and not key-shaped: whirl-core's
        // `looks_like_a_key` refuses a 32-character run of key characters.
        assert_eq!(LABEL, "keychain:whirl-wallhaven");
        assert!(LABEL.contains(':'));
        assert!(LABEL.len() < 32);
    }

    #[test]
    fn a_run_that_names_no_store_is_the_one_that_reaches_nothing() {
        // Unset, empty and `system` are the platform's own store.
        assert!(!asks_no_store(None));
        assert!(!asks_no_store(Some("")));
        assert!(!asks_no_store(Some("system")));
        // `none` is no store, and a word that meant to be `none` and missed is
        // no store too, so a misspelling cannot reach the real one by accident.
        assert!(asks_no_store(Some("none")));
        assert!(asks_no_store(Some("None")));
        assert!(asks_no_store(Some("systen")));
    }

    // The four shapes a store can answer with, and the rules that read them.
    //
    // Every test below drives the rules through a double and never through
    // `security`: no test in this file reads the item, writes it, or asks for
    // access to it, and none of them can prompt for an authorization dialog.
    // The double also records the questions, which is how the two absences are
    // asserted directly rather than inferred: a presence query keeps no output,
    // and a token goes in on standard input and never in an argument.

    /// One question a doubled store was asked.
    #[derive(Debug, Clone)]
    struct Asked {
        /// The arguments the tool was run with.
        args: Vec<String>,
        /// What was fed on the tool's standard input, if anything.
        input: Option<Vec<u8>>,
        /// Whether the caller asked to keep the bytes the tool printed.
        keep_output: bool,
    }

    /// A store that answers from a script, and remembers what it was asked.
    struct Doubled {
        answers: RefCell<VecDeque<Result<Answer, String>>>,
        asked: RefCell<Vec<Asked>>,
    }

    impl Doubled {
        /// A store that answers these, in order, and then has nothing left.
        fn answering(answers: Vec<Result<Answer, String>>) -> Doubled {
            Doubled {
                answers: RefCell::new(answers.into()),
                asked: RefCell::new(Vec::new()),
            }
        }

        /// The questions it was asked, in order.
        fn asked(&self) -> Vec<Asked> {
            self.asked.borrow().clone()
        }
    }

    impl Security for Doubled {
        fn run(
            &self,
            args: &[&str],
            input: Option<&[u8]>,
            keep_output: bool,
        ) -> Result<Answer, String> {
            self.asked.borrow_mut().push(Asked {
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                input: input.map(<[u8]>::to_vec),
                keep_output,
            });
            self.answers
                .borrow_mut()
                .pop_front()
                .expect("every run a test makes has an answer scripted for it")
        }
    }

    /// A store that answered, with this exit status.
    fn exited(code: i32, stdout: &str) -> Result<Answer, String> {
        Ok(Answer {
            code: Some(code),
            stdout: stdout.as_bytes().to_vec(),
        })
    }

    /// A store whose tool was killed before it could answer.
    fn signalled() -> Result<Answer, String> {
        Ok(Answer {
            code: None,
            stdout: Vec::new(),
        })
    }

    /// A store that could not be asked at all.
    fn could_not_run(reason: &str) -> Result<Answer, String> {
        Err(reason.to_string())
    }

    /// The arguments the double should have been asked with.
    fn query() -> Vec<String> {
        find_arguments()
            .iter()
            .map(|argument| (*argument).to_string())
            .collect()
    }

    #[test]
    fn a_store_that_holds_the_item_answers_present_and_is_never_asked_for_the_bytes() {
        // The dump carries a password line on purpose: the presence rule keeps
        // no output at all, so what the tool printed cannot reach it, and the
        // answer is still "it is there".
        let store = Doubled::answering(vec![exited(
            0,
            "svce<blob>=\"whirl-wallhaven\"\npassword: \"fake\"\n",
        )]);
        assert_eq!(presence(&store), Ok(true));
        let asked = store.asked();
        assert_eq!(asked.len(), 1, "one question, one answer");
        assert_eq!(asked[0].args, query());
        assert_eq!(
            asked[0].input, None,
            "a presence query feeds the tool nothing"
        );
        assert!(
            !asked[0].keep_output,
            "and never keeps what the tool printed"
        );
    }

    #[test]
    fn a_store_with_no_item_answers_absent_rather_than_refusing() {
        let store = Doubled::answering(vec![exited(ITEM_NOT_FOUND, "")]);
        assert_eq!(
            presence(&store),
            Ok(false),
            "exit {ITEM_NOT_FOUND} is an answer, not a failure"
        );
    }

    #[test]
    fn a_store_that_refuses_names_the_code_it_refused_with() {
        let store = Doubled::answering(vec![exited(51, "")]);
        let refused = presence(&store).expect_err("exit 51 is not an answer");
        assert_eq!(
            refused.message(),
            format!("{SECURITY} find-generic-password -s {SERVICE} exited 51")
        );
    }

    #[test]
    fn the_sentence_is_the_same_one_when_it_is_printed_or_asked_for() {
        // The window prints the error with `Display` and everything else asks it
        // with `message`: a person must never read two different sentences for
        // one failure. `Unsupported` is the arm this platform cannot reach (it is
        // the not-macOS stub that raises it), so it is the one worth pinning.
        let unsupported = KeychainError::Unsupported("the store is not wired here".to_string());
        assert_eq!(unsupported.message(), "the store is not wired here");
        assert_eq!(unsupported.to_string(), unsupported.message());
        let failed = presence(&Doubled::answering(vec![exited(51, "")]))
            .expect_err("exit 51 is not an answer");
        assert_eq!(failed.to_string(), failed.message());
    }

    #[test]
    fn a_store_that_could_not_be_asked_is_not_an_absent_item() {
        // The one confusion this module may not make: a question that did not
        // happen is not "there is no item". Reading it as one is how a pane
        // comes to tell a person the key they saved has gone.
        let reason = "cannot run /usr/bin/security: No such file or directory";
        let store = Doubled::answering(vec![could_not_run(reason)]);
        let refused = presence(&store).expect_err("not an answer");
        assert_eq!(refused.message(), reason);
    }

    #[test]
    fn a_query_killed_by_a_signal_is_not_an_absent_item_either() {
        let store = Doubled::answering(vec![signalled()]);
        let refused = presence(&store).expect_err("a signal");
        assert!(
            refused.message().contains("killed by a signal"),
            "{}",
            refused.message()
        );
    }

    #[test]
    fn the_attribute_dump_is_the_one_question_that_keeps_what_the_tool_printed() {
        let store = Doubled::answering(vec![exited(
            0,
            "svce<blob>=\"whirl-wallhaven\"\npassword: \"fake-token-not-a-real-key\"\n",
        )]);
        let lines = attributes(&store).expect("the attributes");
        assert!(lines.iter().any(|line| line.contains(SERVICE)), "{lines:?}");
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("fake-token-not-a-real-key")),
            "{lines:?}"
        );
        assert!(
            store.asked()[0].keep_output,
            "the dump is asked for, and kept"
        );
    }

    #[test]
    fn an_absent_item_has_no_attributes_and_a_query_that_did_not_answer_has_none_either() {
        assert_eq!(
            attributes(&Doubled::answering(vec![exited(ITEM_NOT_FOUND, "")])),
            Ok(Vec::new())
        );
        let refused = attributes(&Doubled::answering(vec![exited(7, "")]))
            .expect_err("exit 7 is not an answer");
        assert!(
            refused.message().contains("exited 7"),
            "{}",
            refused.message()
        );
        assert!(
            attributes(&Doubled::answering(vec![signalled()])).is_err(),
            "a signal is not an empty set of attributes"
        );
        assert!(
            attributes(&Doubled::answering(vec![could_not_run("no store")])).is_err(),
            "a store that could not be asked has not answered"
        );
    }

    #[test]
    fn a_write_feeds_the_hex_line_on_stdin_and_then_asks_whether_the_item_landed() {
        let store = Doubled::answering(vec![exited(0, ""), exited(0, "")]);
        assert_eq!(write(&store, "fake-token-not-a-real-key"), Ok(()));
        let asked = store.asked();
        assert_eq!(asked.len(), 2, "the write, and the query that confirms it");
        assert_eq!(asked[0].args, vec!["-i".to_string()]);
        assert_eq!(
            asked[0].input,
            Some(add_line(&hex(b"fake-token-not-a-real-key")).into_bytes()),
            "the line goes in on standard input"
        );
        assert!(!asked[0].keep_output, "the write keeps no output");
        assert_eq!(
            asked[1].args,
            query(),
            "the confirmation is the presence query"
        );
    }

    #[test]
    fn a_write_the_store_refused_is_a_failure_that_names_the_step() {
        let store = Doubled::answering(vec![exited(1, "")]);
        let refused = write(&store, "fake-token-not-a-real-key").expect_err("a refused write");
        assert_eq!(
            refused.message(),
            format!("{SECURITY} add-generic-password -U -a {ACCOUNT} -s {SERVICE} exited 1")
        );
        assert_eq!(
            store.asked().len(),
            1,
            "a write that failed is not confirmed"
        );
    }

    #[test]
    fn a_write_the_store_took_but_that_left_no_item_is_still_a_failure() {
        // Exit 0 from `security -i` describes the session, not the one line, so
        // the write is confirmed by the query afterwards. A store that took the
        // write and holds nothing must not read as a saved key.
        let store = Doubled::answering(vec![exited(0, ""), exited(ITEM_NOT_FOUND, "")]);
        let refused = write(&store, "fake-token-not-a-real-key").expect_err("nothing landed");
        assert_eq!(
            refused.message(),
            format!("the store answered the write but holds no item for service {SERVICE}")
        );
    }

    #[test]
    fn a_write_that_could_not_be_run_is_the_stores_own_reason() {
        let store = Doubled::answering(vec![could_not_run(
            "cannot write to /usr/bin/security: broken pipe",
        )]);
        let refused =
            write(&store, "fake-token-not-a-real-key").expect_err("the write did not happen");
        assert_eq!(
            refused.message(),
            "cannot write to /usr/bin/security: broken pipe"
        );
    }

    #[test]
    fn a_write_killed_by_a_signal_says_so_rather_than_naming_a_code() {
        let store = Doubled::answering(vec![signalled()]);
        let refused =
            write(&store, "fake-token-not-a-real-key").expect_err("a signal is not a success");
        assert!(
            refused.message().ends_with("exited on a signal"),
            "{}",
            refused.message()
        );
    }
}
