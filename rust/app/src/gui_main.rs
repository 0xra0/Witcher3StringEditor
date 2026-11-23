//! GUI entry point for Witcher3StringEditor desktop app.
#![forbid(unsafe_code)]

mod gui;

use eframe::egui;
use gui::app::Witcher3StringEditorApp;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("The Witcher 3 String Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "The Witcher 3 String Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(Witcher3StringEditorApp::new(cc)))),
    )
}
