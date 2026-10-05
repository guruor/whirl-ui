#!/bin/sh
#
# test-install.sh: the install/uninstall acceptance run, with stand-in binaries and
# archives and no network.
#
#   usage: sh scripts/test-install.sh
#
# It builds, in a throwaway directory under ${TMPDIR:-/tmp}:
#
#   stand-in daemons  whirl, whirld and whirl-worker, small scripts that answer exactly
#                     as one release does: `whirl --version` names the release, its usage
#                     lists the `daemon` command, and a stand-in for an older release
#                     answers neither
#   a daemon archive  whirl-v0.2.0-macos-arm64.tar.gz holding the pinned binaries, and
#                     its .sha256, as the release publishes them
#   an app archive    Whirl-0.2.0.zip with Whirl.app/Contents/Info.plist, and its .sha256
#
# and then runs install.sh against them with WHIRL_PREFIX, WHIRL_UI_PREFIX and
# WHIRL_UI_RECEIPT under the throwaway directory, and PATH holding only the stand-in
# prefix and the system, so nothing installed on the machine is found by accident.
#
# The scenarios, one at a time:
#
#   1. an older stand-in that names its version  -> replaced by the pinned release, and
#      the output says which version was replaced and why
#   2. an older stand-in that answers no         -> replaced too, judged by the absence of
#      `--version` and of the `daemon` command
#   3. a pinned stand-in                         -> reused byte for byte, and the output
#      says so
#   4. uninstall.sh over scenario 1's receipt     -> the app and the daemon binaries go,
#      and nothing else does
#
# Every scenario also checks the receipt: it names the daemon binaries in the replaced and
# the reused case alike.
#
# Exit 0 when every scenario holds; 1 on the first failure, naming it.

set -u

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/whirl-install-test.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

# Only the system, and the stand-in prefix. Nothing the machine already has installed is
# on this PATH, so a scenario can never reuse a real daemon by accident.
sys_path=/usr/bin:/bin:/usr/sbin:/sbin

ui_version=0.2.0
whirl_version=v0.2.0

app_archive="$work/Whirl-$ui_version.zip"
app_sha="$app_archive.sha256"
daemon_archive="$work/whirl-$whirl_version-macos-arm64.tar.gz"
daemon_sha="$daemon_archive.sha256"

scenarios=0

fail() {
    printf '\ntest-install: FAIL: %s\n' "$*" >&2
    exit 1
}

ok() {
    printf 'test-install: ok: %s\n' "$*"
}

contains() {
    printf '%s' "$1" | grep -Fq -- "$2"
}

receipt_has() {
    grep -Fqx -- "$2" "$1"
}

# write_daemon <dir> <kind>: the three binaries, as one release answers. kind is `pinned`
# (0.2.0, with `--version` and the `daemon` command), `versioned-old` (0.1.0, with
# `--version`) or `unversioned-old` (neither).
write_daemon() {
    dir=$1
    kind=$2
    mkdir -p "$dir"
    case "$kind" in
        pinned)
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
case "${1-}" in
    --version) printf 'whirl 0.2.0\n'; exit 0 ;;
esac
printf 'whirl: a command is required\n' >&2
printf 'usage: whirl <command>\n' >&2
printf '  next                 set the next image now\n' >&2
printf '  daemon start         ask the supervisor to start the daemon (macOS)\n' >&2
exit 3
SCRIPT
            side=pinned
            ;;
        versioned-old)
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
case "${1-}" in
    --version) printf 'whirl 0.1.0\n'; exit 0 ;;
esac
printf 'whirl: a command is required\n' >&2
printf 'usage: whirl <command>\n' >&2
printf '  next                 set the next image now\n' >&2
exit 3
SCRIPT
            side=versioned-old
            ;;
        unversioned-old)
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
printf 'whirl: a command is required\n' >&2
printf 'usage: whirl <command>\n' >&2
printf '  next                 set the next image now\n' >&2
exit 3
SCRIPT
            side=unversioned-old
            ;;
        *)
            fail "write_daemon: unknown kind $kind"
            ;;
    esac
    chmod +x "$dir/whirl"
    cat > "$dir/whirld" <<SCRIPT
#!/bin/sh
printf '%s\n' "$side whirld"
SCRIPT
    cat > "$dir/whirl-worker" <<SCRIPT
#!/bin/sh
printf '%s\n' "$side whirl-worker"
SCRIPT
    chmod +x "$dir/whirld" "$dir/whirl-worker"
}

make_daemon_archive() {
    build=$1
    mkdir -p "$build/pinned"
    write_daemon "$build/pinned" pinned
    (cd "$build/pinned" && tar -czf "$daemon_archive" whirl whirld whirl-worker)
    (cd "$(dirname "$daemon_archive")" && shasum -a 256 "$(basename "$daemon_archive")" > "$(basename "$daemon_sha")")
}

make_app_archive() {
    build=$1
    mkdir -p "$build/app/Whirl.app/Contents/MacOS"
    cat > "$build/app/Whirl.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.guruor.whirl-ui</string>
    <key>CFBundleExecutable</key>
    <string>whirl-ui</string>
    <key>CFBundleShortVersionString</key>
    <string>$ui_version</string>
    <key>CFBundleVersion</key>
    <string>$ui_version</string>
</dict>
</plist>
PLIST
    printf '#!/bin/sh\nexit 0\n' > "$build/app/Whirl.app/Contents/MacOS/whirl-ui"
    chmod +x "$build/app/Whirl.app/Contents/MacOS/whirl-ui"
    (cd "$build/app" && ditto -c -k --sequesterRsrc --keepParent Whirl.app "$app_archive")
    (cd "$(dirname "$app_archive")" && shasum -a 256 "$(basename "$app_archive")" > "$(basename "$app_sha")")
}

# run_install <prefix> <ui_prefix> <receipt>: install.sh with everything pointed under the
# throwaway directory, and PATH holding only the stand-in prefix and the system.
run_install() {
    prefix=$1
    ui=$2
    receipt=$3
    PATH="$prefix:$sys_path" \
    WHIRL_UI_VERSION="$ui_version" \
    WHIRL_VERSION="$whirl_version" \
    WHIRL_UI_ARCHIVE="$app_archive" \
    WHIRL_UI_SHA256="$app_sha" \
    WHIRL_ARCHIVE="$daemon_archive" \
    WHIRL_SHA256="$daemon_sha" \
    WHIRL_PREFIX="$prefix" \
    WHIRL_UI_PREFIX="$ui" \
    WHIRL_UI_RECEIPT="$receipt" \
    sh "$root/install.sh"
}

# sha_of <path>: the file's sha256, or `absent`.
sha_of() {
    if [ -e "$1" ]; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    else
        printf 'absent\n'
    fi
}

echo "test-install: building the stand-in archives under $work"
make_daemon_archive "$work/daemon-build"
make_app_archive "$work/app-build"
echo "test-install: daemon archive $(basename "$daemon_archive") $(sha_of "$daemon_archive")"
echo "test-install: app archive    $(basename "$app_archive") $(sha_of "$app_archive")"

# ---------------------------------------------------------------------------
# 1. an older stand-in that names its version is replaced
# ---------------------------------------------------------------------------

echo
echo "=== scenario 1: an older daemon that names its version ==="
prefix=$work/s1/prefix
ui=$work/s1/ui
receipt=$work/s1/receipt
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" versioned-old
before=$(sha_of "$prefix/whirl")
out=$(run_install "$prefix" "$ui" "$receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 1: install.sh exited $status"
contains "$out" "is version 0.1.0, older than" || fail "scenario 1: the run does not say the found daemon is 0.1.0 and older"
contains "$out" "so v0.2.0 replaces it" || fail "scenario 1: the run does not say what replaces it"
after=$(sha_of "$prefix/whirl")
[ "$before" != "$after" ] || fail "scenario 1: the older whirl binary was not replaced"
"$prefix/whirl" --version | grep -q 'whirl 0.2.0' || fail "scenario 1: the installed whirl is not the pinned 0.2.0"
receipt_has "$receipt" "binary $prefix/whirl" || fail "scenario 1: the receipt does not name $prefix/whirl"
receipt_has "$receipt" "binary $prefix/whirld" || fail "scenario 1: the receipt does not name $prefix/whirld"
receipt_has "$receipt" "binary $prefix/whirl-worker" || fail "scenario 1: the receipt does not name $prefix/whirl-worker"
receipt_has "$receipt" "app $ui/Whirl.app" || fail "scenario 1: the receipt does not name the app"
scenarios=$((scenarios + 1))
ok "scenario 1: 0.1.0 replaced by 0.2.0, receipt names the daemon binaries"

# ---------------------------------------------------------------------------
# 2. an older stand-in that answers no --version is replaced
# ---------------------------------------------------------------------------

echo
echo "=== scenario 2: an older daemon that answers no --version ==="
prefix=$work/s2/prefix
ui=$work/s2/ui
receipt=$work/s2/receipt
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" unversioned-old
before=$(sha_of "$prefix/whirl")
out=$(run_install "$prefix" "$ui" "$receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 2: install.sh exited $status"
contains "$out" "is older than v0.2.0" || fail "scenario 2: the run does not say the found daemon is older"
contains "$out" "no \`daemon\` command" || fail "scenario 2: the run does not name the missing daemon command"
after=$(sha_of "$prefix/whirl")
[ "$before" != "$after" ] || fail "scenario 2: the older whirl binary was not replaced"
"$prefix/whirl" --version | grep -q 'whirl 0.2.0' || fail "scenario 2: the installed whirl is not the pinned 0.2.0"
receipt_has "$receipt" "binary $prefix/whirl" || fail "scenario 2: the receipt does not name $prefix/whirl"
scenarios=$((scenarios + 1))
ok "scenario 2: the unversioned older daemon is replaced, receipt names the binaries"

# ---------------------------------------------------------------------------
# 3. a pinned stand-in is reused byte for byte
# ---------------------------------------------------------------------------

echo
echo "=== scenario 3: a daemon at the pinned release is reused ==="
prefix=$work/s3/prefix
ui=$work/s3/ui
receipt=$work/s3/receipt
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" pinned
before=$(sha_of "$prefix/whirl")
before_whirld=$(sha_of "$prefix/whirld")
before_worker=$(sha_of "$prefix/whirl-worker")
out=$(run_install "$prefix" "$ui" "$receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 3: install.sh exited $status"
contains "$out" "reusing the daemon already installed" || fail "scenario 3: the run does not say it reused the daemon"
contains "$out" "the same release as v0.2.0; it is kept" || fail "scenario 3: the run does not say the daemon is the pinned release"
after=$(sha_of "$prefix/whirl")
[ "$before" = "$after" ] || fail "scenario 3: the reused whirl binary changed"
[ "$before_whirld" = "$(sha_of "$prefix/whirld")" ] || fail "scenario 3: the reused whirld binary changed"
[ "$before_worker" = "$(sha_of "$prefix/whirl-worker")" ] || fail "scenario 3: the reused whirl-worker binary changed"
receipt_has "$receipt" "binary $prefix/whirl" || fail "scenario 3: the reused daemon is not named in the receipt"
receipt_has "$receipt" "binary $prefix/whirld" || fail "scenario 3: the reused whirld is not named in the receipt"
receipt_has "$receipt" "binary $prefix/whirl-worker" || fail "scenario 3: the reused whirl-worker is not named in the receipt"
scenarios=$((scenarios + 1))
ok "scenario 3: pinned daemon reused byte for byte, receipt names the binaries"

# ---------------------------------------------------------------------------
# 4. uninstall.sh removes what scenario 1's receipt names, and nothing else
# ---------------------------------------------------------------------------

echo
echo "=== scenario 4: uninstall.sh over scenario 1's receipt ==="
s1_receipt=$work/s1/receipt
s1_prefix=$work/s1/prefix
s1_ui=$work/s1/ui
keeper=$work/s1/not-this-installs
mkdir -p "$keeper"
printf 'keep me\n' > "$keeper/keep"
out=$(PATH="$sys_path" WHIRL_UI_RECEIPT="$s1_receipt" sh "$root/uninstall.sh" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 4: uninstall.sh exited $status"
[ ! -e "$s1_prefix/whirl" ] || fail "scenario 4: uninstall.sh left $s1_prefix/whirl"
[ ! -e "$s1_prefix/whirld" ] || fail "scenario 4: uninstall.sh left $s1_prefix/whirld"
[ ! -e "$s1_prefix/whirl-worker" ] || fail "scenario 4: uninstall.sh left $s1_prefix/whirl-worker"
[ ! -e "$s1_ui/Whirl.app" ] || fail "scenario 4: uninstall.sh left the app"
[ ! -f "$s1_receipt" ] || fail "scenario 4: uninstall.sh left the receipt"
[ -f "$keeper/keep" ] || fail "scenario 4: uninstall.sh removed something the receipt did not name"
scenarios=$((scenarios + 1))
ok "scenario 4: the receipt's paths went, the untouched file stayed"

printf '\ntest-install: all %s scenarios passed\n' "$scenarios"
