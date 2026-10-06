#!/bin/sh
#
# check-release-pins.sh: does the install script at this release install the releases it should?
#
#   usage: scripts/check-release-pins.sh <tag> [script]
#
# `install.sh` names the two releases it installs as defaults of its own:
#
#   WHIRL_UI_VERSION   the app release, this repository's, so it is 0.2.4 at tag v0.2.4
#   WHIRL_VERSION      the daemon release, the other repository's, so it is v0.2.1 at the same tag
#
# They are two different questions, and the difference matters.
#
# The app default must name the tag the script is served from: a person who reads that script and runs
# it with no override gets the app release they asked for or nothing, and nothing else in this
# repository compares the two. That comparison is offline and exact.
#
# The daemon default answers to the daemon, not to this tag. `whirl` and `whirl-ui` are released
# independently, so the pin names the daemon release this app is built and tested against, which is
# normally the daemon's latest release and normally is not this tag. What must hold is that the
# release it names exists: a pin naming no release makes `install.sh` stop at its second download,
# after the first one has already succeeded, which is a worse failure than a bad app pin. This
# comparison is therefore a resolution of the archive it names, not a string match.
#
# Both were wrong once, at v0.2.1, which was tagged with both defaults still reading 0.2.0: the route
# the notes document, `raw.githubusercontent.com/guruor/whirl-ui/v0.2.1/install.sh`, installed 0.2.0.
# The release page, the archive and the checksum were all right; only the script a reader runs pointed
# at the release before it. The release workflow runs this against the tag before anything is built.
#
#   exit 0   the app default names the tag, and the daemon default names a release that is there
#   exit 1   either default is missing or names something else, or a check could not be made
#   exit 2   the tag is not a release tag, or the script is not there to read
#
# A check that cannot run is a failure, never a pass: "it could not be reached" and "there is no curl
# here" both exit 1, because neither answers the question the release gate exists to ask.
#
# The prose that names the same releases, in the script's header and in its `--help`, is bumped along
# with the defaults and is not checked here: a comment installs nothing, and a check that reads
# comments fails on rewording. The two defaults are what install.

set -eu

tag=${1-}
script=${2:-install.sh}

# Where the daemon publishes its releases. This mirrors install.sh's own default, and is
# overridable so the check below can be pointed somewhere else.
daemon_base=${WHIRL_DAEMON_BASE:-https://github.com/guruor/whirl/releases/download}

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

# The app default, against the tag, offline.
if [ -z "$ui" ]; then
    printf 'check-release-pins: %s states no WHIRL_UI_VERSION default\n' "$script" >&2
    failed=1
elif [ "$ui" != "${tag#v}" ]; then
    printf 'check-release-pins: %s installs the app release %s, and this release is %s\n' \
        "$script" "$ui" "$tag" >&2
    failed=1
fi

# The daemon default, against the daemon's published releases.
if [ -z "$daemon" ]; then
    printf 'check-release-pins: %s states no WHIRL_VERSION default\n' "$script" >&2
    failed=1
elif ! printf '%s' "$daemon" | grep -q '^v[0-9]\{1,\}\.[0-9]\{1,\}\.[0-9]\{1,\}$'; then
    printf 'check-release-pins: %s names the daemon release %s, which is not a release tag\n' \
        "$script" "$daemon" >&2
    failed=1
else
    url=$daemon_base/$daemon/whirl-$daemon-macos-arm64.tar.gz
    if ! command -v curl >/dev/null 2>&1; then
        printf 'check-release-pins: no curl here, so whether the daemon release %s exists is\n' \
            "$daemon" >&2
        printf '                    unknown, and unknown is not a pass\n' >&2
        failed=1
    else
        code=$(curl -sS -o /dev/null -w '%{http_code}' -L --max-time 30 "$url" 2>/dev/null || true)
        case "${code:-000}" in
            200|302)
                ;;
            404)
                printf 'check-release-pins: %s installs the daemon release %s, and there is no such release\n' \
                    "$script" "$daemon" >&2
                printf '                    %s answered 404\n' "$url" >&2
                failed=1
                ;;
            000)
                printf 'check-release-pins: %s installs the daemon release %s, and %s could not be reached,\n' \
                    "$script" "$daemon" "$url" >&2
                printf '                    so whether it exists is unknown, and unknown is not a pass\n' >&2
                failed=1
                ;;
            *)
                printf 'check-release-pins: %s installs the daemon release %s, and %s answered %s\n' \
                    "$script" "$daemon" "$url" "$code" >&2
                failed=1
                ;;
        esac
    fi
fi

if [ "$failed" = 1 ]; then
    printf 'check-release-pins: a reader who fetches %s and runs it with no override gets\n' \
        "$script" >&2
    printf '                    what the defaults name. The app default must be %s; the daemon\n' \
        "${tag#v}" >&2
    printf '                    default must be a daemon release that exists, which is the daemon\n' >&2
    printf '                    repository'"'"'s latest and not necessarily %s. Land both on main,\n' \
        "$tag" >&2
    printf '                    then tag main.\n' >&2
    exit 1
fi

printf 'check-release-pins: %s installs the app %s and the daemon %s, and this release is %s\n' \
    "$script" "$ui" "$daemon" "$tag"
