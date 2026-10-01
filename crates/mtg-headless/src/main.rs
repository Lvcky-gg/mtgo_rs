//! `mtg-play` — run a game in the terminal.
//!
//! Usage:
//! ```text
//!   mtg-play                 3 turns, step-level trace
//!   mtg-play --turns 8       more turns
//!   mtg-play -v              every event, not just steps
//!   mtg-play --stops         turn off auto-pass, to see what MTGO would ask
//! ```

use mtg_headless::{
    cards::DemoCards,
    opening_game,
    trace::{self, Settings},
};
use mtg_policy::Policy;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| -> Option<u32> {
        let i = args.iter().position(|a| a == name)?;
        args.get(i + 1)?.parse().ok()
    };

    if flag("--help") || flag("-h") {
        println!("mtg-play [--turns N] [-v|--verbose] [--stops]");
        return;
    }

    let settings = Settings {
        turns: value("--turns").unwrap_or(3),
        verbose: flag("-v") || flag("--verbose"),
        policy: Policy {
            // `--stops` disables auto-pass, which is the clearest way to see what
            // the policy layer is actually saving you from.
            auto_pass: !flag("--stops"),
            ..Policy::default()
        },
        ..Settings::default()
    };

    let cards = DemoCards::default();
    let (engine, report) = trace::run(opening_game(), &cards, &settings);

    for line in &report.lines {
        println!("{line}");
    }

    println!("\n── board ──");
    for line in trace::board_summary(&engine, &cards) {
        println!("{line}");
    }

    println!("\n── summary ──");
    println!("turns played      {}", report.turns_played);
    println!("events logged     {}", report.events);
    println!("auto-answered     {}", report.auto_answered);
    println!("shown to player   {}", report.asked);
    println!("ordering prompts  {}", report.ordering_prompts);
    if report.stalled {
        println!("\nstalled — the engine stopped making progress");
        std::process::exit(1);
    }
}
