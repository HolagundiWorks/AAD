//! Cursor overlay drawn on top of the drawing: the command preview, grips, the snap marker,
//! the selection window and the crosshair.
//!
//! These are screen-space strokes in world coordinates, packed with the same mesh as the drawing.

use aad_color::Rgb;
use aad_engine::Session;
use aad_engine::doc::{Entity, Handle};
use aad_engine::grips;
use aad_geom::Vec2;
use aad_render::mesh::{self, MeshData};
use aad_render::{DPrim, DisplayList, Kind};

/// A grip the pointer has picked and not yet placed.
#[derive(Clone, Copy)]
pub struct HotGrip {
    pub handle: Handle,
    pub index: usize,
    pub base: Vec2,
    pub mode: grips::GripMode,
}

impl HotGrip {
    pub fn label(self) -> &'static str {
        use grips::GripMode::*;
        match self.mode {
            Stretch => "** STRETCH **",
            Move => "** MOVE **",
            Rotate => "** ROTATE **",
            Scale => "** SCALE **",
            Mirror => "** MIRROR **",
        }
    }

    pub fn next_mode(&mut self) {
        use grips::GripMode::*;
        self.mode = match self.mode {
            Stretch => Move,
            Move => Rotate,
            Rotate => Scale,
            Scale => Mirror,
            Mirror => Stretch,
        };
    }
}

/// What the pointer is doing, besides the drawing itself.
pub struct Cursor {
    pub aim: Vec2,
    pub raw: Option<Vec2>,
    pub snap: Option<Vec2>,
    pub hot: Option<HotGrip>,
}

pub fn mesh(
    session: &Session,
    cursor: Option<&Cursor>,
    paper: Rgb,
    height: u32,
    origin: Vec2,
    tip: Option<&str>,
    width: u32,
    show_ucs: bool,
    icon_px: f64,
) -> MeshData {
    let Ok(state) = session.state() else { return MeshData::default() };
    let view = state.view();
    let scale = f64::from(height.max(1)) / view.height.max(1e-12);
    let mut list = DisplayList::default();
    if let Some(cursor) = cursor {
        if let Some(hot) = cursor.hot {
            if let Some(entity) = state.doc.entity(hot.handle)
                && let Some(kind) = grip_ghost(&entity.kind, hot, cursor.aim)
            {
                let ghost = Entity { handle: Handle(0), common: entity.common.clone(), kind };
                list = aad_render::build_entities(
                    &state.doc,
                    std::iter::once(&ghost),
                    &aad_render::Options {
                        tolerance: 0.5 / scale,
                        min_dash: 2.0 / scale,
                        text: false,
                        fill: false,
                        lineweights: false,
                        view_height: view.height,
                        ..Default::default()
                    },
                );
            }
            add_poly(&mut list, &[hot.base, cursor.aim], Rgb(255, 255, 255), true);
        } else if session.running.is_some() {
            let ents = session.preview(cursor.aim);
            if !ents.is_empty() {
                list = aad_render::build_entities(
                    &state.doc,
                    ents.iter(),
                    &aad_render::Options {
                        tolerance: 0.5 / scale,
                        min_dash: 2.0 / scale,
                        text: true,
                        fill: true,
                        lineweights: false,
                        view_height: view.height,
                        ..Default::default()
                    },
                );
            }
        }
        if let (Some(pw), Some(raw)) = (session.pending_window, cursor.raw) {
            let crossing = raw.x < pw.corner.x;
            let color = if crossing { Rgb(80, 220, 120) } else { Rgb(80, 160, 255) };
            let a = pw.corner;
            let b = raw;
            add_poly(
                &mut list,
                &[Vec2::new(a.x, a.y), Vec2::new(b.x, a.y), Vec2::new(b.x, b.y), Vec2::new(a.x, b.y), Vec2::new(a.x, a.y)],
                color,
                false,
            );
        }
        if session.running.is_none() {
            let half = session.settings.gripsize.max(1.0) / scale;
            for handle in session.selection().iter().take(200) {
                let Some(entity) = state.doc.entity(*handle) else { continue };
                if state.doc.layer(&entity.common.layer).is_some_and(|layer| layer.locked) {
                    continue;
                }
                for (index, grip) in entity.kind.grips().into_iter().enumerate() {
                    let hot = cursor.hot.is_some_and(|h| h.handle == *handle && h.index == index);
                    add_poly(&mut list, &square(grip, half), if hot { Rgb(255, 40, 40) } else { Rgb(0, 120, 255) }, false);
                }
            }
        }
        if let Some(at) = cursor.snap {
            add_poly(&mut list, &square(at, 6.0 / scale), Rgb(0, 220, 80), false);
        }
        add_crosshair(&mut list, cursor.aim, scale, height, session.settings.cursorsize, session.picking_objects(), session.settings.pickbox);
        if let Some(tip) = tip {
            add_tip(&mut list, cursor.aim, scale, tip);
        }
    }
    if show_ucs {
        add_ucs(&mut list, view.center, width, height, scale, icon_px);
    }
    let mut built = mesh::build_mesh(&list, origin, paper, false);
    mesh::expand_hairlines(&mut built);
    built
}

fn grip_ghost(kind: &aad_engine::doc::EntityKind, hot: HotGrip, aim: Vec2) -> Option<aad_engine::doc::EntityKind> {
    if let Some(matrix) = grips::mode_matrix(hot.mode, hot.base, aim) {
        let mut ghost = kind.clone();
        ghost.transform(&matrix);
        return Some(ghost);
    }
    if hot.mode == grips::GripMode::Stretch { grips::stretch_grip(kind, hot.index, aim) } else { None }
}

fn add_ucs(list: &mut DisplayList, center: Vec2, width: u32, height: u32, scale: f64, icon_px: f64) {
    if !center.is_finite() || !scale.is_finite() || scale <= 0.0 || !icon_px.is_finite() || icon_px <= 0.0 {
        return;
    }
    let px = |n: f64| n * icon_px;
    let to_world = |sx: f64, sy: f64| {
        let cx = f64::from(width.max(1)) * 0.5;
        let cy = f64::from(height.max(1)) * 0.5;
        Vec2::new(center.x + (sx - cx) / scale, center.y - (sy - cy) / scale)
    };
    let origin = to_world(px(36.0), f64::from(height) - px(34.0));
    let axis_x = to_world(px(36.0) + px(60.0), f64::from(height) - px(34.0));
    let axis_y = to_world(px(36.0), f64::from(height) - px(34.0) - px(60.0));
    if ![origin, axis_x, axis_y].into_iter().all(|point| point.is_finite()) {
        return;
    }
    let ink = Rgb(255, 255, 255);
    add_poly(list, &[origin, axis_x], ink, true);
    add_poly(list, &[origin, axis_y], ink, true);
    add_poly(list, &square(origin, px(4.5) / scale), ink, true);
    add_label(list, to_world(px(36.0) + px(70.0), f64::from(height) - px(34.0)), scale, icon_px, "X");
    add_label(list, to_world(px(36.0), f64::from(height) - px(34.0) - px(70.0)), scale, icon_px, "Y");
}

fn add_label(list: &mut DisplayList, at: Vec2, scale: f64, icon_px: f64, text: &str) {
    if !at.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let height = 13.0 * icon_px / scale;
    let shaped = aad_fonts::shape_stroke(text, height, 1.0, 0.0);
    for stroke in &shaped.strokes {
        let pts: Vec<Vec2> = stroke.iter().map(|point| at + *point).collect();
        add_poly(list, &pts, Rgb(255, 255, 255), true);
    }
}

fn add_crosshair(list: &mut DisplayList, at: Vec2, scale: f64, height: u32, cursorsize: f64, picking: bool, pickbox: f64) {
    if !at.is_finite() || scale <= 0.0 {
        return;
    }
    let len = (f64::from(height) * cursorsize.max(1.0) / 100.0).max(12.0) / scale;
    let gap = if picking { (pickbox.max(0.0) * 1.5) / scale } else { 0.0 };
    let x = |dx: f64| Vec2::new(at.x + dx, at.y);
    let y = |dy: f64| Vec2::new(at.x, at.y + dy);
    add_poly(list, &[x(-len), x(-gap)], Rgb(255, 255, 255), true);
    add_poly(list, &[x(gap), x(len)], Rgb(255, 255, 255), true);
    add_poly(list, &[y(-len), y(-gap)], Rgb(255, 255, 255), true);
    add_poly(list, &[y(gap), y(len)], Rgb(255, 255, 255), true);
    if gap > 0.0 {
        add_poly(list, &square(at, gap), Rgb(255, 255, 255), true);
    }
}

fn square(c: Vec2, half: f64) -> [Vec2; 5] {
    [
        Vec2::new(c.x - half, c.y - half),
        Vec2::new(c.x + half, c.y - half),
        Vec2::new(c.x + half, c.y + half),
        Vec2::new(c.x - half, c.y + half),
        Vec2::new(c.x - half, c.y - half),
    ]
}

fn add_tip(list: &mut DisplayList, at: Vec2, scale: f64, text: &str) {
    if !at.is_finite() || !scale.is_finite() || scale <= 0.0 || text.is_empty() {
        return;
    }
    let height = 16.0 / scale;
    let shaped = aad_fonts::shape_stroke(text, height, 1.0, 0.0);
    let pad = 6.0 / scale;
    let origin = Vec2::new(at.x + 22.0 / scale, at.y + 36.0 / scale);
    let x0 = origin.x - pad;
    let y0 = origin.y - pad;
    let x1 = origin.x + shaped.width + pad;
    let y1 = origin.y + height + pad;
    if ![x0, y0, x1, y1].into_iter().all(f64::is_finite) {
        return;
    }
    add_mask(list, &[Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)]);
    for stroke in &shaped.strokes {
        let pts: Vec<Vec2> = stroke.iter().map(|point| origin + *point).collect();
        add_poly(list, &pts, Rgb(255, 255, 255), true);
    }
}

fn add_mask(list: &mut DisplayList, corners: &[Vec2; 4]) {
    let Ok(start) = u32::try_from(list.tris.len()) else { return };
    list.tris.extend_from_slice(&[corners[0], corners[1], corners[2], corners[0], corners[2], corners[3]]);
    list.prims.push(DPrim { handle: Handle(0), color: Rgb(0, 0, 0), aci7: false, lw: 0.0, kind: Kind::Mask, start, len: 6 });
}

fn add_poly(list: &mut DisplayList, pts: &[Vec2], color: Rgb, aci7: bool) {
    if pts.len() < 2 {
        return;
    }
    let Ok(start) = u32::try_from(list.verts.len()) else { return };
    let Ok(len) = u32::try_from(pts.len()) else { return };
    list.verts.extend_from_slice(pts);
    list.prims.push(DPrim { handle: Handle(0), color, aci7, lw: 0.0, kind: Kind::Polyline, start, len });
}
