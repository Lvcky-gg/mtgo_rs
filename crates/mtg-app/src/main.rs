//! `mtg-gui` — the application window: menu, decks, and matches against the bot or over the
//! network. See [`mtg_app::app`].

fn main() -> eframe::Result {
    eframe::run_native(
        "mtgo_rs",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(mtg_app::app::App::new(cc.egui_ctx.clone())))),
    )
}
