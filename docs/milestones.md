# whirl-ui milestones

M1 to M4 for this repository, written 2026-10-02. M1 is the first slice: the protocol client, the tray
item and the read-only settings window. M2 makes the settings window a writer, M3 is start at login,
and M4 is Windows and Linux. What is deliberately out of scope is at the end.

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
| 5 | It is light, and it is not in the Dock. | The running app has no Dock icon (its activation policy is accessory, and it never appears in the Dock), and after 60 s at rest `ps -o rss= -p <pid>` and `ps -o %cpu= -p <pid>` print the idle memory and CPU, stated against the measurement in whirl's `docs/research/frontend-stack.md` section 3. Artifact: the two `ps` lines with the rest time. |
| 6 | The settings window reports the effective plan, and edits nothing. | With a daemon running, the window's Sources, Rotation and App panes show the values `whirl status`, `whirl sources`, `whirl config path` and `whirl config check` print; every edit control is disabled and the reason it is disabled is visible in the window. Artifact: a screenshot of each pane beside those four commands' output. |
| 7 | It stays a client. | Checked against section 8's "must never" list: `lsof -p <pid>` names no path under the daemon's state directory (no state file is written), the wallpaper is unchanged while the app runs and its process tree contains no platform setter (no setter is called), and `pgrep -x whirld` names the same single daemon before launch and after quit (the daemon is never started, stopped or restarted). Artifact: the three commands' output. |

## M2: configuration from the UI, and the token in the platform store

M2 turns the settings window from a report into a writer. The write surface is the config file, under
the writer's contract in whirl's
[`docs/decisions/0002-frontends-write-config-own-no-daemon.md`](https://github.com/guruor/whirl/blob/main/docs/decisions/0002-frontends-write-config-own-no-daemon.md):
the frontend writes the file the daemon reads, and owns no part of the daemon. It still never calls a
platform setter, never touches a state file, and never starts or stops the daemon.

| # | criterion | proof |
|---|---|---|
| 1 | A setting changed in the window is in the config file and in the effective plan. | After the rotation interval is edited in the window, `whirl config path` names the file, the bytes at that path carry the new value, the file's mode is still `0600`, and `whirl config check` prints the new value; `whirl status` prints it once the daemon has started on the edited file (the daemon reads the config once at startup; the supervisor owns the restart). Artifact: the file at the printed path, its mode, and the two commands' lines. |
| 2 | Sources are editable, including Wallhaven. | Adding a `wallhaven` source in the window writes a source record that `whirl config check` accepts and `whirl sources` lists; a record the config refuses is refused by the window with the same reason `whirl config check` prints, rather than silently kept. Artifact: the config file's source records and both commands' output. |
| 3 | The Wallhaven token is written to the platform store and never read back. | Entering a token writes the label `keychain:whirl-wallhaven` into `sources[i].api_key_ref` in the config file, and the secret itself into the platform store (`security find-generic-password -s whirl-wallhaven` on macOS). No code path reads the value back: nothing the app prints contains it, and the config file never holds it (a config whose `api_key_ref` is key-shaped is refused, per whirl's `docs/architecture.md` section 6.3). Artifact: the store query's metadata line (never the secret) and the config file's `api_key_ref` line. |

## M3: start at login

Two independent login items, owned by two different things: the app registers its own, and the daemon
gets a supervisor unit from whirl's own installation, not from this app.

| # | criterion | proof |
|---|---|---|
| 1 | The app installs and removes its own login item. | After "start at login" is enabled in the app, `launchctl print gui/$(id -u)/<app-label>` exits 0 and `sfltool dumpbtm` lists the app; disabling removes it from both. Artifact: the two commands' output before and after. |
| 2 | The app does not own the daemon's lifetime. | `grep -rn 'LaunchAgents' crates/whirl-ui/src` names no write to `~/Library/LaunchAgents`, and `pgrep -x whirld` reports the same single daemon before launch and after quit with the login item enabled (section 8 item 3). Artifact: the grep and the `pgrep` output. |
| 3 | The daemon's unit is whirl's, not the app's. | Once whirl's installation ships the unit, `launchctl print gui/$(id -u)/com.guruor.whirl` exits 0 whether or not the app is installed, and the unit file is the one whirl's installer wrote. Artifact: the command's output and the unit file's path. |

## M4: Windows and Linux

whirl's wallpaper backends on those platforms are stubs in v0.1.0, so this milestone waits on whirl
rather than on this repository. The tray and the settings window become cross-platform once the daemon
can set a wallpaper on the platform at all.

| # | criterion | proof |
|---|---|---|
| 1 | whirl sets a wallpaper on the platform. | On a real Windows or Linux desktop, `whirl status` prints a `current:` image and the platform's own readback returns it. This is whirl's criterion, and this milestone does not start until whirl's own platform work closes. Artifact: whirl's release notes for the platform. |
| 2 | The workspace builds and the app runs on the platform. | `cargo build --workspace --release` succeeds for `windows-latest` and `ubuntu-latest`, and on a real session `cargo run -- --menu-dump` prints the menu rows and the tray shows the daemon's status. Artifact: the CI legs' URLs, the `--menu-dump` output and a screenshot per platform. |
| 3 | The macOS gate is gone. | `grep -rn 'cfg(target_os = "macos")' crates/whirl-ui/src` returns no line that excludes Windows or Linux from shipped behaviour; any gate that remains carries the reason it stays. Artifact: the grep output. |

## Deliberately out of scope

| not planned | why |
|---|---|
| A collection preview, which would show a source's candidates before a rotation picks one. | It needs either a new daemon verb that returns the candidates or the app implementing a source itself, and section 8 item 4 forbids the second: a frontend that implements a source is a second implementation that drifts from the worker's. Deferred until whirl grows the verb, not dropped. |

The repository's license is also open, and it is a decision rather than a milestone: with no `LICENSE`
file all rights are reserved, so nobody can reuse a reference implementation whose reason to exist is
being reused.
