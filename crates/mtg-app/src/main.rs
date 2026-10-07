//! `mtg-gui` — the application window: menu, decks, and matches against the bot or over the
//! network. See [`mtg_app::app`].

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args_os().collect();
    let skip = args.iter().any(|a| a == "--skip-update-once");
    let error = args
        .windows(2)
        .find(|a| a[0] == "--update-error")
        .and_then(|a| std::fs::read_to_string(&a[1]).ok());
    run(skip, error)
}

fn run(skip: bool, error: Option<String>) -> eframe::Result {
    let restart = std::sync::Arc::new(std::sync::Mutex::new(None));
    let pending = restart.clone();
    eframe::run_native(
        "mtgo_rs",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(mtg_app::update::window::Launch::new(
                cc.egui_ctx.clone(),
                skip,
                error,
                pending,
            )))
        }),
    )?;
    let next = restart.lock().expect("update restart lock").take();
    if let Some(next) = next
        && let Err(error) = next.launch()
    {
        return run(true, Some(error));
    }
    Ok(())
}
