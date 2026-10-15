use super::harness::*;
use mtg_core::{AbilityId, Color, Step, Target, Zone, ZoneRef};
use mtg_engine::{
    actions::Action,
    choice::{Answer, ChoiceKind},
};

const ULTIMATUM: &str = "Target opponent sacrifices a creature of their choice, discards three cards, then loses 5 life. You return a creature card from your graveyard to your hand, draw three cards, then gain 5 life.";

#[test]
fn cruel_ultimatum_chooses_a_graveyard_card_during_resolution_and_finishes_its_rewards() {
    let mut t = Table::default();
    let ultimatum = t.card("{0}", "Sorcery", None, ULTIMATUM);
    let bear = t.bear();
    let artifact = t.card("{0}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.put(bear, P1, Zone::Battlefield);
    for _ in 0..3 {
        g.put(bear, P1, Zone::Hand);
    }
    let wanted = g.put(bear, P0, Zone::Graveyard);
    g.put(artifact, P0, Zone::Graveyard);
    g.put(bear, P1, Zone::Graveyard);
    let spell = g.put(ultimatum, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.act_holding(Action::Cast { object: spell }, &[Target::Player(P1)]);
    let chosen = std::cell::Cell::new(false);
    g.drive(
        |_, question| {
            if let ChoiceKind::ChooseObjects { from, min, max } = &question.kind {
                if from.contains(&wanted) {
                    assert_eq!(from, &vec![wanted]);
                    assert_eq!((*min, *max), (1, 1));
                    chosen.set(true);
                    return Some(Answer::Objects(vec![wanted]));
                }
                return Some(Answer::Objects(
                    from.iter().copied().take(*min as usize).collect(),
                ));
            }
            None
        },
        |g| g.stack().is_empty(),
    );
    assert!(chosen.get());
    assert_eq!(g.count(Zone::Hand, P0), hand + 3);
    assert_eq!(g.count(Zone::Hand, P1), 0);
    assert_eq!(g.life(P0), 25);
    assert_eq!(g.life(P1), 15);
    assert!(
        g.engine
            .state
            .battlefield()
            .iter()
            .all(|id| g.engine.state.objects[id].owner == P0)
    );
}

#[test]
fn cruel_ultimatum_continues_when_neither_player_has_a_creature_to_move() {
    let mut t = Table::default();
    let ultimatum = t.card("{0}", "Sorcery", None, ULTIMATUM);
    let mut g = Game::new(t);
    let spell = g.put(ultimatum, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[Target::Player(P1)]);
    assert_eq!(g.count(Zone::Hand, P0), hand + 2);
    assert_eq!(g.life(P0), 25);
    assert_eq!(g.life(P1), 15);
}

#[test]
fn corpse_churn_can_choose_a_freshly_milled_card_or_decline_the_return() {
    for accept in [false, true] {
        let mut t = Table::default();
        let churn = t.card("{0}", "Instant", None, "Mill three cards, then you may return a creature card from your graveyard to your hand.");
        let bear = t.bear();
        let artifact = t.card("{0}", "Artifact", None, "");
        let mut g = Game::new(t);
        let spell = g.put(churn, P0, Zone::Hand);
        g.main();
        let milled: Vec<_> = [artifact, bear, artifact]
            .into_iter()
            .map(|card| g.put(card, P0, Zone::Library))
            .collect();
        let library = ZoneRef::of(Zone::Library, P0);
        let order = g.engine.state.zone_order.get_mut(&library).unwrap();
        order.retain(|id| !milled.contains(id));
        order.splice(0..0, milled);
        let hand = g.count(Zone::Hand, P0);
        g.act(Action::Cast { object: spell }, &[], &[Answer::Bool(accept)]);
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + usize::from(accept));
        let graveyard = g.engine.state.objects_in(ZoneRef::of(Zone::Graveyard, P0));
        assert_eq!(
            graveyard
                .iter()
                .filter(|id| g.engine.state.objects[id].card == bear)
                .count(),
            usize::from(!accept)
        );
    }
}

#[test]
fn flywheel_racers_mana_ability_requires_crewing_and_expires_at_cleanup() {
    let mut t = Table::default();
    let racer = t.card("{0}", "Artifact — Vehicle", Some((3, 2)), "Vigilance\n{T}: Add one mana of any color. Activate only if this permanent is a creature.\nCrew 1");
    let bear = t.bear();
    let spell = t.card("{U}", "Instant", None, "You gain 1 life.");
    let spell_card = spell;
    let mut g = Game::new(t);
    let racer = g.put(racer, P0, Zone::Battlefield);
    let bear = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(spell, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: spell }));
    g.act(activate(racer, 2), &[], &[Answer::Objects(vec![bear])]);
    assert!(g.main().contains(&Action::Cast { object: spell }));
    g.act(
        Action::ActivateManaAbility {
            source: racer,
            ability: AbilityId(1),
            color: Some(Color::Blue),
        },
        &[],
        &[],
    );
    g.cast(spell, &[]);
    assert_eq!(g.life(P0), 21);
    g.until(P1, Step::PrecombatMain);
    g.until(P0, Step::PrecombatMain);
    assert!(!g.engine.state.objects[&racer].tapped);
    let next_spell = g.put(spell_card, P0, Zone::Hand);
    assert!(!g.main().contains(&Action::Cast { object: next_spell }));
}

#[test]
fn witherbloom_commands_targeted_mill_and_nontargeted_return_have_distinct_players() {
    let mut t = Table::default();
    let command = t.card("{0}", "Sorcery", None, "Choose two —\n• Target player mills three cards, then you return a land card from your graveyard to your hand.\n• Destroy target noncreature, nonland permanent with mana value 2 or less.\n• Target creature gets -3/-1 until end of turn.\n• Target opponent loses 2 life and you gain 2 life.");
    let land = t.mountain();
    let mut g = Game::new(t);
    g.put(land, P0, Zone::Graveyard);
    let spell = g.put(command, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    let library = g.count(Zone::Library, P1);
    g.act(
        Action::Cast { object: spell },
        &[Target::Player(P1), Target::Player(P1)],
        &[Answer::Modes(vec![0, 3])],
    );
    assert_eq!(g.count(Zone::Hand, P0), hand);
    assert_eq!(g.count(Zone::Library, P1), library - 3);
    assert_eq!(g.life(P0), 22);
    assert_eq!(g.life(P1), 18);
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == land)
    );
}

#[test]
fn takenuma_discards_itself_then_returns_a_creature_without_targeting() {
    let mut t = Table::default();
    let takenuma = t.card("", "Legendary Land", None, "{T}: Add {B}.\nChannel — {3}{B}, Discard this card: Mill three cards, then return a creature or planeswalker card from your graveyard to your hand. This ability costs {1} less to activate for each legendary creature you control.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(4);
    let source = g.put(takenuma, P0, Zone::Hand);
    let wanted = g.put(bear, P0, Zone::Graveyard);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    let library = g.count(Zone::Library, P0);
    g.act(activate(source, 1), &[], &[Answer::Objects(vec![wanted])]);
    assert_eq!(g.count(Zone::Library, P0), library - 3);
    assert_eq!(g.count(Zone::Hand, P0), hand);
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Graveyard, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == takenuma)
    );
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == bear)
    );
}

#[test]
fn dreadlight_monstrosity_requires_an_exiled_card_owned_by_its_controller() {
    for owner in [None, Some(P0), Some(P1)] {
        let mut t = Table::default();
        let monster = t.card("{0}", "Creature", Some((5, 5)), "{0}: This creature can't be blocked this turn. Activate only if you own a card in exile.");
        let bear = t.bear();
        let mut g = Game::new(t);
        let monster = g.put(monster, P0, Zone::Battlefield);
        if let Some(owner) = owner {
            g.put(bear, owner, Zone::Exile);
        }
        let actions = g.main();
        assert_eq!(actions.contains(&activate(monster, 0)), owner == Some(P0));
        if owner == Some(P0) {
            g.act(activate(monster, 0), &[], &[]);
        }
    }
}
