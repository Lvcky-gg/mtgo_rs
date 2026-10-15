//! Mnemonic Betrayal: "Exile all opponents' graveyards. You may cast spells from among those
//! cards this turn, and mana of any type can be spent to cast them. At the beginning of the
//! next end step, if any of those cards remain exiled, return them to their owners'
//! graveyards. Exile Mnemonic Betrayal."

use super::harness::*;
use mtg_core::{Step, Zone, ZoneRef};
use mtg_engine::{actions::Action, choice::Answer};

const BETRAYAL: &str = "Exile all opponents' graveyards. You may cast spells from among those \
                        cards this turn, and mana of any type can be spent to cast them. At \
                        the beginning of the next end step, if any of those cards remain \
                        exiled, return them to their owners' graveyards.\nExile ~.";

#[test]
fn borrows_the_opponents_graveyard_for_a_turn() {
    let mut t = Table::default();
    let betrayal = t.card("{0}", "Sorcery", None, BETRAYAL);
    let gift = t.card("{G}{G}", "Sorcery", None, "You gain 4 life.");
    let land = t.card("", "Land", None, "");
    let colorless = t.card("", "Land", None, "{T}: Add {C}.");
    let mut g = Game::new(t);
    g.put(colorless, P0, Zone::Battlefield);
    g.put(colorless, P0, Zone::Battlefield);
    g.put(gift, P1, Zone::Graveyard);
    g.put(land, P1, Zone::Graveyard);
    let betrayal = g.put(betrayal, P0, Zone::Hand);
    g.main();
    g.cast(betrayal, &[]);
    assert_eq!(g.count(Zone::Graveyard, P1), 0);
    assert_eq!(g.count(Zone::Graveyard, P0), 0, "Betrayal exiles itself");
    let exiled = |g: &Game, card| {
        g.engine
            .state
            .objects_in(ZoneRef::shared(Zone::Exile))
            .into_iter()
            .find(|o| g.engine.state.objects[o].card == card)
    };
    let gift_obj = exiled(&g, gift).expect("exiled");
    let land_obj = exiled(&g, land).expect("exiled");
    let actions = g.main();
    assert!(
        actions.contains(&Action::Cast { object: gift_obj }),
        "colorless pays {{G}}{{G}}"
    );
    assert!(
        !actions.contains(&Action::PlayLand { object: land_obj }),
        "spells only"
    );
    g.cast(gift_obj, &[]);
    assert_eq!(g.life(P0), 24);
    assert_eq!(
        g.count(Zone::Graveyard, P1),
        1,
        "the cast spell goes to its owner"
    );
    // At the end step, the land goes back.
    let c = g.pending.take().unwrap();
    g.engine.answer(&g.table, c.id, Answer::Pass).unwrap();
    g.drive(
        |_, _| None,
        |g| g.engine.state.step == Step::Cleanup || g.engine.state.active_player == P1,
    );
    assert_eq!(g.count(Zone::Graveyard, P1), 2);
}
