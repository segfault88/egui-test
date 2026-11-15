//! Custom wgpu rendering: the classic triangle, drawn underneath the egui UI.
//!
//! egui-wgpu lets us hook into its render pass with a paint callback:
//! - [`TriangleRenderer`] owns the GPU resources. It is created once and stored in the egui
//!   renderer's `callback_resources`, so the callback can reach it while rendering.
//! - [`TriangleCallback`] is created every frame with that frame's parameters. `prepare` uploads
//!   them to the GPU, and `paint` issues the draw call.

use eframe::{
    egui,
    egui_wgpu::{self, wgpu},
};

/// The GPU resources needed to draw the triangle.
pub struct TriangleRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

/// Must match `Uniforms` in `triangle.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    angle: f32,
    aspect: f32,
    _pad: [f32; 2],
}

impl TriangleRenderer {
    /// Creates the pipeline and registers it with the egui renderer.
    pub fn install(render_state: &egui_wgpu::RenderState) {
        let device = &render_state.device;

        let shader = device.create_shader_module(wgpu::include_wgsl!("triangle.wgsl"));

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("triangle uniforms"),
            size: size_of::<Uniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("triangle bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("triangle bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("triangle pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("triangle pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                // We draw inside egui's render pass, so we must use its surface format.
                targets: &[Some(render_state.target_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        render_state.renderer.write().callback_resources.insert(Self {
            pipeline,
            bind_group,
            uniform_buffer,
        });
    }
}

/// Per-frame parameters for drawing the triangle.
pub struct TriangleCallback {
    /// Rotation in radians.
    pub angle: f32,
    /// Width / height of the area being drawn into.
    pub aspect: f32,
}

impl TriangleCallback {
    /// Paints the triangle into `rect` of the given painter.
    pub fn paint(painter: &egui::Painter, rect: egui::Rect, angle: f32) {
        painter.add(egui_wgpu::Callback::new_paint_callback(
            rect,
            Self {
                angle,
                aspect: rect.aspect_ratio(),
            },
        ));
    }
}

impl egui_wgpu::CallbackTrait for TriangleCallback {
    /// Called before egui's render pass begins: upload this frame's uniforms.
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get::<TriangleRenderer>() {
            let uniforms = Uniforms {
                angle: self.angle,
                aspect: self.aspect,
                _pad: [0.0; 2],
            };
            queue.write_buffer(&renderer.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        }
        Vec::new()
    }

    /// Called inside egui's render pass, with the viewport already set to our rect.
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(renderer) = resources.get::<TriangleRenderer>() {
            render_pass.set_pipeline(&renderer.pipeline);
            render_pass.set_bind_group(0, &renderer.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}
