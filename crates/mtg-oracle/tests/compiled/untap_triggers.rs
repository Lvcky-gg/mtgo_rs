//! Source untap triggers fire only when the source actually becomes untapped.
use super::{dig::stack_top, harness::*};
use mtg_core::{CounterKind, Target, Zone};

#[test]
fn pain_seer_triggers_on_spell_untap_but_not_on_no_change_or_stun_replacement() {
    for mode in 0..3 {
        let mut t = Table::default();
        let seer=t.card("{1}{B}","Creature — Wizard",Some((2,2)),
            "Whenever this creature becomes untapped, reveal the top card of your library and put that card into your hand. You lose life equal to that card's mana value.");
        let untap = t.card("{U}", "Instant", None, "Untap target creature.");
        let expensive = t.card("{3}{U}{U}", "Creature — Bear", Some((3, 3)), "");
        let mut g = Game::new(t);
        g.lands(1);
        let source = g.put(seer, P0, Zone::Battlefield);
        let spell = g.put(untap, P0, Zone::Hand);
        g.main();
        if mode != 1 {
            g.engine.state.objects.get_mut(&source).unwrap().tapped = true;
        }
        if mode == 2 {
            g.engine
                .state
                .objects
                .get_mut(&source)
                .unwrap()
                .counters
                .insert(CounterKind::Stun, 1);
        }
        stack_top(&mut g, &[expensive]);
        let life = g.engine.state.players[&P0].life;
        let hand = g.count(Zone::Hand, P0);
        g.cast(spell, &[Target::Object(source)]);
        assert_eq!(
            g.engine.state.players[&P0].life,
            life - if mode == 0 { 5 } else { 0 }
        );
        assert_eq!(
            g.count(Zone::Hand, P0),
            hand - if mode == 0 { 0 } else { 1 }
        );
        assert_eq!(g.engine.state.revealed_cards.len(), usize::from(mode == 0));
        assert_eq!(g.engine.state.objects[&source].tapped, mode == 2);
    }
}

#[test]
fn untap_step_queues_pain_seer_before_the_normal_draw() {
    let mut t = Table::default();
    let seer=t.card("{1}{B}","Creature — Wizard",Some((2,2)),
        "Whenever this creature becomes untapped, reveal the top card of your library and put that card into your hand. You lose life equal to that card's mana value.");
    let expensive = t.card("{3}{U}{U}", "Creature — Bear", Some((3, 3)), "");
    let bear = t.bear();
    let mut g = Game::new(t);
    let source = g.put(seer, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&source).unwrap().tapped = true;
    stack_top(&mut g, &[expensive, bear]);
    g.main();
    assert!(!g.engine.state.objects[&source].tapped);
    assert_eq!(g.engine.state.players[&P0].life, 15);
    assert_eq!(g.engine.state.revealed_cards.len(), 1);
    assert_eq!(g.engine.state.revealed_cards[0].card, expensive);
    assert_eq!(g.count(Zone::Hand, P0), 2);
}

#[test]
fn kragma_butcher_untap_trigger_pumps_only_its_source() {
    let mut t = Table::default();
    let butcher = t.card(
        "{2}{R}",
        "Creature — Bear",
        Some((2, 3)),
        "Whenever this creature becomes untapped, it gets +2/+0 until end of turn.",
    );
    let untap = t.card("{U}", "Instant", None, "Untap target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(1);
    let source = g.put(butcher, P0, Zone::Battlefield);
    let other = g.put(bear, P0, Zone::Battlefield);
    let spell = g.put(untap, P0, Zone::Hand);
    g.main();
    g.engine.state.objects.get_mut(&source).unwrap().tapped = true;
    g.cast(spell, &[Target::Object(source)]);
    assert_eq!(g.pt(source), (4, 3));
    assert_eq!(g.pt(other), (2, 2));
}

#[test]
fn mesmeric_orb_mills_the_untapped_permanents_controller() {
    for owner in [P0, P1] {
        let mut t = Table::default();
        let orb = t.card(
            "{2}",
            "Artifact",
            None,
            "Whenever a permanent becomes untapped, that permanent's controller mills a card.",
        );
        let untap = t.card("{U}", "Instant", None, "Untap target creature.");
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        g.put(orb, P0, Zone::Battlefield);
        let creature = g.put(bear, owner, Zone::Battlefield);
        let spell = g.put(untap, P0, Zone::Hand);
        g.main();
        g.engine.state.objects.get_mut(&creature).unwrap().tapped = true;
        g.cast(spell, &[Target::Object(creature)]);
        assert_eq!(
            g.count(Zone::Graveyard, owner),
            1 + usize::from(owner == P0)
        );
        assert_eq!(
            g.count(Zone::Graveyard, if owner == P0 { P1 } else { P0 }),
            usize::from(owner == P1)
        );
    }
}

#[test]
fn generic_tap_trigger_filters_creatures_and_binds_event_subject() {
    let mut t = Table::default();
    let watcher = t.card("{2}", "Artifact", None,
        "Whenever a creature an opponent controls becomes tapped, that creature's controller loses 2 life.");
    let tap = t.card("{U}", "Instant", None, "Tap target permanent.");
    let bear = t.bear();
    let artifact = t.card("{1}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    g.put(watcher, P0, Zone::Battlefield);
    let ours = g.put(bear, P0, Zone::Battlefield);
    let theirs = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(artifact, P1, Zone::Battlefield);
    let spells: Vec<_> = (0..3).map(|_| g.put(tap, P0, Zone::Hand)).collect();
    g.main();
    for (spell, target) in spells.into_iter().zip([ours, other, theirs]) {
        g.cast(spell, &[Target::Object(target)]);
    }
    assert_eq!(g.engine.state.players[&P0].life, 20);
    assert_eq!(g.engine.state.players[&P1].life, 18);
}

#[test]
fn insolence_triggers_only_for_its_host_and_damages_hosts_controller() {
    let mut t = Table::default();
    let aura = t.card("{2}{R}", "Enchantment — Aura", None,
        "Enchant creature\nWhenever enchanted creature becomes tapped, this Aura deals 2 damage to that creature's controller.");
    let tap = t.card("{U}", "Instant", None, "Tap target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let host = g.put(bear, P1, Zone::Battlefield);
    let other = g.put(bear, P1, Zone::Battlefield);
    let aura = g.put(aura, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&aura).unwrap().attached_to = Some(host);
    let spells: Vec<_> = (0..3).map(|_| g.put(tap, P0, Zone::Hand)).collect();
    g.main();
    for (spell, target) in spells.into_iter().zip([other, host, host]) {
        g.cast(spell, &[Target::Object(target)]);
    }
    assert_eq!(g.engine.state.players[&P0].life, 20);
    assert_eq!(g.engine.state.players[&P1].life, 18);
}

#[test]
fn equipped_untap_trigger_uses_current_host() {
    let mut t = Table::default();
    let equipment = t.card(
        "{1}",
        "Artifact — Equipment",
        None,
        "Whenever equipped creature becomes untapped, you gain 2 life.\nEquip {1}",
    );
    let untap = t.card("{U}", "Instant", None, "Untap target creature.");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(2);
    let old_host = g.put(bear, P0, Zone::Battlefield);
    let new_host = g.put(bear, P0, Zone::Battlefield);
    let equipment = g.put(equipment, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&equipment)
        .unwrap()
        .attached_to = Some(old_host);
    let spells: Vec<_> = (0..2).map(|_| g.put(untap, P0, Zone::Hand)).collect();
    g.main();
    g.engine
        .state
        .objects
        .get_mut(&equipment)
        .unwrap()
        .attached_to = Some(new_host);
    for host in [old_host, new_host] {
        g.engine.state.objects.get_mut(&host).unwrap().tapped = true;
    }
    for (spell, target) in spells.into_iter().zip([old_host, new_host]) {
        g.cast(spell, &[Target::Object(target)]);
    }
    assert_eq!(g.engine.state.players[&P0].life, 22);
}
