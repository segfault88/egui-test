use eframe::{egui, egui_wgpu};

/// The application state. This code is identical on native and on the web.
pub struct App {
    name: String,
    counter: i32,
    /// Description of the GPU adapter wgpu picked (Metal/Vulkan/DX12 natively, WebGPU in the browser).
    adapter_summary: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // `wgpu_render_state` gives access to the wgpu `Device`, `Queue`, `Adapter` and the
        // egui renderer - this is where you would create your own pipelines and textures.
        let adapter_summary = match &cc.wgpu_render_state {
            Some(render_state) => egui_wgpu::adapter_info_summary(&render_state.adapter.get_info()),
            None => "not running on wgpu".to_owned(),
        };
        log::info!("wgpu adapter: {adapter_summary}");

        Self {
            name: "world".to_owned(),
            counter: 0,
            adapter_summary,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show(ui, |ui| {
            ui.horizontal(|ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                ui.separator();
                ui.label(if frame.is_web() { "Running on the web" } else { "Running natively" });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("egui + wgpu");

            ui.horizontal(|ui| {
                ui.label("Your name:");
                ui.text_edit_singleline(&mut self.name);
            });
            ui.label(format!("Hello, {}!", self.name));

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("−").clicked() {
                    self.counter -= 1;
                }
                ui.label(self.counter.to_string());
                if ui.button("+").clicked() {
                    self.counter += 1;
                }
            });
            ui.add(egui::Slider::new(&mut self.counter, -100..=100).text("counter"));

            ui.separator();

            ui.label("wgpu adapter:");
            ui.monospace(&self.adapter_summary);
        });
    }
}
