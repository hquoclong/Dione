mod app;

fn main() -> eframe::Result<()> {
    // Route panics to stderr with a clear marker instead of a GUI-less abort.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("[ade panic] {info}");
    }));

    let config = ade_core::AppConfig::load();
    let rt = ade_core::runtime::spawn(config);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ADE — Agent Development Environment")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ade",
        options,
        Box::new(move |cc| Ok(Box::new(app::AdeApp::new(cc, rt)))),
    )
}
