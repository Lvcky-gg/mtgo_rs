//! Library qualities retain adjectives and explicitly printed permanent types.
use super::{
    dig::{resolve_selection, stack_top},
    harness::*,
};
use mtg_core::{CardId, ObjectId, Zone, ZoneRef};
use mtg_engine::actions::Action;

fn library_only(g: &mut Game, cards: &[CardId]) -> Vec<ObjectId> {
    let lib = ZoneRef::of(Zone::Library, P0);
    for id in g.engine.state.objects_in(lib) {
        g.engine.state.objects.remove(&id);
    }
    g.engine.state.zone_order.remove(&lib);
    stack_top(g, cards)
}

fn contains(g: &Game, card: CardId, zone: Zone) -> bool {
    let zr = if zone.is_shared() {
        ZoneRef::shared(zone)
    } else {
        ZoneRef::of(zone, P0)
    };
    g.engine
        .state
        .objects_in(zr)
        .iter()
        .any(|id| g.engine.state.objects[id].card == card)
}

#[test]
fn captain_sisay_can_find_legendary_instants_as_well_as_permanents() {
    let mut t = Table::default();
    let sisay = t.card("{2}{G}{W}", "Legendary Creature — Soldier", Some((2, 2)),
        "{T}: Search your library for a legendary card, reveal that card, put it into your hand, then shuffle.");
    let instant = t.card("{U}", "Legendary Instant", None, "Draw a card.");
    let artifact = t.card("{2}", "Legendary Artifact", None, "");
    let ordinary = t.bear();
    let mut g = Game::new(t);
    let source = g.put(sisay, P0, Zone::Battlefield);
    let legal = g.main();
    let activate = legal
        .into_iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: id, .. } if *id == source))
        .unwrap();
    let ids = library_only(&mut g, &[instant, ordinary, artifact]);
    resolve_selection(&mut g, activate, &[ids[0], ids[2]], 0, 1, vec![ids[0]]);
    assert!(contains(&g, instant, Zone::Hand));
    assert!(contains(&g, artifact, Zone::Library));
    assert!(contains(&g, ordinary, Zone::Library));
    assert_eq!(
        g.engine
            .state
            .revealed_cards
            .iter()
            .map(|c| c.card)
            .collect::<Vec<_>>(),
        vec![instant]
    );
}

#[test]
fn unmarked_grave_accepts_nonlegendary_spells_but_not_legendary_cards() {
    for find in [false, true] {
        let mut t = Table::default();
        let tutor = t.card("{1}{B}", "Sorcery", None,
            "Search your library for a nonlegendary card, put that card into your graveyard, then shuffle.");
        let instant = t.card("{U}", "Instant", None, "Draw a card.");
        let land = t.card("", "Land", None, "");
        let legend = t.card("{2}", "Legendary Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let ids = library_only(&mut g, &[instant, legend, land]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &[ids[0], ids[2]],
            0,
            1,
            if find { vec![ids[0]] } else { vec![] },
        );
        assert_eq!(contains(&g, instant, Zone::Graveyard), find);
        assert!(contains(&g, legend, Zone::Library));
        assert!(contains(&g, land, Zone::Library));
        assert_eq!(g.count(Zone::Graveyard, P0), if find { 2 } else { 1 });
    }
}

#[test]
fn adjective_only_searches_include_spells_without_relaxing_color_or_land_filters() {
    for (quality, candidates) in [
        ("blue", vec![0, 1]),
        ("nonland", vec![0, 1, 2]),
        ("noncreature, nonland", vec![0]),
    ] {
        let mut t = Table::default();
        let tutor = t.card("{G}", "Sorcery", None,
            &format!("Search your library for a {quality} card, reveal it, put it into your hand, then shuffle."));
        let instant = t.card("{U}", "Instant", None, "Draw a card.");
        let blue = t.card("{U}", "Creature — Bear", Some((1, 1)), "");
        let green = t.card("{G}", "Creature — Elf", Some((1, 1)), "");
        let land = t.card("", "Land", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let ids = library_only(&mut g, &[instant, blue, green, land]);
        let offered: Vec<_> = candidates.iter().map(|i| ids[*i]).collect();
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &offered,
            0,
            1,
            vec![ids[0]],
        );
        assert!(contains(&g, instant, Zone::Hand));
        assert!(contains(&g, land, Zone::Library));
        assert_eq!(g.engine.state.revealed_cards.len(), 1);
    }
}

#[test]
fn united_battlefront_excludes_instants_and_sorceries_and_checks_mana_value() {
    for lead in ["Look at", "Reveal"] {
        let mut t = Table::default();
        let battlefront = t.card("{2}{W}", "Sorcery", None,
            &format!("{lead} the top six cards of your library. Put up to two noncreature, nonland permanent cards with mana value 3 or less from among them onto the battlefield. Put the rest on the bottom of your library in a random order."));
        let artifact = t.card("{2}", "Artifact", None, "");
        let enchantment = t.card("{3}", "Enchantment", None, "");
        let instant = t.card("{U}", "Instant", None, "Draw a card.");
        let sorcery = t.card("{2}", "Sorcery", None, "Draw a card.");
        let expensive = t.card("{4}", "Artifact", None, "");
        let land = t.card("", "Land", None, "");
        let mut g = Game::new(t);
        g.lands(3);
        let spell = g.put(battlefront, P0, Zone::Hand);
        g.main();
        let ids = library_only(
            &mut g,
            &[artifact, enchantment, instant, sorcery, expensive, land],
        );
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..2],
            0,
            2,
            ids[..2].to_vec(),
        );
        assert!(contains(&g, artifact, Zone::Battlefield));
        assert!(contains(&g, enchantment, Zone::Battlefield));
        for card in [instant, sorcery, expensive, land] {
            assert!(contains(&g, card, Zone::Library));
        }
        assert_eq!(
            g.engine.state.revealed_cards.len(),
            if lead == "Reveal" { 6 } else { 0 }
        );
    }
}

#[test]
fn into_the_north_can_find_basic_or_nonbasic_snow_lands_and_enters_tapped() {
    for pick in [None, Some(0), Some(1)] {
        let mut t = Table::default();
        let north = t.card("{1}{G}", "Sorcery", None,
            "Search your library for a snow land card, put it onto the battlefield tapped, then shuffle.");
        let basic = t.card("", "Basic Snow Land — Forest", None, "");
        let nonbasic = t.card("", "Snow Land", None, "");
        let ordinary = t.card("", "Basic Land — Forest", None, "");
        let artifact = t.card("{2}", "Snow Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(2);
        let spell = g.put(north, P0, Zone::Hand);
        g.main();
        let ids = library_only(&mut g, &[basic, nonbasic, ordinary, artifact]);
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &ids[..2],
            0,
            1,
            pick.map_or_else(Vec::new, |i| vec![ids[i]]),
        );
        if let Some(i) = pick {
            let found = g.find([basic, nonbasic][i]).unwrap();
            assert!(g.engine.state.objects[&found].tapped);
        }
        assert!(g.find(ordinary).is_none());
        assert!(g.find(artifact).is_none());
        assert!(g.engine.state.revealed_cards.is_empty());
    }
}

#[test]
fn glacial_revelation_requires_both_snow_and_a_permanent_type() {
    let mut t = Table::default();
    let revelation = t.card("{2}{G}", "Sorcery", None,
        "Reveal the top six cards of your library. You may put any number of snow permanent cards from among them into your hand. Put the rest into your graveyard.");
    let artifact = t.card("{2}", "Snow Artifact", None, "");
    let creature = t.card("{G}", "Snow Creature — Bear", Some((1, 1)), "");
    let land = t.card("", "Snow Land", None, "");
    let instant = t.card("{U}", "Snow Instant", None, "Draw a card.");
    let sorcery = t.card("{2}", "Snow Sorcery", None, "Draw a card.");
    let ordinary = t.card("{2}", "Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let spell = g.put(revelation, P0, Zone::Hand);
    g.main();
    let ids = library_only(
        &mut g,
        &[artifact, creature, land, instant, sorcery, ordinary],
    );
    resolve_selection(
        &mut g,
        Action::Cast { object: spell },
        &ids[..3],
        0,
        3,
        ids[..3].to_vec(),
    );
    for card in [artifact, creature, land] {
        assert!(contains(&g, card, Zone::Hand));
    }
    for card in [instant, sorcery, ordinary] {
        assert!(contains(&g, card, Zone::Graveyard));
    }
    assert_eq!(g.count(Zone::Graveyard, P0), 4);
    assert_eq!(g.engine.state.revealed_cards.len(), 6);
}

#[test]
fn dead_of_winter_counts_only_your_snow_permanents_and_spares_snow_creatures() {
    let mut t = Table::default();
    let winter = t.card("{2}{B}", "Sorcery", None,
        "All nonsnow creatures get -X/-X until end of turn, where X is the number of snow permanents you control.");
    let snow = t.card("{G}", "Snow Creature — Bear", Some((1, 1)), "");
    let ordinary = t.bear();
    let large = t.card("{3}", "Creature — Bear", Some((3, 3)), "");
    let snow_artifact = t.card("{2}", "Snow Artifact", None, "");
    let mut g = Game::new(t);
    g.lands(3);
    let mine = g.put(snow, P0, Zone::Battlefield);
    let theirs = g.put(snow, P1, Zone::Battlefield);
    g.put(snow_artifact, P0, Zone::Battlefield);
    g.put(ordinary, P0, Zone::Battlefield);
    g.put(ordinary, P1, Zone::Battlefield);
    let survivor = g.put(large, P1, Zone::Battlefield);
    let spell = g.put(winter, P0, Zone::Hand);
    g.main();
    g.cast(spell, &[]);
    assert!(
        g.engine.state.objects.contains_key(&mine) && g.engine.state.objects.contains_key(&theirs)
    );
    assert!(
        g.engine.state.objects.contains_key(&survivor),
        "opponent's snow permanent must not raise X to three"
    );
    assert!(contains(&g, ordinary, Zone::Graveyard));
    assert_eq!(g.count(Zone::Graveyard, P1), 1);
}

#[test]
fn explicit_permanent_search_qualities_are_stricter_than_bare_adjectives() {
    for (quality, candidates) in [
        ("snow", vec![0, 1]),
        ("snow permanent", vec![1]),
        ("legendary permanent", vec![1, 2]),
    ] {
        let mut t = Table::default();
        let tutor = t.card("{G}", "Sorcery", None,
            &format!("Search your library for a {quality} card, reveal it, put it into your hand, then shuffle."));
        let instant = t.card("{U}", "Legendary Snow Instant", None, "Draw a card.");
        let artifact = t.card("{2}", "Legendary Snow Artifact", None, "");
        let land = t.card("", "Legendary Land", None, "");
        let ordinary = t.card("{2}", "Artifact", None, "");
        let mut g = Game::new(t);
        g.lands(1);
        let spell = g.put(tutor, P0, Zone::Hand);
        g.main();
        let ids = library_only(&mut g, &[instant, artifact, land, ordinary]);
        let offered: Vec<_> = candidates.iter().map(|i| ids[*i]).collect();
        resolve_selection(
            &mut g,
            Action::Cast { object: spell },
            &offered,
            0,
            1,
            vec![ids[1]],
        );
        assert!(contains(&g, artifact, Zone::Hand));
        assert!(contains(&g, instant, Zone::Library));
        assert!(contains(&g, land, Zone::Library));
        assert!(contains(&g, ordinary, Zone::Library));
    }
}

#[test]
fn rime_tender_offers_only_other_snow_permanents_and_untaps_the_chosen_one() {
    use mtg_core::Target;
    use mtg_engine::{
        Progress,
        choice::{Answer, ChoiceKind},
    };
    let mut t = Table::default();
    let tender = t.card(
        "{1}{G}",
        "Snow Creature — Elf",
        Some((2, 2)),
        "{T}: Untap another target snow permanent.",
    );
    let land = t.card("", "Snow Land", None, "");
    let artifact = t.card("{2}", "Snow Artifact", None, "");
    let ordinary = t.card("", "Land", None, "");
    let mut g = Game::new(t);
    let source = g.put(tender, P0, Zone::Battlefield);
    let snow_land = g.put(land, P0, Zone::Battlefield);
    let snow_artifact = g.put(artifact, P1, Zone::Battlefield);
    let nonsnow = g.put(ordinary, P0, Zone::Battlefield);
    let legal = g.main();
    for id in [snow_land, snow_artifact, nonsnow] {
        g.engine.state.objects.get_mut(&id).unwrap().tapped = true;
    }
    let activate = legal
        .into_iter()
        .find(|a| matches!(a, Action::ActivateAbility { source: id, .. } if *id == source))
        .unwrap();
    let priority = g.pending.take().unwrap();
    g.engine
        .answer(&g.table, priority.id, Answer::Action(activate))
        .unwrap();
    let mut asked = false;
    for _ in 0..1_000 {
        match g.engine.advance(&g.table) {
            Progress::NeedsChoice(c) => {
                if g.stack().is_empty() && matches!(c.kind, ChoiceKind::Priority { .. }) && asked {
                    break;
                }
                let answer = match &c.kind {
                    ChoiceKind::ChooseTargets { slots, .. } => {
                        assert_eq!(
                            slots,
                            &[vec![
                                Target::Object(snow_land),
                                Target::Object(snow_artifact)
                            ]]
                        );
                        asked = true;
                        Answer::Targets(vec![vec![Target::Object(snow_land)]])
                    }
                    _ => c.default.clone().unwrap_or(Answer::Pass),
                };
                g.engine.answer(&g.table, c.id, answer).unwrap();
            }
            Progress::Continue => {}
            Progress::GameOver { .. } => panic!("game ended"),
        }
    }
    assert!(asked);
    assert!(!g.engine.state.objects[&snow_land].tapped);
    assert!(g.engine.state.objects[&snow_artifact].tapped);
    assert!(g.engine.state.objects[&nonsnow].tapped);
    assert!(g.engine.state.objects[&source].tapped);
}

#[test]
fn an_ability_can_target_its_own_source_unless_it_says_another() {
    use mtg_core::Target;
    for another in [false, true] {
        let mut t = Table::default();
        let untapper = t.card(
            "{G}",
            "Creature — Elf",
            Some((1, 1)),
            if another {
                "{T}: Untap another target creature."
            } else {
                "{T}: Untap target creature."
            },
        );
        let mut g = Game::new(t);
        let source = g.put(untapper, P0, Zone::Battlefield);
        let legal = g.main();
        let activate = legal
            .into_iter()
            .find(|a| matches!(a, Action::ActivateAbility { source: id, .. } if *id == source));
        if another {
            assert!(
                activate.is_none(),
                "the source does not fill an another-target requirement"
            );
        } else {
            let activate = activate.expect("self-targeting is legal when not excluded");
            g.act(activate, &[Target::Object(source)], &[]);
            assert!(
                !g.engine.state.objects[&source].tapped,
                "the ability untaps its own source after paying the tap cost"
            );
            assert!(g.stack().is_empty());
        }
    }
}
