# How Whirl updates itself, and the daemon it needs

A design, no code. It says how the app checks for a newer release, what the person sees, which
preferences exist, what the app may do without asking, what it must hand to the person, and what it
must refuse to do. Written 2026-10-05.

The one rule this file obeys: every claim about this repository is a file and a line that says what
the claim says it says. The daemon's contract lives in another repository
(`https://github.com/guruor/whirl`), and a claim about it names that file too. Nothing here asks the
app to drive an authorization dialog, to use root, or to write the daemon's login unit.

## What exists today, and where

- The About pane has one button that makes one check, and `--check-update` drives the same call.
  The check is one anonymous `GET` to
  `https://api.github.com/repos/{slug}/releases/latest` (`crates/whirl-ui/src/about.rs:216-221`),
  made with `/usr/bin/curl` (`crates/whirl-ui/src/about.rs:229-242`), and it has three outcomes: a newer release is
  published, this build is the newest, or the check could not be made
  (`crates/whirl-ui/src/about.rs:156-184`, `crates/whirl-ui/src/about.rs:203-213`). The status is read from the HTTP code, so a repository
  with nothing published (`404`) and the anonymous rate limit (`403`) are named failures and never
  "up to date" (`crates/whirl-ui/src/about.rs:251-260`).
- A build between two releases is not told to update to an older one: the running version is
  compared as numbers and a release the build is ahead of reads as the newest
  (`crates/whirl-ui/src/about.rs:191-197`, `crates/whirl-ui/src/about.rs:281-293`, and the test at `crates/whirl-ui/src/about.rs:344-356`).
- The pane draws the check as a button and never as a schedule, and says so: "Nothing schedules it"
  (`crates/whirl-ui/src/about.rs:199-202`), "Nothing here runs by itself" (`crates/whirl-ui/src/about.rs:24-27`). The window opens with no
  check made: `Settings::check` is `None` until the button is pressed
  (`crates/whirl-ui/src/settings.rs:201-203`, `crates/whirl-ui/src/settings.rs:841-843`).
- The check's own words on screen are fixed and are not to be duplicated:
  `CHECK_TITLE` = "Is there a newer one?", `CHECK_LABEL` = "Check for a newer release",
  `CHECK_LINE` = "asks GitHub once for this app's newest published release; it downloads nothing,
  sends nothing but the request, and repeats nothing on its own"
  (`crates/whirl-ui/src/settings.rs:159-165`).
- `install.sh` is the installer, and it is written to be re-run rather than to update: "A daemon
  that is already installed is reused, printed, and never touched" (
  **unverified at this head**: a daemon already installed is now reused only when it is the
  same release or newer (`install.sh:7-10`), and an older one is replaced (`install.sh:13-14`);
  the old `install.sh:253-264` reuse block is now the daemon versioning
  at `install.sh:310-371`),
  and an app already at the version asked for is reported present and left
  alone (`install.sh:373-379`). Its rules are the ones this design reuses: verify the checksum
  before anything is unpacked (`install.sh:31-33`, `install.sh:241-257`), a receipt records what a
  run installed (`install.sh:16-23`, `install.sh:168`), and a downloaded copy has its quarantine
  flag cleared after the checksum passes and not before (`install.sh:543-553`).
- Nothing in the app applies an update today. There is no preference file, no last-check time, and
  no prompt. The only file the app owns is the install receipt under
  `~/Library/Application Support/whirl-ui/` (`install.sh:168`).

## Decisions

Two decisions are recorded here as decisions, not buried in a section. The rest of the document
follows from them.

### D1. The restart prompt: offered, default to restarting (Karabiner-Elements pattern)

After the daemon's binaries are replaced, the running daemon keeps the old binary until it restarts.
The app offers a restart, and the prompt's default button is the restart. This is the pattern
Karabiner-Elements uses: it updates itself and then tells the person a restart is needed, with the
restart one click away (its release notes list "the ability to restart Karabiner-Elements from the
menu", <https://karabiner-elements.pqrs.org/docs/releasenotes/>).

The constraint the pattern has to fit: the app must not restart the daemon itself. The frontend
contract's "must never" item 3 is that a frontend never spawns a daemon of its own, because the OS
supervisor owns the daemon's lifetime (whirl `docs/architecture.md` §8 "must never" 3, lines
1865-1871, and §5.1, lines 1551-1572). Running the daemon's own step is the one route that is not
"being the supervisor": whirl's ADR 0002, decision 3, narrows the rule to its reason and reads it as
"a frontend is never the supervisor" (`whirl
docs/decisions/0002-frontends-write-config-own-no-daemon.md:343-393`, the narrowing shipped as §8
"must never" 3 at `whirl docs/architecture.md:1865-1871`, and the implied change that writes it at
`:550-553`), so the app may run the product's own step but must never call `launchctl`, `systemctl`
or `schtasks` itself (ADR 0002, lines 360-361 and 507-511).

The step is `whirl daemon stop` then `whirl daemon start`, the daemon's own lifecycle pair. `whirl
daemon` is the CLI's five-subcommand group: `install`, `uninstall`, `start`, `stop`, `status`. Its
`start` and `stop` are two of the steps the frontend contract names as a frontend's to run
(whirl `crates/whirl-cli/src/daemon.rs:35-47`, and §8 "may rely on" 8 at `whirl
docs/architecture.md:1838-1845`).

Three consequences, stated rather than hidden:

- **Running the step is permitted, and whirl ships it.** §8 "must never" 3 forbids spawning a daemon
  of the app's own, and its "may rely on" 8 names `whirl daemon …` as the steps a frontend makes
  (`whirl docs/architecture.md:1865-1871`, `:1838-1845`; ADR 0002, implied change 4, at `whirl
  docs/decisions/0002-frontends-write-config-own-no-daemon.md:550-553`). The pinned whirl carries
  them: the five subcommands are whirl's `crates/whirl-cli/src/daemon.rs:35-47`, they ship from
  v0.2.0, and `install.sh:163` pins `v0.2.1`. The app already drives three of them (`whirl daemon
  status`, `whirl daemon start` and `whirl daemon stop`) through
  `crates/whirl-ui/src/daemon_cli.rs:147-163` (the `Verb` enum) and its `launch` and `finish_quit`
  (`:473-497`).
- **What does not exist yet is the prompt, not the step.** Nothing in the app applies an update
  today, so nothing calls the restart step; when the update flow is built, the prompt's Restart
  button runs `whirl daemon stop` then `whirl daemon start` and reports what the command said, which
  is what section 5 spells out.
- **The app never runs the supervisor's own command.** `launchctl kickstart -k gui/<uid>/com.guruor.whirl`
  is the supervisor action §5.2 names (`whirl docs/architecture.md:1584-1585`), and it is the
  person's to run, not the app's.

### D2. Automatic mode is per piece: daemon unattended, bundle on approval (decision to confirm)

The reporter did not answer this one, so this is the proposal's default and is marked as a decision
to confirm.

- **The daemon's three binaries may be updated unattended.** They go into `~/.local/bin`
  (`install.sh:7-9`), which is inside the person's home. The one write macOS may ask to authorize is
  the app's prefix, `/Applications`, outside the home ("That one write is the only thing here macOS
  may ask you to authorize", `install.sh:25-29`). So a daemon update needs no authorization and can
  be done without asking.
- **The app's own bundle may not.** Replacing `Whirl.app` in `/Applications` is exactly that write,
  and `install.sh` refuses to drive or answer the dialog: "When the destination needs an
  authorization this script cannot ask for, it says what to do and exits 3 with nothing installed"
  (`install.sh:25-29`, and the refusal at `install.sh:386-398`). An unattended app-bundle update
  therefore cannot be promised.

The trade-off in one place: "automatic" is not one setting, it is a setting per piece. Under
`automatic` the daemon is updated silently and the app bundle is offered for the person's approval.
Under `ask` both are offered. Nothing in `automatic` silently replaces the app bundle, because the
app cannot satisfy that authorization, and promising it would be the promise that fails on the
person's machine rather than in this document.

## 1. The check

**Who checks.** The app, in two places and only these two: on startup, and on demand (the About
pane's button, and `--check-update`). The on-demand call is the existing one (`crates/whirl-ui/src/settings.rs:841-843`,
`crates/whirl-ui/src/main.rs:405-412`); the startup call is new.

**From where.** The endpoint already in use, unchanged:
`https://api.github.com/repos/guruor/whirl-ui/releases/latest` (`crates/whirl-ui/src/about.rs:216-221`). The daemon is a
separate repository with its own tags (`install.sh:162-163` sets `WHIRL_UI_VERSION` and
`WHIRL_VERSION` independently), so a daemon check is a second call to the analogous endpoint,
`https://api.github.com/repos/guruor/whirl/releases/latest`, derived the same way. **Decision to
confirm:** whether the daemon check is a second endpoint (as written here) or whether a whirl-ui
release declares the daemon version it pairs with; the second would need a field this release does
not carry today.

**How often.** At most once per launch and at most once per interval. The interval is a named
constant in the app, `CHECK_INTERVAL_SECONDS = 86400` (24 hours), not a preference, so there is no
second key to configure. The last-check time is written to the app's preference file (section 3)
after every check that ran, whether it reached GitHub or could not be made, on demand as well as on
startup. On startup the check runs
only when `check_on_startup` is true **and** `now - last_check_at >= CHECK_INTERVAL_SECONDS` (with
`last_check_at == 0`, never checked, counting as due).

**When it fails, or there is no network.** The check is silent. A failure never produces a prompt
and never produces a notification: `Check::CouldNot` is recorded, the startup path draws nothing,
and `last_check_at` is still advanced so a machine that is offline does not retry every launch. The
reason is visible only where a person asks for it: the About pane's existing line,
"the check could not be made: {reason}" (`crates/whirl-ui/src/about.rs:181`). This is the existing rule that a check
which could not be made is never a false "up to date" (`crates/whirl-ui/src/about.rs:19-22`), kept as-is.

The check is one `GET` and nothing else; it downloads nothing, sends nothing but the request, and
carries no identifier of the app (`crates/whirl-ui/src/about.rs:13-18`). The update itself is a separate, later,
deliberate action (section 4).

## 2. The states, and the exact strings

| state | on startup | on demand (About pane) |
|---|---|---|
| up to date | silent | the existing line: "this is the newest version published, {version}" (`crates/whirl-ui/src/about.rs:179`) |
| newer available | the prompt below | the existing line: "a newer version is published: {version}, at {page}" (`crates/whirl-ui/src/about.rs:174-177`) |
| skipped version | silent (the version was skipped) | the existing "newer" line; no prompt |
| check failed | silent | "the check could not be made: {reason}" (`crates/whirl-ui/src/about.rs:181`) |

### The update prompt

Title: `A newer version of Whirl is available`

Body, the app line: `Whirl {version} is published. This build is {running}.`

Body, the daemon line, drawn only when the daemon check also found a newer one:
`The whirl daemon {daemon_version} is published. The installed one is {daemon_running}.`

Body, the note under the two lines:
`The app and the daemon update separately. Updating the daemon does not stop the running one; it keeps the old version until it restarts.`

Controls, in this order:

- `Update Now` (the default button)
- `Later`
- `Skip This Version`
- a checkbox: `Don't ask again (update automatically)`

Default answer: **Update Now**, with the checkbox unchecked. `Esc` and the window's close control
are `Later`.

What each control does:

- `Update Now` applies the pieces section 4 allows without asking (the daemon binaries), then
  offers the app bundle for approval if one is available. With the checkbox ticked it also sets
  `update_mode` to `automatic`.
- `Later` closes the prompt and keeps the release unremembered, so the next launch's check may offer
  it again.
- `Skip This Version` writes the release's version to `skipped_version` and closes the prompt. The
  startup check draws nothing for that exact version after this; a later release still prompts.
- `Don't ask again (update automatically)` sets `update_mode` to `automatic` (section 3), which is
  the reporter's "confirm the automatic update" case.

### The restart prompt

Drawn after a daemon update lands, and after the app is told the running daemon is older than the
installed binary.

Title: `Restart the whirl daemon?`

Body: `The whirl daemon {daemon_version} is installed. The running daemon is still {daemon_running} and keeps running the old version until it restarts.`

Controls:

- `Restart Now` (the default button)
- `Later`

Default answer: **Restart Now**. What `Restart Now` runs is section 5.

## 3. The preferences

Storage: `~/Library/Application Support/whirl-ui/preferences.json`, written by the app, mode `0600`,
atomic (temp file in the same directory, `fsync`, `rename`; the ordering rule `install.sh` uses for
its writes, `install.sh:31-33`). It sits beside the app's existing file, the install receipt
(`install.sh:168`). It is **not** the daemon's config file: that file is the daemon's, has its own
schema and keys, and the frontend edits it only under ADR 0002's writer contract.

Names, storage and defaults, beside what exists today. There is no existing preference key, so this
table adds four and duplicates none:

| key | type | values | default | exists today? |
|---|---|---|---|---|
| `check_on_startup` | boolean | true / false | `true` | no, new |
| `update_mode` | string | `"ask"` \| `"automatic"` \| `"never"` | `"ask"` | no, new |
| `skipped_version` | string | a version, or empty for none | `""` | no, new |
| `last_check_at` | integer | Unix seconds, `0` for never | `0` | no, new |

What the values mean, and where they are set:

- `check_on_startup`: whether the startup check runs. Set in the About pane. A file-only knob is
  not put on screen (`docs/design.md:148`), so this one, which is asked for, is on screen.
- `update_mode`: `ask`: every check that finds something prompts. `automatic`: the daemon binaries
  are updated without a prompt and only the app bundle is offered (D2). `never`: the startup check
  still runs and still records `last_check_at`, but the app applies nothing on its own and draws no
  prompt. `ask` and `automatic` are reachable from the prompt; `never` is file-only in the first
  version, and by the file-only rule is not drawn.
- `skipped_version`: written by `Skip This Version`; a startup check that finds exactly this
  version is silent.
- `last_check_at`: written after every check that ran, whether it reached GitHub or could not be
  made, on demand and on startup.

The existing strings above (`CHECK_TITLE`, `CHECK_LABEL`, `CHECK_LINE`, the three `Check::line`
outcomes) are reused and are not restated or renamed. The new strings are the two prompts' titles,
lines and controls in section 2; none of them collides with an existing constant (`crates/whirl-ui/src/settings.rs:159-165`
is the only place the check's words live today).

## 4. Who applies what

Three pieces: the app bundle, the daemon's binaries, and the running daemon. For each: what the app
may do unattended, what it hands to the person, what it must refuse.

The whole path reuses `install.sh`'s rules, not a second installer: verify before writing
(`install.sh:31-33`, `install.sh:241-257`), a checksum published beside every archive
(`install.sh:170-173` names both archives and both `.sha256` files, and `scripts/make-bundle.sh:168-175`
writes them), the receipt (`install.sh:16-23`), and the quarantine clear after the checksum passes
(`install.sh:543-553`). One change to `install.sh` is required and is named here: it plans the daemon
and the app as one run (it refuses at `install.sh:386-398` before writing either half when
`/Applications` needs an authorization). **Unverified at this head:** this sentence also said "it
currently reuses an installed daemon (`install.sh:253-264`), so it cannot update one"; `install.sh`
now versions the daemon it finds and replaces an older one (`install.sh:310-371`, `install.sh:7-10`),
so that reason no longer holds. The change is a piece selector: install the daemon half, the app
half, or both, so the app can run the daemon half alone under D2. That selector is the design's one
required change to `install.sh`; nothing else about its ordering or its refusals changes.

### The daemon's binaries: unattended

What the app may do, naming the command. `<install.sh>` below is the release's own copy, fetched
from the tag being installed the way `install.sh:48-50` documents
(`https://raw.githubusercontent.com/guruor/whirl-ui/<app tag>/install.sh`). It is the copy that
carries the new `--daemon-only` selector, not a copy the app already has. The app runs the daemon
half of that script with the versions pinned to the release it has already decided to install:

```
WHIRL_VERSION=<daemon tag> WHIRL_UI_VERSION=<app version> /bin/sh <install.sh> --daemon-only
```

`install.sh`'s exit codes are unchanged and are what the app branches on (`install.sh:70-72`):
`0` installed or reused as reported, `1` refused (platform, download, checksum, unwritable
destination), `2` an argument was given that is not `--help`, `3` the app's prefix needs an
authorization this script cannot ask for. Under `--daemon-only` the app half is not planned, so `3`
is not reachable on this path.

The steps that run, each in full, are the ones `install.sh` already runs for the daemon: fetch the
archive and its checksum (`install.sh:431-439`), verify (`install.sh:241-257`), extract the three
binaries into the prefix (`install.sh:467-478`). `whirl`, `whirld` and `whirl-worker` must all be
present or the run refuses and installs nothing (`install.sh:471-474`).

What the app must refuse: it must never write the daemon's login unit (the script asks whirl's own
installer and writes no unit file, `install.sh:480-510`), and it must never spawn `whirld` or remove
the socket (`whirl docs/architecture.md` §8 "must never" 3 and 5, lines 1865-1874).

### The app bundle: on approval

What the app may do unattended: fetch and verify the archive. A checksum that does not match is a
refusal with the published and actual digests named and nothing installed (`install.sh:250-254`).

What it must hand to the person. The write into `/Applications` is the authorization the app cannot
ask for (`install.sh:25-29`). So the app does not perform it; it shows the exact command and the
reason, which is the same route `install.sh` prints on refusal:

```
WHIRL_UI_PREFIX="$HOME/Applications" /bin/sh <install.sh>
```

or, to keep `/Applications`, the person runs the installer where the authorization can be given:

```
WHIRL_UI_VERSION=<app version> /bin/sh <install.sh>
```

The app must refuse to drive or dismiss the authorization dialog and must not use root or a password
(`install.sh:25-29`, `install.sh:448-454`). After a new bundle lands, the quarantine clear is the
step that makes it openable at all: the app is ad-hoc signed and not notarized
(`scripts/make-bundle.sh:19-29`, `scripts/make-bundle.sh:144-164`), so without
`xattr -dr com.apple.quarantine /Applications/Whirl.app` (`install.sh:548`, and the documented
first-launch step at `docs/releases/v0.1.0.md:62`) the updated app opens behind the
unidentified-developer dialog. The clear happens only after the checksum has passed
(`install.sh:543-553`), never before.

### The running daemon: offered, not applied

The old daemon keeps running the old binary. The app does not restart it (D1). It offers the restart
and, on the person's answer, does what section 5 says.

## 5. The half-updated case

After the daemon's binaries are replaced, `~/.local/bin/whirl` is the new version and the running
daemon is the old one, because a replaced file is not the running process. What the person is told
is the restart prompt in section 2. The default is to restart (D1).

What `Restart Now` does is the daemon's own start/stop step, named in full:

```
whirl daemon stop
whirl daemon start
```

Its exit codes are the CLI's own (whirl `crates/whirl-cli/src/daemon.rs:21`, and §8 "may rely on" 7
at `whirl docs/architecture.md:1835-1837`): `0` the step was done, `1` whirl refused, `2` there is no
supervised daemon to reach (or the supervisor could not be asked), `3` the command line was wrong.
For the two steps the app runs, the codes that occur are `0` and `1`; `2` is `whirl daemon status`'s
answer for a job the supervisor does not have (whirl
`crates/whirl-cli/src/daemon/macos.rs:324-338`). The app reports what the step did, quoting the
command's own line, and it never runs `launchctl`, `systemctl` or `schtasks` itself (ADR 0002, lines
360-361, 507-511).

A `stop` that finds no job loaded is not an error: it prints `not running: <label>` and exits `0`,
so the pair is safe against a daemon that is already down (whirl
`crates/whirl-cli/src/daemon/macos.rs:283-296`). The alternative the app must never offer is
`launchctl kickstart -k gui/<uid>/com.guruor.whirl` (`whirl docs/architecture.md:1584-1585`): it is
the supervisor's action and the person's to run, not the app's.

`Later` leaves the old daemon running, which is a working state: the daemon reads its config at
startup and keeps rotating with the config it has (ADR 0002, measurement 1,
`docs/decisions/0002-frontends-write-config-own-no-daemon.md:55-100`). Nothing breaks by waiting; the
new binary simply is not in use yet.

## 6. Failure and rollback

| failure | what the app does | the rule it follows |
|---|---|---|
| a download fails its checksum | refuses, prints the published and actual digests, installs nothing; the running daemon and the installed app are untouched | `install.sh:250-254`, `install.sh:31-33` |
| the app's prefix needs an authorization the app cannot ask for | does not run the app half, names the command and the reason, changes nothing | `install.sh:386-398`; exit `3` |
| a release is newer but has no archive for this platform | refuses before downloading, naming the platform; nothing is installed | `install.sh:141-156` (macOS-only, arm64-only, macOS 13+); exit `1` |
| a release would step the version backwards | never offered: the check's own version comparison reads a release the build is ahead of as the newest, not as "newer" | `crates/whirl-ui/src/about.rs:191-197`, `crates/whirl-ui/src/about.rs:281-293`, test `crates/whirl-ui/src/about.rs:344-356` |
| the running daemon is older than the installed binaries | not a failure: the restart prompt covers it, and `Later` leaves a working daemon | section 5 |
| the preference file cannot be written | the check still runs for the session; `last_check_at` is not persisted, so the next launch checks again rather than skipping | `crates/whirl-ui/src/about.rs:19-22` (a check is never silently the newest) |

Rollback of an installed update is the installer's own undo: the receipt names exactly what a run
installed and `uninstall.sh` removes those paths and nothing else (`install.sh:16-23`). The design
adds no second undo path, and the design's required `--daemon-only` selector writes the same receipt
lines for the daemon half (`install.sh:467-478`).

## 7. Out of scope for the first version

- **Staged rollout** (a percentage of installs, or a beta channel). One release is offered to
  everyone who checks.
- **Delta or patch updates.** Each update is the full archive, checked against its published
  sha256.
- **Signing and notarization.** The app stays ad-hoc signed and is not notarized
  (`scripts/make-bundle.sh:19-29`); the checksum is a delivery check and not an identity
  (`docs/releases/v0.1.0.md:69-80`). Changing that is its own decision, not this one.
- **A silent update of the app bundle.** It cannot be done: the write into `/Applications` is the
  authorization the app cannot ask for (`install.sh:25-29`), which is D2's whole reason.
- **Updating the app's own login item or the daemon's login unit.** The app's login item is its own
  business; the daemon's unit is whirl's (`docs/milestones.md:63-78`), and this design writes no
  unit (`install.sh:35-42`).
- **A `config get` / `config set` verb, or any daemon verb for updates.** The protocol grows nothing
  here; the config stays the write surface under ADR 0002 (ADR 0002, decision 1, and its "Forbids"
  list at lines 507-511).

## Citations

Every line below is checkable at the path and line given. Rows are grouped by repository, because
the daemon's contract lives in its own.

| claim | source |
|---|---|
| the release endpoint in use | `crates/whirl-ui/src/about.rs:216-221` |
| the check is one anonymous GET with curl | `crates/whirl-ui/src/about.rs:13-18`, `crates/whirl-ui/src/about.rs:229-242` |
| three outcomes; a failure is never "up to date" | `crates/whirl-ui/src/about.rs:156-184`, `crates/whirl-ui/src/about.rs:251-260` |
| a build ahead of a release reads as the newest | `crates/whirl-ui/src/about.rs:191-197`, `crates/whirl-ui/src/about.rs:281-293`, `crates/whirl-ui/src/about.rs:344-356` |
| nothing schedules the check today | `crates/whirl-ui/src/about.rs:24-27`, `crates/whirl-ui/src/about.rs:199-202` |
| the check button's own words | `crates/whirl-ui/src/settings.rs:159-165` |
| the window opens with no check made | `crates/whirl-ui/src/settings.rs:201-203`, `crates/whirl-ui/src/settings.rs:841-843` |
| `--check-update` and its exit codes | `crates/whirl-ui/src/main.rs:405-412`; constants at `crates/whirl-ui/src/dump.rs:39-45` |
| install.sh is re-run, not an updater; reuses or replaces an installed daemon | `install.sh:7-10`,
`install.sh:13-14`, `install.sh:349-354`, `install.sh:417-439` (the row's "reuses, printed, and never
touched" is **unverified at this head**: a daemon at the pinned release or newer is reused, an older
one is replaced) |
| verify before writing; checksum beside every archive | `install.sh:31-33`, `install.sh:241-257`, `install.sh:170-173` |
| the receipt, and its path | `install.sh:16-23`, `install.sh:168` |
| the one write macOS may ask to authorize; exit 3 | `install.sh:25-29`, `install.sh:386-398` |
| quarantine is cleared after the checksum passes | `install.sh:543-553`; `docs/releases/v0.1.0.md:62` |
| archive and `.sha256` are written by the bundler | `scripts/make-bundle.sh:168-175` |
| the app is ad-hoc signed, not notarized | `scripts/make-bundle.sh:19-29`, `scripts/make-bundle.sh:144-164` |
| the checksum is a delivery check, not an identity | `docs/releases/v0.1.0.md:69-80` |
| the app does not own the daemon's lifetime | `docs/milestones.md:63-78`; `whirl docs/architecture.md` §8 "must never" 3 (lines 1865-1871) |
| §8 "must never" 1-9 | `whirl docs/architecture.md:1857-1885` |
| the OS supervisor owns the daemon's lifetime | `whirl docs/architecture.md` §5.1-§5.2 (lines 1551-1585) |
| `launchctl kickstart -k` is the supervisor's action | `whirl docs/architecture.md:1584-1585` |
| `whirl daemon` has five subcommands, the app runs three | `whirl` `crates/whirl-cli/src/daemon.rs:35-47`; `whirl docs/architecture.md` §8 "may rely on" 8 (lines 1838-1845) |
| a frontend may run whirl's own step, never a supervisor command | `whirl docs/decisions/0002-frontends-write-config-own-no-daemon.md:343-392`, `:448-469`, `:550-553`, `:604-606` |
| the `daemon` verb exists; no released CLI lists a `service` verb | `whirl` `crates/whirl-cli/src/daemon.rs:35-47`; `whirl docs/spec/features.md:69-74`; `install.sh:489-494` |
| the daemon's config is read once at startup | `whirl docs/decisions/0002-frontends-write-config-own-no-daemon.md:55-100` |
| the Karabiner-Elements restart pattern | <https://karabiner-elements.pqrs.org/docs/releasenotes/> ("the ability to restart Karabiner-Elements from the menu") |
