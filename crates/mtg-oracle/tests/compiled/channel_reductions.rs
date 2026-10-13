use super::harness::*;
use mtg_core::{Target, Zone};

const OTAWARA: &str = "{T}: Add {U}.\nChannel — {3}{U}, Discard this card: Return target artifact, creature, enchantment, or planeswalker to its owner's hand. This ability costs {1} less to activate for each legendary creature you control.";
const BOSEIJU: &str = "{T}: Add {G}.\nChannel — {1}{G}, Discard this card: Destroy target artifact, enchantment, or nonbasic land an opponent controls. That player may search their library for a land card with a basic land type, put it onto the battlefield, then shuffle. This ability costs {1} less to activate for each legendary creature you control.";

#[test]
fn boseiju_lets_the_destroyed_permanents_controller_find_a_nonbasic_dual_land() {
    let mut t = Table::default();
    let channel = t.card("", "Legendary Land", None, BOSEIJU);
    let artifact = t.card("{2}", "Artifact", None, "");
    let dual = t.card("", "Land — Forest Island", None, "");
    let mut g = Game::new(t);
    g.lands(2);
    let target = g.put(artifact, P0, Zone::Battlefield);
    g.engine.state.objects.get_mut(&target).unwrap().controller = P1;
    let land = g.put(dual, P1, Zone::Library);
    let card = g.put(channel, P0, Zone::Hand);
    g.main();
    g.act(
        activate(card, 1),
        &[Target::Object(target)],
        &[
            mtg_engine::choice::Answer::Bool(true),
            mtg_engine::choice::Answer::Objects(vec![land]),
        ],
    );
    assert_eq!(g.count(Zone::Graveyard, P0), 2);
    let fetched = g.find(dual).unwrap();
    assert_eq!(g.engine.state.objects[&fetched].controller, P1);
    assert!(!g.engine.state.objects[&fetched].tapped);
}

#[test]
fn channel_reduction_counts_only_your_legendary_creatures_and_keeps_colored_cost() {
    for (mine, theirs, mana, offered) in [
        (0, 0, 4, true),
        (0, 3, 1, false),
        (2, 0, 1, false),
        (2, 0, 2, true),
        (3, 0, 1, true),
        (4, 0, 0, false),
    ] {
        let mut t = Table::default();
        let channel = t.card("", "Legendary Land", None, OTAWARA);
        let legends: Vec<_> = (0..mine + theirs)
            .map(|_| t.card("{2}", "Legendary Creature", Some((2, 2)), ""))
            .collect();
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(mana);
        for (i, legend) in legends.into_iter().enumerate() {
            g.put(legend, if i < mine { P0 } else { P1 }, Zone::Battlefield);
        }
        let target = g.put(bear, P1, Zone::Battlefield);
        let card = g.put(channel, P0, Zone::Hand);
        let actions = g.main();
        assert_eq!(actions.contains(&activate(card, 1)), offered);
        if offered {
            g.act(activate(card, 1), &[Target::Object(target)], &[]);
            assert_eq!(g.count(Zone::Hand, P1), 1);
            assert_eq!(g.count(Zone::Graveyard, P0), 1);
        }
    }
}

#[test]
fn boseiju_cannot_target_a_basic_land_or_your_own_permanent() {
    for own in [false, true] {
        let mut t = Table::default();
        let channel = t.card("", "Legendary Land", None, BOSEIJU);
        let target = if own {
            t.card("{2}", "Artifact", None, "")
        } else {
            t.mountain()
        };
        let mut g = Game::new(t);
        g.lands(2);
        g.put(target, if own { P0 } else { P1 }, Zone::Battlefield);
        let card = g.put(channel, P0, Zone::Hand);
        assert!(!g.main().contains(&activate(card, 1)));
    }
}
