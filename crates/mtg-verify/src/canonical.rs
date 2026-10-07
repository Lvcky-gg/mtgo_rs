use mtg_engine::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalGameState {
    pub format_version: u32,
    pub state: Value,
}

impl CanonicalGameState {
    pub fn from_engine(engine: &Engine) -> Result<Self, String> {
        let mut state = engine.verification_snapshot()?;
        let mut ids = BTreeSet::new();
        let mut times = BTreeSet::new();
        collect(&state, "$ObjectId", &mut ids);
        collect(&state, "$Timestamp", &mut times);
        let ranks = |set: BTreeSet<u64>| -> BTreeMap<u64, u64> {
            set.into_iter()
                .enumerate()
                .map(|(i, n)| (n, i as u64 + 1))
                .collect()
        };
        normalize(&mut state, &ranks(ids), &ranks(times));
        Ok(Self {
            format_version: 1,
            state,
        })
    }
    pub fn digest(&self) -> String {
        hash(&serde_json::to_vec(self).expect("JSON value serializes"))
    }
}
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn collect(value: &Value, tag: &str, out: &mut BTreeSet<u64>) {
    match value {
        Value::Object(map) => {
            if let Some(n) = map.get(tag).and_then(Value::as_u64) {
                out.insert(n);
            }
            for v in map.values() {
                collect(v, tag, out);
            }
        }
        Value::Array(values) => {
            for v in values {
                collect(v, tag, out);
            }
        }
        _ => {}
    }
}
fn normalize(value: &mut Value, ids: &BTreeMap<u64, u64>, times: &BTreeMap<u64, u64>) {
    match value {
        Value::Object(map) => {
            for (tag, ranks) in [("$ObjectId", ids), ("$Timestamp", times)] {
                if let Some(n) = map.get(tag).and_then(Value::as_u64) {
                    map.insert(tag.into(), Value::from(ranks[&n]));
                }
            }
            // Choice correlation ids, explanatory UI text and event sequence ids are not rules.
            if map.contains_key("because") && map.contains_key("kind") {
                map.remove("id");
                map.remove("because");
            }
            if map.contains_key("event") && map.contains_key("cause") {
                map.remove("id");
            }
            for v in map.values_mut() {
                normalize(v, ids, times);
            }
            if let Some(Value::Array(pairs)) = map.get_mut("$map") {
                pairs.sort_by_cached_key(|v| serde_json::to_string(&v[0]).unwrap());
            }
        }
        Value::Array(values) => {
            for v in values {
                normalize(v, ids, times);
            }
        }
        _ => {}
    }
}
pub fn diff(expected: &Value, actual: &Value) -> Vec<String> {
    fn visit(path: String, a: &Value, b: &Value, out: &mut Vec<String>) {
        if a == b || out.len() >= 32 {
            return;
        }
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
                for k in keys {
                    visit(
                        format!("{path}/{k}"),
                        a.get(k).unwrap_or(&Value::Null),
                        b.get(k).unwrap_or(&Value::Null),
                        out,
                    );
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    visit(format!("{path}/{i}"), a, b, out);
                }
            }
            _ => out.push(format!("{path}: expected {a}, actual {b}")),
        }
    }
    let mut out = vec![];
    visit(String::new(), expected, actual, &mut out);
    out
}
