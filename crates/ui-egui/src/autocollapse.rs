//! Pinning and auto-collapse of the side panels.
//!
//! The panels are either *pinned* (always as the docking layout has them) or *unpinned*, in which
//! case they collapse after the pointer has been away from them, and no edit is in progress, for
//! the configured delay. A collapsed panel set is brought back by moving the pointer to the thin
//! edge strip at the side of the drawing area, or by the user opening a panel. The decision is the
//! pure function [`step`]; [`run`] feeds it from the real pointer, focus and popup state and applies
//! the result to `UiState::show_palettes`, so the docking layout and the canvas size follow.
//!
//! What counts as activity: the pointer over a side panel, a drag in progress, a popup or dropdown
//! open, or keyboard focus in a widget that lies in a side panel (a text field being edited).
//! Any of these keeps the panels open.

use egui::{Pos2, Rect};

use crate::CadApp;

/// Width of the strip that re-opens collapsed panels.
pub const EDGE_STRIP: f32 = 8.0;

/// What the panels are doing between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollapseState {
    /// When the panels last saw activity (milliseconds).
    pub last_active: f64,
    /// The panels were hidden by auto-collapse (not by the user), so the edge strips show.
    pub auto_collapsed: bool,
}

/// What the pointer, focus and popups say this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub pointer_in_panels: bool,
    pub pointer_in_edge: bool,
    /// A dropdown or popup is open, a drag is running, or a side-panel widget has keyboard focus.
    pub busy: bool,
}

/// A change to make to the panels' visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Collapse,
    Expand,
}

/// Advance the state machine one frame. `shown` is whether the side panels are currently visible.
pub fn step(pinned: bool, delay_ms: f64, st: &mut CollapseState, shown: bool, a: Activity, now: f64) -> Option<Change> {
    if shown {
        // The user (or a pin) opened them: they are no longer "auto-collapsed".
        let was_auto = st.auto_collapsed;
        st.auto_collapsed = false;
        if pinned || a.pointer_in_panels || a.busy || was_auto {
            st.last_active = now;
            return None;
        }
        if now - st.last_active >= delay_ms {
            st.auto_collapsed = true;
            return Some(Change::Collapse);
        }
        return None;
    }
    if st.auto_collapsed && (pinned || a.pointer_in_edge) {
        st.auto_collapsed = false;
        st.last_active = now;
        return Some(Change::Expand);
    }
    None
}

/// Per frame: read the real activity, advance the machine and apply the change.
pub fn run(app: &mut CadApp, ctx: &egui::Context, dock: Rect) {
    use crate::WorkspaceMode;
    if app.ui.workspace != WorkspaceMode::Classic {
        app.collapse = CollapseState::default();
        return;
    }
    let canvas = app.canvas.rect.unwrap_or(dock);
    let pointer: Option<Pos2> = ctx.input(|i| i.pointer.hover_pos());
    let in_dock = pointer.is_some_and(|p| dock.contains(p));
    let shown = app.ui.show_palettes;
    let in_panels = in_dock && pointer.is_some_and(|p| p.x < canvas.left() || p.x > canvas.right());
    let in_edge = in_dock
        && pointer.is_some_and(|p| (p.x - dock.left()).abs() <= EDGE_STRIP || (dock.right() - p.x).abs() <= EDGE_STRIP)
        && (!shown || in_panels);
    let dragging = ctx.input(|i| i.pointer.any_down()) && in_panels;
    let focus_in_panel = ctx
        .memory(|m| m.focused())
        .and_then(|id| ctx.read_response(id))
        .is_some_and(|r| r.rect.right() < canvas.left() || r.rect.left() > canvas.right());
    let busy = dragging || egui::Popup::is_any_open(ctx) || focus_in_panel;
    let activity = Activity { pointer_in_panels: in_panels, pointer_in_edge: in_edge, busy };
    let delay = f64::from(app.ui.auto_collapse_ms);
    let mut st = app.collapse;
    let change = step(app.ui.pin_panels, delay, &mut st, shown, activity, crate::now_ms());
    app.collapse = st;
    match change {
        Some(Change::Collapse) => app.ui.show_palettes = false,
        Some(Change::Expand) => app.ui.show_palettes = true,
        None => {}
    }
    // The strips that bring the panels back, drawn on the sides of the drawing area.
    if !app.ui.show_palettes && app.collapse.auto_collapsed {
        let t = crate::theme::Tokens::get();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("aad_collapse_strips")));
        for side in [dock.left(), dock.right() - EDGE_STRIP] {
            let r = Rect::from_min_size(egui::pos2(side, dock.top()), egui::vec2(EDGE_STRIP, dock.height()));
            painter.rect_filled(r, 0.0, t.chrome.gamma_multiply(0.85));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    } else if app.ui.show_palettes && !app.ui.pin_panels {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: f64 = 1000.0;

    fn idle() -> Activity {
        Activity::default()
    }

    #[test]
    fn pinned_panels_never_collapse() {
        let mut st = CollapseState::default();
        assert_eq!(step(true, DELAY, &mut st, true, idle(), 0.0), None);
        assert_eq!(step(true, DELAY, &mut st, true, idle(), 60_000.0), None);
    }

    #[test]
    fn unpinned_panels_collapse_after_the_delay_away_from_the_pointer() {
        let mut st = CollapseState::default();
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 0.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 999.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 1000.0), Some(Change::Collapse));
        assert!(st.auto_collapsed);
    }

    #[test]
    fn the_pointer_or_an_active_edit_keeps_them_open() {
        let mut st = CollapseState::default();
        let over = Activity { pointer_in_panels: true, ..Activity::default() };
        let busy = Activity { busy: true, ..Activity::default() };
        for t in [0.0, 500.0, 5000.0, 9000.0] {
            assert_eq!(step(false, DELAY, &mut st, true, over, t), None);
        }
        // The pointer leaves while a field is being edited: still open, and the clock restarts.
        assert_eq!(step(false, DELAY, &mut st, true, busy, 20_000.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 20_500.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 21_000.0), Some(Change::Collapse));
    }

    #[test]
    fn the_edge_strip_brings_them_back_without_a_collapse_loop() {
        let mut st = CollapseState::default();
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 1000.0), Some(Change::Collapse));
        // Hovering elsewhere does nothing; the strip expands them.
        assert_eq!(step(false, DELAY, &mut st, false, idle(), 1100.0), None);
        let edge = Activity { pointer_in_edge: true, ..Activity::default() };
        assert_eq!(step(false, DELAY, &mut st, false, edge, 1200.0), Some(Change::Expand));
        // Expanded with the pointer still near the edge but not yet in the panels: not collapsed
        // within the delay, so there is no flicker.
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 1210.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 2100.0), None);
        assert_eq!(step(false, DELAY, &mut st, true, idle(), 2300.0), Some(Change::Collapse));
    }

    #[test]
    fn panels_the_user_hid_stay_hidden_and_pinning_restores_collapsed_ones() {
        let mut st = CollapseState::default();
        let edge = Activity { pointer_in_edge: true, ..Activity::default() };
        assert_eq!(step(false, DELAY, &mut st, false, edge, 0.0), None, "hidden by the user: no strip, no expansion");
        st.auto_collapsed = true;
        assert_eq!(step(true, DELAY, &mut st, false, idle(), 10.0), Some(Change::Expand), "pinning brings them back");
    }
}
