//! Araumi of the Dead Tide: "{T}, Exile cards from your graveyard equal to the number of
//! opponents you have: Target creature card in your graveyard gains encore until end of
//! turn. The encore cost is equal to its mana cost."

use super::harness::*;
use mtg_core::{Keyword, Target, Zone, ZoneRef};
use mtg_engine::actions::Action;

const ARAUMI: &str = "{T}, Exile cards from your graveyard equal to the number of opponents \
                      you have: Target creature card in your graveyard gains encore until end \
                      of turn. The encore cost is equal to its mana cost.";

fn activations(actions: &[Action], source: mtg_core::ObjectId) -> Vec<Action> {
    actions
        .iter()
        .filter(|a| matches!(a, Action::ActivateAbility { source: s, .. } if *s == source))
        .cloned()
        .collect()
}

#[test]
fn a_graveyard_creature_encores_for_its_mana_cost() {
    let mut t = Table::default();
    let araumi = t.card(
        "{1}{U}{B}",
        "Legendary Creature — Merfolk Wizard",
        Some((1, 4)),
        ARAUMI,
    );
    let ogre = t.card("{2}{G}", "Creature — Bear", Some((3, 3)), "");
    let filler = t.card("{0}", "Sorcery", None, "You gain 1 life.");
    let mut g = Game::new(t);
    g.lands(3);
    let a = g.put(araumi, P0, Zone::Battlefield);
    let ogre_card = g.put(ogre, P0, Zone::Graveyard);
    let filler = g.put(filler, P0, Zone::Graveyard);
    let actions = g.main();
    assert!(activations(&actions, ogre_card).is_empty(), "no encore yet");
    let grant = activations(&actions, a).pop().expect("offered");
    g.act(
        grant,
        &[Target::Object(ogre_card)],
        &[mtg_engine::choice::Answer::Objects(vec![filler])],
    );
    assert_eq!(
        g.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .len(),
        1,
        "one card for one opponent"
    );
    let actions = g.main();
    let encore = activations(&actions, ogre_card)
        .pop()
        .expect("encore granted");
    g.act(encore, &[], &[]);
    let tokens: Vec<_> = g
        .engine
        .state
        .battlefield()
        .into_iter()
        .filter(|o| g.engine.state.objects[o].is_token)
        .collect();
    assert_eq!(tokens.len(), 1, "one copy per opponent");
    assert_eq!(g.pt(tokens[0]), (3, 3));
    assert!(g.has(tokens[0], Keyword::Haste));
    assert_eq!(
        g.count(Zone::Graveyard, P0),
        0,
        "the creature card was exiled"
    );
    assert_eq!(
        g.engine
            .state
            .battlefield()
            .into_iter()
            .filter(|o| g.engine.state.objects[o].tapped)
            .count(),
        4,
        "Araumi and three lands"
    );
}
