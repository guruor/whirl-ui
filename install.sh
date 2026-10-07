#!/bin/sh
#
# install.sh: whirl and Whirl, in one command.
#
# Installs two things and reports on both:
#
#   whirl (the daemon)    the three release binaries (whirl, whirld, whirl-worker)
#                         into $WHIRL_PREFIX, default ~/.local/bin. A daemon already
#                         installed is reused only when it is the same release or newer;
#                         its version is read from `whirl --version`. A CLI that answers
#                         no version predates that flag, so it is replaced, never reused.
#   Whirl (the tray app)  Whirl.app into $WHIRL_UI_PREFIX, default /Applications.
#
# The app and the daemon are installed as one release, and an older daemon is replaced
# rather than reused, so the pair this script leaves behind can work together.
#
# What this run is paired with is written to a receipt, $WHIRL_UI_RECEIPT, default
# ~/Library/Application Support/whirl-ui/install.receipt, and uninstall.sh removes the
# paths that receipt names and nothing else. The daemon is part of that pair: a daemon
# this run installed, and one it reused, are both named by their `binary` lines, so
# uninstall.sh removes the pair together. An app that was already here is reused,
# printed as not touched, and never entered in the receipt, so uninstall.sh leaves it
# alone. A run that neither installed nor reused anything writes no receipt and leaves
# nothing to undo.
#
# The app's prefix is /Applications by default, which is outside your home. That one
# write is the only thing here macOS may ask you to authorize: the script prints what
# may be asked and why, never drives or answers that dialog, and never uses root. When
# the destination needs an authorization this script cannot ask for, it says what to
# do and exits 3 with nothing installed.
#
# Every archive is checked against the sha256 published beside it *before* anything is
# unpacked, so a tampered or truncated download installs nothing. Nothing needs root, a
# password or an answer: one command, one exit code.
#
# This script installs the binaries and nothing else, and where startup is set is not here:
# the app sets it on first run, and, without the app, `whirl daemon install` does. No login
# unit is written here and `launchctl` is never called. A headless or provisioning run can
# ask for it, and this script then runs the daemon's own command and reports what it said:
#
#   WHIRL_UNIT=install   ask whirl's own command to install its login unit
#
# The daemon owns its own unit, so `whirl daemon install` is the only thing that writes it
# (docs/milestones.md, M3 criterion 3). uninstall.sh asks the same command to take the unit
# away before it removes the binary the unit runs, so no login item is left pointing at a
# program that is gone.
#
# Read this file before you run it; it is short on purpose, and the documented route is
# "download, read, run". Piping it into a shell is offered beside that route, never
# instead of it:
#
#   curl -fsSLO https://raw.githubusercontent.com/guruor/whirl-ui/v0.2.4/install.sh
#   less install.sh
#   sh install.sh
#
# Every override below is optional and defaults to the published release. They exist so
# the same script can install a build that is not the published one, and so an
# acceptance run can install into a throwaway prefix with no network.
#
#   WHIRL_UI_VERSION   0.2.4               the app release; the tag is v0.2.4
#   WHIRL_UI_BASE      the release download where Whirl-<version>.zip lives
#   WHIRL_UI_ARCHIVE   <path or URL>       use this archive, skip the download
#   WHIRL_UI_SHA256    <path or URL>       its checksum; default <archive>.sha256
#   WHIRL_UI_PREFIX    /Applications       where Whirl.app is installed
#   WHIRL_UI_RECEIPT   <path>              where the record of what was installed is
#                                          kept, default ~/Library/Application
#                                          Support/whirl-ui/install.receipt
#   WHIRL_VERSION      v0.2.1              the daemon release
#   WHIRL_BASE         the release download where the daemon archive lives
#   WHIRL_ARCHIVE      <path or URL>       use this archive, skip the download
#   WHIRL_SHA256       <path or URL>       its checksum; default <archive>.sha256
#   WHIRL_PREFIX       ~/.local/bin        where the three daemon binaries go
#   WHIRL_UNIT         unset                set to `install` to ask whirl's own command
#                                          to install its login unit; unset writes none
#
# Exit codes: 0 installed or reused, as reported; 1 refused (platform, download,
# checksum, unwritable destination); 2 no arguments are taken; 3 the app's prefix needs
# an authorization this script cannot ask for, and nothing was installed.

set -eu
umask 022

usage() {
    cat <<'USAGE'
install.sh: whirl and Whirl, in one command.

  sh install.sh            install the daemon and the app; reuse a daemon already there
  sh install.sh --help     this text

It downloads the whirl daemon archive and the Whirl.app archive, checks each against
the sha256 published beside it, and installs the binaries. It never writes the daemon's
login unit: startup is set by the app on first run, or, without the app, by
`whirl daemon install`. WHIRL_UNIT=install asks whirl's own command for that unit.

The app and the daemon are installed as one release: a daemon already here is reused
only when it is the same release or newer, and an older daemon is replaced by the
pinned one. A daemon whose CLI answers no version predates the version flag, so it
counts as older and is replaced too.

What this run is paired with is recorded in a receipt (WHIRL_UI_RECEIPT, default
~/Library/Application Support/whirl-ui/install.receipt), and uninstall.sh removes the
paths that receipt names and nothing else, the daemon's binaries included. An app that
was already here is reused, printed as not touched, and left alone by both scripts.

Whirl.app goes into /Applications, outside your home. That is the one write macOS may
ask you to authorize: the script says what may be asked and never answers the dialog
for you. No root and no password are used.

No questions are asked.

Exit codes: 0 ok; 1 refused (platform, download, checksum, unwritable destination);
2 an argument was given that is not --help; 3 the app's destination needs an
authorization this script cannot ask for, and nothing was installed.

Overrides (optional; the defaults are the published release):
  WHIRL_UI_VERSION   0.2.4
  WHIRL_UI_ARCHIVE   path or URL of Whirl-<version>.zip
  WHIRL_UI_SHA256    path or URL of its checksum
  WHIRL_UI_PREFIX    /Applications
  WHIRL_UI_RECEIPT   where the record of what was installed is kept
  WHIRL_VERSION      v0.2.1
  WHIRL_ARCHIVE      path or URL of the daemon archive
  WHIRL_SHA256       path or URL of its checksum
  WHIRL_PREFIX       ~/.local/bin
  WHIRL_UNIT         unset: no login unit. `install` runs `whirl daemon install`

Undo: sh uninstall.sh
USAGE
}

case "${1-}" in
    -h | --help)
        usage
        exit 0
        ;;
    '') ;;
    *)
        printf 'install: no arguments are taken (try --help)\n' >&2
        exit 2
        ;;
esac

# ---------------------------------------------------------------------------
# the platform, and whether v0.1 supports it
#
# A refusal that names the reason beats a partial install, so this runs first and
# exits before anything is downloaded.
# ---------------------------------------------------------------------------

if [ "$(uname -s)" != Darwin ]; then
    printf 'install: v0.1 supports macOS only, and this is %s. Nothing was installed.\n' "$(uname -s)" >&2
    exit 1
fi
if [ "$(uname -m)" != arm64 ]; then
    printf 'install: v0.1 supports Apple silicon (arm64) only, and this is %s. Nothing was installed.\n' "$(uname -m)" >&2
    exit 1
fi
macos_version=$(sw_vers -productVersion)
case "$macos_version" in
    1[3-9].* | 2[0-9].*) ;;
    *)
        printf 'install: Whirl.app needs macOS 13 or later, and this is %s. Nothing was installed.\n' "$macos_version" >&2
        exit 1
        ;;
esac

# ---------------------------------------------------------------------------
# the defaults, and the overrides
# ---------------------------------------------------------------------------

WHIRL_UI_VERSION=${WHIRL_UI_VERSION:-0.2.4}
WHIRL_VERSION=${WHIRL_VERSION:-v0.2.1}

ui_prefix=${WHIRL_UI_PREFIX:-/Applications}
daemon_prefix=${WHIRL_PREFIX:-$HOME/.local/bin}
app="$ui_prefix/Whirl.app"
receipt=${WHIRL_UI_RECEIPT:-$HOME/Library/Application Support/whirl-ui/install.receipt}

ui_archive=${WHIRL_UI_ARCHIVE:-${WHIRL_UI_BASE:-https://github.com/guruor/whirl-ui/releases/download/v$WHIRL_UI_VERSION}/Whirl-$WHIRL_UI_VERSION.zip}
ui_sha=${WHIRL_UI_SHA256:-$ui_archive.sha256}
daemon_archive=${WHIRL_ARCHIVE:-${WHIRL_BASE:-https://github.com/guruor/whirl/releases/download/$WHIRL_VERSION}/whirl-$WHIRL_VERSION-macos-arm64.tar.gz}
daemon_sha=${WHIRL_SHA256:-$daemon_archive.sha256}

# Is the app's destination outside your home? /Applications is, and a write there is the
# one step in this script macOS may put its authorization dialog in front of.
case "$ui_prefix" in
    "$HOME"/*) ui_system_location=no ;;
    *) ui_system_location=yes ;;
esac

# Would install.sh have to create the prefixes? Asked before anything is written, so the
# receipt can say which directories are this install's to remove again.
daemon_prefix_created=no
[ -d "$daemon_prefix" ] || daemon_prefix_created=yes
ui_prefix_created=no
[ -d "$ui_prefix" ] || ui_prefix_created=yes

work=$(mktemp -d "${TMPDIR:-/tmp}/whirl-ui-install.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

note() { printf '%s\n' "$*"; }
refuse() {
    printf 'install: %s\n' "$*" >&2
    exit 1
}

# writable_dir <path>: is this directory, or the nearest parent of it that exists,
# writable by this account? Asking first is what keeps that refusal from arriving half
# way through an install.
writable_dir() {
    dir=$1
    while [ ! -e "$dir" ]; do
        parent=$(dirname "$dir")
        [ "$parent" != "$dir" ] || break
        dir=$parent
    done
    [ -w "$dir" ]
}

# record <kind> <value>: one line in the receipt, once. Written as each artefact lands,
# so a run that stops half way still records exactly what it installed, and uninstall.sh
# never has to guess from a prefix what belongs to this install.
record() {
    if [ ! -f "$receipt" ]; then
        mkdir -p "$(dirname "$receipt")"
        {
            printf '# whirl-ui install receipt. Written by install.sh, read by uninstall.sh.\n'
            printf '# uninstall.sh removes exactly the paths named below, and nothing else.\n'
        } >> "$receipt"
    fi
    line="$1 $2"
    grep -Fqx -- "$line" "$receipt" 2>/dev/null || printf '%s\n' "$line" >> "$receipt"
}

# fetch <source> <dest>. The source is an https URL, a file:// URL or a plain path, and
# all three go through curl, so the download path under test is the same one a real
# install uses.
fetch() {
    case "$1" in
        http://* | https://* | file://*) src=$1 ;;
        /*) src="file://$1" ;;
        *) src="file://$(pwd)/$1" ;;
    esac
    curl -fsSL --retry 3 --connect-timeout 15 -o "$2" "$src" >/dev/null
}

# verify <archive> <checksum-file> [label]. The archive's sha256 must equal the first
# field of the checksum file. The file's own name column is ignored, so an archive that
# was renamed still verifies; only the bytes are compared.
verify() {
    expected=$(awk 'NR==1 { print $1 }' "$2")
    [ -n "$expected" ] || refuse "the checksum file $2 is empty; refusing to install ${3:-$1} unverified"
    case "$expected" in
        *[!0-9a-fA-F]*)
            refuse "the checksum file $2 does not begin with a sha256; refusing to install ${3:-$1} unverified"
            ;;
    esac
    actual=$(shasum -a 256 "$1" | awk '{ print $1 }')
    if [ "$expected" != "$actual" ]; then
        printf 'install: REFUSED: %s\n' "${3:-$1}" >&2
        printf '  published  %s\n' "$expected" >&2
        printf '  actual     %s\n' "$actual" >&2
        refuse "the download does not match its published checksum; nothing was installed"
    fi
    note "  checksum   ok  $actual"
}

# version_ge <a> <b>: is version a at least version b? Both are dotted numbers, compared
# field by field, so 0.2 and 0.2.0 are equal and 0.10 is newer than 0.9. A missing field
# is 0 and a field that is not a number is 0 too, so a version this script cannot parse
# never reads as newer than the pinned one.
version_ge() {
    awk -v a="$1" -v b="$2" 'BEGIN {
        n = split(a, x, "."); m = split(b, y, ".");
        for (i = 1; i <= n || i <= m; i++) {
            xa = (i <= n ? x[i] + 0 : 0);
            ya = (i <= m ? y[i] + 0 : 0);
            if (xa > ya) exit 0;
            if (xa < ya) exit 1;
        }
        exit 0;
    }'
}

# daemon_binary <name>: the installed path of one daemon binary, from wherever the daemon
# was found. The three are installed side by side, so the found binary's own directory is
# asked first, then PATH, then the prefix. Nothing is printed when the path cannot be
# named, which is the case when only the login item was found.
daemon_binary() {
    name=$1
    case "$daemon_found" in
        /*)
            dir=$(dirname "$daemon_found")
            if [ -x "$dir/$name" ]; then
                printf '%s\n' "$dir/$name"
                return 0
            fi
            ;;
    esac
    found=$(command -v "$name" 2>/dev/null || true)
    if [ -n "$found" ]; then
        printf '%s\n' "$found"
        return 0
    fi
    if [ -x "$daemon_prefix/$name" ]; then
        printf '%s\n' "$daemon_prefix/$name"
    fi
    return 0
}

# ---------------------------------------------------------------------------
# plan: reuse first, and verify everything before anything is written
#
# A refusal anywhere below (an unpublished checksum, a tampered archive, a destination
# that needs an authorization this script cannot ask for) leaves the machine exactly as
# it was, because nothing here has installed anything yet.
# ---------------------------------------------------------------------------

daemon_action=install
daemon_found=
daemon_cli=
daemon_version=
daemon_replaced=
if command -v whirld >/dev/null 2>&1; then
    daemon_found="$(command -v whirld)"
elif [ -x "$daemon_prefix/whirld" ]; then
    daemon_found="$daemon_prefix/whirld"
fi

# A daemon found by any route is versioned before it is trusted, and the version is read
# from the CLI, `whirl`, not from `whirld`: `whirl --version` answers from the binary
# itself with no daemon running, while `whirld` has no version flag and running it with
# no arguments starts a daemon, which this script must never do.
if [ -n "$daemon_found" ]; then
    case "$daemon_found" in
        /*)
            if [ -x "$(dirname "$daemon_found")/whirl" ]; then
                daemon_cli="$(dirname "$daemon_found")/whirl"
            fi
            ;;
    esac
    if [ -z "$daemon_cli" ]; then
        daemon_cli="$(command -v whirl 2>/dev/null || true)"
    fi
    if [ -z "$daemon_cli" ] && [ -x "$daemon_prefix/whirl" ]; then
        daemon_cli="$daemon_prefix/whirl"
    fi
    if [ -n "$daemon_cli" ] && [ -x "$daemon_cli" ]; then
        daemon_version=$("$daemon_cli" --version 2>/dev/null | awk '{ print $NF }' | head -1)
        case "$daemon_version" in
            [0-9]*.[0-9]*) ;;
            *) daemon_version= ;;
        esac
    fi

    if [ -n "$daemon_version" ] && version_ge "$daemon_version" "${WHIRL_VERSION#v}"; then
        # It names a release, and that release is the pin or newer: this is the daemon
        # the app should drive, and it is kept.
        daemon_action=reused
    elif [ -n "$daemon_version" ]; then
        daemon_replaced="$daemon_version"
    else
        # Nothing to compare against the pin, so nothing shows this daemon to be the pin
        # or newer, and the pinned release is installed over it. A CLI that answers no
        # `--version` predates the flag, since the pinned release answers it; whether its
        # own usage lists the `daemon` command does not change that, because a pre-version
        # CLI that lists it is still older than the release that will drive it. A daemon
        # with no CLI beside it lands here too, and for the same reason: it cannot be
        # versioned, so it cannot be shown to be this release or newer.
        daemon_replaced=unknown
    fi
fi

app_action=install
if [ -d "$app" ]; then
    installed_version=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist" 2>/dev/null || true)
    if [ "$installed_version" = "$WHIRL_UI_VERSION" ]; then
        app_action=present
    fi
fi

# The app's destination, before anything is fetched or written. /Applications is outside
# the home, and a write there is what macOS can put its authorization dialog in front of.
# This script does not drive or dismiss that dialog: it says what may be asked, and when
# the destination cannot be written without an authorization it cannot ask for, it stops
# here and says what to do rather than half installing.
if [ "$app_action" = install ] && ! writable_dir "$ui_prefix"; then
    if [ "$ui_system_location" = yes ]; then
        printf 'install: %s is not writable by this account, so Whirl.app cannot be placed there.\n' "$ui_prefix" >&2
        printf '  That write needs an authorization macOS asks for in its own dialog (Touch ID or\n' >&2
        printf '  your password), and this script neither drives nor answers it. Nothing was\n' >&2
        printf '  installed and nothing was changed.\n' >&2
        printf '  Two ways forward: run this again where that authorization can be given, or put\n' >&2
        printf '  the app under your own home:\n' >&2
        printf '    WHIRL_UI_PREFIX=%s/Applications sh install.sh\n' "$HOME" >&2
        exit 3
    fi
    refuse "$ui_prefix is not writable, and Whirl.app has nowhere to go; nothing was installed"
fi

if [ "$daemon_action" = reused ]; then
    note "backend:  reusing the daemon already installed: $daemon_found"
    case "$daemon_version" in
        "${WHIRL_VERSION#v}")
            note "  version    $daemon_version, the same release as $WHIRL_VERSION; it is kept"
            ;;
        *)
            note "  version    $daemon_version, newer than $WHIRL_VERSION; it is kept"
            ;;
    esac
    note "  note       this run installs no daemon binary, but the daemon it reuses is named"
    note "             in the receipt, so uninstall.sh removes the app and daemon together"
else
    if [ -n "$daemon_replaced" ]; then
        if [ "$daemon_replaced" = unknown ]; then
            note "backend:  the daemon at ${daemon_cli:-$daemon_found} answers no --version, so it"
            note "             is not shown to be $WHIRL_VERSION or newer; $WHIRL_VERSION replaces it"
        else
            note "backend:  the daemon at $daemon_cli is version $daemon_replaced, older than"
            note "             $WHIRL_VERSION, so $WHIRL_VERSION replaces it"
        fi
        note "             the app and the daemon are installed as one release; a running daemon"
        note "             keeps the old binary until its next restart"
    else
        note "backend:  whirl $WHIRL_VERSION is not installed; it will go into $daemon_prefix"
    fi
    note "  fetching   $daemon_archive"
    fetch "$daemon_archive" "$work/daemon.tar.gz" ||
        refuse "cannot fetch $daemon_archive (is that release published, and the network up?)"
    # whirl's release publishes the archive's sha256 beside it, as this repository does
    # for Whirl.app. Without that file the installer refuses rather than unpack an
    # archive that nothing verified.
    fetch "$daemon_sha" "$work/daemon.sha256" ||
        refuse "whirl publishes no checksum at $daemon_sha; refusing to install an unverified archive"
    verify "$work/daemon.tar.gz" "$work/daemon.sha256" "$daemon_archive"
fi

if [ "$app_action" = present ]; then
    note "frontend: Whirl $WHIRL_UI_VERSION is already installed at $app; nothing to install"
    note "  note       this run touches nothing here and writes no receipt line for it, so"
    note "             uninstall.sh will not remove an app it did not install"
else
    note "frontend: Whirl $WHIRL_UI_VERSION will go into $ui_prefix"
    if [ "$ui_system_location" = yes ]; then
        note "  note       one write, once: $app, and nothing else outside your home."
        note "             macOS may ask you to authorize that write (Touch ID or your password)"
        note "             because $ui_prefix is a system location. This script does not drive"
        note "             or dismiss that dialog: if it appears, it is yours to approve. No root"
        note "             and no password are used anywhere in this script."
    fi
    note "  fetching   $ui_archive"
    fetch "$ui_archive" "$work/Whirl.zip" ||
        refuse "cannot fetch $ui_archive (is that release published, and the network up?)"
    fetch "$ui_sha" "$work/Whirl.zip.sha256" ||
        refuse "no checksum at $ui_sha; refusing to install an unverified archive"
    verify "$work/Whirl.zip" "$work/Whirl.zip.sha256" "$ui_archive"
fi

# ---------------------------------------------------------------------------
# install: nothing above this line wrote to the machine
# ---------------------------------------------------------------------------

if [ "$daemon_action" = install ]; then
    mkdir -p "$daemon_prefix"
    [ "$daemon_prefix_created" = no ] || record dir "$daemon_prefix"
    tar -xzf "$work/daemon.tar.gz" -C "$daemon_prefix"
    for name in whirl whirld whirl-worker; do
        [ -x "$daemon_prefix/$name" ] ||
            refuse "the archive does not hold $name, and a daemon needs all three; nothing else was installed"
    done
    for name in whirl whirld whirl-worker; do
        note "  installed  $daemon_prefix/$name"
        record binary "$daemon_prefix/$name"
    done
fi

if [ "$daemon_action" = reused ]; then
    # A reused daemon is named too. This run installs no binary, but the receipt is what
    # uninstall.sh reads, and naming the daemon here is what makes the app and the daemon
    # one install at the other end.
    for name in whirl whirld whirl-worker; do
        daemon_path=$(daemon_binary "$name")
        [ -n "$daemon_path" ] || continue
        note "  binary     $daemon_path"
        record binary "$daemon_path"
    done
fi

# Where startup is set is not here: the app sets it on first run, and, without the app,
# `whirl daemon install` does. A headless or provisioning run has neither, so it can ask
# for the unit with WHIRL_UNIT=install. This runs the daemon's own command, reports what
# that command said, and does nothing else: no unit file is written from here and
# launchctl is never called, so the unit that lands is whirl's own.
if [ "${WHIRL_UNIT:-}" = install ]; then
    unit_cli=
    if [ -x "$daemon_prefix/whirl" ]; then
        unit_cli="$daemon_prefix/whirl"
    elif [ -n "$daemon_cli" ] && [ -x "$daemon_cli" ]; then
        unit_cli="$daemon_cli"
    else
        unit_cli=$(command -v whirl 2>/dev/null || true)
    fi
    if [ -z "$unit_cli" ] || [ ! -x "$unit_cli" ]; then
        note "  unit       WHIRL_UNIT=install was asked for, but no whirl command is here to"
        note "             install its login unit, so none was written; set startup yourself with"
        note "             \`whirl daemon install\` once the daemon is on this machine."
    else
        note "  unit       WHIRL_UNIT=install: running \`whirl daemon install\`"
        if "$unit_cli" daemon install; then
            record unit delegated
            note "  unit       whirl installed its own login item (com.guruor.whirl). No unit file"
            note "             was written here and launchctl was not called; that is the daemon's"
            note "             own command's doing, and uninstall.sh asks the same command to undo it."
        else
            note "  unit       whirl's own command did not install a login item; see its output above,"
            note "             and set startup yourself with \`whirl daemon install\` when you want it."
        fi
    fi
fi

if [ "$app_action" = install ]; then
    rm -rf "$work/app"
    mkdir -p "$work/app"
    ditto -x -k "$work/Whirl.zip" "$work/app"
    [ -d "$work/app/Whirl.app" ] ||
        refuse "the archive does not hold Whirl.app at its root; nothing was installed"
    mkdir -p "$ui_prefix"
    [ "$ui_prefix_created" = no ] || record dir "$ui_prefix"
    rm -rf "$app"
    if ! ditto "$work/app/Whirl.app" "$app"; then
        printf 'install: the write to %s was refused by the system, so the app is not installed.\n' "$app" >&2
        printf '  If macOS put an authorization dialog in front of that write, approving it is\n' >&2
        printf '  yours to do: this script does not drive or dismiss it. Run the same command again\n' >&2
        printf '  once you have, or put the app under your own home:\n' >&2
        printf '    WHIRL_UI_PREFIX=%s/Applications sh install.sh\n' "$HOME" >&2
        exit 3
    fi
    record app "$app"
    # Why this step exists: Whirl is not notarized, so a copy that arrived through a
    # browser carries macOS's quarantine flag and the first launch would meet the
    # unidentified-developer dialog. The checksum above has already proved the download is
    # the file the release built; clearing the flag is what saves the user that dialog, and
    # it happens only after the check, never before it.
    xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
    note "  installed  $app"
    note "  quarantine cleared: Whirl is not notarized, so without this a downloaded copy opens"
    note "             behind the unidentified-developer dialog. First launch: open it once from"
    note "             $ui_prefix, and if macOS still says it cannot check it, choose Open Anyway"
    note "             under System Settings > Privacy & Security."
fi

# ---------------------------------------------------------------------------
# what happened, and the one command that undoes it
# ---------------------------------------------------------------------------

undo="sh uninstall.sh"
[ "$receipt" = "$HOME/Library/Application Support/whirl-ui/install.receipt" ] ||
    undo="WHIRL_UI_RECEIPT=$receipt sh uninstall.sh"

note ""
note "install: done"
if [ "$daemon_action" = reused ]; then
    note "  backend    reused, not replaced: $daemon_found"
elif [ -n "$daemon_replaced" ]; then
    note "  backend    whirl $WHIRL_VERSION, replacing the older daemon at ${daemon_cli:-$daemon_found}"
else
    note "  backend    whirl $WHIRL_VERSION, three binaries in $daemon_prefix"
fi
if [ "$app_action" = present ]; then
    note "  frontend   already installed, not touched: $app"
else
    note "  frontend   Whirl $WHIRL_UI_VERSION at $app"
fi
note "  undo       $undo"
note "             it removes the app and the daemon binaries the receipt names, and leaves the"
note "             rest of the machine alone"
