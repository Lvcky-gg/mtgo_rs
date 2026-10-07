use mtg_verify::structural::{MAX_INPUT_BYTES, exercise};
use proptest::prelude::*;

#[test]
fn oversized_inputs_are_rejected_before_any_parser() {
    let out = exercise(&vec![b'x'; MAX_INPUT_BYTES + 1]);
    assert!(out.input_rejected_by_size);
    assert_eq!(out.compiler_understood, None);
}

#[test]
fn compiler_does_not_silently_accept_unknown_appended_rule() {
    let out = exercise(b"Draw a card.\nInvert the quantum flavor of the next timeline.");
    assert_eq!(out.compiler_understood, Some(false));
    assert!(out.compiler_unparsed_lines > 0);
}

#[test]
fn seeded_deck_and_external_card_reach_real_parsers() {
    let deck = exercise(include_bytes!("../../../fuzz/corpus/structural/deck.txt"));
    assert_eq!(deck.deck_entries, 2);
    let card = exercise(include_bytes!(
        "../../../fuzz/corpus/structural/external_card.json"
    ));
    assert!(card.scryfall_deserialized);
}

#[test]
fn protocol_and_ir_seeds_reach_real_deserializers() {
    let guest = exercise(include_bytes!(
        "../../../fuzz/corpus/structural/guest_message.json"
    ));
    assert!(guest.guest_message_deserialized);
    let host = exercise(include_bytes!(
        "../../../fuzz/corpus/structural/host_message.json"
    ));
    assert!(host.host_message_deserialized);
    let face = exercise(include_bytes!(
        "../../../fuzz/corpus/structural/card_face.json"
    ));
    assert!(face.card_face_deserialized);
    let scenario = exercise(include_bytes!(
        "../../../fuzz/corpus/structural/scenario.json"
    ));
    assert!(scenario.scenario_accepted);
}

#[test]
fn excessive_json_nesting_is_rejected() {
    let nested = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    let out = exercise(nested.as_bytes());
    assert!(!out.scenario_accepted);
    assert!(!out.card_face_deserialized);
    assert!(!out.guest_message_deserialized);
    assert!(!out.host_message_deserialized);
}

proptest! {
    #[test]
    fn arbitrary_bytes_do_not_panic_and_have_deterministic_parser_outcomes(
        bytes in prop::collection::vec(any::<u8>(), 0..2048),
    ) {
        prop_assert_eq!(exercise(&bytes), exercise(&bytes));
    }
    #[test]
    fn compiler_and_deck_utf8_smoke_is_deterministic(text in ".{0,512}") {
        prop_assert_eq!(exercise(text.as_bytes()), exercise(text.as_bytes()));
    }
}
