//! Versioned, bounded initial-state-plus-answer artifacts, not arbitrary engine snapshots.
use crate::canonical::{CanonicalGameState, diff, hash};
use mtg_core::{CardId, CounterKind, ManaPool, PlayerId, Step, ZoneRef};
use mtg_engine::{
    Choice, Engine, Progress,
    choice::{Answer, ChoiceKind},
    state::{GameObject, GameState, Rng},
};
use mtg_ir::{CardFace, PrintedCards};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

pub const FORMAT_VERSION: u32 = 1;
pub const RULES_VERSION: &str = "mtgo-supported-rules-v1";
pub const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ACTIONS: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameScenario {
    pub format_version: u32,
    pub rules_version: String,
    pub engine_version: String,
    pub card_db_hash: String,
    pub seed: u64,
    #[serde(default)]
    pub metadata: Metadata,
    pub cards: Vec<CardFace>,
    #[serde(default)]
    pub subtypes: Vec<String>,
    pub initial_state: ScenarioState,
    #[serde(default)]
    pub actions: Vec<ScenarioAction>,
    #[serde(default)]
    pub expected: ScenarioAssertions,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub issue: Option<String>,
    pub description: String,
    #[serde(default)]
    pub rules: Vec<String>,
    pub first_affected: Option<String>,
    pub fixed_in: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioState {
    pub players: Vec<ScenarioPlayer>,
    pub active_player: PlayerId,
    pub turn: u32,
    pub step: Step,
    #[serde(default)]
    pub objects: Vec<ScenarioObject>,
    #[serde(default)]
    pub commanders: BTreeMap<PlayerId, CardId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPlayer {
    pub id: PlayerId,
    pub life: i32,
    #[serde(default)]
    pub poison: u32,
    #[serde(default)]
    pub mana: ManaPool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioObject {
    pub card: CardId,
    pub owner: PlayerId,
    pub controller: Option<PlayerId>,
    pub zone: ZoneRef,
    #[serde(default)]
    pub tapped: bool,
    #[serde(default)]
    pub damage: u32,
    #[serde(default)]
    pub counters: BTreeMap<CounterKind, i32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioAction {
    pub who: PlayerId,
    pub answer: Answer,
    #[serde(default)]
    pub expected_choice: Option<ChoiceKind>,
    #[serde(default)]
    pub expected_state: Option<CanonicalGameState>,
    #[serde(default)]
    pub expected_digest: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioAssertions {
    #[serde(default)]
    pub life: BTreeMap<PlayerId, i32>,
    pub final_digest: Option<String>,
    pub final_state: Option<CanonicalGameState>,
}
#[derive(Debug, Serialize)]
pub struct RunReport {
    pub pass: bool,
    pub first_divergent_action: Option<usize>,
    pub message: String,
    pub digest: String,
    pub expected_state: Option<CanonicalGameState>,
    pub actual_state: Option<CanonicalGameState>,
    pub diff: Vec<String>,
}
impl GameScenario {
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(MAX_ARTIFACT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::parse(&bytes)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err("Scenario exceeds size limit".into());
        }
        let scenario: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        scenario.validate()?;
        Ok(scenario)
    }
    pub fn card_hash(&self) -> String {
        hash(&serde_json::to_vec(&(&self.cards, &self.subtypes)).expect("card IR serializes"))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != FORMAT_VERSION {
            return Err("Unsupported scenario format version".into());
        }
        if self.rules_version != RULES_VERSION {
            return Err("Rules version mismatch; explicit fixture migration required".into());
        }
        if self.engine_version.is_empty() {
            return Err("Missing engine/build provenance".into());
        }
        if self.card_db_hash != self.card_hash() {
            return Err("Card database fingerprint mismatch".into());
        }
        if self.actions.len() > MAX_ACTIONS
            || self.cards.len() > 4096
            || self.initial_state.objects.len() > 10000
        {
            return Err("Scenario complexity limit exceeded".into());
        }
        let players = &self.initial_state.players;
        if !(2..=8).contains(&players.len()) {
            return Err("Scenario must have 2–8 players".into());
        }
        let ids: std::collections::BTreeSet<_> = players.iter().map(|p| p.id).collect();
        if ids.len() != players.len() || !ids.contains(&self.initial_state.active_player) {
            return Err("Invalid or duplicate player".into());
        }
        for object in &self.initial_state.objects {
            if object.card.0 as usize >= self.cards.len()
                || !ids.contains(&object.owner)
                || object.controller.is_some_and(|p| !ids.contains(&p))
                || object.zone.player.is_some_and(|p| !ids.contains(&p))
                || object.counters.values().any(|n| *n < 0)
            {
                return Err("Invalid initial object".into());
            }
            let shared = object.zone.zone.is_shared();
            if shared != object.zone.player.is_none()
                || (!shared && object.zone.player != Some(object.owner))
            {
                return Err("Invalid zone ownership".into());
            }
            if object.zone.zone == mtg_core::Zone::Stack {
                return Err("Initial stack continuations are not supported; record their announcement actions".into());
            }
        }
        for (player, card) in &self.initial_state.commanders {
            if !ids.contains(player) || card.0 as usize >= self.cards.len() {
                return Err("Invalid commander".into());
            }
        }
        if self.actions.iter().any(|a| !ids.contains(&a.who)) {
            return Err("Action has unknown player".into());
        }
        Ok(())
    }
    pub fn setup(&self) -> Result<(Engine, ScenarioCards), String> {
        self.validate()?;
        let seats: Vec<_> = self.initial_state.players.iter().map(|p| p.id).collect();
        let mut state = GameState::new(&seats, 20);
        state.active_player = self.initial_state.active_player;
        state.turn = self.initial_state.turn;
        state.step = self.initial_state.step;
        let mut seed = [0; 32];
        for chunk in seed.chunks_mut(8) {
            chunk.copy_from_slice(&self.seed.to_le_bytes());
        }
        state.rng = Rng::from_seed(&seed);
        for player in &self.initial_state.players {
            let p = state.players.get_mut(&player.id).unwrap();
            p.life = player.life;
            p.starting_life = player.life;
            p.poison = player.poison;
            p.mana = player.mana.clone();
        }
        state.commander.commanders = self.initial_state.commanders.clone();
        for input in &self.initial_state.objects {
            let id = state.new_object_id();
            let mut object = GameObject::new(id, input.card, input.owner, input.zone);
            object.controller = input.controller.unwrap_or(input.owner);
            object.tapped = input.tapped;
            object.damage = input.damage;
            object.counters = input.counters.clone();
            object.summoning_sick = false;
            state.objects.insert(id, object);
            if input.zone.zone.is_ordered() {
                state.zone_order.entry(input.zone).or_default().push(id);
            }
        }
        let mut cards = ScenarioCards {
            faces: self.cards.clone(),
            subtypes: self.subtypes.clone(),
            tokens: vec![],
        };
        cards.tokens =
            mtg_ir::walk::register_tokens(cards.faces.iter_mut(), mtg_ir::walk::TOKEN_CARD_BASE);
        Ok((Engine::new(state), cards))
    }
}
pub struct ScenarioCards {
    faces: Vec<CardFace>,
    subtypes: Vec<String>,
    tokens: Vec<CardFace>,
}
impl PrintedCards for ScenarioCards {
    fn face(&self, card: CardId, face: u8) -> Option<&CardFace> {
        if face != 0 {
            return None;
        }
        if card.0 >= mtg_ir::walk::TOKEN_CARD_BASE {
            self.tokens
                .get((card.0 - mtg_ir::walk::TOKEN_CARD_BASE) as usize)
        } else {
            self.faces.get(card.0 as usize)
        }
    }
    fn subtype_name(&self, subtype: mtg_core::Subtype) -> Option<&str> {
        self.subtypes.get(subtype.0 as usize).map(String::as_str)
    }
}
pub fn next_choice(
    engine: &mut Engine,
    cards: &dyn PrintedCards,
) -> Result<Option<Choice>, String> {
    for _ in 0..10000 {
        match engine.advance(cards) {
            Progress::Continue => {}
            Progress::NeedsChoice(choice) => return Ok(Some(choice)),
            Progress::GameOver { .. } => return Ok(None),
        }
    }
    Err("Engine advance budget exceeded (possible nontermination)".into())
}
pub fn run(scenario: &GameScenario, record: bool) -> Result<(RunReport, GameScenario), String> {
    let (mut engine, cards) = scenario.setup()?;
    let mut artifact = scenario.clone();
    let mut choice = next_choice(&mut engine, &cards)?;
    for (index, action) in scenario.actions.iter().enumerate() {
        let Some(current) = choice else {
            return Ok((
                failure(
                    index,
                    "Game ended before recorded action",
                    None,
                    CanonicalGameState::from_engine(&engine)?,
                ),
                artifact,
            ));
        };
        if current.who != action.who
            || (!record
                && action.expected_choice.as_ref().is_some_and(|kind| {
                    serde_json::to_value(kind).ok() != serde_json::to_value(&current.kind).ok()
                }))
        {
            return Ok((
                failure(
                    index,
                    &format!("Unexpected choice: {:?}", current.kind),
                    action.expected_state.clone(),
                    CanonicalGameState::from_engine(&engine)?,
                ),
                artifact,
            ));
        }
        if record {
            artifact.actions[index].expected_choice = Some(current.kind);
        }
        if let Err(error) = engine.answer(&cards, current.id, action.answer.clone()) {
            return Ok((
                failure(
                    index,
                    &format!("Illegal recorded answer: {error:?}"),
                    action.expected_state.clone(),
                    CanonicalGameState::from_engine(&engine)?,
                ),
                artifact,
            ));
        }
        choice = next_choice(&mut engine, &cards)?;
        if choice.is_some()
            && let Err(error) = crate::invariants::verify(&engine.state, &cards)
        {
            return Ok((
                failure(
                    index,
                    &error,
                    None,
                    CanonicalGameState::from_engine(&engine)?,
                ),
                artifact,
            ));
        }
        let state = CanonicalGameState::from_engine(&engine)?;
        if !record
            && (action.expected_state.as_ref().is_some_and(|e| e != &state)
                || action
                    .expected_digest
                    .as_ref()
                    .is_some_and(|e| e != &state.digest()))
        {
            return Ok((
                failure(
                    index,
                    "First divergent checkpoint",
                    action.expected_state.clone(),
                    state,
                ),
                artifact,
            ));
        }
        if record {
            artifact.actions[index].expected_digest = Some(state.digest());
            artifact.actions[index].expected_state = Some(state);
        }
    }
    let state = CanonicalGameState::from_engine(&engine)?;
    for (player, life) in &scenario.expected.life {
        if engine
            .state
            .players
            .get(player)
            .is_none_or(|p| p.life != *life)
        {
            return Ok((
                failure(
                    scenario.actions.len(),
                    &format!("Expected player {} life {life}", player.0),
                    scenario.expected.final_state.clone(),
                    state,
                ),
                artifact,
            ));
        }
    }
    if !record
        && (scenario
            .expected
            .final_digest
            .as_ref()
            .is_some_and(|d| d != &state.digest())
            || scenario
                .expected
                .final_state
                .as_ref()
                .is_some_and(|e| e != &state))
    {
        return Ok((
            failure(
                scenario.actions.len(),
                "Final state differs",
                scenario.expected.final_state.clone(),
                state,
            ),
            artifact,
        ));
    }
    if record {
        artifact.expected.final_digest = Some(state.digest());
        artifact.expected.final_state = Some(state.clone());
    }
    Ok((
        RunReport {
            pass: true,
            first_divergent_action: None,
            message: "PASS".into(),
            digest: state.digest(),
            expected_state: None,
            actual_state: Some(state),
            diff: vec![],
        },
        artifact,
    ))
}
fn failure(
    index: usize,
    message: &str,
    expected: Option<CanonicalGameState>,
    actual: CanonicalGameState,
) -> RunReport {
    let differences = expected
        .as_ref()
        .map(|e| diff(&e.state, &actual.state))
        .unwrap_or_default();
    RunReport {
        pass: false,
        first_divergent_action: Some(index),
        message: message.into(),
        digest: actual.digest(),
        expected_state: expected,
        actual_state: Some(actual),
        diff: differences,
    }
}
