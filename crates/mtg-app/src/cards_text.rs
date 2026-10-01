//! A snapshot of card text for the UI.
//!
//! The renderer needs names and printed text, but [`mtg_ir::PrintedCards`] hands out references
//! and the library lives with the game thread. So the UI gets a plain owned snapshot instead —
//! built once, cheap to share, and `Send`, which is what lets the game and the window live on
//! different threads without either holding the other's data.

use std::collections::BTreeMap;

use mtg_core::{AbilityId, CardId};
use mtg_ir::PrintedCards;

/// Everything the UI needs to draw one card.
#[derive(Clone, Debug, Default)]
pub struct CardText {
    pub name: String,
    pub printing: Option<String>,
    pub cost: String,
    pub type_line: String,
    pub power_toughness: Option<String>,
    /// Printed rules text, shown so a player can read what a card says even when the engine has
    /// no authored behaviour for it.
    pub rules: Option<String>,
    pub is_land: bool,
    pub is_creature: bool,
    /// Printed activated-ability text, keyed by its engine id rather than line order.
    pub activated_abilities: BTreeMap<AbilityId, String>,
    /// Special-action names and their immediate costs, keyed by the engine ability id.
    pub special_actions: BTreeMap<AbilityId, String>,
    pub alternative_casts: BTreeMap<AbilityId, String>,
}

/// Card text by oracle id.
#[derive(Clone, Debug, Default)]
pub struct CardTexts {
    entries: BTreeMap<(CardId, u8), CardText>,
    splits: BTreeMap<CardId, CardText>,
}

impl CardText {
    pub fn artwork(&self) -> String {
        self.printing
            .as_ref()
            .map_or_else(|| self.name.clone(), |id| format!("scryfall:{id}"))
    }
}

impl CardTexts {
    /// Preserve deck artwork preferences across the game/UI thread boundary.
    pub fn for_match(cards: &mtg_session::game::GameCards) -> Self {
        let mut texts = Self::snapshot(cards, cards.ids());
        for ((id, face), text) in &mut texts.entries {
            if let Some(mtg_session::game::CardKey::Oracle(_, Some(printing))) = cards.key(*id) {
                let back = *face == 1
                    && matches!(
                        cards.layout(*id),
                        mtg_ir::Layout::ModalDfc | mtg_ir::Layout::Transforming
                    );
                text.printing = Some(if back {
                    format!("{printing}/back")
                } else {
                    printing.clone()
                });
            }
        }
        for (id, text) in &mut texts.splits {
            if let Some(mtg_session::game::CardKey::Oracle(_, Some(printing))) = cards.key(*id) {
                text.printing = Some(printing.clone());
            }
        }
        texts
    }

    /// Build a snapshot from a card source.
    ///
    /// `ids` is the set worth including — usually every card in the loaded decks, rather than the
    /// whole database, since a snapshot of thirty thousand cards would be wasted work.
    pub fn snapshot(cards: &dyn PrintedCards, ids: impl IntoIterator<Item = CardId>) -> Self {
        let mut entries = BTreeMap::new();
        let mut splits = BTreeMap::new();
        for id in ids {
            let count = match cards.layout(id) {
                mtg_ir::Layout::ModalDfc
                | mtg_ir::Layout::Transforming
                | mtg_ir::Layout::Split
                | mtg_ir::Layout::Adventure => 2,
                _ => 1,
            };
            for index in 0..count {
                let Some(face) = cards.face(id, index) else {
                    continue;
                };
                let ch = face.printed_characteristics();
                entries.insert(
                    (id, index),
                    CardText {
                        name: face.name.to_string(),
                        printing: None,
                        cost: crate::format::mana_cost(&face.mana_cost),
                        type_line: crate::format::type_line(&ch, |s| {
                            cards.subtype_name(s).unwrap_or("").to_string()
                        }),
                        power_toughness: crate::format::power_toughness(&ch),
                        rules: face.oracle_text.as_ref().map(|t| t.to_string()),
                        is_land: ch.has_type(mtg_core::CardType::Land),
                        is_creature: ch.has_type(mtg_core::CardType::Creature),
                        activated_abilities: face
                            .abilities
                            .iter()
                            .filter(|a| matches!(a.kind, mtg_ir::AbilityKind::Activated { .. }))
                            .map(|a| {
                                (
                                    a.id,
                                    a.source_text
                                        .as_ref()
                                        .map(|t| t.to_string())
                                        .unwrap_or_else(|| {
                                            format!("Ability {}", u32::from(a.id.0) + 1)
                                        }),
                                )
                            })
                            .collect(),
                        alternative_casts: face
                            .abilities
                            .iter()
                            .filter_map(|ability| {
                                let mtg_ir::AbilityKind::AlternativeCost { cost, kind, .. } =
                                    &ability.kind
                                else {
                                    return None;
                                };
                                use mtg_ir::ability::AltCost;
                                let name = match kind {
                                    AltCost::Dash => "dash",
                                    AltCost::Evoke => "evoke",
                                    AltCost::Overload => "overload",
                                    AltCost::Warp => "warp",
                                    AltCost::Bestow => "bestow",
                                };
                                let mana = crate::format::mana_cost(&cost.mana);
                                Some((
                                    ability.id,
                                    if mana.is_empty() {
                                        name.into()
                                    } else {
                                        format!("{name} {mana}")
                                    },
                                ))
                            })
                            .collect(),
                        special_actions: face
                            .abilities
                            .iter()
                            .filter_map(|ability| {
                                let (verb, cost) = match &ability.kind {
                                    mtg_ir::AbilityKind::ExileToCastLater {
                                        cost,
                                        face_down,
                                        ..
                                    } => (if *face_down { "Foretell" } else { "Plot" }, cost),
                                    mtg_ir::AbilityKind::Suspend { cost, .. } => ("Suspend", cost),
                                    mtg_ir::AbilityKind::Morph { cost, .. } => {
                                        ("Turn face up", cost)
                                    }
                                    _ => return None,
                                };
                                let mana = crate::format::mana_cost(&cost.mana);
                                Some((
                                    ability.id,
                                    if mana.is_empty() {
                                        verb.into()
                                    } else {
                                        format!("{verb} {mana}")
                                    },
                                ))
                            })
                            .collect(),
                    },
                );
            }
        }
        for (id, _) in entries.keys().filter(|(_, face)| *face == 0) {
            if cards.layout(*id) == mtg_ir::Layout::Split
                && let (Some(left), Some(right), Some(ch)) = (
                    entries.get(&(*id, 0)),
                    entries.get(&(*id, 1)),
                    cards.split_characteristics(*id),
                )
            {
                let mut combined = left.clone();
                combined.name = ch.name.to_string();
                combined.cost = crate::format::mana_cost(&ch.mana_cost);
                combined.type_line = crate::format::type_line(&ch, |s| {
                    cards.subtype_name(s).unwrap_or("").to_string()
                });
                combined.rules = Some(format!(
                    "{}
{}

{}
{}",
                    left.name,
                    left.rules.as_deref().unwrap_or(""),
                    right.name,
                    right.rules.as_deref().unwrap_or("")
                ));
                splits.insert(*id, combined);
            }
        }
        Self { entries, splits }
    }

    pub fn get(&self, id: CardId) -> Option<&CardText> {
        self.get_face(id, 0)
    }

    /// Text for a visible object: split cards show both halves outside the stack.
    pub fn object_text(&self, object: &mtg_engine::view::ObjectView) -> Option<&CardText> {
        let id = object.card?;
        if object.zone.zone != mtg_core::Zone::Stack
            && let Some(combined) = self.splits.get(&id)
        {
            Some(combined)
        } else {
            self.get_face(id, object.face)
        }
    }

    pub fn get_face(&self, id: CardId, face: u8) -> Option<&CardText> {
        self.entries.get(&(id, face))
    }

    pub fn face_name(&self, id: Option<CardId>, face: u8) -> String {
        id.and_then(|id| self.get_face(id, face))
            .map_or_else(|| "(hidden)".into(), |text| text.name.clone())
    }

    /// The card's name, or a placeholder.
    ///
    /// A placeholder rather than an empty string: a face-down or unknown card should read as
    /// something on the board, not as a blank.
    pub fn name(&self, id: Option<CardId>) -> String {
        match id.and_then(|i| self.get(i)) {
            Some(c) => c.name.clone(),
            // A card the viewer is not entitled to see.
            None => "(hidden)".into(),
        }
    }

    pub fn activation_label(&self, card: Option<CardId>, ability: AbilityId) -> String {
        self.activation_label_face(card, 0, ability)
    }

    pub fn activation_label_face(
        &self,
        card: Option<CardId>,
        face: u8,
        ability: AbilityId,
    ) -> String {
        let name = self.face_name(card, face);
        match card
            .and_then(|c| self.get_face(c, face))
            .and_then(|c| c.activated_abilities.get(&ability))
        {
            Some(text) => format!("{name}: {text}"),
            None => format!("Activate {name} (ability {})", u32::from(ability.0) + 1),
        }
    }

    pub fn special_action_label_face(
        &self,
        card: Option<CardId>,
        face: u8,
        ability: AbilityId,
    ) -> String {
        let name = self.face_name(card, face);
        match card
            .and_then(|card| self.get_face(card, face))
            .and_then(|text| text.special_actions.get(&ability))
        {
            Some(action) => format!("{name}: {action}"),
            None => format!("Use {name} (ability {})", u32::from(ability.0) + 1),
        }
    }

    pub fn alternative_cast_label_face(
        &self,
        card: Option<CardId>,
        face: u8,
        ability: AbilityId,
    ) -> String {
        let name = self.face_name(card, face);
        match card
            .and_then(|card| self.get_face(card, face))
            .and_then(|text| text.alternative_casts.get(&ability))
        {
            Some(cost) => format!("Cast {name} with {cost}"),
            None => format!(
                "Cast {name} for its alternative cost (ability {})",
                u32::from(ability.0) + 1
            ),
        }
    }

    pub fn is_land(&self, id: Option<CardId>) -> bool {
        id.and_then(|i| self.get(i)).is_some_and(|c| c.is_land)
    }

    pub fn len(&self) -> usize {
        self.entries.keys().filter(|(_, face)| *face == 0).count()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{CardType, ManaCost, ManaSymbol, Subtype};
    use mtg_ir::CardFace;

    struct Fixture(Vec<CardFace>);

    impl PrintedCards for Fixture {
        fn face(&self, card: CardId, _f: u8) -> Option<&CardFace> {
            self.0.get(card.0 as usize)
        }
        fn subtype_name(&self, s: Subtype) -> Option<&str> {
            (s.0 == 1).then_some("Cleric")
        }
    }

    fn fixture() -> Fixture {
        let mut creature = CardFace {
            name: "Ashen Pilgrim".into(),
            mana_cost: ManaCost {
                symbols: vec![
                    ManaSymbol::Generic(1),
                    ManaSymbol::Colored(mtg_core::Color::White),
                ],
            },
            card_types: vec![CardType::Creature],
            subtypes: vec![Subtype(1)],
            supertypes: Vec::new(),
            power: Some(2),
            toughness: Some(1),
            loyalty: None,
            abilities: Vec::new(),
            oracle_text: Some("An invented card, for testing.".into()),
            colors: None,
        };
        let land = CardFace {
            name: "Quiet Field".into(),
            mana_cost: ManaCost::FREE,
            card_types: vec![CardType::Land],
            subtypes: Vec::new(),
            supertypes: Vec::new(),
            power: None,
            toughness: None,
            loyalty: None,
            abilities: Vec::new(),
            oracle_text: None,
            colors: None,
        };
        creature.loyalty = None;
        Fixture(vec![creature, land])
    }

    #[test]
    fn a_snapshot_carries_what_the_ui_needs_to_draw_a_card() {
        let texts = CardTexts::snapshot(&fixture(), [CardId(0)]);
        let card = texts.get(CardId(0)).expect("present");
        assert_eq!(card.name, "Ashen Pilgrim");
        assert_eq!(card.cost, "{1}{W}");
        assert_eq!(card.type_line, "Creature — Cleric");
        assert_eq!(card.power_toughness.as_deref(), Some("2/1"));
        assert!(
            card.rules.is_some(),
            "printed text is kept so a player can read the card"
        );
    }

    #[test]
    fn a_land_is_flagged_so_it_can_be_drawn_in_its_own_row() {
        let texts = CardTexts::snapshot(&fixture(), [CardId(0), CardId(1)]);
        assert!(!texts.is_land(Some(CardId(0))));
        assert!(texts.is_land(Some(CardId(1))));
    }

    #[test]
    fn a_card_the_viewer_cannot_see_reads_as_hidden_not_as_blank() {
        // An opponent's hand arrives with no card id at all. A blank would look like a bug.
        let texts = CardTexts::snapshot(&fixture(), [CardId(0)]);
        assert_eq!(texts.name(None), "(hidden)");
        assert_eq!(texts.name(Some(CardId(99))), "(hidden)");
    }

    #[test]
    fn only_the_requested_cards_are_included() {
        // The whole database would be wasted work; decks are what get drawn.
        let texts = CardTexts::snapshot(&fixture(), [CardId(1)]);
        assert_eq!(texts.len(), 1);
        assert!(texts.get(CardId(0)).is_none());
    }
    #[test]
    fn special_action_labels_use_the_immediate_cost_and_preserve_hidden_identity() {
        let mut cards = fixture();
        let mut cost = mtg_ir::Cost::free();
        cost.mana.symbols.push(mtg_core::ManaSymbol::Generic(2));
        for (id, kind) in [
            (
                3,
                mtg_ir::AbilityKind::ExileToCastLater {
                    cost: cost.clone(),
                    face_down: true,
                    later: Some(mtg_core::ManaCost {
                        symbols: vec![mtg_core::ManaSymbol::Generic(7)],
                    }),
                    sorcery: false,
                },
            ),
            (
                5,
                mtg_ir::AbilityKind::ExileToCastLater {
                    cost: cost.clone(),
                    face_down: false,
                    later: None,
                    sorcery: true,
                },
            ),
            (
                8,
                mtg_ir::AbilityKind::Morph {
                    cost,
                    megamorph: false,
                    disguise: false,
                },
            ),
        ] {
            cards.0[0].abilities.push(mtg_ir::Ability {
                id: AbilityId(id),
                source_text: None,
                targets: Vec::new(),
                kind,
            });
        }
        let texts = CardTexts::snapshot(&cards, [CardId(0)]);
        for (id, action) in [(3, "Foretell"), (5, "Plot"), (8, "Turn face up")] {
            assert_eq!(
                texts.special_action_label_face(Some(CardId(0)), 0, AbilityId(id)),
                format!("Ashen Pilgrim: {action} {{2}}")
            );
        }
        assert!(
            !texts
                .special_action_label_face(None, 0, AbilityId(3))
                .contains("Ashen Pilgrim")
        );
    }

    #[test]
    fn alternative_cast_labels_distinguish_mechanics_and_costs() {
        use mtg_ir::ability::AltCost;
        let mut cards = fixture();
        for (id, kind) in [
            (3, AltCost::Dash),
            (5, AltCost::Evoke),
            (7, AltCost::Overload),
            (9, AltCost::Warp),
            (11, AltCost::Bestow),
        ] {
            let mut cost = mtg_ir::Cost::free();
            cost.mana
                .symbols
                .push(mtg_core::ManaSymbol::Generic(id as u8));
            cards.0[0].abilities.push(mtg_ir::Ability {
                id: AbilityId(id),
                source_text: None,
                targets: Vec::new(),
                kind: mtg_ir::AbilityKind::AlternativeCost {
                    cost,
                    kind,
                    instead: None,
                },
            });
        }
        let texts = CardTexts::snapshot(&cards, [CardId(0)]);
        for (id, name) in [
            (3, "dash"),
            (5, "evoke"),
            (7, "overload"),
            (9, "warp"),
            (11, "bestow"),
        ] {
            assert_eq!(
                texts.alternative_cast_label_face(Some(CardId(0)), 0, AbilityId(id)),
                format!("Cast Ashen Pilgrim with {name} {{{id}}}")
            );
        }
        assert!(
            !texts
                .alternative_cast_label_face(None, 0, AbilityId(3))
                .contains("Ashen Pilgrim")
        );
    }

    #[test]
    fn activation_labels_distinguish_loyalty_costs_and_preserve_ability_ids() {
        let mut cards = fixture();
        for (id, text) in [(2, "+1: Draw a card."), (7, "-2: You gain 3 life.")] {
            cards.0[0].abilities.push(mtg_ir::Ability {
                id: AbilityId(id),
                source_text: Some(text.into()),
                targets: Vec::new(),
                kind: mtg_ir::AbilityKind::Activated {
                    cost: mtg_ir::Cost::free(),
                    effect: mtg_ir::Effect::Nothing,
                    functions_from: mtg_core::Zone::Battlefield,
                    is_mana_ability: false,
                    is_loyalty_ability: true,
                    timing: mtg_ir::ability::ActivationTiming::SorcerySpeed,
                },
            });
        }
        let texts = CardTexts::snapshot(&cards, [CardId(0)]);
        assert_eq!(
            texts.activation_label(Some(CardId(0)), AbilityId(2)),
            "Ashen Pilgrim: +1: Draw a card."
        );
        assert_eq!(
            texts.activation_label(Some(CardId(0)), AbilityId(7)),
            "Ashen Pilgrim: -2: You gain 3 life."
        );
        assert!(
            !texts
                .activation_label(None, AbilityId(2))
                .contains("Ashen Pilgrim")
        );
    }

    #[test]
    fn a_modal_snapshot_renders_the_selected_face_and_its_own_ability() {
        struct Modal(Fixture);
        impl PrintedCards for Modal {
            fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
                (card == CardId(0))
                    .then(|| self.0.0.get(face as usize))
                    .flatten()
            }
            fn layout(&self, _: CardId) -> mtg_ir::Layout {
                mtg_ir::Layout::ModalDfc
            }
            fn subtype_name(&self, _: Subtype) -> Option<&str> {
                Some("Cleric")
            }
        }
        let mut fixture = fixture();
        fixture.0[1].abilities.push(mtg_ir::Ability {
            id: AbilityId(0),
            targets: Vec::new(),
            source_text: Some("{T}: Add {G}.".into()),
            kind: mtg_ir::AbilityKind::Activated {
                cost: mtg_ir::Cost::free(),
                effect: mtg_ir::Effect::Nothing,
                functions_from: mtg_core::Zone::Battlefield,
                is_mana_ability: true,
                is_loyalty_ability: false,
                timing: mtg_ir::ability::ActivationTiming::Instant,
            },
        });
        let texts = CardTexts::snapshot(&Modal(fixture), [CardId(0)]);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts.face_name(Some(CardId(0)), 0), "Ashen Pilgrim");
        assert_eq!(texts.face_name(Some(CardId(0)), 1), "Quiet Field");
        assert!(texts.get_face(CardId(0), 1).unwrap().is_land);
        assert_eq!(
            texts.activation_label_face(Some(CardId(0)), 1, AbilityId(0)),
            "Quiet Field: {T}: Add {G}."
        );
        assert_eq!(texts.face_name(None, 1), "(hidden)");
    }

    #[test]
    fn split_objects_show_both_halves_off_stack_and_only_the_cast_half_on_stack() {
        struct Split(Fixture);
        impl PrintedCards for Split {
            fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
                (card == CardId(0))
                    .then(|| self.0.0.get(face as usize))
                    .flatten()
            }
            fn layout(&self, _: CardId) -> mtg_ir::Layout {
                mtg_ir::Layout::Split
            }
            fn subtype_name(&self, _: Subtype) -> Option<&str> {
                None
            }
        }
        let mut source = fixture();
        source.0[0].name = "Dawn".into();
        source.0[0].card_types = vec![CardType::Sorcery];
        source.0[1].name = "Dusk".into();
        source.0[1].card_types = vec![CardType::Instant];
        source.0[1].mana_cost = ManaCost {
            symbols: vec![ManaSymbol::Colored(mtg_core::Color::Red)],
        };
        let texts = CardTexts::snapshot(&Split(source), [CardId(0)]);
        let me = mtg_core::PlayerId(0);
        let them = mtg_core::PlayerId(1);
        let mut state = mtg_engine::state::GameState::new(&[me, them], 20);
        let object = state.place(
            CardId(0),
            me,
            mtg_core::ZoneRef::of(mtg_core::Zone::Hand, me),
        );
        let view = mtg_engine::view::project(&state, me);
        let text = texts.object_text(&view.visible[&object]).unwrap();
        assert_eq!(text.name, "Dawn // Dusk");
        assert_eq!(text.cost, "{1}{W}{R}");
        assert!(text.rules.as_ref().unwrap().contains("Dusk"));
        state.objects.get_mut(&object).unwrap().zone =
            mtg_core::ZoneRef::shared(mtg_core::Zone::Stack);
        state.objects.get_mut(&object).unwrap().face = 1;
        let view = mtg_engine::view::project(&state, them);
        let text = texts.object_text(&view.visible[&object]).unwrap();
        assert_eq!(text.name, "Dusk");
        assert_eq!(text.cost, "{R}");
        state.objects.get_mut(&object).unwrap().zone =
            mtg_core::ZoneRef::of(mtg_core::Zone::Hand, me);
        let hidden = mtg_engine::view::project(&state, them);
        assert!(texts.object_text(&hidden.visible[&object]).is_none());
    }
}
