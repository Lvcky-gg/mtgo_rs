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
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let settings = match parse_settings(&args) {
        Ok(Some(settings)) => settings,
        Ok(None) => {
            println!("mtg-play [--turns N] [-v|--verbose] [--stops]");
            return;
        }
        Err(error) => {
            eprintln!("error: {error}; try `mtg-play --help`");
            std::process::exit(2);
        }
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

fn parse_settings(args: &[String]) -> Result<Option<Settings>, String> {
    let mut settings = Settings::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--turns" => {
                let value = args.next().ok_or("--turns needs a number")?;
                settings.turns = value
                    .parse()
                    .map_err(|_| format!("invalid turn count {value:?}"))?;
            }
            "-v" | "--verbose" => settings.verbose = true,
            "--stops" => settings.policy.auto_pass = false,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Some(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn valid_options_preserve_defaults_and_select_the_requested_run() {
        let defaults = parse_settings(&[]).unwrap().unwrap();
        assert_eq!(defaults.turns, 3);
        assert!(defaults.policy.auto_pass);
        assert!(!defaults.verbose);
        let settings = parse_settings(&args(&["--stops", "--turns", "8", "-v"]))
            .unwrap()
            .unwrap();
        assert_eq!(settings.turns, 8);
        assert!(!settings.policy.auto_pass);
        assert!(settings.verbose);
    }

    #[test]
    fn invalid_options_are_errors_instead_of_running_three_turns() {
        for invalid in [
            vec!["--turns"],
            vec!["--turns", "nope"],
            vec!["--turns", "-1"],
            vec!["--turns", "4294967296"],
            vec!["--truns", "8"],
        ] {
            assert!(parse_settings(&args(&invalid)).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn help_returns_without_starting_a_game() {
        for flag in ["-h", "--help"] {
            assert!(parse_settings(&args(&[flag])).unwrap().is_none());
        }
    }
}
