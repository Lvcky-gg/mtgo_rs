//! Check the Rust value network against the training script's own outputs.
//!
//! `train_value.py` writes `value.bin` and `value.bin.check.json`: encoded held-out positions
//! with the value PyTorch gave each. This runs the same positions through
//! [`ValueNet::evaluate`] and fails if any differs, so a mismatch between the two forward
//! passes can never reach the arena unnoticed.
//!
//! Run: `cargo run --release -p mtg-app --example value_check -- PATH/value.bin`

use mtg_policy::search::{
    encode::{Encoded, NC, NG, Slot, Token},
    value::ValueNet,
};

#[derive(serde::Deserialize)]
struct Case {
    global: Vec<f32>,
    tokens: Vec<(u8, u32, Vec<f32>)>,
    value: f64,
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: value_check PATH/value.bin");
    let net = ValueNet::load(path.as_ref()).unwrap_or_else(|e| panic!("{e}"));
    let cases: Vec<Case> = serde_json::from_str(
        &std::fs::read_to_string(format!("{path}.check.json")).expect("read the check file"),
    )
    .expect("parse the check file");
    let mut worst = 0f64;
    let mut encoded = Vec::new();
    for c in &cases {
        let mut global = [0f32; NG];
        global.copy_from_slice(&c.global);
        let tokens = c
            .tokens
            .iter()
            .map(|(slot, name, f)| {
                let mut features = [0f32; NC];
                features.copy_from_slice(f);
                Token {
                    slot: Slot::from_index(*slot).expect("a known slot"),
                    name: *name,
                    features,
                }
            })
            .collect();
        let e = Encoded { global, tokens };
        let rust = net.evaluate(&e);
        encoded.push(e);
        worst = worst.max((rust - c.value).abs());
        println!("python {:.6}  rust {rust:.6}", c.value);
    }
    println!("{} positions, largest difference {worst:.2e}", cases.len());
    let started = std::time::Instant::now();
    let mut n = 0u32;
    while started.elapsed().as_millis() < 500 {
        for e in &encoded {
            std::hint::black_box(net.evaluate(e));
            n += 1;
        }
    }
    println!(
        "{:.0} µs per evaluation",
        started.elapsed().as_secs_f64() * 1e6 / f64::from(n)
    );
    assert!(worst < 1e-4, "the Rust and PyTorch forward passes disagree");
}
