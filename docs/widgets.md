# Widgets: Calculator and Pomodoro timer

Two small widgets, opened from **Tools** (`ui.toggle.calculator`, `ui.toggle.pomodoro`).
Both are thin: the logic is in the engine (`crates/engine/src/calc.rs`, `pomodoro.rs`);
the code in `crates/ui-egui/src/widgets.rs` only draws it and sends the same commands the control
channel and tests use (`ui.calculator`, `ui.pomodoro`). Whether a widget is shown is saved with the
interface preferences; the calculator's contents and the timer are not.

## Calculator

The architect calculator (from the HC Works Architect Calculator). It replaces the earlier
expression calculator (`cal` with trigonometry and variables), which was removed.

- Expressions with `+ − × ÷ ^`, parentheses and operator precedence.
- Feet and inches: `10' 6"`, `10'`, `6"`, `5' - 3"`.
- The number of factors decides what is measured: one is a length, two an area, three or more a
  volume. `10' 6" x 20'` is 210.00 ft² (19.51 m²); `3 x 4 x 5` is 60.00 m³ (2,118.88 ft³);
  `(2+3) x 4` is 20.00 m².
- Metres are the working unit; the calculator does not read the drawing's units. A result is
  shown in imperial when the expression used feet or inches, and the six figures (m, ft, m², ft²,
  m³, ft³) are always listed under the display.
- Typing: there is no on-screen keypad. The window is a field with the cursor already in it; type the
  expression on the keyboard and the answer shows under it as you type. Enter evaluates (`3 x 4`
  becomes `12.00 m²`), Escape clears. **Copy result** copies the display.

Command line and scripts: `cal` (aliases `calc`, `qc`, `quickcalc`) takes `{expr}` and returns
`kind`, `metres`, `squareMetres`, `cubicMetres` and the formatted `results`. It never touches the
drawing. `ui.calculator` takes `{keys}` (typed as if on the keyboard) or `{action: clear|backspace|equals}` and returns the
display.

Limits: expressions are at most 512 characters and 64 levels deep; division by zero and results
that are not finite are errors, never panics. There are no functions (`sin`, `sqrt`) and no
variables.

## Pomodoro timer

A focus timer drawn on the canvas as a dial, at its bottom-right corner (where a view cube would
sit), half transparent (more opaque while the pointer is on it). It is part of the canvas, so
docked panels never overlap it and anything that floats above the drawing (a panel, a dialog, a
menu) covers it. It has no buttons.

- 25 minutes by default; a block is 5 to 60 minutes in 5-minute steps.
- **Click the dial** to start or stop it.
- **Drag around the dial** while it is stopped to set the time; it snaps to 5 minutes on release.
- The status bar shows the countdown (and a bar that empties) even when the dial is hidden; click
  it, or use Tools → Pomodoro Timer, to show or hide the dial.
- The timer keeps counting while the dial is hidden. When the block ends the dial appears and the
  status bar says so. There is no sound.

`ui.pomodoro` takes `{action: toggle|start|pause|reset|status}` or `{action: "set", minutes}` and
returns `label`, `running`, `left` (seconds) and `minutes`.

## Provenance

- The calculator is a Rust port of the Architect Calculator Android app
  (`HolagundiWorks/architect-calculator`, by the same author as this project).
- The timer's behaviour (25-minute default, 5 to 60 minute dial, click and double click) follows
  the Pomodoro clock in the AQC desktop program (`HolagundiWorks/AQC`, AGPL-3.0). The Rust code
  here was written from that behaviour and shares no source with it.

## Tests

`cargo test -p cadcraft-engine --lib calc pomodoro`, `cargo test -p cadcraft-engine --test calculator`
and `cargo test -p cadcraft-ui-egui --test widgets`.
