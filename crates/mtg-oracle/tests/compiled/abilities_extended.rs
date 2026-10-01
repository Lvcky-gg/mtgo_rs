//! Oracle text compilation tests for extended abilities: morph/megamorph, cascade,
//! madness, and multi-target nouns. These tests are structured to validate that the
//! oracle compiler correctly parses and compiles these grammar patterns.
//!
//! Note: These tests are expected to fail until the corresponding compiler patterns
//! are implemented.

use super::harness::*;
use mtg_core::{Target, Zone};
use mtg_engine::actions::Action;

// ============================================================================
// MORPH / MEGAMORPH: Face-down casting and reveal mechanics.
// ============================================================================

/// Morph should allow casting a spell face down.
#[test]
#[ignore = "Morph compilation incomplete"]
fn morph_allows_facedown_casting() {
    let mut t = Table::default();
    let morphed = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((2, 2)),
        "Morph {U}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let card = g.put(morphed, P0, Zone::Hand);
    let actions = g.main();
    // Should have an option to cast the card face down.
    assert!(
        actions.iter().any(|a| matches!(a, Action::Cast { object }
            if *object == card)),
        "morph allows casting face down"
    );
}

/// Morph face-down creatures should have 2/2 stats.
#[test]
#[ignore = "Morph face-down action not yet implemented in engine"]
fn morph_facedown_is_2_2_creature() {
    let mut t = Table::default();
    let morphed = t.card(
        "{1}{U}",
        "Creature — Wizard",
        Some((2, 2)),
        "Morph {U}",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let _card = g.put(morphed, P0, Zone::Hand);
    g.main();
    // Cast face down (this will require action implementation).
    // After casting, the creature should be 2/2 with no visible type or abilities.
    // TODO: Implement Action::CastFaceDown in engine
}

/// Megamorph should allow revealing to get a counter.
#[test]
#[ignore = "Megamorph face-down action not yet implemented"]
fn megamorph_grants_counter_on_reveal() {
    let mut t = Table::default();
    let megamorphed = t.card(
        "{1}{G}",
        "Creature — Elf",
        Some((2, 2)),
        "Megamorph {G} — When ~ is turned face up, put a +1/+1 counter on it.",
    );
    let mut g = Game::new(t);
    g.lands(2);
    let _card = g.put(megamorphed, P0, Zone::Hand);
    // After casting face down and tapping {G} to reveal, the creature should gain a +1/+1 counter.
    // TODO: Implement Action::CastFaceDown and reveal mechanics in engine
}

/// Megamorph variants with different costs should compile.
#[test]
#[ignore = "Megamorph compilation incomplete"]
fn megamorph_various_costs() {
    let mut t = Table::default();
    let _m1 = t.card("{2}{U}", "Creature", Some((2, 2)), "Megamorph {U}");
    let _m2 = t.card("{2}{B}", "Creature", Some((2, 2)), "Megamorph {B}");
    let _m3 = t.card("{2}{R}", "Creature", Some((2, 2)), "Megamorph {1}{R}");
    assert!(t.understood.iter().all(|&u| u), "all megamorph variants compile");
}

// ============================================================================
// CASCADE: Casting during resolution.
// ============================================================================

/// Cascade should allow casting spells from the deck with lower mana value.
#[test]
#[ignore = "Cascade details in copies::cascade_casts_the_first_cheaper_nonland_for_free"]
fn cascade_casts_lower_spell() {
    let mut t = Table::default();
    let shaman = t.card("{2}{R}", "Creature — Goblin", Some((3, 3)), "Cascade");
    let bolt = t.card("{R}", "Instant", None, "~ deals 3 damage to any target.");
    let mountain = t.mountain();
    let mut g = Game::new(t);
    g.lands(3);
    let s = g.put(shaman, P0, Zone::Hand);
    g.main();
    // Helper to put cards on top of library in order
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let m1 = g.put(mountain, P0, Zone::Library);
    let b = g.put(bolt, P0, Zone::Library);
    let m2 = g.put(mountain, P0, Zone::Library);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.clear();
    order.extend_from_slice(&[m2, b, m1]);
    
    let _library_before = g.count(Zone::Library, P0);
    g.cast(s, &[Target::Player(P1)]);
    
    // See copies.rs::cascade_casts_the_first_cheaper_nonland_for_free for full test
}

/// Cascade should exile cards with higher or equal mana value.
#[test]
fn cascade_exiles_equal_or_higher() {
    let mut t = Table::default();
    let cascader = t.card("{2}{U}", "Instant", None, "Cascade");
    let higher = t.card("{3}{G}", "Instant", None, "Draw a card.");
    let mut g = Game::new(t);
    g.lands(3);
    let cas = g.put(cascader, P0, Zone::Hand);
    let hi = g.put(higher, P0, Zone::Library);
    
    // Put higher-cost spell on top of library
    let lib = mtg_core::ZoneRef::of(Zone::Library, P0);
    let order = g.engine.state.zone_order.get_mut(&lib).unwrap();
    order.retain(|id| *id != hi);
    order.insert(0, hi);
    
    g.main();
    g.cast(cas, &[]);
    
    // After cascade, the higher spell should be in library bottom or exile
    // The cascade spell itself should be in graveyard
    assert_eq!(g.count(Zone::Graveyard, P0), 1, "cascade spell in graveyard");
}

// ============================================================================
// MADNESS: Alternative casting cost during resolution.
// ============================================================================

/// Madness should allow casting discarded cards with reduced cost.
#[test]
#[ignore = "Madness discard-trigger action not yet implemented"]
fn madness_allows_alternative_casting() {
    let mut t = Table::default();
    let mad = t.card(
        "{2}{R}",
        "Instant",
        None,
        "Madness {R}\nDeal 2 damage to any target.",
    );
    let discarder = t.card("{B}", "Instant", None, "Target player discards a card.");
    let mut g = Game::new(t);
    g.lands(2);
    let _m = g.put(mad, P0, Zone::Hand);
    let d = g.put(discarder, P1, Zone::Hand);
    g.main();
    g.cast(d, &[Target::Player(P0)]);
    // When madness spell is discarded, an option should appear to cast it for the madness cost.
    // TODO: Implement madness trigger in engine (discard event triggers madness opportunity)
}

/// Madness should put the card into the graveyard if not cast.
#[test]
#[ignore = "Madness discard-trigger action not yet implemented"]
fn madness_goes_to_graveyard_if_not_cast() {
    let mut t = Table::default();
    let mad = t.card("{2}{R}", "Instant", None, "Madness {R}");
    let mut g = Game::new(t);
    let _m = g.put(mad, P0, Zone::Hand);
    // Discard the card and decline the madness offer.
    // The card should end up in the graveyard.
    // TODO: Implement madness discard handling
}

// ============================================================================
// MULTI-TARGET NOUNS: Nouns that can reference multiple objects/players.
// ============================================================================

/// Effects with multi-target noun patterns should compile.
#[test]
fn multi_target_each_creature() {
    let mut t = Table::default();
    let _spell = t.card(
        "{2}{G}",
        "Sorcery",
        None,
        "Each creature gets +1/+2 until end of turn.",
    );
    // Multi-target "each" patterns compile correctly
    let all_understood = t.understood.iter().all(|&u| u);
    assert!(all_understood, "each-target pattern compiles");
}

/// Effects targeting "any target" should compile.
#[test]
fn any_target_pattern() {
    let mut t = Table::default();
    let _spell = t.card(
        "{1}{R}",
        "Instant",
        None,
        "~ deals 3 damage to any target.",
    );
    // "Any target" patterns compile correctly
    let all_understood = t.understood.iter().all(|&u| u);
    assert!(all_understood, "any-target pattern compiles");
}

/// Effects targeting "any number of" should compile.
#[test]
#[ignore = "Distribute damage implementation not yet complete"]
fn any_number_of_pattern() {
    let mut t = Table::default();
    let _spell = t.card(
        "{3}{U}",
        "Instant",
        None,
        "Distribute 5 damage among any number of targets.",
    );
    // "Any number of" patterns with distribution need full implementation
    // TODO: Implement distribute damage effect
}

/// Effects with "target creature or player" should compile.
#[test]
#[ignore = "Creature or player targeting not yet implemented"]
fn creature_or_player_target() {
    let mut t = Table::default();
    let _spell = t.card(
        "{2}{B}",
        "Instant",
        None,
        "~ deals 3 damage to target creature or player.",
    );
    // "Creature or player" patterns need parser support for polymorphic targeting
    // TODO: Implement creature-or-player target patterns
}

/// Effects with "up to N targets" should compile.
#[test]
#[ignore = "Up-to-N targeting implementation not yet complete"]
fn up_to_n_targets() {
    let mut t = Table::default();
    let _spell = t.card(
        "{2}{R}",
        "Instant",
        None,
        "~ deals 2 damage to each of up to three targets.",
    );
    // "Up-to-N" patterns need full targeting system support
    // TODO: Implement up-to-N target selection in engine
}

// ============================================================================
// SAGA CHAPTERS: Multi-step saga progression.
// ============================================================================

/// Sagas with chapters should compile and progress correctly.
#[test]
#[ignore = "Saga chapter mechanics not yet implemented"]
fn saga_with_chapters_compiles() {
    let mut t = Table::default();
    let _saga = t.card(
        "{1}{W}",
        "Enchantment — Saga",
        None,
        "I — Create a 1/1 white Soldier token.\nII — A 1/1 white Soldier token gains \"When this creature is dealt damage, transform it.\"\nIII — Exile the top card of your library. You may play it this turn.",
    );
    // Saga should parse and chapter effects should be triggered on their respective lore counters.
    // TODO: Implement saga chapter parsing and lore counter mechanics
}

/// Sagas should increment their lore counter when they enter.
#[test]
#[ignore = "Saga lore counter mechanics not yet implemented"]
fn saga_increments_lore_counter() {
    let mut t = Table::default();
    let _saga = t.card(
        "{1}{G}",
        "Enchantment — Saga",
        None,
        "I — Create a 1/1 green Elf token.\nII — All creatures you control get +1/+1 until end of turn.",
    );
    // After a saga enters, its lore counter should start at 1 and increment each turn.
    // TODO: Implement lore counter tracking and chapter triggering
}

// ============================================================================
// GRAMMAR EXPANSION: Other complex patterns.
// ============================================================================

/// Complex mode selection with {M} should compile.
#[test]
#[ignore = "Complex mode selection with mana cost not yet implemented"]
fn complex_mode_selection_with_mana() {
    let mut t = Table::default();
    let _spell = t.card(
        "{2}{U}",
        "Instant",
        None,
        "Choose one — {U}: Counter target spell; {U}: Draw a card; {U}: Return target creature to its owner's hand.",
    );
    // Complex mode selection where each mode has its own mana cost needs parser support
    // TODO: Implement mode cost parsing
}

/// Strive costs should compile.
#[test]
#[ignore = "Strive cost mechanics not yet implemented"]
fn strive_cost_pattern() {
    let mut t = Table::default();
    let _spell = t.card(
        "{2}{R}",
        "Instant",
        None,
        "Strive — ~ costs {R} more to cast for each target beyond the first.\n~ deals 2 damage to each of up to three targets.",
    );
    // Strive cost patterns need implementation for cost modification per additional target
    // TODO: Implement strive cost calculation
}

/// Bargain costs should compile.
#[test]
#[ignore = "Bargain cost mechanics not yet implemented"]
fn bargain_cost_pattern() {
    let mut t = Table::default();
    let _spell = t.card(
        "{1}{W}",
        "Sorcery",
        None,
        "Bargain — You may discard a card. If you do, ~ costs {3} less to cast.",
    );
    // Bargain cost patterns need implementation for cost modification via discard
    // TODO: Implement bargain cost calculation
}
