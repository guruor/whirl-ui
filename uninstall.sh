#!/bin/sh
#
# uninstall.sh: undo what install.sh put here, and nothing else.
#
# install.sh writes a receipt naming every path it installs - $WHIRL_UI_RECEIPT, default
# ~/Library/Application Support/whirl-ui/install.receipt - and this script removes the
# paths that receipt names. The receipt is the whole of the decision: a daemon or an app
# that install.sh found already here is reused, printed as not touched, and never written
# to the receipt, so this script leaves it alone and says so. That is where the two
# scripts agree.
#
# It never stops a process it did not start:
#
#   the daemon   is stopped only through whirl's own command, and only when the receipt
#                records that install.sh asked whirl to install its login unit. A daemon
#                you started by hand keeps running: this script does not kill it, and
#                prints that it did not.
#   Whirl.app    is never killed either. If Whirl is running from the bundle this receipt
#                names, that bundle is left in place and the script says how to quit it,
#                rather than force-quitting behind your back.
#
# It deliberately leaves everything that holds your data, and prints each path with the
# command that removes it. Your pictures are not among them: no step here reads or writes
# anything under your Pictures folder, and no step here touches config, cache, state or
# the log.
#
# One override, the same name install.sh takes:
#
#   WHIRL_UI_RECEIPT   the receipt to read, default
#                      ~/Library/Application Support/whirl-ui/install.receipt
#
# Exit codes: 0 removed what the receipt named, or had nothing to remove; 1 a path could
# not be removed and is still named by the receipt; 3 Whirl is running from the bundle
# this receipt owns, so nothing was removed: quit it and run this again.

set -eu
umask 022

receipt=${WHIRL_UI_RECEIPT:-$HOME/Library/Application Support/whirl-ui/install.receipt}

left_alone() {
    printf '\nleft alone on purpose: these hold your data, not the install.\n'
    printf '  config and state  %s\n' "$HOME/Library/Application Support/whirl"
    printf '  cache             %s\n' "$HOME/Library/Caches/whirl"
    printf '  log               %s\n' "$HOME/Library/Logs/whirl"
    printf 'remove them too with:\n'
    printf '  rm -rf "%s" "%s" "%s"\n' \
        "$HOME/Library/Application Support/whirl" \
        "$HOME/Library/Caches/whirl" \
        "$HOME/Library/Logs/whirl"
}

# The receipt names paths to delete, so it is read defensively: an empty or malformed line
# is skipped, and a path this script will not remove - the root, your home, anything less
# than two directories deep - is refused rather than deleted.
safe_to_remove() {
    case "$1" in
        / | "$HOME" | "$HOME"/) return 1 ;;
        */*/*) return 0 ;;
        *) return 1 ;;
    esac
}

# record_of <kind>: every value on the receipt's lines of that kind, one per line, with
# the rest of the line taken whole so a path holding a space survives it.
record_of() {
    while IFS= read -r line; do
        case "$line" in
            '' | '#'*) continue ;;
        esac
        [ "${line%% *}" = "$1" ] || continue
        printf '%s\n' "${line#* }"
    done < "$receipt"
}

if [ ! -f "$receipt" ]; then
    printf 'receipt    %s (no such file)\n' "$receipt"
    printf 'install.sh recorded nothing installed on this machine, so nothing was removed.\n'
    printf "A daemon or app that is here is not this install's to remove, and no process is\n"
    printf 'stopped: this script does not kill what it did not start.\n'
    left_alone
    exit 0
fi

printf 'receipt    %s\n' "$receipt"

app=$(record_of app | head -1)

# Is Whirl running from the bundle this receipt owns? Then the bundle stays. Quitting a
# running app is the operator's to do, not this script's: it does not kill a process it
# did not start. The pattern is anchored to this bundle's own executable, so an app
# running from anywhere else is not this uninstall's business either.
if [ -n "$app" ] && [ -d "$app" ]; then
    running=$(pgrep -f "^$app/Contents/MacOS/" 2>/dev/null || true)
    if [ -n "$running" ]; then
        printf 'kept       %s\n' "$app"
        printf 'Whirl is running from that bundle (pid %s), so nothing was removed.\n' \
            "$(printf '%s' "$running" | tr '\n' ' ')"
        printf 'Quit Whirl from its menu bar item, then run this again.\n'
        exit 3
    fi
fi

# The daemon's login unit is whirl's, and only whirl's own installer ever put it there. The
# receipt says whether install.sh asked for that; when it did, this asks whirl's own
# command to take it away again, and never removes a unit file itself. Asking first also
# keeps the daemon's own stop path whirl's: no process is killed from here.
whirl_bin=$(record_of binary | grep '/whirl$' | head -1 || true)
if [ "$(record_of unit | head -1)" = delegated ]; then
    if [ -n "$whirl_bin" ] && [ -x "$whirl_bin" ]; then
        printf "unit       whirl owns the daemon's login item; asking whirl to remove it:\n"
        "$whirl_bin" service uninstall || true
        printf "unit       that is what whirl's own command did with its own login item, above.\n"
        printf '           Anything it stopped, it stopped itself; this script kills nothing.\n'
    else
        printf "unit       the receipt says whirl's login item is here, but whirl's own binary is\n"
        printf '           gone; no unit file is removed from here, and none was\n'
    fi
else
    printf 'daemon     not stopped: install.sh installed no login unit here and never started a\n'
    printf "           daemon, so there is none of this install's to stop. A daemon you started\n"
    printf "           yourself is still running; stop it with whirl's own command when you want\n"
    printf '           it stopped\n'
fi

# Every path the receipt names, and only those.
refused=no
while IFS= read -r line; do
    case "$line" in
        '' | '#'*) continue ;;
    esac
    kind=${line%% *}
    case "$kind" in
        app | binary) ;;
        *) continue ;;
    esac
    path=${line#* }
    if ! safe_to_remove "$path"; then
        printf 'refused    %s (not a path this script removes)\n' "$path"
        refused=yes
        continue
    fi
    if [ ! -e "$path" ]; then
        printf 'absent     %s (nothing to remove)\n' "$path"
    elif rm -rf "$path"; then
        printf 'removed    %s\n' "$path"
    else
        printf 'refused    %s (the system would not remove it)\n' "$path"
        refused=yes
    fi
done < "$receipt"

# A directory install.sh created goes too, but only once it is empty: rmdir takes nothing
# else, so a prefix that still holds anything of yours stays.
while IFS= read -r line; do
    case "$line" in
        '' | '#'*) continue ;;
    esac
    [ "${line%% *}" = dir ] || continue
    path=${line#* }
    if safe_to_remove "$path" && rmdir "$path" 2>/dev/null; then
        printf 'removed    %s (the empty directory install.sh created)\n' "$path"
    fi
done < "$receipt"

# The receipt is install.sh's own bookkeeping, so it goes with the rest once everything it
# named is gone; a later install.sh run writes a new one.
if [ "$refused" = no ]; then
    rm -f "$receipt"
    printf 'removed    %s\n' "$receipt"
    rmdir "$(dirname "$receipt")" 2>/dev/null || true
else
    printf 'kept       %s (a path above was refused, and this still names it)\n' "$receipt"
fi

left_alone

if [ "$refused" = yes ]; then
    exit 1
fi
