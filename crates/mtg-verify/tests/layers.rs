//! CR 613 — the layer system. Expectations are derived from the Comprehensive Rules
//! text (2026-09-25 release), not from recorded production output. Every effect comes
//! from rules text compiled by the real compiler and is created through ordinary
//! activations, so the engine is exercised the way a game would exercise it.
mod support;

use mtg_core::{CardId, CardType, ManaCost, ObjectId, PlayerId, Target, Zone, ZoneRef};
use mtg_engine::{
    Engine, Progress,
    actions::Action,
    choice::{Answer, ChoiceKind},
    layers::PrintedCards,
    state::GameState,
};
use mtg_ir::CardFace;
use support::{P0, P1};

// Subjects.
const BEAR: CardId = CardId(0); // 2/2
const WALL: CardId = CardId(1); // 1/4
const GOYF: CardId = CardId(2); // */*, P/T = number of creatures you control
const LORD: CardId = CardId(3); // 1/1, "Other creatures you control get +1/+1."
// Noncreature artifacts with one activated ability each, so they never become subjects.
const SHRINK: CardId = CardId(10);
const SWAP: CardId = CardId(11);
const GROW: CardId = CardId(12);
const PUMP: CardId = CardId(13);
const SILENCE: CardId = CardId(14);
const STEAL: CardId = CardId(15);
const ANTHEM: CardId = CardId(16);
const WINGS: CardId = CardId(17);
const MUTE: CardId = CardId(4); // 1/1, "All creatures lose all abilities."
const BIRD: CardId = CardId(5); // 1/1, "Flying"
const SPRITE: CardId = CardId(6); // blue 2/2
const PAINT: CardId = CardId(18);
const BLUE_LORD: CardId = CardId(19);
const BLACK_WINGS: CardId = CardId(20);

struct NoSubtypes;
impl mtg_oracle::convert::Subtypes for NoSubtypes {
    fn intern(&self, _name: &str) -> Option<u16> {
        None
    }
}

fn face(name: &str, types: &[CardType], pt: Option<(i32, i32)>, text: &str) -> CardFace {
    let compiled = mtg_oracle::compile::compile(
        &mtg_oracle::compile::FaceText {
            name,
            card_types: types,
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "",
        },
        &NoSubtypes,
    );
    assert!(
        compiled.unparsed.is_empty(),
        "{name}: {:?}",
        compiled.unparsed
    );
    CardFace {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        card_types: types.to_vec(),
        subtypes: vec![],
        supertypes: vec![],
        power: pt.map(|p| p.0),
        toughness: pt.map(|p| p.1),
        loyalty: None,
        abilities: compiled.abilities,
        oracle_text: Some(text.into()),
        colors: None,
    }
}

struct Faces(Vec<(CardId, CardFace)>);

impl Faces {
    fn new() -> Self {
        let creature = [CardType::Creature];
        let artifact = [CardType::Artifact];
        Self(vec![
            (BEAR, face("Bear", &creature, Some((2, 2)), "")),
            (WALL, face("Wall", &creature, Some((1, 4)), "")),
            (
                GOYF,
                face(
                    "Goyf",
                    &creature,
                    Some((0, 0)),
                    "This creature's power and toughness are each equal to the number of creatures you control.",
                ),
            ),
            (
                LORD,
                face(
                    "Lord",
                    &creature,
                    Some((1, 1)),
                    "Other creatures you control get +1/+1.",
                ),
            ),
            (
                SHRINK,
                face(
                    "Shrink",
                    &artifact,
                    None,
                    "{0}: Target creature has base power and toughness 0/1 until end of turn.",
                ),
            ),
            (
                SWAP,
                face(
                    "Swap",
                    &artifact,
                    None,
                    "{0}: Switch target creature's power and toughness until end of turn.",
                ),
            ),
            (
                GROW,
                face(
                    "Grow",
                    &artifact,
                    None,
                    "{0}: Put a +1/+1 counter on target creature.",
                ),
            ),
            (
                PUMP,
                face(
                    "Pump",
                    &artifact,
                    None,
                    "{0}: Target creature gets +2/+0 until end of turn.",
                ),
            ),
            (
                SILENCE,
                face(
                    "Silence",
                    &artifact,
                    None,
                    "{0}: Target creature loses all abilities until end of turn.",
                ),
            ),
            (
                STEAL,
                face(
                    "Steal",
                    &artifact,
                    None,
                    "{0}: Gain control of target creature until end of turn.",
                ),
            ),
            (
                ANTHEM,
                face(
                    "Anthem",
                    &artifact,
                    None,
                    "Creatures you control get +1/+1.",
                ),
            ),
            (
                WINGS,
                face(
                    "Wings",
                    &artifact,
                    None,
                    "{0}: Target creature gains flying until end of turn.",
                ),
            ),
            (
                MUTE,
                face(
                    "Mute",
                    &[CardType::Creature],
                    Some((1, 1)),
                    "All creatures lose all abilities.",
                ),
            ),
            (
                BIRD,
                face("Bird", &[CardType::Creature], Some((1, 1)), "Flying"),
            ),
            (SPRITE, {
                let mut f = face("Sprite", &[CardType::Creature], Some((2, 2)), "");
                f.colors = Some(vec![mtg_core::Color::Blue]);
                f
            }),
            (
                PAINT,
                face(
                    "Paint",
                    &artifact,
                    None,
                    "{0}: Target creature becomes the color of your choice until end of turn.",
                ),
            ),
            (
                BLUE_LORD,
                face("Blue Lord", &artifact, None, "Blue creatures get +1/+1."),
            ),
            (
                BLACK_WINGS,
                face(
                    "Black Wings",
                    &artifact,
                    None,
                    "Black creatures you control have flying.",
                ),
            ),
        ])
    }
}

impl PrintedCards for Faces {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        self.0.iter().find(|(id, _)| *id == card).map(|(_, f)| f)
    }

    fn subtype_name(&self, _subtype: mtg_core::Subtype) -> Option<&str> {
        None
    }
}

struct Game {
    engine: Engine,
    cards: Faces,
}

impl Game {
    /// Two players in P0's first main phase, with libraries so nobody decks.
    fn new(setup: impl FnOnce(&mut GameState)) -> Self {
        let mut state = GameState::new(&[P0, P1], 20);
        state.turn = 2;
        state.step = mtg_core::Step::PrecombatMain;
        state.active_player = P0;
        state.priority = Some(P0);
        for p in [P0, P1] {
            for _ in 0..5 {
                state.place(BEAR, p, ZoneRef::of(Zone::Library, p));
            }
        }
        setup(&mut state);
        Self {
            engine: Engine::new(state),
            cards: Faces::new(),
        }
    }

    fn next_choice(&mut self) -> mtg_engine::Choice {
        for _ in 0..200 {
            match self.engine.advance(&self.cards) {
                Progress::NeedsChoice(choice) => return choice,
                Progress::GameOver { .. } => panic!("unexpected game over"),
                _ => {}
            }
        }
        panic!("engine did not reach a decision");
    }

    /// P0 activates `source`'s ability targeting `target`; both players pass until it
    /// has resolved.
    fn activate(&mut self, source: ObjectId, target: ObjectId) {
        self.activate_choosing(source, target, "");
    }

    /// As [`Game::activate`], answering a choice among labelled options (a color) with
    /// the one named `pick`.
    fn activate_choosing(&mut self, source: ObjectId, target: ObjectId, pick: &str) {
        let ability = self
            .cards
            .face(self.engine.state.objects[&source].card, 0)
            .unwrap()
            .abilities[0]
            .id;
        let choice = self.next_choice();
        assert_eq!(choice.who, P0);
        self.engine
            .answer(
                &self.cards,
                choice.id,
                Answer::Action(Action::ActivateAbility { source, ability }),
            )
            .unwrap();
        for _ in 0..20 {
            let choice = self.next_choice();
            let stack_empty = self
                .engine
                .state
                .objects_in(ZoneRef::shared(Zone::Stack))
                .is_empty();
            let answer = match &choice.kind {
                ChoiceKind::ChooseTargets { .. } => {
                    Answer::Targets(vec![vec![Target::Object(target)]])
                }
                ChoiceKind::ChooseModes { available, .. } => Answer::Modes(vec![
                    available
                        .iter()
                        .position(|l| l.eq_ignore_ascii_case(pick))
                        .unwrap_or_else(|| panic!("no option {pick:?} in {available:?}"))
                        as u8,
                ]),
                ChoiceKind::Priority { .. } if stack_empty => return,
                ChoiceKind::Priority { .. } => Answer::Pass,
                other => panic!("unexpected choice {other:?}"),
            };
            self.engine.answer(&self.cards, choice.id, answer).unwrap();
        }
        panic!("the ability did not resolve");
    }

    fn pt(&self, id: ObjectId) -> (i32, i32) {
        let c = mtg_engine::layers::compute(&self.engine.state, &self.cards, id)
            .expect("object exists");
        (c.power.unwrap(), c.toughness.unwrap())
    }
}

fn on_battlefield(state: &mut GameState, card: CardId, controller: PlayerId) -> ObjectId {
    let id = state.place(card, controller, ZoneRef::shared(Zone::Battlefield));
    state.objects.get_mut(&id).unwrap().summoning_sick = false;
    id
}

/// CR 613.4b/613.4c: setting base P/T (7b) always applies before counters (7c),
/// whichever came first. 2/2 with a +1/+1 counter, then base 0/1: 1/2.
#[test]
fn cr_613_4_counters_apply_after_base_setting_regardless_of_timestamp() {
    for counter_first in [true, false] {
        let (mut bear, mut grow, mut shrink) = (ObjectId(0), ObjectId(0), ObjectId(0));
        let mut g = Game::new(|s| {
            bear = on_battlefield(s, BEAR, P0);
            grow = on_battlefield(s, GROW, P0);
            shrink = on_battlefield(s, SHRINK, P0);
        });
        if counter_first {
            g.activate(grow, bear);
            g.activate(shrink, bear);
        } else {
            g.activate(shrink, bear);
            g.activate(grow, bear);
        }
        assert_eq!(g.pt(bear), (1, 2), "counter first: {counter_first}");
    }
}

/// CR 613.4c/613.4d: switching (7d) applies after modifications (7c), even when the
/// switch effect is older. A 1/4 with +2/+0 is 3/4, switched 4/3.
#[test]
fn cr_613_4d_switch_applies_after_modifications_regardless_of_timestamp() {
    for switch_first in [true, false] {
        let (mut wall, mut swap, mut pump) = (ObjectId(0), ObjectId(0), ObjectId(0));
        let mut g = Game::new(|s| {
            wall = on_battlefield(s, WALL, P0);
            swap = on_battlefield(s, SWAP, P0);
            pump = on_battlefield(s, PUMP, P0);
        });
        if switch_first {
            g.activate(swap, wall);
            g.activate(pump, wall);
        } else {
            g.activate(pump, wall);
            g.activate(swap, wall);
        }
        assert_eq!(g.pt(wall), (4, 3), "switch first: {switch_first}");
    }
}

/// CR 613.4a/613.4b/613.4c: a characteristic-defining ability (7a) is overridden by an
/// effect setting base P/T (7b); modifications (7c) still apply on top.
#[test]
fn cr_613_4a_cda_then_base_setting_then_anthem() {
    let (mut goyf, mut shrink) = (ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        goyf = on_battlefield(s, GOYF, P0);
        on_battlefield(s, BEAR, P0);
        on_battlefield(s, BEAR, P0);
        shrink = on_battlefield(s, SHRINK, P0);
    });
    assert_eq!(g.pt(goyf), (3, 3), "three creatures: the CDA itself");
    g.activate(shrink, goyf);
    assert_eq!(g.pt(goyf), (0, 1), "7b overrides 7a");
    let (mut goyf, mut shrink) = (ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        goyf = on_battlefield(s, GOYF, P0);
        on_battlefield(s, ANTHEM, P0);
        shrink = on_battlefield(s, SHRINK, P0);
    });
    assert_eq!(g.pt(goyf), (2, 2), "one creature, plus the anthem");
    g.activate(shrink, goyf);
    assert_eq!(g.pt(goyf), (1, 2), "7c still applies after 7b");
}

/// CR 613.1f before 613.1g: removing a creature's abilities (layer 6) removes the P/T
/// effect its own static ability generates, but not an effect that came from another
/// source's resolved ability.
#[test]
fn cr_613_1_ability_removal_stops_the_lords_anthem_not_a_resolved_pump() {
    let (mut lord, mut bear, mut silence, mut pump) =
        (ObjectId(0), ObjectId(0), ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        lord = on_battlefield(s, LORD, P0);
        bear = on_battlefield(s, BEAR, P0);
        silence = on_battlefield(s, SILENCE, P0);
        pump = on_battlefield(s, PUMP, P0);
    });
    assert_eq!(g.pt(bear), (3, 3));
    g.activate(pump, bear);
    assert_eq!(g.pt(bear), (5, 3));
    g.activate(silence, lord);
    assert_eq!(g.pt(bear), (4, 2), "the Lord's ability is gone");
    g.activate(silence, bear);
    assert_eq!(
        g.pt(bear),
        (4, 2),
        "losing its own abilities doesn't end another source's pump"
    );
}

/// CR 604.3 / 613.1f: a characteristic-defining ability is a static ability. A creature
/// that loses it has no 7a effect and uses its printed */* as 0/0, so it dies (CR
/// 704.5f). Layer 6 comes before 7a, so 613.6's "continues to apply" doesn't save it.
#[test]
fn cr_613_cda_removed_in_layer_6_never_applies_in_7a() {
    let (mut goyf, mut silence) = (ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        goyf = on_battlefield(s, GOYF, P0);
        on_battlefield(s, BEAR, P0);
        silence = on_battlefield(s, SILENCE, P0);
    });
    assert_eq!(g.pt(goyf), (2, 2));
    g.activate(silence, goyf);
    assert!(
        !g.engine.state.objects.contains_key(&goyf),
        "a 0/0 is put into its owner's graveyard"
    );
    assert_eq!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .len(),
        1
    );
}

/// CR 613.1b before 613.1g: control is decided in layer 2, so "creatures you control"
/// anthems follow the new controller. P1's Bear, stolen by P0, gets P0's anthem and
/// stops getting P1's.
#[test]
fn cr_613_1b_control_change_moves_the_creature_between_anthems() {
    let (mut bear, mut steal) = (ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        bear = on_battlefield(s, BEAR, P1);
        on_battlefield(s, LORD, P1);
        on_battlefield(s, ANTHEM, P0);
        on_battlefield(s, ANTHEM, P0);
        steal = on_battlefield(s, STEAL, P0);
    });
    assert_eq!(g.pt(bear), (3, 3), "P1's Lord");
    g.activate(steal, bear);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, bear),
        Some(P0)
    );
    assert_eq!(g.pt(bear), (4, 4), "P0's two anthems, not P1's Lord");
}

/// CR 613.3: within layer 6, effects apply in timestamp order. Losing all abilities
/// and then gaining flying leaves flying; gaining flying first, then losing all
/// abilities, leaves none.
#[test]
fn cr_613_3_ability_adding_and_removing_follow_timestamps() {
    for wings_first in [true, false] {
        let (mut bear, mut wings, mut silence) = (ObjectId(0), ObjectId(0), ObjectId(0));
        let mut g = Game::new(|s| {
            bear = on_battlefield(s, BEAR, P0);
            wings = on_battlefield(s, WINGS, P0);
            silence = on_battlefield(s, SILENCE, P0);
        });
        if wings_first {
            g.activate(wings, bear);
            g.activate(silence, bear);
        } else {
            g.activate(silence, bear);
            g.activate(wings, bear);
        }
        let c = mtg_engine::layers::compute(&g.engine.state, &g.cards, bear).unwrap();
        let flying = c.granted_keywords.contains(&mtg_core::Keyword::Flying);
        assert_eq!(flying, !wings_first, "wings first: {wings_first}");
    }
}

/// CR 613.6: an effect that has started to apply keeps applying even if its ability
/// is removed in the process. A creature whose static ability makes all creatures
/// lose all abilities loses that ability too, yet the effect still applies: the Bird
/// has no flying and the Lord's anthem is gone.
#[test]
fn cr_613_6_global_ability_loss_from_a_creature_still_applies() {
    let (mut bird, mut bear) = (ObjectId(0), ObjectId(0));
    let g = Game::new(|s| {
        on_battlefield(s, MUTE, P0);
        on_battlefield(s, LORD, P0);
        bird = on_battlefield(s, BIRD, P0);
        bear = on_battlefield(s, BEAR, P0);
    });
    let c = mtg_engine::layers::compute(&g.engine.state, &g.cards, bird).unwrap();
    assert!(c.abilities.is_empty(), "{:?}", c.abilities);
    assert_eq!(g.pt(bear), (2, 2));
}

/// CR 613.1e before 613.1g: color is decided in layer 5, before power and toughness, so
/// "blue creatures get +1/+1" includes a creature that became blue and excludes a blue
/// one that became red.
#[test]
fn cr_613_1e_color_change_decides_a_color_anthem() {
    let (mut bear, mut sprite, mut paint) = (ObjectId(0), ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        bear = on_battlefield(s, BEAR, P0);
        sprite = on_battlefield(s, SPRITE, P0);
        paint = on_battlefield(s, PAINT, P0);
        on_battlefield(s, BLUE_LORD, P0);
    });
    assert_eq!(
        (g.pt(bear), g.pt(sprite)),
        ((2, 2), (3, 3)),
        "printed colors"
    );
    g.activate_choosing(paint, bear, "blue");
    let colors = mtg_engine::layers::compute(&g.engine.state, &g.cards, bear)
        .unwrap()
        .colors;
    assert!(colors.contains(mtg_core::Color::Blue), "{colors:?}");
    assert_eq!(g.pt(bear), (3, 3), "the Bear became blue");
    g.activate_choosing(paint, sprite, "red");
    assert_eq!(g.pt(sprite), (2, 2), "the Sprite is no longer blue");
}

/// CR 613.1e before 613.1f: color (layer 5) decides "black creatures you control have
/// flying" (layer 6).
#[test]
fn cr_613_1e_color_change_decides_an_ability_grant() {
    let (mut bear, mut paint) = (ObjectId(0), ObjectId(0));
    let mut g = Game::new(|s| {
        bear = on_battlefield(s, BEAR, P0);
        paint = on_battlefield(s, PAINT, P0);
        on_battlefield(s, BLACK_WINGS, P0);
    });
    let flying = |g: &Game| {
        mtg_engine::layers::compute(&g.engine.state, &g.cards, bear)
            .unwrap()
            .granted_keywords
            .contains(&mtg_core::Keyword::Flying)
    };
    assert!(!flying(&g));
    g.activate_choosing(paint, bear, "black");
    assert!(flying(&g), "the Bear became black");
}
