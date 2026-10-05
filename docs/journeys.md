# whirl-ui journeys

What a person is trying to do with this app, walked against what the app does today.

This file exists because the design was built from the architecture out. The visual language, the panes,
the config keys and the tests were settled before the app was used, and the faults a person found in a
week of using it were in none of them. A folder browser the platform already draws, a Wallhaven source
with no address, a window that cannot be found again, a daemon that nothing starts, a quit that never
mentions the daemon: none of those are architectural, so a document read from the architecture cannot
see them. This one reads from the person.

## The rule

**A change to anything a person can see states its journey before it is designed.**

The journey is the four parts every entry below has: what the person is trying to do, what they see
today, what they should see, and what it costs. It is written before the change is designed, from a
real use of the app, and it is checked against the app at the commit that holds it. `docs/design.md`
carries the same rule, so the two documents cannot drift apart.

A "today" line is a quotation. Either the maintainer's words after using the app, or a run reproduced
against the app and quoted with the command that produced it. A journey whose "today" cannot be quoted
is a guess, not a journey.

## How to read an entry

Four parts, in this order:

- **What the person is trying to do** — the one thing, in their words.
- **What they see today** — quoted, with the words or the command it came from.
- **What they should see** — the same attempt, with the thing that stopped them gone.
- **What it costs** — what the gap is, and what closing it takes. Not a design, and not a plan.

## 1. The first launch after install

**What the person is trying to do.** Open the app after installing it and have wallpapers rotate.

**What they see today.** Nothing but a menu bar mark that cannot do anything with it. The maintainer:

> when the app is opened for the first time after install it by default just runs the frontend without
> starting the daemon. Ideally it should start automatically.

The app says the daemon is not there and offers no way to start it. `--dump-status` exits 2 and prints

    whirl-ui: the daemon is not reachable: whirl.sock (mode 0600) [stale socket: connection refused]: Connection refused (os error 61)

Launching the menu bar item on a machine with no daemon prints that same line and then

    whirl-ui: 1791169130180813000 the settings window is hidden: no dialog is open

`pgrep -xl whirld` prints nothing before the launch and nothing after it, and there is no
`com.guruor.whirl` unit in `~/Library/LaunchAgents`, so nothing will bring the daemon back by itself
either. Nothing in the app offers to start it.

**What they should see.** The daemon running, or a control that starts it and says what it did. The
window already has the place for the sentence; it should carry the control beside it.

**What it costs.** The app may not spawn the daemon: its contract is to be a client of the daemon, and
its one part in the daemon's lifetime is the daemon's own command, never a process of its own. Starting
it has to go through the daemon's own command, and that command has to exist first. This is the
expensive half of the five.

**Status: known and open.** No change is in progress, because it waits on the daemon's own command.

## 2. Adding a folder of your own pictures

**What the person is trying to do.** Point the app at the folder where their wallpapers live.

**What they see today.** A folder browser the app draws itself. The maintainer:

> The wallpaper directory browser should ideally use system file picker to choose the directory if
> available, so the user can select directory by using a well known UI and we dont need to reimplement
> it. I think there should be some default filepicker available.

The entry point is in the Sources pane, which `--dump-settings` prints as

    Wallpapers come from
      [x] A folder on this Mac: <the folder a source already points at>
            [Change…] [Remove]
      [Add a folder…] [Add Wallhaven]
      adding a folder of your own pictures, or Wallhaven's collection, saves it to the config file straight away; whirl uses it when it next reads the file

(The row above the controls carries the folder a source already points at; it is elided here because it
is the person's own path.)

`[Add a folder…]` opens an app-drawn chooser. Reproduced with `whirl-ui --screenshot chooser`, it is a
panel titled "Choose a folder", showing the folder it is in, then `[Up] [Use this folder] [Cancel]` and
a list of the folders inside. It has to re-earn the sidebar, iCloud, recent folders and keyboard
behaviour that the platform's own panel already has, one click at a time.

**What they should see.** The platform's own folder panel, which the person already knows how to use.

**What it costs.** One call to the platform panel, with the drawn browser kept as the fallback for a
run that has no panel. The panel's click cannot be tested, so the writes behind it stay the part a test
covers.

**Status: a change is in progress.**

## 3. Adding a Wallhaven collection

**What the person is trying to do.** Fetch wallpapers from a Wallhaven collection they have a URL for.

**What they see today.** A source that is saved with nothing to fetch. The maintainer:

> The wallhaven option doesnt even ask for a collection URL. ... a user can either add a public
> collection URL or a Private collection URL. Token is usually optional but it might be needed in case
> of accessing private collection or to avoid the wallhaven limitations. The URL field should be
> editable.

`[Add Wallhaven]` asks for nothing and writes a source with no address. Reproduced against a scratch
config, `whirl-ui --source add-wallhaven wtest` answers

    [x] Wallhaven, a remote collection: needs a key
    Wallhaven: saved, the config file now says it, and the wallpaper on screen does not change until whirl next reads the file

and the source it wrote is

    {"id":"wtest","kind":"wallhaven","weight":1,"api_key_ref":"keychain:whirl-wallhaven"}

There is no collection in the source and no field on its row to put one in, so it names nothing to
fetch, and an address cannot be added or changed after the source exists.

**What they should see.** A field that asks for the collection's address when the source is added, and
the same field to change it later, with the address reduced to what the daemon reads.

**What it costs.** The address has to be parsed (the three forms a person actually holds) and reduced
to the `<username>/<id>` pair the daemon reads, with anything else refused before it is written. The
key field already exists and already writes only a label, so no schema change.

**Status: a change is in progress.**

## 4. Coming back to a window you have lost

**What the person is trying to do.** Get back to the settings window after switching away from it.

**What they see today.** The app cannot be found in the app switcher, and asking for the window a
second time does nothing. The maintainer:

> When app is opened, it should show in app switcher so user can switch to the opened window.

and

> if an app window is open and I switch to a different window and somehow not able to find where the
> past window was so I tried to open the app settings again to switch to previously opened window but
> it did nothing.

`lsappinfo info` for the running app reports an agent with no bundle identity:

    "LSDisplayName"="whirl-ui"
    "CFBundleIdentifier"=[ NULL ]
    "ApplicationType"="UIElement"

So the app has no Dock tile, nothing in the window switcher, and a window the window manager does not
manage. `Settings…` with a window already on screen changes nothing, so nothing raises the window that
is already there.

**What they should see.** The window in the app switcher while it is on screen, and `Settings…` with
one already open bringing that window to the front, key and focused, rather than a second window or
nothing.

**What it costs.** The activation policy has to become conditional (an agent while no window is open, a
managed app while one is), and the already-open path has to raise and focus the window rather than
request it again. The empty window that used to appear at launch is a separate fault and is already
fixed.

**Status: a change is in progress.**

## 5. Quitting

**What the person is trying to do.** Close the app and know what happened to their wallpapers.

**What they see today.** `Quit` closes the app and says nothing about the daemon. The maintainer:

> When closing the frontend we can confirm with if user wants to close the daemon as well, by default
> we can keep it unchecked so only frontend closes and daemon keeps running.

`--menu-dump` prints the rows that leave the rotation alone:

    ---
    Settings…
    Quit

`Quit` ends the frontend and never touches the daemon. That is the right behaviour and an unstated one:
the person is not told the daemon keeps running, or offered to stop it.

**What they should see.** One question on quit, with "Also stop whirl" unchecked, and a "don't ask
again" that remembers the answer. Unchecked closes the app and leaves the daemon running, and the
sentence says so rather than leaving it to the label.

**What it costs.** The same wall as the first launch: stopping the daemon has to go through the
daemon's own command, which does not exist yet, and the remembered answer is the app's own preference
rather than anything the daemon owns.

**Status: known and open.** No change is in progress, because it waits on the same command as the
first launch.

## Where these stand

Two of the five are known and open, with no change in progress: the first launch and the quit, and both
wait on the same daemon command that does not exist yet. Three have a change in progress: the folder
chooser, the Wallhaven address, and the lost window. None of the five is a new pane or a new setting;
each is a way through that the person did not have.

This file is walked again when one of them changes. A journey whose "should see" has landed is moved to
the app's other documents and the entry here is updated in the same change, so this file never
describes an app that no longer exists.
