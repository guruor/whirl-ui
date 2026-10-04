# whirl-ui design

What this app is meant to look like, and what each thing the design draws would cost in
configuration. Two sections, and one rule the file exists to enforce: an element is never "future
work". It either **exists, at a path**, it needs a **named new key**, it needs a **named daemon
feature**, or it is **not needed**.

Written 2026-10-04, from the design reference and the daemon as it stands. Where this document has
an opinion it says so and leaves the decision open: whether an enhancement is worth building is the
maintainer's call.

## 1. The visual language

### The reference

One dark settings-window mockup, supplied 2026-10-04 as `photo_2026-10-04_13-08-53.jpg` (1280x853,
JPEG). It is not committed here: it is the maintainer's own file, named here only so that the
sampling below can be run again against the same input.

It draws a window with a navigation sidebar, a segmented control, cards with hairline dividers, list
rows, toggles, a status footer carrying the connection state and the version, and a blue swirl
wordmark.

### The palette, and where the numbers come from

The values are sampled from the image's own pixels with ImageMagick, not read by eye and not guessed
by a vision model. The command:

    magick photo_2026-10-04_13-08-53.jpg -colors 12 -depth 8 -format %c histogram:info:- | sort -rn

Its whole output. The twelve clusters sum to 1,091,840, the image's 1280x853 pixels, so the
quantiser dropped nothing:

    979189: (10,22,39) #0A1627 srgb(10,22,39)
     61849: (27,44,84) #1B2C54 srgb(27,44,84)
     13678: (53,72,96) #354860 srgb(53,72,96)
      9030: (95,111,145) #5F6F91 srgb(95,111,145)
      8510: (77,90,113) #4D5A71 srgb(77,90,113)
      5376: (163,177,203) #A3B1CB srgb(163,177,203)
      4779: (73,94,215) #495ED7 srgb(73,94,215)
      4642: (116,137,160) #7489A0 srgb(116,137,160)
      1736: (56,78,182) #384EB6 srgb(56,78,182)
      1251: (110,147,211) #6E93D3 srgb(110,147,211)
      1164: (73,54,51) #493633 srgb(73,54,51)
       636: (149,132,107) #95846B srgb(149,132,107)

Eight of the twelve name a role:

| role | value | pixels | the image |
|---|---|---|---|
| canvas | `#0A1627` | 979,189 | 89.68%, dominant |
| panel, card | `#1B2C54` | 61,849 | 5.66% |
| hairline, separator | `#354860` | 13,678 | 1.25% |
| accent (active tab, primary button, toggle on) | `#495ED7` | 4,779 | 0.44% |
| accent pressed, darker | `#384EB6` | 1,736 | 0.16% |
| accent highlight | `#6E93D3` | 1,251 | 0.11% |
| text muted | `#7489A0` | 4,642 | 0.43% |
| text secondary | `#A3B1CB` | 5,376 | 0.49% |

The remaining four name no role and are not part of the language: `#5F6F91` (0.83%) and `#4D5A71`
(0.78%) are mid-greys the quantiser produced between the hairlines and the text; `#493633` (0.11%)
and `#95846B` (0.06%) are warm and come from a wallpaper preview inside the mockup, not from the
window chrome.

The reference's primary text is near-white over the canvas. The 12-colour quantisation produced no
near-white cluster, so there is no sampled value for it here and this document does not invent one:
a later pass that needs it should sample it (drop `-colors`, or raise it) rather than guess.

How far this is from a stock dark theme: the accent is an indigo, `#495ED7`, not a framework blue,
and the canvas is a navy, `#0A1627`, not a slate. The palette is the contract for the app's dark
theme; it is not a starting point to be improved on.

### Two rules that come from the platform, not from taste

- **The menu bar image is a template image.** macOS draws it from its alpha and tints it to match the
  menu bar and its appearance, so it is black plus alpha, never colour. The blue swirl lives in the
  app icon and the window's own header, not in the menu bar. The assets directory states this and the
  suite holds it (`crates/whirl-ui/assets/README.md`).
- **Dark only.** The reference is a dark window. The light theme is not part of it.

### Where this stands in the code

The palette is the contract; the window applying it is what this document is held to. As it stands
the window is two choices and one line of state (where the wallpapers come from, how often they
change, and whether whirl answered), in `crates/whirl-ui/src/settings.rs`; the sidebar of named panes
the reference draws is not part of it yet, which is what the next section is about.

## 2. Future enhancements

Each element the reference draws, classified as what it needs **today**. The final column is this
document's opinion and nothing else: the decision belongs to the maintainer, and every row is left
open.

| what the design shows | what it needs today | opinion (open) | the config change it implies |
|---|---|---|---|
| **Per-source weight**: a weight control on each source | **exists, at `crates/whirl-core/src/config.rs`** (`SourceConfig::weight`) and **`docs/spec/features.md` 2.1** ("Relative chance of this source being chosen per rotation"). No daemon change. | Sensible as a later control. A weight control is UI work only. | none |
| **Avoid repeats** | **exists, at `crates/whirl-core/src/config.rs`** (`Dedupe::recent_entries`), carried into the effective plan as `dedupe.recent_entries` (`docs/architecture.md` 2.6). No daemon change. | Nothing to build but a control. | none |
| **"Shuffle wallpapers" as a toggle** | **not needed** for what exists: rotation already picks one source by a weighted random draw (`docs/spec/features.md` F2, Part 1). A toggle that chose an order would be **new key: `rotation.order = "shuffle" \| "sequential"`**. Careful: a Wallhaven source already has an `order` key (`crates/whirl-core/src/config.rs`), but that is the API's sort, `desc`/`asc`, not a rotation order, so the two must not share a name. | The toggle draws a choice that does not exist. Worth the key only if a sequential rotation is actually wanted. | add `rotation.order`, default `shuffle` (today's behaviour) |
| **A per-source on/off toggle that keeps its weight** | **new key: `sources[].enabled`**. A source has no `enabled` field today: `weight = 0` is the off idiom (`docs/spec/features.md` 2.1, "`0` disables without deleting"), and the protocol's `enabled=` is derived as `weight > 0` (`crates/whirl-core/src/config.rs`). | The one worth doing first. As built, the toggle silently destroys the number a person set: `crates/whirl-ui/src/config_file.rs::set_source_enabled` writes `0` and says why it cannot keep the old value ("the schema has nowhere to keep it"), with the test `disabling_writes_zero_and_enabling_restores_the_default_weight`. One boolean removes the whole problem. | add `enabled`; keep `weight` as the ratio. Schema bump, below |
| **Schedule: active hours, custom windows** | **new key: `schedule.windows`**, plus **daemon feature: window evaluation**. Only `schedule.interval_seconds` (`crates/whirl-core/src/config.rs`) and `startup.mode` / `startup.respect_manual` (`crates/whirl-core/src/config.rs`) exist. This reopens a non-goal rather than filling a gap: `docs/spec/features.md` Part 3 lists "Scheduling rules beyond a fixed interval (work hours, holidays, per-workspace)" as a non-goal. | The biggest item and the least necessary. A window needs the daemon to read the clock and skip or force a rotation, which is the resident state machine the non-goal was written to avoid. Not yet. | add `schedule.windows`; daemon work |
| **Per-display** | **exists, at `crates/whirl-core/src/config.rs`** (`DisplayMode`, `DisplaySection::mode`, `display.mode = "all" \| "per-display"`). The honest part is the platform, not the key: `per-display` is accepted everywhere and resolved per platform, and it is refused on GNOME as impossible and KDE as out of scope, while on macOS it is unverified and runs as `all` with `display_mode_reason: unverified_platform` (`docs/architecture.md` 3.7; the reasons are `unverified_platform`, `impossible_on_this_desktop`, `out_of_scope_on_this_desktop`, `no_displays`, `docs/architecture.md` 2.10). Going further is **daemon feature: per-display rotation queues**. | A Displays pane is one radio and a refusal sentence on most desktops, which is worth showing precisely because it is honest about the fallback. The rotation model behind it is the expensive part. | none for the pane; a rotation-model change for real per-display |
| **Pause while presenting or gaming** | **daemon feature: presentation/game detection**. Nothing in the daemon watches fullscreen or presentation state, and the config has no key it could read. Pause itself already exists (`pause` / `resume`, `docs/architecture.md` 2.5): what is missing is the trigger, not the action. | The trigger is the whole cost, and it is a state no protocol verb reports. Not yet. | none until the trigger exists; a key would only configure a detector |
| **Appearance pane** | **not needed**. The config has no appearance key and the app is dark only, so a pane here would invent a setting rather than surface one. | Leave it out until there is a light theme, and let the theme come first. | none |
| **Network pane** | **not needed**. There is no network key in the config, and the app fetches nothing: the daemon owns the sources and every piece of state, and the app reads the socket (`README.md`). A proxy setting would be a daemon key the app merely draws. | Not this app's surface. | none |
| **Advanced pane** | **not needed**. The file-only knobs that exist (`log_level`, `backend`, `cache.*`, `filters.*`, `crates/whirl-core/src/config.rs`) are governed by the window's own rule, that nothing which can only be set in the file is on screen (`crates/whirl-ui/src/settings.rs`). An Advanced pane is a change to that rule, not a missing key. | Leaving it out is a decision worth keeping. | none |

### Already possible from the protocol

The reference's status footer, and a reader's assumption that these are unbuilt, are worth correcting:
the daemon's protocol already answers all of them, so each is UI work in this repository and not a
missing capability.

| the design shows | the protocol already carries it | where |
|---|---|---|
| status | `status`, the stable key set | `docs/architecture.md` 2.10 |
| version | `version` → `daemon_version:`, `protocol:`, `platform:` | `docs/architecture.md` 2.5 |
| config path | `config path` → `config: <abs path>` | `docs/architecture.md` 2.5 |
| diagnostics | `config check` → the `source:` records and one `plan:` line | `docs/architecture.md` 2.5 |
| history | `history [<n>]` → `count:` then `entry:` lines | `docs/architecture.md` 2.5 |
| favourites | `favorites`, `favorite [<id>]`, `unfavorite <id>` | `docs/architecture.md` 2.5 |
| a config reload | the `config_reloaded` event | `docs/architecture.md` 2.9 |

Three of them are already read today, through the dump modes rather than through the window:
`cargo run -- --dump-status`, `--dump-sources` and `--dump-config-check` (`README.md`).

### Config simplifications this implies

Two additions and one pair-change. Named exactly, with what breaks.

**Add `sources[].enabled` (boolean), and keep `weight` as the ratio.** This is the change with a real
cost, because it moves a key's meaning rather than adding one. Today `weight = 0` means "off"
(`docs/spec/features.md` 2.1); under the new pair, on/off moves to `enabled` and `weight` is the ratio
alone, so `weight = 0` stops meaning what it means now.

What breaks:

- **The config schema version moves, `1` to `2`** (`CONFIG_SCHEMA`, `crates/whirl-core/src/config.rs`).
  The daemon's own rule for that number is "bumped only when a key changes meaning"
  (`_comment_config_schema`, `docs/architecture.md` 4.2 and `crates/whirl-core/src/config.rs`), and
  `weight = 0` changing meaning is exactly that. Adding a key alone would not force it: an unknown
  field is a warning and is ignored (`crates/whirl-core/src/config.rs`), so a version that did not
  move would leave an older daemon silently ignoring `enabled` and still rotating the source.
- **What an older daemon does with the new file is a flag, not a refusal.** A `config_schema` newer
  than the build is read, reported as `state_schema_newer: 1` in `status`, and never rewritten
  (`crates/whirl-core/src/config.rs`, `docs/architecture.md` 2.10). So the version bump is the
  signal, and it is worth being precise about that: it does not stop an older daemon from acting on a
  file it cannot fully understand.
- **The parser must keep reading the old spelling.** A file with `weight = 0` and no `enabled` has to
  keep meaning disabled, or every source a person had turned off silently rejoins the rotation.
- **In this repository**, `crates/whirl-ui/src/config_file.rs::set_source_enabled` changes with it,
  and so does its test `disabling_writes_zero_and_enabling_restores_the_default_weight`: the writer
  becomes "write `enabled`, leave `weight` alone" instead of "write `weight = 0` and accept the loss".

**Add `rotation.order = "shuffle" | "sequential"`, default `"shuffle"`.** Today's behaviour is the
default, so nothing changes meaning and, by the rule above, this alone does not force a schema bump.
The naming hazard is the real cost: `order` already exists on a Wallhaven source as the API's sort
(`desc`/`asc`, `crates/whirl-core/src/config.rs`), so the rotation order must be a differently named
key, not a new meaning for that one.

**Add `schedule.windows`.** A new key plus daemon work: the daemon would have to evaluate the window,
not merely read it, so the key is the cheap half. It belongs with the schema discussion for the same
reason as `enabled`: a key the daemon does not read is worse than no key, because the file would look
like it worked.
