mod app;

use app::App;

/// Native entry point: winit window + wgpu (Metal / Vulkan / DX12).
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([480.0, 360.0])
            .with_min_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native(
        "egui + wgpu",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// Web entry point: renders into the `<canvas>` in `index.html` using WebGPU
/// (falling back to WebGL2 in browsers without WebGPU).
#[cfg(target_arch = "wasm32")]
fn main() {
    use web_sys::wasm_bindgen::JsCast as _;

    // Redirect `log` messages to the browser console.
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id is not a <canvas>");

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(App::new(cc)))),
            )
            .await;

        // Replace the loading text with an error message if startup failed.
        if let Some(loading) = document.get_element_by_id("loading_text") {
            match result {
                Ok(()) => loading.remove(),
                Err(err) => {
                    loading.set_inner_html("<p>The app has crashed. See the developer console for details.</p>");
                    panic!("failed to start eframe: {err:?}");
                }
            }
        }
    });
}
