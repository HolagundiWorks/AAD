//! GPU canvas backend (`wgpu`) — the real [`Renderer`](crate::Renderer) behind
//! the headless seam (spec §4.3 / ADR-0001).
//!
//! Feature-gated (`--features gpu`) so the drawlist/camera core stays std-only.
//!
//! # Verifiable without a screen
//!
//! The canvas renders **offscreen**: it acquires an adapter with no surface,
//! rasterizes a [`DrawList`] into an off-screen texture, and can read that
//! texture back as RGBA / PNG. No window, no display — which is exactly what
//! lets this be tested in headless CI (a software adapter such as Windows' WARP
//! or Vulkan lavapipe is enough). The windowed presentation loop is a thin layer
//! on top of the same pipeline and is added where there is a screen to prove it.
//!
//! Every [`DrawCommand`](crate::DrawCommand) is flattened to line segments —
//! polylines directly, circles and arcs tessellated, markers and text anchors as
//! small crosshairs — coloured per layer, and drawn as a single `LineList` pass.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::{Camera, DrawCommand, DrawList, Renderer};
use aadt_geometry::Point2;

/// Why the GPU canvas could not start or produce an image.
#[derive(Debug)]
pub enum GpuError {
    /// No graphics adapter was available — no GPU and no software fallback.
    NoAdapter,
    /// The device could not be created.
    Device(String),
    /// Reading the rendered texture back failed.
    Readback(String),
    /// Encoding or writing the PNG failed.
    Encode(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::NoAdapter => write!(f, "no graphics adapter (no GPU or software fallback)"),
            GpuError::Device(m) => write!(f, "creating GPU device: {m}"),
            GpuError::Readback(m) => write!(f, "reading rendered image back: {m}"),
            GpuError::Encode(m) => write!(f, "encoding PNG: {m}"),
        }
    }
}

impl std::error::Error for GpuError {}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    /// Position in normalized device coordinates (−1..1, y up).
    pos: [f32; 2],
    color: [f32; 3],
}

/// The clear colour shared by the offscreen canvas and the window — a dark
/// "model space" so the light layer colours read.
pub(crate) const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.055,
    g: 0.07,
    b: 0.09,
    a: 1.0,
};

/// Build the line-drawing pipeline. Shared so the offscreen renderer and the
/// windowed [`view`](crate::view) rasterize through exactly the same shader and
/// state — the pipeline proven by the offscreen pixel tests.
pub(crate) fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("aadt-canvas-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("aadt-canvas-pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// An off-screen `wgpu` canvas. Construct with [`WgpuRenderer::new`], paint with
/// [`Renderer::render`], then pull the result with [`to_rgba`](Self::to_rgba) or
/// [`save_png`](Self::save_png).
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
    background: wgpu::Color,
    vertices: Vec<Vertex>,
}

impl WgpuRenderer {
    /// Start an off-screen canvas of `width`×`height` pixels.
    ///
    /// Prefers a real GPU but accepts a software adapter, so it works on a
    /// headless CI box. Returns [`GpuError::NoAdapter`] when nothing at all is
    /// available — the caller degrades (e.g. falls back to another exporter)
    /// rather than crashing.
    pub fn new(width: u32, height: u32) -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| GpuError::NoAdapter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("aadt-canvas"),
            required_features: wgpu::Features::empty(),
            // Downlevel limits so a software adapter qualifies.
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("aadt-canvas-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let pipeline = create_pipeline(&device, format);

        Ok(Self {
            device,
            queue,
            texture,
            view,
            pipeline,
            width,
            height,
            background: BACKGROUND,
            vertices: Vec::new(),
        })
    }

    /// Read the last painted frame back as tightly-packed RGBA8 bytes
    /// (`width * height * 4`).
    pub fn to_rgba(&self) -> Result<Vec<u8>, GpuError> {
        // Texture rows must be copied at a 256-byte-aligned stride.
        let unpadded = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("aadt-canvas-readback"),
            size: (padded * self.height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;

        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        let mut rgba = Vec::with_capacity((unpadded * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded) as usize;
            rgba.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(rgba)
    }

    /// Save the last painted frame as a PNG.
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), GpuError> {
        let rgba = self.to_rgba()?;
        let file = std::fs::File::create(path).map_err(|e| GpuError::Encode(e.to_string()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| GpuError::Encode(e.to_string()))?
            .write_image_data(&rgba)
            .map_err(|e| GpuError::Encode(e.to_string()))?;
        Ok(())
    }

    /// Flatten a [`DrawList`] into NDC line-segment vertices for one camera.
    fn build_vertices(&self, camera: &Camera, list: &DrawList) -> Vec<Vertex> {
        build_vertices(camera, list, self.width, self.height)
    }
}

/// Flatten a [`DrawList`] into NDC line-segment vertices for a camera and a
/// `width`×`height` target. Shared by the offscreen canvas and the window, so
/// both draw pixel-for-pixel the same way.
pub(crate) fn build_vertices(
    camera: &Camera,
    list: &DrawList,
    width: u32,
    height: u32,
) -> Vec<Vertex> {
    {
        let mut v = Vec::new();
        let (w, h) = (width as f64, height as f64);
        // World point → NDC (screen y is down; NDC y is up).
        let ndc = |p: Point2| -> [f32; 2] {
            let (sx, sy) = camera.world_to_screen(p);
            [(sx / w * 2.0 - 1.0) as f32, (1.0 - sy / h * 2.0) as f32]
        };

        for cmd in &list.commands {
            let color = layer_color(cmd.layer());
            let mut seg = |a: Point2, b: Point2| {
                v.push(Vertex { pos: ndc(a), color });
                v.push(Vertex { pos: ndc(b), color });
            };
            match cmd {
                DrawCommand::Polyline { points, closed, .. } => {
                    for pair in points.windows(2) {
                        seg(pair[0], pair[1]);
                    }
                    if *closed && points.len() > 2 {
                        seg(points[points.len() - 1], points[0]);
                    }
                }
                DrawCommand::Circle { center, radius, .. } => {
                    tessellate_arc(*center, *radius, 0.0, 360.0, 64, &mut seg);
                }
                DrawCommand::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                    ..
                } => {
                    let sweep = normalize_sweep(*start_angle, *end_angle);
                    let steps = ((sweep / 360.0 * 64.0).ceil() as usize).max(2);
                    tessellate_arc(*center, *radius, *start_angle, *end_angle, steps, &mut seg);
                }
                // Anchors have no glyph geometry — a small screen-space crosshair
                // marks the spot so the frame shows where annotation sits.
                DrawCommand::Text { position, .. } | DrawCommand::Marker { position, .. } => {
                    crosshair(camera, *position, &mut v, color);
                }
            }
        }
        v
    }
}

impl Renderer for WgpuRenderer {
    fn render(&mut self, camera: &Camera, list: &DrawList) {
        self.vertices = self.build_vertices(camera, list);

        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("aadt-canvas-vertices"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("aadt-canvas-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !self.vertices.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..self.vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }
}

/// A small screen-space crosshair (an `×`) at a world point.
fn crosshair(camera: &Camera, at: Point2, out: &mut Vec<Vertex>, color: [f32; 3]) {
    let (w, h) = camera.viewport;
    let (sx, sy) = camera.world_to_screen(at);
    let d = 4.0;
    let to_ndc = |x: f64, y: f64| [(x / w * 2.0 - 1.0) as f32, (1.0 - y / h * 2.0) as f32];
    for (ax, ay, bx, by) in [
        (sx - d, sy - d, sx + d, sy + d),
        (sx - d, sy + d, sx + d, sy - d),
    ] {
        out.push(Vertex {
            pos: to_ndc(ax, ay),
            color,
        });
        out.push(Vertex {
            pos: to_ndc(bx, by),
            color,
        });
    }
}

/// Emit `steps` chords of an arc/circle in world space via `seg`.
fn tessellate_arc(
    center: Point2,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    steps: usize,
    seg: &mut impl FnMut(Point2, Point2),
) {
    let sweep = normalize_sweep(start_deg, end_deg);
    let at = |t: f64| {
        let a = (start_deg + sweep * t).to_radians();
        Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
    };
    let mut prev = at(0.0);
    for i in 1..=steps {
        let cur = at(i as f64 / steps as f64);
        seg(prev, cur);
        prev = cur;
    }
}

/// Positive swept angle in degrees, wrapping through zero (a `350→10` arc sweeps
/// 20°). A zero/negative span becomes a full 360° turn (used for circles).
fn normalize_sweep(start_deg: f64, end_deg: f64) -> f64 {
    let mut d = end_deg - start_deg;
    while d <= 0.0 {
        d += 360.0;
    }
    d
}

/// A colour for a layer, keyed on the AADT layer-name conventions. Light hues on
/// the dark ground; unknown layers fall back to a neutral grey.
fn layer_color(layer: &str) -> [f32; 3] {
    let u = layer.to_ascii_uppercase();
    if u.starts_with("A-WALL") {
        [0.92, 0.92, 0.94]
    } else if u.starts_with("A-ROOM") {
        [0.35, 0.72, 0.90]
    } else if u.starts_with("A-DOOR") || u.starts_with("A-GLAZ") {
        [0.45, 0.82, 0.55]
    } else if u.starts_with("S-") {
        [0.92, 0.45, 0.35]
    } else if u.starts_with("A-ANNO") || u.starts_with("A-DIMS") {
        [0.88, 0.68, 0.32]
    } else if u.starts_with("A-DETL") {
        [0.90, 0.82, 0.35]
    } else {
        [0.62, 0.64, 0.70]
    }
}

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_drawlist;
    use aadt_geometry::Segment;
    use aadt_model::{Document, Geometry};

    /// Build a renderer, or skip the test when no adapter exists (a bare CI box
    /// with neither GPU nor software fallback). Printed, not silent.
    fn renderer_or_skip(w: u32, h: u32) -> Option<WgpuRenderer> {
        match WgpuRenderer::new(w, h) {
            Ok(r) => Some(r),
            Err(GpuError::NoAdapter) => {
                eprintln!("SKIP: no graphics adapter available in this environment");
                None
            }
            Err(e) => panic!("unexpected GPU init failure: {e}"),
        }
    }

    fn background_pixel(rgba: &[u8]) -> [u8; 4] {
        // Top-left corner: nothing is drawn there, so it stays the clear colour.
        [rgba[0], rgba[1], rgba[2], rgba[3]]
    }

    #[test]
    fn renders_lines_to_non_blank_pixels() {
        let Some(mut r) = renderer_or_skip(256, 256) else {
            return;
        };
        // An X across the viewport, guaranteed to cross the centre.
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(
                Point2::new(-10.0, -10.0),
                Point2::new(10.0, 10.0),
            )),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(
                Point2::new(-10.0, 10.0),
                Point2::new(10.0, -10.0),
            )),
        );
        let mut cam = Camera::new((256.0, 256.0));
        cam.fit_document(&doc, 0.1);

        r.render(&cam, &build_drawlist(&doc));
        let rgba = r.to_rgba().expect("readback");

        assert_eq!(rgba.len(), 256 * 256 * 4);
        let bg = background_pixel(&rgba);
        // Some pixel differs from the background — i.e. lines were actually drawn.
        let drawn = rgba.chunks_exact(4).any(|p| p != bg);
        assert!(
            drawn,
            "frame is entirely the background colour — nothing drew"
        );
        // And the exact centre pixel sits on both diagonals, so it must be lit.
        let mid = (128 * 256 + 128) * 4;
        assert_ne!(
            &rgba[mid..mid + 4],
            &bg,
            "centre pixel should lie on the drawn X"
        );
    }

    #[test]
    fn empty_document_is_all_background() {
        let Some(mut r) = renderer_or_skip(64, 64) else {
            return;
        };
        let doc = Document::new();
        let cam = Camera::new((64.0, 64.0));
        r.render(&cam, &build_drawlist(&doc));
        let rgba = r.to_rgba().expect("readback");
        let bg = background_pixel(&rgba);
        assert!(
            rgba.chunks_exact(4).all(|p| p == bg),
            "an empty drawing must clear to the background only"
        );
    }
}
