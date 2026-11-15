use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::{egui, egui_wgpu};

use crate::{
    camera::OrbitCamera,
    scene::{SceneCallback, SceneParams, SceneRenderer},
};

/// The application state. This code is identical on native and on the web.
pub struct App {
    camera: OrbitCamera,
    /// Where the moon is on its orbit, in radians.
    moon_angle: f32,
    /// Whether the moon moves on its own.
    animate: bool,
    /// Orbit speed while animating, in radians per second.
    orbit_speed: f32,
    /// Compass direction of the light, in radians.
    light_azimuth: f32,
    /// Angle of the light above the moon's orbital plane, in radians.
    light_elevation: f32,
    /// Amount of light that reaches surfaces facing away from the light.
    ambient: f32,
    /// Whether the spheres cast shadows on each other.
    shadows: bool,
    /// Description of the GPU adapter wgpu picked (Metal/Vulkan/DX12 natively, WebGPU in the browser).
    adapter_summary: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // `wgpu_render_state` gives access to the wgpu `Device`, `Queue`, `Adapter` and the
        // egui renderer - this is where we create our own pipelines.
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("this app requires the wgpu renderer");

        SceneRenderer::install(render_state);

        let adapter_summary = egui_wgpu::adapter_info_summary(&render_state.adapter.get_info());
        log::info!("wgpu adapter: {adapter_summary}");

        Self {
            camera: OrbitCamera::default(),
            moon_angle: 0.0,
            animate: true,
            orbit_speed: 0.5,
            light_azimuth: 0.5,
            light_elevation: 10f32.to_radians(),
            ambient: 0.03,
            shadows: true,
            adapter_summary,
        }
    }

    /// Unit vector pointing from the scene towards the light.
    fn light_direction(&self) -> glam::Vec3 {
        let (azimuth, elevation) = (self.light_azimuth, self.light_elevation);
        glam::Vec3::new(
            elevation.cos() * azimuth.cos(),
            elevation.sin(),
            elevation.cos() * azimuth.sin(),
        )
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.animate {
            let dt = ui.input(|i| i.stable_dt);
            self.moon_angle = (self.moon_angle + self.orbit_speed * dt).rem_euclid(TAU);
            // egui only repaints on input by default; keep animating.
            ui.request_repaint();
        }

        // The 3D scene fills the whole window, behind all egui widgets. Dragging or scrolling
        // on it (rather than on a window) moves the camera.
        let rect = ui.max_rect();
        let response = ui.interact(rect, ui.scope_id().with("scene"), egui::Sense::drag());
        self.camera.handle_input(ui, &response);

        SceneCallback::paint(
            ui.painter(),
            rect,
            SceneParams {
                view_proj: self.camera.view_proj(rect.aspect_ratio()),
                camera_position: self.camera.position(),
                moon_angle: self.moon_angle,
                light_direction: self.light_direction(),
                ambient: self.ambient,
                shadows: self.shadows,
            },
        );

        egui::Window::new("Controls")
            .default_pos([16.0, 16.0])
            .default_width(280.0)
            .show(ui.ctx(), |ui| {
                ui.label("Drag to rotate the camera, scroll to zoom.");

                ui.separator();

                ui.checkbox(&mut self.animate, "Animate orbit");
                ui.add_enabled(
                    self.animate,
                    egui::Slider::new(&mut self.orbit_speed, -3.0..=3.0).text("orbit speed (rad/s)"),
                );
                ui.add(egui::Slider::new(&mut self.moon_angle, 0.0..=TAU).text("moon angle (rad)"));

                ui.separator();

                ui.add(egui::Slider::new(&mut self.light_azimuth, 0.0..=TAU).text("light azimuth (rad)"));
                ui.add(
                    egui::Slider::new(&mut self.light_elevation, -FRAC_PI_2..=FRAC_PI_2).text("light elevation (rad)"),
                );
                ui.add(egui::Slider::new(&mut self.ambient, 0.0..=0.5).text("ambient light"));
                ui.checkbox(&mut self.shadows, "Shadows");

                ui.separator();

                ui.horizontal(|ui| {
                    egui::widgets::global_theme_preference_buttons(ui);
                    ui.separator();
                    ui.label(if frame.is_web() { "Running on the web" } else { "Running natively" });
                });

                ui.collapsing("wgpu adapter", |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&self.adapter_summary).monospace()).wrap());
                });
            });
    }

    /// The color of space.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.01, 1.0]
    }
}
