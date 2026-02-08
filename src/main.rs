mod analyzer;
mod app;
mod background;
mod journalctl;
mod ui;
mod workers;

use app::JlogApp;

fn main() -> eframe::Result<()> {
    #[cfg(target_os = "linux")]
    if std::env::var("WINIT_UNIX_BACKEND").is_err() {
        // SAFETY: called at program start before any other threads are spawned.
        unsafe { std::env::set_var("WINIT_UNIX_BACKEND", "x11") };
    }

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
