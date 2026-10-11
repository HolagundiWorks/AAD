# Widgets: Calculator and Pomodoro timer

Two small floating windows, opened from **Tools** (`ui.toggle.calculator`, `ui.toggle.pomodoro`).
Both are thin: the logic is in the engine (`crates/engine/src/calc.rs`, `pomodoro.rs`), the
windows in `crates/ui-egui/src/widgets.rs` only draw it. Buttons send the same commands the
control channel and tests use (`ui.calculator`, `ui.pomodoro`). Whether a window is open is saved
with the interface preferences; the calculator's contents and the timer are not.

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
- Typing: the keypad, or the keyboard while the pointer is over the window (digits, operators,
  `'` `"`, Enter for `=`, Backspace, Escape to clear). **Copy result** copies the display.

Command line and scripts: `cal` (aliases `calc`, `qc`, `quickcalc`) takes `{expr}` and returns
`kind`, `metres`, `squareMetres`, `cubicMetres` and the formatted `results`. It never touches the
drawing. `ui.calculator` takes `{keys}` or `{action: clear|backspace|equals}` and returns the
display.

Limits: expressions are at most 512 characters and 64 levels deep; division by zero and results
that are not finite are errors, never panics. There are no functions (`sin`, `sqrt`) and no
variables.

## Pomodoro timer

A focus timer on a dial. 25 minutes by default; the block is 5 to 60 minutes in 5-minute steps.

- Click the dial or **Start** to start or pause; double click or **Reset** to go back to a full
  block; **−5** and **+5** change the length while stopped.
- Drag around the dial while stopped to set the length; it snaps to 5 minutes on release.
- The timer keeps counting when its window is closed. When the block ends the window opens and
  the status bar says so. There is no sound.

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
