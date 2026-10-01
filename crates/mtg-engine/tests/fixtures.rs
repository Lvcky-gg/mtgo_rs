//! A guard rail on the test fixtures themselves.
//!
//! The card set is a positional list addressed by separate constants, so inserting a
//! card anywhere but the end renumbers everything after it. That failure is nasty
//! because it shows up as many unrelated tests breaking at once, far from the cause.
//! This turns it into one obvious failure.

mod common;

use common::*;
use mtg_engine::layers::PrintedCards;

#[test]
fn every_card_constant_points_at_the_card_it_names() {
    let cards = TestCards::default();
    let mut wrong = Vec::new();

    for (id, expected) in CARD_NAMES {
        let actual = cards.face(*id, 0).map(|f| f.name.to_string());
        if actual.as_deref() != Some(*expected) {
            wrong.push(format!("{id:?}: expected {expected:?}, found {actual:?}"));
        }
    }

    assert!(
        wrong.is_empty(),
        "card fixture indices have shifted — a card was inserted somewhere other than \
         the end of the list:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn the_name_table_covers_every_fixture() {
    // Otherwise a new card can be added without being guarded.
    let cards = TestCards::default();
    let mut n = 0;
    while cards.face(mtg_core::CardId(n), 0).is_some() {
        n += 1;
    }
    assert_eq!(
        n as usize,
        CARD_NAMES.len(),
        "every fixture card needs an entry in CARD_NAMES"
    );
}
