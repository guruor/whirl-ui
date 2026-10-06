# Contributing to whirl-ui

The front door is [`README.md`](README.md). This file is the development side of the repository: the
toolchain, the checks a pull request must pass, the verbs that make the window's behaviour checkable,
the two bundle scripts, and the release path.

## What left the README

The README became the product page, so its development material moved here. Every old section and its
new home, so nothing was lost in the rewrite:

| README section | now |
| --- | --- |
| Layout | this file, Layout |
| Build and run | this file, Toolchain and The checks before a pull request |
| Modes | this file, Verify verbs |
| Signing, and the one approval | this file, Signing and the bundle |
| the release-bundle paragraph under Docs | this file, The release path |
| the `RUSTUP_TOOLCHAIN` paragraph | this file, Toolchain |
| Install | stayed in README.md |
| Docs | stayed in README.md, as the link list |
| License | stayed in README.md (as Licence) |

## Layout

    crates/whirlui-client/   the protocol client (library)
    crates/whirl-ui/         the tray app (binary)

## Toolchain

Install Rust with [rustup](https://rustup.rs). The toolchain is pinned in `rust-toolchain.toml`:
channel `1.95.0` with the `rustfmt` and `clippy` components, so `rustup show` installs the compiler and
both components before anything runs, and no other setup step is needed. The pin exists rather than
tracking `stable` because rustfmt output changes between releases, which turns formatting into a
"wrong on your machine" review topic. The pinned version is 1.95.0 rather than the 1.94.0 the protocol
client alone needs: the window is `eframe 0.36.2`, whose own MSRV is 1.95.

If a version manager injects `RUSTUP_TOOLCHAIN`, the file above stops winning and a different compiler
is used. Run cargo without that variable and with rustup first on `PATH` to restore the pin:

    env -u RUSTUP_TOOLCHAIN PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace

With the pin in place:

    $ cargo --version
    cargo 1.95.0 (f2d3ce0bd 2026-03-21)
    $ rustc --version
    rustc 1.95.0 (59807616e 2026-04-14)

## The checks before a pull request

Three commands, run from the workspace root:

    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

`.github/workflows/ci.yml` runs them on every push to `main` and every pull request: `fmt` on
`ubuntu-latest`, and `clippy` and `test` on `ubuntu-latest`, `macos-latest` and `windows-latest`. The
tray is macOS-first: macOS-only code is behind `#[cfg(target_os = "macos")]`, so the Linux and Windows
legs compile the same workspace and stay green while those platforms are unfinished.

Clippy is gated per target in two places, and both allowances are load-bearing.
`crates/whirl-ui/src/login_item.rs` puts an `allow(dead_code, reason = ...)` under
`cfg_attr(not(target_os = "macos"), ...)`, on the `State` enum and again on `State::of`: the four
variants are constructed only through a function that reads macOS's `SMAppService` status numbers, and
a use inside `#[cfg(test)]` does not count in a non-test build. Without the allowance the Linux and
Windows clippy legs fail on dead code; on macOS neither attribute fires.

The one thing the suite cannot assert is the menu bar launch itself, because it needs a window server.
A maintainer checks it on a Mac:

    scripts/check-launch-window.sh    # fails if the app shows a window with no dialog open

The app talks to a running daemon and will not start one for you: build and run `whirld` from the
[whirl repository](https://github.com/guruor/whirl) first. With no daemon reachable the app has nothing
to show and says so.

Run on the pinned toolchain, in this workspace:

    $ cargo fmt --all -- --check
    fmt exit=0
    $ cargo clippy --workspace --all-targets -- -D warnings
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.07s
        clippy exit=0
    $ cargo test --workspace
        running 100 tests
        test result: ok. 99 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
        ... seven more suites, every one ok ...
        (the eight suites together: 143 passed, 0 failed, 1 ignored)

## Verify verbs

A window cannot be asserted by a test, so every question the settings window answers is also answerable
from a terminal, in the daemon's own words rather than in a format invented here. Paths in the outputs
below are shortened: the run used a throwaway home and a workspace checkout.

| verb | what it is evidence for |
| --- | --- |
| `--dump-status` | the daemon's status, key by key |
| `--dump-sources` | each source's enabled state and the reason it has one |
| `--dump-config-check` | the effective plan the daemon adopted |
| `--dump-settings` | what the settings window shows, as text |
| `--screenshot <path> [state]` | the window's own pixels rather than the screen's |
| `--check-update` | the About pane's release check, one line |
| `--login-item status\|register\|unregister` | the app's own login item, as macOS reports it |
| `--menu-dump` | the tray menu's rows, as text |

The dump modes exit 0 on success, 1 if the daemon refused, 2 if it is unreachable and 3 if the command
line cannot work. `--dump-settings` prints its panes with no daemon too, where each one renders the
reason it has nothing to show: a pane that showed an empty list would be saying something untrue.

    $ whirl-ui --dump-sources
    count: 1
    source: pictures local weight=1 enabled=1 last=- reason=-

    $ whirl-ui --dump-config-check
    queued
    source: pictures local weight=1 enabled=0 last=- reason=sources[id=pictures].paths (compiled default): no configured path can be read: <home>/Pictures/Wallpapers: No such file or directory (os error 2)
    plan: schedule.interval_seconds=1800 schedule.worker_deadline_seconds=300 startup.enabled=1 startup.mode=last startup.respect_manual=1 display.mode=all display.mode_effective=all min_width=1600 min_height=900 filters.max_bytes=41943040 filters.ratio_tolerance=0.02 filters.target_ratio=- state.history_entries=50 dedupe.recent_entries=50 cache.root=- cache.max_bytes=2147483648 cache.max_files=500 cache.grace_seconds=600 cache.orphan_grace_seconds=300 backend=noop sources=1

    $ whirl-ui --dump-status
    daemon_version: whirl 0.1.0
    protocol: 2
    platform: macos
    pid: <pid>
    seq: 1
    uptime_s: 254
    rss_kb: 2176
    paused: 0
    rotating: 0
    rotation_count: 0
    interval_s: 1800
    (the dump continues with the cache, state, display and source keys, the
    stable key set of whirl's docs/architecture.md 2.10)

    $ whirl-ui --menu-dump
    no image set yet
    Next
    Previous
    Pause
    Favourite
    ---
    Settings…
    Quit

    $ whirl-ui --check-update
    this is the newest version published, 0.1.0

`--screenshot` writes the window's own pixels rather than the screen's, so the file carries the window
and nothing else that happened to be on the machine. It takes any of the states `sources`, `rotation`,
`helper`, `control-panel`, `about`, `chooser`, `key`, `rejected`, `words`, `check-newer`,
`check-newest`, `check-failed`:

    $ whirl-ui --screenshot <path> sources
    whirl-ui: wrote the settings window to <path>

`--login-item` is about the *app's own bundle*, which is what macOS registers a login item for: from
`Whirl.app` it registers that app, and from a bare binary it refuses and names the executable it looked
at. Every verb ends by printing the status, so a before and an after are the same two lines.

    $ whirl-ui --login-item status
    whirl-ui: <workspace>/target/release/whirl-ui is not inside an app bundle, so it has no login item
    of its own; a login item is a registration of the app's bundle, and Whirl.app is the bundle this
    app ships as (scripts/make-bundle.sh writes one)

## Signing and the bundle

`scripts/make-signing-identity.sh [create|status|delete]` creates one self-signed code-signing
certificate in the login keychain, once, and is idempotent; nothing in it needs sudo, a password or an
answer. The private key is generated by the script, stays in the keychain, and is never written into
this repository or printed.

    $ scripts/make-signing-identity.sh status
    signing-identity: Whirl Local Signing is in ~/Library/Keychains/login.keychain-db; builds are signed with it

`scripts/make-bundle.sh [version]` builds the app and packages it as `Whirl.app`, writing
`dist/Whirl.app`, `dist/Whirl-<version>.zip` and that archive's `.sha256`. It signs with the identity
when one is there and ad-hoc when it is not, which is the case on a release runner and in a fresh
checkout, and it says which of the two it used.

**A build with no identity is ad-hoc signed, which is why a downloaded copy needs one approval.** Signed
ad-hoc (`codesign --sign -`), the *designated requirement* macOS remembers a launch approval against
is the hash of the binary, so two builds of one source are two different apps and an approval given to
the first says nothing about the second. The certificate replaces the ad-hoc `-` and makes the
requirement the bundle identifier plus the certificate, so a rebuild reuses the approval. Whirl is not
notarized either way: `spctl -a -vv` refuses the bundle however it is signed, and the download notes
carry the first-launch step a quarantined copy needs.

The script's own header carries the measurements behind this: the two designated requirements, and the
difference between `delete` and `security delete-certificate`. `WHIRL_SIGNING_KEYCHAIN=<path>` points
it at another keychain, for a throwaway one in a test.

    $ scripts/make-bundle.sh      # the keychain path is shortened here; the script prints the full one
    ...
    make-bundle: dist/Whirl.app/Contents/MacOS/whirl-ui is the app; it is an agent app (LSUIElement) and it is signed with the local signing identity (~/Library/Keychains/login.keychain-db)
    make-bundle: dist/Whirl-0.1.0.zip
    6787cb0c76dff89c2ddd2008ec9e583cf17ec6d1f23725072354efffd4c64553  Whirl-0.1.0.zip
    make-bundle: spctl -a -vv refuses this bundle, because a self-signed or ad-hoc app is not notarized

## The release path

A tag is the release. Pushing a `v*` tag runs `.github/workflows/release.yml`, which builds the bundle
from that tag and attaches `dist/*.zip` and `dist/*.zip.sha256` to a GitHub release for it.

The workflow's jobs run in order. `guard` fails unless the tagged commit is an ancestor of
`origin/main`; `pin` fails unless the `install.sh` at that tag installs that release; `bundle` names
both of them in `needs:`, so a tag pushed anywhere else, or a script that points at another release,
builds nothing and publishes nothing. Ancestry covers both legitimate cases, a tag at `main`'s tip and
`main` having moved ahead of the tag since it was cut.

Cutting a release, in order: bump `WHIRL_UI_VERSION` in `install.sh` to the new version, bump
`version` in `Cargo.toml` and the `whirl-ui` and `whirlui-client` entries in `Cargo.lock`, write
`docs/releases/<version>.md`, land all of it on `main`, then push the tag on `main`'s tip.
`WHIRL_VERSION` is the daemon's pin and moves only when the daemon has released: the two repositories
are released independently, so it normally trails the tag, and the `pin` job resolves it rather than
comparing it. Both defaults are hand-kept, which is why that job exists. v0.2.1 was tagged with both
still reading 0.2.0, and the route its notes document installed 0.2.0. v0.2.2 was first cut with the
daemon pin bumped to match the tag, which named a daemon release that does not exist.

The release's own words are `docs/releases/<version>.md`; the GitHub release page points at that file
rather than carrying prose of its own, and `docs/releases/` is the list of them.

## A first pull request

- **Branch.** `<type>/<slug>`, as the merged branches show: `feat/about-pane`,
  `docs/reporting-research`, `chore/release-guard-and-notes`.
- **Commits.** One change per commit with an imperative subject, and a body that says the problem, the
  fix and the evidence: the command you ran and what it printed. Cite what a reader can check (the
  commit, the test, the measurement). This repository is public, so a commit subject, a branch name, a
  code comment and anything the code prints name no private tracker, no internal tool and no home
  directory.
- **Before opening it.** Run the three checks above and paste their output into the pull request.
