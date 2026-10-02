//! The window.
//!
//! Draws a [`PlayerView`] and whatever question is outstanding, and sends back an answer. It holds
//! no rules: every button comes from the engine's own legal-action list, so there is nothing here
//! that could offer an illegal play.
//!
//! A card with an image is drawn as that image, turned sideways when tapped, with its text on
//! hover. One without is a plain panel — a border, the name and cost, the type line,
//! power/toughness and the printed text.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{Receiver, Sender},
};

use egui::{Color32, RichText, Stroke, Ui, Vec2};
use mtg_core::ObjectId;
use mtg_engine::{ChoiceKind, PlayerView, actions::Action, choice::Answer};

use crate::{
    Question,
    art::CardArt,
    board::{self, Board},
    cards_text::CardTexts,
    format, mana_text, widgets,
};

use crate::theme::{
    BORDER as CARD_EDGE, GOLD as TAPPED_EDGE, GOLD as ACCENT, SURFACE as CARD_FILL,
};
/// Attacking creatures.
const ATTACK: Color32 = Color32::from_rgb(220, 90, 80);

/// A card image on the board, upright. Scryfall's images are 488×680.
const ART_SIZE: Vec2 = Vec2::new(120.0, 120.0 * 680.0 / 488.0);

/// A card in hand: the same size as on the board, so a card does not change size as it is played.
const HAND_CARD: Vec2 = ART_SIZE;
const HAND_GAP: f32 = 8.0;
/// egui's "small" text size, which the board panels use.
const SMALL_TEXT: f32 = 10.0;
/// Give rendering a turn even when a policy produces many positions at once.
const VIEW_BATCH_LIMIT: usize = 128;
/// How much larger the card under the pointer is drawn, so it can be read without leaving the hand.
const HAND_ZOOM: f32 = 1.8;

pub struct GuiApp {
    questions: Receiver<Question>,
    /// Every view the game passes through, for narration and a live board.
    views: Option<Receiver<PlayerView>>,
    answers: Sender<Answer>,
    texts: CardTexts,
    art: CardArt,
    auto_answered: Arc<AtomicU64>,

    /// The outstanding question, if any.
    current: Option<Question>,
    last_answered_question: Option<u64>,
    rejected_answer: bool,
    /// The last position seen, so the board stays drawn between questions.
    last_view: Option<PlayerView>,
    /// Targets or objects picked so far, for a multi-select question.
    picked: Vec<ObjectId>,
    /// Modes picked so far, for a modal spell that takes more than one.
    picked_modes: Vec<u8>,
    /// Numeric value being edited for a large X range.
    picked_x: u32,
    /// One selected target per slot, preserving the engine's slot order.
    picked_targets: std::collections::BTreeMap<usize, mtg_core::Target>,
    /// Target slots ordered by their most recent selection.
    target_pick_order: Vec<usize>,
    /// Blocker amounts being edited instead of accepting the default damage.
    custom_damage: Option<Vec<(ObjectId, u32)>>,
    /// Destination chosen for each selected attacker.
    attack_destinations: std::collections::BTreeMap<ObjectId, mtg_core::Target>,
    /// (blocker, attacker) pairs declared so far.
    blocks: Vec<(ObjectId, ObjectId)>,
    /// A blocker clicked on the board, waiting for the attacker it should block.
    pending_blocker: Option<ObjectId>,
    play_choice: Option<ObjectId>,
    /// How large permanents are drawn: shrunk when the board has more rows than fit.
    board_scale: f32,
    board_zoom: Option<f32>,
    journal_open: bool,
    /// A graveyard or exile being looked through, by whose and which.
    zone_view: Option<(mtg_core::PlayerId, mtg_core::Zone)>,
    /// Triggers being ordered, as batch indices in the order they will *resolve*.
    trigger_order: Vec<usize>,
    /// Blocker indices in the player's chosen damage-assignment order.
    blocker_order: Vec<usize>,
    /// The first click on Concede asks for a second.
    confirm_concede: bool,
    /// A running log, newest last.
    log: Vec<String>,
    finished: bool,
}

impl GuiApp {
    pub fn new(
        questions: Receiver<Question>,
        answers: Sender<Answer>,
        texts: CardTexts,
        art: CardArt,
        auto_answered: Arc<AtomicU64>,
    ) -> Self {
        Self {
            questions,
            views: None,
            answers,
            texts,
            art,
            auto_answered,
            current: None,
            last_answered_question: None,
            rejected_answer: false,
            last_view: None,
            picked: Vec::new(),
            picked_modes: Vec::new(),
            picked_x: 0,
            picked_targets: Default::default(),
            target_pick_order: Vec::new(),
            custom_damage: None,
            attack_destinations: Default::default(),
            blocks: Vec::new(),
            pending_blocker: None,
            play_choice: None,
            board_scale: 1.0,
            board_zoom: None,
            journal_open: true,
            zone_view: None,
            trigger_order: Vec::new(),
            blocker_order: Vec::new(),
            confirm_concede: false,
            log: Vec::new(),
            finished: false,
        }
    }

    /// Follow every position the game passes through, not only the ones that ask something.
    pub fn watch(&mut self, views: Receiver<PlayerView>) {
        self.views = Some(views);
    }

    /// Move the board to `view`, logging what changed on the way.
    fn advance_to(&mut self, view: PlayerView) {
        if let Some(before) = &self.last_view {
            self.log
                .extend(crate::narrate::changes(before, &view, &self.texts));
        }
        self.last_view = Some(view);
    }

    fn poll(&mut self) -> bool {
        let observed: Vec<PlayerView> = self
            .views
            .as_ref()
            .map(|rx| rx.try_iter().take(VIEW_BATCH_LIMIT).collect())
            .unwrap_or_default();
        let full_batch = observed.len() == VIEW_BATCH_LIMIT;
        for view in observed {
            self.advance_to(view);
        }
        // A question's view is newer than the preceding observations. Drain those
        // first so a subsequent frame cannot roll the board back behind its question.
        if full_batch {
            return true;
        }
        if self.current.is_some() {
            return false;
        }
        match self.questions.try_recv() {
            Ok(q) => {
                let rejected = self.last_answered_question == Some(q.choice.id);
                self.advance_to(q.view.clone());
                self.clear_selection();
                self.rejected_answer = rejected;
                self.last_answered_question = None;
                self.current = Some(q);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.finished = true,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        false
    }

    fn answer(&mut self, answer: Answer) {
        self.last_answered_question = if matches!(&answer, Answer::Undo) {
            None
        } else {
            self.current.as_ref().map(|q| q.choice.id)
        };
        if self.answers.send(answer).is_err() {
            self.finished = true;
        }
        self.current = None;
        self.clear_selection();
    }

    /// Ctrl+Z. A selection still being made is taken back first, one pick at a time, since
    /// it is the most recent thing the player did; after that, the engine's own undo.
    ///
    /// The engine decides whether a game action can be taken back — the question carries
    /// that — so nothing here can undo past what an opponent has seen.
    fn undo(&mut self) {
        if self.custom_damage.take().is_some()
            || self.play_choice.take().is_some()
            || self.pending_blocker.take().is_some()
            || self.blocks.pop().is_some()
            || self.picked.pop().is_some()
            || self.picked_modes.pop().is_some()
        {
            return;
        }
        if let Some(slot) = self.target_pick_order.pop() {
            self.picked_targets.remove(&slot);
            return;
        }
        if self.current.as_ref().is_some_and(|q| q.choice.undo) {
            self.log.push("undone".into());
            self.answer(Answer::Undo);
        }
    }

    fn has_selection(&self) -> bool {
        self.custom_damage.is_some()
            || self.play_choice.is_some()
            || self.pending_blocker.is_some()
            || !self.blocks.is_empty()
            || !self.picked.is_empty()
            || !self.picked_modes.is_empty()
            || !self.picked_targets.is_empty()
    }

    /// Selections belong to one question; a new one starts clean.
    fn clear_selection(&mut self) {
        self.rejected_answer = false;
        self.picked.clear();
        self.picked_modes.clear();
        self.picked_x = 0;
        self.picked_targets.clear();
        self.target_pick_order.clear();
        self.custom_damage = None;
        self.attack_destinations.clear();
        self.blocks.clear();
        self.pending_blocker = None;
        self.play_choice = None;
        self.trigger_order.clear();
        self.blocker_order.clear();
        self.confirm_concede = false;
    }

    fn action_label(&self, view: &PlayerView, action: &Action) -> String {
        if let Action::ActivateAbility { source, ability } = action {
            let object = board::object(view, *source);
            self.texts.activation_label_face(
                object.and_then(|o| o.card),
                object.map_or(0, |o| o.face),
                *ability,
            )
        } else if let Action::CastAlternative { object, ability } = action {
            let object = board::object(view, *object);
            self.texts.alternative_cast_label_face(
                object.and_then(|o| o.card),
                object.map_or(0, |o| o.face),
                *ability,
            )
        } else if let Action::SpecialAction { source, ability } = action {
            let object = board::object(view, *source);
            self.texts.special_action_label_face(
                object.and_then(|o| o.card),
                object.map_or(0, |o| o.face),
                *ability,
            )
        } else {
            format::action_label(action, |id| {
                if let Some((object, face, _)) = action.play()
                    && object == id
                {
                    let card = board::object(view, id).and_then(|o| o.card);
                    let mut name = self.texts.face_name(card, face);
                    if board::object(view, id).is_some_and(|o| {
                        o.zone.zone == mtg_core::Zone::Exile
                            && o.adventure_player == Some(view.viewer)
                    }) {
                        name.push_str(" (from adventure)");
                    } else if let Some(object) = board::object(view, id) {
                        match object.zone.zone {
                            mtg_core::Zone::Graveyard => name.push_str(" (from graveyard)"),
                            mtg_core::Zone::Exile => name.push_str(" (from exile)"),
                            mtg_core::Zone::Library => name.push_str(" (from library)"),
                            mtg_core::Zone::Command => name.push_str(" (from command zone)"),
                            _ => {}
                        }
                    }
                    let cost = card
                        .and_then(|c| self.texts.get_face(c, face))
                        .map(|t| t.cost.as_str())
                        .unwrap_or("");
                    if cost.is_empty() {
                        name
                    } else {
                        format!("{name} {cost}")
                    }
                } else {
                    self.name_of(view, id)
                }
            })
        }
    }

    fn target_name(&self, view: &PlayerView, target: mtg_core::Target) -> String {
        match target {
            mtg_core::Target::Object(o) => self.name_of(view, o),
            mtg_core::Target::Player(p) => format::player_name(view, p),
        }
    }

    fn attack_answer(&self, attackers: &[ObjectId], default: Option<mtg_core::Target>) -> Answer {
        if let Some(default) = default {
            Answer::Attackers(
                attackers
                    .iter()
                    .map(|a| {
                        (
                            *a,
                            self.attack_destinations.get(a).copied().unwrap_or(default),
                        )
                    })
                    .collect(),
            )
        } else {
            Answer::Objects(attackers.to_vec())
        }
    }

    fn name_of(&self, view: &PlayerView, id: ObjectId) -> String {
        board::object(view, id)
            .and_then(|o| self.texts.object_text(o))
            .map_or_else(|| "(hidden)".into(), |t| t.name.clone())
    }
}

impl GuiApp {
    /// Replace the card text snapshot: a match sends its own once its card table is built.
    pub fn set_texts(&mut self, texts: CardTexts) {
        self.texts = texts;
    }

    /// Hand back the image cache, so the next match reuses what this one loaded.
    pub fn into_art(self) -> CardArt {
        self.art
    }

    /// Whether the game has stopped asking questions: the match is over or has gone away.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Draw the game into the window: status bar, log, question, hand and board.
    pub fn show(&mut self, ui: &mut Ui) {
        if self.poll() {
            ui.ctx().request_repaint();
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        if !typing && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        // Space passes priority, as long as nothing is being typed into.
        if !typing
            && matches!(
                self.current.as_ref().map(|q| &q.choice.kind),
                Some(ChoiceKind::Priority { .. })
            )
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.answer(Answer::Pass);
        }
        let ctx = ui.ctx().clone();
        // The game runs on another thread, so keep repainting rather than waiting for input.
        ctx.request_repaint_after(std::time::Duration::from_millis(50));

        let view = self.last_view.clone();

        egui::Panel::top("status")
            .frame(crate::theme::panel())
            .show(ui, |ui| {
                if let Some(v) = &view {
                    phase_bar(ui, v, &self.texts);
                }
                ui.horizontal(|ui| {
                    match &view {
                        Some(v) => ui.label(RichText::new(format::status_line(v)).strong()),
                        None => ui.label("waiting for the game to start…"),
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Showing this is how a player sees the client working for them rather than
                        // wondering why it is not asking.
                        ui.label(
                            RichText::new(format!(
                                "{} handled for you",
                                self.auto_answered.load(Ordering::Relaxed)
                            ))
                            .color(ACCENT)
                            .small(),
                        );
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Card size")
                            .small()
                            .color(crate::theme::MUTED),
                    );
                    egui::ComboBox::from_id_salt("arena-card-size")
                        .selected_text(match self.board_zoom {
                            None => "Fit board",
                            Some(size) if size < 0.7 => "Small",
                            Some(size) if size < 0.9 => "Medium",
                            _ => "Large",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.board_zoom, None, "Fit board");
                            ui.selectable_value(&mut self.board_zoom, Some(0.6), "Small");
                            ui.selectable_value(&mut self.board_zoom, Some(0.8), "Medium");
                            ui.selectable_value(&mut self.board_zoom, Some(1.0), "Large");
                        });
                    let journal = ui
                        .selectable_label(self.journal_open, "Journal")
                        .on_hover_text(
                            "Show or hide the match journal to make room for the battlefield",
                        );
                    if journal.clicked() {
                        self.journal_open = !self.journal_open;
                    }
                });
            });

        if self.journal_open {
            egui::Panel::right("log")
                .frame(crate::theme::panel())
                .default_size(260.0)
                .min_size(190.0)
                .show(ui, |ui| {
                    ui.heading("Match journal");
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.log {
                                // Turn headers stand out, so a long log can be scanned by turn.
                                let text = RichText::new(line);
                                ui.label(if line.starts_with('—') {
                                    text.strong().color(ACCENT)
                                } else {
                                    text.color(crate::theme::MUTED).size(13.0)
                                });
                            }
                        });
                });
        }

        egui::Panel::bottom("question")
            .frame(crate::theme::panel())
            .min_size(120.0)
            .show(ui, |ui| {
                self.draw_question(ui);
            });

        // Declared after the question panel, so it sits just above it: the hand is next to the
        // buttons that act on it.
        egui::Panel::bottom("hand")
            .frame(egui::Frame::new().fill(crate::theme::INK).inner_margin(8))
            .exact_size(HAND_CARD.y + 42.0)
            .resizable(false)
            .show(ui, |ui| {
                if let Some(v) = &view {
                    let hand_size = v.players.get(&v.viewer).map_or(0, |player| player.hand_size);
                    ui.label(RichText::new(format!("Your hand · {hand_size}")).strong().small())
                        .on_hover_text("Hover over a card to inspect it. Click a highlighted card to play or select it.");
                    self.draw_hand(ui, v);
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::INK).inner_margin(12))
            .show(ui, |ui| match &view {
                Some(v) => {
                    let board = board::arrange(v);
                    self.draw_board(ui, v, &board);
                    self.zone_window(ui, v);
                }
                None => {
                    ui.centered_and_justified(|ui| ui.label("no position yet"));
                }
            });
        widgets::show_enlarged_card(ui, &mut self.art);
    }
}

impl GuiApp {
    fn draw_board(&mut self, ui: &mut Ui, view: &PlayerView, board: &Board) {
        // Shrink the cards when their rows would not fit, so the whole board is always in view.
        let rows = |side: &board::Side| {
            let (others, lands) = board::rows(&side.battlefield, |id| {
                board::object(view, id)
                    .and_then(|o| self.texts.object_text(o))
                    .is_some_and(|t| t.is_land)
            });
            usize::from(!others.is_empty()) + usize::from(!lands.is_empty())
        };
        let row_count = board.opponents.iter().map(rows).sum::<usize>() + rows(&board.mine);
        let stack_height = if board.stack.is_empty() {
            36.0
        } else {
            ART_SIZE.y * 0.7 + 56.0
        };
        let headers =
            76.0 * (board.opponents.len() + 1) as f32 + row_count as f32 * 22.0 + stack_height;
        let per_row = (ui.available_height() - headers) / row_count.max(1) as f32;
        self.board_scale = self
            .board_zoom
            .unwrap_or_else(|| (per_row / (ART_SIZE.y + 10.0)).clamp(0.45, 1.0));

        egui::ScrollArea::vertical().show(ui, |ui| {
            for side in &board.opponents {
                self.draw_side_panel(ui, view, side);
                ui.add_space(10.0);
            }

            if board.stack.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("STACK").size(10.0).color(crate::theme::MUTED));
                    ui.label(
                        RichText::new("No spells or abilities waiting to resolve")
                            .small()
                            .color(crate::theme::MUTED),
                    );
                });
            } else {
                crate::theme::surface().show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!("Stack · {}", board.stack.len())).strong(),
                            );
                            ui.label(
                                RichText::new("First card resolves next")
                                    .small()
                                    .color(crate::theme::MUTED),
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            let scale = std::mem::replace(&mut self.board_scale, 0.7);
                            for id in board.stack.iter().rev() {
                                let response = self.draw_card(ui, view, *id, false);
                                if board::object(view, *id).is_some_and(|o| o.is_ability) {
                                    tag(ui.painter(), response.rect, "ability", ACCENT);
                                }
                            }
                            self.board_scale = scale;
                        });
                    });
                });
            }
            ui.add_space(10.0);
            self.draw_side_panel(ui, view, &board.mine);
        });
    }

    fn draw_side_panel(&mut self, ui: &mut Ui, view: &PlayerView, side: &board::Side) {
        let active = view.active_player == side.player;
        egui::Frame::new()
            .fill(crate::theme::PANEL)
            .stroke(Stroke::new(
                1.0,
                if active {
                    ACCENT.gamma_multiply(0.5)
                } else {
                    CARD_EDGE
                },
            ))
            .corner_radius(12)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    self.draw_side(ui, view, side);
                });
            });
    }

    fn draw_side(&mut self, ui: &mut Ui, view: &PlayerView, side: &board::Side) {
        let visible_library = board::visible_library_cards(view, side.player);
        let exiled: Vec<ObjectId> = view
            .visible
            .values()
            .filter(|o| o.zone.zone == mtg_core::Zone::Exile && o.controller == side.player)
            .map(|o| o.id)
            .collect();
        let command: Vec<ObjectId> = view
            .visible
            .values()
            .filter(|o| {
                o.zone.zone == mtg_core::Zone::Command && o.zone.player == Some(side.player)
            })
            .map(|o| o.id)
            .collect();
        ui.horizontal_wrapped(|ui| {
            let who = format::player_name(view, side.player);
            ui.label(RichText::new(who).strong().size(16.0));
            if view.active_player == side.player {
                ui.label(RichText::new("ACTIVE TURN").size(10.0).color(ACCENT));
            }
            egui::Frame::new()
                .fill(CARD_FILL)
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(side.life.to_string())
                                .strong()
                                .size(24.0)
                                .color(if side.life <= 5 {
                                    ATTACK
                                } else {
                                    crate::theme::TEXT
                                }),
                        );
                        ui.label(RichText::new("LIFE").size(10.0).color(crate::theme::MUTED));
                    });
                });
            if side.poison > 0 {
                ui.label(RichText::new(format!("{} poison", side.poison)).color(TAPPED_EDGE));
            }
            if side.energy > 0 {
                ui.label(RichText::new(format!("{} energy", side.energy)).color(ACCENT));
            }
            if side.player == view.viewer {
                ui.label(RichText::new(format!("hand {}", side.hand_size)).weak());
            } else {
                // An opponent's hand: as many face-down cards as they hold.
                ui.label(RichText::new("hand").weak());
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(10.0 * side.hand_size.min(12) as f32 + 12.0, 18.0),
                    egui::Sense::hover(),
                );
                for i in 0..side.hand_size.min(12) {
                    let card = egui::Rect::from_min_size(
                        rect.min + Vec2::new(10.0 * i as f32, 0.0),
                        Vec2::new(13.0, 18.0),
                    );
                    ui.painter().rect(
                        card,
                        2.0,
                        Color32::from_rgb(60, 52, 44),
                        Stroke::new(1.0, Color32::from_rgb(120, 100, 80)),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.label(RichText::new(side.hand_size.to_string()).weak());
            }
            ui.label(RichText::new(format!("library {}", side.library_size)).weak());
            if !visible_library.is_empty() && ui.small_button("top card").clicked() {
                self.zone_view = Some((side.player, mtg_core::Zone::Library));
            }
            // Commander damage, the other way to lose a Commander game: 21 from one commander.
            for n in side.commander_damage.values() {
                let color = if *n >= 15 { ATTACK } else { TAPPED_EDGE };
                ui.label(RichText::new(format!("commander damage {n}/21")).color(color))
                    .on_hover_text("21 combat damage from a single commander loses the game");
            }
            // Graveyard and exile open a window listing the cards.
            let graveyard = format!("graveyard {}", side.graveyard.len());
            if ui
                .add_enabled(
                    !side.graveyard.is_empty(),
                    egui::Button::new(graveyard).small(),
                )
                .clicked()
            {
                self.zone_view = Some((side.player, mtg_core::Zone::Graveyard));
            }
            if !exiled.is_empty() && ui.small_button(format!("exile {}", exiled.len())).clicked() {
                self.zone_view = Some((side.player, mtg_core::Zone::Exile));
            }
            let pool = format::mana_pool(&side.mana);
            if !pool.is_empty() {
                ui.label(RichText::new("floating").color(ACCENT).strong());
                self.mana_label(ui, &pool, 14.0, ACCENT);
            }
        });

        // The command zone: a commander waiting to be cast, clickable when it can be.
        if !command.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Command zone").weak().small());
                for id in command {
                    let casts: Vec<Action> = match self.current.as_ref().map(|q| &q.choice.kind) {
                        Some(ChoiceKind::Priority { legal }) => legal
                            .actions
                            .iter()
                            .filter(|a| {
                                a.play()
                                    .is_some_and(|(object, _, land)| object == id && !land)
                            })
                            .cloned()
                            .collect(),
                        _ => Vec::new(),
                    };
                    let scale = std::mem::replace(&mut self.board_scale, 0.6);
                    let response = self.draw_card(ui, view, id, !casts.is_empty());
                    self.board_scale = scale;
                    if !casts.is_empty() {
                        pointer_cursor(&response);
                        if response.clicked() {
                            if casts.len() == 1 {
                                self.answer(Answer::Action(casts[0].clone()));
                            } else {
                                self.play_choice = Some(id);
                            }
                        }
                    }
                }
            });
        }

        // Creatures in front, lands behind — how a player reads a board.
        let (others, lands) = board::rows(&side.battlefield, |id| {
            board::object(view, id)
                .and_then(|o| self.texts.object_text(o))
                .is_some_and(|t| t.is_land)
        });

        // Mirrored for an opponent: their lands at the far edge, their creatures facing yours
        // across the middle, as across a table.
        let order = if side.player == view.viewer {
            [others, lands]
        } else {
            [lands, others]
        };
        if side.battlefield.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("No permanents on this battlefield")
                    .small()
                    .color(crate::theme::MUTED),
            );
        }
        for row in order {
            if row.is_empty() {
                continue;
            }
            let lands = row
                .first()
                .and_then(|id| board::object(view, *id))
                .and_then(|object| self.texts.object_text(object))
                .is_some_and(|text| text.is_land);
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    if lands { "LANDS" } else { "PERMANENTS" },
                    row.len()
                ))
                .size(10.0)
                .color(crate::theme::MUTED),
            );
            ui.horizontal_wrapped(|ui| {
                for id in row {
                    // A question about the board is answered on the board: attackers,
                    // blockers and targets are clicked, not picked from a list of names.
                    let pick = self.board_pick(id);
                    let options = self.permanent_options(id);
                    let response =
                        self.draw_card(ui, view, id, !options.is_empty() || pick.is_some());
                    if let Some(pick) = pick {
                        self.mark_pick(ui, view, id, &response);
                        pointer_cursor(&response);
                        if response.clicked() {
                            self.click_pick(id, pick);
                        }
                    } else {
                        self.offer_permanent_actions(view, &response, options);
                    }
                }
            });
        }
    }

    /// Disclosed cards in a graveyard, exile, or library, opened from the side header.
    fn zone_window(&mut self, ui: &mut Ui, view: &PlayerView) {
        let Some((who, zone)) = self.zone_view else {
            return;
        };
        let ids: Vec<ObjectId> = match zone {
            mtg_core::Zone::Library => board::visible_library_cards(view, who),
            mtg_core::Zone::Graveyard => view
                .players
                .get(&who)
                .map(|p| p.graveyard.clone())
                .unwrap_or_default(),
            _ => view
                .visible
                .values()
                .filter(|o| o.zone.zone == zone && o.controller == who)
                .map(|o| o.id)
                .collect(),
        };
        let whose = if who == view.viewer {
            "Your".to_string()
        } else {
            format!("{}'s", format::player_name(view, who))
        };
        let what = match zone {
            mtg_core::Zone::Graveyard => "graveyard",
            mtg_core::Zone::Library => "visible library cards",
            _ => "exiled cards",
        };
        let mut open = true;
        egui::Window::new(format!("{whose} {what} ({})", ids.len()))
            .open(&mut open)
            .default_width(700.0)
            .show(&ui.ctx().clone(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let scale = std::mem::replace(&mut self.board_scale, 0.8);
                            // Newest last in the view; show the most recent first.
                            for id in ids.iter().rev() {
                                let actions: Vec<Action> =
                                    match self.current.as_ref().map(|q| &q.choice.kind) {
                                        Some(ChoiceKind::Priority { legal }) => {
                                            board::hand_play_options(&legal.actions, &[*id])
                                                .remove(id)
                                                .unwrap_or_default()
                                        }
                                        _ => Vec::new(),
                                    };
                                let response = self.draw_card(ui, view, *id, !actions.is_empty());
                                if !actions.is_empty() {
                                    pointer_cursor(&response);
                                    if response.clicked() {
                                        if actions.len() == 1 {
                                            self.answer(Answer::Action(actions[0].clone()));
                                        } else {
                                            self.play_choice = Some(*id);
                                        }
                                    }
                                }
                            }
                            self.board_scale = scale;
                        });
                    });
            });
        if !open {
            self.zone_view = None;
        }
    }

    /// What clicking this object would do for the outstanding question, if anything.
    fn board_pick(&self, id: ObjectId) -> Option<BoardPick> {
        match self.current.as_ref().map(|q| &q.choice.kind)? {
            ChoiceKind::DeclareAttackers { eligible, .. } => {
                eligible.contains(&id).then_some(BoardPick::Toggle)
            }
            ChoiceKind::DeclareBlockers { eligible } => {
                if eligible.iter().any(|(b, _)| *b == id) {
                    Some(BoardPick::Blocker)
                } else if self.pending_blocker.is_some_and(|b| {
                    eligible
                        .iter()
                        .any(|(blocker, attackers)| *blocker == b && attackers.contains(&id))
                }) {
                    Some(BoardPick::Attacker)
                } else {
                    None
                }
            }
            ChoiceKind::ChooseTargets { slots } if slots.len() == 1 => slots[0]
                .contains(&mtg_core::Target::Object(id))
                .then_some(BoardPick::Target),
            ChoiceKind::ChooseObjects { from, .. } => {
                from.contains(&id).then_some(BoardPick::Toggle)
            }
            _ => None,
        }
    }

    fn click_pick(&mut self, id: ObjectId, pick: BoardPick) {
        match pick {
            BoardPick::Toggle => {
                let max = match self.current.as_ref().map(|q| &q.choice.kind) {
                    Some(ChoiceKind::ChooseObjects { max, .. }) => *max as usize,
                    _ => usize::MAX,
                };
                if self.picked.contains(&id) || self.picked.len() < max {
                    toggle(&mut self.picked, id);
                }
            }
            BoardPick::Blocker => {
                // Clicking a creature that is already blocking takes the block back.
                if self.blocks.iter().any(|(b, _)| *b == id) {
                    self.blocks.retain(|(b, _)| *b != id);
                    self.pending_blocker = None;
                } else {
                    self.pending_blocker = if self.pending_blocker == Some(id) {
                        None
                    } else {
                        Some(id)
                    };
                }
            }
            BoardPick::Attacker => {
                if let Some(blocker) = self.pending_blocker.take() {
                    self.blocks.retain(|(b, _)| *b != blocker);
                    self.blocks.push((blocker, id));
                }
            }
            BoardPick::Target => {
                self.answer(Answer::Targets(vec![vec![mtg_core::Target::Object(id)]]));
            }
        }
    }

    fn toggle_block(&mut self, blocker: ObjectId, attacker: ObjectId) {
        let chosen = self.blocks.contains(&(blocker, attacker));
        self.blocks.retain(|(b, _)| *b != blocker);
        if !chosen {
            self.blocks.push((blocker, attacker));
        }
        if self.pending_blocker == Some(blocker) {
            self.pending_blocker = None;
        }
    }

    /// Show a board choice on the card: picked, waiting for its attacker, or blocking one.
    fn mark_pick(&self, ui: &mut Ui, view: &PlayerView, id: ObjectId, response: &egui::Response) {
        let rect = response.rect;
        let painter = ui.painter();
        let strong = Stroke::new(3.5, TAPPED_EDGE);
        let note = |text: String| tag(painter, rect, &text, TAPPED_EDGE);
        if self.picked.contains(&id) {
            painter.rect_stroke(rect, 5.0, strong, egui::StrokeKind::Outside);
            if matches!(
                self.current.as_ref().map(|q| &q.choice.kind),
                Some(ChoiceKind::DeclareAttackers { .. })
            ) {
                note("ATTACKING".into());
            }
        }
        if self.pending_blocker == Some(id) {
            painter.rect_stroke(rect, 5.0, strong, egui::StrokeKind::Outside);
            note("blocks… (click an attacker)".into());
        }
        if let Some((_, attacker)) = self.blocks.iter().find(|(b, _)| *b == id) {
            painter.rect_stroke(rect, 5.0, strong, egui::StrokeKind::Outside);
            note(format!("blocks {}", self.name_of(view, *attacker)));
        }
    }

    /// What the engine currently offers to do with a permanent, if anything.
    fn permanent_options(&self, id: ObjectId) -> Vec<Action> {
        match self.current.as_ref().map(|q| &q.choice.kind) {
            Some(ChoiceKind::Priority { legal }) => board::permanent_actions(legal, id),
            _ => Vec::new(),
        }
    }

    /// Clicking a permanent: one option is taken at once — tapping a basic land is a single
    /// click — and several open a menu, e.g. a dual land's colours.
    fn offer_permanent_actions(
        &mut self,
        view: &PlayerView,
        response: &egui::Response,
        options: Vec<Action>,
    ) {
        if options.is_empty() {
            return;
        }
        pointer_cursor(response);
        if let [only] = options.as_slice() {
            let label = self.action_label(view, only);
            let hovered = response.clone().on_hover_ui_at_pointer(|ui| {
                let color = ui.visuals().text_color();
                self.mana_label(ui, &label, 14.0, color);
            });
            if hovered.clicked() {
                self.answer(Answer::Action(only.clone()));
            }
            return;
        }
        egui::Popup::menu(response).show(|ui| {
            for action in &options {
                let label = self.action_label(view, action);
                if self.mana_button(ui, &label, None).clicked() {
                    self.answer(Answer::Action(action.clone()));
                }
            }
        });
    }

    /// The viewer's hand, fanned along the bottom.
    ///
    /// Cards overlap once they no longer fit. The card under the pointer is drawn enlarged above
    /// the rest, and one the engine currently offers to play is outlined and plays on click.
    fn draw_hand(&mut self, ui: &mut Ui, view: &PlayerView) {
        let hand = board::arrange(view).mine.hand;
        if hand.is_empty() {
            ui.centered_and_justified(|ui| ui.label(RichText::new("no cards in hand").weak()));
            return;
        }

        // Playable only while the outstanding question is priority for this position.
        let plays = match self.current.as_ref().map(|q| &q.choice.kind) {
            Some(ChoiceKind::Priority { legal }) => board::hand_play_options(&legal.actions, &hand),
            _ => Default::default(),
        };
        // A question that picks cards out of this hand — bottoming after a mulligan, a
        // discard — is answered by clicking the cards themselves.
        let choosing: Option<(Vec<ObjectId>, usize)> =
            match self.current.as_ref().map(|q| &q.choice.kind) {
                Some(ChoiceKind::ChooseObjects { from, max, .. })
                    if !from.is_empty() && from.iter().all(|o| hand.contains(o)) =>
                {
                    Some((from.clone(), *max as usize))
                }
                Some(ChoiceKind::DiscardToHandSize { count }) => {
                    Some((hand.clone(), *count as usize))
                }
                _ => None,
            };
        let offered = |id: &ObjectId| {
            plays.contains_key(id) || choosing.as_ref().is_some_and(|(from, _)| from.contains(id))
        };

        let area = ui.available_rect_before_wrap();
        let (area, response) = ui.allocate_exact_size(area.size(), egui::Sense::click());
        let lefts = board::fan(hand.len(), HAND_CARD.x, area.width(), HAND_GAP);
        let top = area.center().y - HAND_CARD.y / 2.0;
        let slot = |i: usize| {
            egui::Rect::from_min_size(egui::pos2(area.left() + lefts[i], top), HAND_CARD)
        };

        let hovered = response
            .hover_pos()
            .filter(|p| (top..top + HAND_CARD.y).contains(&p.y))
            .and_then(|p| board::card_under(&lefts, HAND_CARD.x, p.x - area.left()));

        let painter = ui.painter().clone();
        for (i, id) in hand.iter().enumerate() {
            if hovered != Some(i) {
                // A picked card rises, so the choice is visible at a glance.
                let lift = if self.picked.contains(id) {
                    Vec2::new(0.0, -7.0)
                } else {
                    Vec2::ZERO
                };
                self.paint_card(&painter, view, *id, slot(i).translate(lift), offered(id));
                if self.picked.contains(id) {
                    painter.rect_stroke(
                        slot(i).translate(lift),
                        5.0,
                        Stroke::new(3.0, TAPPED_EDGE),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }

        let Some(i) = hovered else { return };
        let id = hand[i];
        if let Some(text) = board::object(view, id)
            .and_then(|object| self.texts.object_text(object))
            .cloned()
        {
            response
                .clone()
                .on_hover_ui(|ui| self.card_details(ui, &text));
        }
        let playable = plays.get(&id).cloned();

        // Enlarged on a foreground layer so it may rise out of the panel over the board, and
        // kept on screen at the ends of a long hand.
        let big = HAND_CARD * HAND_ZOOM;
        let screen = ui.ctx().content_rect();
        let left = (slot(i).center().x - big.x / 2.0)
            .clamp(screen.left(), (screen.right() - big.x).max(screen.left()));
        let zoomed = egui::Rect::from_min_size(egui::pos2(left, area.bottom() - big.y), big);
        let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("hand-zoom"));
        self.paint_card(
            &ui.ctx().layer_painter(layer),
            view,
            id,
            zoomed,
            playable.is_some() || offered(&id),
        );

        if let Some((_, max)) = &choosing
            && offered(&id)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() && (self.picked.contains(&id) || self.picked.len() < *max) {
                toggle(&mut self.picked, id);
            }
            return;
        }

        if let Some(actions) = playable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                if actions.len() == 1 {
                    self.answer(Answer::Action(actions[0].clone()));
                } else {
                    self.play_choice = Some(id);
                }
            }
        }
    }

    /// A card painted into an exact rectangle: its image if it has one, else a compact panel.
    /// For the hand, where cards overlap and so cannot be laid out as widgets.
    fn paint_card(
        &mut self,
        painter: &egui::Painter,
        view: &PlayerView,
        id: ObjectId,
        rect: egui::Rect,
        playable: bool,
    ) {
        let text = board::object(view, id)
            .and_then(|o| self.texts.object_text(o))
            .cloned();
        let texture = text
            .as_ref()
            .and_then(|t| self.art.get(painter.ctx(), &t.artwork()))
            .map(|t| t.id());
        let radius = 5.0;

        match (texture, text) {
            (Some(texture), _) => {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.add(
                    egui::epaint::RectShape::filled(rect, radius, Color32::WHITE)
                        .with_texture(texture, uv),
                );
            }
            (None, text) => {
                painter.rect(
                    rect,
                    radius,
                    CARD_FILL,
                    Stroke::new(1.0, CARD_EDGE),
                    egui::StrokeKind::Inside,
                );
                let inner = rect.shrink(6.0);
                let (name, detail) = match text {
                    Some(t) => (
                        format!("{}  {}", t.name, t.cost).trim_end().to_string(),
                        [
                            Some(t.type_line.clone()),
                            t.rules.clone(),
                            t.power_toughness.clone(),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                    ),
                    None => ("(hidden)".to_string(), String::new()),
                };
                // Scale text with the card, so the enlarged copy is actually easier to read.
                let scale = rect.width() / HAND_CARD.x;
                let clipped = painter.with_clip_rect(rect);
                let title = self.paint_mana_text(
                    &clipped,
                    inner.left_top(),
                    &name,
                    egui::FontId::proportional(12.0 * scale),
                    Color32::WHITE,
                    inner.width(),
                );
                self.paint_mana_text(
                    &clipped,
                    inner.left_top() + Vec2::new(0.0, title.y + 4.0),
                    &detail,
                    egui::FontId::proportional(10.0 * scale),
                    Color32::GRAY,
                    inner.width(),
                );
            }
        }

        if playable {
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(2.5, ACCENT),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// One card: its image if there is one, else an original panel.
    ///
    /// `actionable` outlines it, as the hand does: the engine is offering something to do with it.
    /// Returns a click response keyed by the object, so a menu opened on it survives the board
    /// being laid out again.
    fn draw_card(
        &mut self,
        ui: &mut Ui,
        view: &PlayerView,
        id: ObjectId,
        actionable: bool,
    ) -> egui::Response {
        let click_id = egui::Id::new(("permanent", id));
        let obj = board::object(view, id);
        let text = obj.and_then(|o| self.texts.object_text(o)).cloned();
        let tapped = obj.is_some_and(|o| o.tapped);
        let mut notes = obj.map(state_notes).unwrap_or_default();
        if let Some(target) = obj.and_then(|object| object.attached_to) {
            notes.push(format!("Attached to {}", self.name_of(view, target)));
        }
        if let Some(object) = obj.filter(|object| object.zone.zone == mtg_core::Zone::Stack)
            && !object.targets.is_empty()
        {
            let targets: Vec<_> = object
                .targets
                .iter()
                .map(|target| self.target_name(view, *target))
                .collect();
            notes.push(format!("Targets: {}", targets.join(", ")));
        }

        let texture = text
            .as_ref()
            .and_then(|t| self.art.get(ui.ctx(), &t.artwork()))
            .cloned();
        if let (Some(texture), Some(text)) = (texture, text.as_ref()) {
            return ui
                .vertical(|ui| {
                    // Tapped turns the card, as on a table: the slot is laid out sideways and the
                    // image rotated into it about its centre.
                    let size = ART_SIZE * self.board_scale;
                    let slot = if tapped {
                        Vec2::new(size.y, size.x)
                    } else {
                        size
                    };
                    let (rect, _) = ui.allocate_exact_size(slot, egui::Sense::hover());
                    let response = ui.interact(rect, click_id, egui::Sense::click());
                    let mut image = egui::Image::new(&texture).corner_radius(5.0);
                    if tapped {
                        image = image.rotate(std::f32::consts::FRAC_PI_2, Vec2::splat(0.5));
                    }
                    image.paint_at(ui, egui::Rect::from_center_size(rect.center(), size));
                    // In combat: attackers in red, blockers tagged with what they block — the
                    // board says who is fighting whom without reading the log.
                    if let Some(o) = obj {
                        if o.attacking {
                            ui.painter().rect_stroke(
                                rect,
                                5.0,
                                Stroke::new(3.0, ATTACK),
                                egui::StrokeKind::Outside,
                            );
                            let label = o
                                .attacking_target
                                .map(|t| format!("→ {}", self.target_name(view, t)))
                                .unwrap_or_else(|| "ATTACKING".into());
                            tag(ui.painter(), rect, &label, ATTACK);
                        } else if let Some(attacker) = o.blocking {
                            ui.painter().rect_stroke(
                                rect,
                                5.0,
                                Stroke::new(3.0, TAPPED_EDGE),
                                egui::StrokeKind::Outside,
                            );
                            tag(
                                ui.painter(),
                                rect,
                                &format!("blocking {}", self.name_of(view, attacker)),
                                TAPPED_EDGE,
                            );
                        }
                    }
                    if actionable {
                        // Around the slot rather than the image, so it follows a tapped card's turn.
                        ui.painter().rect_stroke(
                            rect,
                            5.0,
                            Stroke::new(2.5, ACCENT),
                            egui::StrokeKind::Outside,
                        );
                    }
                    let response = response.on_hover_ui(|ui| self.card_details(ui, text));

                    // Game state the printed image cannot show.
                    let damage_or_counters: Vec<_> = notes
                        .iter()
                        .filter(|n| n.as_str() != "tapped")
                        .cloned()
                        .collect();
                    if !damage_or_counters.is_empty() {
                        ui.label(
                            RichText::new(damage_or_counters.join(" · "))
                                .color(TAPPED_EDGE)
                                .small(),
                        );
                    }
                    response
                })
                .inner;
        }

        let edge = if actionable {
            ACCENT
        } else if tapped {
            TAPPED_EDGE
        } else {
            CARD_EDGE
        };
        let frame = egui::Frame::new()
            .fill(CARD_FILL)
            .stroke(Stroke::new(if actionable { 2.5 } else { 1.0 }, edge))
            .corner_radius(4.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.set_width(150.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let name = text
                            .as_ref()
                            .map(|t| t.name.clone())
                            .unwrap_or_else(|| "(hidden)".into());
                        ui.label(RichText::new(name).strong().small());
                        if let Some(t) = &text
                            && !t.cost.is_empty()
                        {
                            let weak = ui.visuals().weak_text_color();
                            self.mana_label(ui, &t.cost, SMALL_TEXT, weak);
                        }
                    });

                    if let Some(t) = &text {
                        ui.label(RichText::new(&t.type_line).weak().small());
                        if let Some(pt) = &t.power_toughness {
                            ui.label(RichText::new(pt).small());
                        }
                        if let Some(rules) = &t.rules {
                            // Truncated on the card, readable in full on hover: a board full of
                            // paragraphs is unreadable, but the text still has to be reachable.
                            let weak = ui.visuals().weak_text_color();
                            self.mana_label(ui, &mana_text::truncate(rules, 70), SMALL_TEXT, weak)
                                .on_hover_ui(|ui| self.card_details(ui, t));
                        }
                    }

                    if !notes.is_empty() {
                        ui.label(RichText::new(notes.join(" · ")).color(TAPPED_EDGE).small());
                    }
                });
            });
        let response = ui.interact(frame.response.rect, click_id, egui::Sense::click());
        if let Some(text) = text.as_ref() {
            response.on_hover_ui(|ui| self.card_details(ui, text))
        } else {
            response
        }
    }

    fn paint_mana_text(
        &mut self,
        painter: &egui::Painter,
        origin: egui::Pos2,
        text: &str,
        font: egui::FontId,
        color: Color32,
        width: f32,
    ) -> Vec2 {
        widgets::paint_mana_text(&mut self.art, painter, origin, text, font, color, width)
    }

    fn mana_label(&mut self, ui: &mut Ui, text: &str, size: f32, color: Color32) -> egui::Response {
        widgets::mana_label(ui, &mut self.art, text, size, color)
    }

    fn mana_button(&mut self, ui: &mut Ui, text: &str, color: Option<Color32>) -> egui::Response {
        widgets::mana_button(ui, &mut self.art, text, color)
    }

    /// Everything the text panel shows, symbols included, for hovering over a card.
    fn card_details(&mut self, ui: &mut Ui, text: &crate::cards_text::CardText) {
        widgets::offer_printing_enlargement(ui, &text.name, text.printing.as_deref());
        // The card itself, large, beside its text: the image is what a player recognises, the
        // text is what the engine actually plays.
        if let Some(texture) = self.art.get(ui.ctx(), &text.artwork()).cloned() {
            ui.horizontal_top(|ui| {
                ui.add(
                    egui::Image::new(&texture)
                        .fit_to_exact_size(Vec2::new(260.0, 260.0 * 680.0 / 488.0)),
                );
                ui.vertical(|ui| self.card_text(ui, text));
            });
            return;
        }
        self.card_text(ui, text);
    }

    fn card_text(&mut self, ui: &mut Ui, text: &crate::cards_text::CardText) {
        ui.set_max_width(300.0);
        let color = ui.visuals().text_color();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&text.name).strong());
            if !text.cost.is_empty() {
                self.mana_label(ui, &text.cost, 14.0, color);
            }
        });
        ui.label(RichText::new(&text.type_line).weak());
        if let Some(rules) = &text.rules {
            ui.add_space(4.0);
            self.mana_label(ui, rules, 14.0, color);
        }
        if let Some(pt) = &text.power_toughness {
            ui.add_space(4.0);
            ui.label(RichText::new(pt).strong());
        }
    }

    fn object_hover(
        &mut self,
        response: egui::Response,
        view: &PlayerView,
        id: ObjectId,
    ) -> egui::Response {
        if let Some(text) = board::object(view, id)
            .and_then(|object| object.card.map(|card| (card, object.face)))
            .and_then(|(card, face)| self.texts.get_face(card, face))
            .cloned()
        {
            response.on_hover_ui(|ui| self.card_details(ui, &text))
        } else {
            response
        }
    }

    fn action_hover(
        &mut self,
        response: egui::Response,
        view: &PlayerView,
        action: &Action,
    ) -> egui::Response {
        let (id, face) = match *action {
            Action::CastFace { object, face } | Action::PlayLandFace { object, face } => {
                (object, face)
            }
            Action::Cast { object }
            | Action::PlayLand { object }
            | Action::CastAlternative { object, .. }
            | Action::CastFaceDown { object } => (object, 0),
            Action::ActivateAbility { source, .. }
            | Action::ActivateManaAbility { source, .. }
            | Action::SpecialAction { source, .. } => {
                return self.object_hover(response, view, source);
            }
            Action::Pass | Action::Concede => return response,
        };
        if let Some(text) = board::object(view, id)
            .and_then(|object| object.card)
            .and_then(|card| self.texts.get_face(card, face))
            .cloned()
        {
            response.on_hover_ui(|ui| self.card_details(ui, &text))
        } else {
            response
        }
    }

    /// The outstanding question, as buttons.
    fn draw_question(&mut self, ui: &mut Ui) {
        let Some(question) = self.current.clone() else {
            if self.finished {
                ui.label(RichText::new("The game has ended.").strong());
            } else {
                ui.label(RichText::new("waiting…").weak());
            }
            return;
        };

        let top_of_stack = question
            .view
            .stack
            .last()
            .map(|id| self.name_of(&question.view, *id));
        let heading = match &question.choice.kind {
            ChoiceKind::Priority { .. } => {
                format::priority_heading(&question.view, top_of_stack.as_deref())
            }
            _ => format::choice_heading(&question.choice),
        };
        ui.label(RichText::new("YOUR DECISION").size(10.0).color(ACCENT));
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(heading).strong().size(18.0));
            if question.choice.undo || self.has_selection() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Undo").on_hover_text("Ctrl+Z").clicked() {
                        self.undo();
                    }
                });
            }
        });

        // When the engine could not prove an ordering irrelevant, it says which shared state made
        // the difference. Showing that is what makes the prompt worth reading.
        if let Some(why) = format::conflict_explanation(&question.choice) {
            ui.label(RichText::new(why).color(ACCENT).small());
        }

        let view = &question.view;
        if self.rejected_answer {
            ui.label(RichText::new("That answer was not legal. Choose again.").color(TAPPED_EDGE));
        }

        match &question.choice.kind {
            ChoiceKind::Priority { legal } => {
                if let Some(id) = self.play_choice {
                    ui.label("Choose how to play this card");
                    let options = board::hand_play_options(&legal.actions, &[id])
                        .remove(&id)
                        .unwrap_or_default();
                    for action in &options {
                        let response = ui.button(self.action_label(view, action));
                        let response = self.action_hover(response, view, action);
                        if response.clicked() {
                            self.answer(Answer::Action(action.clone()));
                            return;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.play_choice = None;
                    }
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    // Meaningful actions first, then Pass, then Concede — so the common case is
                    // where the eye already is.
                    let actions: Vec<&Action> = legal.meaningful().collect();
                    for action in board::distinct_actions_with_zones(
                        &actions,
                        |id| board::object(view, id).and_then(|o| o.card),
                        |id| board::object(view, id).map(|o| o.zone),
                    ) {
                        let label = self.action_label(view, action);
                        let response = ui.button(label);
                        if self.action_hover(response, view, action).clicked() {
                            self.answer(Answer::Action(action.clone()));
                            return;
                        }
                    }
                    let pass = format::pass_label(view, top_of_stack.as_deref());
                    if ui
                        .add(crate::theme::primary_button(
                            RichText::new(pass).strong().color(crate::theme::INK),
                        ))
                        .on_hover_text("Space")
                        .clicked()
                    {
                        self.answer(Answer::Pass);
                        return;
                    }
                    ui.add_space(24.0);
                    // Conceding takes two clicks: one misplaced click should not end a game.
                    if self.confirm_concede {
                        if ui
                            .button(RichText::new("Really concede?").color(TAPPED_EDGE))
                            .clicked()
                        {
                            self.confirm_concede = false;
                            self.answer(Answer::Action(Action::Concede));
                            return;
                        }
                        if ui.button("Cancel concede").clicked() {
                            self.confirm_concede = false;
                        }
                    } else if ui.button(RichText::new("Concede").weak()).clicked() {
                        self.confirm_concede = true;
                    }
                });
            }

            ChoiceKind::Confirm => {
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        self.answer(Answer::Bool(true));
                        return;
                    }
                    if ui.button("No").clicked() {
                        self.answer(Answer::Bool(false));
                    }
                });
            }

            ChoiceKind::ChooseModes {
                available,
                count,
                min,
            } => {
                let count = *count as usize;
                let min = min.map_or(count, usize::from);
                if count == 1 && min == 1 {
                    // One mode: a click is the whole answer.
                    for (i, label) in available.iter().enumerate() {
                        if self.mana_button(ui, label, None).clicked() {
                            self.answer(Answer::Modes(vec![i as u8]));
                            return;
                        }
                    }
                } else {
                    let how_many = if min == count {
                        format!("choose {count}")
                    } else {
                        format!("choose {min} to {count}")
                    };
                    ui.label(RichText::new(how_many).small());
                    for (i, label) in available.iter().enumerate() {
                        let chosen = self.picked_modes.contains(&(i as u8));
                        let color = chosen.then_some(ACCENT);
                        if self.mana_button(ui, label, color).clicked() {
                            toggle(&mut self.picked_modes, i as u8);
                        }
                    }
                    // Within the allowed range: the engine would top up a short answer, and a
                    // player should not have a mode chosen for them without seeing it.
                    let ready = (min..=count).contains(&self.picked_modes.len());
                    if ui
                        .add_enabled(ready, egui::Button::new("Confirm"))
                        .clicked()
                    {
                        let mut modes = std::mem::take(&mut self.picked_modes);
                        modes.sort_unstable();
                        self.answer(Answer::Modes(modes));
                    }
                }
            }

            ChoiceKind::ChooseX { min, max } => {
                let (min, max) = (*min, *max);
                if max.saturating_sub(min) <= 12 {
                    ui.horizontal_wrapped(|ui| {
                        for n in min..=max {
                            if ui.button(n.to_string()).clicked() {
                                self.answer(Answer::Number(n));
                                return;
                            }
                        }
                    });
                } else {
                    self.picked_x = self.picked_x.clamp(min, max);
                    ui.horizontal(|ui| {
                        ui.label(format!("Choose X ({min}–{max})"));
                        ui.add(egui::DragValue::new(&mut self.picked_x).range(min..=max));
                        if ui.button("Confirm").clicked() {
                            self.answer(Answer::Number(self.picked_x));
                        }
                    });
                }
            }

            ChoiceKind::ChooseTargets { slots } => {
                let slots = slots.clone();
                for (i, options) in slots.iter().enumerate() {
                    ui.label(RichText::new(format!("Target {}", i + 1)).small());
                    if options.is_empty() {
                        ui.label(RichText::new("No legal targets").weak());
                    }
                    ui.horizontal_wrapped(|ui| {
                        for target in options {
                            let label = match target {
                                mtg_core::Target::Object(o) => self.name_of(view, *o),
                                mtg_core::Target::Player(p) => format::player_name(view, *p),
                            };
                            let chosen = self.picked_targets.get(&i) == Some(target);
                            let clicked = ui
                                .vertical(|ui| {
                                    let card_clicked =
                                        if let mtg_core::Target::Object(object) = target {
                                            self.draw_card(ui, view, *object, true).clicked()
                                        } else {
                                            false
                                        };
                                    let response = ui.selectable_label(chosen, label);
                                    let response = if let mtg_core::Target::Object(object) = target
                                    {
                                        self.object_hover(response, view, *object)
                                    } else {
                                        response
                                    };
                                    response.clicked() || card_clicked
                                })
                                .inner;
                            if clicked {
                                if slots.len() == 1 {
                                    self.answer(Answer::Targets(vec![vec![*target]]));
                                    return;
                                }
                                self.target_pick_order.retain(|slot| *slot != i);
                                if chosen {
                                    self.picked_targets.remove(&i);
                                } else {
                                    self.picked_targets.insert(i, *target);
                                    self.target_pick_order.push(i);
                                }
                            }
                        }
                    });
                }
                if slots.len() != 1 || slots[0].is_empty() {
                    let answer = selected_target_answer(&slots, &self.picked_targets);
                    if ui
                        .add_enabled(answer.is_some(), egui::Button::new("Confirm targets"))
                        .clicked()
                        && let Some(answer) = answer
                    {
                        self.answer(answer);
                    }
                }
            }

            ChoiceKind::DeclareAttackers {
                eligible,
                defenders,
            } => {
                let eligible = eligible.clone();
                let defenders = defenders.clone();
                ui.horizontal_wrapped(|ui| {
                    for id in &eligible {
                        let chosen = self.picked.contains(id);
                        let label = RichText::new(self.name_of(view, *id));
                        let label = if chosen {
                            label.color(ACCENT).strong()
                        } else {
                            label
                        };
                        let response = ui.button(label);
                        if self.object_hover(response, view, *id).clicked() {
                            toggle(&mut self.picked, *id);
                        }
                    }
                });
                ui.label(
                    RichText::new("Click creatures on the battlefield to choose attackers.")
                        .weak()
                        .small(),
                );
                if defenders.len() > 1 {
                    for attacker in self.picked.clone() {
                        let mut selected = self
                            .attack_destinations
                            .get(&attacker)
                            .copied()
                            .unwrap_or(defenders[0]);
                        ui.horizontal(|ui| {
                            ui.label(format!("{} attacks", self.name_of(view, attacker)));
                            egui::ComboBox::from_id_salt(("attack_destination", attacker))
                                .selected_text(self.target_name(view, selected))
                                .show_ui(ui, |ui| {
                                    for destination in &defenders {
                                        ui.selectable_value(
                                            &mut selected,
                                            *destination,
                                            self.target_name(view, *destination),
                                        );
                                    }
                                });
                        });
                        self.attack_destinations.insert(attacker, selected);
                    }
                }
                ui.horizontal(|ui| {
                    let n = self.picked.len();
                    if n > 0
                        && ui
                            .button(RichText::new(format!("Attack with {n}")).strong())
                            .clicked()
                    {
                        let picked = std::mem::take(&mut self.picked);
                        self.answer(self.attack_answer(&picked, defenders.first().copied()));
                        return;
                    }
                    if eligible.len() > 1 && ui.button("Attack with all").clicked() {
                        self.answer(self.attack_answer(&eligible, defenders.first().copied()));
                        return;
                    }
                    if ui
                        .button(if n == 0 {
                            "No attack"
                        } else {
                            "Cancel — no attack"
                        })
                        .clicked()
                    {
                        self.answer(Answer::Objects(Vec::new()));
                    }
                });
            }

            ChoiceKind::KeepOneLegend { name, candidates } => {
                ui.label(RichText::new(format!("keep which {name}?")).small());
                ui.horizontal_wrapped(|ui| {
                    for id in candidates {
                        let clicked = ui
                            .vertical(|ui| {
                                let card = self.draw_card(ui, view, *id, true);
                                let response = ui.button(self.name_of(view, *id));
                                self.object_hover(response, view, *id).clicked() || card.clicked()
                            })
                            .inner;
                        if clicked {
                            self.answer(Answer::Objects(vec![*id]));
                            return;
                        }
                    }
                });
            }

            kind @ (ChoiceKind::ChooseObjects { .. } | ChoiceKind::DiscardToHandSize { .. }) => {
                let (from, min, max) =
                    object_selection_options(kind, view).expect("object selection question");
                let range = if min == max {
                    min.to_string()
                } else {
                    format!("{min}–{max}")
                };
                ui.label(RichText::new(format!("choose {range}")).small());
                ui.horizontal_wrapped(|ui| {
                    for id in &from {
                        let chosen = self.picked.contains(id);
                        let label = RichText::new(self.name_of(view, *id));
                        let label = if chosen {
                            label.color(ACCENT).strong()
                        } else {
                            label
                        };
                        // A full selection disables the rest rather than silently replacing one.
                        let enabled = chosen || self.picked.len() < max;
                        ui.vertical(|ui| {
                            let card = self.draw_card(ui, view, *id, enabled);
                            if chosen {
                                ui.painter().rect_stroke(
                                    card.rect,
                                    4.0,
                                    Stroke::new(3.0, TAPPED_EDGE),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            let button = ui.add_enabled(enabled, egui::Button::new(label));
                            let button = self.object_hover(button, view, *id);
                            if enabled && (card.clicked() || button.clicked()) {
                                toggle(&mut self.picked, *id);
                            }
                        });
                    }
                });
                let ready = (min..=max).contains(&self.picked.len());
                if ui
                    .add_enabled(ready, egui::Button::new("Confirm"))
                    .clicked()
                {
                    let picked = std::mem::take(&mut self.picked);
                    self.answer(Answer::Objects(picked));
                }
            }

            ChoiceKind::DeclareBlockers { eligible } => {
                // One row per potential blocker, with a button per attacker it may block. The
                // engine validates the whole declaration (menace included) and re-asks if it is
                // illegal, so this only has to offer the pairs it was given.
                for (blocker, attackers) in eligible {
                    ui.horizontal_wrapped(|ui| {
                        let response =
                            ui.label(RichText::new(self.name_of(view, *blocker)).strong().small());
                        self.object_hover(response, view, *blocker);
                        ui.label(RichText::new("blocks").weak().small());
                        let current = self.blocks.iter().find(|(b, _)| b == blocker).map(|p| p.1);
                        for attacker in attackers {
                            let chosen = current == Some(*attacker);
                            let label = RichText::new(self.name_of(view, *attacker));
                            let label = if chosen {
                                label.color(ACCENT).strong()
                            } else {
                                label
                            };
                            let response = ui.button(label);
                            if self.object_hover(response, view, *attacker).clicked() {
                                self.toggle_block(*blocker, *attacker);
                            }
                        }
                    });
                }
                ui.label(
                    RichText::new(
                        "Click one of your creatures, then the attacker it should block.",
                    )
                    .weak()
                    .small(),
                );
                ui.horizontal(|ui| {
                    let n = self.blocks.len();
                    if n > 0
                        && ui
                            .button(RichText::new(format!("Block with {n}")).strong())
                            .clicked()
                    {
                        let blocks = std::mem::take(&mut self.blocks);
                        self.answer(Answer::Blocks(blocks));
                        return;
                    }
                    if ui.button("No blocks").clicked() {
                        self.answer(Answer::Blocks(Vec::new()));
                    }
                });
            }

            ChoiceKind::OrderBlockers { attacker, blockers } => {
                if self.blocker_order.len() != blockers.len() {
                    self.blocker_order = (0..blockers.len()).collect();
                }
                ui.label(format!(
                    "Order blockers for {}",
                    self.name_of(view, *attacker)
                ));
                ui.label(RichText::new("First receives damage first").weak().small());
                let mut swap = None;
                let n = self.blocker_order.len();
                for (pos, index) in self.blocker_order.clone().into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", pos + 1));
                        if ui
                            .add_enabled(pos > 0, egui::Button::new("⏶").small())
                            .clicked()
                        {
                            swap = Some((pos, pos - 1));
                        }
                        if ui
                            .add_enabled(pos + 1 < n, egui::Button::new("⏷").small())
                            .clicked()
                        {
                            swap = Some((pos, pos + 1));
                        }
                        let response = ui.label(self.name_of(view, blockers[index]));
                        self.object_hover(response, view, blockers[index]);
                    });
                }
                if let Some((a, b)) = swap {
                    self.blocker_order.swap(a, b);
                }
                if ui.button("Confirm order").clicked() {
                    let order = std::mem::take(&mut self.blocker_order);
                    self.answer(Answer::Order(order));
                }
            }

            ChoiceKind::KeepOrMulligan { mulligans_taken } => {
                let taken = *mulligans_taken;
                if taken > 0 {
                    ui.label(
                        RichText::new(format!(
                            "Keeping means putting {taken} card{} on the bottom next.",
                            if taken == 1 { "" } else { "s" }
                        ))
                        .weak(),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("Keep").strong().size(16.0))
                        .clicked()
                    {
                        self.answer(Answer::Bool(true));
                        return;
                    }
                    if taken < 7
                        && ui
                            .button(format!("Mulligan (to {})", 7 - (taken + 1).min(7)))
                            .clicked()
                    {
                        self.answer(Answer::Bool(false));
                    }
                });
            }

            ChoiceKind::OrderTriggers {
                triggers, labels, ..
            } => {
                // Only asked when the engine could not prove the order irrelevant; the
                // explanation above says what they both touch.
                if self.trigger_order.len() != triggers.len() {
                    self.trigger_order = (0..triggers.len()).collect();
                }
                ui.label(RichText::new("Top resolves first").weak().small());
                let mut swap = None;
                let n = self.trigger_order.len();
                for (pos, index) in self.trigger_order.clone().into_iter().enumerate() {
                    let label = labels
                        .get(index)
                        .map_or_else(|| format!("trigger {}", index + 1), |l| l.to_string());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", pos + 1)).weak());
                        if ui
                            .add_enabled(pos > 0, egui::Button::new("⏶").small())
                            .clicked()
                        {
                            swap = Some((pos, pos - 1));
                        }
                        if ui
                            .add_enabled(pos + 1 < n, egui::Button::new("⏷").small())
                            .clicked()
                        {
                            swap = Some((pos, pos + 1));
                        }
                        let color = ui.visuals().text_color();
                        self.mana_label(ui, &label, 14.0, color);
                    });
                }
                if let Some((a, b)) = swap {
                    self.trigger_order.swap(a, b);
                }
                if ui.button("Put them on the stack").clicked() {
                    // The engine takes placement order, and the last placed resolves first.
                    let mut order = std::mem::take(&mut self.trigger_order);
                    order.reverse();
                    self.answer(Answer::Order(order));
                }
            }

            ChoiceKind::AssignCombatDamage {
                attacker,
                among,
                total,
            } => {
                ui.label(format!(
                    "{} assigns {total} combat damage",
                    self.name_of(view, *attacker)
                ));
                let answer = mtg_policy::well_formed(&question.choice, view);
                if let Some(mut rows) = self.custom_damage.take() {
                    for (index, (object, amount)) in rows.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let response =
                                ui.label(format!("{}. {}", index + 1, self.name_of(view, *object)));
                            self.object_hover(response, view, *object);
                            ui.add(egui::DragValue::new(amount).range(0..=*total));
                        });
                    }
                    let assigned: u64 = rows.iter().map(|(_, amount)| u64::from(*amount)).sum();
                    ui.label(format!("Assigned {assigned} of {total}"));
                    if assigned > u64::from(*total) {
                        ui.label(
                            RichText::new(format!(
                                "Reduce assigned damage by {}",
                                assigned - u64::from(*total)
                            ))
                            .color(TAPPED_EDGE),
                        );
                    } else if assigned < u64::from(*total) {
                        ui.label(format!(
                            "Unassigned: {} (trample only)",
                            u64::from(*total) - assigned
                        ));
                    }
                    ui.label(RichText::new("Divide damage among blockers as you choose. Unassigned damage requires trample and lethal damage to every blocker.").weak().small());
                    if ui
                        .add_enabled(
                            assigned <= u64::from(*total),
                            egui::Button::new("Confirm damage"),
                        )
                        .clicked()
                    {
                        self.answer(Answer::DamageAssignment(rows));
                        return;
                    }
                    if !ui.button("Use default instead").clicked() {
                        self.custom_damage = Some(rows);
                    }
                    return;
                }
                if let Answer::DamageAssignment(rows) = &answer {
                    for (object, amount) in rows {
                        ui.label(format!("{}: {amount} damage", self.name_of(view, *object)));
                    }
                    let assigned: u64 = rows.iter().map(|(_, amount)| u64::from(*amount)).sum();
                    let excess = u64::from(*total).saturating_sub(assigned);
                    if excess > 0 && question.choice.default.is_some() {
                        let defender = board::object(view, *attacker)
                            .and_then(|object| object.attacking_target)
                            .map_or_else(
                                || "Defender".into(),
                                |target| self.target_name(view, target),
                            );
                        ui.label(format!("{defender}: {excess} trample damage"));
                    }
                }
                ui.label(
                    RichText::new("Using the engine's default damage assignment.")
                        .weak()
                        .small(),
                );
                if ui.button("Confirm default damage").clicked() {
                    self.answer(answer);
                    return;
                }
                if ui.button("Customize damage").clicked()
                    && let Answer::DamageAssignment(rows) = answer
                {
                    self.custom_damage = Some(
                        among
                            .iter()
                            .map(|object| {
                                (
                                    *object,
                                    rows.iter()
                                        .find(|(id, _)| id == object)
                                        .map_or(0, |(_, n)| *n),
                                )
                            })
                            .collect(),
                    );
                }
            }

            // Anything without bespoke controls yet gets the engine's own default, which is always
            // legal — shown as such rather than pretending to be a real choice.
            other => {
                ui.label(
                    RichText::new(format!(
                        "no controls for {} yet — taking the default",
                        describe_kind(other)
                    ))
                    .weak()
                    .small(),
                );
                if ui.button("Continue").clicked() {
                    let answer = mtg_policy::well_formed(&question.choice, &question.view);
                    self.answer(answer);
                }
            }
        }
    }
}

/// The turn's steps in a row, the current one lit — in the accent colour on your turn, a warm
/// one on the opponent's, so whose turn it is reads at a glance.
fn phase_bar(ui: &mut Ui, view: &PlayerView, texts: &CardTexts) {
    for target in &view.prevent_damage_to {
        let name = match target {
            mtg_core::Target::Player(player) => format::player_name(view, *player),
            mtg_core::Target::Object(object) => view
                .visible
                .get(object)
                .and_then(|o| texts.object_text(o))
                .map_or_else(|| "(hidden)".into(), |t| t.name.clone()),
        };
        ui.label(
            RichText::new(format!("All damage to {name} prevented this turn"))
                .color(ACCENT)
                .small(),
        );
    }
    if view.prevent_combat_damage {
        ui.label(
            RichText::new("All combat damage prevented this turn")
                .color(ACCENT)
                .small(),
        );
    }
    use mtg_core::Step as S;
    let current = match view.step {
        S::FirstStrikeCombatDamage => S::CombatDamage,
        S::Cleanup => S::End,
        other => other,
    };
    let lit = if view.active_player == view.viewer {
        ACCENT
    } else {
        TAPPED_EDGE
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (step, label) in format::PHASE_BAR {
            let on = step == current;
            let text = RichText::new(label).small();
            let text = if on {
                text.strong().color(Color32::BLACK)
            } else {
                text.weak()
            };
            egui::Frame::new()
                .fill(if on { lit } else { Color32::TRANSPARENT })
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| ui.label(text));
        }
    });
}

/// A short label across the bottom of a card, on a dark band so it reads over any art.
fn tag(painter: &egui::Painter, rect: egui::Rect, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let band = egui::Rect::from_center_size(
        rect.center_bottom() - Vec2::new(0.0, size.y / 2.0 + 4.0),
        size,
    );
    painter.rect_filled(band, 4.0, Color32::from_black_alpha(210));
    painter.rect_stroke(band, 4.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    painter.galley(band.min + Vec2::new(5.0, 2.0), galley, Color32::WHITE);
}

/// What a click on the board means for the outstanding question.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoardPick {
    /// Add to, or take out of, a selection: attackers, chosen objects.
    Toggle,
    /// Start a block with this creature.
    Blocker,
    /// Finish a block: the waiting blocker blocks this attacker.
    Attacker,
    /// Choose this as the target.
    Target,
}

/// Object-selection bounds and candidates, including cleanup's visible own hand.
fn object_selection_options(
    kind: &ChoiceKind,
    view: &PlayerView,
) -> Option<(Vec<ObjectId>, usize, usize)> {
    match kind {
        ChoiceKind::ChooseObjects { from, min, max } => {
            Some((from.clone(), *min as usize, *max as usize))
        }
        ChoiceKind::DiscardToHandSize { count } => {
            let hand = view
                .visible
                .values()
                .filter(|o| {
                    o.zone.zone == mtg_core::Zone::Hand && o.zone.player == Some(view.viewer)
                })
                .map(|o| o.id)
                .collect();
            Some((hand, *count as usize, *count as usize))
        }
        _ => None,
    }
}

/// Assemble complete target slots without accepting stale or illegal selections.
fn selected_target_answer(
    slots: &[Vec<mtg_core::Target>],
    picked: &std::collections::BTreeMap<usize, mtg_core::Target>,
) -> Option<Answer> {
    let targets: Option<Vec<_>> = slots
        .iter()
        .enumerate()
        .map(|(i, options)| {
            if options.is_empty() {
                Some(Vec::new())
            } else {
                picked
                    .get(&i)
                    .filter(|target| options.contains(target))
                    .map(|target| vec![*target])
            }
        })
        .collect();
    targets.map(Answer::Targets)
}

/// A pointer cursor over something clickable.
fn pointer_cursor(response: &egui::Response) {
    if response.hovered() {
        response.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// Tapped, damage and counters, as short phrases.
fn state_notes(obj: &mtg_engine::view::ObjectView) -> Vec<String> {
    let mut notes = Vec::new();
    if obj.tapped {
        notes.push("tapped".to_string());
    }
    if obj.damage > 0 {
        notes.push(format!("{} damage", obj.damage));
    }
    for (kind, n) in &obj.counters {
        if *n != 0 {
            notes.push(format::counter_total(*kind, *n));
        }
    }
    notes
}

/// Add `item` if absent, remove it if present.
fn toggle<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if let Some(i) = list.iter().position(|x| *x == item) {
        list.remove(i);
    } else {
        list.push(item);
    }
}

fn describe_kind(kind: &ChoiceKind) -> &'static str {
    match kind {
        ChoiceKind::OrderTriggers { .. } => "trigger ordering",
        ChoiceKind::OrderReplacements { .. } => "replacement ordering",
        ChoiceKind::PayMana { .. } => "mana payment",
        ChoiceKind::OrderBlockers { .. } => "blocker ordering",
        ChoiceKind::AssignCombatDamage { .. } => "damage assignment",
        ChoiceKind::DiscardToHandSize { .. } => "discarding to hand size",
        ChoiceKind::KeepOrMulligan { .. } => "keeping or mulliganing",
        _ => "this question",
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    use mtg_core::{PlayerId, Target};
    use std::collections::BTreeMap;

    fn question_app(kind: ChoiceKind) -> (GuiApp, Receiver<Answer>, egui::Context) {
        let ctx = egui::Context::default();
        let (_questions_tx, questions) = std::sync::mpsc::channel();
        let (answers, received) = std::sync::mpsc::channel();
        let art = CardArt::start(
            ctx.clone(),
            crate::art::ArtConfig {
                names: Default::default(),
                db: None,
                cache_dir: std::env::temp_dir().join("mtgo-question-ui-test"),
            },
        );
        let mut app = GuiApp::new(
            questions,
            answers,
            CardTexts::default(),
            art,
            Default::default(),
        );
        let viewer = PlayerId(0);
        app.current = Some(Question {
            choice: mtg_engine::Choice {
                id: 1,
                who: viewer,
                kind,
                because: "test".into(),
                default: None,
                undo: false,
            },
            view: PlayerView {
                prevent_combat_damage: false,
                prevent_damage_to: Vec::new(),
                viewer,
                turn: 1,
                active_player: viewer,
                step: mtg_core::Step::PrecombatMain,
                priority: Some(viewer),
                players: Default::default(),
                visible: Default::default(),
                stack: Vec::new(),
            },
        });
        (app, received, ctx)
    }

    fn click_label(ctx: &egui::Context, label: &str, mut draw: impl FnMut(&mut Ui)) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| draw(ui));
        output.textures_delta.clear();
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("question control {label:?} was not rendered"));
        let input = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| draw(ui));
        output.textures_delta.clear();
    }

    fn click_question_label(app: &mut GuiApp, ctx: &egui::Context, label: &str) {
        click_label(ctx, label, |ui| app.draw_question(ui));
    }

    #[test]
    fn object_choices_render_cards_and_enforce_the_pick_limit() {
        use mtg_headless::cards::{DUMMY, DemoCards, SENTRY};
        let cards = DemoCards::default();
        let mut state = mtg_engine::state::GameState::new(&[PlayerId(0), PlayerId(1)], 20);
        let first = state.place(
            DUMMY,
            PlayerId(0),
            mtg_core::ZoneRef::of(mtg_core::Zone::Hand, PlayerId(0)),
        );
        let second = state.place(
            SENTRY,
            PlayerId(0),
            mtg_core::ZoneRef::of(mtg_core::Zone::Hand, PlayerId(0)),
        );
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseObjects {
            from: vec![first, second],
            min: 1,
            max: 1,
        });
        app.texts = CardTexts::snapshot(&cards, [DUMMY, SENTRY]);
        app.current.as_mut().unwrap().view = mtg_engine::view::project(&state, PlayerId(0));
        let first_name = app.texts.name(Some(DUMMY));
        let second_name = app.texts.name(Some(SENTRY));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw_question(ui));
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text().contains("Creature"))
        ));
        click_question_label(&mut app, &ctx, &first_name);
        click_question_label(&mut app, &ctx, &second_name);
        assert_eq!(app.picked, vec![first]);
        click_question_label(&mut app, &ctx, "Confirm");
        assert!(matches!(answers.try_recv().unwrap(), Answer::Objects(ids) if ids == vec![first]));
    }

    #[test]
    fn bottom_cast_action_hover_offers_enlargement_without_casting() {
        use mtg_headless::cards::{DUMMY, DemoCards};
        let cards = DemoCards::default();
        let mut state = mtg_engine::state::GameState::new(&[PlayerId(0), PlayerId(1)], 20);
        let object = state.place(
            DUMMY,
            PlayerId(0),
            mtg_core::ZoneRef::of(mtg_core::Zone::Hand, PlayerId(0)),
        );
        let action = Action::Cast { object };
        let (mut app, answers, ctx) = question_app(ChoiceKind::Priority {
            legal: mtg_engine::actions::LegalActions {
                actions: vec![action.clone()],
                ..Default::default()
            },
        });
        app.texts = CardTexts::snapshot(&cards, [DUMMY]);
        app.current.as_mut().unwrap().view = mtg_engine::view::project(&state, PlayerId(0));
        let label = app.action_label(&app.current.as_ref().unwrap().view, &action);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw_question(ui));
        output.textures_delta.clear();
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                } else {
                    None
                }
            })
            .expect("cast button should render");
        let mut offered = false;
        for time in [1.0, 2.0, 3.0] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events: if time == 1.0 {
                        vec![egui::Event::PointerMoved(pos)]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
                |ui| app.draw_question(ui),
            );
            output.textures_delta.clear();
            offered |= output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                egui::epaint::Shape::Text(text) if text.galley.text() == "Enlarge card")
            });
        }
        assert!(
            offered,
            "hovering the bottom cast button should offer enlargement"
        );
        assert!(answers.try_recv().is_err());
    }

    #[test]
    fn enlarge_action_opens_a_persistent_popup_without_answering() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::Confirm);
        click_label(&ctx, "Enlarge card", |ui| {
            widgets::offer_card_enlargement(ui, "Faithless Looting")
        });
        assert_eq!(
            ctx.data(|data| data.get_temp::<String>(egui::Id::new("enlarged-card")))
                .as_deref(),
            Some("Faithless Looting")
        );
        let mut first_frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            widgets::show_enlarged_card(ui, &mut app.art)
        });
        first_frame.textures_delta.clear();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            widgets::show_enlarged_card(ui, &mut app.art)
        });
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::epaint::Shape::Text(text)
            if text.galley.text() == "Faithless Looting")
        ));
        assert!(answers.try_recv().is_err());
        assert!(app.current.is_some());
    }

    #[test]
    fn repeated_questions_explain_rejected_answers_but_not_undo() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::Confirm);
        let (questions, receiver) = std::sync::mpsc::channel();
        app.questions = receiver;
        let mut question = app.current.clone().unwrap();
        app.answer(Answer::Bool(true));
        assert!(matches!(answers.try_recv().unwrap(), Answer::Bool(true)));
        questions.send(question.clone()).unwrap();
        app.poll();
        assert!(app.rejected_answer);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw_question(ui));
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::epaint::Shape::Text(text)
                if text.galley.text() == "That answer was not legal. Choose again.")
        }));
        app.answer(Answer::Bool(false));
        question.choice.id += 1;
        questions.send(question.clone()).unwrap();
        app.poll();
        assert!(!app.rejected_answer);
        app.answer(Answer::Undo);
        questions.send(question).unwrap();
        app.poll();
        assert!(!app.rejected_answer);
    }

    #[test]
    fn custom_damage_controls_seed_cancel_undo_and_confirm_allocations() {
        let rows = vec![(ObjectId(2), 2), (ObjectId(3), 4)];
        let (mut app, answers, ctx) = question_app(ChoiceKind::AssignCombatDamage {
            attacker: ObjectId(1),
            among: vec![ObjectId(2), ObjectId(3)],
            total: 6,
        });
        app.current.as_mut().unwrap().choice.default = Some(Answer::DamageAssignment(rows.clone()));
        click_question_label(&mut app, &ctx, "Customize damage");
        assert_eq!(app.custom_damage, Some(rows));
        assert!(app.has_selection());
        click_question_label(&mut app, &ctx, "Use default instead");
        assert!(app.custom_damage.is_none());
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Customize damage");
        click_question_label(&mut app, &ctx, "Undo");
        assert!(!app.has_selection());
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Customize damage");
        app.custom_damage = Some(vec![(ObjectId(2), 6), (ObjectId(3), 6)]);
        click_question_label(&mut app, &ctx, "Confirm damage");
        assert!(answers.try_recv().is_err());
        app.custom_damage = Some(vec![(ObjectId(2), 6), (ObjectId(3), 0)]);
        click_question_label(&mut app, &ctx, "Confirm damage");
        let Answer::DamageAssignment(actual) = answers.try_recv().unwrap() else {
            panic!("damage expected");
        };
        assert_eq!(actual, vec![(ObjectId(2), 6), (ObjectId(3), 0)]);
        assert!(app.custom_damage.is_none());
        assert!(app.current.is_none());
    }

    #[test]
    fn default_damage_preview_includes_excess_trample_damage() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::AssignCombatDamage {
            attacker: ObjectId(1),
            among: vec![ObjectId(2), ObjectId(3)],
            total: 4,
        });
        let question = app.current.as_mut().unwrap();
        question.choice.default = Some(Answer::DamageAssignment(vec![
            (ObjectId(2), 1),
            (ObjectId(3), 1),
        ]));
        question.view.visible.insert(
            ObjectId(1),
            mtg_engine::view::ObjectView {
                id: ObjectId(1),
                zone: mtg_core::ZoneRef::shared(mtg_core::Zone::Battlefield),
                controller: PlayerId(0),
                card: None,
                face: 0,
                adventure_player: None,
                tapped: true,
                damage: 0,
                counters: Default::default(),
                attached_to: None,
                targets: Vec::new(),
                is_ability: false,
                ability: None,
                attacking: true,
                attacking_target: Some(Target::Player(PlayerId(1))),
                blocking: None,
            },
        );
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw_question(ui));
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::epaint::Shape::Text(text)
                if text.galley.text() == "Opponent: 2 trample damage")
        }));
        assert!(answers.try_recv().is_err());
    }

    #[test]
    fn default_combat_damage_is_confirmed_without_changing_the_assignment() {
        let rows = vec![(ObjectId(2), 3), (ObjectId(3), 2)];
        let (mut app, answers, ctx) = question_app(ChoiceKind::AssignCombatDamage {
            attacker: ObjectId(1),
            among: vec![ObjectId(2), ObjectId(3)],
            total: 5,
        });
        app.current.as_mut().unwrap().choice.default = Some(Answer::DamageAssignment(rows.clone()));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw_question(ui));
        output.textures_delta.clear();
        for amount in [3, 2] {
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::epaint::Shape::Text(text)
                    if text.galley.text() == format!("(hidden): {amount} damage"))
            }));
        }
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Confirm default damage");
        let Answer::DamageAssignment(actual) = answers.try_recv().unwrap() else {
            panic!("damage assignment expected");
        };
        assert_eq!(actual, rows);
        assert!(app.current.is_none());
    }

    #[test]
    fn target_controls_wait_for_confirmation_and_submit_every_slot() {
        let mine = Target::Player(PlayerId(0));
        let theirs = Target::Player(PlayerId(1));
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseTargets {
            slots: vec![vec![mine], vec![theirs]],
        });
        click_question_label(&mut app, &ctx, "You");
        assert!(answers.try_recv().is_err());
        assert!(app.current.is_some());
        click_question_label(&mut app, &ctx, "Confirm targets");
        assert!(
            answers.try_recv().is_err(),
            "incomplete target selection must remain disabled"
        );
        click_question_label(&mut app, &ctx, "Opponent");
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Confirm targets");
        let Answer::Targets(targets) = answers.try_recv().unwrap() else {
            panic!("target answer expected")
        };
        assert_eq!(targets, vec![vec![mine], vec![theirs]]);
        assert!(app.current.is_none());
    }

    #[test]
    fn optional_single_mode_controls_allow_declining_all_modes() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseModes {
            available: vec!["Optional mode".into()],
            count: 1,
            min: Some(0),
        });
        click_question_label(&mut app, &ctx, "Confirm");
        let Answer::Modes(modes) = answers.try_recv().unwrap() else {
            panic!("mode answer expected")
        };
        assert!(modes.is_empty());

        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseModes {
            available: vec!["Optional mode".into()],
            count: 1,
            min: Some(0),
        });
        click_question_label(&mut app, &ctx, "Optional mode");
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Confirm");
        let Answer::Modes(modes) = answers.try_recv().unwrap() else {
            panic!("mode answer expected")
        };
        assert_eq!(modes, vec![0]);
    }

    #[test]
    fn single_required_mode_still_submits_immediately() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseModes {
            available: vec!["Required mode".into()],
            count: 1,
            min: None,
        });
        click_question_label(&mut app, &ctx, "Required mode");
        let Answer::Modes(modes) = answers.try_recv().unwrap() else {
            panic!("mode answer expected")
        };
        assert_eq!(modes, vec![0]);
    }

    #[test]
    fn blocker_order_controls_submit_the_displayed_order() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::OrderBlockers {
            attacker: ObjectId(3),
            blockers: vec![ObjectId(1), ObjectId(2)],
        });
        click_question_label(&mut app, &ctx, "⏷");
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Confirm order");
        let Answer::Order(order) = answers.try_recv().unwrap() else {
            panic!("order answer expected")
        };
        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn panel_block_assignment_clears_only_its_pending_blocker() {
        let first = ObjectId(1);
        let second = ObjectId(2);
        let attacker = ObjectId(3);
        let other_attacker = ObjectId(4);
        let (mut app, answers, _) = question_app(ChoiceKind::DeclareBlockers {
            eligible: vec![
                (first, vec![attacker, other_attacker]),
                (second, vec![attacker]),
            ],
        });
        app.click_pick(first, BoardPick::Blocker);
        app.toggle_block(second, attacker);
        assert_eq!(app.pending_blocker, Some(first));
        app.toggle_block(first, attacker);
        assert!(app.pending_blocker.is_none());
        assert_eq!(app.board_pick(other_attacker), None);
        app.click_pick(other_attacker, BoardPick::Attacker);
        assert_eq!(app.blocks, vec![(second, attacker), (first, attacker)]);
        app.toggle_block(first, other_attacker);
        assert_eq!(
            app.blocks,
            vec![(second, attacker), (first, other_attacker)]
        );
        app.toggle_block(first, other_attacker);
        assert_eq!(app.blocks, vec![(second, attacker)]);
        assert!(answers.try_recv().is_err());
    }

    #[test]
    fn undo_cancels_pending_blockers_before_completed_blocks_or_engine_actions() {
        let first = ObjectId(1);
        let second = ObjectId(2);
        let attacker = ObjectId(3);
        let (mut app, answers, ctx) = question_app(ChoiceKind::DeclareBlockers {
            eligible: vec![(first, vec![attacker]), (second, vec![attacker])],
        });
        app.current.as_mut().unwrap().choice.undo = true;
        app.click_pick(first, BoardPick::Blocker);
        app.click_pick(attacker, BoardPick::Attacker);
        app.click_pick(second, BoardPick::Blocker);
        assert!(app.has_selection());
        click_question_label(&mut app, &ctx, "Undo");
        assert_eq!(app.pending_blocker, None);
        assert_eq!(app.blocks, vec![(first, attacker)]);
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Undo");
        assert!(app.blocks.is_empty());
        assert!(!app.has_selection());
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Undo");
        assert!(matches!(answers.try_recv().unwrap(), Answer::Undo));
    }

    #[test]
    fn a_single_target_still_submits_immediately() {
        let target = Target::Player(PlayerId(0));
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseTargets {
            slots: vec![vec![target]],
        });
        click_question_label(&mut app, &ctx, "You");
        let Answer::Targets(targets) = answers.try_recv().unwrap() else {
            panic!("target answer expected")
        };
        assert_eq!(targets, vec![vec![target]]);
    }

    #[test]
    fn undo_removes_the_most_recent_target_instead_of_the_highest_slot() {
        let mine = Target::Player(PlayerId(0));
        let theirs = Target::Player(PlayerId(1));
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseTargets {
            slots: vec![vec![mine], vec![theirs]],
        });
        click_question_label(&mut app, &ctx, "Opponent");
        click_question_label(&mut app, &ctx, "You");
        click_question_label(&mut app, &ctx, "Undo");
        assert_eq!(app.picked_targets, BTreeMap::from([(1, theirs)]));
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "You");
        click_question_label(&mut app, &ctx, "You");
        assert_eq!(app.target_pick_order, vec![1]);
        click_question_label(&mut app, &ctx, "Undo");
        assert!(app.picked_targets.is_empty());
        assert!(app.target_pick_order.is_empty());
        assert!(!app.has_selection());
        assert!(answers.try_recv().is_err());
    }

    #[test]
    fn concession_confirmation_can_be_cancelled_without_answering() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::Priority {
            legal: Default::default(),
        });
        click_question_label(&mut app, &ctx, "Concede");
        assert!(app.confirm_concede);
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Cancel concede");
        assert!(!app.confirm_concede);
        assert!(app.current.is_some());
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Concede");
        click_question_label(&mut app, &ctx, "Really concede?");
        assert!(matches!(
            answers.try_recv().unwrap(),
            Answer::Action(Action::Concede)
        ));
        assert!(app.current.is_none());
        assert!(!app.confirm_concede);
    }

    #[test]
    fn undo_closes_play_options_before_undoing_a_game_action() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::Priority {
            legal: Default::default(),
        });
        app.play_choice = Some(ObjectId(1));
        assert!(app.has_selection());
        click_question_label(&mut app, &ctx, "Undo");
        assert!(app.play_choice.is_none());
        assert!(!app.has_selection());
        assert!(app.current.is_some());
        assert!(answers.try_recv().is_err());

        app.current.as_mut().unwrap().choice.undo = true;
        app.play_choice = Some(ObjectId(1));
        click_question_label(&mut app, &ctx, "Undo");
        assert!(app.play_choice.is_none());
        assert!(answers.try_recv().is_err());
        click_question_label(&mut app, &ctx, "Undo");
        assert!(matches!(answers.try_recv().unwrap(), Answer::Undo));
    }

    #[test]
    fn x_controls_support_large_values_and_the_u32_boundary() {
        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseX { min: 10, max: 1000 });
        app.picked_x = 200;
        click_question_label(&mut app, &ctx, "Confirm");
        assert!(matches!(answers.try_recv().unwrap(), Answer::Number(200)));

        let (mut app, answers, ctx) = question_app(ChoiceKind::ChooseX {
            min: u32::MAX - 5,
            max: u32::MAX,
        });
        click_question_label(&mut app, &ctx, &u32::MAX.to_string());
        assert!(matches!(
            answers.try_recv().unwrap(),
            Answer::Number(u32::MAX)
        ));
    }

    #[test]
    fn view_backlogs_are_bounded_and_drained_before_a_new_question() {
        let viewer = PlayerId(0);
        let mut view = PlayerView {
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer,
            turn: 1,
            active_player: viewer,
            step: mtg_core::Step::PrecombatMain,
            priority: Some(viewer),
            players: Default::default(),
            visible: Default::default(),
            stack: Vec::new(),
        };
        let (questions_tx, questions) = std::sync::mpsc::channel();
        let (answers, _answers_rx) = std::sync::mpsc::channel();
        let (views_tx, views) = std::sync::mpsc::channel();
        let art = CardArt::start(
            egui::Context::default(),
            crate::art::ArtConfig {
                names: Default::default(),
                db: None,
                cache_dir: std::env::temp_dir().join("mtgo-ui-poll-test"),
            },
        );
        let mut app = GuiApp::new(
            questions,
            answers,
            CardTexts::default(),
            art,
            Default::default(),
        );
        app.watch(views);
        for turn in 1..=(VIEW_BATCH_LIMIT + 1) as u32 {
            view.turn = turn;
            views_tx.send(view.clone()).unwrap();
        }
        view.turn += 1;
        questions_tx
            .send(Question {
                choice: mtg_engine::Choice {
                    id: 1,
                    who: viewer,
                    kind: ChoiceKind::Confirm,
                    because: "test".into(),
                    default: Some(Answer::Bool(false)),
                    undo: false,
                },
                view: view.clone(),
            })
            .unwrap();
        assert!(app.poll());
        assert_eq!(
            app.last_view.as_ref().unwrap().turn,
            VIEW_BATCH_LIMIT as u32
        );
        assert!(app.current.is_none());
        assert!(!app.poll());
        assert_eq!(app.last_view.as_ref().unwrap().turn, view.turn);
        assert!(app.current.is_some());
        assert!(!app.poll());
        assert_eq!(app.last_view.as_ref().unwrap().turn, view.turn);
    }

    #[test]
    fn cleanup_discards_offer_only_the_viewers_hand_and_exact_count() {
        use mtg_core::{Zone, ZoneRef};
        use mtg_engine::view::ObjectView;
        let viewer = PlayerId(0);
        let object = |id, zone| ObjectView {
            id: ObjectId(id),
            zone,
            controller: viewer,
            card: None,
            face: 0,
            adventure_player: None,
            tapped: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            targets: Vec::new(),
            is_ability: false,
            ability: None,
            attacking: false,
            attacking_target: None,
            blocking: None,
        };
        let objects = [
            object(1, ZoneRef::of(Zone::Hand, viewer)),
            object(2, ZoneRef::of(Zone::Hand, viewer)),
            object(3, ZoneRef::of(Zone::Hand, PlayerId(1))),
            object(4, ZoneRef::of(Zone::Graveyard, viewer)),
        ];
        let view = PlayerView {
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer,
            turn: 1,
            active_player: viewer,
            step: mtg_core::Step::Cleanup,
            priority: None,
            players: Default::default(),
            visible: objects
                .into_iter()
                .map(|object| (object.id, object))
                .collect(),
            stack: Vec::new(),
        };
        assert_eq!(
            object_selection_options(&ChoiceKind::DiscardToHandSize { count: 1 }, &view),
            Some((vec![ObjectId(1), ObjectId(2)], 1, 1))
        );
        assert_eq!(
            object_selection_options(
                &ChoiceKind::ChooseObjects {
                    from: vec![ObjectId(4)],
                    min: 0,
                    max: 1
                },
                &view,
            ),
            Some((vec![ObjectId(4)], 0, 1))
        );
    }

    #[test]
    fn all_target_slots_must_be_selected_and_legal() {
        let player = Target::Player(PlayerId(0));
        let object = Target::Object(ObjectId(3));
        let slots = vec![vec![player, object], vec![object]];
        let mut picked = BTreeMap::from([(1, object)]);
        assert!(selected_target_answer(&slots, &picked).is_none());
        picked.insert(0, player);
        let Some(Answer::Targets(targets)) = selected_target_answer(&slots, &picked) else {
            panic!("complete selections must produce a target answer");
        };
        assert_eq!(targets, vec![vec![player], vec![object]]);
        picked.insert(1, player);
        assert!(selected_target_answer(&slots, &picked).is_none());
    }

    #[test]
    fn repeated_targets_and_empty_slots_keep_their_positions() {
        let target = Target::Object(ObjectId(3));
        let slots = vec![vec![target], vec![], vec![target]];
        let picked = BTreeMap::from([(0, target), (2, target)]);
        let Some(Answer::Targets(targets)) = selected_target_answer(&slots, &picked) else {
            panic!("the same legal target may occupy multiple slots");
        };
        assert_eq!(targets, vec![vec![target], vec![], vec![target]]);
    }
}
