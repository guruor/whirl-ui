#!/bin/sh
#
# uninstall.sh: undo install.sh, and nothing else.
#
# Removes what install.sh put on the machine, and only that:
#
#   Whirl.app            from $WHIRL_UI_PREFIX, default /Applications, first
#                        stopping it if it is running
#   the three binaries   whirl, whirld and whirl-worker from $WHIRL_PREFIX,
#                        default ~/.local/bin
#   the daemon's unit    through whirl's own uninstaller; this script never
#                        removes a unit file itself (docs/milestones.md, M3
#                        criterion 3)
#
# It deliberately leaves everything that holds your data, and prints each path
# with the command that removes it. Your pictures are not among them: no step
# here reads or writes anything under your Pictures folder, and no step here
# touches config, cache, state or the log.
#
# Overrides are the same names install.sh takes, and default the same way:
#
#   WHIRL_UI_PREFIX   /Applications    where Whirl.app was installed
#   WHIRL_PREFIX      ~/.local/bin     where the three daemon binaries went

set -eu
umask 022

ui_prefix=${WHIRL_UI_PREFIX:-/Applications}
daemon_prefix=${WHIRL_PREFIX:-$HOME/.local/bin}
app="$ui_prefix/Whirl.app"

# The unit belongs to whirl, so it is removed through whirl, before the binaries
# that can do it are taken away. This script never removes a unit file itself.
unit_handled=no
for candidate in "$daemon_prefix/whirl" whirl; do
    if [ -x "$candidate" ] && "$candidate" help 2>/dev/null | grep -q '^  service'; then
        "$candidate" service uninstall
        unit_handled=yes
        break
    fi
done
if [ "$unit_handled" = no ]; then
    printf 'unit       whirl ships no login-item remover yet, so none was touched\n'
fi

if [ -d "$app" ]; then
    # A running copy would keep a deleted bundle alive and re-assert its menu bar
    # item, so it is stopped first. `whirl-ui` is the executable inside the app.
    pkill -x whirl-ui >/dev/null 2>&1 || true
    rm -rf "$app"
    printf 'removed    %s\n' "$app"
else
    printf 'absent     %s (nothing to remove)\n' "$app"
fi

for name in whirl whirld whirl-worker; do
    if [ -e "$daemon_prefix/$name" ]; then
        rm -f "$daemon_prefix/$name"
        printf 'removed    %s\n' "$daemon_prefix/$name"
    else
        printf 'absent     %s (nothing to remove)\n' "$daemon_prefix/$name"
    fi
done

# install.sh created these directories if they were not there; rmdir only takes
# an empty one, so a directory holding anything else is left alone.
rmdir "$daemon_prefix" 2>/dev/null || true
rmdir "$ui_prefix" 2>/dev/null || true

printf '\nleft alone on purpose: these hold your data, not the install.\n'
printf '  config and state  %s\n' "$HOME/Library/Application Support/whirl"
printf '  cache             %s\n' "$HOME/Library/Caches/whirl"
printf '  log               %s\n' "$HOME/Library/Logs/whirl"
printf 'remove them too with:\n'
printf '  rm -rf "%s" "%s" "%s"\n' \
    "$HOME/Library/Application Support/whirl" \
    "$HOME/Library/Caches/whirl" \
    "$HOME/Library/Logs/whirl"
