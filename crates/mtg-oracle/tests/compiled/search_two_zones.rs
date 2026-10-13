use super::harness::*;
use mtg_core::{Event, Keyword, Step, Zone};
use mtg_engine::choice::ChoiceKind;
use mtg_engine::{actions::Action, choice::Answer};

const FINALE: &str = "Search your library and/or graveyard for a creature card with mana value X or less and put it onto the battlefield. If you search your library this way, shuffle. If X is 10 or more, creatures you control get +X/+X and gain haste until end of turn.";

#[test]
fn finale_searches_either_zone_or_both_and_shuffles_only_a_searched_library() {
    for mode in 0..3 {
        let mut t = Table::default();
        let finale = t.card("{X}{G}{G}", "Sorcery", None, FINALE);
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(4);
        let spell = g.put(finale, P0, Zone::Hand);
        let library = g.put(bear, P0, Zone::Library);
        let graveyard = g.put(bear, P0, Zone::Graveyard);
        g.main();
        let selected = if mode == 0 { library } else { graveyard };
        g.act(
            Action::Cast { object: spell },
            &[],
            &[
                Answer::Number(2),
                Answer::Modes(vec![mode]),
                Answer::Objects(vec![selected]),
            ],
        );
        assert_eq!(g.pt(g.find(bear).unwrap()), (2, 2));
        assert_eq!(
            g.engine
                .log
                .iter()
                .filter(|e| matches!(e.event, Event::Shuffled { player: P0 }))
                .count(),
            usize::from(mode != 1)
        );
    }
}

#[test]
fn finale_at_ten_buffs_the_found_creature_and_other_creatures_until_end_of_turn() {
    let mut t = Table::default();
    let finale = t.card("{X}{G}{G}", "Sorcery", None, FINALE);
    let bear = t.bear();
    let other = t.card("{0}", "Creature", Some((1, 3)), "");
    let mut g = Game::new(t);
    g.lands(12);
    let spell = g.put(finale, P0, Zone::Hand);
    let found = g.put(bear, P0, Zone::Graveyard);
    let other = g.put(other, P0, Zone::Battlefield);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Number(10),
            Answer::Modes(vec![1]),
            Answer::Objects(vec![found]),
        ],
    );
    let found = g.find(bear).unwrap();
    assert_eq!(g.pt(found), (12, 12));
    assert_eq!(g.pt(other), (11, 13));
    assert!(g.has(found, Keyword::Haste));
    g.until(P1, Step::PrecombatMain);
    assert_eq!(g.pt(found), (2, 2));
    assert_eq!(g.pt(other), (1, 3));
    assert!(!g.has(found, Keyword::Haste));
}

#[test]
fn a_failed_library_search_still_shuffles() {
    let mut t = Table::default();
    let finale = t.card("{X}{G}{G}", "Sorcery", None, FINALE);
    let mut g = Game::new(t);
    g.lands(4);
    let spell = g.put(finale, P0, Zone::Hand);
    g.main();
    g.act(
        Action::Cast { object: spell },
        &[],
        &[
            Answer::Number(2),
            Answer::Modes(vec![0]),
            Answer::Objects(vec![]),
        ],
    );
    assert!(
        g.engine
            .log
            .iter()
            .any(|e| e.event == Event::Shuffled { player: P0 })
    );
}

#[test]
fn delivery_moogle_reveals_an_artifact_within_its_mana_value_limit() {
    let mut t = Table::default();
    let moogle = t.card("{0}", "Creature", Some((2, 2)), "When ~ enters, search your library and/or graveyard for an artifact card with mana value 2 or less, reveal it, and put it into your hand. If you search your library this way, shuffle.");
    let artifact = t.card("{2}", "Artifact", None, "");
    let too_large = t.card("{3}", "Artifact", None, "");
    let mut g = Game::new(t);
    let spell = g.put(moogle, P0, Zone::Hand);
    let found = g.put(artifact, P0, Zone::Graveyard);
    g.put(too_large, P0, Zone::Graveyard);
    g.main();
    let before = g.count(Zone::Hand, P0);
    g.act_holding(Action::Cast { object: spell }, &[]);
    let pending = g.pending.take().unwrap();
    g.engine.answer(&g.table, pending.id, Answer::Pass).unwrap();
    g.drive(
        |_, c| match &c.kind {
            ChoiceKind::ChooseModes { .. } => Some(Answer::Modes(vec![1])),
            ChoiceKind::ChooseObjects { from, min, max } => {
                assert_eq!(from, &vec![found], "larger artifacts are excluded");
                assert_eq!(
                    (*min, *max),
                    (1, 1),
                    "a public-zone search must find a matching card"
                );
                Some(Answer::Objects(vec![found]))
            }
            _ => None,
        },
        |g| g.stack().is_empty(),
    );
    assert_eq!(g.count(Zone::Hand, P0), before);
    assert!(
        g.engine
            .log
            .iter()
            .any(|e| e.event == Event::Revealed { object: found })
    );
    assert!(
        !g.engine
            .log
            .iter()
            .any(|e| e.event == Event::Shuffled { player: P0 })
    );
}
