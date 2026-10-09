//! The application: main menu, decks, match setup, and the match itself.
//!
//! Every match runs on a worker thread and the window only draws — the same arrangement the game
//! screen always had, now one level up. The worker is a local match against the bot, a hosted
//! match, or a joined one; to the window they differ only in what the setup screen asks for and
//! in the waiting screen before the first game.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
};

use egui::{RichText, Ui};
use mtg_net::fairness::Seed;
use mtg_session::{
    game::{CardKey, DeckSpec, Format, GameCards, MatchSettings, deck_problems},
    matches::{GameEnd, GameStart, MatchEnd, play_local_match_with_players},
};
use mtg_store::Store;

use crate::{
    PolicyThenUi, UiDecider,
    art::{ArtConfig, CardArt},
    cards_text::CardTexts,
    decks::{self, DeckId, ImportSummary, LocalSource},
    net,
    seat::{BotSeat, MatchEvent, UiSeat},
    ui::GuiApp,
};

use crate::theme::{GOOD, WARN};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Bot,
    Host,
    Join,
    /// A deck against the goldfish, from the deck builder.
    Playtest,
}

impl Mode {
    fn title(self) -> &'static str {
        match self {
            Mode::Bot => "Play vs Bot",
            Mode::Host => "Host a game",
            Mode::Join => "Join a game",
            Mode::Playtest => "Playtest",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Menu,
    Decks,
    Setup(Mode),
    Match,
}

/// A match in progress, as the window sees it.
struct Running {
    mode: Mode,
    /// `None` only while the art cache is being handed back.
    gui: Option<GuiApp>,
    events: Receiver<MatchEvent>,
    sideboards: Sender<DeckSpec>,
    left: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    status: VecDeque<String>,
    invite: Option<String>,
    settings: Option<MatchSettings>,
    game: Option<GameStart>,
    /// The last game's result, shown until the next game starts.
    last_game: Option<GameEnd>,
    sideboard: Option<(DeckSpec, BTreeMap<CardKey, String>)>,
    ended: Option<Result<MatchEnd, String>>,
}

enum DatabaseUpdate {
    Progress(String),
    CatalogReady,
    Finished(Result<String, String>),
}

enum DatabasePoll {
    CatalogReady,
    Pending { backlog: bool },
    Finished(Result<String, String>),
    Stopped,
}

fn drain_database_updates(
    rx: &Receiver<DatabaseUpdate>,
    status: &mut Option<String>,
) -> DatabasePoll {
    for _ in 0..MAX_EVENTS_PER_FRAME {
        match rx.try_recv() {
            Ok(DatabaseUpdate::Progress(progress)) => *status = Some(progress),
            Ok(DatabaseUpdate::CatalogReady) => return DatabasePoll::CatalogReady,
            Ok(DatabaseUpdate::Finished(result)) => return DatabasePoll::Finished(result),
            Err(TryRecvError::Empty) => return DatabasePoll::Pending { backlog: false },
            Err(TryRecvError::Disconnected) => return DatabasePoll::Stopped,
        }
    }
    DatabasePoll::Pending { backlog: true }
}

pub struct App {
    ctx: egui::Context,
    screen: Screen,
    /// The image cache, lent to each match's game screen and handed back after.
    art: Option<CardArt>,
    store: Option<Store>,
    /// A failed database open or refresh, retained until an explicit retry.
    store_error: Option<String>,
    decks: Vec<(DeckId, String)>,
    card_count: u64,

    format: Format,
    best_of: u8,
    players: u8,
    my_deck: DeckId,
    bot_deck: DeckId,
    join_link: String,
    host_port: String,
    host_address: String,
    host_internet: bool,
    host_name: String,
    ngrok_token: String,
    ngrok_domain: String,
    remember_ngrok: bool,
    nearby: Option<mtg_net::discovery::Browser>,
    nearby_error: Option<String>,
    setup_error: Option<String>,
    /// Coverage of the chosen deck, recomputed when the choice changes.
    coverage: Option<(DeckId, usize, usize)>,

    import_name: String,
    import_text: String,
    import_result: Option<Result<ImportSummary, String>>,
    selected_deck: Option<DeckId>,
    /// A deck whose Delete has been clicked once.
    confirm_delete: Option<i64>,
    db_file: String,
    db_job: Option<Receiver<DatabaseUpdate>>,
    db_status: Option<String>,

    running: Option<Running>,
    /// The deck builder, while a deck is open in it.
    builder: Option<crate::builder::Builder>,
}

impl App {
    pub fn new(ctx: egui::Context) -> Self {
        crate::theme::install(&ctx);
        let mut app = Self {
            art: Some(CardArt::start(ctx.clone(), ArtConfig::standard())),
            ctx,
            screen: Screen::Menu,
            store: None,
            store_error: None,
            decks: Vec::new(),
            card_count: 0,
            format: Format::Constructed,
            best_of: 1,
            players: 2,
            my_deck: DeckId::Demo,
            bot_deck: DeckId::Demo,
            join_link: String::new(),
            host_port: "0".into(),
            host_address: String::new(),
            host_internet: false,
            host_name: "Local game".into(),
            ngrok_token: String::new(),
            ngrok_domain: String::new(),
            remember_ngrok: false,
            nearby: None,
            nearby_error: None,
            setup_error: None,
            coverage: None,
            import_name: String::new(),
            import_text: String::new(),
            import_result: None,
            selected_deck: None,
            confirm_delete: None,
            db_file: String::new(),
            db_job: None,
            db_status: None,
            running: None,
            builder: None,
        };
        app.refresh();
        if let Some(token) = app
            .store
            .as_ref()
            .and_then(|store| store.meta("ngrok_authtoken").ok().flatten())
            .filter(|token| !token.is_empty())
        {
            app.ngrok_token = token;
            app.remember_ngrok = true;
            app.host_internet = true;
        } else if let Ok(token) = std::env::var("NGROK_AUTHTOKEN") {
            app.ngrok_token = token;
            app.host_internet = !app.ngrok_token.trim().is_empty();
        }
        app.ngrok_domain = app
            .store
            .as_ref()
            .and_then(|store| store.meta("ngrok_domain").ok().flatten())
            .filter(|domain| !domain.is_empty())
            .or_else(|| std::env::var("NGROK_DOMAIN").ok())
            .unwrap_or_default();
        app.start_db_job(None);
        app
    }

    /// Re-read the database: after an import, or a deck saved or deleted.
    fn refresh(&mut self) {
        self.store = decks::open_store();
        self.refresh_from_store();
    }

    /// Refresh displayed data without replacing the active database connection.
    fn refresh_from_store(&mut self) {
        let snapshot = (|| {
            let count = self.store.as_ref().map_or(Ok(0), |store| {
                store.card_count().map_err(|error| error.to_string())
            })?;
            Ok::<_, String>((count, decks::deck_list(self.store.as_ref())?))
        })();
        let (count, next) = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.store_error = Some(error);
                return;
            }
        };
        self.store_error = None;
        self.card_count = count;
        self.my_deck =
            retained_deck_choice(self.my_deck, &self.decks, &next).unwrap_or(DeckId::Demo);
        self.bot_deck =
            retained_deck_choice(self.bot_deck, &self.decks, &next).unwrap_or(DeckId::Demo);
        self.selected_deck = self
            .selected_deck
            .and_then(|id| retained_deck_choice(id, &self.decks, &next));
        self.decks = next;
        self.coverage = None;
    }

    fn deck_name(&self, id: DeckId) -> String {
        self.decks
            .iter()
            .find(|(d, _)| *d == id)
            .map_or_else(|| "?".into(), |(_, n)| n.clone())
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.poll_db_job();
        egui::Panel::bottom("bug-report-footer")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(format!("MTGO RS v{}", env!("CARGO_PKG_VERSION")));
                    let details = format!(
                        "Version: {}\nPlatform: {} ({})\n\nSteps to reproduce:\n\nExpected result:\n\nActual result:\n",
                        env!("CARGO_PKG_VERSION"),
                        std::env::consts::OS,
                        std::env::consts::ARCH,
                    );
                    let query = url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("subject", "MTGO RS bug report")
                        .append_pair("body", &details)
                        .finish()
                        .replace('+', "%20");
                    ui.hyperlink_to(
                        "Report a bug: mail@johnodonnell.xyz",
                        format!("mailto:mail@johnodonnell.xyz?{query}"),
                    )
                    .on_hover_text("Opens your email app with a bug-report template.");
                    if ui.small_button("Copy email").clicked() {
                        ui.ctx().copy_text("mail@johnodonnell.xyz".to_owned());
                    }
                });
            });
        if self.screen != Screen::Setup(Mode::Join) {
            self.nearby = None;
            self.nearby_error = None;
        }
        match self.screen {
            Screen::Menu => self.menu(ui),
            Screen::Decks => self.decks_screen(ui),
            Screen::Setup(mode) => self.setup(ui, mode),
            Screen::Match => self.match_screen(ui),
        }
        if let Some(art) = &mut self.art {
            crate::widgets::show_enlarged_card(ui, art);
        }
    }
}

// ---- menu ------------------------------------------------------------------

impl App {
    fn menu(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::INK).inner_margin(20))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.10);
                    ui.label(
                        RichText::new("M T G O   R S")
                            .size(13.0)
                            .color(crate::theme::GOLD),
                    );
                    ui.add_space(12.0);
                    ui.heading(RichText::new("Make your next move.").size(38.0).strong());
                    ui.label(
                        RichText::new("Build a deck. Find a match. Play your cards.")
                            .color(crate::theme::MUTED),
                    );
                    ui.add_space(30.0);

                    let button = |ui: &mut Ui, text: &str| {
                        ui.add_sized(
                            [ui.available_width().min(320.0), 44.0],
                            egui::Button::new(RichText::new(text).size(16.0)),
                        )
                    };
                    for mode in [Mode::Bot, Mode::Host, Mode::Join] {
                        let response = if mode == Mode::Bot {
                            ui.add_sized(
                                [ui.available_width().min(320.0), 46.0],
                                egui::Button::new(
                                    RichText::new(mode.title())
                                        .size(16.0)
                                        .strong()
                                        .color(crate::theme::INK),
                                )
                                .fill(crate::theme::GOLD),
                            )
                        } else {
                            button(ui, mode.title())
                        };
                        if response.clicked() {
                            self.setup_error = None;
                            self.screen = Screen::Setup(mode);
                        }
                        ui.add_space(6.0);
                    }
                    if button(ui, "Decks").clicked() {
                        self.screen = Screen::Decks;
                    }
                    ui.add_space(6.0);
                    if button(ui, "Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    ui.add_space(24.0);
                    if self.card_count == 0 {
                        ui.label(
                            RichText::new("Preparing the card database automatically…").color(WARN),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!(
                                "{} cards in the local database",
                                self.card_count
                            ))
                            .weak(),
                        );
                    }
                    if let Some(status) = &self.db_status {
                        ui.label(status);
                    }
                });
            });
    }
}

// ---- decks -----------------------------------------------------------------

impl App {
    fn decks_screen(&mut self, ui: &mut Ui) {
        // Building needs somewhere to save, so the database is created on first visit.
        if self.store.is_none() && self.store_error.is_none() {
            match decks::create_store() {
                Ok(store) => {
                    self.store = Some(store);
                    self.refresh_from_store();
                }
                Err(error) => self.store_error = Some(error),
            }
        }
        if self.builder.is_some() {
            return self.builder_screen(ui);
        }

        egui::Panel::top("decks-top")
            .frame(crate::theme::panel())
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("⏴ Menu").clicked() {
                        self.screen = Screen::Menu;
                        self.confirm_delete = None;
                    }
                    ui.heading("Decks");
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.store.is_some(),
                            crate::theme::primary_button(
                                RichText::new("+ New deck")
                                    .strong()
                                    .color(crate::theme::INK),
                            ),
                        )
                        .clicked()
                    {
                        self.builder = Some(crate::builder::Builder::new(
                            crate::builder::Draft::new("New deck"),
                        ));
                        self.confirm_delete = None;
                    }
                });
                if let Some(error) = &self.store_error {
                    ui.label(
                        RichText::new(format!("Could not read the card database: {error}"))
                            .color(WARN),
                    );
                    if ui.button("Retry database").clicked() {
                        self.store_error = None;
                        if self.store.is_some() {
                            self.refresh_from_store();
                        }
                        ui.ctx().request_repaint();
                    }
                }
            });

        egui::Panel::bottom("card-db")
            .frame(crate::theme::panel())
            .show(ui, |ui| self.card_database(ui));

        egui::Panel::left("deck-list")
            .frame(crate::theme::panel())
            .default_size(340.0)
            .show(ui, |ui| {
                ui.heading("Your decks");
                ui.label(
                    RichText::new("Select to inspect · double-click to edit")
                        .small()
                        .color(crate::theme::MUTED),
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut delete = None;
                    for (id, name) in self.decks.clone() {
                        let selected = self.selected_deck == Some(id);
                        egui::Frame::new()
                            .fill(if selected {
                                crate::theme::SURFACE
                            } else {
                                crate::theme::PANEL
                            })
                            .stroke(egui::Stroke::new(
                                1.0,
                                if selected {
                                    crate::theme::GOLD
                                } else {
                                    crate::theme::BORDER
                                },
                            ))
                            .corner_radius(8)
                            .inner_margin(8)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    let label = ui
                                        .selectable_label(selected, &name)
                                        .on_hover_text("click for details, double-click to edit");
                                    if label.clicked() {
                                        if self.selected_deck != Some(id) {
                                            self.confirm_delete = None;
                                        }
                                        self.selected_deck = Some(id);
                                    }
                                    if label.double_clicked() {
                                        self.open_builder(id);
                                    }
                                    // Deleting takes two clicks: the first arms it, the second confirms.
                                    if let DeckId::Stored(n) = id {
                                        if self.confirm_delete == Some(n) {
                                            if ui
                                                .small_button(
                                                    RichText::new("Really delete?").color(WARN),
                                                )
                                                .clicked()
                                            {
                                                delete = Some(n);
                                            }
                                            if ui.small_button("Cancel").clicked() {
                                                self.confirm_delete = None;
                                            }
                                        } else if ui.small_button("Delete").clicked() {
                                            self.confirm_delete = Some(n);
                                        }
                                    }
                                });
                            });
                        ui.add_space(6.0);
                    }
                    if let (Some(n), Some(store)) = (delete, &self.store) {
                        self.confirm_delete = None;
                        match store.delete_deck(n) {
                            Ok(()) => {
                                if self.selected_deck == Some(DeckId::Stored(n)) {
                                    self.selected_deck = None;
                                }
                                self.refresh_from_store();
                            }
                            Err(error) => {
                                self.import_result =
                                    Some(Err(format!("Could not delete deck: {error}")));
                            }
                        }
                    }
                });

                if let Some(id) = self.selected_deck {
                    ui.separator();
                    self.deck_details(ui, id);
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::INK).inner_margin(24))
            .show(ui, |ui| self.import_form(ui));
    }

    fn open_builder(&mut self, id: DeckId) {
        self.confirm_delete = None;
        let draft = match (id, &self.store) {
            (DeckId::Stored(n), Some(store)) => crate::builder::Draft::load(store, n),
            _ => Err("the demo deck is built in and cannot be edited".into()),
        };
        match draft {
            Ok(d) => self.builder = Some(crate::builder::Builder::new(d)),
            Err(e) => self.import_result = Some(Err(e)),
        }
    }

    /// The deck builder: its bar across the top, the deck on the right, search in the middle.
    fn builder_screen(&mut self, ui: &mut Ui) {
        use crate::builder::BuilderAction;
        let art = self
            .art
            .get_or_insert_with(|| CardArt::start(self.ctx.clone(), ArtConfig::standard()));
        let (Some(builder), Some(store)) = (self.builder.as_mut(), self.store.as_mut()) else {
            self.builder = None;
            return;
        };

        let previous_id = builder.draft.id;
        let mut action = BuilderAction::Nothing;
        egui::Panel::top("builder-top")
            .frame(crate::theme::panel())
            .show(ui, |ui| action = builder.top_bar(ui, store));
        egui::Panel::right("builder-deck")
            .frame(crate::theme::panel())
            .default_size(400.0)
            .min_size(280.0)
            .max_size((ui.available_width() * 0.48).max(280.0))
            .show(ui, |ui| builder.deck(ui, store, art));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(crate::theme::INK).inner_margin(18))
            .show(ui, |ui| builder.search(ui, store, art));
        let saved_id = builder.draft.id;

        match action {
            BuilderAction::Nothing => {}
            BuilderAction::Close => {
                self.builder = None;
                self.refresh_from_store();
            }
            BuilderAction::Saved => {
                if let Some(id) = saved_id {
                    self.my_deck = deck_choice_after_save(self.my_deck, previous_id, id);
                    self.bot_deck = deck_choice_after_save(self.bot_deck, previous_id, id);
                    self.selected_deck = self
                        .selected_deck
                        .map(|choice| deck_choice_after_save(choice, previous_id, id));
                }
                self.refresh_from_store();
            }
            BuilderAction::Playtest(deck, format) => {
                let settings = MatchSettings { format, best_of: 1 };
                self.launch(Mode::Playtest, settings, deck, None, 0);
            }
        }
    }

    fn deck_details(&mut self, ui: &mut Ui, id: DeckId) {
        ui.horizontal(|ui| {
            if matches!(id, DeckId::Stored(_))
                && ui.button(RichText::new("Edit").strong()).clicked()
            {
                self.open_builder(id);
            }
            if ui
                .button("Playtest")
                .on_hover_text("against an opponent who does nothing")
                .clicked()
                && let Ok(deck) = decks::deck_spec(self.store.as_ref(), id)
            {
                let format = if deck.commander.is_some() {
                    Format::Commander
                } else {
                    Format::Constructed
                };
                self.launch(
                    Mode::Playtest,
                    MatchSettings { format, best_of: 1 },
                    deck,
                    None,
                    0,
                );
            }
        });
        match decks::deck_spec(self.store.as_ref(), id) {
            Ok(spec) => {
                let side: u32 = spec.side.iter().map(|(_, n)| u32::from(*n)).sum();
                ui.label(format!("{} main · {} sideboard", spec.main_count(), side));
                if spec.commander.is_some() {
                    ui.label("has a commander");
                }
                let (ok, total) = decks::coverage(self.store.as_ref(), &spec);
                let color = if ok == total { GOOD } else { WARN };
                ui.label(
                    RichText::new(format!("{ok} of {total} unique cards fully supported"))
                        .color(color),
                )
                .on_hover_text(
                    "Checks each distinct card's full rules text and layout, including every face. \
                         Open the deck builder for details about unsupported rules.",
                );
            }
            Err(e) => {
                ui.label(RichText::new(e).color(WARN));
            }
        }
    }

    fn import_form(&mut self, ui: &mut Ui) {
        let dropped = ui.ctx().input(|input| input.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let result = if dropped.len() == 1 {
                read_deck_file(dropped[0].as_ref())
            } else {
                Err("Drop one deck file at a time.".into())
            };
            match result {
                Ok(text) => {
                    self.import_text = text;
                    self.import_result = None;
                    if self.import_name.trim().is_empty() {
                        self.import_name = dropped[0]
                            .path()
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Imported deck")
                            .to_owned();
                    }
                }
                Err(error) => self.import_result = Some(Err(error)),
            }
        }
        ui.label(
            RichText::new("BRING YOUR OWN DECK")
                .size(10.0)
                .color(crate::theme::GOLD),
        );
        ui.heading("Import a deck");
        ui.add_space(6.0);
        ui.label(
            RichText::new("Paste a list or drop a UTF-8 text, CSV, TSV, or JSON deck file. Text lists from Arena, MTGO, Moxfield, Archidekt, and TappedOut are supported. CSV/TSV use count, name, and section columns; JSON uses an entries array with count, name, and section.")
                .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut self.import_name).changed() {
                self.import_result = None;
            }
        });
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 140.0)
            .show(ui, |ui| {
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut self.import_text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(18)
                            .hint_text("4 Lightning Bolt\n20 Mountain\n\nSideboard\n2 …"),
                    )
                    .changed()
                {
                    self.import_result = None;
                }
            });

        let can = self.store.is_some() && self.card_count > 0;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can,
                    crate::theme::primary_button(
                        RichText::new("Import deck")
                            .strong()
                            .color(crate::theme::INK),
                    ),
                )
                .clicked()
            {
                let result = self
                    .store
                    .as_mut()
                    .ok_or_else(|| "no card database".to_string())
                    .and_then(|store| {
                        decks::import_deck(store, &self.import_name, &self.import_text)
                    });
                if result.as_ref().is_ok_and(ImportSummary::is_complete) {
                    self.import_text.clear();
                }
                self.import_result = Some(result);
                self.refresh_from_store();
            }
            if !can {
                ui.label(RichText::new("import the card database first (below)").color(WARN));
            }
        });

        match &self.import_result {
            Some(Ok(s)) => {
                let commander = if s.commander { " + commander" } else { "" };
                ui.label(
                    RichText::new(format!(
                        "Saved: {} main, {} sideboard{commander}.",
                        s.main, s.side
                    ))
                    .color(GOOD),
                );
                for line in s.unmatched.iter().take(12) {
                    ui.label(RichText::new(format!("not found: {line}")).color(WARN));
                }
                for line in s.unreadable.iter().take(6) {
                    ui.label(RichText::new(format!("could not read: {line}")).color(WARN));
                }
                if !s.is_complete() {
                    ui.label(
                        RichText::new(
                            "The pasted list is kept above so you can correct these lines.",
                        )
                        .weak(),
                    );
                }
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(WARN));
            }
            None => {}
        }
    }

    fn card_database(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Card database").strong());
            ui.label(format!("{} cards", self.card_count));
        });
        let busy = self.db_job.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Download from Scryfall"))
                .on_hover_text("Download and import Scryfall's oracle card database. Progress is shown below. Once a day at most.")
                .clicked()
            {
                self.start_db_job(None);
            }
            ui.label("or import a file:");
            ui.add(egui::TextEdit::singleline(&mut self.db_file).hint_text("/path/to/oracle-cards.json"));
            if ui.add_enabled(!busy && !self.db_file.trim().is_empty(), egui::Button::new("Import file")).clicked() {
                self.start_db_job(Some(self.db_file.trim().to_string()));
            }
        });
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    self.db_status
                        .as_deref()
                        .unwrap_or("Opening card database…"),
                );
            });
        } else if let Some(status) = &self.db_status {
            ui.label(status);
        }
        ui.add_space(4.0);
    }

    /// Import the card database off the UI thread: from Scryfall, or from a file on disk.
    fn start_db_job(&mut self, file: Option<String>) {
        let (tx, rx) = mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                let mut store = decks::create_store()?;
                let catalog = match file {
                    Some(path) => {
                        let f = std::fs::File::open(&path)
                            .map_err(|e| format!("cannot open {path}: {e}"))?;
                        let report = mtg_oracle::fetch::import_from_with_progress(
                            &mut store,
                            std::io::BufReader::new(f),
                            &mut |progress| {
                                let _ = tx.send(DatabaseUpdate::Progress(progress.status()));
                            },
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(report.summary())
                    }
                    None => match mtg_oracle::fetch::fetch_and_import_with_progress(
                        &mut store,
                        false,
                        &mut |progress| {
                            let _ = tx.send(DatabaseUpdate::Progress(progress.status()));
                        },
                    ) {
                        Ok(Some(report)) => Ok(report.summary()),
                        Ok(None) => {
                            Ok("already up to date (Scryfall's file is refreshed daily)".into())
                        }
                        Err(e) => Err(e.to_string()),
                    },
                };
                let _ = tx.send(DatabaseUpdate::CatalogReady);
                ctx.request_repaint();
                // Even offline, an existing catalog can repair/check its image cache.
                let images = crate::art::populate_cache(&store, &mut |status| {
                    let _ = tx.send(DatabaseUpdate::Progress(status));
                    ctx.request_repaint();
                });
                match (catalog, images) {
                    (Ok(catalog), Ok(images)) => Ok(format!("{catalog}. {images}")),
                    (Err(error), _) => Err(error),
                    (_, Err(error)) => Err(error),
                }
            })();
            let _ = tx.send(DatabaseUpdate::Finished(result));
            ctx.request_repaint();
        });
        self.db_job = Some(rx);
        self.db_status = None;
    }

    fn poll_db_job(&mut self) {
        if let Some(rx) = &self.db_job {
            match drain_database_updates(rx, &mut self.db_status) {
                DatabasePoll::CatalogReady => {
                    self.refresh();
                    if self.art.is_some() {
                        self.art = Some(CardArt::start(self.ctx.clone(), ArtConfig::standard()));
                    }
                    self.ctx.request_repaint();
                }
                DatabasePoll::Finished(result) => {
                    self.db_status = Some(match result {
                        Ok(status) => status,
                        Err(error) => format!("Database update failed: {error}"),
                    });
                    self.db_job = None;
                    self.refresh();
                }
                DatabasePoll::Pending { backlog: true } => self.ctx.request_repaint(),
                DatabasePoll::Pending { backlog: false } => {}
                DatabasePoll::Stopped => {
                    self.db_status =
                        Some("Database update stopped unexpectedly. Please retry.".into());
                    self.db_job = None;
                }
            }
        }
        if self.db_job.is_some() {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

// ---- setup -----------------------------------------------------------------

impl App {
    fn setup(&mut self, ui: &mut Ui, mode: Mode) {
        egui::CentralPanel::default().show(ui, |ui| {
            // A centred card of settings, rather than a form pressed into a corner.
            let width = 620.0_f32.min(ui.available_width());
            ui.add_space(ui.available_height() * 0.08);
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                egui::Frame::group(ui.style())
                    .inner_margin(20.0)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            self.setup_form(ui, mode);
                        });
                    });
            });
        });
        self.update_coverage();
    }

    fn setup_form(&mut self, ui: &mut Ui, mode: Mode) {
        {
            ui.horizontal(|ui| {
                if ui.button("⏴ Menu").clicked() {
                    self.screen = Screen::Menu;
                }
                ui.heading(mode.title());
            });
            ui.add_space(12.0);

            egui::Grid::new("setup")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    if mode == Mode::Join {
                        ui.label("Invite link");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.join_link)
                                .desired_rows(3)
                                .desired_width(460.0)
                                .hint_text("paste the link the host sent you"),
                        );
                        ui.end_row();
                    } else {
                        ui.label("Format");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.format, Format::Constructed, "Constructed")
                                .on_hover_text("60+ cards, up to 4 of each, 20 life");
                            ui.radio_value(&mut self.format, Format::Standard, "Standard")
                                .on_hover_text("Standard legality from the current card catalog");
                            ui.radio_value(&mut self.format, Format::Commander, "Commander")
                                .on_hover_text(
                                    "a commander and 99 singleton cards in its colours, 40 life",
                                );
                        });
                        ui.end_row();

                        if self.format == Format::Commander
                            && matches!(mode, Mode::Host | Mode::Bot)
                        {
                            ui.label("Players");
                            ui.horizontal(|ui| {
                                for count in 2..=4 {
                                    ui.radio_value(&mut self.players, count, count.to_string());
                                }
                            });
                            ui.end_row();
                        } else {
                            self.players = 2;
                        }
                        if self.players > 2 {
                            self.best_of = 1;
                        }
                        ui.label("Match");
                        ui.add_enabled_ui(self.players == 2, |ui| {
                            ui.radio_value(&mut self.best_of, 1, "Best of 1");
                            ui.radio_value(&mut self.best_of, 3, "Best of 3");
                        });
                        ui.end_row();
                    }

                    ui.label("Your deck");
                    self.deck_picker(ui, "my-deck", true);
                    ui.end_row();

                    if mode == Mode::Bot {
                        ui.label("Bot's deck");
                        self.deck_picker(ui, "bot-deck", false);
                        ui.end_row();
                    }

                    if mode == Mode::Host {
                        ui.label("Game name");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.host_name).desired_width(220.0),
                        );
                        ui.end_row();
                        ui.label("Play with");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.host_internet, false, "Nearby players");
                            ui.radio_value(&mut self.host_internet, true, "Friends online");
                        });
                        ui.end_row();
                    }
                });

            if mode == Mode::Host {
                if self.host_internet {
                    ui.label("An internet tunnel opens automatically. Share the invite with your friend.");
                    ui.horizontal(|ui| {
                        ui.label("Ngrok authtoken");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ngrok_token)
                                .password(true)
                                .desired_width(300.0),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.hyperlink_to(
                            "Create a free ngrok account",
                            "https://dashboard.ngrok.com/signup",
                        );
                        ui.hyperlink_to(
                            "Get your authtoken",
                            "https://dashboard.ngrok.com/get-started/your-authtoken",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.remember_ngrok, "Remember token on this device");
                        if ui.small_button("Forget token").clicked() {
                            let result = self
                                .store
                                .as_ref()
                                .map_or(Ok(()), |store| store.set_meta("ngrok_authtoken", ""));
                            match result {
                                Ok(()) => {
                                    self.ngrok_token.clear();
                                    self.remember_ngrok = false;
                                }
                                Err(error) => {
                                    self.setup_error =
                                        Some(format!("Could not forget token: {error}"))
                                }
                            }
                        }
                    });
                    ui.label(RichText::new("Only the host needs an account. No ngrok installation or router setup is needed.").weak());
                } else {
                    ui.label("Players on the same network can find this game under Nearby games.");
                }
                egui::CollapsingHeader::new("Advanced connection settings").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Port");
                        ui.add(egui::TextEdit::singleline(&mut self.host_port).desired_width(80.0))
                            .on_hover_text("0 chooses a free port automatically.");
                    });
                    if self.host_internet {
                        ui.horizontal(|ui| {
                            ui.label("Ngrok domain");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ngrok_domain)
                                    .desired_width(260.0)
                                    .hint_text("optional — your assigned ngrok domain"),
                            );
                        });
                        ui.hyperlink_to(
                            "Find your ngrok domain",
                            "https://dashboard.ngrok.com/domains",
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Additional address");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.host_address)
                                    .desired_width(220.0),
                            );
                        });
                    }
                });
            }
            if mode == Mode::Join {
                ui.label(RichText::new("The host chooses the format and match length.").weak());
                self.nearby_games(ui);
            }
            if self.format == Format::Commander && mode != Mode::Join {
                ui.label(
                    RichText::new(
                        "Commander needs imported decks with a commander; the demo deck has none.",
                    )
                    .weak(),
                );
            }

            ui.add_space(8.0);
            if let Some((_, ok, total)) = self.coverage.filter(|(id, ..)| *id == self.my_deck) {
                let color = if ok == total { GOOD } else { WARN };
                ui.label(
                    RichText::new(format!(
                        "{ok} of {total} unique cards in your deck are fully supported"
                    ))
                    .color(color),
                );
            }
            if let Some(err) = &self.setup_error {
                ui.label(RichText::new(err).color(WARN));
            }

            ui.add_space(12.0);
            let label = match mode {
                Mode::Bot | Mode::Playtest => "Start",
                Mode::Host => "Host",
                Mode::Join => "Join",
            };
            if ui
                .add_sized(
                    [160.0, 36.0],
                    egui::Button::new(RichText::new(label).size(16.0)),
                )
                .clicked()
            {
                self.start(mode);
            }
        }
    }

    /// Recompute the chosen deck's playable coverage when the choice changes.
    fn update_coverage(&mut self) {
        if self.coverage.is_none_or(|(id, ..)| id != self.my_deck)
            && let Ok(spec) = decks::deck_spec(self.store.as_ref(), self.my_deck)
        {
            let (ok, total) = decks::coverage(self.store.as_ref(), &spec);
            self.coverage = Some((self.my_deck, ok, total));
        }
    }

    fn deck_picker(&mut self, ui: &mut Ui, id: &str, mine: bool) {
        let current = if mine { self.my_deck } else { self.bot_deck };
        let mut chosen = current;
        egui::ComboBox::from_id_salt(id)
            .width(300.0)
            .selected_text(self.deck_name(current))
            .show_ui(ui, |ui| {
                for (deck, name) in &self.decks {
                    ui.selectable_value(&mut chosen, *deck, name);
                }
            });
        if mine {
            self.my_deck = chosen;
        } else {
            self.bot_deck = chosen;
        }
    }

    /// Discover signed nearby lobbies without blocking drawing.
    fn nearby_games(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Nearby games").strong());
        if self.nearby.is_none() && self.nearby_error.is_none() {
            match mtg_net::discovery::Browser::new() {
                Ok(browser) => self.nearby = Some(browser),
                Err(_) => {
                    self.nearby_error = Some(
                        "Nearby discovery is unavailable. You can still paste an invite link."
                            .into(),
                    )
                }
            }
        }
        if let Some(error) = &self.nearby_error {
            ui.label(error);
            if ui.button("Retry discovery").clicked() {
                self.nearby_error = None;
            }
        }
        if let Some(browser) = &mut self.nearby {
            browser.poll();
            let games: Vec<_> = browser.games().cloned().collect();
            if games.is_empty() {
                ui.label(RichText::new("Looking for games on your network…").weak());
            }
            egui::ScrollArea::vertical()
                .id_salt("nearby-games")
                .max_height(160.0)
                .show(ui, |ui| {
                    for game in games {
                        ui.push_id(game.invite.session.0, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(&game.name);
                                if ui.button("Join").clicked() {
                                    self.join_link = game.link;
                                    self.start(Mode::Join);
                                }
                            });
                        });
                    }
                });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Check setup, then start the match on a worker thread.
    fn start(&mut self, mode: Mode) {
        if mode == Mode::Host && self.host_internet && self.ngrok_token.trim().is_empty() {
            self.setup_error = Some("Paste your ngrok authtoken to host online.".into());
            return;
        }
        if mode == Mode::Host
            && let Some(store) = &self.store
        {
            let saved = if self.remember_ngrok {
                self.ngrok_token.trim()
            } else {
                ""
            };
            if let Err(error) = store
                .set_meta("ngrok_authtoken", saved)
                .and_then(|()| store.set_meta("ngrok_domain", self.ngrok_domain.trim()))
            {
                self.setup_error = Some(format!("Could not save hosting settings: {error}"));
                return;
            }
        }
        self.setup_error = None;
        let settings = MatchSettings {
            format: self.format,
            best_of: self.best_of,
        };
        let mine = match decks::deck_spec(self.store.as_ref(), self.my_deck) {
            Ok(d) => d,
            Err(e) => return self.setup_error = Some(e),
        };
        let bots = match mode {
            Mode::Bot => match decks::deck_spec(self.store.as_ref(), self.bot_deck) {
                Ok(d) => Some(d),
                Err(e) => return self.setup_error = Some(e),
            },
            _ => None,
        };
        if mode == Mode::Join && self.join_link.trim().is_empty() {
            return self.setup_error = Some("paste the invite link first".into());
        }

        // Legality is checked before anything starts, rather than after a connection is made.
        if mode != Mode::Join {
            let source = LocalSource::standard();
            let all: Vec<&DeckSpec> = std::iter::once(&mine).chain(&bots).collect();
            let cards = match GameCards::build(all.iter().copied(), &source) {
                Ok(c) => c,
                Err(missing) => {
                    return self.setup_error = Some(format!(
                        "{} cards are missing or have unsupported rules; open the deck builder for details",
                        missing.len()
                    ));
                }
            };
            for (whose, deck) in [("Your", Some(&mine)), ("The bot's", bots.as_ref())] {
                let Some(deck) = deck else { continue };
                let problems = deck_problems(settings.format, deck, &cards);
                if !problems.is_empty() {
                    let shown: Vec<_> = problems.into_iter().take(4).collect();
                    return self.setup_error = Some(format!(
                        "{whose} deck is not legal for {}: {}",
                        settings.format.name(),
                        shown.join("; ")
                    ));
                }
            }
        }
        let port: u16 = match self.host_port.trim().parse() {
            Ok(p) => p,
            Err(_) if mode == Mode::Host => {
                return self.setup_error = Some("the port must be a number".into());
            }
            Err(_) => 0,
        };
        self.launch(mode, settings, mine, bots, port);
    }

    /// Start a match on a worker thread and switch to the match screen.
    fn launch(
        &mut self,
        mode: Mode,
        settings: MatchSettings,
        mine: DeckSpec,
        other: Option<DeckSpec>,
        port: u16,
    ) {
        let bots = other;
        // The seat the worker plays through, and the window's ends of its channels.
        let (mut decider, questions, answers) = UiDecider::new();
        let views = decider.watch();
        let inner = PolicyThenUi::new(mtg_policy::Policy::default(), decider);
        let auto = Arc::clone(&inner.auto_answered);
        let (events_tx, events) = mpsc::channel();
        let (sideboards, sideboard_rx) = mpsc::channel();
        let left = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let mut seat = UiSeat {
            inner,
            events: events_tx.clone(),
            sideboards: sideboard_rx,
            left: Arc::clone(&left),
        };

        let art = self
            .art
            .take()
            .unwrap_or_else(|| CardArt::start(self.ctx.clone(), ArtConfig::standard()));
        let mut gui = GuiApp::new(questions, answers, CardTexts::default(), art, auto);
        gui.watch(views);

        let link = self.join_link.clone();
        let players = self.players;
        let address = self.host_address.clone();
        let tunnel_token = self.host_internet.then(|| self.ngrok_token.clone());
        let host_name = self.host_name.clone();
        let tunnel_domain = Some(self.ngrok_domain.clone());
        self.nearby = None;
        let worker_cancel = Arc::clone(&cancel);
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("match".into())
            .spawn(move || {
                let result = match mode {
                    Mode::Bot => local_match(
                        settings,
                        mine,
                        bots.unwrap_or_default(),
                        &mut seat,
                        &mut BotSeat::default(),
                        players,
                    ),
                    Mode::Playtest => local_match(
                        settings,
                        mine,
                        decks::goldfish_deck(),
                        &mut seat,
                        &mut crate::seat::GoldfishSeat,
                        2,
                    ),
                    Mode::Host => net::identity().and_then(|identity| {
                        let source = LocalSource::standard();
                        let config = net::HostConfig {
                            settings,
                            players,
                            deck: mine,
                            port,
                            public_address: Some(address),
                            tunnel_token,
                            tunnel_domain,
                            name: host_name,
                            identity: &identity,
                            source: &source,
                        };
                        net::host(config, &mut seat, &events_tx, &worker_cancel)
                    }),
                    Mode::Join => net::identity().and_then(|identity| {
                        net::join_with_cancel(
                            &link,
                            mine,
                            &identity,
                            &mut seat,
                            &events_tx,
                            &worker_cancel,
                        )
                    }),
                };
                let _ = events_tx.send(MatchEvent::MatchEnded(result));
                ctx.request_repaint();
            })
            .expect("spawn the match thread");

        self.running = Some(Running {
            mode,
            gui: Some(gui),
            events,
            sideboards,
            left,
            cancel,
            status: VecDeque::new(),
            invite: None,
            settings: (mode != Mode::Join).then_some(settings),
            game: None,
            last_game: None,
            sideboard: None,
            ended: None,
        });
        self.screen = Screen::Match;
    }
}

fn score_label(score: &[u8], you: mtg_core::PlayerId) -> String {
    score
        .iter()
        .enumerate()
        .map(|(i, wins)| {
            let name = if i == you.0 as usize {
                "you".into()
            } else {
                format!("player {}", i + 1)
            };
            format!("{name} {wins}")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A match against the bot, on the worker thread.
fn local_match(
    settings: MatchSettings,
    mine: DeckSpec,
    bots: DeckSpec,
    seat: &mut UiSeat,
    bot: &mut dyn mtg_session::matches::Seat,
    players: u8,
) -> Result<MatchEnd, String> {
    let source = LocalSource::standard();
    let cards = GameCards::build([&mine, &bots], &source).map_err(|missing| {
        format!(
            "{} cards are missing or have unsupported rules; open the deck builder for details",
            missing.len()
        )
    })?;
    let mut decks = vec![mine];
    decks.extend(std::iter::repeat_n(bots, players as usize - 1));
    let mut extra_bots: Vec<_> = (2..players).map(|_| BotSeat::default()).collect();
    let mut seats: Vec<&mut dyn mtg_session::matches::Seat> = vec![seat, bot];
    seats.extend(
        extra_bots
            .iter_mut()
            .map(|bot| bot as &mut dyn mtg_session::matches::Seat),
    );
    Ok(play_local_match_with_players(
        settings,
        &cards,
        &mut decks,
        Seed::random(),
        &mut seats,
    ))
}

// ---- the match ---------------------------------------------------------------

impl App {
    fn match_screen(&mut self, ui: &mut Ui) {
        let Some(run) = self.running.as_mut() else {
            self.screen = Screen::Menu;
            return;
        };
        if drain_events(run) {
            ui.ctx().request_repaint();
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));

        let mut leave = false;
        egui::Panel::top("match-bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(s) = run.settings {
                    ui.label(RichText::new(s.format.name()).strong());
                    ui.label(format!("best of {}", s.best_of));
                }
                if let Some(g) = &run.game {
                    let score = run.last_game.as_ref().map_or(&g.score, |end| &end.score);
                    ui.label(format!("game {}", g.number));
                    ui.label(RichText::new(score_label(score, g.you)).strong());
                    if g.first == g.you {
                        ui.label(RichText::new("you play first").weak());
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if run.ended.is_some() {
                        "Back to menu"
                    } else {
                        "Leave match"
                    };
                    if ui.button(text).clicked() {
                        leave = true;
                    }
                });
            });
        });

        if run.game.is_none() {
            waiting(ui, run);
        } else if let Some(gui) = run.gui.as_mut() {
            gui.show(ui);
        }

        let ctx = ui.ctx().clone();
        if let Some(end) = run
            .last_game
            .as_ref()
            .filter(|_| run.ended.is_none() && run.sideboard.is_none())
        {
            let you = run.game.as_ref().map_or(mtg_core::PlayerId(0), |g| g.you);
            let verdict = match end.winner {
                Some(w) if w == you => "You won",
                Some(_) => "You lost",
                None => "A draw",
            };
            egui::Window::new(format!("Game {}", end.number))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                .show(&ctx, |ui| {
                    ui.label(RichText::new(verdict).size(20.0).strong());
                    ui.label("the next game starts in a moment");
                });
        }

        if let Some((deck, names)) = run.sideboard.as_mut() {
            let mut done = false;
            egui::Window::new("Sideboarding")
                .collapsible(false)
                .default_width(560.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    sideboard_editor(ui, deck, names);
                    ui.separator();
                    let valid = mtg_session::matches::sideboard_sizes_valid(deck);
                    if !valid {
                        ui.label(
                            RichText::new(
                                "Main deck needs at least 60 cards; sideboard allows at most 15.",
                            )
                            .color(WARN),
                        );
                    }
                    if ui
                        .add_enabled(
                            valid,
                            egui::Button::new(RichText::new("Done — play the next game").strong()),
                        )
                        .clicked()
                    {
                        done = true;
                    }
                });
            if done {
                let (deck, _) = run.sideboard.take().expect("present");
                let _ = run.sideboards.send(deck);
            }
        }

        if let Some(ended) = &run.ended {
            egui::Window::new("Match over")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    match ended {
                        Ok(end) => {
                            let you = run.game.as_ref().map_or(mtg_core::PlayerId(0), |g| g.you);
                            let verdict = match end.winner {
                                Some(w) if w == you => "You won the match",
                                Some(_) => "You lost the match",
                                None => "No result",
                            };
                            ui.label(RichText::new(verdict).size(22.0).strong());
                            ui.label(score_label(&end.score, you));
                            if let Some(reason) = &end.reason {
                                ui.label(RichText::new(reason).weak());
                            }
                        }
                        Err(e) => {
                            ui.label(
                                RichText::new("The match could not continue")
                                    .size(18.0)
                                    .strong(),
                            );
                            ui.label(RichText::new(e).color(WARN));
                        }
                    }
                    if ui.button("Back to menu").clicked() {
                        leave = true;
                    }
                });
        }

        if leave {
            self.leave_match();
        }
    }

    /// Leave the match. The worker concedes on the player's behalf and winds down on its own;
    /// the window goes back to the menu at once.
    fn leave_match(&mut self) {
        if let Some(mut run) = self.running.take() {
            run.left.store(true, Ordering::Relaxed);
            run.cancel.store(true, Ordering::Relaxed);
            if let Some((deck, _)) = run.sideboard.take() {
                let _ = run.sideboards.send(deck);
            }
            if let Some(gui) = run.gui.take() {
                self.art = Some(gui.into_art());
            }
            // A playtest came from the deck builder, so that is where it goes back to.
            self.screen = if run.mode == Mode::Playtest {
                Screen::Decks
            } else {
                Screen::Menu
            };
        } else {
            self.screen = Screen::Menu;
        }
        self.refresh();
    }
}

/// Retain recent progress without letting repeated connection failures grow the UI history.
const MAX_STATUS_MESSAGES: usize = 100;
/// Leave time to draw and handle input even when the worker has a burst of events.
const MAX_EVENTS_PER_FRAME: usize = 128;

/// Returns true when another frame should promptly continue draining the queue.
fn drain_events(run: &mut Running) -> bool {
    for _ in 0..MAX_EVENTS_PER_FRAME {
        let event = match run.events.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => {
                if run.ended.is_none() {
                    run.ended = Some(Err("The match worker stopped unexpectedly.".into()));
                    run.sideboard = None;
                }
                return false;
            }
        };
        match event {
            MatchEvent::Status(s) => {
                if run.status.len() == MAX_STATUS_MESSAGES {
                    run.status.pop_front();
                }
                run.status.push_back(s);
            }
            MatchEvent::Invite(link) => run.invite = Some(link),
            MatchEvent::MatchStarted { settings, texts } => {
                run.settings = Some(settings);
                if let Some(gui) = run.gui.as_mut() {
                    gui.set_texts(texts);
                }
            }
            MatchEvent::GameStarted(start) => {
                run.game = Some(start);
                run.last_game = None;
            }
            MatchEvent::GameEnded(end) => run.last_game = Some(end),
            MatchEvent::Sideboard { deck, names } => run.sideboard = Some((deck, names)),
            MatchEvent::MatchEnded(result) => {
                run.ended = Some(result);
                run.sideboard = None;
            }
        }
    }
    true
}

/// Before the first game: connection progress, and for a host the link to send.
fn waiting(ui: &mut Ui, run: &mut Running) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.add_space(24.0);
        ui.heading(match run.mode {
            Mode::Bot | Mode::Playtest => "Starting…",
            Mode::Host => "Hosting",
            Mode::Join => "Joining",
        });
        ui.add_space(8.0);

        if let Some(link) = run.invite.clone() {
            ui.label("Send this link to your opponent. It works for one hour and one guest.");
            ui.horizontal(|ui| {
                let mut shown = link.clone();
                ui.add(egui::TextEdit::singleline(&mut shown).desired_width(520.0));
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(link);
                }
            });
            ui.label(
                RichText::new(
                    "Nearby players can find your game automatically. Online friends can join this invite when internet hosting is enabled.",
                )
                .weak(),
            );
            ui.add_space(8.0);
        }

        egui::ScrollArea::vertical()
            .id_salt("connection-status")
            .max_height(240.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &run.status {
                    ui.label(line);
                }
            });
        if run.ended.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("waiting").weak());
            });
        }
    });
}

/// Move cards between main deck and sideboard, one copy per click.
fn sideboard_editor(ui: &mut Ui, deck: &mut DeckSpec, names: &BTreeMap<CardKey, String>) {
    let name = |k: &CardKey| names.get(k).cloned().unwrap_or_else(|| format!("{k:?}"));
    let side: u32 = deck.side.iter().map(|(_, n)| u32::from(*n)).sum();
    ui.label(format!(
        "Main deck {} · sideboard {side}",
        deck.main_count()
    ));

    let mut to_side = None;
    let mut to_main = None;
    ui.columns(2, |cols| {
        cols[0].label(RichText::new("Main deck").strong());
        egui::ScrollArea::vertical()
            .id_salt("main")
            .max_height(320.0)
            .show(&mut cols[0], |ui| {
                for (k, n) in &deck.main {
                    ui.horizontal(|ui| {
                        let card_name = name(k);
                        ui.label(format!("{n}× {card_name}")).on_hover_ui(|ui| {
                            crate::widgets::offer_card_enlargement(ui, &card_name)
                        });
                        if ui
                            .small_button("⏵")
                            .on_hover_text("move one to the sideboard")
                            .clicked()
                        {
                            to_side = Some(k.clone());
                        }
                    });
                }
            });
        cols[1].label(RichText::new("Sideboard").strong());
        egui::ScrollArea::vertical()
            .id_salt("side")
            .max_height(320.0)
            .show(&mut cols[1], |ui| {
                for (k, n) in &deck.side {
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("⏴")
                            .on_hover_text("move one to the main deck")
                            .clicked()
                        {
                            to_main = Some(k.clone());
                        }
                        let card_name = name(k);
                        ui.label(format!("{n}× {card_name}")).on_hover_ui(|ui| {
                            crate::widgets::offer_card_enlargement(ui, &card_name)
                        });
                    });
                }
            });
    });

    if let Some(k) = to_side {
        move_sideboard_card(&mut deck.main, &mut deck.side, k);
    }
    if let Some(k) = to_main {
        move_sideboard_card(&mut deck.side, &mut deck.main, k);
    }
}

/// Follow the edited deck even when a save changes both its name and ID.
fn deck_choice_after_save(choice: DeckId, previous_id: Option<i64>, saved_id: i64) -> DeckId {
    if previous_id.is_some_and(|id| choice == DeckId::Stored(id)) {
        DeckId::Stored(saved_id)
    } else {
        choice
    }
}

/// Keep saved choices across same-name replacements that allocate a new ID.
fn retained_deck_choice(
    id: DeckId,
    previous: &[(DeckId, String)],
    current: &[(DeckId, String)],
) -> Option<DeckId> {
    if current.iter().any(|(candidate, _)| *candidate == id) {
        return Some(id);
    }
    let DeckId::Stored(_) = id else { return None };
    let (_, name) = previous.iter().find(|(candidate, _)| *candidate == id)?;
    current.iter().find_map(|(candidate, current_name)| {
        (matches!(candidate, DeckId::Stored(_)) && current_name == name).then_some(*candidate)
    })
}

/// Transfer one copy without overflowing a counted entry or consuming a zero entry.
fn move_sideboard_card(
    from: &mut Vec<(CardKey, u8)>,
    to: &mut Vec<(CardKey, u8)>,
    key: CardKey,
) -> bool {
    let Some(i) = from.iter().position(|(k, count)| *k == key && *count > 0) else {
        return false;
    };
    from[i].1 -= 1;
    if from[i].1 == 0 {
        from.remove(i);
    }
    match to
        .iter_mut()
        .find(|(k, count)| *k == key && *count < u8::MAX)
    {
        Some(entry) => entry.1 += 1,
        None => to.push((key, 1)),
    }
    true
}

pub(crate) fn read_deck_file(file: &dyn egui::DroppedFile) -> Result<String, String> {
    const LIMIT: usize = 2 * 1024 * 1024;
    let file =
        std::fs::File::open(file.path()).map_err(|e| format!("Could not open deck file: {e}"))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, LIMIT as u64 + 1), &mut bytes)
        .map_err(|e| format!("Could not read deck file: {e}"))?;
    if bytes.len() > LIMIT {
        return Err("Deck file exceeds 2 MiB.".into());
    }
    String::from_utf8(bytes).map_err(|_| "Deck files must contain UTF-8 text.".into())
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn catalog_becomes_available_before_image_population_finishes() {
        let (updates, receiver) = mpsc::channel();
        updates.send(DatabaseUpdate::CatalogReady).unwrap();
        updates
            .send(DatabaseUpdate::Progress("Preparing images".into()))
            .unwrap();
        let mut status = None;
        assert!(matches!(
            drain_database_updates(&receiver, &mut status),
            DatabasePoll::CatalogReady
        ));
        assert!(matches!(
            drain_database_updates(&receiver, &mut status),
            DatabasePoll::Pending { backlog: false }
        ));
        assert_eq!(status.as_deref(), Some("Preparing images"));
    }

    #[test]
    fn database_progress_is_bounded_without_losing_a_queued_result() {
        let (updates, receiver) = mpsc::channel();
        for i in 0..MAX_EVENTS_PER_FRAME + 1 {
            updates
                .send(DatabaseUpdate::Progress(format!("row {i}")))
                .unwrap();
        }
        updates
            .send(DatabaseUpdate::Finished(Ok("complete".into())))
            .unwrap();
        drop(updates);
        let mut status = None;
        assert!(matches!(
            drain_database_updates(&receiver, &mut status),
            DatabasePoll::Pending { backlog: true }
        ));
        assert_eq!(status, Some(format!("row {}", MAX_EVENTS_PER_FRAME - 1)));
        assert!(matches!(drain_database_updates(&receiver, &mut status),
            DatabasePoll::Finished(Ok(result)) if result == "complete"));
        assert_eq!(status, Some(format!("row {}", MAX_EVENTS_PER_FRAME)));
    }

    #[test]
    fn database_progress_distinguishes_waiting_from_worker_shutdown() {
        let (updates, receiver) = mpsc::channel();
        let mut status = None;
        assert!(matches!(
            drain_database_updates(&receiver, &mut status),
            DatabasePoll::Pending { backlog: false }
        ));
        updates
            .send(DatabaseUpdate::Progress("loading".into()))
            .unwrap();
        drop(updates);
        assert!(matches!(
            drain_database_updates(&receiver, &mut status),
            DatabasePoll::Stopped
        ));
        assert_eq!(status.as_deref(), Some("loading"));
    }

    #[test]
    fn renamed_saves_remap_only_choices_using_the_previous_deck() {
        assert_eq!(
            deck_choice_after_save(DeckId::Stored(1), Some(1), 9),
            DeckId::Stored(9)
        );
        assert_eq!(
            deck_choice_after_save(DeckId::Stored(2), Some(1), 9),
            DeckId::Stored(2)
        );
        assert_eq!(
            deck_choice_after_save(DeckId::Demo, Some(1), 9),
            DeckId::Demo
        );
        assert_eq!(
            deck_choice_after_save(DeckId::Stored(1), None, 9),
            DeckId::Stored(1)
        );
        assert_eq!(
            deck_choice_after_save(DeckId::Stored(1), Some(1), 1),
            DeckId::Stored(1)
        );
    }

    #[test]
    fn deck_choices_follow_replacements_but_do_not_select_unrelated_decks() {
        let previous = vec![
            (DeckId::Demo, "Demo".into()),
            (DeckId::Stored(1), "My deck".into()),
        ];
        let current = vec![
            (DeckId::Demo, "Demo".into()),
            (DeckId::Stored(9), "My deck".into()),
        ];
        assert_eq!(
            retained_deck_choice(DeckId::Stored(1), &previous, &current),
            Some(DeckId::Stored(9))
        );
        assert_eq!(
            retained_deck_choice(DeckId::Demo, &previous, &current),
            Some(DeckId::Demo)
        );
        assert_eq!(
            retained_deck_choice(DeckId::Stored(7), &previous, &current),
            None
        );
        let renamed = vec![(DeckId::Stored(1), "Renamed".into())];
        assert_eq!(
            retained_deck_choice(DeckId::Stored(1), &previous, &renamed),
            Some(DeckId::Stored(1))
        );
        let deleted = vec![
            (DeckId::Demo, "My deck".into()),
            (DeckId::Stored(8), "Other deck".into()),
        ];
        assert_eq!(
            retained_deck_choice(DeckId::Stored(1), &previous, &deleted),
            None
        );
    }

    #[test]
    fn sideboard_transfers_preserve_copies_at_count_boundaries() {
        let key = CardKey::Demo(0);
        let mut deck = DeckSpec {
            main: vec![(key.clone(), 255)],
            side: vec![(key.clone(), 1)],
            ..Default::default()
        };
        let original = deck.clone();
        assert!(move_sideboard_card(
            &mut deck.side,
            &mut deck.main,
            key.clone()
        ));
        assert_eq!(deck.main, vec![(key.clone(), 255), (key.clone(), 1)]);
        assert!(deck.side.is_empty());
        assert!(deck.same_cards_as(&original));
        assert!(move_sideboard_card(
            &mut deck.main,
            &mut deck.side,
            key.clone()
        ));
        assert!(deck.same_cards_as(&original));

        let mut from = vec![(key.clone(), 0), (key.clone(), 1)];
        let mut to = Vec::new();
        assert!(move_sideboard_card(&mut from, &mut to, key.clone()));
        assert_eq!(from, vec![(key.clone(), 0)]);
        assert_eq!(to, vec![(key.clone(), 1)]);
        assert!(!move_sideboard_card(&mut from, &mut to, key));
        assert_eq!(to.len(), 1);
    }

    fn running(events: Receiver<MatchEvent>) -> Running {
        let (sideboards, _receiver) = mpsc::channel();
        Running {
            mode: Mode::Host,
            gui: None,
            events,
            sideboards,
            left: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            status: VecDeque::new(),
            invite: None,
            settings: None,
            game: None,
            last_game: None,
            sideboard: Some((DeckSpec::default(), BTreeMap::new())),
            ended: None,
        }
    }

    #[test]
    fn a_worker_that_stops_without_a_result_does_not_leave_the_ui_waiting() {
        let (events, receiver) = mpsc::channel();
        events
            .send(MatchEvent::Status("connecting".into()))
            .unwrap();
        drop(events);
        let mut run = running(receiver);
        assert!(!drain_events(&mut run));
        assert!(matches!(run.ended, Some(Err(ref error))
            if error.contains("stopped unexpectedly")));
        assert!(run.sideboard.is_none());
        assert_eq!(run.status.back().map(String::as_str), Some("connecting"));
    }

    #[test]
    fn a_final_event_behind_a_large_batch_survives_worker_shutdown() {
        let (events, receiver) = mpsc::channel();
        for i in 0..MAX_EVENTS_PER_FRAME + 1 {
            events
                .send(MatchEvent::Status(format!("attempt {i}")))
                .unwrap();
        }
        events
            .send(MatchEvent::MatchEnded(Err("peer disconnected".into())))
            .unwrap();
        drop(events);
        let mut run = running(receiver);
        assert!(drain_events(&mut run));
        assert!(run.ended.is_none(), "queued final event must be read first");
        assert!(!drain_events(&mut run));
        assert!(matches!(run.ended, Some(Err(ref error)) if error == "peer disconnected"));
        assert!(run.sideboard.is_none());
        assert_eq!(run.status.len(), MAX_STATUS_MESSAGES);
        assert_eq!(
            run.status.back().unwrap(),
            &format!("attempt {MAX_EVENTS_PER_FRAME}")
        );
    }
}
