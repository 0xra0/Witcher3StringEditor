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

    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).filter(|s| !s.starts_with("--")).map(std::path::PathBuf::from);
    let dialog_arg = args.iter().position(|a| a == "--dialog").and_then(|idx| args.get(idx + 1)).cloned();

    eframe::run_native(
        "The Witcher 3 String Editor",
        native_options,
        Box::new(move |cc| {
            let mut app = Witcher3StringEditorApp::new(cc);
            if let Some(path) = initial_file {
                app.load_file(&path);
            }
            if let Some(ref d) = dialog_arg {
                match d.as_str() {
                    "backup" => app.show_backup = true,
                    "translate" => app.show_translate = true,
                    "settings" => app.show_settings = true,
                    "dict" => app.show_dict = true,
                    "recent" => app.show_recent = true,
                    "log" => app.show_log = true,
                    "about" => app.show_about = true,
                    _ => {}
                }
            }
            Ok(Box::new(app))
        }),
    )
}
