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
#      `--version`, even though its usage lists no `daemon` verb
#   3. a pinned stand-in                         -> reused byte for byte, and the output
#      says so
#   4. uninstall.sh over scenario 1's receipt     -> the app and the daemon binaries go,
#      and nothing else does
#   5. the installer's default path               -> install.sh writes no login unit and
#      says nothing about startup, which is the app's on first run and `whirl daemon
#      install`'s without the app
#   6. WHIRL_UNIT=install                         -> install.sh runs `whirl daemon install`
#      and records the login unit as whirl's own
#   7. the binary a login unit runs               -> uninstall.sh asks whirl's own command
#      to remove the unit before it deletes that binary, so no login item is left pointing
#      at a program that is gone
#   8. a receipt that does not name that binary   -> no uninstall is asked for
#   9. whirl refuses to remove the unit            -> the binary it runs is kept, and the
#      run says the login item is still there rather than claiming a clean removal
#  10. an older stand-in that answers no `--version` while its usage does list the `daemon`
#      verbs                                       -> replaced as well: "cannot name a
#      version" is not evidence of being current, so a CLI that answers no version is
#      never reused
#  11. a stand-in newer than the pin               -> reused, and the daemon archive is
#      pointed at a path that does not exist, so a run that downloaded anything would
#      refuse; the three binaries are unchanged
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
# (0.2.0, with `--version` and the whole `daemon` verb list, the login-unit pair included),
# `newer` (0.3.0, the same shape), `versioned-old` (0.1.0, with `--version`),
# `unversioned-old` (neither) or `daemon-only` (no `--version`, yet its usage lists the
# `daemon` verbs, the shape whirl v0.2.0 shipped). `versioned-old` and `unversioned-old`
# list no `daemon` verb at all, which is how whirl v0.1 answered, and the pinned one
# writes each unit verb it is asked for into $WHIRL_UNIT_LOG when that is set.
write_daemon() {
    dir=$1
    kind=$2
    mkdir -p "$dir"
    case "$kind" in
        pinned)
            # The released v0.2 CLI's own verb list, the login-unit pair included. A
            # stand-in answers `daemon install` and `daemon uninstall` by leaving a mark in
            # $WHIRL_UNIT_LOG, which is what makes the unit path observable from outside:
            # this list stopped at `daemon start` before, so install.sh could ask for a
            # verb no stand-in had and no scenario would have noticed.
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
usage() {
    printf 'usage: whirl <command>\n'
    printf '  next                 set the next image now\n'
    printf '  daemon install       write the login unit and load it (macOS)\n'
    printf '  daemon uninstall     stop the daemon and remove the login unit (macOS)\n'
    printf '  daemon start         ask the supervisor to start the daemon (macOS)\n'
    printf '  daemon stop          ask the supervisor to stop the daemon (macOS)\n'
    printf '  help                 this text\n'
}
case "${1-}" in
    --version) printf 'whirl 0.2.0\n'; exit 0 ;;
    help) usage; exit 0 ;;
    daemon)
        case "${2-}" in
            install|uninstall|start|stop|status)
                [ -n "${WHIRL_UNIT_LOG-}" ] && printf '%s\n' "$2" >> "$WHIRL_UNIT_LOG"
                exit 0
                ;;
        esac
        ;;
esac
usage >&2
printf 'whirl: a command is required\n' >&2
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
        daemon-only)
            # The shape whirl v0.2.0 shipped: `--version` is not a verb yet, so the CLI
            # answers an error and its usage, and that usage does list the `daemon` verbs.
            # Reading "it lists `daemon`" as "it is current" is what kept a stale daemon.
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
printf 'whirl: --version is not a command, or it has the wrong number of arguments\n' >&2
printf 'usage: whirl <command>\n' >&2
printf '  next                 set the next image now\n' >&2
printf '  daemon install       write the login unit and load it (macOS)\n' >&2
printf '  daemon uninstall     stop the daemon and remove the login unit (macOS)\n' >&2
printf '  daemon start         ask the supervisor to start the daemon (macOS)\n' >&2
printf '  daemon stop          ask the supervisor to stop the daemon (macOS)\n' >&2
exit 3
SCRIPT
            side=daemon-only
            ;;
        newer)
            cat > "$dir/whirl" <<'SCRIPT'
#!/bin/sh
usage() {
    printf 'usage: whirl <command>\n'
    printf '  next                 set the next image now\n'
    printf '  daemon install       write the login unit and load it (macOS)\n'
    printf '  daemon uninstall     stop the daemon and remove the login unit (macOS)\n'
}
case "${1-}" in
    --version) printf 'whirl 0.3.0\n'; exit 0 ;;
    help) usage; exit 0 ;;
    daemon)
        case "${2-}" in
            install|uninstall|start|stop|status)
                [ -n "${WHIRL_UNIT_LOG-}" ] && printf '%s\n' "$2" >> "$WHIRL_UNIT_LOG"
                exit 0
                ;;
        esac
        ;;
esac
usage >&2
printf 'whirl: a command is required\n' >&2
exit 3
SCRIPT
            side=newer
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

# write_stub_cli <dir> <log>: a stand-in `whirl` for a scratch PATH, the same shape the
# daemon lifecycle's own tests use. It records its full argv, one call per line, so the
# question "what did the script actually ask the daemon to do" is answerable from the log.
# When $WHIRL_STUB_WATCH names a path, it also records whether that path still existed when
# it ran; that is what shows the unit was removed before the binary it runs was deleted.
# $WHIRL_STUB_UNINSTALL_CODE makes `daemon uninstall` refuse, which is how a scenario
# exercises the path where whirl will not remove its own unit.
write_stub_cli() {
    cli_dir=$1
    cli_log=$2
    mkdir -p "$cli_dir"
    cat > "$cli_dir/whirl" <<'SCRIPT'
#!/bin/sh
log=${WHIRL_STUB_LOG:?}
printf '%s\n' "$*" >> "$log"
if [ -n "${WHIRL_STUB_WATCH:-}" ]; then
    if [ -e "$WHIRL_STUB_WATCH" ]; then
        printf 'watch %s present\n' "$WHIRL_STUB_WATCH" >> "$log"
    else
        printf 'watch %s gone\n' "$WHIRL_STUB_WATCH" >> "$log"
    fi
fi
if [ "${1-} ${2-}" = "daemon uninstall" ]; then
    code=${WHIRL_STUB_UNINSTALL_CODE:-0}
    if [ "$code" != 0 ]; then
        printf 'whirl: cannot remove the login item: refused\n' >&2
    fi
    exit "$code"
fi
exit 0
SCRIPT
    chmod +x "$cli_dir/whirl"
}

# write_unit <home> <program>: a stand-in login unit where uninstall.sh reads it, with
# ProgramArguments naming <program>. Its shape is launchd's: the label and the argument
# vector, and the first argument is the program the supervisor runs.
write_unit() {
    unit_home=$1
    unit_program=$2
    unit_file="$unit_home/Library/LaunchAgents/com.guruor.whirl.plist"
    mkdir -p "$(dirname "$unit_file")"
    cat > "$unit_file" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.guruor.whirl</string>
    <key>ProgramArguments</key>
    <array>
        <string>$unit_program</string>
    </array>
</dict>
</plist>
PLIST
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

# run_install <prefix> <ui_prefix> <receipt> [name=value ...]: install.sh with everything
# pointed under the throwaway directory, and PATH holding only the stand-in prefix and the
# system. Any further `name=value` arguments are set for this run alone, which is how a
# scenario asks for WHIRL_UNIT without leaving it set for the next one.
run_install() {
    prefix=$1
    ui=$2
    receipt=$3
    shift 3
    env PATH="$prefix:$sys_path" \
        WHIRL_UI_VERSION="$ui_version" \
        WHIRL_VERSION="$whirl_version" \
        WHIRL_UI_ARCHIVE="$app_archive" \
        WHIRL_UI_SHA256="$app_sha" \
        WHIRL_ARCHIVE="$daemon_archive" \
        WHIRL_SHA256="$daemon_sha" \
        WHIRL_PREFIX="$prefix" \
        WHIRL_UI_PREFIX="$ui" \
        WHIRL_UI_RECEIPT="$receipt" \
        "$@" \
        sh "$root/install.sh"
}

# run_uninstall <home> <path> <receipt>: uninstall.sh with a home of its own, so the login
# unit it reads is the one a scenario wrote and never the machine's own, the given PATH,
# and the receipt named.
run_uninstall() {
    env HOME="$1" PATH="$2" WHIRL_UI_RECEIPT="$3" sh "$root/uninstall.sh"
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
contains "$out" "answers no --version" || fail "scenario 2: the run does not say the found daemon answers no version"
contains "$out" "is not shown to be v0.2.0 or newer" || fail "scenario 2: the run does not say why the daemon is replaced"
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
s1_home=$work/s1/home
keeper=$work/s1/not-this-installs
mkdir -p "$s1_home" "$keeper"
printf 'keep me\n' > "$keeper/keep"
out=$(run_uninstall "$s1_home" "$sys_path" "$s1_receipt" 2>&1)
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

# ---------------------------------------------------------------------------
# 5. the installer's default path writes no login unit
# ---------------------------------------------------------------------------

echo
echo "=== scenario 5: the installer's default path writes no login unit ==="
prefix=$work/s5/prefix
ui=$work/s5/ui
receipt=$work/s5/receipt
unit_log=$work/s5/unit.log
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" unversioned-old
WHIRL_UNIT_LOG="$unit_log"
export WHIRL_UNIT_LOG
out=$(run_install "$prefix" "$ui" "$receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 5: install.sh exited $status"
[ ! -s "$unit_log" ] ||
    fail "scenario 5: the default run asked whirl for a login unit, which is not the default"
contains "$out" "  unit       " &&
    fail "scenario 5: the default run says something about startup, beyond the header line"
unset WHIRL_UNIT_LOG
scenarios=$((scenarios + 1))
ok "scenario 5: the default install writes no unit; startup is the app's or whirl's to set"

# ---------------------------------------------------------------------------
# 6. WHIRL_UNIT=install runs whirl's own login-unit command
# ---------------------------------------------------------------------------

echo
echo "=== scenario 6: WHIRL_UNIT=install runs whirl's own login-unit command ==="
prefix=$work/s6/prefix
ui=$work/s6/ui
receipt=$work/s6/receipt
unit_log=$work/s6/unit.log
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" unversioned-old
WHIRL_UNIT_LOG="$unit_log"
export WHIRL_UNIT_LOG
out=$(run_install "$prefix" "$ui" "$receipt" WHIRL_UNIT=install 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 6: install.sh exited $status"
grep -qx install "$unit_log" 2>/dev/null ||
    fail "scenario 6: WHIRL_UNIT=install never asked whirl to install its login unit"
contains "$out" "whirl daemon install" ||
    fail "scenario 6: the run does not name the whirl command it ran"
receipt_has "$receipt" "unit delegated" ||
    fail "scenario 6: the receipt does not record the login unit as whirl's"
unset WHIRL_UNIT_LOG
scenarios=$((scenarios + 1))
ok "scenario 6: WHIRL_UNIT=install ran whirl daemon install and recorded the unit as whirl's"

# ---------------------------------------------------------------------------
# 7. the binary a login unit runs is deleted only after whirl removes the unit
# ---------------------------------------------------------------------------

echo
echo "=== scenario 7: the login unit is removed before the binary it runs is deleted ==="
s7=$work/s7
s7_prefix=$s7/prefix
s7_elsewhere=$s7/elsewhere
s7_home=$s7/home
s7_bin=$s7/bin
s7_receipt=$s7/receipt
s7_log=$s7/calls.log
mkdir -p "$s7_prefix" "$s7_elsewhere" "$s7_home" "$s7_bin" "$(dirname "$s7_receipt")"
printf '#!/bin/sh\nexit 0\n' > "$s7_prefix/whirld"
printf '#!/bin/sh\nexit 0\n' > "$s7_prefix/whirl-worker"
chmod +x "$s7_prefix/whirld" "$s7_prefix/whirl-worker"
# The receipt names the daemon binaries, and the login unit runs the daemon's own binary.
# The whirl that removes the unit is a stand-in on a scratch PATH, not a receipt path.
{
    printf 'binary %s\n' "$s7_prefix/whirld"
    printf 'binary %s\n' "$s7_prefix/whirl-worker"
} > "$s7_receipt"
write_unit "$s7_home" "$s7_prefix/whirld"
write_stub_cli "$s7_bin" "$s7_log"
out=$(WHIRL_STUB_LOG="$s7_log" WHIRL_STUB_WATCH="$s7_prefix/whirld" \
    run_uninstall "$s7_home" "$s7_bin:$sys_path" "$s7_receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 7: uninstall.sh exited $status"
grep -qx 'daemon uninstall' "$s7_log" ||
    fail "scenario 7: uninstall.sh never asked whirl to remove the login unit"
contains "$out" "before the binary goes" ||
    fail "scenario 7: the run does not say the unit is removed before the binary"
grep -qx "watch $s7_prefix/whirld present" "$s7_log" ||
    fail "scenario 7: whirl ran after the binary it runs was already deleted"
[ ! -e "$s7_prefix/whirld" ] ||
    fail "scenario 7: the binary the login unit runs was not removed"
[ ! -f "$s7_receipt" ] || fail "scenario 7: uninstall.sh left the receipt"
scenarios=$((scenarios + 1))
ok "scenario 7: whirl removed the login unit before the binary it runs was deleted"

# ---------------------------------------------------------------------------
# 8. a receipt that does not name the unit's binary asks whirl for nothing
# ---------------------------------------------------------------------------

echo
echo "=== scenario 8: a login unit pointing elsewhere is left alone ==="
s8=$work/s8
s8_prefix=$s8/prefix
s8_elsewhere=$s8/elsewhere
s8_home=$s8/home
s8_bin=$s8/bin
s8_receipt=$s8/receipt
s8_log=$s8/calls.log
mkdir -p "$s8_prefix" "$s8_elsewhere" "$s8_home" "$s8_bin" "$(dirname "$s8_receipt")"
printf '#!/bin/sh\nexit 0\n' > "$s8_prefix/whirld"
chmod +x "$s8_prefix/whirld"
printf 'binary %s\n' "$s8_prefix/whirld" > "$s8_receipt"
# The login unit runs a binary this receipt does not own, so nothing here stops it.
printf '#!/bin/sh\nexit 0\n' > "$s8_elsewhere/whirld"
chmod +x "$s8_elsewhere/whirld"
write_unit "$s8_home" "$s8_elsewhere/whirld"
write_stub_cli "$s8_bin" "$s8_log"
out=$(WHIRL_STUB_LOG="$s8_log" run_uninstall "$s8_home" "$s8_bin:$sys_path" "$s8_receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 8: uninstall.sh exited $status"
[ ! -s "$s8_log" ] ||
    fail "scenario 8: uninstall.sh asked whirl to remove a unit for a binary the receipt does not own"
[ ! -e "$s8_prefix/whirld" ] || fail "scenario 8: the receipt's binary was not removed"
[ -x "$s8_elsewhere/whirld" ] || fail "scenario 8: uninstall.sh touched a binary it does not own"
scenarios=$((scenarios + 1))
ok "scenario 8: a unit pointing at a binary outside the receipt is left alone"

# ---------------------------------------------------------------------------
# 9. a whirl that refuses leaves the login item's binary in place
# ---------------------------------------------------------------------------

echo
echo "=== scenario 9: a whirl that refuses does not leave a unit pointing at nothing ==="
s9=$work/s9
s9_prefix=$s9/prefix
s9_home=$s9/home
s9_bin=$s9/bin
s9_receipt=$s9/receipt
s9_log=$s9/calls.log
mkdir -p "$s9_prefix" "$s9_home" "$s9_bin" "$(dirname "$s9_receipt")"
printf '#!/bin/sh\nexit 0\n' > "$s9_prefix/whirld"
chmod +x "$s9_prefix/whirld"
printf 'binary %s\n' "$s9_prefix/whirld" > "$s9_receipt"
write_unit "$s9_home" "$s9_prefix/whirld"
write_stub_cli "$s9_bin" "$s9_log"
out=$(WHIRL_STUB_LOG="$s9_log" WHIRL_STUB_UNINSTALL_CODE=1 \
    run_uninstall "$s9_home" "$s9_bin:$sys_path" "$s9_receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 1 ] ||
    fail "scenario 9: uninstall.sh exited $status, not 1, after whirl refused"
[ -e "$s9_prefix/whirld" ] ||
    fail "scenario 9: the binary the login unit runs was deleted although the unit was not removed"
[ -f "$s9_receipt" ] ||
    fail "scenario 9: the receipt was removed although a path above was kept"
contains "$out" "still points at" ||
    fail "scenario 9: the run does not say the login item is still there"
scenarios=$((scenarios + 1))
ok "scenario 9: a refused unit removal keeps the binary and reports the login item is still there"

# ---------------------------------------------------------------------------
# 10. a CLI that answers no --version while its usage lists the `daemon` verbs is replaced
#
# This is the case the reuse rule used to get wrong: "it answers no --version" was read as
# "it is not older than the pin", so a stale daemon was kept and the override installed
# nothing.
# ---------------------------------------------------------------------------

echo
echo "=== scenario 10: a CLI that answers no --version but lists daemon is replaced ==="
prefix=$work/s10/prefix
ui=$work/s10/ui
receipt=$work/s10/receipt
mkdir -p "$prefix" "$ui" "$(dirname "$receipt")"
write_daemon "$prefix" daemon-only
before=$(sha_of "$prefix/whirl")
out=$(run_install "$prefix" "$ui" "$receipt" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] || fail "scenario 10: install.sh exited $status"
contains "$out" "reusing the daemon" &&
    fail "scenario 10: a CLI that answers no version was reused"
contains "$out" "answers no --version" ||
    fail "scenario 10: the run does not say the found daemon answers no version"
contains "$out" "v0.2.0 replaces it" ||
    fail "scenario 10: the run does not say the pin replaces the daemon"
[ "$before" != "$(sha_of "$prefix/whirl")" ] ||
    fail "scenario 10: the unversioned whirl binary was not replaced"
"$prefix/whirl" --version | grep -q 'whirl 0.2.0' ||
    fail "scenario 10: the installed whirl is not the pinned 0.2.0"
receipt_has "$receipt" "binary $prefix/whirl" || fail "scenario 10: the receipt does not name $prefix/whirl"
receipt_has "$receipt" "binary $prefix/whirld" || fail "scenario 10: the receipt does not name $prefix/whirld"
receipt_has "$receipt" "binary $prefix/whirl-worker" || fail "scenario 10: the receipt does not name $prefix/whirl-worker"
scenarios=$((scenarios + 1))
ok "scenario 10: a CLI that answers no version is replaced, not reused, and the receipt names the binaries"

# ---------------------------------------------------------------------------
# 11. a CLI newer than the pin is reused, and nothing is downloaded
#
# The app is already at the pinned version, and the daemon archive is pointed at a path
# that does not exist, so a run that fetched anything at all would refuse and exit 1. A
# pass here is the proof: no download, no changed binary.
# ---------------------------------------------------------------------------

echo
echo "=== scenario 11: a daemon newer than the pin is reused, nothing downloaded ==="
prefix=$work/s11/prefix
ui=$work/s11/ui
receipt=$work/s11/receipt
mkdir -p "$prefix" "$ui/Whirl.app/Contents" "$(dirname "$receipt")"
cat > "$ui/Whirl.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleShortVersionString</key>
    <string>$ui_version</string>
</dict>
</plist>
PLIST
write_daemon "$prefix" newer
before=$(sha_of "$prefix/whirl")
before_whirld=$(sha_of "$prefix/whirld")
before_worker=$(sha_of "$prefix/whirl-worker")
out=$(run_install "$prefix" "$ui" "$receipt" \
    WHIRL_ARCHIVE="$work/s11-absent.tar.gz" WHIRL_SHA256="$work/s11-absent.sha256" 2>&1)
status=$?
printf '%s\n' "$out"
[ "$status" -eq 0 ] ||
    fail "scenario 11: install.sh exited $status, so it fetched an archive it should not need"
contains "$out" "fetching" &&
    fail "scenario 11: the run downloaded something although the daemon was reused"
contains "$out" "reusing the daemon already installed" ||
    fail "scenario 11: the run does not say it reused the daemon"
contains "$out" "0.3.0, newer than v0.2.0; it is kept" ||
    fail "scenario 11: the run does not name the newer version it kept"
[ "$before" = "$(sha_of "$prefix/whirl")" ] || fail "scenario 11: the reused whirl binary changed"
[ "$before_whirld" = "$(sha_of "$prefix/whirld")" ] || fail "scenario 11: the reused whirld changed"
[ "$before_worker" = "$(sha_of "$prefix/whirl-worker")" ] || fail "scenario 11: the reused whirl-worker changed"
receipt_has "$receipt" "binary $prefix/whirl" || fail "scenario 11: the reused daemon is not named in the receipt"
receipt_has "$receipt" "binary $prefix/whirld" || fail "scenario 11: the reused whirld is not named in the receipt"
receipt_has "$receipt" "binary $prefix/whirl-worker" || fail "scenario 11: the reused whirl-worker is not named in the receipt"
scenarios=$((scenarios + 1))
ok "scenario 11: a daemon newer than the pin is reused, nothing downloaded, binaries untouched"

printf '\ntest-install: all %s scenarios passed\n' "$scenarios"
