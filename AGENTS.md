# Waybill

`docs/design.md` is the spec; the README is the tour. Read the spec section
for whatever you change.

## Crates

- `waybill-core`: the model and the protocol. `task.rs` reads what
  `task export` prints, `model.rs` sorts tasks into lanes and works out what
  each one frees, `lines.rs` lays dependency chains out as rail lines,
  `pulse.rs` counts what the charts draw, `lamp.rs` is the one lamp rule,
  `plan.rs` is where each view puts its cards (lane order, the Desk's
  sections, what fits in the drawer) and `theme.rs` is the palette. Those
  are plain data in, plain data out: callers pass `now` and a `Zone`.
  `act.rs` is every change to the list (each one a `task` command),
  `say.rs` is what the user can say to it from any screen (the actions,
  the buttons a card offers, the `/` line's words, what a drop means),
  `claim.rs` is who holds a started task, `agent.rs` is the pick-up protocol
  built on both, `lint.rs` the house rules. A new rule about where a task
  sits belongs in `model.rs` with a test; a new way to change the list
  belongs in `act.rs` with a case in `tests/e2e.rs`.
  `wire.rs` is what `waybill serve` sends the phone.
- `waybill`: the CLI and the waybar module (one binary, `waybill`).
  `serve.rs` is `waybill serve` and `waybill pair`, the phone's server.
- `waybill-board`: the GTK4 window and the drawer. `shell.rs` is the frame,
  `views/` holds one file per view, `detail.rs` is the card pane (its
  buttons come from `say::offers`), `drawer.rs` is the layer-shell drawer,
  and everything drawn with cairo is in `charts.rs`. `theme.rs` holds the
  core's palette for the drawing thread; it feeds both the CSS and the
  charts. `anim.rs` and `stage.rs` came from Tablet's board.
- `android/`: the Android app (Kotlin, Compose). It decides nothing: it
  draws the `Screen` that `wire.rs` builds and sends back the words
  `say::parse` reads. `Wire.kt` mirrors the wire, `ui/` holds one file per
  view, `Watch.kt` is the background look for notifications, the widget
  and the tile. A rule belongs in the core, never here; a new field shows
  up in `Wire.kt` and in the view that draws it.
- `bar-anchor` is no longer in this repo: it's the shared
  [bar-anchor](https://github.com/NathanielInman/bar-anchor) crate, pinned by tag in the root `Cargo.toml`.
  Change it there and move the tag; its `Anchor` JSON must stay
  identical across the apps.

## Build and test

`cargo build --workspace --locked`, `cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --all -- --check`. CI runs all four. The board needs GTK 4.12 or
newer and gtk4-layer-shell (`.github/scripts/gtk4-layer-shell.sh` is how CI
gets it). The end-to-end test needs a `task` binary and skips without one.

The app builds with Gradle in `android/`. Run it under `lowprio`, one build
at a time:

```sh
cd android
lowprio ./gradlew --no-daemon --max-workers=4 testDebugUnitTest assembleDebug
lowprio ./gradlew --no-daemon --max-workers=4 recordRoborazziDebug   # the README's phone shots
```

Its tests read `android/app/src/test/resources/demo/*.json`, which a Rust
test writes from the demo list. When the wire changes, `cargo test` says
the files are stale: run `WAYBILL_BLESS=1 cargo test -p waybill-core
fixtures`, then the app's tests, then record the screenshots again.

To look at the board without a desktop, render it under GTK's broadway
backend:

```sh
gtk4-broadwayd :7 &
GDK_BACKEND=broadway BROADWAY_DISPLAY=:7 target/debug/waybill-board \
  --demo --shot out.png --tab pulse --open "Playtest the lantern"
```

`--tab` takes `board`, `desk`, `lines`, `log` or `pulse`; `--open` opens the
first card whose title contains the text; `--size 1560x960` sets the frame.
Look at the PNG after any change to a view, a chart or the stylesheet.
`WAYBILL_TRACE=1` prints a line each time the board rereads the list.

## The hero

`python3 docs/assets/hero.py` writes `docs/assets/hero.svg`, the README's
animation. The drawer in it is drawn by `draw_drawer()` above the line that
reads `# ---- end of the panel ----`; dot-files' bar tour
(`docs/bar/apps.py` there) runs that top part to draw the same drawer. So
when the drawer changes in `drawer.rs`, change it in `hero.py` too, keep
the panel's code above the line, and rerun both builds.

## The task list is real

The machine you work on has a real Taskwarrior database that other sessions
are writing to, and a real desktop somebody is using.

- Use `--demo` for anything visual. `waybill_core::demo` builds the same
  made-up list for the same moment, and tests assert against it.
- Try writes against a throwaway list: a taskrc with its own
  `data.location`, a config whose `task.command` is `task rc:<that taskrc>`,
  and `WAYBILL_CONFIG_DIR` and `WAYBILL_RUNTIME_DIR` pointed at temp folders.
  `crates/waybill-core/tests/e2e.rs` does exactly this and is the model.
  `waybill-board --open <title> --do "<words>"` runs a board action without
  a pointer.
- Reads pass `rc.gc=off`, so looking at the list never renumbers it.
- `waybill toggle` opens the drawer on the real screen. Check the drawer
  with `waybill-board --drawer --demo --shot out.png` instead.

## Privacy

Task titles, notes and visit lines are private. Never commit a real export,
a real snapshot or a screenshot of the real list: fixtures are written by
hand, and screenshots in `docs/` come from `--demo`.

## Isolation

`WAYBILL_CONFIG_DIR` points a dev build at another config folder and
`WAYBILL_RUNTIME_DIR` at another folder for claims and the drawer's socket.

## When behavior changes

Update in the same commit:

- `docs/design.md` for a new config key, lane rule, chart or CLI command.
- The README for anything a new user meets (views, keys, install, config).
- `packaging/examples/config.toml` for a new config key.

Commits are conventional, scoped by area: `core`, `cli`, `board`, `drawer`,
`bar`, `anchor`, `charts`, `theme`, `demo`, `android`, `docs`, `ci`.

A change to what a screen shows reaches the phone too: when a view, a card
action or the wire changes, look at the phone's screenshots after
recording them, as you would the board's PNG.

## Platforms

The suite targets Linux, Windows and macOS. `platforms` in
`docs/assets/suite.toml` lists where this app builds today, and `planned`
what a port still owes; CI and the pre-push hook hold it to `platforms`.

- Turn the hook on once per clone: `git config core.hooksPath .githooks`.
  Before a push it runs clippy for every platform listed (needs
  `rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin` and, for
  crates with C code, `mingw-w64-gcc`). A failure there is a broken build on
  that platform: fix it, don't push past it with `--no-verify`.
- Code for one platform goes behind `#[cfg(...)]`, helpers included: a
  function only Linux calls is dead code on Windows, and `-D warnings` fails
  the build there.
- A test that needs something one platform has (a Linux-only binary, Unix
  socket behavior, a POSIX shell, a path layout) is gated with `#[cfg(...)]`
  or skips elsewhere.
