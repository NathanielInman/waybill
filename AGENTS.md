# Waybill

`docs/design.md` is the spec; the README is the tour. Read the spec section
for whatever you change.

## Crates

- `waybill-core`: the model. `task.rs` reads what `task export` prints,
  `model.rs` sorts tasks into lanes and works out what each one frees,
  `lines.rs` lays dependency chains out as rail lines, `pulse.rs` counts what
  the charts draw, `note.rs` reads a note far enough to lay it out, `demo.rs`
  builds a made-up list. Plain data in, plain data out: no UI, no clock of
  its own (callers pass `now` and a `Zone`), and nothing that writes to the
  task database. A new rule about where a task sits belongs in `model.rs`
  with a test.
- `waybill`: the CLI (one binary, `waybill`).
- `waybill-board`: the GTK4 window. `shell.rs` is the frame, `views/` holds
  one file per view, `detail.rs` is the card pane, and everything drawn with
  cairo (charts, rail lines, the marks on cards) is in `charts.rs`.
  `theme.rs` owns the palette, which feeds both the CSS and the charts.

## Build and test

`cargo build --workspace --locked`, `cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --all -- --check`. CI runs all four. The board needs GTK 4.12 or
newer.

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

## The task list is real

The machine you work on has a real Taskwarrior database that other sessions
are writing to.

- Use `--demo` for anything visual. `waybill_core::demo` builds the same
  made-up list for the same moment, and tests assert against it.
- Nothing in this repo writes to the task database yet. When that changes
  (stop 2), every write goes through the `task` binary, and tests run it
  against a throwaway `TASKDATA`, never the user's.
- Reads pass `rc.gc=off`, so looking at the list never renumbers it.

## Privacy

Task titles, notes and visit lines are private. Never commit a real export,
a real snapshot or a screenshot of the real list: fixtures are written by
hand, and screenshots in `docs/` come from `--demo`.

## Isolation

`WAYBILL_CONFIG_DIR` points a dev build at another config folder.

## When behavior changes

Update in the same commit:

- `docs/design.md` for a new config key, lane rule, chart or CLI command.
- The README for anything a new user meets (views, keys, install, config).
- `packaging/examples/config.toml` for a new config key.

Commits are conventional, scoped by area: `core`, `cli`, `board`, `charts`,
`theme`, `demo`, `docs`, `ci`.
