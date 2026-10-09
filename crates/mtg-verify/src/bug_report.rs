//! Full-information replay attachments for private bug triage.
//! Captured observations are evidence of reproduction, never rules expectations.
use crate::scenario::{GameScenario, MAX_ARTIFACT_BYTES, RunReport, run};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

pub const FORMAT_VERSION: u32 = 1;
// Includes the scenario and both expected/actual canonical diagnostic states.
pub const MAX_REPORT_BYTES: usize = 4 * MAX_ARTIFACT_BYTES;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayReport {
    pub format_version: u32,
    pub scenario: GameScenario,
    pub observed: RunReport,
}

pub fn capture(scenario: &GameScenario) -> Result<ReplayReport, String> {
    let (observed, _) = run(scenario, false)?;
    Ok(ReplayReport {
        format_version: FORMAT_VERSION,
        scenario: scenario.clone(),
        observed,
    })
}

impl ReplayReport {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_REPORT_BYTES {
            return Err("Replay report exceeds size limit".into());
        }
        let report: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        report.validate()?;
        Ok(report)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_REPORT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::parse(&bytes)
    }

    fn validate(&self) -> Result<(), String> {
        if self.format_version != FORMAT_VERSION {
            return Err("Unsupported replay report format version".into());
        }
        // Enforce the same nested scenario limits as standalone replay parsing.
        GameScenario::parse(&serde_json::to_vec(&self.scenario).map_err(|e| e.to_string())?)?;
        Ok(())
    }

    /// A reproduced failing scenario is a successful report verification.
    /// Differences indicate changed behavior or untrustworthy captured observations.
    pub fn verify(&self) -> Result<RunReport, String> {
        self.validate()?;
        let (actual, _) = run(&self.scenario, false)?;
        if actual != self.observed {
            return Err(format!(
                "Replay report diverged: captured action {:?} digest {}, actual action {:?} digest {}; captured {}, actual {}",
                self.observed.first_divergent_action,
                self.observed.digest,
                actual.first_divergent_action,
                actual.digest,
                self.observed.message,
                actual.message
            ));
        }
        Ok(actual)
    }
}
