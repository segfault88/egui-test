use eframe::{egui, egui_wgpu};

use crate::triangle::{TriangleCallback, TriangleRenderer};

/// The application state. This code is identical on native and on the web.
pub struct App {
    /// Triangle rotation, in radians.
    angle: f32,
    /// Whether the triangle rotates on its own.
    spin: bool,
    /// Rotation speed while spinning, in radians per second.
    speed: f32,
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

        TriangleRenderer::install(render_state);

        let adapter_summary = egui_wgpu::adapter_info_summary(&render_state.adapter.get_info());
        log::info!("wgpu adapter: {adapter_summary}");

        Self {
            angle: 0.0,
            spin: true,
            speed: 1.0,
            adapter_summary,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.spin {
            let dt = ui.input(|i| i.stable_dt);
            self.angle = (self.angle + self.speed * dt).rem_euclid(std::f32::consts::TAU);
            // egui only repaints on input by default; keep animating.
            ui.request_repaint();
        }

        // The 3D scene fills the whole window, behind all egui widgets.
        TriangleCallback::paint(ui.painter(), ui.max_rect(), self.angle);

        egui::Window::new("Controls")
            .default_pos([16.0, 16.0])
            .default_width(280.0)
            .show(ui.ctx(), |ui| {
                ui.checkbox(&mut self.spin, "Spin");
                ui.add_enabled(
                    self.spin,
                    egui::Slider::new(&mut self.speed, -5.0..=5.0).text("speed (rad/s)"),
                );
                ui.add(egui::Slider::new(&mut self.angle, 0.0..=std::f32::consts::TAU).text("angle (rad)"));

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

    /// The color behind the triangle.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.extreme_bg_color.to_normalized_gamma_f32()
    }
}
