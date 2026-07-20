//! Interactive window (`winit` + `wgpu` surface) — the live pan/zoom canvas.
//!
//! Feature-gated (`--features gui`). This is the one part of the canvas that
//! **cannot be verified headlessly** — a surface needs a real window on a real
//! display. It is deliberately thin: all the behaviour worth testing lives
//! elsewhere and is proven —
//!
//! - **what to draw** → [`build_drawlist`](crate::build_drawlist) (tested),
//! - **how pixels rasterize** → the pipeline shared with the offscreen
//!   [`gpu`](crate::gpu) canvas (proven by its pixel-readback tests),
//! - **how input moves the camera** → [`Interactor`](crate::interact::Interactor)
//!   (unit-tested pan/zoom).
//!
//! What remains here is only the plumbing: create a window, configure a surface,
//! and forward `winit` events into the tested pieces. Left mouse drags to pan,
//! the scroll wheel zooms about the cursor, `F` re-fits, `Esc` quits.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use aadt_model::Document;

use crate::gpu::{build_vertices, create_pipeline, BACKGROUND};
use crate::interact::Interactor;
use crate::{build_drawlist, Camera, DrawList};

/// Open the interactive canvas on `doc` and run until the window is closed.
///
/// Blocks on the event loop, so this is a terminal call for the thread.
pub fn run(doc: Document) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    // Redraw on demand (input/resize), not continuously — a drawing is static
    // until the user moves the camera, so there is no reason to spin the CPU.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        drawlist: build_drawlist(&doc),
        doc,
        interactor: Interactor::new(Camera::new((1280.0, 800.0))),
        cursor: (0.0, 0.0),
        gpu: None,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// The wgpu objects that only exist once a window does.
struct WindowGpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
}

struct App {
    doc: Document,
    drawlist: DrawList,
    interactor: Interactor,
    cursor: (f64, f64),
    gpu: Option<WindowGpu>,
}

impl App {
    fn redraw(&mut self) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            // Timeout / occluded / lost / outdated: reconfigure, skip this frame.
            _ => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let verts = build_vertices(
            self.interactor.camera(),
            &self.drawlist,
            gpu.config.width,
            gpu.config.height,
        );
        let vbuf = wgpu::util::DeviceExt::create_buffer_init(
            &gpu.device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("aadt-view-vertices"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            },
        );

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("aadt-view-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !verts.is_empty() {
                pass.set_pipeline(&gpu.pipeline);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..verts.len() as u32, 0..1);
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
        gpu.queue.present(frame);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let window = match event_loop
            .create_window(Window::default_attributes().with_title("AADT — canvas"))
        {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("aadt: could not open a window: {e}");
                event_loop.exit();
                return;
            }
        };

        let instance = wgpu::Instance::default();
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aadt: could not create a drawing surface: {e}");
                event_loop.exit();
                return;
            }
        };
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })) {
                Ok(a) => a,
                Err(_) => {
                    eprintln!("aadt: no graphics adapter available");
                    event_loop.exit();
                    return;
                }
            };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("aadt-view"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("device");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let pipeline = create_pipeline(&device, format);
        let (vw, vh) = (config.width as f64, config.height as f64);

        self.gpu = Some(WindowGpu {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
        });

        // Frame the drawing on open.
        self.interactor.resize(vw, vh);
        self.interactor.fit_document(&self.doc, 0.08);
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    let (w, h) = (gpu.config.width as f64, gpu.config.height as f64);
                    self.interactor.resize(w, h);
                    if let Some(gpu) = self.gpu.as_ref() {
                        gpu.window.request_redraw();
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.interactor.cursor_moved(position.x, position.y);
                if self.interactor.is_dragging() {
                    if let Some(gpu) = self.gpu.as_ref() {
                        gpu.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => self.interactor.press(),
                        ElementState::Released => self.interactor.release(),
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 50.0,
                };
                self.interactor.zoom(steps, self.cursor);
                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                        PhysicalKey::Code(KeyCode::KeyF) => {
                            self.fit_to_drawing();
                            if let Some(gpu) = self.gpu.as_ref() {
                                gpu.window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

impl App {
    /// Re-fit the camera to the whole drawing (the `F` key, and initial framing).
    fn fit_to_drawing(&mut self) {
        self.interactor.fit_document(&self.doc, 0.08);
    }
}
