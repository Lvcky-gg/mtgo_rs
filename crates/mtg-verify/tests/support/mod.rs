#![allow(dead_code)]

use mtg_core::{AbilityId, CardId, CardType, Keyword, ManaCost, PlayerId, Step};
use mtg_engine::{Engine, Progress, layers::PrintedCards, state::GameState};
use mtg_ir::{Ability, AbilityKind, CardFace};

pub const P0: PlayerId = PlayerId(0);
pub const P1: PlayerId = PlayerId(1);
pub const CREATURE: CardId = CardId(0);

pub struct Cards(pub CardFace);

impl Cards {
    pub fn creature(toughness: i32, indestructible: bool) -> Self {
        Self(CardFace {
            name: "Verification creature".into(),
            mana_cost: ManaCost::FREE,
            card_types: vec![CardType::Creature],
            subtypes: vec![],
            supertypes: vec![],
            power: Some(2),
            toughness: Some(toughness),
            loyalty: None,
            abilities: if indestructible {
                vec![Ability {
                    id: AbilityId(0),
                    kind: AbilityKind::Keyword(Keyword::Indestructible),
                    targets: vec![],
                    source_text: None,
                }]
            } else {
                vec![]
            },
            oracle_text: None,
            colors: None,
        })
    }
}

impl PrintedCards for Cards {
    fn face(&self, card: CardId, _face: u8) -> Option<&CardFace> {
        (card == CREATURE).then_some(&self.0)
    }

    fn subtype_name(&self, _subtype: mtg_core::Subtype) -> Option<&str> {
        None
    }
}

pub fn main_state() -> GameState {
    let mut state = GameState::new(&[P0, P1], 20);
    state.turn = 2;
    state.step = Step::PrecombatMain;
    state.active_player = P0;
    state.priority = Some(P0);
    state
}

pub fn stabilize(engine: &mut Engine, cards: &Cards) {
    for _ in 0..100 {
        match engine.advance(cards) {
            Progress::NeedsChoice(_) => return,
            Progress::GameOver { .. } => panic!("unexpected game over"),
            _ => {}
        }
    }
    panic!("engine did not reach a stable priority boundary");
}
