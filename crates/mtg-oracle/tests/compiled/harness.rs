//! A table of cards built from printed text, and a driver that plays them.
//!
//! Cards go through exactly the path an imported card takes — `convert::to_engine_face`,
//! which runs the rules-text compiler — so a test here proves the whole chain: the text
//! compiles, and the engine then does what the text says. All card names are invented.

#![allow(dead_code)]

use mtg_core::{CardId, ObjectId, PlayerId, Step, Subtype, Target, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, Choice, ChoiceKind},
    state::GameState,
};
use mtg_ir::{CardFace, PrintedCards};
use mtg_oracle::convert::{Subtypes, to_engine_face};
use mtg_store::FaceRow;

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);

const SUBTYPES: &[&str] = &[
    "",
    "Aura",
    "Equipment",
    "Mountain",
    "Forest",
    "Plains",
    "Island",
    "Swamp",
    "Elf",
    "Goblin",
    "Bear",
    "Wizard",
    "Soldier",
    "Swamp2",
    "Food",
    "Clue",
    "Zombie",
    "Spirit",
    "Thopter",
    "Treasure",
    "Vehicle",
    "Shapeshifter",
    "Eldrazi",
    "Scion",
    "Servo",
    "Phyrexian",
    "Germ",
    "Ally",
    "Mercenary",
    "Saga",
    "Army",
    "Blood",
    "Warrior",
    "Wall",
    "Powerstone",
    "Elemental",
    "Rebel",
    "Omen",
    "Dragon",
    "Gate",
    "Desert",
    "Merfolk",
    "Tyranid",
    "Gargoyle",
    "Hero",
    "Monk",
    "Dinosaur",
    "Kithkin",
    "Golem",
];

#[derive(Default)]
pub struct Table {
    faces: Vec<CardFace>,
    tokens: Vec<CardFace>,
    backs: std::collections::BTreeMap<CardId, CardFace>,
    layouts: std::collections::BTreeMap<CardId, mtg_ir::Layout>,
    /// Which cards were compiled without leftovers, so a test can insist on it.
    pub understood: Vec<bool>,
}

struct Names;
impl Subtypes for Names {
    fn intern(&self, name: &str) -> Option<u16> {
        SUBTYPES.iter().position(|n| *n == name).map(|i| i as u16)
    }
}

impl Table {
    /// Add a card from its printed text. Panics if the compiler did not understand every
    /// line: these tests are about cards that claim to be playable.
    #[track_caller]
    pub fn card(
        &mut self,
        cost: &str,
        type_line: &str,
        pt: Option<(i32, i32)>,
        text: &str,
    ) -> CardId {
        let name = format!("Card {}", self.faces.len());
        self.named_card(&name, cost, type_line, pt, text)
    }

    /// Compile a fixture with an explicit name for name-dependent searches.
    #[track_caller]
    pub fn named_card(
        &mut self,
        name: &str,
        cost: &str,
        type_line: &str,
        pt: Option<(i32, i32)>,
        text: &str,
    ) -> CardId {
        let name = name.to_owned();
        let row = FaceRow {
            name: name.clone(),
            mana_cost: cost.into(),
            type_line: type_line.into(),
            oracle_text: (!text.is_empty()).then(|| text.replace("~", &name)),
            power: pt.map(|p| p.0.to_string()),
            toughness: pt.map(|p| p.1.to_string()),
            ..Default::default()
        };
        let types = mtg_oracle::typeline::parse(&row.type_line);
        let compiled = mtg_oracle::compile::compile(
            &mtg_oracle::compile::FaceText {
                name: &row.name,
                card_types: &types.card_types,
                subtypes: &types.subtypes,
                oracle_text: row.oracle_text.as_deref(),
                mana_cost: row.mana_cost.as_str(),
            },
            &Names,
        );
        assert!(
            compiled.understood(),
            "{text:?} not understood: {:?}",
            compiled.unparsed
        );
        let mut face = to_engine_face(&row, &Names);
        let first = mtg_ir::walk::TOKEN_CARD_BASE + self.tokens.len() as u32;
        let made = mtg_ir::walk::register_tokens(std::iter::once(&mut face), first);
        self.tokens.extend(made);
        self.faces.push(face);
        self.understood.push(true);
        CardId(self.faces.len() as u32 - 1)
    }

    pub fn loyalty(&mut self, cost: &str, type_line: &str, loyalty: i32, text: &str) -> CardId {
        let id = self.card(cost, type_line, None, text);
        self.faces[id.0 as usize].loyalty = Some(loyalty);
        id
    }

    pub fn creature_walker(&mut self, loyalty: i32, pt: (i32, i32), text: &str) -> CardId {
        let id = self.card("", "Creature Planeswalker", Some(pt), text);
        self.faces[id.0 as usize].loyalty = Some(loyalty);
        id
    }

    /// Pair two compiled faces; the back's original id remains a separate fixture card.
    pub fn pair(&mut self, front: CardId, back: CardId, layout: mtg_ir::Layout) {
        self.backs
            .insert(front, self.faces[back.0 as usize].clone());
        self.layouts.insert(front, layout);
    }

    pub fn mountain(&mut self) -> CardId {
        self.card("", "Basic Land — Mountain", None, "")
    }

    pub fn bear(&mut self) -> CardId {
        self.card("{1}{G}", "Creature — Bear", Some((2, 2)), "")
    }
}

impl PrintedCards for Table {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        if let Some(i) = card.0.checked_sub(mtg_ir::walk::TOKEN_CARD_BASE) {
            return (face == 0).then(|| self.tokens.get(i as usize)).flatten();
        }
        match face {
            0 => self.faces.get(card.0 as usize),
            1 => self.backs.get(&card),
            _ => None,
        }
    }
    fn layout(&self, card: CardId) -> mtg_ir::Layout {
        self.layouts
            .get(&card)
            .copied()
            .unwrap_or(mtg_ir::Layout::Normal)
    }

    fn subtype_name(&self, subtype: Subtype) -> Option<&str> {
        SUBTYPES.get(subtype.0 as usize).copied()
    }

    fn subtype_named(&self, name: &str) -> Option<Subtype> {
        let i = SUBTYPES.iter().position(|s| *s == name)?;
        Some(Subtype(u16::try_from(i).ok()?))
    }
}

/// A game at P0's turn 2, before the untap step, with both libraries stocked.
pub struct Game {
    pub engine: Engine,
    pub table: Table,
    /// The priority prompt `until` stopped at, which `act` answers.
    pub pending: Option<Choice>,
}

impl Game {
    pub fn new(mut table: Table) -> Self {
        let filler = table.bear();
        let mut state = GameState::new(&[P0, P1], 20);
        state.turn = 2;
        state.active_player = P0;
        state.step = Step::Untap;
        state.priority = Some(P0);
        for _ in 0..20 {
            state.place(filler, P0, ZoneRef::of(Zone::Library, P0));
            state.place(filler, P1, ZoneRef::of(Zone::Library, P1));
        }
        Self {
            engine: Engine::new(state),
            table,
            pending: None,
        }
    }

    /// Put a card somewhere before the game starts. Permanents placed this way are not
    /// summoning sick: they have been there since before the turn.
    pub fn put(&mut self, card: CardId, owner: PlayerId, zone: Zone) -> ObjectId {
        let zr = if zone.is_shared() {
            ZoneRef::shared(zone)
        } else {
            ZoneRef::of(zone, owner)
        };
        self.engine.state.place(card, owner, zr)
    }

    /// `n` untapped lands that each tap for any colour.
    pub fn lands(&mut self, n: usize) {
        let m = self
            .table
            .card("", "Land", None, "{T}: Add one mana of any color.");
        for _ in 0..n {
            self.put(m, P0, Zone::Battlefield);
        }
    }

    /// Advance until `who` has priority in `step` of their own turn, with an empty stack.
    /// Every other
    /// question is answered with its default.
    pub fn until(&mut self, who: PlayerId, step: Step) -> Vec<Action> {
        for _ in 0..10_000 {
            match self.engine.advance(&self.table) {
                Progress::Continue => {}
                Progress::GameOver { .. } => panic!("game ended"),
                Progress::NeedsChoice(c) => {
                    if let ChoiceKind::Priority { legal } = &c.kind
                        && c.who == who
                        && self.engine.state.active_player == who
                        && self.engine.state.step == step
                        && self.stack().is_empty()
                    {
                        let actions = legal.actions.clone();
                        self.pending = Some(c);
                        return actions;
                    }
                    self.default_answer(c);
                }
            }
        }
        panic!("never reached {step:?} for {who:?}");
    }

    pub fn main(&mut self) -> Vec<Action> {
        self.until(P0, Step::PrecombatMain)
    }

    /// Take an action at the current priority prompt, choose `targets` in order when asked,
    /// answer any other question with `answers` (in order) or its default, and let the stack
    /// resolve until P0 has priority on an empty stack again.
    #[track_caller]
    pub fn act(&mut self, action: Action, targets: &[Target], answers: &[Answer]) {
        self.act_then(action, targets, answers, true);
    }

    /// Like [`Game::act`], but stop at P0's next priority even with the stack not empty —
    /// to respond to one's own spell.
    #[track_caller]
    pub fn act_holding(&mut self, action: Action, targets: &[Target]) {
        self.act_then(action, targets, &[], false);
    }

    #[track_caller]
    fn act_then(&mut self, action: Action, targets: &[Target], answers: &[Answer], empty: bool) {
        let c = self.pending.take().expect("no priority prompt to act on");
        let step = self.engine.state.step;
        self.engine
            .answer(&self.table, c.id, Answer::Action(action.clone()))
            .unwrap_or_else(|e| panic!("{action:?} refused: {e:?}"));
        let mut targets = targets.iter().copied();
        let mut answers = answers.iter().cloned();
        for _ in 0..10_000 {
            match self.engine.advance(&self.table) {
                Progress::Continue => {}
                Progress::GameOver { .. } => return,
                Progress::NeedsChoice(c) => match &c.kind {
                    ChoiceKind::ChooseTargets { .. } => {
                        // Out of listed targets, an optional slot takes its default of none.
                        let answer = match (targets.next(), &c.default) {
                            (Some(t), _) => Answer::Targets(vec![vec![t]]),
                            (None, Some(Answer::Targets(d))) if d.iter().all(Vec::is_empty) => {
                                Answer::Targets(d.clone())
                            }
                            (None, _) => panic!("asked for more targets than given"),
                        };
                        self.engine
                            .answer(&self.table, c.id, answer)
                            .expect("target accepted");
                    }
                    ChoiceKind::Priority { legal } => {
                        if (self.stack().is_empty() || !empty)
                            && c.who == P0
                            && self.engine.state.step == step
                        {
                            let _ = legal;
                            self.pending = Some(c);
                            return;
                        }
                        self.engine.answer(&self.table, c.id, Answer::Pass).unwrap();
                    }
                    _ => match answers.next() {
                        Some(a) => self.engine.answer(&self.table, c.id, a).unwrap(),
                        None => self.default_answer(c),
                    },
                },
            }
        }
        panic!("the stack never emptied");
    }

    /// From P0's main phase: attack with `attackers`, have P1 declare `blocks` as
    /// (blocker, attacker) pairs, answer target questions with `targets` and other
    /// questions with `answers`, and play on to P0's postcombat main phase.
    pub fn combat(
        &mut self,
        attackers: &[ObjectId],
        blocks: &[(ObjectId, ObjectId)],
        targets: &[Target],
        answers: &[Answer],
    ) {
        self.combat_declaration(
            Answer::Objects(attackers.to_vec()),
            blocks,
            targets,
            answers,
        );
    }

    pub fn combat_at(&mut self, attackers: &[(ObjectId, Target)], blocks: &[(ObjectId, ObjectId)]) {
        self.combat_declaration(Answer::Attackers(attackers.to_vec()), blocks, &[], &[]);
    }

    fn combat_declaration(
        &mut self,
        declaration: Answer,
        blocks: &[(ObjectId, ObjectId)],
        targets: &[Target],
        answers: &[Answer],
    ) {
        let c = self.pending.take().expect("no priority prompt");
        self.engine.answer(&self.table, c.id, Answer::Pass).unwrap();
        let mut targets = targets.iter().copied();
        let mut answers = answers.iter().cloned();
        for _ in 0..10_000 {
            match self.engine.advance(&self.table) {
                Progress::Continue => {}
                Progress::GameOver { .. } => return,
                Progress::NeedsChoice(c) => match &c.kind {
                    ChoiceKind::DeclareAttackers { .. } => {
                        self.engine
                            .answer(&self.table, c.id, declaration.clone())
                            .expect("attack declared");
                    }
                    ChoiceKind::DeclareBlockers { .. } => {
                        self.engine
                            .answer(&self.table, c.id, Answer::Blocks(blocks.to_vec()))
                            .expect("blocks declared");
                    }
                    ChoiceKind::ChooseTargets { .. } => {
                        let t = targets.next().expect("asked for more targets than given");
                        self.engine
                            .answer(&self.table, c.id, Answer::Targets(vec![vec![t]]))
                            .expect("target accepted");
                    }
                    ChoiceKind::Priority { .. } => {
                        if c.who == P0
                            && self.engine.state.step == Step::PostcombatMain
                            && self.stack().is_empty()
                        {
                            self.pending = Some(c);
                            return;
                        }
                        self.engine.answer(&self.table, c.id, Answer::Pass).unwrap();
                    }
                    _ => match answers.next() {
                        Some(a) => self.engine.answer(&self.table, c.id, a).unwrap(),
                        None => self.default_answer(c),
                    },
                },
            }
        }
        panic!("combat never finished");
    }

    /// Drive the game, letting `decide` answer any choice it wants to (else the default),
    /// until `done` holds at a priority prompt, which is left pending.
    pub fn drive(
        &mut self,
        mut decide: impl FnMut(&Game, &Choice) -> Option<Answer>,
        done: impl Fn(&Game) -> bool,
    ) {
        for _ in 0..10_000 {
            match self.engine.advance(&self.table) {
                Progress::Continue => {}
                Progress::GameOver { .. } => panic!("game ended"),
                Progress::NeedsChoice(c) => {
                    if matches!(c.kind, ChoiceKind::Priority { .. }) && done(self) {
                        self.pending = Some(c);
                        return;
                    }
                    let answer = decide(self, &c)
                        .or_else(|| c.default.clone())
                        .unwrap_or(Answer::Pass);
                    self.engine
                        .answer(&self.table, c.id, answer)
                        .unwrap_or_else(|e| panic!("{:?}: {e:?}", c.kind));
                }
            }
        }
        panic!("never done");
    }

    pub fn cast(&mut self, object: ObjectId, targets: &[Target]) {
        self.act(Action::Cast { object }, targets, &[]);
    }

    fn default_answer(&mut self, c: Choice) {
        let a = c.default.clone().unwrap_or_else(|| match &c.kind {
            ChoiceKind::ChooseObjects { from, min, .. } => {
                Answer::Objects(from.iter().copied().take(*min as usize).collect())
            }
            ChoiceKind::KeepOneLegend { candidates, .. } => {
                Answer::Objects(candidates.first().copied().into_iter().collect())
            }
            ChoiceKind::OrderTriggers { triggers, .. } => Answer::Order(triggers.clone()),
            _ => Answer::Pass,
        });
        self.engine
            .answer(&self.table, c.id, a)
            .expect("default answer");
    }

    pub fn stack(&self) -> Vec<ObjectId> {
        self.engine.state.objects_in(ZoneRef::shared(Zone::Stack))
    }

    pub fn life(&self, p: PlayerId) -> i32 {
        self.engine.state.player(p).life
    }

    pub fn count(&self, zone: Zone, owner: PlayerId) -> usize {
        self.engine.state.objects_in(ZoneRef::of(zone, owner)).len()
    }

    /// The object on the battlefield now made from `card`, if there is exactly one.
    pub fn find(&self, card: CardId) -> Option<ObjectId> {
        let ids: Vec<_> = self
            .engine
            .state
            .battlefield()
            .into_iter()
            .filter(|id| self.engine.state.objects[id].card == card)
            .collect();
        (ids.len() == 1).then(|| ids[0])
    }

    pub fn pt(&mut self, id: ObjectId) -> (i32, i32) {
        let ch = self
            .engine
            .characteristics(&self.table, id)
            .expect("exists");
        (ch.power.unwrap_or(0), ch.toughness.unwrap_or(0))
    }

    pub fn has(&mut self, id: ObjectId, k: mtg_core::Keyword) -> bool {
        let ch = self
            .engine
            .characteristics(&self.table, id)
            .expect("exists")
            .clone();
        ch.granted_keywords.contains(&k)
            || self
                .table
                .face(
                    self.engine.state.objects[&id].card,
                    self.engine.state.objects[&id].face,
                )
                .is_some_and(|f| {
                    f.abilities.iter().any(|a| {
                        ch.abilities.contains(&a.id)
                            && matches!(a.kind, mtg_ir::AbilityKind::Keyword(x) if x == k)
                    })
                })
    }
}

/// Whether an action list offers activating this ability.
pub fn offers(actions: &[Action], source: ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::ActivateAbility { source: s, .. } if *s == source))
}

pub fn activate(source: ObjectId, ability: u16) -> Action {
    Action::ActivateAbility {
        source,
        ability: mtg_core::AbilityId(ability),
    }
}
