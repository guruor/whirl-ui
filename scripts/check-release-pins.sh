#!/bin/sh
#
# check-release-pins.sh: does the install script at this release install this release?
#
#   usage: scripts/check-release-pins.sh <tag> [script]
#
# `install.sh` names the two releases it installs as defaults of its own:
#
#   WHIRL_UI_VERSION   the app release, 0.2.1
#   WHIRL_VERSION      the daemon release, v0.2.1
#
# A person who reads the script at a tag and runs it with no override installs exactly
# those two. They are hand-kept, and nothing else in this repository compares them with
# the tag the script is served from, so a release cut without bumping them installs an
# older pair than the release says. That happened once: v0.2.1 was tagged with both
# defaults still reading 0.2.0, so the route the notes document,
# `raw.githubusercontent.com/guruor/whirl-ui/v0.2.1/install.sh`, installed 0.2.0. The
# release page, the archive and the checksum were all right; only the script a reader runs
# pointed at the release before it.
#
# This script is that comparison, and it is the whole of it. It reads the two defaults out
# of the script and fails when either names something else, or when either is missing at
# all: a default that cannot be read is a failure and not an empty answer, because the
# question "does this match the tag" has no answer when there is nothing to compare. The
# release workflow runs it against the tag before anything is built.
#
#   exit 0   both defaults name the tag
#   exit 1   one of them names another release, or could not be read; it says which
#   exit 2   the tag is not a release tag, or the script is not there to read
#
# The prose that names the same release, in the script's header and in its `--help`, is
# bumped along with the defaults and is not checked here: a comment installs nothing, and
# a check that reads comments fails on rewording. The two defaults are what install.

set -eu

tag=${1-}
script=${2:-install.sh}

case "$tag" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *)
        printf 'check-release-pins: usage: scripts/check-release-pins.sh <tag> [script]\n' >&2
        printf '                    and <tag> is a release tag, v<major>.<minor>.<patch>;\n' >&2
        printf '                    this is %s\n' "${tag:-nothing}" >&2
        exit 2
        ;;
esac

if [ ! -f "$script" ]; then
    printf 'check-release-pins: no %s to read, so there is nothing to compare with %s\n' \
        "$script" "$tag" >&2
    exit 2
fi

# pin <name>: the default inside `NAME=${NAME:-value}`, printed on its own. The line is
# matched from its start, so the same name appearing in the header prose, in `--help` or
# in a comparison is not mistaken for the default itself.
pin() {
    awk -v want="$1" '
        index($0, want "=${" want ":-") == 1 {
            value = $0
            sub(/^.*:-/, "", value)
            sub(/}[[:space:]]*$/, "", value)
            print value
            exit
        }' "$script"
}

ui=$(pin WHIRL_UI_VERSION)
daemon=$(pin WHIRL_VERSION)

failed=0

if [ -z "$ui" ]; then
    printf 'check-release-pins: %s states no WHIRL_UI_VERSION default\n' "$script" >&2
    failed=1
elif [ "$ui" != "${tag#v}" ]; then
    printf 'check-release-pins: %s installs the app release %s, and this release is %s\n' \
        "$script" "$ui" "$tag" >&2
    failed=1
fi

if [ -z "$daemon" ]; then
    printf 'check-release-pins: %s states no WHIRL_VERSION default\n' "$script" >&2
    failed=1
elif [ "$daemon" != "$tag" ]; then
    printf 'check-release-pins: %s installs the daemon release %s, and this release is %s\n' \
        "$script" "$daemon" "$tag" >&2
    failed=1
fi

if [ "$failed" = 1 ]; then
    printf 'check-release-pins: a reader who fetches %s and runs it with no override gets\n' \
        "$script" >&2
    printf '                    what the defaults name, which is not %s. Bump them to %s and\n' \
        "$tag" "${tag#v}" >&2
    printf '                    %s, land that on main, and tag main again.\n' "$tag" >&2
    exit 1
fi

printf 'check-release-pins: %s installs the app %s and the daemon %s, and this release is %s\n' \
    "$script" "$ui" "$daemon" "$tag"
