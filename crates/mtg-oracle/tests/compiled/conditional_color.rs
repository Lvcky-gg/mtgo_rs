use super::harness::*;
use mtg_core::{Color, Target, Zone};
use mtg_engine::actions::Action;

#[test]
fn printed_blast_modes_compile_and_destroy_the_matching_color() {
    for (color, cost) in [("blue", "{U}"), ("red", "{R}")] {
        let mut t = Table::default();
        let blast = t.card("{0}", "Instant", None, &format!("Choose one —\n• Counter target spell if it's {color}.\n• Destroy target permanent if it's {color}."));
        let creature = t.card(cost, "Creature", Some((2, 2)), "");
        let mut g = Game::new(t);
        let target = g.put(creature, P1, Zone::Battlefield);
        let spell = g.put(blast, P0, Zone::Hand);
        g.main();
        g.act(
            Action::Cast { object: spell },
            &[Target::Object(target)],
            &[mtg_engine::choice::Answer::Modes(vec![1])],
        );
        assert!(!g.engine.state.objects.contains_key(&target));
    }
}

#[test]
fn conditional_removal_can_target_any_color_but_only_destroys_blue() {
    for (cost, destroyed) in [("{U}", true), ("{G}", false), ("{U}{G}", true)] {
        let mut t = Table::default();
        let blast = t.card(
            "{0}",
            "Instant",
            None,
            "Destroy target permanent if it's blue.",
        );
        let creature = t.card(cost, "Creature", Some((2, 2)), "");
        let mut g = Game::new(t);
        let target = g.put(creature, P1, Zone::Battlefield);
        let spell = g.put(blast, P0, Zone::Hand);
        assert!(g.main().contains(&Action::Cast { object: spell }));
        g.cast(spell, &[Target::Object(target)]);
        assert_eq!(g.engine.state.objects.contains_key(&target), !destroyed);
        assert_eq!(g.count(Zone::Graveyard, P1), usize::from(destroyed));
    }
}

#[test]
fn conditional_counter_can_target_nonblue_spells() {
    for (cost, life) in [("{U}", 20), ("{G}", 23)] {
        let mut t = Table::default();
        let original = t.card(cost, "Sorcery", None, "You gain 3 life.");
        let blast = t.card("{0}", "Instant", None, "Counter target spell if it's blue.");
        let mut g = Game::new(t);
        g.lands(2);
        // Supply the original spell's color directly; payment consumes it on cast.
        g.engine.state.players.get_mut(&P0).unwrap().mana.add(
            Some(if cost == "{U}" {
                Color::Blue
            } else {
                Color::Green
            }),
            1,
        );
        let original = g.put(original, P0, Zone::Hand);
        let counter = g.put(blast, P0, Zone::Hand);
        g.main();
        g.act_holding(Action::Cast { object: original }, &[]);
        let on_stack = g.stack()[0];
        g.cast(counter, &[Target::Object(on_stack)]);
        assert_eq!(g.life(P0), life);
    }
}

#[test]
fn conditional_removal_reads_color_at_resolution() {
    let mut t = Table::default();
    let blast = t.card(
        "{0}",
        "Instant",
        None,
        "Destroy target permanent if it's blue.",
    );
    let creature = t.card("{G}", "Creature", Some((2, 2)), "");
    let blue = t.card(
        "{0}",
        "Instant",
        None,
        "Choose a color. Target creature becomes the chosen color until end of turn.",
    );
    let mut g = Game::new(t);
    let target = g.put(creature, P1, Zone::Battlefield);
    let spell = g.put(blast, P0, Zone::Hand);
    let color = g.put(blue, P0, Zone::Hand);
    g.main();
    g.act_holding(Action::Cast { object: spell }, &[Target::Object(target)]);
    g.act(
        Action::Cast { object: color },
        &[Target::Object(target)],
        &[mtg_engine::choice::Answer::Modes(vec![1])],
    );
    assert!(!g.engine.state.objects.contains_key(&target));
}
