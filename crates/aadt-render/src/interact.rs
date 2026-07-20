//! Interactive camera control — the "brains" of the canvas, kept pure.
//!
//! Pan (drag) and zoom (scroll about the cursor) are just transforms on a
//! [`Camera`], so they live here as a plain state machine with **no windowing or
//! GPU dependency**. That is deliberate: the input→camera behaviour is the part
//! worth testing, and separating it from `winit`/`wgpu` lets it be unit-tested
//! headlessly. The windowed [`view`](crate::view) layer is then a thin shell that
//! feeds real events in and paints the result.

use crate::Camera;
use aadt_geometry::Point2;
use aadt_model::Document;

/// Drives a [`Camera`] from pointer input. Hold one per view.
#[derive(Debug, Clone)]
pub struct Interactor {
    camera: Camera,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    min_scale: f64,
    max_scale: f64,
}

impl Interactor {
    /// Wrap a camera. Zoom is clamped to a wide but finite range so a runaway
    /// scroll can neither collapse the scale to zero nor overflow it.
    pub fn new(camera: Camera) -> Self {
        Self {
            camera,
            dragging: false,
            last_cursor: None,
            min_scale: 1e-6,
            max_scale: 1e9,
        }
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Frame the whole drawing (zoom-to-extents).
    pub fn fit_document(&mut self, doc: &Document, margin: f64) {
        self.camera.fit_document(doc, margin);
    }

    /// The viewport was resized; keep the world centre fixed.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.camera.viewport = (width.max(1.0), height.max(1.0));
    }

    /// Primary button pressed at the current cursor — begin a pan.
    pub fn press(&mut self) {
        self.dragging = true;
    }

    /// Primary button released — end the pan.
    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// The cursor moved to `(x, y)` in screen pixels. While dragging, the world
    /// point under the cursor is held fixed (grab-and-drag), so the drawing
    /// follows the pointer 1:1.
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        if self.dragging {
            if let Some((lx, ly)) = self.last_cursor {
                let (dx, dy) = (x - lx, y - ly);
                // Screen delta → world delta (Y is flipped between the two).
                self.camera.center.x -= dx / self.camera.scale;
                self.camera.center.y += dy / self.camera.scale;
            }
        }
        self.last_cursor = Some((x, y));
    }

    /// Zoom by `steps` notches (positive = in) about the cursor, so the world
    /// point under the pointer stays under the pointer — the behaviour every CAD
    /// user expects from a scroll wheel.
    pub fn zoom(&mut self, steps: f64, cursor: (f64, f64)) {
        let (cx, cy) = cursor;
        let anchor = self.camera.screen_to_world(cx, cy);

        let factor = 1.15_f64.powf(steps);
        let new_scale = (self.camera.scale * factor).clamp(self.min_scale, self.max_scale);
        self.camera.scale = new_scale;

        // Re-place the centre so `screen_to_world(cursor)` still yields `anchor`.
        let (w, h) = self.camera.viewport;
        self.camera.center = Point2::new(
            anchor.x - (cx - w * 0.5) / new_scale,
            anchor.y - (h * 0.5 - cy) / new_scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interactor(w: f64, h: f64) -> Interactor {
        Interactor::new(Camera::new((w, h)))
    }

    #[test]
    fn drag_pans_the_world_under_the_cursor() {
        let mut it = interactor(800.0, 600.0);
        it.camera.scale = 2.0;
        // Grab the world point currently under (400, 300)…
        it.cursor_moved(400.0, 300.0);
        let grabbed = it.camera().screen_to_world(400.0, 300.0);
        it.press();
        // …drag 100px right, 40px down.
        it.cursor_moved(500.0, 340.0);
        it.release();
        // That same world point is now under the new cursor position.
        let now = it.camera().screen_to_world(500.0, 340.0);
        assert!((now.x - grabbed.x).abs() < 1e-9);
        assert!((now.y - grabbed.y).abs() < 1e-9);
    }

    #[test]
    fn cursor_move_without_drag_does_not_pan() {
        let mut it = interactor(800.0, 600.0);
        let before = *it.camera();
        it.cursor_moved(100.0, 100.0);
        it.cursor_moved(700.0, 500.0);
        assert_eq!(*it.camera(), before, "no button held → no pan");
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let mut it = interactor(1000.0, 800.0);
        let cursor = (250.0, 600.0); // off-centre, so a naive zoom would drift
        let anchor_before = it.camera().screen_to_world(cursor.0, cursor.1);

        it.zoom(3.0, cursor); // zoom in
        assert!(it.camera().scale > 1.0, "zoomed in");
        let anchor_after = it.camera().screen_to_world(cursor.0, cursor.1);
        assert!((anchor_after.x - anchor_before.x).abs() < 1e-9);
        assert!((anchor_after.y - anchor_before.y).abs() < 1e-9);

        // Zooming back out by the same notches restores the scale.
        it.zoom(-3.0, cursor);
        assert!((it.camera().scale - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_is_clamped_and_never_degenerate() {
        let mut it = interactor(400.0, 400.0);
        for _ in 0..500 {
            it.zoom(10.0, (200.0, 200.0)); // hammer zoom-in
        }
        assert!(it.camera().scale.is_finite());
        assert!(it.camera().scale <= 1e9 + 1.0);
        for _ in 0..500 {
            it.zoom(-10.0, (200.0, 200.0)); // hammer zoom-out
        }
        assert!(it.camera().scale >= 1e-6);
        assert!(it.camera().scale > 0.0, "scale must never reach zero");
    }

    #[test]
    fn resize_updates_the_viewport() {
        let mut it = interactor(800.0, 600.0);
        it.resize(1024.0, 768.0);
        assert_eq!(it.camera().viewport, (1024.0, 768.0));
        // Degenerate sizes are floored, not accepted as zero.
        it.resize(0.0, 0.0);
        assert_eq!(it.camera().viewport, (1.0, 1.0));
    }
}
