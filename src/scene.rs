//! Custom wgpu rendering: a planet with an orbiting moon, drawn underneath the egui UI.
//!
//! egui-wgpu lets us hook into its render pass with a paint callback:
//! - [`SceneRenderer`] owns the GPU resources. It is created once and stored in the egui
//!   renderer's `callback_resources`, so the callback can reach it while rendering.
//! - [`SceneCallback`] is created every frame with that frame's parameters. `prepare` uploads
//!   them to the GPU and renders the shadow map, and `paint` issues the draw call.
//!
//! Shadows use a shadow map: each frame the scene's depth is first rendered from the light's
//! point of view. When shading a pixel, the shader checks whether something else was closer to
//! the light there - if so, the pixel is in shadow.

use eframe::{
    egui,
    egui_wgpu::{self, wgpu},
};
use glam::{
    Mat4, Vec3, Vec4,
    camera::rh::{proj::directx as proj, view},
};
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

/// Radius of a sphere around the origin that contains everything that can cast or receive a shadow.
const SCENE_RADIUS: f32 = MOON_ORBIT_RADIUS + MOON_RADIUS + 0.1;

const SHADOW_MAP_SIZE: u32 = 2048;
const SHADOW_MAP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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
    /// Whether the spheres cast shadows on each other.
    pub shadows: bool,
}

/// Must match `Globals` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: Mat4,
    light_view_proj: Mat4,
    camera_position: Vec4,
    light_direction: Vec4,
    ambient: f32,
    encode_srgb: f32,
    shadows: f32,
    _pad: f32,
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
            light_view_proj: self.light_view_proj(),
            camera_position: self.camera_position.extend(1.0),
            light_direction: self.light_direction.extend(0.0),
            ambient: self.ambient,
            encode_srgb: if encode_srgb { 1.0 } else { 0.0 },
            shadows: if self.shadows { 1.0 } else { 0.0 },
            _pad: 0.0,
        }
    }

    /// World space -> clip space as seen from the light.
    ///
    /// The light's rays are parallel, so this is an orthographic projection: a box aligned with
    /// the light direction that just contains the whole scene.
    fn light_view_proj(&self) -> Mat4 {
        let distance = 2.0 * SCENE_RADIUS;
        // Any "up" works, as long as it isn't parallel to the light direction.
        let up = if self.light_direction.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let view = view::look_at_mat4(self.light_direction * distance, Vec3::ZERO, up);
        let proj = proj::orthographic(
            -SCENE_RADIUS,
            SCENE_RADIUS,
            -SCENE_RADIUS,
            SCENE_RADIUS,
            distance - SCENE_RADIUS,
            distance + SCENE_RADIUS,
        );
        proj * view
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
    /// Depth-only pipeline that renders the scene from the light's point of view.
    shadow_pipeline: wgpu::RenderPipeline,
    /// The depth texture `shadow_pipeline` renders into...
    shadow_map_view: wgpu::TextureView,
    /// ...and the bind group that lets the main pipeline read it.
    shadow_bind_group: wgpu::BindGroup,
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

        let shadow_map_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("shadow map"),
                size: wgpu::Extent3d {
                    width: SHADOW_MAP_SIZE,
                    height: SHADOW_MAP_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_MAP_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        // A comparison sampler returns the result of `depth <= shadow map depth` rather than
        // the depth itself, bilinearly filtered.
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow map layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow map"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
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
            bind_group_layouts: &[Some(&globals_layout), Some(&shadow_layout)],
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

        // The shadow pass can't have the shadow map bound, since it is rendering to it.
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            // Depth only: no fragment shader and no color targets.
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_MAP_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                // Push the stored depth away from the light a little, more so on surfaces at a
                // steep angle to it, so surfaces don't shadow themselves.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        render_state.renderer.write().callback_resources.insert(Self {
            pipeline,
            shadow_pipeline,
            shadow_map_view,
            shadow_bind_group,
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
    /// Called before egui's render pass begins: upload this frame's data and render the shadow map.
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get::<SceneRenderer>() {
            queue.write_buffer(&renderer.globals_buffer, 0, bytemuck::bytes_of(&self.params.globals(renderer.encode_srgb)));
            queue.write_buffer(
                &renderer.instance_buffer,
                0,
                bytemuck::cast_slice(&self.params.instances()),
            );

            // Our own render pass, recorded ahead of egui's on the same encoder.
            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &renderer.shadow_map_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            shadow_pass.set_pipeline(&renderer.shadow_pipeline);
            shadow_pass.set_bind_group(0, &renderer.globals_bind_group, &[]);
            renderer.draw_spheres(&mut shadow_pass);
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
            render_pass.set_bind_group(1, &renderer.shadow_bind_group, &[]);
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
