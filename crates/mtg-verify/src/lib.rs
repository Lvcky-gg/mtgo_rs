//! Offline, full-information verification. Never send scenario or diagnostic artifacts
//! to opponents: they intentionally contain private cards and RNG state.
pub mod campaign;
pub mod canonical;
pub mod confidence;
pub mod differential;
pub mod invariants;
pub mod mutation;
pub mod scenario;

pub mod structural;
