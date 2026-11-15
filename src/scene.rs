//! Custom wgpu rendering: a planet with an orbiting moon, drawn underneath the egui UI.
//!
//! egui-wgpu lets us hook into its render pass with a paint callback:
//! - [`SceneRenderer`] owns the GPU resources. It is created once and stored in the egui
//!   renderer's `callback_resources`, so the callback can reach it while rendering.
//! - [`SceneCallback`] is created every frame with that frame's parameters. `prepare` uploads
//!   them to the GPU, and `paint` issues the draw call.

use eframe::{
    egui,
    egui_wgpu::{self, wgpu},
};
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt as _;

use crate::sphere;

/// Bits of depth buffer to ask eframe for. egui itself doesn't need one, but the 3D scene does,
/// and it is drawn inside egui's render pass.
pub const DEPTH_BITS: u8 = 32;

// Colors are in linear space.
const PLANET_RADIUS: f32 = 1.0;
const PLANET_COLOR: [f32; 4] = [0.02, 0.12, 0.8, 1.0];
const MOON_RADIUS: f32 = 0.27;
const MOON_COLOR: [f32; 4] = [0.04, 0.5, 0.07, 1.0];
const MOON_ORBIT_RADIUS: f32 = 2.6;

/// Everything needed to draw one frame of the scene.
pub struct SceneParams {
    /// World space -> clip space for the camera.
    pub view_proj: Mat4,
    /// Camera position in world space.
    pub camera_position: Vec3,
    /// Where the moon is on its (circular) orbit, in radians.
    pub moon_angle: f32,
    /// Unit vector pointing from the scene towards the light. The light is infinitely far away
    /// (like the sun), so its rays are parallel.
    pub light_direction: Vec3,
    /// Amount of light that reaches surfaces facing away from the light.
    pub ambient: f32,
}

/// Must match `Globals` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: Mat4,
    camera_position: Vec4,
    light_direction: Vec4,
    ambient: f32,
    encode_srgb: f32,
    _pad: [f32; 2],
}

/// Must match `InstanceInput` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: Mat4,
    color: [f32; 4],
}

impl Instance {
    fn sphere(center: Vec3, radius: f32, color: [f32; 4]) -> Self {
        Self {
            model: Mat4::from_translation(center) * Mat4::from_scale(Vec3::splat(radius)),
            color,
        }
    }
}

impl SceneParams {
    fn globals(&self, encode_srgb: bool) -> Globals {
        Globals {
            view_proj: self.view_proj,
            camera_position: self.camera_position.extend(1.0),
            light_direction: self.light_direction.extend(0.0),
            ambient: self.ambient,
            encode_srgb: if encode_srgb { 1.0 } else { 0.0 },
            _pad: [0.0; 2],
        }
    }

    fn instances(&self) -> [Instance; 2] {
        // The moon moves on a circle in the XZ plane.
        let moon_center = MOON_ORBIT_RADIUS * Vec3::new(self.moon_angle.cos(), 0.0, self.moon_angle.sin());
        [
            Instance::sphere(Vec3::ZERO, PLANET_RADIUS, PLANET_COLOR),
            Instance::sphere(moon_center, MOON_RADIUS, MOON_COLOR),
        ]
    }
}

/// The GPU resources needed to draw the scene.
pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    globals_bind_group: wgpu::BindGroup,
    globals_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// The lighting is computed in linear color space. If the render target isn't an sRGB
    /// format (egui prefers ones that aren't), the shader must do the conversion itself.
    encode_srgb: bool,
}

impl SceneRenderer {
    /// Creates the GPU resources and registers them with the egui renderer.
    pub fn install(render_state: &egui_wgpu::RenderState) {
        let device = &render_state.device;

        let shader = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));

        // Both spheres are instances of the same unit sphere mesh.
        let (positions, indices) = sphere::unit_sphere(64, 32);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere vertices"),
            contents: bytemuck::cast_slice(&positions),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene globals"),
            size: size_of::<Globals>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene instances"),
            size: size_of::<[Instance; 2]>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<[f32; 3]>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Instance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, // model matrix
                    5 => Float32x4, // color
                ],
            }),
        ];

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                // We draw inside egui's render pass, so we must use its surface format...
                targets: &[Some(render_state.target_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            // ...and its depth buffer format.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: egui_wgpu::depth_format_from_bits(DEPTH_BITS, 0).expect("unsupported depth format"),
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        render_state.renderer.write().callback_resources.insert(Self {
            pipeline,
            globals_bind_group,
            globals_buffer,
            instance_buffer,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            encode_srgb: !render_state.target_format.is_srgb(),
        });
    }
}

/// Draws the scene for one frame.
pub struct SceneCallback {
    params: SceneParams,
}

impl SceneCallback {
    /// Paints the scene into `rect` of the given painter.
    pub fn paint(painter: &egui::Painter, rect: egui::Rect, params: SceneParams) {
        painter.add(egui_wgpu::Callback::new_paint_callback(rect, Self { params }));
    }
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    /// Called before egui's render pass begins: upload this frame's data.
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get::<SceneRenderer>() {
            queue.write_buffer(&renderer.globals_buffer, 0, bytemuck::bytes_of(&self.params.globals(renderer.encode_srgb)));
            queue.write_buffer(
                &renderer.instance_buffer,
                0,
                bytemuck::cast_slice(&self.params.instances()),
            );
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
        if let Some(renderer) = resources.get::<SceneRenderer>() {
            render_pass.set_pipeline(&renderer.pipeline);
            render_pass.set_bind_group(0, &renderer.globals_bind_group, &[]);
            renderer.draw_spheres(render_pass);
        }
    }
}

impl SceneRenderer {
    fn draw_spheres(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.index_count, 0, 0..2);
    }
}
