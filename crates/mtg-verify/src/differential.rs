//! Independent, deliberately small rules propositions. Unsupported mechanics are
//! not silently approximated: callers supply a scoped observation, not a game.
use serde::{Deserialize, Serialize};

pub trait RulesOracle<Input> {
    type Output: PartialEq;
    fn evaluate(&self, input: &Input) -> Self::Output;
    fn provenance(&self) -> &'static str;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disagreement<T> {
    pub reference: T,
    pub production: T,
}

pub fn compare<I, O: RulesOracle<I>>(
    oracle: &O,
    input: &I,
    production: O::Output,
) -> Result<(), Disagreement<O::Output>> {
    let reference = oracle.evaluate(input);
    if reference == production {
        Ok(())
    } else {
        Err(Disagreement {
            reference,
            production,
        })
    }
}

/// Already-computed creature characteristics; this model does not verify layers,
/// regeneration, replacement effects, or whether characteristics were computed correctly.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CreatureSbaInput {
    pub toughness: i32,
    pub marked_damage: u32,
    pub deathtouch_damage: bool,
    pub indestructible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatureSba {
    Survives,
    PutIntoGraveyard,
    Destroy,
}

pub struct CreatureSbaOracle;
impl RulesOracle<CreatureSbaInput> for CreatureSbaOracle {
    type Output = CreatureSba;
    fn provenance(&self) -> &'static str {
        "Scoped independent CR 704.5f/g/h decision table; requires independent review"
    }
    fn evaluate(&self, input: &CreatureSbaInput) -> CreatureSba {
        // A decision table, separate from engine control flow. Indestructible
        // protects against destruction, never the zero-toughness SBA.
        match (
            input.toughness.is_positive(),
            input.indestructible,
            input.deathtouch_damage && input.marked_damage != 0,
            i64::from(input.marked_damage) >= i64::from(input.toughness),
        ) {
            (false, _, _, _) => CreatureSba::PutIntoGraveyard,
            (true, false, true, _) | (true, false, _, true) => CreatureSba::Destroy,
            _ => CreatureSba::Survives,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_toughness_is_not_destruction_even_when_indestructible() {
        let input = CreatureSbaInput {
            toughness: 0,
            marked_damage: 0,
            deathtouch_damage: false,
            indestructible: true,
        };
        assert_eq!(
            CreatureSbaOracle.evaluate(&input),
            CreatureSba::PutIntoGraveyard
        );
        assert!(compare(&CreatureSbaOracle, &input, CreatureSba::Survives).is_err());
    }
    #[test]
    fn production_sba_matches_independent_table_across_small_state_space() {
        use mtg_core::{AbilityId, CardId, CardType, ManaCost, PlayerId, Zone, ZoneRef};
        use mtg_engine::{sba, state::GameState};
        use mtg_ir::{Ability, AbilityKind, CardFace, PrintedCards, ability::Keyword};
        struct Cards(CardFace);
        impl PrintedCards for Cards {
            fn subtype_name(&self, _: mtg_core::Subtype) -> Option<&str> {
                None
            }
            fn face(&self, _: CardId, face: u8) -> Option<&CardFace> {
                (face == 0).then_some(&self.0)
            }
        }
        for toughness in -2..=5 {
            for marked_damage in 0..=7 {
                for deathtouch_damage in [false, true] {
                    for indestructible in [false, true] {
                        let input = CreatureSbaInput {
                            toughness,
                            marked_damage,
                            deathtouch_damage,
                            indestructible,
                        };
                        let cards = Cards(CardFace {
                            name: "Scoped test creature".into(),
                            mana_cost: ManaCost::FREE,
                            card_types: vec![CardType::Creature],
                            subtypes: vec![],
                            supertypes: vec![],
                            power: Some(1),
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
                        });
                        let mut state = GameState::new(&[PlayerId(0), PlayerId(1)], 20);
                        let id =
                            state.place(CardId(0), PlayerId(0), ZoneRef::shared(Zone::Battlefield));
                        let object = state.objects.get_mut(&id).unwrap();
                        object.damage = marked_damage;
                        object.dealt_deathtouch_damage = deathtouch_damage;
                        let mut actual = CreatureSba::Survives;
                        for action in sba::check(&state, &cards).actions {
                            match action {
                                sba::Sba::PutIntoGraveyard {
                                    object,
                                    rule: "704.5f",
                                } if object == id => actual = CreatureSba::PutIntoGraveyard,
                                sba::Sba::Destroy { object, .. } if object == id => {
                                    actual = CreatureSba::Destroy
                                }
                                _ => {}
                            }
                        }
                        assert!(
                            compare(&CreatureSbaOracle, &input, actual).is_ok(),
                            "input: {input:?}, actual: {actual:?}"
                        );
                    }
                }
            }
        }
    }
}
