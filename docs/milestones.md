# whirl-ui milestones

M1 to M4 for this repository, written 2026-10-02. M1 is the first slice: the protocol client, the tray
item and the read-only settings window. M2 makes the settings window a writer, M3 is start at login,
and M4 is Windows and Linux. The app's appearance is not a milestone: it is settled in
`docs/design.md`, which also holds the reference's extra enhancements. A report a tester can send is
future work, parked outside the milestone sequence, with the list in `docs/research/reporting.md`.
What is deliberately out of scope is at the end.

## Why this file exists

The repository shipped with a README, a CI file and no plan, so nothing on disk said what this frontend
is for, which milestone is being worked, or what has to hold before one can close. This file is that
plan. Its criteria are the thing to hold a closing summary to.

## What a milestone is here

- A **milestone** is a batch of work, named `M1`, `M2`, and so on.
- A milestone **closes when its cards close and its criteria hold**.
- **A criterion is a capability and its proof**: a command run against a ref plus the value it must
  print, or an artifact that must exist. It is never "the feature works", a pull request's state, or
  another card's status.

Where a criterion here names a line, it is the line as the daemon prints it, because the frontend has
no vocabulary of its own: whirl's `docs/architecture.md` section 2 is the protocol and section 8 is the
contract this repository obeys
(<https://github.com/guruor/whirl/blob/main/docs/architecture.md>).

## M1: the client, the tray item, and the read-only settings window

Three pieces: `crates/whirlui-client`, which speaks whirl's protocol; the tray item, which reads
`status`, follows `subscribe` and drives the daemon's own verbs; and the settings window, which shows
the effective plan and refuses to edit anything yet.

### M1's exit criteria

| # | criterion | proof |
|---|---|---|
| 1 | The workspace builds and its suite passes under the pinned toolchain. | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` each exit 0, and the `fmt`, `clippy (ubuntu-latest)`, `clippy (macos-latest)`, `clippy (windows-latest)`, `test (ubuntu-latest)`, `test (macos-latest)` and `test (windows-latest)` jobs in `.github/workflows/ci.yml` are green at the milestone's head. Artifact: the CI run URL for that head. |
| 2 | The client negotiates the protocol, reads the snapshot, and survives a gap. | Against a running `whirld`, `cargo test -p whirlui-client` prints, each passing: `negotiates_the_greeting_and_hello` (the greeting's `protocol` number is read and `hello` sent, per section 2.4), `reads_status` (the stable key set of 2.10), `re_reads_status_after_a_seq_gap` (a `seq` that skips a value triggers a fresh `status`, per section 8 item 3), and `refuses_an_unknown_protocol_version` (a daemon speaking a number the client does not know is refused, not guessed at). Artifact: those four test names and their output lines. |
| 3 | The tray menu is checkable without a display. | `cargo run -- --menu-dump` prints one row per menu entry, in the order they appear on screen, and exits 0 with no display attached. Artifact: the printed rows, plus a screenshot of the real macOS tray menu, kept with the milestone's closing report. |
| 4 | The menu's verbs reach the daemon, and the app's view follows the daemon. | With the tray running, `whirl status` prints a different image line before and after the menu's Next; Pause changes `paused=0` to `paused=1` and Resume changes it back; and after a change made from the CLI the app's "now" line updates within 1 s, measured by reading `date +%s%N` either side of the change. Artifact: the `whirl status` lines before and after, and the measured interval. |
| 5 | It is light, and it is not in the Dock. | The running app has no Dock icon while no window is open (its activation policy is accessory then, and it appears nowhere in the Dock in that state), and after 60 s at rest `ps -o rss= -p <pid>` and `ps -o %cpu= -p <pid>` print the idle memory and CPU, stated against the measurement in whirl's `docs/research/frontend-stack.md` section 3. Artifact: the two `ps` lines with the rest time. **Amended 2026-10-05, deliberately.** The policy used to be accessory unconditionally, and the no-Dock-tile reading was right while nothing was on screen and wrong about what it cost: an agent app's windows are not managed by the window manager, so the settings window could not be raised once another app was over it and the app had no Cmd+Tab entry to be switched back to. It is now conditional -- accessory while no window is open, regular while one is, accessory again when the last one closes -- so the Dock tile is the window's and does not outlive it. What this criterion protects, an app that is not in the Dock while it is just a menu bar item, is unchanged; the switch is `crate::window::policy_for` and the pair is asserted by `tests::the_app_is_a_regular_app_only_while_a_window_is_open`. |
| 6 | The settings window reports the effective plan, and edits nothing. | With a daemon running, the window's Sources, Rotation and Background Helper panes show the values `whirl status`, `whirl sources`, `whirl config path` and `whirl config check` print; every edit control is disabled and the reason it is disabled is visible in the window. Artifact: a screenshot of each pane beside those four commands' output. |
| 7 | It stays a client. | Checked against section 8's "must never" list: `lsof -p <pid>` names no path under the daemon's state directory (no state file is written), the wallpaper is unchanged while the app runs and its process tree contains no platform setter (no setter is called), and `pgrep -x whirld` names the same single daemon before launch and after quit (the daemon is never started, stopped or restarted). Artifact: the three commands' output. |

## M2: configuration from the UI, and the token in the platform store

M2 turns the settings window from a report into a writer. The write surface is the config file, under
the writer's contract in whirl's
[`docs/decisions/0002-frontends-write-config-own-no-daemon.md`](https://github.com/guruor/whirl/blob/main/docs/decisions/0002-frontends-write-config-own-no-daemon.md):
the frontend writes the file the daemon reads, and owns no part of the daemon. It still never calls a
platform setter, never touches a state file, and starts no daemon: every part it has in the daemon's
lifecycle is the daemon's own command, `whirl daemon install|uninstall|start|stop|status`, which asks
the OS supervisor for the job the supervisor owns (whirl's `docs/architecture.md` section 8, "may
rely on" 8).

| # | criterion | proof |
|---|---|---|
| 1 | A setting changed in the window is in the config file and in the effective plan. | After the rotation interval is edited in the window, `whirl config path` names the file, the bytes at that path carry the new value, the file's mode is still `0600`, and `whirl config check` prints the new value; `whirl status` prints it once the daemon has started on the edited file (the daemon reads the config once at startup; the supervisor owns the restart). Artifact: the file at the printed path, its mode, and the two commands' lines. |
| 2 | Sources are editable, including Wallhaven. | Adding a `wallhaven` source in the window writes a source record that `whirl config check` accepts and `whirl sources` lists; a record the config refuses is refused by the window with the same reason `whirl config check` prints, rather than silently kept. Artifact: the config file's source records and both commands' output. |
| 3 | The Wallhaven token is written to the platform store and never read back. | Entering a token writes the label `keychain:whirl-wallhaven` into `sources[i].api_key_ref` in the config file, and the secret itself into the platform store (`security find-generic-password -s whirl-wallhaven` on macOS). No code path reads the value back: nothing the app prints contains it, and the config file never holds it (a config whose `api_key_ref` is key-shaped is refused, per whirl's `docs/architecture.md` section 6.3). Artifact: the store query's metadata line (never the secret) and the config file's `api_key_ref` line. |

## M3: start at login

Two independent login items, owned by two different things: the app owns its own login item through
macOS, and the daemon's supervisor unit is whirl's, written only by whirl's own command,
`whirl daemon install`, and never by this app. This app reaches the daemon's lifecycle only by running
that command: `whirl daemon install`, `whirl daemon uninstall`, `whirl daemon start`, `whirl daemon
stop` and `whirl daemon status`, all five verbs `whirl daemon` has, and never by writing the unit or
calling `launchctl` itself (whirl's
`docs/architecture.md` section 8, "may rely on" 8, and the accepted ADR
[`docs/decisions/0002-frontends-write-config-own-no-daemon.md`](https://github.com/guruor/whirl/blob/main/docs/decisions/0002-frontends-write-config-own-no-daemon.md),
decision 3).

| # | criterion | proof |
|---|---|---|
| 1 | The app installs and removes its own login item. | The app's own mode does both: `whirl-ui --login-item register` then `whirl-ui --login-item status` prints `login item: enabled (status 1)`, and `sfltool dumpbtm` lists one row for the app (`Name: Whirl`, `Identifier: 2.com.guruor.whirl-ui`, `URL: file:///Applications/Whirl.app/`); `whirl-ui --login-item unregister` prints `login item: not registered (status 0)` and the Login Items pane no longer lists Whirl (the operator's own visual check). The bundle is what macOS registers and the status is read from the app's own `SMAppService.mainApp.status`, so the mode refuses from a bare binary and names the path it looked at rather than registering the directory it sits in. `sfltool dumpbtm` runs without root (exit 0, measured 2026-10-04). Two limits belong in the criterion rather than behind it, because a proof nobody can produce is worse than none: `unregister` leaves the app's row in the BTM database as `Disposition: [disabled, allowed, notified]` (the API has no delete, and the row survives the bundle being moved away), and no part of this is a launchd service, so `launchctl print gui/$(id -u)/<label>` exits 113 for every label (measured on macOS 26.7) and proves nothing either way. Artifact: the two status lines, the two `dumpbtm` rows, and the Login Items pane. |
| 2 | The app does not own the daemon's lifetime. | `grep -rn 'LaunchAgents' crates/whirl-ui/src` names no write to `~/Library/LaunchAgents`, and that grep keeps the meaning it always had: the app writes no unit and spawns no process, and the one way it changes the unit is whirl's own command rather than a file. That reach is the whole of `crates/whirl-ui/src/daemon_cli.rs`: `grep -rn 'Command::new' crates/whirl-ui/src` names `whirl` for a daemon command (its other spawns are `curl` for the About pane's release check, `/usr/bin/security` for the platform store and `open` for the Login Items pane in System Settings, and none of them is the daemon), and the file's `Verb` enum is `Install`, `Uninstall`, `Start`, `Stop` and `Status`, the five verbs `whirl daemon` has -- `install`, `uninstall`, `start`, `stop` and `status`, from whirl's `crates/whirl-cli/src/daemon.rs` -- so the app reaches the daemon's lifecycle through whirl's own command and nothing wider, and starts no process of the daemon's kind: the supervisor starts one when the app asks the supervisor's own command to (section 8, "may rely on" 8). A quit with "Also stop whirl" unchecked changes nothing: `pgrep -x whirld` names the same process before and after it. Artifact: the two greps, the log the stand-in `whirl` records, and the `pgrep` output. |
| 3 | The daemon's unit is whirl's, not the app's. | The unit is written only by `whirl daemon install` and lives at `~/Library/LaunchAgents/com.guruor.whirl.plist` (whirl's `docs/architecture.md` 5.2), so `ls ~/Library/LaunchAgents` names it once that step has been run and not before. `launchctl print gui/$(id -u)/com.guruor.whirl` exits 0 when the job is loaded and 113 (`Could not find service`) when it is not, whether or not the app is installed (measured on macOS 26.7); `whirl daemon status` is the finer answer, exit 0 running, 1 loaded and stopped, 2 no such job. Nothing in this repository writes it: `grep -rn 'LaunchAgents' crates/` names no write. Artifact: the two commands' output and the unit file's path. |

## M4: Windows and Linux

whirl's wallpaper backends on those platforms are stubs in v0.1.0, so this milestone waits on whirl
rather than on this repository. The tray and the settings window become cross-platform once the daemon
can set a wallpaper on the platform at all.

| # | criterion | proof |
|---|---|---|
| 1 | whirl sets a wallpaper on the platform. | On a real Windows or Linux desktop, `whirl status` prints a `current:` image and the platform's own readback returns it. This is whirl's criterion, and this milestone does not start until whirl's own platform work closes. Artifact: whirl's release notes for the platform. |
| 2 | The workspace builds and the app runs on the platform. | `cargo build --workspace --release` succeeds for `windows-latest` and `ubuntu-latest`, and on a real session `cargo run -- --menu-dump` prints the menu rows and the tray shows the daemon's status. Artifact: the CI legs' URLs, the `--menu-dump` output and a screenshot per platform. |
| 3 | The macOS gate is gone. | `grep -rn 'cfg(target_os = "macos")' crates/whirl-ui/src` returns no line that excludes Windows or Linux from shipped behaviour; any gate that remains carries the reason it stays. Artifact: the grep output. |

## The report a tester can send

A tester on a machine we cannot reach has no way to hand us a log, and the app writes almost
none: it has no log file and its `eprintln!` lines die with the terminal, while the daemon keeps
`~/Library/Logs/whirl/whirl.log`. Two defects from one day, a window that opened black and a
Gatekeeper refusal on an ad-hoc signed bundle, both left nothing behind. The maintainer asked for a
reporting mechanism and framed it as future work:

> we can see if we can add some kind of reporting mechanism so when testing on different os we should
> be able to report the logs to the developer. Make sure the logs doesn't contain any private or
> secret info. The logs should be self sufficient so we can understand how the app is behaving
> without missing any critical detail. We can research for best option other cross-platform free apps
> use for this and we can follow the same. We can add this as future work in milestone.

This is parked outside the milestone sequence: no milestone above depends on it, and which part is
worth building first is the maintainer's call. The survey of what free, cross-platform apps do, the
fields a self-sufficient report must carry, the rule and mechanism that keep a secret out, how the
bundle reaches the developer, and the first slice are in `docs/research/reporting.md` alone and are
not repeated here.

## Appearance

The app's appearance is settled in `docs/design.md`, not here, and that file is authoritative for it:
the visual language, and the palette sampled from the maintainer's own reference mockup with the
command that produced each value recorded beside it. It was written 2026-10-04 from that reference, so
a later reader can re-derive the numbers rather than read them by eye. The palette there is the
contract for the dark theme, not a starting point to be improved on.

## The reference's extra functionality

`docs/design.md` also takes each element the reference draws and states what it needs today and what
config change it would imply. Those items are parked outside the milestone sequence: no milestone in
this file depends on them, and which of them are worth building is the maintainer's call. The list
lives in `docs/design.md` alone and is not repeated here.

## Deliberately out of scope

| not planned | why |
|---|---|
| A collection preview, which would show a source's candidates before a rotation picks one. | It needs either a new daemon verb that returns the candidates or the app implementing a source itself, and section 8 item 4 forbids the second: a frontend that implements a source is a second implementation that drifts from the worker's. Deferred until whirl grows the verb, not dropped. |

The repository's license is a decision rather than a milestone: it is MIT, declared in the workspace
manifest and in `LICENSE` at the root.
