//! Casting from the graveyard with an additional cost: retrace, jump-start, escape.
use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::{actions::Action, choice::Answer};

fn exiled(g: &Game) -> usize {
    g.engine
        .state
        .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
        .len()
}

fn offered(actions: &[Action], object: mtg_core::ObjectId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Cast { object: o } if *o == object))
}

#[test]
fn retrace_discards_a_land_and_stays_in_the_graveyard() {
    let mut t = Table::default();
    let flame = t.card(
        "{R}",
        "Sorcery",
        None,
        "~ deals 2 damage to any target.\nRetrace",
    );
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(1);
    let f = g.put(flame, P0, Zone::Graveyard);
    let bear = g.put(mountain, P0, Zone::Hand);
    let _ = bear;
    let actions = g.main();
    assert!(offered(&actions, f));
    g.cast(f, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 18);
    assert_eq!(g.count(Zone::Graveyard, P0), 2, "the land and the spell");
}

#[test]
fn jump_start_discards_and_exiles() {
    let mut t = Table::default();
    let idea = t.card("{U}", "Sorcery", None, "Draw two cards.\nJump-start");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let i = g.put(idea, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(i, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 2);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        1,
        "the bear; the spell is exiled"
    );
    assert_eq!(exiled(&g), 1);
}

#[test]
fn escape_exiles_other_cards_from_the_graveyard() {
    let mut t = Table::default();
    let bolt = t.card(
        "{R}",
        "Instant",
        None,
        "~ deals 3 damage to any target.\nEscape—{R}, Exile two other cards from your graveyard.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bolt, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Graveyard);
    g.put(bear, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(offered(&actions, b));
    g.cast(b, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 17);
    assert_eq!(exiled(&g), 2, "the two bears");
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "the bolt itself");
}

#[test]
fn an_ability_can_exile_a_card_from_the_graveyard_as_its_cost() {
    let mut t = Table::default();
    let ghoul = t.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "{1}, Exile a creature card from your graveyard: You gain 2 life.",
    );
    let bear = t.bear();
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mut g = Game::new(t);
    g.lands(1);
    let gh = g.put(ghoul, P0, Zone::Battlefield);
    g.put(bolt, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(!offers(&actions, gh), "no creature card in the graveyard");
    let _ = bear;
    let mut t2 = Table::default();
    let ghoul = t2.card(
        "{1}{B}",
        "Creature — Zombie",
        Some((2, 2)),
        "{1}, Exile a creature card from your graveyard: You gain 2 life.",
    );
    let bear = t2.bear();
    let mut g = Game::new(t2);
    g.lands(1);
    let gh = g.put(ghoul, P0, Zone::Battlefield);
    g.put(bear, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(offers(&actions, gh));
    g.act(activate(gh, 0), &[], &[]);
    assert_eq!(g.life(P0), 22);
    assert_eq!(exiled(&g), 1);
    assert_eq!(g.count(Zone::Graveyard, P0), 0);
}

fn special(actions: &[Action], source: mtg_core::ObjectId) -> Option<Action> {
    actions
        .iter()
        .find(|a| matches!(a, Action::SpecialAction { source: s, .. } if *s == source))
        .cloned()
}

fn in_exile(g: &Game, card: mtg_core::CardId) -> mtg_core::ObjectId {
    g.engine
        .state
        .objects_in(mtg_core::ZoneRef::shared(Zone::Exile))
        .into_iter()
        .find(|id| g.engine.state.objects[id].card == card)
        .expect("in exile")
}

#[test]
fn foretell_then_cast_on_a_later_turn_for_less() {
    let mut t = Table::default();
    let bolt = t.card(
        "{3}{R}",
        "Instant",
        None,
        "~ deals 3 damage to any target.\nForetell {R}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let b = g.put(bolt, P0, Zone::Hand);
    let actions = g.main();
    g.act(special(&actions, b).expect("foretell offered"), &[], &[]);
    let e = in_exile(&g, bolt);
    assert!(g.engine.state.objects[&e].face_down, "foretold face down");
    let actions = g.main();
    assert!(!offered(&actions, e), "not the turn it was foretold");
    g.until(P0, mtg_core::Step::End);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(offered(&actions, e));
    g.cast(e, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 17, "cast for {{R}}");
}

#[test]
fn plot_then_cast_free_on_a_later_turn() {
    let mut t = Table::default();
    let blast = t.card(
        "{4}{R}",
        "Sorcery",
        None,
        "~ deals 4 damage to any target.\nPlot {1}",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(blast, P0, Zone::Hand);
    let actions = g.main();
    g.act(special(&actions, b).expect("plot offered"), &[], &[]);
    let e = in_exile(&g, blast);
    assert!(!g.engine.state.objects[&e].face_down);
    g.until(P0, mtg_core::Step::End);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(offered(&actions, e));
    g.cast(e, &[Target::Player(P1)]);
    assert_eq!(g.life(P1), 16, "free, with one land");
}

#[test]
fn warp_exiles_at_end_step_and_can_be_cast_later() {
    let mut t = Table::default();
    let pilot = t.card("{3}{R}", "Creature — Human Pilot", Some((3, 3)), "Warp {R}");
    let mut g = Game::new(t);
    g.lands(4);
    let p = g.put(pilot, P0, Zone::Hand);
    let actions = g.main();
    let warp = actions
        .iter()
        .find(|a| matches!(a, Action::CastAlternative { object, .. } if *object == p))
        .cloned()
        .expect("warp offered");
    g.act(warp, &[], &[]);
    assert!(g.find(pilot).is_some(), "on the battlefield");
    g.until(P1, mtg_core::Step::Upkeep);
    let e = in_exile(&g, pilot);
    let actions = g.until(P0, mtg_core::Step::PrecombatMain);
    assert!(offered(&actions, e), "castable from exile on a later turn");
    g.cast(e, &[]);
    assert!(g.find(pilot).is_some());
}

#[test]
fn disturb_casts_the_back_face_from_the_graveyard() {
    let mut t = Table::default();
    let front = t.card(
        "{1}{W}",
        "Creature — Human Soldier",
        Some((1, 1)),
        "Disturb {1}{W}",
    );
    let back = t.card(
        "",
        "Creature — Spirit",
        Some((1, 1)),
        "Flying\nIf ~ would be put into a graveyard from anywhere, exile it instead.",
    );
    t.pair(front, back, mtg_ir::Layout::Transforming);
    let mut g = Game::new(t);
    g.lands(2);
    let c = g.put(front, P0, Zone::Graveyard);
    let actions = g.main();
    let cast = actions
        .iter()
        .find(|a| matches!(a, Action::CastFace { object, face: 1 } if *object == c))
        .cloned()
        .expect("disturb offered");
    assert!(
        !offered(&actions, c),
        "the front face is not castable from the graveyard"
    );
    g.act(cast, &[], &[]);
    let spirit = g.find(front).expect("on the battlefield");
    assert_eq!(g.engine.state.objects[&spirit].face, 1, "transformed");
    assert!(g.has(spirit, mtg_core::Keyword::Flying));
}

#[test]
fn reanimate_an_opponents_creature_under_your_control() {
    let mut t = Table::default();
    let rise = t.card(
        "{B}",
        "Sorcery",
        None,
        "Put target creature card from a graveyard onto the battlefield under your control.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let b = g.put(bear, P1, Zone::Graveyard);
    let r = g.put(rise, P0, Zone::Hand);
    g.main();
    g.cast(r, &[Target::Object(b)]);
    let on = g.find(bear).expect("on the battlefield");
    assert_eq!(g.engine.state.objects[&on].owner, P1);
    assert_eq!(
        mtg_engine::layers::controller(&g.engine.state, on),
        Some(P0),
        "under your control"
    );
}

#[test]
fn a_trigger_that_works_from_the_graveyard() {
    let mut t = Table::default();
    let ghost = t.card(
        "{1}{B}",
        "Creature — Spirit",
        Some((2, 1)),
        "At the beginning of your upkeep, if this card is in your graveyard, you may pay {1}. \
         If you do, return this card to your hand.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    g.put(ghost, P0, Zone::Graveyard);
    // Game::new starts before P0's untap step: the upkeep trigger is coming. Say yes to
    // paying, and pass otherwise, until the main phase.
    for _ in 0..1000 {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::NeedsChoice(c) => {
                if g.engine.state.step == mtg_core::Step::PrecombatMain {
                    break;
                }
                let a = match &c.kind {
                    mtg_engine::choice::ChoiceKind::Priority { .. } => Answer::Pass,
                    mtg_engine::choice::ChoiceKind::Confirm => Answer::Bool(true),
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, a).unwrap();
            }
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::GameOver { .. } => panic!("game over"),
        }
    }
    let in_hand = g
        .engine
        .state
        .objects_in(mtg_core::ZoneRef::of(Zone::Hand, P0))
        .iter()
        .any(|id| g.engine.state.objects[id].card == ghost);
    assert!(in_hand, "paid {{1}} and returned it");
}

#[test]
fn an_ability_exiles_its_card_from_the_graveyard() {
    let mut t = Table::default();
    let shade = t.card(
        "{1}{B}",
        "Creature — Shade",
        Some((1, 1)),
        "{1}, Exile this card from your graveyard: You gain 2 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(shade, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(offers(&actions, s));
    g.act(activate(s, 0), &[], &[]);
    assert_eq!(g.life(P0), 22);
    assert_eq!(exiled(&g), 1);
}
