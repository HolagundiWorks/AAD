//! Spacebar-primary command engine (IDLE → COMMAND → INPUT → COMPLETE).
//!
//! Mouse = position · Keyboard = intent · Space = execute.
//! Digits + Space accept typed values; empty Space advances/finishes;
//! Double-Space finishes and repeats the previous command.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Stable command identifier (catalog key).
pub type CommandId = &'static str;

/// Formal engine states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    Idle,
    Command { id: CommandId },
    Input { id: CommandId, step: usize },
    Complete { id: CommandId },
}

/// How empty Space finishes an open command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishRule {
    /// Empty Space finishes once at least `min` points have been accepted.
    EmptySpaceAfterMinPoints(usize),
    /// Last step auto-completes (e.g. rectangle second corner).
    AutoOnLastStep,
    /// Explicit empty Space always finishes (polyline).
    ExplicitSpace,
}

/// One interactive input step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Point {
        prompt: &'static str,
    },
    /// Repeatable point step (line / wall / polyline chain).
    NextPoint {
        prompt: &'static str,
    },
    Distance {
        prompt: &'static str,
    },
    Thickness {
        prompt: &'static str,
    },
    /// Multi-select phase (verb-noun MOVE/COPY/… when selection is empty).
    Selection {
        prompt: &'static str,
    },
    Side {
        prompt: &'static str,
    },
    Text {
        prompt: &'static str,
    },
    Entity {
        prompt: &'static str,
    },
}

/// Static command definition.
#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub id: CommandId,
    pub aliases: &'static [&'static str],
    pub steps: &'static [StepKind],
    pub repeatable: bool,
    pub finish: FinishRule,
    /// Maps to the existing interactive `Tool` bootstrap token (uppercase).
    pub tool_token: &'static str,
}

/// The §33 per-command capability contract (BricsCAD Developer Rules). The
/// engine already provides these uniformly; this makes them explicit and
/// queryable rather than implicit. Derived from a command's step interface
/// (see [`CommandDef::capabilities`]) so it can never drift from behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The command edits the document, so it participates in UNDO/REDO.
    /// (Inquiry commands like LIST/DIST are read-only.)
    pub can_undo: bool,
    /// Enter / double-Space repeats the command (mirrors `repeatable`).
    pub can_repeat: bool,
    /// Shows a rubber-band / hover preview while a spatial step is in progress.
    pub supports_preview: bool,
    /// Spatial picks honor object snap (OSNAP).
    pub supports_snap: bool,
    /// Accepts typed values at the crosshair (dynamic input).
    pub supports_dynamic_input: bool,
    /// Can be driven entirely from the command line / palette.
    pub supports_command_line: bool,
    /// Highlights the pickset during a selection phase.
    pub supports_selection_preview: bool,
}

/// Inquiry commands that only read the model (never undoable).
fn is_query_command(id: CommandId) -> bool {
    matches!(id, "list" | "dist")
}

impl Capabilities {
    /// Human-readable "·"-joined capability tags, for palette/help tooltips.
    pub fn summary(&self) -> String {
        let mut tags: Vec<&str> = Vec::new();
        if self.can_undo {
            tags.push("Undo");
        }
        if self.can_repeat {
            tags.push("Repeat");
        }
        if self.supports_preview {
            tags.push("Preview");
        }
        if self.supports_snap {
            tags.push("Snap");
        }
        if self.supports_dynamic_input {
            tags.push("Dynamic input");
        }
        if self.supports_selection_preview {
            tags.push("Selection preview");
        }
        if self.supports_command_line {
            tags.push("Command line");
        }
        tags.join(" · ")
    }
}

impl CommandDef {
    /// Derive the §33 capability contract from this command's step interface.
    /// Kept derived (not stored per-literal) so the contract can't drift from
    /// the actual steps the engine runs.
    pub fn capabilities(&self) -> Capabilities {
        let has_spatial = self.steps.iter().any(|s| {
            matches!(
                s,
                StepKind::Point { .. }
                    | StepKind::NextPoint { .. }
                    | StepKind::Distance { .. }
                    | StepKind::Thickness { .. }
                    | StepKind::Side { .. }
                    | StepKind::Entity { .. }
            )
        });
        // Snap applies to spatial *picks* (a coordinate or an entity), not to
        // scalar (Thickness) or free-text steps.
        let has_snappable = self.steps.iter().any(|s| {
            matches!(
                s,
                StepKind::Point { .. }
                    | StepKind::NextPoint { .. }
                    | StepKind::Distance { .. }
                    | StepKind::Side { .. }
                    | StepKind::Entity { .. }
            )
        });
        let has_selection = self
            .steps
            .iter()
            .any(|s| matches!(s, StepKind::Selection { .. }));
        Capabilities {
            can_undo: !is_query_command(self.id),
            can_repeat: self.repeatable,
            supports_preview: has_spatial,
            supports_snap: has_snappable,
            // Every step accepts a typed value at the crosshair.
            supports_dynamic_input: !self.steps.is_empty(),
            supports_command_line: true,
            supports_selection_preview: has_selection,
        }
    }
}

/// Result of resolving a typed / palette token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveHit {
    Exact(CommandId),
    Fuzzy(CommandId),
}

/// Detect single Space vs double-Space vs hold-Space.
#[derive(Debug, Clone)]
pub struct SpaceGesture {
    pressed_at: Option<Instant>,
    last_release: Option<Instant>,
    hold_fired: bool,
    pub double_window: Duration,
    pub hold_threshold: Duration,
}

impl Default for SpaceGesture {
    fn default() -> Self {
        Self {
            pressed_at: None,
            last_release: None,
            hold_fired: false,
            double_window: Duration::from_millis(280),
            hold_threshold: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceAction {
    /// Quick tap — confirm / next / accept typed.
    Single,
    /// Two taps within the double window — finish + repeat.
    Double,
    /// Held long enough — open command palette (no Single).
    HoldPalette,
}

impl SpaceGesture {
    pub fn on_press(&mut self, now: Instant) {
        self.pressed_at = Some(now);
        self.hold_fired = false;
    }

    /// Poll while Space is held; returns HoldPalette once.
    pub fn poll_hold(&mut self, now: Instant, still_down: bool) -> Option<SpaceAction> {
        if !still_down {
            return None;
        }
        let t0 = self.pressed_at?;
        if !self.hold_fired && now.duration_since(t0) >= self.hold_threshold {
            self.hold_fired = true;
            return Some(SpaceAction::HoldPalette);
        }
        None
    }

    /// Call on Space release. Returns Single or Double; None if hold already fired.
    pub fn on_release(&mut self, now: Instant) -> Option<SpaceAction> {
        self.pressed_at = None;
        if self.hold_fired {
            self.hold_fired = false;
            return None;
        }
        let is_double = self
            .last_release
            .map(|t| now.duration_since(t) <= self.double_window)
            .unwrap_or(false);
        self.last_release = Some(now);
        Some(if is_double {
            SpaceAction::Double
        } else {
            SpaceAction::Single
        })
    }

    pub fn cancel_press(&mut self) {
        self.pressed_at = None;
        self.hold_fired = false;
    }
}

/// Runtime command engine.
#[derive(Debug, Clone)]
pub struct CommandEngine {
    pub state: EngineState,
    pub previous: Option<CommandId>,
    pub history: VecDeque<CommandId>,
    /// Points accepted in the current command (for FinishRule).
    pub point_count: usize,
    history_cap: usize,
}

impl Default for CommandEngine {
    fn default() -> Self {
        Self {
            state: EngineState::Idle,
            previous: None,
            history: VecDeque::new(),
            point_count: 0,
            history_cap: 8,
        }
    }
}

impl CommandEngine {
    pub fn is_idle(&self) -> bool {
        matches!(self.state, EngineState::Idle)
    }

    pub fn is_active(&self) -> bool {
        !self.is_idle()
    }

    pub fn current_id(&self) -> Option<CommandId> {
        match self.state {
            EngineState::Idle => None,
            EngineState::Command { id }
            | EngineState::Input { id, .. }
            | EngineState::Complete { id } => Some(id),
        }
    }

    pub fn step_index(&self) -> Option<usize> {
        match self.state {
            EngineState::Input { step, .. } => Some(step),
            EngineState::Command { .. } => Some(0),
            _ => None,
        }
    }

    pub fn prompt(&self) -> Option<&'static str> {
        let id = self.current_id()?;
        let def = find_def(id)?;
        let step = self.step_index().unwrap_or(0);
        let kind = def.steps.get(step).or_else(|| def.steps.last())?;
        Some(match kind {
            StepKind::Point { prompt }
            | StepKind::NextPoint { prompt }
            | StepKind::Distance { prompt }
            | StepKind::Thickness { prompt }
            | StepKind::Selection { prompt }
            | StepKind::Side { prompt }
            | StepKind::Text { prompt }
            | StepKind::Entity { prompt } => *prompt,
        })
    }

    pub fn current_step_kind(&self) -> Option<StepKind> {
        let id = self.current_id()?;
        let def = find_def(id)?;
        let step = self.step_index().unwrap_or(0);
        def.steps
            .get(step)
            .copied()
            .or_else(|| def.steps.last().copied())
    }

    /// Start a command by id (after alias resolve).
    pub fn start(&mut self, id: CommandId) -> Option<&'static CommandDef> {
        let def = find_def(id)?;
        self.point_count = 0;
        self.state = EngineState::Command { id: def.id };
        self.state = EngineState::Input {
            id: def.id,
            step: 0,
        };
        self.push_history(def.id);
        Some(def)
    }

    /// Cancel to Idle without updating previous.
    pub fn cancel(&mut self) {
        self.state = EngineState::Idle;
        self.point_count = 0;
    }

    /// Record a point acceptance; may stay on NextPoint step.
    pub fn accept_point(&mut self) {
        self.point_count += 1;
        let Some(id) = self.current_id() else {
            return;
        };
        let Some(def) = find_def(id) else {
            return;
        };
        let step = self.step_index().unwrap_or(0);
        let kind = def.steps.get(step).copied();
        match kind {
            Some(StepKind::NextPoint { .. }) => {
                // stay on this step
            }
            Some(StepKind::Point { .. }) => {
                if step + 1 < def.steps.len() {
                    let next = def.steps[step + 1];
                    if matches!(next, StepKind::NextPoint { .. }) {
                        self.state = EngineState::Input { id, step: step + 1 };
                    } else if matches!(def.finish, FinishRule::AutoOnLastStep)
                        && step + 1 == def.steps.len() - 1
                    {
                        // move to last step; completion happens on next accept
                        self.state = EngineState::Input { id, step: step + 1 };
                    } else {
                        self.state = EngineState::Input { id, step: step + 1 };
                    }
                } else if matches!(def.finish, FinishRule::AutoOnLastStep) {
                    self.complete();
                }
            }
            _ => {}
        }
    }

    /// Advance after accepting a non-point typed value (distance/thickness).
    pub fn advance_step(&mut self) {
        let Some(id) = self.current_id() else {
            return;
        };
        let Some(def) = find_def(id) else {
            return;
        };
        let step = self.step_index().unwrap_or(0);
        if step + 1 < def.steps.len() {
            self.state = EngineState::Input { id, step: step + 1 };
        } else if matches!(def.finish, FinishRule::AutoOnLastStep) {
            self.complete();
        }
    }

    /// Empty Space while active: finish if rule allows; else no-op.
    pub fn empty_space_finish(&mut self) -> bool {
        let Some(id) = self.current_id() else {
            return false;
        };
        let Some(def) = find_def(id) else {
            return false;
        };
        let can = match def.finish {
            FinishRule::EmptySpaceAfterMinPoints(n) => self.point_count >= n,
            FinishRule::ExplicitSpace => self.point_count >= 1,
            FinishRule::AutoOnLastStep => false,
        };
        if can {
            self.complete();
            true
        } else {
            false
        }
    }

    pub fn complete(&mut self) {
        if let Some(id) = self.current_id() {
            self.previous = Some(id);
            self.state = EngineState::Complete { id };
            // Immediately settle to Idle (Complete is instantaneous for GUI).
            self.state = EngineState::Idle;
            self.point_count = 0;
        }
    }

    /// After Double-Space: finish if needed, then restart previous.
    pub fn finish_and_repeat(&mut self) -> Option<&'static CommandDef> {
        if self.is_active() {
            let id = self.current_id();
            self.complete();
            if let Some(id) = id {
                self.previous = Some(id);
            }
        }
        let prev = self.previous?;
        let def = find_def(prev)?;
        if !def.repeatable {
            return None;
        }
        self.start(prev)
    }

    /// Idle Space: restart previous command if any.
    pub fn repeat_previous(&mut self) -> Option<&'static CommandDef> {
        let prev = self.previous?;
        let def = find_def(prev)?;
        if !def.repeatable {
            return None;
        }
        self.start(prev)
    }

    /// Clear current step input (Shift+Space) — reset point count for open chain steps only.
    pub fn previous_input(&mut self) {
        if matches!(
            self.current_step_kind(),
            Some(StepKind::NextPoint { .. } | StepKind::Point { .. })
        ) && self.point_count > 0
        {
            self.point_count -= 1;
        }
    }

    fn push_history(&mut self, id: CommandId) {
        self.history.retain(|x| *x != id);
        self.history.push_front(id);
        while self.history.len() > self.history_cap {
            self.history.pop_back();
        }
    }
}

/// Full command catalog for the Space engine.
pub fn catalog() -> &'static [CommandDef] {
    CATALOG
}

pub fn find_def(id: CommandId) -> Option<&'static CommandDef> {
    CATALOG.iter().find(|c| c.id == id)
}

/// Exact alias match (case-insensitive).
pub fn resolve_exact(token: &str) -> Option<CommandId> {
    let up = token.trim().to_ascii_uppercase();
    if up.is_empty() {
        return None;
    }
    for c in CATALOG {
        if c.id.eq_ignore_ascii_case(&up) {
            return Some(c.id);
        }
        if c.aliases.iter().any(|a| a.eq_ignore_ascii_case(&up)) {
            return Some(c.id);
        }
    }
    None
}

/// Prefix / fuzzy match for palette (`WA` → wall). Prefers shortest alias hit.
pub fn resolve_fuzzy(token: &str) -> Option<ResolveHit> {
    let up = token.trim().to_ascii_uppercase();
    if up.is_empty() {
        return None;
    }
    if let Some(id) = resolve_exact(&up) {
        return Some(ResolveHit::Exact(id));
    }
    let mut best: Option<(CommandId, usize)> = None;
    for c in CATALOG {
        let candidates = std::iter::once(c.id).chain(c.aliases.iter().copied());
        for a in candidates {
            let a_up = a.to_ascii_uppercase();
            if a_up.starts_with(&up) {
                let score = a_up.len();
                if best.map(|(_, s)| score < s).unwrap_or(true) {
                    best = Some((c.id, score));
                }
            }
        }
    }
    best.map(|(id, _)| ResolveHit::Fuzzy(id))
}

/// Resolve a typed command token for Enter / dispatch. Exact id or alias only
/// (AutoCAD-style: `OFF` must not start OFFSET; use `O` or `OFFSET`).
pub fn resolve_alias(token: &str) -> Option<CommandId> {
    resolve_exact(token)
}

/// Whether empty Space should finish this command given point count.
pub fn can_finish(def: &CommandDef, point_count: usize) -> bool {
    match def.finish {
        FinishRule::EmptySpaceAfterMinPoints(n) => point_count >= n,
        FinishRule::ExplicitSpace => point_count >= 1,
        FinishRule::AutoOnLastStep => false,
    }
}

// --- Catalog -----------------------------------------------------------------

const LINE_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "LINE — first point",
    },
    StepKind::NextPoint {
        prompt: "LINE — next point or [U]ndo/[C]lose (Space finish)",
    },
];

const PL_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "POLYLINE — first point",
    },
    StepKind::NextPoint {
        prompt: "POLYLINE — next point or [U]ndo/[C]lose (Space finishes)",
    },
];

const REC_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "RECTANGLE — first corner or [D]imensions",
    },
    StepKind::Point {
        prompt: "RECTANGLE — opposite corner (or @W,H)",
    },
];

const XLINE_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "XLINE — point or [H]or/[V]er/[A]ng",
    },
    StepKind::NextPoint {
        prompt: "XLINE — through point (Space finishes)",
    },
];

const TRIM_STEPS: &[StepKind] = &[StepKind::Entity {
    prompt: "TRIM — select object to trim or [C]utting edges (Space finishes)",
}];

const CIRCLE_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "CIRCLE — center or [2P/3P/D]iameter",
    },
    StepKind::Point {
        prompt: "CIRCLE — radius or [D]iameter (type + Space)",
    },
];

const OFFSET_STEPS: &[StepKind] = &[
    StepKind::Distance {
        prompt: "OFFSET — distance (type + Space)",
    },
    StepKind::Entity {
        prompt: "OFFSET — select object",
    },
    StepKind::Side {
        prompt: "OFFSET — pick side (Space finishes)",
    },
];

const MOVE_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "MOVE — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "MOVE — base point",
    },
    StepKind::Point {
        prompt: "MOVE — destination",
    },
];

const COPY_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "COPY — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "COPY — base point",
    },
    StepKind::Point {
        prompt: "COPY — destination (Space finishes)",
    },
];

const WALL_STEPS: &[StepKind] = &[
    StepKind::Thickness {
        prompt: "WALL — thickness (type + Space)",
    },
    StepKind::Point {
        prompt: "WALL — start point",
    },
    StepKind::NextPoint {
        prompt: "WALL — end / next point (Space finish · Double-Space repeat)",
    },
];

const ROTATE_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "ROTATE — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "ROTATE — base point",
    },
    StepKind::Point {
        prompt: "ROTATE — angle (type degrees or pick point)",
    },
];

const SCALE_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "SCALE — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "SCALE — base point",
    },
    StepKind::Point {
        prompt: "SCALE — factor (type) or [R]eference",
    },
];

const MIRROR_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "MIRROR — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "MIRROR — first axis point",
    },
    StepKind::Point {
        prompt: "MIRROR — second axis point",
    },
];

const ARRAYRECT_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "ARRAYRECT — select objects (Space when done)",
    },
    StepKind::Distance {
        prompt: "ARRAYRECT — rows (type + Space)",
    },
    StepKind::Distance {
        prompt: "ARRAYRECT — columns (type + Space)",
    },
    StepKind::Distance {
        prompt: "ARRAYRECT — column spacing dx (type or pick)",
    },
    StepKind::Distance {
        prompt: "ARRAYRECT — row spacing dy (type or pick)",
    },
];

const ARRAYPOLAR_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "ARRAYPOLAR — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "ARRAYPOLAR — center point",
    },
    StepKind::Distance {
        prompt: "ARRAYPOLAR — count (type + Space)",
    },
    StepKind::Distance {
        prompt: "ARRAYPOLAR — fill angle degrees <360>",
    },
];

const ARRAYPATH_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "ARRAYPATH — select objects (Space when done)",
    },
    StepKind::Entity {
        prompt: "ARRAYPATH — select path (line/pline/arc/circle)",
    },
    StepKind::Distance {
        prompt: "ARRAYPATH — count (type + Space)",
    },
];

const ERASE_STEPS: &[StepKind] = &[StepKind::Selection {
    prompt: "ERASE — select objects (Space erases)",
}];

const LIST_STEPS: &[StepKind] = &[StepKind::Selection {
    prompt: "LIST — reports entities (runs immediately)",
}];

const LENGTHEN_STEPS: &[StepKind] = &[
    StepKind::Entity {
        prompt: "LENGTHEN — select a line or polyline",
    },
    StepKind::Distance {
        prompt: "LENGTHEN — delta length (type + Space)",
    },
];

const DIMSTYLE_STEPS: &[StepKind] = &[StepKind::Text {
    prompt: "DIMSTYLE — opens Dims manager",
}];

const STRETCH_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "STRETCH — select objects (Space when done)",
    },
    StepKind::Point {
        prompt: "STRETCH — first crossing-window corner",
    },
    StepKind::Point {
        prompt: "STRETCH — opposite corner",
    },
    StepKind::Point {
        prompt: "STRETCH — base point",
    },
    StepKind::Point {
        prompt: "STRETCH — destination",
    },
];

const BLOCK_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "BLOCK — select objects (Space when done)",
    },
    StepKind::Text {
        prompt: "BLOCK — name + Space",
    },
    StepKind::Point {
        prompt: "BLOCK — base point",
    },
];

const EXPLODE_STEPS: &[StepKind] = &[StepKind::Selection {
    prompt: "EXPLODE — select inserts (Space when done)",
}];

const MATCHPROP_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "MATCHPROP — click source",
    },
    StepKind::Point {
        prompt: "MATCHPROP — click targets (Esc finishes)",
    },
];

const MLEADER_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "MLEADER — arrowhead",
    },
    StepKind::Point {
        prompt: "MLEADER — landing",
    },
    StepKind::Point {
        prompt: "MLEADER — text point",
    },
    StepKind::Text {
        prompt: "MLEADER — type note + Space",
    },
];

const DIST_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "DIST — first point",
    },
    StepKind::Point {
        prompt: "DIST — second point",
    },
];

const DIMRAD_STEPS: &[StepKind] = &[StepKind::Point {
    prompt: "DIMRADIUS — click a circle or arc",
}];

const DIMDIA_STEPS: &[StepKind] = &[StepKind::Point {
    prompt: "DIMDIAMETER — click a circle or arc",
}];

const DIMANG_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "DIMANGULAR — vertex",
    },
    StepKind::Point {
        prompt: "DIMANGULAR — first ray",
    },
    StepKind::Point {
        prompt: "DIMANGULAR — second ray",
    },
];

const WBLOCK_STEPS: &[StepKind] = &[
    StepKind::Selection {
        prompt: "WBLOCK — select objects (Space when done)",
    },
    StepKind::Text {
        prompt: "WBLOCK — block name + Space",
    },
];

const DIM_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "DIM — first definition point",
    },
    StepKind::Point {
        prompt: "DIM — second definition point",
    },
    StepKind::Point {
        prompt: "DIM — dimension line location",
    },
];

const QDIM_STEPS: &[StepKind] = &[StepKind::Point {
    prompt: "QDIM — click dim-line side (select objects first)",
}];

const DIMCONT_STEPS: &[StepKind] = &[StepKind::Point {
    prompt: "DIMCONTINUE — next definition point (Space finishes)",
}];

const FILLET_STEPS: &[StepKind] = &[
    StepKind::Distance {
        prompt: "FILLET — radius (type + Space, 0 = sharp)",
    },
    StepKind::Entity {
        prompt: "FILLET — select first line",
    },
    StepKind::Entity {
        prompt: "FILLET — select second line",
    },
];

const EXTEND_STEPS: &[StepKind] = &[
    StepKind::Entity {
        prompt: "EXTEND — select boundary edge",
    },
    StepKind::Entity {
        prompt: "EXTEND — select object to extend (Space finishes)",
    },
];

const HATCH_STEPS: &[StepKind] = &[StepKind::Entity {
    prompt: "HATCH — pick a point inside a closed region",
}];

const ARC_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "ARC — start point or [C]enter",
    },
    StepKind::Point {
        prompt: "ARC — second point on arc",
    },
    StepKind::Point {
        prompt: "ARC — end point",
    },
];

const TEXT_STEPS: &[StepKind] = &[
    StepKind::Point {
        prompt: "TEXT — insertion point",
    },
    StepKind::Text {
        prompt: "TEXT — type value + Space",
    },
];

const PLACE_STEPS: &[StepKind] = &[StepKind::Point {
    prompt: "PLACE — click insertion point",
}];

static CATALOG: &[CommandDef] = &[
    CommandDef {
        id: "line",
        aliases: &["L", "LINE"],
        steps: LINE_STEPS,
        repeatable: true,
        finish: FinishRule::EmptySpaceAfterMinPoints(2),
        tool_token: "LINE",
    },
    CommandDef {
        id: "polyline",
        aliases: &["PL", "PLINE", "POLYLINE"],
        steps: PL_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "POLYLINE",
    },
    CommandDef {
        id: "rectangle",
        aliases: &["REC", "RECTANG", "RECTANGLE"],
        steps: REC_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "RECTANGLE",
    },
    CommandDef {
        id: "xline",
        aliases: &["XL", "XLINE"],
        steps: XLINE_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "XLINE",
    },
    CommandDef {
        id: "circle",
        aliases: &["C", "CIRCLE"],
        steps: CIRCLE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "CIRCLE",
    },
    CommandDef {
        id: "arc",
        aliases: &["A", "ARC"],
        steps: ARC_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "ARC",
    },
    CommandDef {
        id: "offset",
        aliases: &["O", "OFFSET"],
        steps: OFFSET_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "OFFSET",
    },
    CommandDef {
        id: "move",
        aliases: &["M", "MOVE"],
        steps: MOVE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "MOVE",
    },
    CommandDef {
        id: "copy",
        aliases: &["CO", "CP", "COPY"],
        steps: COPY_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "COPY",
    },
    CommandDef {
        id: "rotate",
        aliases: &["RO", "ROTATE"],
        steps: ROTATE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "ROTATE",
    },
    CommandDef {
        id: "scale",
        aliases: &["SC", "SCALE"],
        steps: SCALE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "SCALE",
    },
    CommandDef {
        id: "mirror",
        aliases: &["MI", "MIRROR"],
        steps: MIRROR_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "MIRROR",
    },
    CommandDef {
        id: "arrayrect",
        aliases: &["AR", "ARRAY", "ARRAYRECT"],
        steps: ARRAYRECT_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "ARRAYRECT",
    },
    CommandDef {
        id: "arraypolar",
        aliases: &["ARRAYPOLAR", "ARRAYPOL"],
        steps: ARRAYPOLAR_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "ARRAYPOLAR",
    },
    CommandDef {
        id: "arraypath",
        aliases: &["ARRAYPATH", "APATH"],
        steps: ARRAYPATH_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "ARRAYPATH",
    },
    CommandDef {
        id: "erase",
        aliases: &["E", "ERASE"],
        steps: ERASE_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "ERASE",
    },
    CommandDef {
        id: "list",
        aliases: &["LI", "LS", "LIST"],
        steps: LIST_STEPS,
        repeatable: false,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "LIST",
    },
    CommandDef {
        id: "lengthen",
        aliases: &["LE", "LEN", "LENGTHEN"],
        steps: LENGTHEN_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "LENGTHEN",
    },
    CommandDef {
        id: "dimstyle",
        aliases: &["D", "DIMSTYLE"],
        steps: DIMSTYLE_STEPS,
        repeatable: false,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIMSTYLE",
    },
    CommandDef {
        id: "stretch",
        aliases: &["S", "STRETCH"],
        steps: STRETCH_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "STRETCH",
    },
    CommandDef {
        id: "block",
        aliases: &["B", "BLOCK"],
        steps: BLOCK_STEPS,
        repeatable: false,
        finish: FinishRule::ExplicitSpace,
        tool_token: "BLOCK",
    },
    CommandDef {
        id: "wblock",
        aliases: &["W", "WBLOCK"],
        steps: WBLOCK_STEPS,
        repeatable: false,
        finish: FinishRule::ExplicitSpace,
        tool_token: "WBLOCK",
    },
    CommandDef {
        id: "explode",
        aliases: &["X", "EXPLODE"],
        steps: EXPLODE_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "EXPLODE",
    },
    CommandDef {
        id: "matchprop",
        aliases: &["MA", "MATCHPROP"],
        steps: MATCHPROP_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "MATCHPROP",
    },
    CommandDef {
        id: "mleader",
        aliases: &["MLEADER", "LEADER", "MLD"],
        steps: MLEADER_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "MLEADER",
    },
    CommandDef {
        id: "dist",
        aliases: &["DI", "DIST", "DISTANCE"],
        steps: DIST_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIST",
    },
    CommandDef {
        id: "dim",
        aliases: &["DIM", "DIMLINEAR"],
        steps: DIM_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIM",
    },
    CommandDef {
        id: "dimradius",
        aliases: &["DIMRADIUS", "DIMRAD"],
        steps: DIMRAD_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIMRADIUS",
    },
    CommandDef {
        id: "dimdiameter",
        aliases: &["DIMDIAMETER", "DIMDIA"],
        steps: DIMDIA_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIMDIAMETER",
    },
    CommandDef {
        id: "dimangular",
        aliases: &["DIMANGULAR", "DIMANG"],
        steps: DIMANG_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DIMANGULAR",
    },
    CommandDef {
        id: "qdim",
        aliases: &["QDIM", "QUICKDIM"],
        steps: QDIM_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "QDIM",
    },
    CommandDef {
        id: "dimcontinue",
        aliases: &["DIMCONTINUE", "DIMCONT", "DCO"],
        steps: DIMCONT_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "DIMCONTINUE",
    },
    CommandDef {
        id: "fillet",
        aliases: &["F", "FILLET"],
        steps: FILLET_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "FILLET",
    },
    CommandDef {
        id: "trim",
        aliases: &["TR", "TRIM"],
        steps: TRIM_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "TRIM",
    },
    CommandDef {
        id: "extend",
        aliases: &["EX", "EXTEND"],
        steps: EXTEND_STEPS,
        repeatable: true,
        finish: FinishRule::ExplicitSpace,
        tool_token: "EXTEND",
    },
    CommandDef {
        id: "hatch",
        aliases: &["H", "HATCH"],
        steps: HATCH_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "HATCH",
    },
    CommandDef {
        id: "text",
        aliases: &["T", "TEXT", "DTEXT", "DT"],
        steps: TEXT_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "TEXT",
    },
    CommandDef {
        id: "wall",
        aliases: &["WA", "WALL"],
        steps: WALL_STEPS,
        repeatable: true,
        finish: FinishRule::EmptySpaceAfterMinPoints(2),
        tool_token: "WALL",
    },
    // Place tools: seed geometry blocks then INSERT (see ensure_place_blocks).
    CommandDef {
        id: "door",
        aliases: &["DO", "DOOR"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DOOR",
    },
    CommandDef {
        id: "window",
        aliases: &["WI", "WINDOW"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "WINDOW",
    },
    CommandDef {
        id: "column",
        aliases: &["COL", "COLUMN"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "COLUMN",
    },
    CommandDef {
        id: "beam",
        aliases: &["BE", "BEAM"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "BEAM",
    },
    CommandDef {
        id: "slab",
        aliases: &["SL", "SLAB"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "SLAB",
    },
    CommandDef {
        id: "tree",
        aliases: &["TREE"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "TREE",
    },
    CommandDef {
        id: "shrub",
        aliases: &["SHRUB", "BUSH"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "SHRUB",
    },
    CommandDef {
        id: "chair",
        aliases: &["CHAIR"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "CHAIR",
    },
    CommandDef {
        id: "desk",
        aliases: &["DESK"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "DESK",
    },
    CommandDef {
        id: "sofa",
        aliases: &["SOFA", "COUCH"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "SOFA",
    },
    CommandDef {
        id: "bed",
        aliases: &["BED"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "BED",
    },
    CommandDef {
        id: "table",
        aliases: &["FTABLE", "RDTABLE"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "FTABLE",
    },
    CommandDef {
        id: "toilet",
        aliases: &["TOILET", "WC"],
        steps: PLACE_STEPS,
        repeatable: true,
        finish: FinishRule::AutoOnLastStep,
        tool_token: "TOILET",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_contract_is_derived_from_steps() {
        // LINE draws with spatial steps: preview / snap / dynamic-input / undo /
        // repeat / command-line, but has no selection phase.
        let line = find_def("line").unwrap().capabilities();
        assert!(line.can_undo && line.can_repeat);
        assert!(line.supports_preview && line.supports_snap);
        assert!(line.supports_dynamic_input && line.supports_command_line);
        assert!(!line.supports_selection_preview);

        // MOVE is verb-noun, so it has a selection phase to preview.
        let mv = find_def("move").unwrap().capabilities();
        assert!(mv.supports_selection_preview && mv.can_undo);

        // LIST is an inquiry command: read-only (no undo), still command-line.
        let list = find_def("list").unwrap().capabilities();
        assert!(!list.can_undo && list.supports_command_line);

        // §33 invariant: every command is addressable from the command line.
        for def in CATALOG {
            assert!(
                def.capabilities().supports_command_line,
                "{} must be command-line addressable",
                def.id
            );
        }
    }

    /// §34 QA checklist, asserted headlessly for **every** command in the
    /// catalog: starts, cancels, repeats (per `repeatable`), keyboard-line
    /// addressable, capability/step consistency (preview/snap/mixed input), and
    /// undo semantics (mutating vs inquiry). The purely-interactive rows
    /// (mouse-only / keyboard-only end-to-end) are represented by their
    /// structural guarantees — a spatial step for the pointer, a command-line
    /// path for the keyboard, dynamic input for mixed.
    #[test]
    fn qa_checklist_every_command() {
        for def in CATALOG {
            let id = def.id;
            let caps = def.capabilities();

            // — starts: every declared alias resolves back to this command.
            for a in def.aliases {
                assert_eq!(resolve_alias(a), Some(id), "{id}: alias {a} must resolve");
            }

            // — starts: the engine enters an active state with a live prompt.
            let mut eng = CommandEngine::default();
            assert!(eng.start(id).is_some(), "{id}: must start");
            assert!(eng.is_active(), "{id}: active after start");
            if !def.steps.is_empty() {
                assert!(eng.prompt().is_some(), "{id}: active command shows a prompt");
            }

            // — cancels: Esc returns to idle.
            eng.cancel();
            assert!(eng.is_idle(), "{id}: cancel returns to idle");

            // — repeats: complete, then Space-repeat matches `repeatable`.
            eng.start(id);
            eng.complete();
            assert!(eng.is_idle(), "{id}: idle after complete");
            let repeated = eng.repeat_previous();
            if def.repeatable {
                assert!(
                    repeated.is_some() && eng.is_active(),
                    "{id}: repeatable command repeats"
                );
            } else {
                assert!(repeated.is_none(), "{id}: non-repeatable must not repeat");
            }

            // — keyboard: always command-line addressable.
            assert!(caps.supports_command_line, "{id}: command-line addressable");

            // — snap implies a snappable spatial pick step exists.
            if caps.supports_snap {
                assert!(
                    def.steps.iter().any(|s| matches!(
                        s,
                        StepKind::Point { .. }
                            | StepKind::NextPoint { .. }
                            | StepKind::Distance { .. }
                            | StepKind::Side { .. }
                            | StepKind::Entity { .. }
                    )),
                    "{id}: snap implies a spatial pick step"
                );
            }

            // — mixed input: any step accepts a typed value at the crosshair.
            if !def.steps.is_empty() {
                assert!(caps.supports_dynamic_input, "{id}: steps accept typed input");
            }

            // — undo/redo: mutating commands undo; inquiry commands do not.
            assert_eq!(
                caps.can_undo,
                !is_query_command(id),
                "{id}: undo matches mutation"
            );
        }
    }

    #[test]
    fn resolve_exact_and_fuzzy() {
        assert_eq!(resolve_alias("L"), Some("line"));
        assert_eq!(resolve_alias("WA"), Some("wall"));
        assert_eq!(resolve_alias("DO"), Some("door"));
        assert_eq!(resolve_alias("CO"), Some("copy")); // not circle
        assert_eq!(resolve_alias("C"), Some("circle"));
        assert_eq!(resolve_alias("O"), Some("offset"));
        assert_eq!(resolve_alias("W"), Some("wblock"));
        assert_eq!(resolve_alias("S"), Some("stretch"));
        assert_eq!(resolve_alias("LI"), Some("list"));
        assert_eq!(resolve_alias("LS"), Some("list"));
        assert_eq!(resolve_alias("LE"), Some("lengthen"));
        assert_eq!(resolve_alias("D"), Some("dimstyle"));
        assert!(resolve_alias("OFF").is_none());
        assert!(matches!(
            resolve_fuzzy("OFF"),
            Some(ResolveHit::Fuzzy("offset"))
        ));
        assert_eq!(resolve_alias("DI"), Some("dist"));
        assert_eq!(resolve_alias("DISTANCE"), Some("dist"));
        assert_eq!(resolve_alias("B"), Some("block"));
        assert_eq!(resolve_alias("X"), Some("explode"));
        assert_eq!(resolve_alias("MA"), Some("matchprop"));
        assert_eq!(resolve_alias("DT"), Some("text"));
        assert_eq!(resolve_alias("DIMRADIUS"), Some("dimradius"));
        assert_eq!(resolve_alias("MLEADER"), Some("mleader"));
        // Fuzzy still helps palette prefixes (exact aliases already covered above).
        assert!(matches!(
            resolve_fuzzy("POLY"),
            Some(ResolveHit::Fuzzy("polyline"))
        ));
    }

    #[test]
    fn line_finish_rule() {
        let def = find_def("line").unwrap();
        assert!(!can_finish(def, 1));
        assert!(can_finish(def, 2));
    }

    #[test]
    fn engine_line_space_finish_and_repeat() {
        let mut e = CommandEngine::default();
        e.start("line").unwrap();
        e.accept_point();
        e.accept_point();
        assert!(e.empty_space_finish());
        assert!(e.is_idle());
        assert_eq!(e.previous, Some("line"));
        let def = e.finish_and_repeat().unwrap();
        assert_eq!(def.id, "line");
        assert!(!e.is_idle());
    }

    #[test]
    fn offset_distance_advances_step() {
        let mut e = CommandEngine::default();
        e.start("offset").unwrap();
        assert!(matches!(
            e.current_step_kind(),
            Some(StepKind::Distance { .. })
        ));
        e.advance_step();
        assert!(matches!(
            e.current_step_kind(),
            Some(StepKind::Entity { .. })
        ));
    }

    #[test]
    fn space_gesture_double() {
        let mut g = SpaceGesture::default();
        let t0 = Instant::now();
        g.on_press(t0);
        assert_eq!(
            g.on_release(t0 + Duration::from_millis(20)),
            Some(SpaceAction::Single)
        );
        g.on_press(t0 + Duration::from_millis(40));
        assert_eq!(
            g.on_release(t0 + Duration::from_millis(60)),
            Some(SpaceAction::Double)
        );
    }

    #[test]
    fn space_gesture_hold_palette() {
        let mut g = SpaceGesture::default();
        let t0 = Instant::now();
        g.on_press(t0);
        assert_eq!(
            g.poll_hold(t0 + Duration::from_millis(260), true),
            Some(SpaceAction::HoldPalette)
        );
        assert_eq!(g.on_release(t0 + Duration::from_millis(300)), None);
    }
}
