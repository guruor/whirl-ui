#!/bin/bash
#
# Fail if the menu bar item puts a window on screen while no dialog is open.
#
# The defect this guards: eframe shows the root window after its first painted
# frame whatever `visible` the builder asked for (`eframe`
# `native/epi_integration.rs`, `post_rendering`), so an app whose window must
# start hidden has to hide it again from a pass (`crates/whirl-ui/src/tray.rs`,
# `App::state_window`). Without that guard the app presented an empty window,
# in the frame's own clear colour, 1.3 s to 2.3 s after launch: measured
# 2026-10-04, 10 of 10 launches without the guard and none with it.
#
# This is a script and not a `cargo test` because it needs a window server and a
# GUI session, which the three CI legs do not have. A maintainer runs it on a
# Mac. It drives nothing: it reads the window list, and asks for no permission.
#
# usage: scripts/check-launch-window.sh [binary] [seconds]
#   binary    defaults to target/release/whirl-ui, built if it is missing
#   seconds   how long to watch after launch, defaults to 8
#
# Exit 0 when no window appeared, 1 when one did, 3 when the check could not run.
set -u

here=$(cd -- "$(dirname -- "$0")" && pwd)
root=$(cd -- "$here/.." && pwd)
binary=${1:-$root/target/release/whirl-ui}
watch=${2:-8}
scratch=$(mktemp -d)
pid=""

cleanup() {
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
  fi
  rm -f -- "$scratch/window-probe" "$scratch/swift.log" "$scratch/app.log"
  rmdir -- "$scratch" 2>/dev/null
}
trap cleanup EXIT

if [ ! -x "$binary" ]; then
  echo "building $binary" >&2
  (cd -- "$root" && cargo build --release) || exit 3
fi
if [ ! -x "$binary" ]; then
  echo "no binary at $binary" >&2
  exit 3
fi
if ! swiftc -O "$here/window-probe.swift" -o "$scratch/window-probe" 2>"$scratch/swift.log"; then
  echo "cannot build the window probe:" >&2
  sed -n '1,40p' "$scratch/swift.log" >&2
  exit 3
fi

# No mode argument: the menu bar item, which is the app a user starts.
"$binary" >"$scratch/app.log" 2>&1 &
pid=$!

# A window of zero size is how the window server lists one that was never laid
# out or ordered in, and no one can see it; a window with an area is the defect.
found=""
start=$(date +%s)
while [ $(( $(date +%s) - start )) -lt "$watch" ]; do
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "the app exited before the end of the watch:" >&2
    sed -n '1,40p' "$scratch/app.log" >&2
    exit 3
  fi
  found=$("$scratch/window-probe" "$pid" | grep '^window id=' | grep -v 'size=0x0' | head -1)
  if [ -n "$found" ]; then break; fi
  sleep 0.2
done

echo "the app's log:"
sed -n '1,40p' "$scratch/app.log"

if [ -n "$found" ]; then
  echo "FAIL: a window was on screen with no dialog open:"
  echo "  $found"
  exit 1
fi
echo "ok: no window on screen in ${watch}s of a menu bar launch"
exit 0
