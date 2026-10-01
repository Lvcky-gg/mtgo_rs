//! On-demand printing discovery, away from the render thread.
use crate::{art::CardArt, widgets};
use egui::{RichText, Vec2};
use serde::Deserialize;
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize)]
struct Printing {
    id: String,
    oracle_id: String,
    set_name: String,
    collector_number: String,
    lang: String,
    #[serde(default)]
    digital: bool,
    image_uris: Option<ImageUris>,
    #[serde(default)]
    card_faces: Vec<PrintingFace>,
}

#[derive(Clone, Debug, Deserialize)]
struct ImageUris {
    normal: String,
}
#[derive(Clone, Debug, Deserialize)]
struct PrintingFace {
    image_uris: Option<ImageUris>,
}

impl Printing {
    fn image_uri(&self) -> Option<&str> {
        self.image_uris
            .as_ref()
            .or_else(|| self.card_faces.first()?.image_uris.as_ref())
            .map(|uris| uris.normal.as_str())
    }

    fn label(&self) -> String {
        format!(
            "{} · #{} · {}{}",
            self.set_name,
            self.collector_number,
            self.lang.to_uppercase(),
            if self.digital { " · digital" } else { "" }
        )
    }
}

#[derive(Deserialize)]
struct Page {
    data: Vec<Printing>,
    has_more: bool,
    next_page: Option<String>,
}

pub struct Picker {
    pub oracle: u32,
    name: String,
    uuid: String,
    ready: Receiver<Result<Vec<Printing>, String>>,
    result: Option<Result<Vec<Printing>, String>>,
    filter: String,
}

impl Picker {
    pub fn start(ctx: egui::Context, oracle: u32, uuid: String, name: String) -> Self {
        let (send, ready) = mpsc::channel();
        let worker_uuid = uuid.clone();
        std::thread::spawn(move || {
            let _ = send.send(fetch_printings(&worker_uuid));
            ctx.request_repaint();
        });
        Self {
            oracle,
            name,
            uuid,
            ready,
            result: None,
            filter: String::new(),
        }
    }

    /// Selection has an outer option for an action, and an inner option for default art.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        art: &mut CardArt,
        selected: Option<&str>,
    ) -> (bool, Option<Option<String>>) {
        if let Ok(result) = self.ready.try_recv() {
            self.result = Some(result);
        }
        let mut open = true;
        let mut selection = None;
        egui::Window::new(format!("Printings · {}", self.name))
            .id(egui::Id::new("printing-picker"))
            .open(&mut open)
            .default_size(Vec2::new(740.0, 620.0))
            .show(ctx, |ui| {
                ui.label("Choose artwork for all copies of this card in your deck.");
                if ui
                    .selectable_label(selected.is_none(), "Default printing")
                    .clicked()
                {
                    selection = Some(None);
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter by set, collector number, or language"),
                );
                match &self.result {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading printings…");
                        });
                    }
                    Some(Err(error)) => {
                        ui.colored_label(crate::theme::WARN, error);
                        if ui.button("Retry").clicked() {
                            let fresh = Self::start(
                                ctx.clone(),
                                self.oracle,
                                self.uuid.clone(),
                                self.name.clone(),
                            );
                            self.ready = fresh.ready;
                            self.result = None;
                        }
                    }
                    Some(Ok(printings)) => {
                        ui.label(
                            RichText::new(format!(
                                "{} printings · hover a card to enlarge",
                                printings.len()
                            ))
                            .color(crate::theme::MUTED),
                        );
                        let filter = self.filter.to_lowercase();
                        let width = 146.0;
                        let columns = ((ui.available_width() + 16.0) / (width + 16.0))
                            .floor()
                            .max(1.0) as usize;
                        let visible: Vec<_> = printings
                            .iter()
                            .filter(|p| p.label().to_lowercase().contains(&filter))
                            .collect();
                        if visible.is_empty() {
                            ui.label("No printings match this filter.");
                        }
                        ui.spacing_mut().item_spacing.y = 16.0;
                        egui::ScrollArea::vertical().show_rows(
                            ui,
                            270.0,
                            visible.len().div_ceil(columns),
                            |ui, rows| {
                                egui::Grid::new("printing-grid")
                                    .num_columns(columns)
                                    .spacing(Vec2::splat(16.0))
                                    .show(ui, |ui| {
                                        for (index, printing) in visible
                                            .iter()
                                            .skip(rows.start * columns)
                                            .take(rows.len() * columns)
                                            .enumerate()
                                        {
                                            ui.vertical(|ui| {
                                                ui.set_width(width);
                                                ui.set_min_height(270.0);
                                                let size = Vec2::new(width, width * 680.0 / 488.0);
                                                let response = if let Some(texture) = art
                                                    .printing(
                                                        ctx,
                                                        &printing.id,
                                                        printing.image_uri(),
                                                    )
                                                    .cloned()
                                                {
                                                    ui.add(
                                                        egui::Image::new(&texture)
                                                            .fit_to_exact_size(size)
                                                            .sense(egui::Sense::click()),
                                                    )
                                                } else {
                                                    ui.add_sized(
                                                        size,
                                                        egui::Button::new(&self.name),
                                                    )
                                                };
                                                let response = response.on_hover_ui(|ui| {
                                                    ui.label(printing.label());
                                                    widgets::offer_printing_enlargement(
                                                        ui,
                                                        &self.name,
                                                        Some(&printing.id),
                                                    );
                                                    if let Some(texture) = art
                                                        .printing(
                                                            ctx,
                                                            &printing.id,
                                                            printing.image_uri(),
                                                        )
                                                        .cloned()
                                                    {
                                                        ui.add(
                                                            egui::Image::new(&texture)
                                                                .fit_to_exact_size(Vec2::new(
                                                                    260.0,
                                                                    260.0 * 680.0 / 488.0,
                                                                )),
                                                        );
                                                    }
                                                });
                                                if response.clicked() {
                                                    selection = Some(Some(printing.id.clone()));
                                                }
                                                if selected == Some(printing.id.as_str()) {
                                                    ui.painter().rect_stroke(
                                                        response.rect,
                                                        6,
                                                        egui::Stroke::new(2.0, crate::theme::GOLD),
                                                        egui::StrokeKind::Inside,
                                                    );
                                                }
                                                ui.label(
                                                    RichText::new(printing.label()).size(12.0),
                                                );
                                            });
                                            if (index + 1) % columns == 0 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                            },
                        );
                    }
                }
            });
        if selection.is_some() {
            open = false;
        }
        (open, selection)
    }
}

fn fetch_printings(uuid: &str) -> Result<Vec<Printing>, String> {
    if uuid.len() != 36 || !uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("This card has no Scryfall identity; no printings are available.".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .into();
    let mut url = format!(
        "https://api.scryfall.com/cards/search?q=oracleid%3A{uuid}&unique=prints&include_extras=true&include_multilingual=true&include_variations=true"
    );
    let mut all = Vec::new();
    loop {
        if !url.starts_with("https://api.scryfall.com/") {
            return Err("Unexpected printing pagination URL.".into());
        }
        crate::art::wait_api_turn(&url);
        let body = agent
            .get(&url)
            .header(
                "User-Agent",
                concat!("mtgo_rs/", env!("CARGO_PKG_VERSION"), " (personal client)"),
            )
            .header("Accept", "application/json")
            .call()
            .map_err(|e| format!("Could not load printings: {e}"))?
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("Could not read printings: {e}"))?;
        let page: Page = serde_json::from_str(&body)
            .map_err(|e| format!("Could not read printing data: {e}"))?;
        all.extend(page.data.into_iter().filter(|p| p.oracle_id == uuid));
        if !page.has_more {
            break;
        }
        url = page
            .next_page
            .ok_or("Printing response is missing its next page.")?;
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printing_picker_hover_offers_enlargement_without_selecting() {
        let ctx = egui::Context::default();
        let (_, ready) = mpsc::channel();
        let mut picker = Picker {
            oracle: 1,
            name: "Sample card".into(),
            uuid: "sample".into(),
            ready,
            result: Some(Ok(vec![Printing {
                id: "sample-printing".into(),
                oracle_id: "sample".into(),
                set_name: "Sample set".into(),
                collector_number: "12".into(),
                lang: "ja".into(),
                digital: false,
                image_uris: None,
                card_faces: vec![],
            }])),
            filter: String::new(),
        };
        let mut art = CardArt::start(
            ctx.clone(),
            crate::art::ArtConfig {
                names: Default::default(),
                db: None,
                cache_dir: std::env::temp_dir().join("mtgo-printing-hover-test"),
            },
        );
        let mut pos = None;
        for _ in 0..3 {
            let mut output = ctx.run_ui(Default::default(), |_| {
                picker.show(&ctx, &mut art, None);
            });
            output.textures_delta.clear();
            pos = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == "Sample card" => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .or(pos);
        }
        let pos = pos.expect("printing tile should render");
        let mut offered = false;
        for time in [1.0, 2.0, 3.0] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events: if time == 1.0 {
                        vec![egui::Event::PointerMoved(pos)]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |_| {
                    let (_, selection) = picker.show(&ctx, &mut art, None);
                    assert!(selection.is_none(), "hover must not change artwork");
                },
            );
            output.textures_delta.clear();
            offered |= output.shapes.iter().any(|shape| matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Enlarge card"));
        }
        assert!(offered);
    }

    #[test]
    fn double_faced_printings_use_the_front_face_image() {
        let printing: Printing = serde_json::from_str(r#"{
            "id":"sample", "oracle_id":"oracle", "set_name":"Set", "collector_number":"1", "lang":"en",
            "card_faces":[{"image_uris":{"normal":"https://cards.scryfall.io/front.jpg"}},{"image_uris":{"normal":"https://cards.scryfall.io/back.jpg"}}]
        }"#).unwrap();
        assert_eq!(
            printing.image_uri(),
            Some("https://cards.scryfall.io/front.jpg")
        );
    }

    #[test]
    #[ignore = "requires a locally downloaded Scryfall response"]
    fn downloaded_printing_response_decodes() {
        let path = std::env::var("MTGO_PRINTING_RESPONSE").expect("set response path");
        let page: Page = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(!page.data.is_empty());
        assert!(
            page.data
                .iter()
                .all(|printing| printing.image_uri().is_some())
        );
    }
}
