//! Arranging a view into something a board can be drawn from.
//!
//! Pure: takes a [`PlayerView`] and groups it by player and zone. Separated from the renderer so
//! the grouping can be tested without a window, and so the renderer stays a function of this
//! rather than of the game.
//!
//! One property is worth noticing here rather than in the drawing code: the opponent's hand comes
//! out as a **count with no contents**, because that is all the view contains. The UI cannot leak
//! hidden information even by accident, since it never receives any.

use std::collections::BTreeMap;

use mtg_core::{CardId, ObjectId, PlayerId, Zone};
use mtg_engine::{
    PlayerView,
    actions::{Action, LegalActions},
    view::ObjectView,
};

/// Everything to draw for one player.
///
/// `Default` is hand-written rather than derived: a `PlayerId` has no meaningful default — a seat
/// is an identity, not a quantity — so giving it one in `mtg-core` to satisfy a UI struct would be
/// the wrong place to fix this.
#[derive(Clone, Debug)]
pub struct Side {
    pub player: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub energy: u32,
    pub city_blessing: bool,
    /// How many cards are in hand. For an opponent this is all that is known.
    pub hand_size: u32,
    pub library_size: u32,
    /// Cards in hand, populated only for the viewer.
    pub hand: Vec<ObjectId>,
    pub battlefield: Vec<ObjectId>,
    pub graveyard: Vec<ObjectId>,
    /// Floating mana, by pool slot.
    pub mana: [u16; 6],
    /// Commander damage taken, by the commander's owner.
    pub commander_damage: std::collections::BTreeMap<PlayerId, u32>,
    pub has_lost: bool,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            player: PlayerId(0),
            life: 0,
            poison: 0,
            energy: 0,
            city_blessing: false,
            hand_size: 0,
            library_size: 0,
            hand: Vec::new(),
            battlefield: Vec::new(),
            graveyard: Vec::new(),
            mana: [0; 6],
            commander_damage: Default::default(),
            has_lost: false,
        }
    }
}

/// The board, from one player's point of view.
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub mine: Side,
    pub opponents: Vec<Side>,
    /// Top of the stack first, matching how it resolves.
    pub stack: Vec<ObjectId>,
}

/// Group a view by player and zone.
pub fn arrange(view: &PlayerView) -> Board {
    let mut board = Board {
        stack: view.stack.clone(),
        ..Default::default()
    };

    for summary in view.players.values() {
        let mut side = Side {
            player: summary.id,
            life: summary.life,
            poison: summary.poison,
            energy: summary.energy,
            city_blessing: summary.city_blessing,
            hand_size: summary.hand_size,
            library_size: summary.library_size,
            graveyard: summary.graveyard.clone(),
            mana: summary.mana,
            commander_damage: summary.commander_damage.clone(),
            has_lost: false,
            hand: Vec::new(),
            battlefield: Vec::new(),
        };

        for obj in view.visible.values() {
            match obj.zone.zone {
                Zone::Battlefield if obj.controller == summary.id => {
                    side.battlefield.push(obj.id);
                }
                // Only ever populated for the viewer: an opponent's hand is not in the view.
                Zone::Hand if obj.zone.player == Some(summary.id) => {
                    side.hand.push(obj.id);
                }
                _ => {}
            }
        }

        // Stable order, so the board does not reshuffle itself between frames.
        side.battlefield.sort_unstable();
        side.hand.sort_unstable();

        if summary.id == view.viewer {
            board.mine = side;
        } else {
            board.opponents.push(side);
        }
    }

    board.opponents.sort_by_key(|s| s.player);
    board
}

/// Cards already disclosed in a player's library. Hidden library contents never enter here.
pub fn visible_library_cards(view: &PlayerView, player: PlayerId) -> Vec<ObjectId> {
    view.visible
        .values()
        .filter(|object| object.zone.zone == Zone::Library && object.zone.player == Some(player))
        .map(|object| object.id)
        .collect()
}

/// Split a battlefield into the rows a player expects: lands at the back, everything else in
/// front.
///
/// A presentation choice, not a rules one — the engine has no notion of rows — but it is how
/// players actually read a board, so it belongs in the layout rather than being left to the eye.
pub fn rows(
    battlefield: &[ObjectId],
    is_land: impl Fn(ObjectId) -> bool,
) -> (Vec<ObjectId>, Vec<ObjectId>) {
    let mut lands = Vec::new();
    let mut others = Vec::new();
    for id in battlefield {
        if is_land(*id) {
            lands.push(*id);
        } else {
            others.push(*id);
        }
    }
    (others, lands)
}

/// Collapse actions that differ only in which copy of the same card they use.
///
/// Four identical lands in hand produce four "play" actions, and four identical buttons read as
/// a bug. Casting or playing one copy is the same decision as another, so only the first is
/// kept. Activated abilities are left alone: two copies on the battlefield can differ (tapped,
/// counters, damage), so which one is used can matter.
pub fn distinct_actions<'a>(
    actions: &[&'a Action],
    card_of: impl Fn(ObjectId) -> Option<CardId>,
) -> Vec<&'a Action> {
    distinct_actions_with_zones(actions, card_of, |_| None)
}

/// Equivalent copies in different zones must remain separate casting choices.
pub fn distinct_actions_with_zones<'a>(
    actions: &[&'a Action],
    card_of: impl Fn(ObjectId) -> Option<CardId>,
    zone_of: impl Fn(ObjectId) -> Option<mtg_core::ZoneRef>,
) -> Vec<&'a Action> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for action in actions {
        let key = action.play().and_then(|(object, face, land)| {
            card_of(object).map(|card| (land, card, face, zone_of(object)))
        });
        if let Some(key) = key {
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
        }
        out.push(*action);
    }
    out
}

/// The cards in hand that can be played right now, and the action that plays each.
///
/// Read straight off the engine's legal-action list: casting, playing a land, or a
/// special action such as foretell. Nothing here decides legality.
pub fn hand_plays(actions: &[Action], hand: &[ObjectId]) -> BTreeMap<ObjectId, Action> {
    hand_play_options(actions, hand)
        .into_iter()
        .filter_map(|(id, mut options)| (options.len() == 1).then(|| (id, options.remove(0))))
        .collect()
}

/// Every offered face and special action remains a separate option; never silently choose.
pub fn hand_play_options(actions: &[Action], hand: &[ObjectId]) -> BTreeMap<ObjectId, Vec<Action>> {
    let mut options: BTreeMap<ObjectId, Vec<Action>> = BTreeMap::new();
    for action in actions {
        let object = match action {
            Action::SpecialAction { source, .. } => Some(*source),
            Action::CastFaceDown { object } | Action::CastAlternative { object, .. } => {
                Some(*object)
            }
            _ => action.play().map(|(object, _, _)| object),
        };
        if let Some(object) = object
            && hand.contains(&object)
        {
            options.entry(object).or_default().push(action.clone());
        }
    }
    options
}

/// What clicking a permanent can do right now: its activated abilities, then its mana
/// abilities — one entry per colour for a source with a choice.
///
/// Like [`hand_plays`], this only reads the engine's list. Mana abilities come from
/// `LegalActions::mana_abilities`, which the auto-pass check ignores, so they are offered here
/// without making an untapped land count as a reason to stop.
pub fn permanent_actions(legal: &LegalActions, id: ObjectId) -> Vec<Action> {
    legal
        .all()
        .filter(|a| match a {
            Action::ActivateAbility { source, .. }
            | Action::SpecialAction { source, .. }
            | Action::ActivateManaAbility { source, .. } => *source == id,
            _ => false,
        })
        .cloned()
        .collect()
}

/// Left edges for `count` cards of width `card` laid out in `width`.
///
/// Spaced by `gap` and centred while they fit; once they do not, overlapped evenly so the first
/// and last card still touch the edges. A full hand fans instead of wrapping onto a second row.
pub fn fan(count: usize, card: f32, width: f32, gap: f32) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    let natural = card + gap;
    let total = card + natural * (count - 1) as f32;
    let (start, step) = if total <= width || count == 1 {
        (((width - total) / 2.0).max(0.0), natural)
    } else {
        (0.0, ((width - card) / (count - 1) as f32).max(0.0))
    };
    (0..count).map(|i| start + step * i as f32).collect()
}

/// Which card a point at horizontal offset `x` is over, given [`fan`]'s edges.
///
/// Later cards are drawn over earlier ones, so where they overlap the later one wins — the card
/// the pointer visibly touches.
pub fn card_under(lefts: &[f32], card: f32, x: f32) -> Option<usize> {
    lefts.iter().rposition(|&left| x >= left && x < left + card)
}

/// Whether a view shows anything worth drawing yet.
pub fn is_empty(view: &PlayerView) -> bool {
    view.visible.is_empty() && view.stack.is_empty()
}

/// Look up one object in a view.
pub fn object(view: &PlayerView, id: ObjectId) -> Option<&ObjectView> {
    view.visible.get(&id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::ZoneRef;
    use mtg_engine::view::{ObjectView, PlayerSummary};
    use std::collections::BTreeMap;

    const ME: PlayerId = PlayerId(0);
    const THEM: PlayerId = PlayerId(1);

    #[test]
    fn identical_cards_in_different_zones_keep_their_cast_choices() {
        let actions = [
            Action::Cast {
                object: ObjectId(1),
            },
            Action::Cast {
                object: ObjectId(2),
            },
            Action::Cast {
                object: ObjectId(3),
            },
        ];
        let refs: Vec<_> = actions.iter().collect();
        let choices = distinct_actions_with_zones(
            &refs,
            |_| Some(CardId(7)),
            |id| {
                Some(ZoneRef::of(
                    if id == ObjectId(3) {
                        Zone::Graveyard
                    } else {
                        Zone::Hand
                    },
                    ME,
                ))
            },
        );
        assert_eq!(choices, vec![&actions[0], &actions[2]]);
    }

    #[test]
    fn library_browser_only_contains_disclosed_cards_from_that_library() {
        let mut view = a_view();
        assert!(visible_library_cards(&view, ME).is_empty());
        view.visible.insert(
            ObjectId(90),
            obj(90, ZoneRef::of(Zone::Library, ME), THEM, Some(CardId(1))),
        );
        view.visible.insert(
            ObjectId(91),
            obj(91, ZoneRef::of(Zone::Library, THEM), ME, Some(CardId(2))),
        );
        assert_eq!(visible_library_cards(&view, ME), vec![ObjectId(90)]);
        assert_eq!(visible_library_cards(&view, THEM), vec![ObjectId(91)]);
    }

    #[test]
    fn hand_cards_offer_special_actions_without_silently_choosing_over_casting() {
        let card = ObjectId(90);
        let other = ObjectId(91);
        let special = Action::SpecialAction {
            source: card,
            ability: mtg_core::AbilityId(3),
        };
        let actions = vec![
            Action::Cast { object: card },
            special.clone(),
            Action::SpecialAction {
                source: other,
                ability: mtg_core::AbilityId(4),
            },
        ];
        let options = hand_play_options(&actions, &[card]);
        assert_eq!(options.len(), 1);
        assert_eq!(options[&card], actions[..2]);
        assert!(
            hand_plays(&actions, &[card]).is_empty(),
            "casting and foretelling require a choice"
        );
        assert_eq!(
            hand_plays(std::slice::from_ref(&special), &[card])[&card],
            special
        );
    }

    fn obj(id: u32, zone: ZoneRef, controller: PlayerId, card: Option<CardId>) -> ObjectView {
        ObjectView {
            id: ObjectId(id),
            zone,
            controller,
            card,
            face: 0,
            adventure_player: None,
            tapped: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            targets: Vec::new(),
            is_ability: false,
            ability: None,
            attacking: false,
            attacking_target: None,
            blocking: None,
        }
    }

    /// A view as the host would project it for `ME`: own hand visible, opponent's not.
    fn a_view() -> PlayerView {
        let mut players = BTreeMap::new();
        players.insert(
            ME,
            PlayerSummary {
                id: ME,
                life: 20,
                poison: 0,
                energy: 0,
                hand_size: 2,
                library_size: 30,
                graveyard: vec![ObjectId(50)],
                mana: [0; 6],
                commander_damage: Default::default(),
            },
        );
        players.insert(
            THEM,
            PlayerSummary {
                id: THEM,
                life: 17,
                poison: 1,
                energy: 0,
                hand_size: 4,
                library_size: 28,
                graveyard: Vec::new(),
                mana: [0; 6],
                commander_damage: Default::default(),
            },
        );

        let mut visible = BTreeMap::new();
        for o in [
            obj(1, ZoneRef::shared(Zone::Battlefield), ME, Some(CardId(0))),
            obj(2, ZoneRef::shared(Zone::Battlefield), ME, Some(CardId(1))),
            obj(3, ZoneRef::shared(Zone::Battlefield), THEM, Some(CardId(0))),
            obj(10, ZoneRef::of(Zone::Hand, ME), ME, Some(CardId(1))),
            obj(11, ZoneRef::of(Zone::Hand, ME), ME, Some(CardId(0))),
            // The opponent's hand appears only as a count; if it were in the view at all it
            // would be card-less.
        ] {
            visible.insert(o.id, o);
        }

        PlayerView {
            revealed_cards: Vec::new(),
            prevent_combat_damage: false,
            prevent_damage_to: Vec::new(),
            viewer: ME,
            turn: 3,
            active_player: ME,
            step: mtg_core::Step::PrecombatMain,
            priority: Some(ME),
            players,
            visible,
            stack: vec![ObjectId(20)],
        }
    }

    #[test]
    fn the_viewer_is_separated_from_their_opponents() {
        let board = arrange(&a_view());
        assert_eq!(board.mine.player, ME);
        assert_eq!(board.opponents.len(), 1);
        assert_eq!(board.opponents[0].player, THEM);
    }

    #[test]
    fn permanents_are_grouped_by_controller() {
        let board = arrange(&a_view());
        assert_eq!(board.mine.battlefield, vec![ObjectId(1), ObjectId(2)]);
        assert_eq!(board.opponents[0].battlefield, vec![ObjectId(3)]);
    }

    #[test]
    fn the_viewers_hand_is_populated() {
        let board = arrange(&a_view());
        assert_eq!(board.mine.hand, vec![ObjectId(10), ObjectId(11)]);
    }

    #[test]
    fn an_opponents_hand_is_a_count_with_no_contents() {
        // The redaction, visible at the UI layer: the count is known, the cards are not.
        let board = arrange(&a_view());
        let theirs = &board.opponents[0];
        assert_eq!(theirs.hand_size, 4, "the count is known");
        assert!(theirs.hand.is_empty(), "but not a single card of it");
    }

    #[test]
    fn life_poison_and_energy_come_through_for_both_players() {
        let mut view = a_view();
        view.players.get_mut(&ME).unwrap().energy = 4;
        view.players.get_mut(&THEM).unwrap().energy = 2;
        let board = arrange(&view);
        assert_eq!(board.mine.life, 20);
        assert_eq!(board.opponents[0].life, 17);
        assert_eq!(board.opponents[0].poison, 1);
        assert_eq!(board.mine.energy, 4);
        assert_eq!(board.opponents[0].energy, 2);
    }

    #[test]
    fn the_stack_is_carried_top_first() {
        let board = arrange(&a_view());
        assert_eq!(board.stack, vec![ObjectId(20)]);
    }

    #[test]
    fn the_order_is_stable_between_frames() {
        // Otherwise the board would visibly reshuffle itself as the map iterates.
        let view = a_view();
        assert_eq!(
            arrange(&view).mine.battlefield,
            arrange(&view).mine.battlefield
        );
    }

    #[test]
    fn lands_are_split_into_their_own_row() {
        let battlefield = vec![ObjectId(1), ObjectId(2), ObjectId(3)];
        let (others, lands) = rows(&battlefield, |id| id.0 % 2 == 1);
        assert_eq!(lands, vec![ObjectId(1), ObjectId(3)]);
        assert_eq!(others, vec![ObjectId(2)]);
    }

    #[test]
    fn an_empty_view_is_recognised() {
        let mut view = a_view();
        assert!(!is_empty(&view));
        view.visible.clear();
        view.stack.clear();
        assert!(is_empty(&view));
    }

    #[test]
    fn only_cards_the_engine_offered_are_playable_from_hand() {
        let hand = [ObjectId(10), ObjectId(11), ObjectId(12)];
        let actions = [
            Action::PlayLand {
                object: ObjectId(10),
            },
            Action::Cast {
                object: ObjectId(12),
            },
            // Not in hand: a card cast from elsewhere is not the hand's business.
            Action::Cast {
                object: ObjectId(40),
            },
            Action::ActivateAbility {
                source: ObjectId(11),
                ability: mtg_core::AbilityId(0),
            },
            Action::Pass,
        ];
        let plays = hand_plays(&actions, &hand);
        assert_eq!(
            plays.keys().copied().collect::<Vec<_>>(),
            vec![ObjectId(10), ObjectId(12)]
        );
        assert!(matches!(plays[&ObjectId(12)], Action::Cast { .. }));
    }

    #[test]
    fn a_permanent_offers_its_own_abilities_and_each_mana_colour() {
        let ability = mtg_core::AbilityId(0);
        let tap = |source, color| Action::ActivateManaAbility {
            source: ObjectId(source),
            ability,
            color,
        };
        let legal = LegalActions {
            who: Some(ME),
            actions: vec![
                Action::ActivateAbility {
                    source: ObjectId(1),
                    ability,
                },
                Action::ActivateAbility {
                    source: ObjectId(2),
                    ability,
                },
                Action::Cast {
                    object: ObjectId(1),
                },
            ],
            mana_abilities: vec![
                tap(1, Some(mtg_core::Color::White)),
                tap(1, Some(mtg_core::Color::Green)),
                tap(3, None),
            ],
            targets: Vec::new(),
        };

        let one = permanent_actions(&legal, ObjectId(1));
        assert_eq!(
            one.len(),
            3,
            "its ability and both colours, nothing belonging to others"
        );
        assert!(
            matches!(one[0], Action::ActivateAbility { .. }),
            "abilities before mana"
        );
        assert_eq!(permanent_actions(&legal, ObjectId(3)), vec![tap(3, None)]);
        assert!(permanent_actions(&legal, ObjectId(9)).is_empty());
    }

    #[test]
    fn a_hand_that_fits_is_spaced_and_centred() {
        let lefts = fan(3, 100.0, 1000.0, 10.0);
        assert_eq!(lefts, vec![340.0, 450.0, 560.0]);
    }

    #[test]
    fn a_hand_that_does_not_fit_overlaps_edge_to_edge() {
        let lefts = fan(5, 100.0, 300.0, 10.0);
        assert_eq!(lefts.first(), Some(&0.0));
        assert_eq!(
            *lefts.last().unwrap() + 100.0,
            300.0,
            "the last card ends at the edge"
        );
        assert!(
            lefts.windows(2).all(|w| w[1] - w[0] < 100.0),
            "cards overlap"
        );
    }

    #[test]
    fn an_empty_or_single_hand_lays_out() {
        assert!(fan(0, 100.0, 500.0, 10.0).is_empty());
        assert_eq!(fan(1, 100.0, 500.0, 10.0), vec![200.0]);
        assert_eq!(
            fan(1, 100.0, 50.0, 10.0),
            vec![0.0],
            "never off the left edge"
        );
    }

    #[test]
    fn where_cards_overlap_the_one_on_top_is_under_the_pointer() {
        let lefts = [0.0, 50.0, 100.0];
        assert_eq!(card_under(&lefts, 100.0, 25.0), Some(0));
        assert_eq!(
            card_under(&lefts, 100.0, 75.0),
            Some(1),
            "1 is drawn over 0"
        );
        assert_eq!(card_under(&lefts, 100.0, 150.0), Some(2));
        assert_eq!(card_under(&lefts, 100.0, 250.0), None);
    }

    #[test]
    fn copies_of_the_same_card_offer_one_button() {
        // Objects 1 and 2 are the same land; 3 is a different card.
        let card_of = |id: ObjectId| Some(CardId(if id.0 == 3 { 9 } else { 0 }));
        let plays = [
            Action::PlayLand {
                object: ObjectId(1),
            },
            Action::PlayLand {
                object: ObjectId(2),
            },
            Action::PlayLand {
                object: ObjectId(3),
            },
        ];
        let refs: Vec<&Action> = plays.iter().collect();
        let kept = distinct_actions(&refs, card_of);
        assert_eq!(kept.len(), 2);
        assert!(matches!(
            kept[0],
            Action::PlayLand {
                object: ObjectId(1)
            }
        ));
    }

    #[test]
    fn abilities_of_identical_permanents_stay_separate() {
        // Which copy activates can matter — one may be damaged or carry counters.
        let card_of = |_| Some(CardId(0));
        let ability = mtg_core::AbilityId(0);
        let acts = [
            Action::ActivateAbility {
                source: ObjectId(1),
                ability,
            },
            Action::ActivateAbility {
                source: ObjectId(2),
                ability,
            },
        ];
        let refs: Vec<&Action> = acts.iter().collect();
        assert_eq!(distinct_actions(&refs, card_of).len(), 2);
    }

    #[test]
    fn face_choices_survive_copy_deduplication_and_require_an_explicit_hand_choice() {
        let first = ObjectId(1);
        let second = ObjectId(2);
        let actions = vec![
            Action::Cast { object: first },
            Action::CastFace {
                object: first,
                face: 1,
            },
            Action::Cast { object: second },
            Action::CastFace {
                object: second,
                face: 1,
            },
        ];
        let refs: Vec<_> = actions.iter().collect();
        let distinct = distinct_actions(&refs, |_| Some(CardId(7)));
        assert_eq!(distinct.len(), 2);
        assert_eq!(distinct[0], &actions[0]);
        assert_eq!(distinct[1], &actions[1]);
        let choices = hand_play_options(&actions, &[first, second]);
        assert_eq!(choices[&first], actions[..2]);
        assert_eq!(choices[&second], actions[2..]);
        assert!(
            hand_plays(&actions, &[first, second]).is_empty(),
            "a single-click helper must not silently choose one of two faces"
        );
    }
}
