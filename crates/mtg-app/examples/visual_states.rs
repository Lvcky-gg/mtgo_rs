//! Capture the real arena renderer in deterministic edge-case positions.
//! Run: cargo run -p mtg-app --example visual_states -- /tmp/mtgo-visual
//! These fixtures test presentation; they do not simulate rules transitions.

use mtg_app::{
    Question,
    art::{ArtConfig, CardArt},
    cards_text::CardTexts,
    ui::GuiApp,
};
use mtg_core::{PlayerId, Step, Target, Zone, ZoneRef};
use mtg_engine::{Choice, ChoiceKind, PlayerView, choice::Answer, state::GameState};
use mtg_headless::cards::{
    ADDER, BANNER, COLOSSUS, DUAL, DUMMY, DemoCards, PLAINS, SENTRY, SERGEANT,
};
use std::{path::PathBuf, sync::mpsc, time::Instant};

struct Fixture {
    before: Option<PlayerView>,
    name: &'static str,
    view: PlayerView,
    kind: Option<ChoiceKind>,
    default: Option<Answer>,
}

fn fixtures() -> Vec<Fixture> {
    let me = PlayerId(0);
    let opponent = PlayerId(1);
    let mut state = GameState::new(&[me, opponent], 20);
    state.turn = 1;
    state.step = Step::PrecombatMain;
    state.priority = Some(me);
    let hand: Vec<_> = [PLAINS, DUAL, DUMMY, SENTRY, SERGEANT, COLOSSUS, BANNER]
        .into_iter()
        .map(|card| state.place(card, me, ZoneRef::of(Zone::Hand, me)))
        .collect();
    for _ in 0..7 {
        state.place(ADDER, opponent, ZoneRef::of(Zone::Hand, opponent));
    }
    for player in [me, opponent] {
        for _ in 0..25 {
            state.place(PLAINS, player, ZoneRef::of(Zone::Library, player));
        }
    }
    let opening = mtg_engine::view::project(&state, me);
    let mut result = vec![
        Fixture {
            before: None,
            name: "01-mulligan",
            view: opening.clone(),
            kind: Some(ChoiceKind::KeepOrMulligan { mulligans_taken: 3 }),
            default: None,
        },
        Fixture {
            before: None,
            name: "02-bottom-three",
            view: opening,
            kind: Some(ChoiceKind::ChooseObjects {
                from: hand.clone(),
                min: 3,
                max: 3,
            }),
            default: None,
        },
    ];
    state.place(DUMMY, me, ZoneRef::of(Zone::Hand, me));
    state.place(ADDER, me, ZoneRef::of(Zone::Hand, me));
    state.step = Step::Cleanup;
    state.priority = None;
    result.push(Fixture {
        before: None,
        name: "03-cleanup-discard",
        view: mtg_engine::view::project(&state, me),
        kind: Some(ChoiceKind::DiscardToHandSize { count: 2 }),
        default: None,
    });

    let attacker = state.place(COLOSSUS, me, ZoneRef::shared(Zone::Battlefield));
    let first = state.place(SENTRY, opponent, ZoneRef::shared(Zone::Battlefield));
    let second = state.place(SERGEANT, opponent, ZoneRef::shared(Zone::Battlefield));
    state.step = Step::CombatDamage;
    let mut combat = mtg_engine::view::project(&state, me);
    let object = combat.visible.get_mut(&attacker).unwrap();
    object.attacking = true;
    object.attacking_target = Some(Target::Player(opponent));
    object.tapped = true;
    for blocker in [first, second] {
        combat.visible.get_mut(&blocker).unwrap().blocking = Some(attacker);
    }
    result.push(Fixture {
        before: None,
        name: "04-trample-damage",
        view: combat,
        kind: Some(ChoiceKind::AssignCombatDamage {
            attacker,
            among: vec![first, second],
            total: 5,
        }),
        default: Some(Answer::DamageAssignment(vec![(first, 3), (second, 1)])),
    });

    state.step = Step::PrecombatMain;
    state.priority = Some(me);
    let targets = vec![
        Target::Player(me),
        Target::Player(opponent),
        Target::Object(attacker),
        Target::Object(first),
    ];
    let mut protected = mtg_engine::view::project(&state, me);
    protected.prevent_combat_damage = true;
    protected.prevent_damage_to = vec![Target::Player(me), Target::Object(attacker)];
    result.push(Fixture {
        before: None,
        name: "05-targets-and-prevention",
        view: protected,
        kind: Some(ChoiceKind::ChooseTargets {
            slots: vec![targets.clone(), targets],
        }),
        default: None,
    });
    result.push(Fixture {
        before: None,
        name: "06-optional-modes",
        view: mtg_engine::view::project(&state, me),
        kind: Some(ChoiceKind::ChooseModes {
            available: vec!["Draw a card".into(), "Gain 3 life".into()],
            count: 2,
            min: Some(0),
        }),
        default: None,
    });

    for player in [me, opponent] {
        for card in [
            PLAINS, DUAL, DUMMY, SENTRY, SERGEANT, ADDER, COLOSSUS, BANNER,
        ] {
            for _ in 0..4 {
                state.place(card, player, ZoneRef::shared(Zone::Battlefield));
            }
        }
        state.place(DUMMY, player, ZoneRef::of(Zone::Graveyard, player));
        state.place(ADDER, player, ZoneRef::shared(Zone::Exile));
    }
    state.players.get_mut(&me).unwrap().life = 1;
    state.players.get_mut(&opponent).unwrap().poison = 9;
    let mut crowded = mtg_engine::view::project(&state, me);
    crowded
        .players
        .get_mut(&me)
        .unwrap()
        .commander_damage
        .insert(opponent, 20);
    let stack_a = state.place(BANNER, me, ZoneRef::shared(Zone::Stack));
    let stack_b = state.place(DUMMY, opponent, ZoneRef::shared(Zone::Stack));
    let mut stacked = mtg_engine::view::project(&state, me);
    stacked.stack = vec![stack_a, stack_b];
    result.push(Fixture {
        before: None,
        name: "07-stack",
        view: stacked,
        kind: Some(ChoiceKind::Priority {
            legal: Default::default(),
        }),
        default: Some(Answer::Pass),
    });
    result.push(Fixture {
        before: None,
        name: "08-crowded-board",
        view: crowded.clone(),
        kind: Some(ChoiceKind::Priority {
            legal: Default::default(),
        }),
        default: Some(Answer::Pass),
    });
    result.push(Fixture {
        before: None,
        name: "09-game-ended",
        view: crowded,
        kind: None,
        default: None,
    });
    let mut fetched_state = GameState::new(&[me, opponent], 20);
    fetched_state.turn = 1;
    fetched_state.step = Step::PrecombatMain;
    fetched_state.priority = Some(me);
    for _ in 0..6 {
        fetched_state.place(PLAINS, me, ZoneRef::shared(Zone::Battlefield));
    }
    let before = mtg_engine::view::project(&fetched_state, me);
    fetched_state.place(DUAL, me, ZoneRef::shared(Zone::Battlefield));
    result.push(Fixture {
        before: Some(before),
        name: "10-fetched-land",
        view: mtg_engine::view::project(&fetched_state, me),
        kind: Some(ChoiceKind::Priority {
            legal: Default::default(),
        }),
        default: Some(Answer::Pass),
    });
    let candidates = state.objects_in(ZoneRef::of(Zone::Library, me));
    state
        .objects
        .get_mut(candidates.last().unwrap())
        .unwrap()
        .card = COLOSSUS;
    result.push(Fixture {
        before: None,
        name: "11-tutor-library-selection",
        view: mtg_engine::view::project_showing(&state, me, &candidates),
        kind: Some(ChoiceKind::ChooseObjects {
            from: candidates,
            min: 1,
            max: 1,
        }),
        default: None,
    });
    let before = mtg_engine::view::project(&state, me);
    state.place(COLOSSUS, me, ZoneRef::of(Zone::Hand, me));
    result.push(Fixture {
        before: Some(before),
        name: "12-tutored-card-in-hand",
        view: mtg_engine::view::project(&state, me),
        kind: Some(ChoiceKind::Priority {
            legal: Default::default(),
        }),
        default: Some(Answer::Pass),
    });
    result
}

struct CaptureApp {
    fixtures: Vec<Fixture>,
    index: usize,
    arena: GuiApp,
    ctx: egui::Context,
    output: PathBuf,
    since: Instant,
    requested: bool,
    scrolled: bool,
    scroll_sent: bool,
    // Keep channels alive until deliberately presenting the finished state.
    questions: Option<mpsc::Sender<Question>>,
    views: mpsc::Sender<PlayerView>,
    _answers: mpsc::Receiver<Answer>,
}

impl CaptureApp {
    fn new(ctx: egui::Context, output: PathBuf) -> Self {
        mtg_app::theme::install(&ctx);
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("arena-audio-settings"), (true, 0.0_f32))
        });
        let (questions, rx) = mpsc::channel();
        let (answers, received) = mpsc::channel();
        let (views, observed) = mpsc::channel();
        let cards = DemoCards::default();
        let art = CardArt::start(
            ctx.clone(),
            ArtConfig {
                names: Default::default(),
                db: None,
                cache_dir: output.join("cache"),
            },
        );
        let mut arena = GuiApp::new(
            rx,
            answers,
            CardTexts::snapshot(
                &cards,
                [
                    PLAINS, DUAL, DUMMY, SENTRY, SERGEANT, ADDER, COLOSSUS, BANNER,
                ],
            ),
            art,
            Default::default(),
        );
        arena.watch(observed);
        let mut app = Self {
            fixtures: fixtures(),
            index: 0,
            arena,
            ctx,
            output,
            since: Instant::now(),
            requested: false,
            scrolled: false,
            scroll_sent: false,
            questions: Some(questions),
            views,
            _answers: received,
        };
        app.load();
        app
    }

    fn load(&mut self) {
        let fixture = &self.fixtures[self.index];
        // Start a fresh arena to clear the preceding unanswered question.
        let (tx, rx) = mpsc::channel();
        let (answers, received) = mpsc::channel();
        let (views, observed) = mpsc::channel();
        let cards = DemoCards::default();
        let art = CardArt::start(
            self.ctx.clone(),
            ArtConfig {
                names: Default::default(),
                db: None,
                cache_dir: self.output.join("cache"),
            },
        );
        self.arena = GuiApp::new(
            rx,
            answers,
            CardTexts::snapshot(
                &cards,
                [
                    PLAINS, DUAL, DUMMY, SENTRY, SERGEANT, ADDER, COLOSSUS, BANNER,
                ],
            ),
            art,
            Default::default(),
        );
        self.arena.watch(observed);
        if let Some(before) = &fixture.before {
            views.send(before.clone()).unwrap();
        }
        views.send(fixture.view.clone()).unwrap();
        if let Some(kind) = &fixture.kind {
            tx.send(Question {
                view: fixture.view.clone(),
                choice: Choice {
                    id: self.index as u64 + 1,
                    who: fixture.view.viewer,
                    kind: kind.clone(),
                    because: "Visual validation fixture".into(),
                    default: fixture.default.clone(),
                    undo: false,
                },
            })
            .unwrap();
            self.questions = Some(tx);
        } else {
            self.questions = None;
        }
        self.views = views;
        self._answers = received;
        self.since = Instant::now();
        self.requested = false;
        self.scrolled = false;
        self.scroll_sent = false;
    }
}

impl eframe::App for CaptureApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.scrolled && !self.scroll_sent {
            let rect = ctx.content_rect();
            let pointer_y = if self.index == 10 {
                rect.bottom() - 270.0
            } else {
                rect.bottom() - 60.0
            };
            input.events.extend([
                egui::Event::PointerMoved(egui::pos2(rect.left() + 300.0, pointer_y)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-20000.0, -20000.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.scroll_sent = true;
        } else if self.scroll_sent && self.index == 10 {
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(5.0, 5.0)));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let screenshot = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(pixels) = screenshot {
            let suffix = if self.scrolled { "-scrolled" } else { "" };
            let path = self
                .output
                .join(format!("{}{suffix}.png", self.fixtures[self.index].name));
            let rgba: Vec<u8> = pixels
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                &path,
                &rgba,
                pixels.size[0] as u32,
                pixels.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("{}", path.display());
            if !self.scrolled && (self.index <= 5 || self.index >= 10) {
                self.scrolled = true;
                self.requested = false;
                self.since = Instant::now();
                return;
            }
            self.index += 1;
            if self.index == self.fixtures.len() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.load();
        }
        self.arena.show(ui);
        let settle_ms = if self.index == 0 && !self.scrolled {
            5000
        } else {
            800
        };
        if !self.requested && self.since.elapsed().as_millis() > settle_ms {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.requested = true;
        }
    }
}

fn main() -> eframe::Result {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("mtgo-visual"));
    std::fs::create_dir_all(&output).expect("create screenshot directory");
    eframe::run_native(
        "mtgo visual validation",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_app_id("mtgo-visual-validation")
                .with_inner_size([1280.0, 860.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(CaptureApp::new(cc.egui_ctx.clone(), output)))),
    )
}
