mod analyzer;
mod app;
mod background;
mod journalctl;
mod ui;
mod workers;

use app::JlogApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 800.0])
            .with_title("jlog - Log Viewer")
            ,
        ..Default::default()
    };

    eframe::run_native(
        "jlog",
        options,
        Box::new(|cc| Ok(Box::new(JlogApp::new(cc)))),
    )
}
