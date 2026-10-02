# Working in this repo

Read this before your first change. It is the project's own guide: what it is, how to run it, what a
good change looks like, and the one rule about what this repository may say. Edit it as the project
grows; it is a working document, not a formality.

## What this is

A lightweight tray frontend for the whirl wallpaper daemon

## Run it

Replace these with the real commands before the first feature lands, and keep them true.

    make test          # or: cargo test, pytest, npm test

## What a good change looks like

- The commit explains the change and why: the problem, the fix, the evidence.
- Paste the command you ran and what it printed. "Tests pass" is not evidence; the output is.
- One change per commit, and the tests around it stay green.
- If a change cannot be explained without an internal reference, explain the change instead.

## The one rule about provenance

This repository is public. Keep private tooling out of it: no ticket, card or issue id from a tracker
we use privately, no colleague's or agent's name, no home-directory or scratch path, and no reference
to internal tooling that is not in this repository. A commit subject, a branch name, a code comment
and anything the code prints are all published, and history is not editable.

Cite what a reader can check instead: the commit, the test, the measurement. For example
`tests/sweep.rs::drops_stale_lock`, or "the sweep drops a stale lock in 12 ms, measured 2026-09-27",
rather than an id nobody outside can open.

## Layout

    src/        the code
    tests/      the tests
    docs/       design notes and decisions
