# Input forms

Most architecture commands take JSON. A menu click has none, so each command that needs input has
a form declared as data in `crates/engine/src/forms.rs` (`FORMS`). One declaration serves three
callers:

- **Dialog** (menu and toolbar): the settings are shown in a dialog drawn by
  `crates/ui-egui/src/formdialog.rs`: project details, floors and heights (a table), beam depths,
  the opening schedule (a table), rates (a table), room type and options, wall thickness, stair
  rise and width, take-off options, and so on. OK validates the values and shows the reason if one
  is wrong. If the command also needs points, they are picked on the drawing afterwards.
- **Command line**: typing a command that has a form (and no prompts of its own) asks the missing
  values one by one, with points picked on the drawing. Enter skips an optional question; Escape
  cancels. A bad answer keeps the question.
- **Scripts, control channel, agent**: unchanged. They call the command with JSON and never see a
  dialog.

Both paths end in the same call, `forms::parse_input` turns the collected text into the command's
parameters, and a refusal from the command is shown and ends the command. Numbers must be finite;
tables are limited to 500 rows and point lists to 10 000 points.

Forms exist for: `project`, `floors`, `beams`, `schedule`, `takeoff.rates`, `room`, `hcwlayers` (choices: standard, permit, presentation, both, all),
`takeoff.linear|brick|beams|columns|rects|paint|ceiling|floor|area|slab`, `wall.draw`, `wall.door`,
`wall.window`, `grid.axis`, `column.place`, `lintel.draw`, `stair.plan`, `lift.shaft`, `rail.draw`,
`level.mark`, `section.marker`, `elevation.marker`, `north.arrow`, `slope.arrow`,
`electrical.connect|number|schedule` and `bridge.export`. `bridge.import` has its own file picker
and preview window ([hcw-kit-port.md](hcw-kit-port.md)).

Adding a form for a new command is one entry in `FORMS`; `forms::tests` checks that every form
names a real command and that field keys are unique.

Limits: the dialog translates field labels only when the interface string catalog has them (the
catalog does not yet cover the form labels, so they show in English); `schedule`'s dialog edits
the whole opening list, not one entry.
