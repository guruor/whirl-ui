//! The stand-in `whirl` the app is tested against, shared by the test binaries
//! that drive it.
//!
//! `whirl daemon <verb>` is the one command this app runs, so the stand-in is
//! the seam every test of that seam goes through: it answers each verb with the
//! exit code and the words the test gave it, records every call in a log, and
//! can be given a home directory of its own. `daemon.rs` drives the lifecycle
//! modes with it and `settings.rs` drives the window's switch with it, which is
//! why it lives here rather than in either file.
//!
//! The module is unix-only, like the app's own command handling: the stand-in is
//! a shell script.
#![cfg(unix)]
// One test binary uses less of the harness than the other, so an item that is
// dead in `settings.rs` is not dead in `daemon.rs`.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

/// The stand-in: records `daemon <verb>`, answers with words and an exit code.
///
/// The answer per verb is a default that a test overrides with the environment,
/// so one script can be a daemon that is absent, one that refuses, or one that
/// was done, without being rewritten between runs.
const STAND_IN: &str = r#"#!/bin/sh
# A stand-in for the daemon's own command. It is the single program the app is
# allowed to start, so it records every call in one place and answers each verb.
log=${WHIRL_STUB_LOG:?}
printf '%s\n' "$*" >> "$log"
# This binary records that it was the one the app ran, beside itself, so a test
# with two copies can see which copy the app chose. `${0%/*}` rather than
# `dirname`, because a test may run with almost nothing on `PATH`.
: > "${0%/*}/ran"
# A test can make one call take its time, so that the app's deadline can be
# shown firing: the stand-in sleeps before it answers anything, and records the
# pid it sleeps under beside itself. The sleep is `exec`d, so that pid is the
# process the app must end rather than a shell waiting on one.
case "$1" in
  --version) sleep_for=${WHIRL_STUB_SLEEP_VERSION:-} ;;
  *)         sleep_for=${WHIRL_STUB_SLEEP_DAEMON:-} ;;
esac
if [ -n "$sleep_for" ]; then
  printf '%s\n' "$$" > "${0%/*}/pid"
  exec sleep "$sleep_for"
fi
# The app identifies a binary that answered the wrong way by asking for its
# static version. A stand-in answers only when the test gave it one, so a test
# that does not is the build from before the flag exists.
case "$1" in
  --version)
    if [ -n "${WHIRL_STUB_VERSION:-}" ]; then
      printf '%s\n' "$WHIRL_STUB_VERSION"
      exit 0
    fi
    exit 3
    ;;
esac
case "$2" in
  status)    code=${WHIRL_STUB_STATUS_CODE:-0};    words=${WHIRL_STUB_STATUS_WORDS:-status: com.guruor.whirl running};;
  start)     code=${WHIRL_STUB_START_CODE:-0};     words=${WHIRL_STUB_START_WORDS:-started: com.guruor.whirl};;
  stop)      code=${WHIRL_STUB_STOP_CODE:-0};      words=${WHIRL_STUB_STOP_WORDS:-stopped: com.guruor.whirl};;
  install)   code=${WHIRL_STUB_INSTALL_CODE:-0};   words=${WHIRL_STUB_INSTALL_WORDS:-installed: com.guruor.whirl};;
  uninstall) code=${WHIRL_STUB_UNINSTALL_CODE:-0}; words=${WHIRL_STUB_UNINSTALL_WORDS:-uninstalled: com.guruor.whirl};;
  *)         code=3; words="whirl: daemon is not a command, or it has the wrong number of arguments";;
esac
if [ "$code" = 0 ]; then
  printf '%s\n' "$words"
else
  # The real CLI prints a refusal, an unreachable daemon and a usage error on
  # standard error, and the app's stream split is what these tests check.
  printf '%s\n' "$words" >&2
fi
exit "$code"
"#;

/// A scratch directory with the stand-in in it, a scratch home, and the file
/// the stand-in logs to.
pub(crate) struct Stub {
    directory: PathBuf,
    /// A home directory of its own, so the app's search for the daemon cannot
    /// see the machine the tests run on: no `~/.local/bin/whirl`, no receipt at
    /// `~/Library/Application Support/whirl-ui/install.receipt`. A test writes
    /// into it by hand, so it is not private to this module.
    pub(crate) home: PathBuf,
    /// The file the stand-in appends every call to. A test points the app at it
    /// itself when it starts the app another way, so it is not private either.
    pub(crate) log: PathBuf,
}

impl Stub {
    pub(crate) fn new(tag: &str) -> Stub {
        let directory =
            std::env::temp_dir().join(format!("whirlui-daemon-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let command = directory.join("whirl");
        std::fs::write(&command, STAND_IN).expect("the stand-in");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&command)
                .expect("the stand-in")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&command, permissions).expect("an executable stand-in");
        }
        let home = directory.join("home");
        std::fs::create_dir_all(&home).expect("a scratch home");
        Stub {
            log: directory.join("calls.log"),
            directory,
            home,
        }
    }

    /// The absolute path of the stand-in, which is also the path the app must
    /// name when it reports which binary it ran.
    pub(crate) fn command(&self) -> PathBuf {
        self.directory.join("whirl")
    }

    /// Every call the app made, one `daemon <verb>` or `--version` per line.
    pub(crate) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Whether this copy was the one the app ran. The mark is beside the binary
    /// rather than in the shared call log, so two copies in one run are told
    /// apart.
    pub(crate) fn ran(&self) -> bool {
        self.directory.join("ran").exists()
    }

    /// Pay the one-off cost of running a file this machine has never run.
    ///
    /// The first execution of a script that was just written costs hundreds of
    /// milliseconds on macOS, measured here at 0.69 s against 0.07 s for the
    /// second run of the same file, because the platform checks a new executable
    /// before it will run it. A test that puts a bound on how long a child may
    /// live has to spend that cost before the app runs, or the bound is spent on
    /// the check rather than on the child. The warm-up's call log is a file of
    /// its own, and the marks it leaves beside the stand-in are cleared, so the
    /// app's own run is all that `calls`, `slept` and `ran` see.
    pub(crate) fn warm(&self) {
        let _ = Command::new(self.command())
            .arg("--version")
            .env("WHIRL_STUB_LOG", self.directory.join("warmup.log"))
            .env_remove("WHIRL_STUB_SLEEP_VERSION")
            .env_remove("WHIRL_STUB_SLEEP_DAEMON")
            .output();
        let _ = std::fs::remove_file(self.directory.join("ran"));
        let _ = std::fs::remove_file(self.directory.join("pid"));
    }

    /// The pid a call recorded before sleeping, when one got that far.
    pub(crate) fn slept(&self) -> Option<String> {
        std::fs::read_to_string(self.directory.join("pid"))
            .ok()
            .map(|pid| pid.trim().to_string())
    }

    /// Whether the app ended the sleeper rather than leaving it running.
    ///
    /// `ps` is asked about the pid the stand-in recorded: the sleep is the app's
    /// own child, so a child that was merely abandoned would still be a process
    /// here, and one killed without being reaped would be a zombie that `ps`
    /// still names.
    pub(crate) fn ended(&self) -> bool {
        let Some(pid) = self.slept() else {
            return false;
        };
        match Command::new("ps").args(["-o", "pid=", "-p", &pid]).output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().is_empty(),
            Err(_) => false,
        }
    }

    /// A `PATH` of the stand-in and nothing else the app could need.
    pub(crate) fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", self.directory.display())
    }

    /// Write an install receipt under the stub's own home, as `install.sh`
    /// does: the lines are written verbatim, and only the `binary` line for
    /// `whirl` is the command the app must find.
    pub(crate) fn write_receipt(&self, lines: &[&str]) {
        let receipt = self
            .home
            .join("Library/Application Support/whirl-ui/install.receipt");
        std::fs::create_dir_all(receipt.parent().expect("the receipt's directory"))
            .expect("the receipt's directory");
        std::fs::write(&receipt, lines.join("\n") + "\n").expect("the receipt");
    }
}

/// Run the app with the stand-in on `PATH`.
///
/// The search's inputs are all controlled here, so no part of the machine leaks
/// in: `HOME` is the stub's own, `WHIRL_PREFIX` and `WHIRL_UI_RECEIPT` are
/// removed unless the test passes one, and `PATH` is the stub's by default.
/// A test overrides any of them by name.
pub(crate) fn app(stub: &Stub, env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_whirl-ui"));
    command.args(args);
    command.env("PATH", stub.path());
    command.env("HOME", &stub.home);
    command.env("WHIRL_STUB_LOG", &stub.log);
    command.env_remove("WHIRL_PREFIX");
    command.env_remove("WHIRL_UI_RECEIPT");
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("the app runs")
}
