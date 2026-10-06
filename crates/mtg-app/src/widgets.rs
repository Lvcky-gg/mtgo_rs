//! Drawing shared by every screen: text with mana symbols in it, as labels and buttons.
//!
//! Symbols are Scryfall's images once they arrive (see `art`), and a plain disc in the symbol's
//! colour until then, so a cost always reads.

use egui::{Color32, RichText, Stroke, Ui, Vec2};

use crate::{
    art::CardArt,
    mana_text::{self, Piece},
};

/// An explicit action inside card hovers; selection clicks retain their meaning.
pub fn offer_card_enlargement(ui: &mut Ui, name: &str) {
    offer_printing_enlargement(ui, name, None);
}

pub fn offer_printing_enlargement(ui: &mut Ui, name: &str, printing: Option<&str>) {
    if ui.button("Enlarge card").clicked() {
        enlarge_printing(ui.ctx(), name, printing);
    }
}

/// Open inspection without changing card selection or deck composition.
pub fn enlarge_printing(ctx: &egui::Context, name: &str, printing: Option<&str>) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("enlarged-card"),
            printing.map_or_else(|| name.to_owned(), |id| format!("scryfall:{id}")),
        );
        data.insert_temp(egui::Id::new("enlarged-card-title"), name.to_owned());
    });
}

/// Shared persistent inspection window for match, builder, and sample cards.
pub fn show_enlarged_card(ui: &mut Ui, art: &mut CardArt) {
    let key = egui::Id::new("enlarged-card");
    let Some(name) = ui.ctx().data(|data| data.get_temp::<String>(key)) else {
        return;
    };
    let mut open = true;
    let title = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("enlarged-card-title")))
        .unwrap_or_else(|| name.clone());
    egui::Window::new(&title)
        .id(key)
        .open(&mut open)
        .resizable(true)
        .default_size(Vec2::new(488.0, 680.0))
        .show(ui.ctx(), |ui| {
            if let Some(texture) = art.get(ui.ctx(), &name).cloned() {
                let height = ui.available_height().max(150.0);
                let width = ui.available_width().max(100.0);
                let scale = (width / 488.0).min(height / 680.0);
                ui.add(
                    egui::Image::new(&texture).fit_to_exact_size(Vec2::new(488.0, 680.0) * scale),
                );
            } else {
                ui.label(&name);
                ui.label("Card image is loading or unavailable.");
            }
        });
    if !open {
        ui.ctx().data_mut(|data| data.remove::<String>(key));
    }
}

/// Paint text with its mana symbols inline, wrapped to `width`, from `origin`. Returns the
/// size it took. Labels, buttons and the hand all draw symbols through here or through
/// [`symbol_image`], so a symbol looks the same everywhere.
pub fn paint_mana_text(
    art: &mut CardArt,
    painter: &egui::Painter,
    origin: egui::Pos2,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> Vec2 {
    let (placed, lines, metrics) = layout_mana_text(painter, text, &font, width);
    let mut right: f32 = 0.0;
    for p in placed {
        let top = origin.y + p.line as f32 * metrics.line;
        match p.piece {
            Piece::Text(t) => {
                let galley = painter.layout_no_wrap(t.to_string(), font.clone(), color);
                let y = top + (metrics.line - galley.size().y) / 2.0;
                right = right.max(p.x + galley.size().x);
                painter.galley(egui::pos2(origin.x + p.x, y), galley, color);
            }
            Piece::Symbol(code) => {
                let y = top + (metrics.line - metrics.symbol) / 2.0;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(origin.x + p.x, y),
                    Vec2::splat(metrics.symbol),
                );
                paint_symbol(art, painter, rect, code);
                right = right.max(p.x + metrics.symbol);
            }
        }
    }
    Vec2::new(right, lines as f32 * metrics.line)
}

/// A label whose `{…}` symbols are drawn as symbols. Wraps to the space available.
pub fn mana_label(
    ui: &mut Ui,
    art: &mut CardArt,
    text: &str,
    size: f32,
    color: Color32,
) -> egui::Response {
    let font = egui::FontId::proportional(size);
    let width = ui.available_width().max(size * 4.0);
    let (placed, lines, metrics) = layout_mana_text(ui.painter(), text, &font, width);
    let used = placed
        .iter()
        .map(|p| {
            p.x + match p.piece {
                Piece::Text(t) => text_width(ui.painter(), t, &font),
                Piece::Symbol(_) => metrics.symbol,
            }
        })
        .fold(0.0, f32::max);
    let size = Vec2::new(used, lines as f32 * metrics.line);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter().clone();
    paint_mana_text(art, &painter, rect.min, text, font, color, width);
    response
}

/// A button whose label may hold symbols. A symbol whose image has not arrived shows as its
/// braces, so the button always says what it does.
pub fn mana_button(
    ui: &mut Ui,
    art: &mut CardArt,
    text: &str,
    color: Option<Color32>,
) -> egui::Response {
    let size = ui.text_style_height(&egui::TextStyle::Button);
    let mut atoms = egui::Atoms::default();
    for piece in mana_text::parse(text) {
        match piece {
            Piece::Text(t) => {
                let t = RichText::new(t);
                atoms.push_right(match color {
                    Some(c) => t.color(c).strong(),
                    None => t,
                });
            }
            Piece::Symbol(code) => match symbol_image(art, ui.ctx(), code, size) {
                Some(image) => atoms.push_right(image),
                None => atoms.push_right(format!("{{{code}}}")),
            },
        }
    }
    ui.add(egui::Button::new(atoms))
}

/// A symbol's image at `size`, once its texture has arrived.
pub fn symbol_image(
    art: &mut CardArt,
    ctx: &egui::Context,
    code: &str,
    size: f32,
) -> Option<egui::Image<'static>> {
    let texture = art.symbol(ctx, code)?;
    let sized = egui::load::SizedTexture::new(texture.id(), Vec2::splat(size));
    Some(egui::Image::from_texture(sized))
}

/// One symbol: Scryfall's image once it has arrived, else a plain disc in its colour with its
/// code on it, so a cost is readable from the first frame and offline.
pub fn paint_symbol(art: &mut CardArt, painter: &egui::Painter, rect: egui::Rect, code: &str) {
    if let Some(texture) = art.symbol(painter.ctx(), code) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture.id(), rect, uv, Color32::WHITE);
        return;
    }
    let colors = mana_text::symbol_colors(code);
    let fill = colors
        .first()
        .map_or(Color32::from_gray(190), |c| symbol_fill(*c));
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    if let Some(second) = colors.get(1) {
        // A hybrid: the second colour as a ring, so both halves read at a glance.
        painter.circle_stroke(
            rect.center(),
            rect.width() / 2.0 - 1.0,
            Stroke::new(rect.width() * 0.18, symbol_fill(*second)),
        );
    }
    let label: String = code.chars().filter(|c| *c != '/').collect();
    let scale = if label.chars().count() > 1 { 0.5 } else { 0.7 };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(rect.height() * scale),
        Color32::from_gray(20),
    );
}

/// How big symbols and lines are for a font: a symbol a little taller than a capital, and a line
/// tall enough to hold one without touching the next.
struct Metrics {
    symbol: f32,
    line: f32,
}

fn layout_mana_text<'a>(
    painter: &egui::Painter,
    text: &'a str,
    font: &egui::FontId,
    width: f32,
) -> (Vec<mana_text::Placed<'a>>, usize, Metrics) {
    let metrics = Metrics {
        symbol: font.size * 1.05,
        line: font.size * 1.35,
    };
    let pieces = mana_text::parse(text);
    // One point of space after a symbol, so adjacent symbols in a cost do not touch.
    let measure = |t: &str| text_width(painter, t, font);
    let (placed, lines) = mana_text::flow(&pieces, measure, metrics.symbol + 1.0, width);
    (placed, lines, metrics)
}

fn text_width(painter: &egui::Painter, text: &str, font: &egui::FontId) -> f32 {
    painter
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
        .size()
        .x
}

/// A symbol's fallback colour: close to the printed hue, legible under dark text.
fn symbol_fill(color: char) -> Color32 {
    match color {
        'W' => Color32::from_rgb(248, 231, 185),
        'U' => Color32::from_rgb(170, 224, 250),
        'B' => Color32::from_rgb(203, 194, 191),
        'R' => Color32::from_rgb(249, 170, 143),
        'G' => Color32::from_rgb(155, 211, 174),
        _ => Color32::from_gray(190),
    }
}
