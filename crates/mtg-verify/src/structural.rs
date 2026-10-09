//! Bounded smoke/fuzz entry point for actual untrusted parsing boundaries.
//! Successful deserialization is not proof of semantic validity. Only scenarios
//! have an engine-state validation gate here; protocol handlers are not exercised.
use mtg_core::CardType;
use mtg_net::session::{GuestMessage, HostMessage};
use mtg_oracle::{
    compile::{FaceText, compile},
    convert::Subtypes,
};
use serde::{Deserialize, Serialize};

pub const MAX_INPUT_BYTES: usize = 64 * 1024;
struct NoSubtypes;
impl Subtypes for NoSubtypes {
    fn intern(&self, _: &str) -> Option<u16> {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralObservation {
    pub input_rejected_by_size: bool,
    pub scenario_accepted: bool,
    pub replay_report_accepted: bool,
    pub card_face_deserialized: bool,
    pub guest_message_deserialized: bool,
    pub host_message_deserialized: bool,
    pub scryfall_deserialized: bool,
    pub deck_entries: usize,
    pub compiler_understood: Option<bool>,
    pub compiler_unparsed_lines: usize,
}

/// All allocations are driven by at most 64KiB of input. This is an input bound,
/// not an allocator budget or timeout. libFuzzer adds process RSS/time limits.
/// No sockets, databases, filesystem writes, or game execution are involved.
pub fn exercise(bytes: &[u8]) -> StructuralObservation {
    if bytes.len() > MAX_INPUT_BYTES {
        return StructuralObservation {
            input_rejected_by_size: true,
            ..Default::default()
        };
    }
    let mut out = StructuralObservation {
        scenario_accepted: crate::scenario::GameScenario::parse(bytes).is_ok(),
        replay_report_accepted: crate::bug_report::ReplayReport::parse(bytes).is_ok(),
        card_face_deserialized: serde_json::from_slice::<mtg_ir::CardFace>(bytes).is_ok(),
        guest_message_deserialized: serde_json::from_slice::<GuestMessage>(bytes).is_ok(),
        host_message_deserialized: serde_json::from_slice::<HostMessage>(bytes).is_ok(),
        ..Default::default()
    };
    // serde_json's default recursion limit is deliberately not disabled.
    if let Ok(card) = serde_json::from_slice::<mtg_oracle::scryfall::Card>(bytes) {
        out.scryfall_deserialized = true;
        let _ = card.importable();
        if let Some(stored) = mtg_oracle::convert::to_stored(&card) {
            for face in &stored.faces {
                let _ = mtg_oracle::convert::to_engine_face(face, &NoSubtypes);
            }
        }
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        let deck = mtg_deck::parse(text);
        out.deck_entries = deck.entries.len();
        let face = FaceText {
            name: "Fuzz card",
            card_types: &[CardType::Instant],
            subtypes: &[],
            oracle_text: Some(text),
            mana_cost: "{1}",
        };
        let compiled = compile(&face, &NoSubtypes);
        out.compiler_understood = Some(compiled.understood());
        out.compiler_unparsed_lines = compiled.unparsed.len();
    }
    out
}
