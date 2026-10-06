//! "Deals N damage divided as you choose among …" (CR 601.2d): the division is announced
//! with the targets, one slot per point of damage.
use super::harness::*;
use mtg_core::{Target, Zone};

fn damage(g: &Game, id: mtg_core::ObjectId) -> u32 {
    g.engine.state.objects[&id].damage
}

#[test]
fn all_of_it_to_one_target_or_split_between_two() {
    for split in [false, true] {
        let mut t = Table::default();
        let bolt = t.card(
            "{R}",
            "Sorcery",
            None,
            "~ deals 2 damage divided as you choose among one or two targets.",
        );
        let bear = t.bear();
        let mut g = Game::new(t);
        g.lands(1);
        let s = g.put(bolt, P0, Zone::Hand);
        let b = g.put(bear, P1, Zone::Battlefield);
        g.main();
        if split {
            g.cast(s, &[Target::Object(b), Target::Player(P1)]);
            assert_eq!(damage(&g, b), 1);
            assert_eq!(g.life(P1), 19);
        } else {
            g.cast(s, &[Target::Object(b), Target::Object(b)]);
            assert!(
                !g.engine.state.objects.contains_key(&b),
                "2 damage killed it"
            );
            assert_eq!(g.life(P1), 20);
        }
    }
}

#[test]
fn a_target_named_twice_is_dealt_its_damage_in_one_event_and_targeted_once() {
    let mut t = Table::default();
    let bolt = t.card(
        "{R}",
        "Sorcery",
        None,
        "~ deals 2 damage divided as you choose among one or two targets.",
    );
    let troll = t.card(
        "{2}{G}",
        "Creature — Bear",
        Some((4, 4)),
        "Whenever this creature is dealt damage, you gain 1 life.\nWhenever this creature \
         becomes the target of a spell or ability, you gain 1 life.",
    );
    let mut g = Game::new(t);
    g.lands(1);
    let s = g.put(bolt, P0, Zone::Hand);
    let tr = g.put(troll, P1, Zone::Battlefield);
    g.main();
    g.cast(s, &[Target::Object(tr), Target::Object(tr)]);
    assert_eq!(damage(&g, tr), 2);
    assert_eq!(
        g.life(P1),
        22,
        "one targeting trigger and one damage trigger"
    );
}

#[test]
fn any_number_of_target_creatures_one_point_each() {
    let mut t = Table::default();
    let rain = t.card(
        "{2}{R}",
        "Instant",
        None,
        "~ deals 3 damage divided as you choose among any number of target creatures.",
    );
    let bear = t.bear();
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(rain, P0, Zone::Hand);
    let bs: Vec<_> = (0..3).map(|_| g.put(bear, P1, Zone::Battlefield)).collect();
    g.main();
    let targets: Vec<_> = bs.iter().map(|b| Target::Object(*b)).collect();
    g.cast(s, &targets);
    assert!(bs.iter().all(|b| damage(&g, *b) == 1));
}
