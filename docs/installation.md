# Installing whirl and Whirl

Two routes, same outcome: the daemon running, a picture on the desktop, and the tray app talking to
the daemon. Route A is one script you download, read and run. Route B is the same steps typed out.
Neither route needs root and neither asks a question.

Everything below was run on one machine, macOS 26.7 on Apple silicon, on 2026-10-04, and the quotes
are from that run: the daemon from `guruor/whirl` at `development` `aec199e`, the app built by
`scripts/make-bundle.sh` at `c75ab45` (the commit that adds the bundle), the installer at `c1379d5`.
The run used a throwaway home and a folder of test pictures; paths from it are shortened to `~` or to
`<your folder>` below. Nothing else in a quote is changed.

## What you start with

- macOS 13 or later on Apple silicon. The installer checks all three and refuses before writing
  anything: `install: v0.1 supports Apple silicon (arm64) only, and this is x86_64. Nothing was
  installed.`
- A terminal, and network access for the downloads.
- A folder of pictures. whirl reads `.jpg .jpeg .png .heic .webp`; the config it writes asks a local
  folder for 2560x1440 or larger, and the shared floor is 1600x900. A file that fails the filters, or
  whose header it cannot measure, is counted and skipped rather than guessed at from its name; step 6
  says where to read those counts.

Not assumed: Rust, `~/.cargo`, whirl, Whirl.app, a config file, or a running daemon.

## Route A: one command

    curl -fsSLO https://raw.githubusercontent.com/guruor/whirl-ui/v0.1.0/install.sh
    less install.sh
    sh install.sh

Read it first on purpose: it is the same steps as route B, short enough to check, and it is meant to
be read before it runs. Then `sh install.sh`, which with nothing installed prints the block below. The
two `fetching` lines name the two published archives; this run pointed the script at locally built
ones with the overrides under "Route A today", which is the only difference, so those two lines read
`<the daemon archive>` and `<the app archive>` here:

    backend:  whirl v0.1.0 is not installed; it will go into ~/.local/bin
      fetching   <the daemon archive>
      checksum   ok  a6114d4af63485c0700f7d7fd5ba54e52cd95d5e4ada8555aedb6b69b4eda1e9
    frontend: Whirl 0.1.0 will go into /Applications
      fetching   <the app archive>
      checksum   ok  e22ef1ee49f87e4ff027a49c8eeaba579eb8c644532a0c762c0334d0a5944fbb
      installed  ~/.local/bin/whirl
      installed  ~/.local/bin/whirld
      installed  ~/.local/bin/whirl-worker
      unit       whirl v0.1.0 ships no login-item installer yet, so none
                 was written, here or anywhere; start the daemon yourself until it does
      installed  /Applications/Whirl.app
      quarantine cleared: the app is not notarized, so a downloaded copy would
                 otherwise open behind the unidentified-developer dialog

    install: done
      backend    whirl v0.1.0, three binaries in ~/.local/bin
      frontend   Whirl 0.1.0 at /Applications/Whirl.app
      undo       sh uninstall.sh

What it does, in order:

1. Refuses early. Not macOS, not arm64, or older than 13, and it writes nothing.
2. Reuse before download. If `whirld` is on `PATH`, or at `$WHIRL_PREFIX/whirld`, or a loaded login
   item exists, it prints `reusing the daemon already installed: <path>` and touches nothing.
   Otherwise it downloads the daemon archive, checks it, and installs the three binaries into
   `$WHIRL_PREFIX` (default `~/.local/bin`).
3. Writes no login unit. The daemon owns its unit, so the script asks whirl's own installer to write
   one when whirl has that verb. whirl v0.1.0 does not have it, and the script says that in one line
   rather than inventing a unit file.
4. Downloads `Whirl-0.1.0.zip`, checks it, `ditto`s `Whirl.app` into `$WHIRL_UI_PREFIX` (default
   `/Applications`), and clears `com.apple.quarantine` on it, because a downloaded bundle carries
   that flag and would otherwise open behind the unidentified-developer dialog.
5. Prints the undo line. `sh uninstall.sh` stops the app, removes the app and the three binaries, and
   prints the data paths it deliberately leaves alone. The removal table below names those same
   paths one at a time, which is what this run used to put the machine back.

Every archive is checked against the sha256 published beside it *before* anything is unpacked. A
checksum that is missing, empty, not a sha256, or a mismatch refuses the whole install:
`the download does not match its published checksum; nothing was installed`.

What route A does not do: start the daemon, and write config. Those belong to the daemon, and they
are steps 5 and 6 below, identical on both routes.

### Route A today: the half that is missing

Run exactly as published, route A stops at its first download. No `v0.1.0` tag is cut on either
repository yet (`git tag` is empty in both, `gh release list` is empty in both), so neither archive
exists. And the daemon one will arrive without a checksum when it does: whirl's release workflow
requires `dist/` to hold exactly the three platform archives and attaches `dist/*`, so it publishes
no `.sha256`, while `install.sh` asks for `<archive>.sha256` and, in its own words, refuses that
download:

    whirl publishes no checksum at <archive url>; refusing to install an unverified archive

What is missing, precisely: the tag `v0.1.0` on both repositories, and a
`whirl-v0.1.0-macos-arm64.tar.gz.sha256` published beside the daemon archive. (The app half is
already right: this repository's release workflow attaches `dist/*.zip` and `dist/*.zip.sha256`.)

Until that is published, the overrides the script documents for exactly this case
(`WHIRL_ARCHIVE` / `WHIRL_SHA256`, `WHIRL_UI_ARCHIVE` / `WHIRL_UI_SHA256`) point it at archives built
by the same commands the release workflow runs, and both halves install. That is how the run quoted
above was made, and how to check the script before the first release.

## Route B: the same thing, by hand

### 1. Rust

    curl -fsSLO https://sh.rustup.rs
    less rustup-init.sh
    sh rustup-init.sh -y --no-modify-path

- What it does: installs the Rust toolchain into `~/.cargo` and `~/.rustup`.
- What to check: `cargo --version` prints `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`, `rustc --version`
  prints `rustc 1.95.0 (59807616e 2026-04-14)`.
- What it writes: `~/.cargo` and `~/.rustup`, and nothing else with `--no-modify-path`.
- To undo it later: `rustup self uninstall`.

### 2. The daemon

    git clone --branch development https://github.com/guruor/whirl
    cd whirl
    cargo install --path crates/whirl-cli    --locked   # the whirl CLI
    cargo install --path crates/whirld       --locked   # the daemon
    cargo install --path crates/whirl-worker --locked   # the worker

- What it does: builds three binaries from the clone and copies them into `$CARGO_HOME/bin`, which is
  `~/.cargo/bin`. `--locked` builds the versions `Cargo.lock` names.
- Why three: the pipeline is `whirl` (CLI) → `whirld` (daemon) → `whirl-worker` (the process that
  fetches and sets one wallpaper). A release carries all three; two is not a daemon.
- What to check: `whirl help` lists the verbs. Each install ends with a `Finished` line for the
  release profile and an ``Installed package `whirl-cli v0.1.0` (executable `whirl`)`` line.
- What it writes: the three binaries in `~/.cargo/bin`, and `target/` inside the clone. The first
  build is several minutes: the workspace compiles.

### 3. The app

    git clone https://github.com/guruor/whirl-ui
    cd whirl-ui
    git checkout main          # the bundle commit is on main once it merges
    ./scripts/make-bundle.sh

- What it does: builds the tray app and packages it as an app bundle, ad-hoc signed and named for the
  version.
- What to check: the script prints what it built, ending with
  `make-bundle: <the bundle>/Contents/MacOS/whirl-ui is the app; it is an agent app (LSUIElement) and it is ad-hoc signed`,
  and `shasum -a 256 -c dist/Whirl-0.1.0-local.zip.sha256` prints `dist/Whirl-0.1.0-local.zip: OK`.
- What it writes: `dist/Whirl.app`, `dist/Whirl-0.1.0-local.zip`, that archive's `.sha256`, and
  `target/`. With no tag cut the version is `0.1.0-local`; on a `v*` tag the same script writes
  `Whirl-<version>.zip`. The build needs the network the first time: the app depends on `whirl-core`
  at a revision pinned in `Cargo.lock` from the git repository.

### 4. Install the app

    ditto dist/Whirl.app /Applications/Whirl.app

- What it does: copies the bundle into place. No root, no password.
- What to check: `xattr -l /Applications/Whirl.app` prints nothing (a copy you built carries no
  quarantine flag), `codesign -dv --verbose=2 /Applications/Whirl.app` reports `flags=0x2(adhoc)` and
  `Signature=adhoc`, and `open -a /Applications/Whirl.app` puts an icon in the menu bar. Open its
  window (**Settings…** from the menu bar item): the footer shows a green **Connected** dot and the
  version, and the Sources pane holds the sources the daemon reported, not a copy the app keeps.
- What it writes: `/Applications/Whirl.app`.
- If you downloaded the archive instead of building it: clear the quarantine flag first, see
  "What is unsigned" below.

### 5. Start the daemon

    whirld

- What it does: on the first run, writes its config with every key at its default value, then
  listens. It names what it adopted:

      whirld: wrote the default config at ~/Library/Application Support/whirl/config.json
      whirld: config ~/Library/Application Support/whirl/config.json
      whirld: state ~/Library/Application Support/whirl/state cache ~/Library/Caches/whirl
      whirld: backend native
      whirld: listening on ~/Library/Application Support/whirl/whirl.sock
      sweep files=0 bytes=0 removed=0 reclaimed=0 orphans=0 deferred=0

- What to check: `whirl status` prints `daemon_version: whirl 0.1.0`, `protocol: 2`, a `pid` and
  `uptime_s`. If it cannot reach the daemon it says so and exits non-zero.
- What it writes: `config.json` (every key at its default, each with a `_comment_…` line beside it,
  and any key whose name starts with `_` is a comment every whirl parser ignores), `state/`,
  `whirl.sock`, and `~/Library/Caches/whirl`. No log directory was created in either run: what the
  daemon has to say about a rotation, including a picture it skipped, it says in its own output.
- The config it writes has two sources: a local folder (default paths `~/Pictures/Wallpapers` and
  `/Volumes/Media/walls`, 2560x1440 floor) and a Wallhaven search (weight 3, no key needed for sfw
  searches). `weight: 0` disables a source without deleting it.

### 6. Point it at your folder, and put a picture on the desktop

Edit the local source's `paths` in `~/Library/Application Support/whirl/config.json` to the folder you
want (the app's Sources pane writes the same key), then start the daemon again so it reads the file.
Ask for a rotation:

    $ whirl next
    queued
    set: 8cb8eb16…ce95 pictures:7b0c5ed6…b845 source <your folder>/0P9h0fM.jpg

- `queued` means the request is on the daemon's queue. `set:` is the daemon's own record of what it
  put on the desktop: the digest of the image, the digest of its origin key, the source id, and the
  path it read.
- What to check, all four:
  - the desktop changes. It is not instant: the screen showed the new picture 10 s after `set:` in
    this run. Give it a moment before deciding it failed.
  - `whirl history` lists the entry, and `whirl status`'s `rotation_count` went up.
  - `whirl status` reports `anchor_verified: 1`, which is the daemon reading the platform's current
    image back and finding the picture it set, rather than trusting that the setter worked.
  - `whirl config check` reports the folder: `source: pictures local weight=1 enabled=1 last=-
    candidates=8 admitted=5 rejected_resolution=3 rejected_ratio=0 rejected_size=0 rejected_type=0
    rejected_dedupe=0`. Candidates are the files the walk measured; the counters say why the rest did
    not make it. Two things put `candidates` below the number of files in the folder, and both are
    counted rather than silent: files the filters reject (3 here, below the source's 2560x1440) and
    files whose header could not be measured, reported as
    `warning: source pictures: entries skipped: 2 unreadable, 0 symlink, 0 revisited`. That second one
    is a known issue in whirl's own release notes, not something your folder did wrong: a local source
    measures a file by reading a 1024-byte head window, so a readable JPEG whose frame header starts
    past that window is dropped. If `candidates=` is lower than the folder holds, suspect it first.

### The two routes end in the same place

Run from a machine with no whirl, no Whirl.app and no config: route B and route A (with the archives
pointed at locally built files, as above) each end with the daemon answering, a rotation recorded and
on the desktop, and the tray app running against it with a green **Connected** dot. The difference is
only who types the commands.

## What this changed on my machine, and how to undo each part

| What | Where | To remove it |
| --- | --- | --- |
| the three daemon binaries | `~/.local/bin/whirl`, `whirld`, `whirl-worker` | `rm -f ~/.local/bin/whirl ~/.local/bin/whirld ~/.local/bin/whirl-worker` |
| the daemon's login unit | nowhere: whirl v0.1.0 ships no installer for it and neither route writes one | nothing to remove |
| the tray app | `/Applications/Whirl.app` | `rm -rf /Applications/Whirl.app` |
| config | `~/Library/Application Support/whirl/config.json` | `rm -f …/config.json` |
| state: history, favourites, locks | `~/Library/Application Support/whirl/state/` | `rm -rf …/state` |
| socket | `~/Library/Application Support/whirl/whirl.sock` | `rm -f …/whirl.sock` |
| image cache | `~/Library/Caches/whirl/` | `rm -rf ~/Library/Caches/whirl` |
| log, if the daemon ever writes one | `~/Library/Logs/whirl/` | `rm -rf ~/Library/Logs/whirl` |
| Rust, route B only | `~/.cargo`, `~/.rustup` | `rustup self uninstall` |
| the clones, route B only | `whirl/`, `whirl-ui/` | `rm -rf` the two directories |

Each `…` in the table stands for `~/Library/Application Support/whirl`. The run behind this document
removed these paths one at a time rather than in a single command; what is left either way is nothing,
and nothing outside them is touched.

Route A's own undo is one command, and it prints the paths it leaves behind:

    sh uninstall.sh

Deleting `config.json`, `state/` and the cache is what removes your rotation history and favourites;
nothing else on the machine knows about them. Neither route replaces an existing `config.json`: the
daemon writes one only when there is none, and it reads yours as it stands.

## What is unsigned, and what macOS says

The archive comes with a checksum. `shasum -a 256` of the archive equals the published `.sha256`,
which proves the bytes you have are the bytes that were published: a truncated or altered download
fails the check before anything is unpacked, and the install stops. It proves nothing about who built
them. The digest is published next to the archive over the same connection, so whoever can replace
one can replace the other: it is a delivery check, not an identity.

The app is ad-hoc signed, so nothing on the machine verifies who built it:

    $ codesign -dv --verbose=2 /Applications/Whirl.app
    Identifier=com.guruor.whirl-ui
    Format=app bundle with Mach-O thin (arm64)
    CodeDirectory v=20400 size=17580 flags=0x2(adhoc) hashes=543+3 location=embedded
    Signature=adhoc

    $ spctl -a -vv /Applications/Whirl.app
    /Applications/Whirl.app: rejected        # exit 3

That is Gatekeeper refusing the bundle, and it does so whether or not the quarantine flag is present:
the ad-hoc signature is what fails assessment. whirl releases are not notarized, and the build script
says so rather than implying otherwise.

The flag is what decides whether you meet that refusal as a dialog. macOS sets
`com.apple.quarantine` on anything it can tell arrived from the network, a browser download included.
Launch a quarantined copy and Gatekeeper refuses it: the app does not open, and the dialog offers to
move it to the Bin or to cancel the launch. Apple's wording in that dialog moves between releases;
what does not move is the refusal and the way through it, and the dialog's exact text was not observed
for this bundle. That way through is to launch it once and refuse it, then allow it under **System
Settings → Privacy & Security → Open Anyway**, or to clear the flag ahead of time:

    xattr -l /Applications/Whirl.app                      # shows com.apple.quarantine, if it is there
    xattr -dr com.apple.quarantine /Applications/Whirl.app
    xattr -l /Applications/Whirl.app                      # nothing, and the app launches

A copy you built yourself carries no quarantine flag at all, so route B step 4 needs no such step, and
route A clears the flag for you after verifying the checksum. A bundle you unpacked by hand from a
downloaded archive does need it.

## Reference: commits this describes

| What | Where |
| --- | --- |
| the app bundle and `scripts/make-bundle.sh` | `c75ab45` |
| the one-command installer and its undo | `c1379d5` |
| the daemon built here | `guruor/whirl` `development` `aec199e` |
