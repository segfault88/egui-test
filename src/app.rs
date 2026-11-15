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
    /// Smoothed time between frames, in seconds.
    frame_time: f32,
    /// Smoothed CPU time spent producing a frame, in seconds. Unlike `frame_time` this excludes
    /// waiting for vsync, so it shows how much headroom there is.
    cpu_time: f32,
    /// Whether presentation waits for the display's refresh. Native only.
    vsync: bool,
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
            frame_time: 0.0,
            cpu_time: 0.0,
            vsync: true,
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

/// Updates an exponential moving average.
fn smooth(average: &mut f32, sample: f32) {
    *average = if *average == 0.0 { sample } else { *average + 0.05 * (sample - *average) };
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Exponential moving average of the real time since the previous frame.
        let dt = ui.input(|i| i.unstable_dt);
        smooth(&mut self.frame_time, dt);
        // eframe times the previous frame for us: running this function, tessellating the UI,
        // and encoding and submitting the GPU commands. It does not include the time the GPU
        // then takes to execute them.
        if let Some(cpu_time) = frame.info().cpu_usage {
            smooth(&mut self.cpu_time, cpu_time);
        }

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

                if self.animate {
                    ui.monospace(format!(
                        "frame {:5.2} ms ({:.0} fps)",
                        self.frame_time * 1000.0,
                        1.0 / self.frame_time
                    ));
                    ui.monospace(format!(
                        "cpu   {:5.2} ms ({:.0}% of frame)",
                        self.cpu_time * 1000.0,
                        100.0 * self.cpu_time / self.frame_time
                    ))
                    .on_hover_text(
                        "Time spent building the UI and encoding GPU commands, excluding the wait for vsync \
                         and the GPU's own execution time.",
                    );
                } else {
                    // egui only repaints on input when nothing is animating.
                    ui.monospace("not animating: no steady frame rate");
                }

                // Browsers always present in step with the display, so this is native only.
                // With vsync off, the frame rate above is the true limit, GPU included.
                if !frame.is_web()
                    && ui.checkbox(&mut self.vsync, "Vsync").changed()
                    && let Some(mut config) = frame.wgpu_surface_config()
                {
                    config.present_mode = if self.vsync {
                        egui_wgpu::wgpu::PresentMode::AutoVsync
                    } else {
                        egui_wgpu::wgpu::PresentMode::AutoNoVsync
                    };
                    frame.set_wgpu_surface_config(config);
                }

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
