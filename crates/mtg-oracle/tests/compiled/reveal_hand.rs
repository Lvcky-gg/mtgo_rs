//! Revealed cards retain their identity after moving to hand.
use super::{dig::stack_top, harness::*};
use mtg_core::{Event, Zone, ZoneRef};

#[test]
fn life_loss_uses_the_card_moved_into_hand_and_reveal_precedes_move() {
    for pronoun in ["its", "that card's"] {
        let mut t = Table::default();
        let spell=t.card("{U}","Sorcery",None,&format!(
            "Reveal the top card of your library and put that card into your hand. You lose life equal to {pronoun} mana value."));
        let expensive = t.card("{3}{U}{U}", "Creature — Bear", Some((3, 3)), "");
        let cheap = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(spell, P0, Zone::Hand);
        g.main();
        let life = g.engine.state.players[&P0].life;
        let ids = stack_top(&mut g, &[expensive, cheap]);
        g.cast(spell, &[]);
        assert_eq!(g.engine.state.players[&P0].life, life - 5);
        assert!(
            g.engine
                .state
                .objects_in(ZoneRef::of(Zone::Hand, P0))
                .iter()
                .any(|id| g.engine.state.objects[id].card == expensive)
        );
        assert_eq!(
            g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0))[0],
            ids[1]
        );
        assert_eq!(g.engine.state.revealed_cards.len(), 1);
        let reveal = g
            .engine
            .log
            .iter()
            .position(|e| matches!(e.event,Event::Revealed {object} if object==ids[0]))
            .unwrap();
        let moved = g
            .engine
            .log
            .iter()
            .position(|e| matches!(e.event,Event::ZoneChange {object,..} if object==ids[0]))
            .unwrap();
        assert!(reveal < moved);
    }
}

#[test]
fn empty_library_does_not_draw_or_lose_life() {
    let mut t = Table::default();
    let spell=t.card("{U}","Sorcery",None,
        "Reveal the top card of your library and put it into your hand. You lose life equal to its mana value.");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(spell, P0, Zone::Hand);
    g.main();
    let library = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(library) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.remove(&library);
    let life = g.engine.state.players[&P0].life;
    g.cast(spell, &[]);
    assert_eq!(g.engine.state.players[&P0].life, life);
    assert!(g.engine.state.revealed_cards.is_empty());
    assert!(g.pending.is_some());
}

#[test]
fn dark_confidant_and_dark_tutelage_printed_triggers_compile_completely() {
    let mut t = Table::default();
    t.card("{1}{B}","Creature — Wizard",Some((2,1)),
        "At the beginning of your upkeep, reveal the top card of your library and put that card into your hand. You lose life equal to its mana value.");
    t.card("{2}{B}","Enchantment",None,
        "At the beginning of your upkeep, reveal the top card of your library and put that card into your hand. You lose life equal to its mana value.");
}

#[test]
fn reanimate_reads_the_moved_cards_mana_value_after_taking_control() {
    let mut t = Table::default();
    let reanimate = t.card("{B}", "Sorcery", None,
        "Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value.");
    let creature = t.card("{3}{U}{U}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(reanimate, P0, Zone::Hand);
    let target = g.put(creature, P1, Zone::Graveyard);
    g.main();
    let life = g.engine.state.players[&P0].life;
    g.cast(spell, &[mtg_core::Target::Object(target)]);
    let moved = g.find(creature).unwrap();
    assert_eq!(g.engine.state.objects[&moved].controller, P0);
    assert_eq!(g.engine.state.objects[&moved].owner, P1);
    assert_eq!(g.engine.state.players[&P0].life, life - 5);
}

#[test]
fn return_to_hand_reads_the_returned_cards_mana_value() {
    let mut t = Table::default();
    let spell_card = t.card("{W}", "Sorcery", None,
        "Return target artifact card from your graveyard to your hand. You gain life equal to that card's mana value.");
    let artifact = t.card("{4}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let target = g.put(artifact, P0, Zone::Graveyard);
    let spell = g.put(spell_card, P0, Zone::Hand);
    g.main();
    let life = g.engine.state.players[&P0].life;
    g.cast(spell, &[mtg_core::Target::Object(target)]);
    assert_eq!(g.engine.state.players[&P0].life, life + 4);
    assert!(
        g.engine
            .state
            .objects_in(ZoneRef::of(Zone::Hand, P0))
            .iter()
            .any(|id| g.engine.state.objects[id].card == artifact)
    );
}
