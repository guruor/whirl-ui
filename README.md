# whirl-ui

A lightweight tray frontend for the [whirl](https://github.com/guruor/whirl) wallpaper daemon.

The daemon is headless. It owns the wallpaper, the rotation, the sources and every piece of state, and
it exposes no user interface at all: its interface is the socket protocol on the other side of which
it listens. This app is an ordinary client of that protocol. It connects, reads `status`, follows
`subscribe` for change notification and prints what the daemon reports, which means it writes no state
file, calls no platform setter, never starts, stops or restarts the daemon, and holds no copy of the
configuration it did not read back through the protocol. The whole of its obligation to the daemon is
whirl's [`docs/architecture.md` section
8](https://github.com/guruor/whirl/blob/main/docs/architecture.md#8-frontend-contract), "Frontend
contract", which lists what a frontend may rely on and what it must never do.

The tray is **macOS first**. macOS-only code is gated behind `#[cfg(target_os = "macos")]` so that the
Linux and Windows builds compile the same workspace and stay green while those platforms are
unfinished.

## Install

One command installs the daemon and the app, and reuses a daemon that is already there. It downloads
two archives, checks each against the sha256 published beside it, and writes nothing until both checks
pass. It needs no root, no password and no answer. Read it first:

    curl -fsSLO https://raw.githubusercontent.com/guruor/whirl-ui/v0.1.0/install.sh
    less install.sh
    sh install.sh

The same thing as one line, for anyone who has read it and trusts it:

    curl -fsSL https://raw.githubusercontent.com/guruor/whirl-ui/v0.1.0/install.sh | sh

The daemon goes into `~/.local/bin` and `Whirl.app` into `/Applications`. The script prints what it
installed, what it skipped and why, and the one command that undoes it. `uninstall.sh` removes exactly
what was installed and names what it deliberately leaves alone: the daemon's config, state, cache and
log. The daemon's own login unit is whirl's to install, so this script never writes one.

## Layout

    crates/whirlui-client/   the protocol client (library)
    crates/whirl-ui/         the tray app (binary)

## Build and run

Install Rust with [rustup](https://rustup.rs). The toolchain is pinned in `rust-toolchain.toml`, so
rustup installs the right compiler and components on first use; no other setup step is needed.

    cargo build --workspace                                  # build both crates
    cargo run                                                # build and run the app
    cargo test --workspace                                   # run the test suite
    cargo fmt --all -- --check                               # the formatting gate
    cargo clippy --workspace --all-targets -- -D warnings    # the lint gate, warnings denied

The app talks to a running daemon, and it will not start one for you: build and run `whirld` from the
[whirl repository](https://github.com/guruor/whirl) first. With no daemon reachable it has nothing to
show and says so.

## Modes

With no arguments the app starts its menu bar item (macOS), whose `Settings…` row opens the settings
window. A window cannot be asserted by a test, so every question it answers is also answerable from a
terminal, in the daemon's own words rather than in a format invented here:

    cargo run -- --dump-status         # the daemon's status, key by key
    cargo run -- --dump-sources        # each source's enabled state and the reason it has one
    cargo run -- --dump-config-check   # the effective plan the daemon adopted
    cargo run -- --dump-settings       # what the settings window shows, as text
    cargo run -- --screenshot shot.png # run the window, write it to a PNG, and exit
    cargo run -- --login-item status   # the app's own login item, as macOS reports it

`--login-item` takes `status`, `register` or `unregister`, and every verb ends by printing the status, so
a before and an after are the same two lines. It is about the *app's own bundle*, which is what macOS
registers a login item for: from `Whirl.app` it registers that app, and from a bare binary it refuses and
names the executable it looked at. `scripts/make-bundle.sh` is what writes `Whirl.app`.

The dump modes exit 0 on success, 1 if the daemon refused, 2 if it is unreachable and 3 if the command
line cannot work. `--dump-settings` prints its three panes with no daemon too, where each one renders
the reason it has nothing to show: a pane that showed an empty list would be saying something untrue.

`--screenshot` writes the window's own pixels rather than the screen's, so the file carries the window
and nothing else that happened to be on the machine.

If a version manager injects `RUSTUP_TOOLCHAIN`, the file above stops winning and a different compiler
is used. Run cargo without that variable and with rustup first on `PATH` to restore the pin:

    env -u RUSTUP_TOOLCHAIN PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace

## Docs

    docs/milestones.md    M1 to M4, their exit criteria, and what is deliberately out of scope
    docs/design.md        the visual language, and what each element the design draws would cost in config
    scripts/make-bundle.sh  Whirl.app, ad-hoc signed, and the archive a release carries

The release bundle is built by hand and by CI on a tag, from the same script:
`scripts/make-bundle.sh [version]`. There is no Developer ID and no notarization behind it, so a
downloaded copy is quarantined and its first launch needs the step named beside the download.

## License

MIT. See [LICENSE](LICENSE).
