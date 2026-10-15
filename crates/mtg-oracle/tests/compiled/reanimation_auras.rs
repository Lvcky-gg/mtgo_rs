//! Animate Dead and Necromancy: return a creature card from a graveyard, enchant it, and
//! sacrifice it when the Aura leaves.

use super::harness::*;
use mtg_core::{Target, Zone};

const ANIMATE: &str = "Enchant creature card in a graveyard\nWhen this Aura enters, if it's on \
                       the battlefield, it loses \"enchant creature card in a graveyard\" and \
                       gains \"enchant creature put onto the battlefield with this Aura.\" \
                       Return enchanted creature card to the battlefield under your control \
                       and attach this Aura to it. When this Aura leaves the battlefield, that \
                       creature's controller sacrifices it.\nEnchanted creature gets -1/-0.";
const NECROMANCY: &str = "You may cast this spell as though it had flash. If you cast it any \
                          time a sorcery couldn't have been cast, the controller of the \
                          permanent it becomes sacrifices it at the beginning of the next \
                          cleanup step.\nWhen this enchantment enters, if it's on the \
                          battlefield, it becomes an Aura with \"enchant creature put onto the \
                          battlefield with ~.\" Put target creature card from a graveyard onto \
                          the battlefield under your control and attach this enchantment to \
                          it. When this enchantment leaves the battlefield, that creature's \
                          controller sacrifices it.";
const SHATTER: &str = "Destroy target enchantment.";

fn controller(g: &Game, id: mtg_core::ObjectId) -> mtg_core::PlayerId {
    mtg_engine::layers::controller(&g.engine.state, id).unwrap()
}

#[test]
fn animate_dead_steals_from_a_graveyard_until_it_leaves() {
    let mut t = Table::default();
    let animate = t.card("{0}", "Enchantment — Aura", None, ANIMATE);
    let shatter = t.card("{0}", "Instant", None, SHATTER);
    let ogre = t.card("{2}{G}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    let corpse = g.put(ogre, P1, Zone::Graveyard);
    let animate_card = animate;
    let animate = g.put(animate, P0, Zone::Hand);
    let shatter = g.put(shatter, P0, Zone::Hand);
    g.main();
    g.cast(animate, &[Target::Object(corpse)]);
    let back = g.find(ogre).expect("returned");
    assert_eq!(controller(&g, back), P0);
    assert_eq!(g.pt(back), (2, 3), "-1/-0");
    let aura = g.find(animate_card).expect("still on the battlefield");
    assert_eq!(g.engine.state.objects[&aura].attached_to, Some(back));
    g.cast(shatter, &[Target::Object(aura)]);
    assert!(g.find(ogre).is_none(), "sacrificed with the Aura");
    assert_eq!(
        g.count(Zone::Graveyard, P1),
        1,
        "back to its owner's graveyard"
    );
}

#[test]
fn necromancy_becomes_an_aura_on_what_it_returns() {
    let mut t = Table::default();
    let necro = t.card("{0}", "Enchantment", None, NECROMANCY);
    let kill = t.card("{0}", "Instant", None, "Destroy target creature.");
    let ogre = t.card("{2}{G}", "Creature — Bear", Some((3, 3)), "");
    let mut g = Game::new(t);
    let corpse = g.put(ogre, P0, Zone::Graveyard);
    let necro_card = necro;
    let necro = g.put(necro, P0, Zone::Hand);
    let kill = g.put(kill, P0, Zone::Hand);
    g.main();
    g.cast(necro, &[Target::Object(corpse)]);
    let back = g.find(ogre).expect("returned");
    assert_eq!(g.pt(back), (3, 3));
    let aura = g.find(necro_card).expect("attached");
    assert_eq!(g.engine.state.objects[&aura].attached_to, Some(back));
    // The creature dies: the Aura has nothing to enchant and goes too.
    g.cast(kill, &[Target::Object(back)]);
    assert!(g.find(necro_card).is_none());
}
