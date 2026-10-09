//! Confidence is an evidence claim, not code coverage. Evidence must be tied to
//! the build assessed; self-authored tests alone never establish VERIFIED.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    Experimental,
    Tested,
    Verified,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub build: String,
    pub artifact: String,
    pub author: String,
    pub independent_reviewer: Option<String>,
    pub golden_tests: u64,
    pub property_cases: u64,
    pub differential_cases: u64,
    pub fuzz_actions: u64,
    pub regressions: u64,
    pub production_mutants_killed: u64,
    pub production_mutants_total: u64,
    pub failures: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrimitiveEvidence {
    pub primitive: String,
    pub evidence: Vec<Evidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thresholds {
    pub property_cases: u64,
    pub differential_cases: u64,
    pub fuzz_actions: u64,
    pub min_production_mutants: u64,
    pub mutation_kill_rate: f64,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self {
            property_cases: 10_000,
            differential_cases: 1_000,
            fuzz_actions: 10_000,
            min_production_mutants: 20,
            mutation_kill_rate: 0.95,
        }
    }
}
impl PrimitiveEvidence {
    pub fn confidence(&self, build: &str, thresholds: &Thresholds) -> Confidence {
        let evidence: Vec<_> = self.evidence.iter().filter(|e| e.build == build).collect();
        if evidence.is_empty()
            || evidence
                .iter()
                .any(|e| e.failures != 0 || e.artifact.is_empty())
        {
            return Confidence::Experimental;
        }
        // Deduplicate artifact references: rerendering one report is not more evidence.
        let mut seen = BTreeSet::new();
        let reviewed: Vec<_> = evidence
            .iter()
            .filter(|e| {
                e.independent_reviewer
                    .as_ref()
                    .is_some_and(|r| !r.is_empty() && r != &e.author)
                    && !e.author.is_empty()
                    && seen.insert(e.artifact.as_str())
            })
            .collect();
        let sum =
            |f: fn(&Evidence) -> u64| reviewed.iter().fold(0_u64, |n, e| n.saturating_add(f(e)));
        let total = sum(|e| e.production_mutants_total);
        let killed = sum(|e| e.production_mutants_killed);
        let valid_mutations = reviewed
            .iter()
            .all(|e| e.production_mutants_killed <= e.production_mutants_total);
        let verified = sum(|e| e.golden_tests) > 0
            && sum(|e| e.property_cases) >= thresholds.property_cases
            && sum(|e| e.differential_cases) >= thresholds.differential_cases
            && sum(|e| e.fuzz_actions) >= thresholds.fuzz_actions
            && total >= thresholds.min_production_mutants
            && total > 0
            && valid_mutations
            && thresholds.mutation_kill_rate.is_finite()
            && (0.0..=1.0).contains(&thresholds.mutation_kill_rate)
            && killed as f64 / total as f64 >= thresholds.mutation_kill_rate;
        if verified {
            Confidence::Verified
        } else if evidence
            .iter()
            .any(|e| e.golden_tests.saturating_add(e.regressions) > 0)
        {
            Confidence::Tested
        } else {
            Confidence::Experimental
        }
    }
}
/// Missing dependencies are experimental; unsupported empty dependency graphs
/// cannot earn confidence vacuously.
pub fn card_confidence(
    dependencies: &[String],
    primitives: &BTreeMap<String, Confidence>,
) -> Confidence {
    dependencies
        .iter()
        .map(|id| {
            primitives
                .get(id)
                .copied()
                .unwrap_or(Confidence::Experimental)
        })
        .min()
        .unwrap_or(Confidence::Experimental)
}

pub const REGISTRY_FORMAT_VERSION: u32 = 1;
pub const MAX_REGISTRY_BYTES: usize = 1024 * 1024;

/// Review identities and artifact paths are advisory metadata, not authenticated
/// proof of review. Only a trusted CI/review process may publish release evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceRegistry {
    pub format_version: u32,
    pub build: String,
    pub thresholds: Thresholds,
    pub primitives: Vec<PrimitiveEvidence>,
    pub cards: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfidenceReport {
    pub format_version: u32,
    pub build: String,
    pub primitives: BTreeMap<String, Confidence>,
    pub cards: BTreeMap<String, Confidence>,
    pub missing_dependencies: BTreeMap<String, Vec<String>>,
}

impl ConfidenceRegistry {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take((MAX_REGISTRY_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Self::parse(&bytes)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_REGISTRY_BYTES {
            return Err("Confidence registry exceeds size limit".into());
        }
        let registry: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        registry.validate()?;
        Ok(registry)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != REGISTRY_FORMAT_VERSION {
            return Err("Unsupported confidence registry version".into());
        }
        if self.build.trim().is_empty() {
            return Err("Missing assessed build identity".into());
        }
        let thresholds = &self.thresholds;
        if thresholds.property_cases == 0
            || thresholds.differential_cases == 0
            || thresholds.fuzz_actions == 0
            || thresholds.min_production_mutants == 0
            || !thresholds.mutation_kill_rate.is_finite()
            || !(0.0..=1.0).contains(&thresholds.mutation_kill_rate)
        {
            return Err("Invalid confidence thresholds".into());
        }
        let mut ids = BTreeSet::new();
        for primitive in &self.primitives {
            if primitive.primitive.trim().is_empty() || !ids.insert(&primitive.primitive) {
                return Err("Empty or duplicate primitive identifier".into());
            }
            for evidence in &primitive.evidence {
                if evidence.build.trim().is_empty()
                    || evidence.artifact.trim().is_empty()
                    || evidence.author.trim().is_empty()
                    || evidence.production_mutants_killed > evidence.production_mutants_total
                {
                    return Err(format!("Invalid evidence for {}", primitive.primitive));
                }
            }
        }
        for (card, dependencies) in &self.cards {
            if card.trim().is_empty()
                || dependencies
                    .iter()
                    .any(|dependency| dependency.trim().is_empty())
            {
                return Err("Empty card or dependency identifier".into());
            }
        }
        // Missing dependencies are allowed, visible in reports, and experimental.
        Ok(())
    }
    pub fn report(&self) -> Result<ConfidenceReport, String> {
        self.validate()?;
        let primitives: BTreeMap<_, _> = self
            .primitives
            .iter()
            .map(|primitive| {
                (
                    primitive.primitive.clone(),
                    primitive.confidence(&self.build, &self.thresholds),
                )
            })
            .collect();
        let cards = self
            .cards
            .iter()
            .map(|(card, dependencies)| (card.clone(), card_confidence(dependencies, &primitives)))
            .collect();
        let missing_dependencies = self
            .cards
            .iter()
            .filter_map(|(card, dependencies)| {
                let missing: Vec<_> = dependencies
                    .iter()
                    .filter(|dependency| !primitives.contains_key(*dependency))
                    .cloned()
                    .collect();
                (!missing.is_empty()).then(|| (card.clone(), missing))
            })
            .collect();
        Ok(ConfidenceReport {
            format_version: REGISTRY_FORMAT_VERSION,
            build: self.build.clone(),
            primitives,
            cards,
            missing_dependencies,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SolverExposure {
    pub trajectories: u64,
    pub fully_verified: u64,
    pub tested_exposure: u64,
    pub experimental_exposure: u64,
}
impl SolverExposure {
    /// Supply the minimum confidence observed anywhere along a complete trajectory.
    pub fn record(&mut self, confidence: Confidence) {
        self.trajectories += 1;
        match confidence {
            Confidence::Verified => self.fully_verified += 1,
            Confidence::Tested => self.tested_exposure += 1,
            Confidence::Experimental => self.experimental_exposure += 1,
        }
    }
    pub fn permitted_for_competitive_analysis(confidence: Confidence) -> bool {
        confidence == Confidence::Verified
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builder_only_and_stale_reports_cannot_verify() {
        let evidence = Evidence {
            build: "build-a".into(),
            artifact: "report.json".into(),
            author: "builder".into(),
            independent_reviewer: None,
            golden_tests: 42,
            property_cases: 100_000,
            differential_cases: 100_000,
            fuzz_actions: 100_000,
            regressions: 0,
            production_mutants_killed: 20,
            production_mutants_total: 20,
            failures: 0,
        };
        let mut primitive = PrimitiveEvidence {
            primitive: "sba".into(),
            evidence: vec![evidence],
        };
        assert_eq!(
            primitive.confidence("build-a", &Thresholds::default()),
            Confidence::Tested
        );
        assert_eq!(
            primitive.confidence("build-b", &Thresholds::default()),
            Confidence::Experimental
        );
        primitive.evidence[0].independent_reviewer = Some("rules-reviewer".into());
        assert_eq!(
            primitive.confidence("build-a", &Thresholds::default()),
            Confidence::Verified
        );
        primitive.evidence[0].failures = 1;
        assert_eq!(
            primitive.confidence("build-a", &Thresholds::default()),
            Confidence::Experimental
        );
    }
    #[test]
    fn missing_card_dependency_propagates_to_solver() {
        let confidence = card_confidence(&["unknown".into()], &BTreeMap::new());
        let mut exposure = SolverExposure::default();
        exposure.record(confidence);
        assert_eq!(exposure.experimental_exposure, 1);
        assert!(!SolverExposure::permitted_for_competitive_analysis(
            confidence
        ));
    }
    #[test]
    fn registry_missing_dependency_is_reported_and_experimental() {
        let registry = ConfidenceRegistry {
            format_version: REGISTRY_FORMAT_VERSION,
            build: "test-build".into(),
            thresholds: Thresholds::default(),
            primitives: vec![],
            cards: BTreeMap::from([("fixture".into(), vec!["unknown".into()])]),
        };
        let report = registry.report().unwrap();
        assert_eq!(report.cards["fixture"], Confidence::Experimental);
        assert_eq!(report.missing_dependencies["fixture"], vec!["unknown"]);
        let bytes = serde_json::to_vec(&registry).unwrap();
        assert!(ConfidenceRegistry::parse(&bytes).is_ok());
    }
    #[test]
    fn registry_rejects_invalid_version_and_thresholds() {
        let mut registry = ConfidenceRegistry {
            format_version: REGISTRY_FORMAT_VERSION + 1,
            build: "test-build".into(),
            thresholds: Thresholds::default(),
            primitives: vec![],
            cards: BTreeMap::new(),
        };
        assert!(registry.report().is_err());
        registry.format_version = REGISTRY_FORMAT_VERSION;
        registry.thresholds.property_cases = 0;
        assert!(registry.report().is_err());
    }
    #[test]
    fn bootstrap_registry_never_claims_accepted_evidence() {
        let registry =
            ConfidenceRegistry::parse(include_bytes!("../../../verification/primitives.json"))
                .unwrap();
        let report = registry.report().unwrap();
        assert_eq!(report.primitives.len(), 13);
        assert!(
            report
                .primitives
                .values()
                .chain(report.cards.values())
                .all(|confidence| *confidence == Confidence::Experimental)
        );
        assert!(report.missing_dependencies.is_empty());
    }
}
