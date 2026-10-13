//! Presents the shared canvas mesh into the child window.
//!
//! The vertex layout and shader are [`aad_render::mesh`]. This module only uploads them and
//! issues the draw.

use aad_render::mesh::{CANVAS_SHADER, INSTANCE_SIZE, MeshData, VERTEX_SIZE};
use wgpu::util::DeviceExt;

use crate::surface;

const CHUNK: usize = 4_194_300;
const UNIFORM_SIZE: u64 = 48;

struct Batch {
    buffer: wgpu::Buffer,
    count: u32,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    linear_out: bool,
    line_pipeline: wgpu::RenderPipeline,
    tri_pipeline: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    lines: Vec<Batch>,
    tris: Vec<Batch>,
    quads: Vec<Batch>,
    breaks: Vec<[usize; 3]>,
    key: Option<u64>,
    clear: wgpu::Color,
    over_lines: Vec<Batch>,
    over_tris: Vec<Batch>,
    over_quads: Vec<Batch>,
    over_breaks: Vec<[usize; 3]>,
}

impl Gpu {
    pub fn open(hwnd: isize, width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = surface::from_hwnd(&instance, hwnd)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| e.to_string())?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
        let mut config = surface.get_default_config(&adapter, width.max(1), height.max(1)).ok_or("the drawing surface is not supported")?;
        if let Some(format) = surface.get_capabilities(&adapter).formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = format;
        }
        let format = config.format;
        surface.configure(&device, &config);
        let linear_out = format.is_srgb();
        let (line_pipeline, tri_pipeline, quad_pipeline, uniform, bind_group) = pipelines(&device, format);
        Ok(Gpu {
            surface,
            device,
            queue,
            config,
            linear_out,
            line_pipeline,
            tri_pipeline,
            quad_pipeline,
            uniform,
            bind_group,
            lines: Vec::new(),
            tris: Vec::new(),
            quads: Vec::new(),
            breaks: Vec::new(),
            key: None,
            clear: wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            over_lines: Vec::new(),
            over_tris: Vec::new(),
            over_quads: Vec::new(),
            over_breaks: Vec::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return Ok(());
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        Ok(())
    }

    pub fn set_clear(&mut self, rgb: aad_color::Rgb) {
        self.clear = wgpu::Color { r: f64::from(rgb.0) / 255.0, g: f64::from(rgb.1) / 255.0, b: f64::from(rgb.2) / 255.0, a: 1.0 };
    }

    pub fn set_overlay(&mut self, mesh: &MeshData) {
        self.over_lines = upload(&self.device, "cad_over_lines", &mesh.lines, VERTEX_SIZE);
        self.over_tris = upload(&self.device, "cad_over_fills", &mesh.tris, VERTEX_SIZE);
        self.over_quads = upload(&self.device, "cad_over_quads", &mesh.quads, INSTANCE_SIZE);
        self.over_breaks = mesh.breaks.clone();
    }

    pub fn present(&mut self, key: u64, mesh: &MeshData, uniform: &[f32; 12]) -> Result<(), String> {
        if self.key != Some(key) {
            self.lines = upload(&self.device, "cad_lines", &mesh.lines, VERTEX_SIZE);
            self.tris = upload(&self.device, "cad_fills", &mesh.tris, VERTEX_SIZE);
            self.quads = upload(&self.device, "cad_quads", &mesh.quads, INSTANCE_SIZE);
            self.breaks = mesh.breaks.clone();
            self.key = Some(key);
        }
        let mut words = *uniform;
        words[3] = if self.linear_out { 1.0 } else { 0.0 };
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.queue.write_buffer(&self.uniform, 0, &bytes);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => return Err(format!("drawing surface: {other:?}")),
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            other => return Err(format!("drawing surface: {other:?}")),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cad") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cad"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(self.clear), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_parts(&mut pass, &self.tris, &self.lines, &self.quads, &self.breaks);
            self.draw_parts(&mut pass, &self.over_tris, &self.over_lines, &self.over_quads, &self.over_breaks);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(frame);
        Ok(())
    }

    fn draw_parts(&self, pass: &mut wgpu::RenderPass<'_>, tris: &[Batch], lines: &[Batch], quads: &[Batch], breaks: &[[usize; 3]]) {
        pass.set_bind_group(0, &self.bind_group, &[]);
        let total = |bs: &[Batch]| bs.iter().map(|b| b.count as usize).sum::<usize>();
        let all = [total(tris), total(lines), total(quads)];
        let mut from = [0usize; 3];
        for to in breaks.iter().copied().chain(std::iter::once(all)) {
            for (i, (pipeline, batches)) in
                [(&self.tri_pipeline, tris), (&self.line_pipeline, lines), (&self.quad_pipeline, quads)].into_iter().enumerate()
            {
                let parts = batch_ranges(batches.iter().map(|b| b.count), from[i]..to[i]);
                if parts.is_empty() {
                    continue;
                }
                pass.set_pipeline(pipeline);
                for (j, range) in parts {
                    let Some(batch) = batches.get(j) else { continue };
                    pass.set_vertex_buffer(0, batch.buffer.slice(..));
                    if i == 2 { pass.draw(0..6, range) } else { pass.draw(range, 0..1) }
                }
            }
            from = to;
        }
    }
}

fn pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline, wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let module = device
        .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("cad_canvas"), source: wgpu::ShaderSource::Wgsl(CANVAS_SHADER.into()) });
    let entry = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("cad_canvas"), entries: &[entry(0)] });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cad_canvas"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cad_canvas_view"),
        size: UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cad_canvas"),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let pipeline = |label: &str, vs: &str, topology: wgpu::PrimitiveTopology, buffer: wgpu::VertexBufferLayout<'static>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some(vs), buffers: &[Some(buffer)], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    (
        pipeline("cad_lines", "vs_line", wgpu::PrimitiveTopology::LineList, vertex_layout()),
        pipeline("cad_fills", "vs_line", wgpu::PrimitiveTopology::TriangleList, vertex_layout()),
        pipeline("cad_quads", "vs_quad", wgpu::PrimitiveTopology::TriangleList, instance_layout()),
        uniform,
        bind_group,
    )
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Unorm8x4];
    wgpu::VertexBufferLayout { array_stride: VERTEX_SIZE as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS }
}

fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4, 3 => Float32];
    wgpu::VertexBufferLayout { array_stride: INSTANCE_SIZE as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &ATTRS }
}

fn upload(device: &wgpu::Device, label: &str, bytes: &[u8], stride: usize) -> Vec<Batch> {
    bytes
        .chunks(CHUNK.saturating_mul(stride))
        .filter(|c| c.len() >= stride)
        .map(|c| Batch {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: c,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: u32::try_from(c.len() / stride).unwrap_or(0),
        })
        .collect()
}

fn batch_ranges(counts: impl IntoIterator<Item = u32>, range: std::ops::Range<usize>) -> Vec<(usize, std::ops::Range<u32>)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    for (j, n) in counts.into_iter().enumerate() {
        let n = n as usize;
        let lo = range.start.max(off);
        let hi = range.end.min(off + n);
        if lo < hi {
            out.push((j, (lo - off) as u32..(hi - off) as u32));
        }
        off += n;
    }
    out
}
