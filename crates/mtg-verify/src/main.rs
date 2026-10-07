use mtg_verify::{
    campaign,
    scenario::{GameScenario, run},
};
use std::{fs, path::Path};

fn main() {
    if let Err(error) = execute() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
fn execute() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let [command, path] = args.as_slice()
        && command == "confidence"
    {
        let report = mtg_verify::confidence::ConfidenceRegistry::load(Path::new(path))?.report()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let (command,path,output)=match args.as_slice() {
        [command,path] if command=="replay"=>("run",path.as_str(),None),
        [group,command,path] if group=="scenario"=>(command.as_str(),path.as_str(),None),
        [group,command,path,output] if group=="scenario"=>(command.as_str(),path.as_str(),Some(output.as_str())),
        _=>return Err("Usage: mtgo-rs scenario run FILE | replay FILE | scenario record FILE OUTPUT | scenario minimize FILE OUTPUT | scenario fuzz FILE OUTPUT | confidence REGISTRY.json".into()),
    };
    let scenario = GameScenario::load(Path::new(path))?;
    match command {
        "run" => {
            let (report, _) = run(&scenario, false)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            if !report.pass {
                return Err(report.message);
            }
        }
        "record" => {
            let (report, artifact) = run(&scenario, true)?;
            if !report.pass {
                return Err(report.message);
            }
            write(output, &artifact)?;
            println!("PASS {}", report.digest);
        }
        "minimize" => {
            let reduced = campaign::minimize(&scenario, 1000)?;
            write(output, &reduced)?;
            println!(
                "Reduced {} actions to {}",
                scenario.actions.len(),
                reduced.actions.len()
            );
        }
        "fuzz" => {
            let steps = std::env::var("MTGO_VERIFY_ACTIONS")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(1000);
            let report = campaign::semantic(&scenario, scenario.seed, steps)?;
            write(output, &report.reproduction)?;
            println!("{} actions, seed {}", report.actions, report.seed);
            if let Some(f) = report.failure {
                return Err(f);
            }
        }
        _ => return Err("Unknown scenario command".into()),
    }
    Ok(())
}
fn write(output: Option<&str>, artifact: &GameScenario) -> Result<(), String> {
    let path = output.ok_or("Missing output path")?;
    // Regression artifacts are additive; accidental overwrites require explicit removal.
    use std::io::Write;
    let mut file = fs::File::create_new(path).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(artifact).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
