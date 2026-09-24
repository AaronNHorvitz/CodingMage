//! `CodingMage` native desktop entry point.

fn main() -> std::process::ExitCode {
    let fonts = match codingmage_ui::fonts::select_fonts()
        .and_then(|selected| codingmage_ui::fonts::font_definitions(&selected))
    {
        Ok(fonts) => fonts,
        Err(error) => {
            eprintln!("codingmage-ui: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CodingMage")
            .with_app_id("codingmage-ui")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size(codingmage_ui::app::MIN_WINDOW),
        ..Default::default()
    };
    let outcome = eframe::run_native(
        "CodingMage",
        options,
        Box::new(move |creation| {
            creation.egui_ctx.set_fonts(fonts);
            Ok(Box::new(codingmage_ui::App::new(&creation.egui_ctx)))
        }),
    );
    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("codingmage-ui: the native window could not be created: {error}");
            std::process::ExitCode::from(3)
        }
    }
}
