//! Offline, full-information verification. Never send scenario or diagnostic artifacts
//! to opponents: they intentionally contain private cards and RNG state.
pub mod bug_report;
pub mod campaign;
pub mod canonical;
pub mod confidence;
pub mod differential;
pub mod invariants;
pub mod mana_reference;
pub mod mutation;
pub mod scenario;

pub mod structural;
pub mod target_reference;
