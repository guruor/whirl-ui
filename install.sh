#!/bin/sh
#
# install.sh: whirl and Whirl, in one command.
#
# Installs two things and reports on both:
#
#   whirl (the daemon)    the three release binaries (whirl, whirld, whirl-worker)
#                         into $WHIRL_PREFIX, default ~/.local/bin. A daemon that
#                         is already installed is reused, printed, and never touched.
#   Whirl (the tray app)  Whirl.app into $WHIRL_UI_PREFIX, default /Applications.
#
# Every archive is checked against the sha256 published beside it *before*
# anything is unpacked, so a tampered or truncated download installs nothing.
# Nothing needs root, a password or an answer: one command, one exit code.
#
# This script never writes the daemon's login unit. The daemon owns its own unit,
# so the script asks whirl's own installer to put it there and does nothing else
# with it (docs/milestones.md, M3 criterion 3). No unit file is written here.
#
# Read this file before you run it; it is short on purpose, and the documented
# route is "download, read, run". Piping it into a shell is offered beside that
# route, never instead of it:
#
#   curl -fsSLO https://raw.githubusercontent.com/guruor/whirl-ui/v0.1.0/install.sh
#   less install.sh
#   sh install.sh
#
# Every override below is optional and defaults to the published release. They
# exist so the same script can install a build that is not the published one, and
# so an acceptance run can install into a throwaway prefix with no network.
#
#   WHIRL_UI_VERSION   0.1.0               the app release; the tag is v0.1.0
#   WHIRL_UI_BASE      the release download where Whirl-<version>.zip lives
#   WHIRL_UI_ARCHIVE   <path or URL>       use this archive, skip the download
#   WHIRL_UI_SHA256    <path or URL>       its checksum; default <archive>.sha256
#   WHIRL_UI_PREFIX    /Applications       where Whirl.app is installed
#   WHIRL_VERSION      v0.1.0              the daemon release
#   WHIRL_BASE         the release download where the daemon archive lives
#   WHIRL_ARCHIVE      <path or URL>       use this archive, skip the download
#   WHIRL_SHA256       <path or URL>       its checksum; default <archive>.sha256
#   WHIRL_PREFIX       ~/.local/bin        where the three daemon binaries go

set -eu
umask 022

usage() {
    cat <<'USAGE'
install.sh: whirl and Whirl, in one command.

  sh install.sh            install the daemon and the app; reuse a daemon already there
  sh install.sh --help     this text

It downloads the whirl daemon archive and the Whirl.app archive, checks each
against the sha256 published beside it, and installs them. It never writes the
daemon's login unit: whirl owns that unit and installs it. No root, no password,
no questions.

Overrides (optional; the defaults are the published release):
  WHIRL_UI_VERSION   0.1.0
  WHIRL_UI_ARCHIVE   path or URL of Whirl-<version>.zip
  WHIRL_UI_SHA256    path or URL of its checksum
  WHIRL_UI_PREFIX    /Applications
  WHIRL_VERSION      v0.1.0
  WHIRL_ARCHIVE      path or URL of the daemon archive
  WHIRL_SHA256       path or URL of its checksum
  WHIRL_PREFIX       ~/.local/bin

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

WHIRL_UI_VERSION=${WHIRL_UI_VERSION:-0.1.0}
WHIRL_VERSION=${WHIRL_VERSION:-v0.1.0}

ui_prefix=${WHIRL_UI_PREFIX:-/Applications}
daemon_prefix=${WHIRL_PREFIX:-$HOME/.local/bin}
app="$ui_prefix/Whirl.app"

ui_archive=${WHIRL_UI_ARCHIVE:-${WHIRL_UI_BASE:-https://github.com/guruor/whirl-ui/releases/download/v$WHIRL_UI_VERSION}/Whirl-$WHIRL_UI_VERSION.zip}
ui_sha=${WHIRL_UI_SHA256:-$ui_archive.sha256}
daemon_archive=${WHIRL_ARCHIVE:-${WHIRL_BASE:-https://github.com/guruor/whirl/releases/download/$WHIRL_VERSION}/whirl-$WHIRL_VERSION-macos-arm64.tar.gz}
daemon_sha=${WHIRL_SHA256:-$daemon_archive.sha256}

work=$(mktemp -d "${TMPDIR:-/tmp}/whirl-ui-install.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

note() { printf '%s\n' "$*"; }
refuse() {
    printf 'install: %s\n' "$*" >&2
    exit 1
}

# fetch <source> <dest>. The source is an https URL, a file:// URL or a plain
# path, and all three go through curl, so the download path under test is the
# same one a real install uses.
fetch() {
    case "$1" in
        http://* | https://* | file://*) src=$1 ;;
        /*) src="file://$1" ;;
        *) src="file://$(pwd)/$1" ;;
    esac
    curl -fsSL --retry 3 --connect-timeout 15 -o "$2" "$src" >/dev/null
}

# verify <archive> <checksum-file> [label]. The archive's sha256 must equal the
# first field of the checksum file. The file's own name column is ignored, so an
# archive that was renamed still verifies; only the bytes are compared.
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

# ---------------------------------------------------------------------------
# plan: reuse first, and verify everything before anything is written
#
# A refusal anywhere below (an unpublished checksum, a tampered archive) leaves
# the machine exactly as it was, because nothing here has installed anything yet.
# ---------------------------------------------------------------------------

daemon_action=install
daemon_found=
if command -v whirld >/dev/null 2>&1; then
    daemon_action=reused
    daemon_found="$(command -v whirld)"
elif [ -x "$daemon_prefix/whirld" ]; then
    daemon_action=reused
    daemon_found="$daemon_prefix/whirld"
elif launchctl print "gui/$(id -u)/com.guruor.whirl" >/dev/null 2>&1; then
    daemon_action=reused
    daemon_found="the loaded login item com.guruor.whirl"
fi

app_action=install
if [ -d "$app" ]; then
    installed_version=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist" 2>/dev/null || true)
    if [ "$installed_version" = "$WHIRL_UI_VERSION" ]; then
        app_action=present
    fi
fi

if [ "$daemon_action" = reused ]; then
    note "backend:  reusing the daemon already installed: $daemon_found"
else
    note "backend:  whirl $WHIRL_VERSION is not installed; it will go into $daemon_prefix"
    note "  fetching   $daemon_archive"
    fetch "$daemon_archive" "$work/daemon.tar.gz" ||
        refuse "cannot fetch $daemon_archive (is that release published, and the network up?)"
    # whirl's release publishes the archive's sha256 beside it, as this
    # repository does for Whirl.app. Without that file the installer refuses
    # rather than unpack an archive that nothing verified.
    fetch "$daemon_sha" "$work/daemon.sha256" ||
        refuse "whirl publishes no checksum at $daemon_sha; refusing to install an unverified archive"
    verify "$work/daemon.tar.gz" "$work/daemon.sha256" "$daemon_archive"
fi

if [ "$app_action" = present ]; then
    note "frontend: Whirl $WHIRL_UI_VERSION is already installed at $app; nothing to install"
else
    note "frontend: Whirl $WHIRL_UI_VERSION will go into $ui_prefix"
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
    tar -xzf "$work/daemon.tar.gz" -C "$daemon_prefix"
    for name in whirl whirld whirl-worker; do
        [ -x "$daemon_prefix/$name" ] ||
            refuse "the archive does not hold $name, and a daemon needs all three; nothing else was installed"
    done
    note "  installed  $daemon_prefix/whirl"
    note "  installed  $daemon_prefix/whirld"
    note "  installed  $daemon_prefix/whirl-worker"

    # The daemon's login unit is whirl's, not this app's: this asks whirl's own
    # installer to put it there and never writes a unit file itself. whirl v0.1
    # ships no such installer, so the call is made only when whirl's own usage
    # lists the verb, and otherwise the absence is named rather than papered over.
    if "$daemon_prefix/whirl" help 2>/dev/null | grep -q '^  service'; then
        "$daemon_prefix/whirl" service install
        note "  unit       whirl installed its own login item (com.guruor.whirl)"
    else
        note "  unit       whirl $WHIRL_VERSION ships no login-item installer yet, so none"
        note "             was written, here or anywhere; start the daemon yourself until it does"
    fi
fi

if [ "$app_action" = install ]; then
    rm -rf "$work/app"
    mkdir -p "$work/app"
    ditto -x -k "$work/Whirl.zip" "$work/app"
    [ -d "$work/app/Whirl.app" ] ||
        refuse "the archive does not hold Whirl.app at its root; nothing was installed"
    mkdir -p "$ui_prefix"
    rm -rf "$app"
    ditto "$work/app/Whirl.app" "$app"
    # Why this step exists: Whirl is not notarized, so a copy that arrived
    # through a browser carries macOS's quarantine flag and the first launch
    # would meet the unidentified-developer dialog. The checksum above has
    # already proved the download is the file the release built; clearing the
    # flag is what saves the user that dialog, and it happens only after the
    # check, never before it.
    xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
    note "  installed  $app"
    note "  quarantine cleared: the app is not notarized, so a downloaded copy would"
    note "             otherwise open behind the unidentified-developer dialog"
fi

# ---------------------------------------------------------------------------
# what happened, and the one command that undoes it
# ---------------------------------------------------------------------------

undo=
[ "$daemon_prefix" = "$HOME/.local/bin" ] || undo="WHIRL_PREFIX=$daemon_prefix "
[ "$ui_prefix" = /Applications ] || undo="${undo}WHIRL_UI_PREFIX=$ui_prefix "
undo="${undo}sh uninstall.sh"

note ""
note "install: done"
if [ "$daemon_action" = reused ]; then
    note "  backend    reused, not touched: $daemon_found"
else
    note "  backend    whirl $WHIRL_VERSION, three binaries in $daemon_prefix"
fi
if [ "$app_action" = present ]; then
    note "  frontend   already installed, not touched: $app"
else
    note "  frontend   Whirl $WHIRL_UI_VERSION at $app"
fi
note "  undo       $undo"
