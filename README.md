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

## Layout

    crates/whirlui-client/   the protocol client (library)
    crates/whirl-ui/         the tray app (binary)

## Build and run

Install Rust with [rustup](https://rustup.rs). The toolchain is pinned in `rust-toolchain.toml`, so
rustup installs the right compiler and components on first use; no other setup step is needed.

    cargo build --workspace                                  # build both crates
    cargo run                                                # build and run the tray app
    cargo test --workspace                                   # run the test suite
    cargo fmt --all -- --check                               # the formatting gate
    cargo clippy --workspace --all-targets -- -D warnings    # the lint gate, warnings denied

The app talks to a running daemon, and it will not start one for you: build and run `whirld` from the
[whirl repository](https://github.com/guruor/whirl) first. With no daemon reachable it has nothing to
show and says so.

If a version manager injects `RUSTUP_TOOLCHAIN`, the file above stops winning and a different compiler
is used. Run cargo without that variable and with rustup first on `PATH` to restore the pin:

    env -u RUSTUP_TOOLCHAIN PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace

## License

There is no `LICENSE` file, which means all rights are reserved. That is an open decision rather than
an oversight: the intent is a reference implementation other people can reuse, and no license is the
thing that currently prevents it.
