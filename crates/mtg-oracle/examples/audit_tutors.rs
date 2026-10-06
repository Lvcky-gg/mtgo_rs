//! Audit exported library-search faces without modifying the card database.
//! Input: {"faces":[{"name":...,"type_line":...,"mana_cost":...,"oracle_text":...}],
//!         "subtypes":{"Forest":1,...}}
use mtg_oracle::{compile, convert::Subtypes, typeline};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Input {
    faces: Vec<Face>,
    subtypes: BTreeMap<String, u16>,
}
#[derive(Deserialize)]
struct Face {
    name: String,
    type_line: String,
    mana_cost: String,
    oracle_text: String,
}
impl Subtypes for Input {
    fn intern(&self, name: &str) -> Option<u16> {
        self.subtypes.get(name).copied()
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("provide exported tutor JSON")?;
    let input: Input = serde_json::from_reader(std::fs::File::open(path)?)?;
    let rows: Vec<_> = input.faces.iter().map(|face| {
        let types = typeline::parse(&face.type_line);
        let compiled = compile::compile(&compile::FaceText {
            name: &face.name,
            card_types: &types.card_types,
            subtypes: &types.subtypes,
            oracle_text: Some(&face.oracle_text),
            mana_cost: &face.mana_cost,
        }, &input);
        serde_json::json!({"name":face.name,"understood":compiled.understood(),"unparsed":compiled.unparsed})
    }).collect();
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}
