//! "Look at the top N cards of your library", take some, and put the rest away.
use super::harness::*;
use mtg_core::{Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

pub(super) fn stack_top(g: &mut Game, cards: &[mtg_core::CardId]) -> Vec<mtg_core::ObjectId> {
    let ids: Vec<_> = cards.iter().map(|c| g.put(*c, P0, Zone::Library)).collect();
    let lib = ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|o| !ids.contains(o));
    for (i, id) in ids.iter().enumerate() {
        order.insert(i, *id);
    }
    ids
}

#[test]
fn reveal_a_creature_and_bottom_the_rest() {
    let mut t = Table::default();
    let dig = t.card(
        "{G}",
        "Sorcery",
        None,
        "Look at the top three cards of your library. You may reveal a creature card from \
         among them and put it into your hand. Put the rest on the bottom of your library in \
         a random order.",
    );
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[rock, bear, rock]);
    g.act(
        Action::Cast { object: d },
        &[],
        &[Answer::Objects(vec![ids[1]])],
    );
    let hand: Vec<_> = g
        .engine
        .state
        .objects_in(ZoneRef::of(Zone::Hand, P0))
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert!(hand.contains(&bear), "the creature card");
    let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    let bottom: Vec<_> = lib[lib.len() - 2..]
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect();
    assert_eq!(bottom, vec![rock, rock], "the rest on the bottom");
}

#[test]
fn one_to_hand_and_the_rest_to_the_graveyard() {
    let mut t = Table::default();
    let dig = t.card(
        "{U}",
        "Sorcery",
        None,
        "Look at the top three cards of your library. Put one of them into your hand and the \
         rest into your graveyard.",
    );
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let d = g.put(dig, P0, Zone::Hand);
    g.main();
    let hand = g.count(Zone::Hand, P0);
    stack_top(&mut g, &[rock, rock, rock]);
    g.cast(d, &[]);
    assert_eq!(g.count(Zone::Hand, P0), hand - 1 + 1);
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        3,
        "two of them and the sorcery"
    );
}

#[test]
fn chosen_type_dig_selects_only_matching_cards() {
    use mtg_ir::PrintedCards;
    let mut t = Table::default();
    let icon = t.card(
        "{3}", "Artifact", None,
        "{1}, {T}: Look at the top three cards of your library. You may reveal a creature card of the chosen type from among them and put it into your hand. Put the rest on the bottom of your library in a random order.",
    );
    let bear = t.bear();
    let elf = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
    let mut g = Game::new(t);
    g.lands(1);
    let source = g.put(icon, P0, Zone::Battlefield);
    g.engine
        .state
        .objects
        .get_mut(&source)
        .unwrap()
        .chosen_subtype = g.table.subtype_named("Bear");
    let legal = g.main();
    let activate = legal
        .into_iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: id, .. } if *id == source))
        .unwrap();
    let ids = stack_top(&mut g, &[elf, bear, elf]);
    g.act(activate, &[], &[Answer::Objects(vec![ids[1]])]);
    let hand = g.engine.state.objects_in(ZoneRef::of(Zone::Hand, P0));
    assert!(
        hand.iter()
            .any(|id| g.engine.state.objects[id].card == bear)
    );
    assert!(!hand.iter().any(|id| g.engine.state.objects[id].card == elf));
}

#[test]
fn abbreviated_bottom_destination_keeps_the_rest_in_the_library() {
    let mut t = Table::default();
    let dig = t.card("{G}", "Sorcery", None,
        "Look at the top three cards of your library. You may reveal a creature card from among them and put it into your hand. Put the rest on the bottom in a random order.");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(1);
    let spell = g.put(dig, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[rock, bear, rock]);
    g.act(
        Action::Cast { object: spell },
        &[],
        &[Answer::Objects(vec![ids[1]])],
    );
    let lib = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
    assert!(
        lib[lib.len() - 2..]
            .iter()
            .all(|id| g.engine.state.objects[id].card == rock)
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 1);
}

#[test]
fn required_reveal_takes_a_matching_card_and_handles_no_matches() {
    for matches in [true, false] {
        let mut t = Table::default();
        let dig = t.card("{G}", "Sorcery", None,
            "Look at the top three cards of your library. Reveal a creature card from among them and put it into your hand. Put the rest on the bottom in a random order.");
        let bear = t.bear();
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(dig, P0, Zone::Hand);
        g.main();
        stack_top(&mut g, &[rock, if matches { bear } else { rock }, rock]);
        let hand = g.count(Zone::Hand, P0);
        g.cast(spell, &[]);
        assert_eq!(g.count(Zone::Hand, P0), hand - 1 + usize::from(matches));
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
    }
}

#[test]
fn collected_company_takes_zero_one_or_two_eligible_creatures() {
    for take in 0..=2 {
        let mut t = Table::default();
        let company = t.card("{3}{G}", "Instant", None,
            "Look at the top six cards of your library. Put up to two creature cards with mana value 3 or less from among them onto the battlefield. Put the rest on the bottom of your library in any order.");
        let bear = t.bear();
        let three = t.card("{3}", "Creature — Elf", Some((3, 3)), "");
        let four = t.card("{4}", "Creature — Bear", Some((4, 4)), "");
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(4);
        let spell = g.put(company, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[bear, three, four, rock, rock, rock]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..2],
            0,
            2,
            ids[..take].to_vec(),
        );
        let battlefield = g
            .engine
            .state
            .objects_in(ZoneRef::shared(Zone::Battlefield));
        assert_eq!(
            battlefield
                .iter()
                .filter(|id| {
                    let card = g.engine.state.objects[id].card;
                    card == bear || card == three
                })
                .count(),
            take
        );
        assert!(
            !battlefield
                .iter()
                .any(|id| g.engine.state.objects[id].card == four)
        );
        let library = g.engine.state.objects_in(ZoneRef::of(Zone::Library, P0));
        let bottom: Vec<_> = library[library.len() - (6 - take)..]
            .iter()
            .map(|id| g.engine.state.objects[id].card)
            .collect();
        assert_eq!(bottom, [bear, three, four, rock, rock, rock][take..]);
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
    }
}

/// Resolve through the first library-selection question, checking what the engine
/// offers rather than merely submitting a known legal answer.
pub(super) fn resolve_selection(
    g: &mut Game,
    action: Action,
    candidates: &[mtg_core::ObjectId],
    min: u32,
    max: u32,
    answer: Vec<mtg_core::ObjectId>,
) {
    g.act_holding(action, &[]);
    let priority = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, priority.id, Answer::Pass)
        .unwrap();
    let mut asked = false;
    for _ in 0..1_000 {
        match g.engine.advance(&g.table) {
            mtg_engine::Progress::NeedsChoice(c) => {
                if g.stack().is_empty()
                    && matches!(c.kind, mtg_engine::choice::ChoiceKind::Priority { .. })
                {
                    assert!(asked, "selection was offered");
                    g.pending = Some(c);
                    return;
                }
                let response = match &c.kind {
                    mtg_engine::choice::ChoiceKind::ChooseObjects {
                        from,
                        min: lo,
                        max: hi,
                    } if !asked => {
                        assert_eq!(from, candidates);
                        assert_eq!((*lo, *hi), (min, max));
                        asked = true;
                        Answer::Objects(answer.clone())
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, response).unwrap();
            }
            mtg_engine::Progress::Continue => {}
            mtg_engine::Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    panic!("selection did not finish");
}

fn cards_in(g: &Game, zone: Zone) -> Vec<mtg_core::CardId> {
    g.engine
        .state
        .objects_in(if zone.is_shared() {
            ZoneRef::shared(zone)
        } else {
            ZoneRef::of(zone, P0)
        })
        .iter()
        .map(|id| g.engine.state.objects[id].card)
        .collect()
}

#[test]
fn mulch_reveals_all_four_then_moves_every_land_and_mills_the_rest() {
    use mtg_core::Event;
    let mut t = Table::default();
    let mulch = t.card("{1}{G}", "Sorcery", None,
        "Reveal the top four cards of your library. Put all land cards revealed this way into your hand and the rest into your graveyard.");
    let forest = t.card("", "Basic Land — Forest", None, "");
    let island = t.card("", "Basic Land — Island", None, "");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let spell = g.put(mulch, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[forest, bear, island, rock, forest]);
    // Even a malformed empty answer cannot skip the mandatory matching cards.
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &[ids[0], ids[2]],
        2,
        2,
        vec![],
    );
    let hand = cards_in(&g, Zone::Hand);
    assert_eq!(hand.iter().filter(|c| **c == forest).count(), 1);
    assert!(hand.contains(&island));
    assert!(!hand.contains(&bear));
    let grave = cards_in(&g, Zone::Graveyard);
    assert!(grave.contains(&bear) && grave.contains(&rock) && grave.contains(&mulch));
    assert_eq!(
        g.engine
            .state
            .revealed_cards
            .iter()
            .map(|c| c.card)
            .collect::<Vec<_>>(),
        vec![forest, bear, island, rock]
    );
    assert_eq!(
        g.engine.view_for(P1).revealed_cards,
        g.engine.state.revealed_cards
    );
    assert!(!g.engine.view_for(P1).visible.contains_key(&ids[4]));
    let last_reveal = g
        .engine
        .log
        .iter()
        .rposition(|e| matches!(e.event, Event::Revealed { .. }))
        .unwrap();
    let first_move = g.engine.log.iter().position(|e| matches!(e.event,
        Event::ZoneChange { object, from, .. } if ids[..4].contains(&object) && from.zone == Zone::Library)).unwrap();
    assert!(last_reveal < first_move);
}

#[test]
fn grisly_salvage_selects_a_creature_a_land_or_nothing_from_only_the_top_five() {
    for pick in [None, Some(0), Some(1)] {
        let mut t = Table::default();
        let salvage = t.card("{B}{G}", "Instant", None,
            "Reveal the top five cards of your library. You may put a creature or land card from among them into your hand. Put the rest into your graveyard.");
        let bear = t.bear();
        let land = t.card("", "Land", None, "");
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(salvage, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[bear, land, rock, rock, rock, bear]);
        let opponent = g.put(bear, P1, Zone::Library);
        let answer = pick.map_or_else(Vec::new, |i| vec![opponent, ids[5], ids[i], ids[i]]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..2],
            0,
            1,
            answer,
        );
        let hand = cards_in(&g, Zone::Hand);
        assert_eq!(
            hand.iter().filter(|c| **c == bear || **c == land).count(),
            usize::from(pick.is_some())
        );
        if let Some(i) = pick {
            assert!(hand.contains(&[bear, land][i]));
        }
        assert_eq!(
            g.count(Zone::Graveyard, P0),
            6 - usize::from(pick.is_some())
        );
        assert_eq!(g.engine.state.revealed_cards.len(), 5);
        assert_eq!(g.engine.state.objects[&opponent].zone.zone, Zone::Library);
        assert_eq!(
            cards_in(&g, Zone::Library)
                .iter()
                .filter(|c| **c == bear)
                .count(),
            1
        );
    }
}

#[test]
fn goblin_ringleader_takes_all_goblin_cards_including_noncreatures_and_orders_the_rest() {
    let mut t = Table::default();
    let ringleader = t.card("{3}{R}", "Creature — Goblin", Some((2, 2)),
        "Haste\nWhen this creature enters, reveal the top four cards of your library. Put all Goblin cards revealed this way into your hand and the rest on the bottom of your library in any order.");
    let goblin = t.card("{R}", "Creature — Goblin", Some((1, 1)), "");
    let kindred = t.card("{2}", "Kindred Artifact — Goblin", None, "");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(4);
    let spell = g.put(ringleader, P0, Zone::Hand);
    g.main();
    stack_top(&mut g, &[goblin, bear, kindred, rock]);
    g.cast(spell, &[]);
    let hand = cards_in(&g, Zone::Hand);
    assert!(hand.contains(&goblin) && hand.contains(&kindred));
    assert!(!hand.contains(&bear) && !hand.contains(&rock));
    assert!(g.find(ringleader).is_some());
    let lib = cards_in(&g, Zone::Library);
    assert_eq!(&lib[lib.len() - 2..], &[bear, rock]);
    assert_eq!(g.engine.state.revealed_cards.len(), 4);
}

#[test]
fn pieces_of_the_puzzle_uses_a_shared_two_card_limit_for_instants_and_sorceries() {
    let mut t = Table::default();
    let pieces = t.card("{2}{U}", "Sorcery", None,
        "Reveal the top five cards of your library. Put up to two instant and/or sorcery cards from among them into your hand and the rest into your graveyard.");
    let instant = t.card("{U}", "Instant", None, "Draw a card.");
    let sorcery = t.card("{R}", "Sorcery", None, "Draw a card.");
    let bear = t.bear();
    let rock = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(pieces, P0, Zone::Hand);
    g.main();
    let ids = stack_top(&mut g, &[instant, sorcery, instant, bear, rock]);
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..3],
        0,
        2,
        vec![ids[0], ids[0], ids[1], ids[2]],
    );
    let hand = cards_in(&g, Zone::Hand);
    assert_eq!(
        hand.iter()
            .filter(|c| **c == instant || **c == sorcery)
            .count(),
        2
    );
    assert!(hand.contains(&instant) && hand.contains(&sorcery));
    assert_eq!(g.count(Zone::Graveyard, P0), 4);
    assert_eq!(g.engine.state.revealed_cards.len(), 5);
}

#[test]
fn enshrined_memories_reveals_x_and_keeps_all_creatures_even_with_a_short_library() {
    for (x, library_len) in [(0, 3), (3, 3), (5, 3)] {
        let mut t = Table::default();
        let memories = t.card("{X}{G}", "Sorcery", None,
            "Reveal the top X cards of your library. Put all creature cards revealed this way into your hand and the rest on the bottom of your library in any order.");
        let bear = t.bear();
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(x as usize + 1);
        let spell = g.put(memories, P0, Zone::Hand);
        g.main();
        let lib = ZoneRef::of(Zone::Library, P0);
        // The fixture normally supplies filler cards; this scenario needs a genuinely
        // short library to check that X cannot over-read it.
        let existing = g.engine.state.objects_in(lib);
        for id in existing {
            g.engine.state.objects.remove(&id);
        }
        g.engine.state.zone_order.remove(&lib);
        stack_top(&mut g, &[bear, rock, bear][..library_len]);
        g.act(Action::Cast { object: spell }, &[], &[Answer::Number(x)]);
        assert_eq!(
            g.engine.state.revealed_cards.len(),
            (x as usize).min(library_len)
        );
        let hand = cards_in(&g, Zone::Hand);
        assert_eq!(
            hand.iter().filter(|c| **c == bear).count(),
            if x == 0 { 0 } else { 2 }
        );
        assert!(!hand.contains(&rock));
        assert_eq!(g.count(Zone::Library, P0), if x == 0 { 3 } else { 1 });
    }
}

#[test]
fn muxus_puts_only_matching_goblin_creatures_into_play_and_runs_entry_effects() {
    let mut t = Table::default();
    let muxus = t.card("{4}{R}{R}", "Creature — Goblin", Some((4, 4)),
        "When this creature enters, reveal the top six cards of your library. Put all Goblin creature cards with mana value 5 or less from among them onto the battlefield and the rest on the bottom of your library in a random order.");
    let counters = t.card(
        "{1}{R}",
        "Creature — Goblin",
        Some((1, 1)),
        "This creature enters with two +1/+1 counters on it.",
    );
    let tapped = t.card(
        "{4}{R}",
        "Creature — Goblin",
        Some((3, 3)),
        "This creature enters tapped.",
    );
    let draw = t.card(
        "{R}",
        "Creature — Goblin",
        Some((1, 1)),
        "When this creature enters, draw a card.",
    );
    let expensive = t.card("{5}{R}", "Creature — Goblin", Some((4, 4)), "");
    let kindred = t.card("{1}{R}", "Kindred Artifact — Goblin", None, "");
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(6);
    let spell = g.put(muxus, P0, Zone::Hand);
    g.main();
    stack_top(&mut g, &[counters, tapped, draw, expensive, kindred, bear]);
    let hand = g.count(Zone::Hand, P0);
    g.cast(spell, &[]);
    let counter_id = g.find(counters).expect("the two-mana Goblin enters");
    assert_eq!(
        g.engine.state.objects[&counter_id].counters[&mtg_core::CounterKind::PlusOnePlusOne],
        2
    );
    let tapped_id = g.find(tapped).expect("the five-mana Goblin enters");
    assert!(g.engine.state.objects[&tapped_id].tapped);
    assert!(g.find(draw).is_some());
    assert_eq!(
        g.count(Zone::Hand, P0),
        hand,
        "ETB draw offsets casting Muxus"
    );
    assert!(g.find(expensive).is_none());
    assert!(g.find(kindred).is_none());
    assert!(g.find(bear).is_none());
    let lib = cards_in(&g, Zone::Library);
    let bottom = &lib[lib.len() - 3..];
    assert!(bottom.contains(&expensive) && bottom.contains(&kindred) && bottom.contains(&bear));
    assert_eq!(g.engine.state.revealed_cards.len(), 6);
}

#[test]
fn reveal_dig_does_not_accept_unsupported_quotas_attacking_entry_or_undefined_x() {
    use mtg_oracle::compile::{FaceText, SubtypeNames, compile};
    for text in [
        "Reveal the top four cards of your library. Put a creature card and/or a land card from among them into your hand. Put the rest into your graveyard.",
        "Reveal the top four cards of your library. Put up to two creature cards with total mana value 3 or less from among them onto the battlefield and the rest into your graveyard.",
        "Reveal the top four cards of your library. Put all creature cards from among them onto the battlefield tapped and attacking and the rest into your graveyard.",
        "Reveal the top X cards of your library. Put all creature cards revealed this way into your hand and the rest into your graveyard.",
        "Look at the top four cards of your library. Put all creature cards revealed this way into your hand and the rest into your graveyard.",
    ] {
        let face = FaceText {
            name: "Unsupported Selection",
            card_types: &[mtg_core::CardType::Sorcery],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "{G}",
        };
        assert!(
            !compile(&face, &SubtypeNames(vec![])).understood(),
            "{text}"
        );
    }
}

#[test]
fn selected_permanents_ask_their_entry_choices_without_duplicating_reveals() {
    for reveal in [false, true] {
        let mut t = Table::default();
        let text = format!(
            "{} the top three cards of your library. Put a creature card from among them onto the battlefield and the rest on the bottom of your library in a random order.",
            if reveal { "Reveal" } else { "Look at" }
        );
        let dig = t.card("{G}", "Sorcery", None, &text);
        let chooser = t.card(
            "{1}",
            "Creature — Bear",
            Some((1, 1)),
            "As this creature enters, choose a color.",
        );
        let rock = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(dig, P0, Zone::Hand);
        g.main();
        let ids = stack_top(&mut g, &[chooser, rock, rock]);
        g.act(
            Action::Cast { object: spell },
            &[],
            &[Answer::Objects(vec![ids[0]]), Answer::Modes(vec![2])],
        );
        let source = g.find(chooser).expect("chosen creature enters");
        assert_eq!(
            g.engine.state.objects[&source].chosen_color,
            Some(mtg_core::Color::Black)
        );
        assert_eq!(
            g.engine.state.revealed_cards.len(),
            if reveal { 3 } else { 0 }
        );
        assert_eq!(g.count(Zone::Graveyard, P0), 1);
    }
}
