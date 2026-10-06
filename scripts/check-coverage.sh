#!/bin/sh
#
# The two coverage floors, checked against the report `cargo llvm-cov` has just left
# behind.
#
#   usage: TOTAL_FLOOR=70 DECISION_FLOOR=84 DRAWING='<regex>' scripts/check-coverage.sh
#
# The floors are two because the crate is two things, and the numbers are the workflow's
# rather than this script's: `.github/workflows/ci.yml` hands over the floor for the
# whole workspace, the floor for everything the DRAWING regex leaves, and that regex,
# each beside the comment that says what it was measured from. This script is only the
# comparison, so that a floor that misses says which one missed and by how much.
# `cargo llvm-cov report --fail-under-lines` does the same arithmetic but exits 1 in
# silence, which leaves a red run with nothing in its log about what it wanted.
#
# It reads and it runs no test: `cargo llvm-cov report` reads the coverage data a
# previous `cargo llvm-cov` wrote under `target/llvm-cov-target`, so the suite runs once
# for both floors. `jq` reads the one number out of the JSON report, and the runners
# carry it.
#
# It prints each floor with the number it was checked against, and on a miss it also
# writes a GitHub `::error::` annotation, so the reason is on the pull request and not
# only in the raw log. Exit 0 when both hold, 1 when a floor is missed, 2 when it is
# called without the three values.

set -eu

if [ -z "${TOTAL_FLOOR-}" ] || [ -z "${DECISION_FLOOR-}" ] || [ -z "${DRAWING-}" ]; then
    echo "check-coverage: TOTAL_FLOOR, DECISION_FLOOR and DRAWING must all be set" >&2
    echo "usage: TOTAL_FLOOR=70 DECISION_FLOOR=84 DRAWING='<regex>'" \
        "scripts/check-coverage.sh" >&2
    exit 2
fi

# The line coverage of the whole report, and of the report with the files DRAWING
# matches left out.
percent_all() {
    cargo llvm-cov report --json --summary-only |
        jq -r '.data[0].totals.lines.percent'
}

percent_without_drawing() {
    cargo llvm-cov report --json --summary-only --ignore-filename-regex "$DRAWING" |
        jq -r '.data[0].totals.lines.percent'
}

# Two decimals is as much as a log needs; the comparison below uses the full number.
rounded() {
    awk -v value="$1" 'BEGIN { printf "%.2f", value + 0 }'
}

total=$(percent_all)
decisions=$(percent_without_drawing)

missed=no

printf 'whole workspace: %s%% (floor %s%%)\n' \
    "$(rounded "$total")" "$TOTAL_FLOOR"
if awk -v got="$total" -v floor="$TOTAL_FLOOR" \
    'BEGIN { exit !(got + 0 < floor + 0) }'; then
    echo "::error::line coverage over the whole workspace is" \
        "$(rounded "$total")%, below the floor of $TOTAL_FLOOR%"
    missed=yes
fi

printf 'decisions, with the drawing left out: %s%% (floor %s%%)\n' \
    "$(rounded "$decisions")" "$DECISION_FLOOR"
if awk -v got="$decisions" -v floor="$DECISION_FLOOR" \
    'BEGIN { exit !(got + 0 < floor + 0) }'; then
    echo "::error::line coverage over the files that are not the drawing is" \
        "$(rounded "$decisions")%, below the floor of $DECISION_FLOOR%: that floor is" \
        "the one about the decisions, so a decision module has lost its tests, or a" \
        "module that is drawing is missing from DRAWING and is counting as a decision"
    missed=yes
fi

[ "$missed" = no ]
