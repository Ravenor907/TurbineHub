//! The `--json` jobs without the app, for the build's checks and for trying the quick panel's backend:
//!   cargo run --example json -- --self-test <home>        every job on each fake machine with this home folder
//!   cargo run --example json -- <root> <home> <job>       one job, read from these folders
use turbinehub_engine::cli::{self, Ctx};
use turbinehub_engine::fake::{self, Preset};
use turbinehub_engine::json::{self, Json};

fn ctx(root: &std::path::Path, home: &std::path::Path) -> Ctx {
    Ctx { root: root.into(), home: home.into(), config_home: home.join(".config"), efibootmgr: fake::EFI_WITH_WINDOWS.into(), version: "dev".into() }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--self-test", home] => {
            let home = std::path::Path::new(home);
            let mut problems = 0;
            for (name, preset) in [("ally-x", Preset::AllyX), ("desktop", Preset::Desktop), ("tv-pc", Preset::TvPc)] {
                let root = std::env::temp_dir().join(format!("turbinehub-json-{name}-{}", std::process::id()));
                fake::machine(&root, preset);
                let c = ctx(&root, home);
                for job in cli::JOBS.iter().copied().filter(|j| !["game", "open"].contains(j)).chain(["game=1245620", "game=epic:Heron"]) {
                    let (text, code) = cli::run(job, &c);
                    let ok = code == 0 && json::parse(&text).ok().and_then(|j| j.get("ok").and_then(Json::as_bool)) == Some(true);
                    if !ok {
                        problems += 1;
                        println!("::error title=--json {job} on {name}::exit {code}: {}", text.chars().take(300).collect::<String>());
                    }
                }
                let (text, _) = cli::run("game=1245620", &c);
                if json::parse(&text).ok().and_then(|j| j.get("found").and_then(Json::as_bool)) != Some(true) {
                    problems += 1;
                    println!("::error title=--json game on {name}::the fake home's game isn't found: {text}");
                }
                println!("{name}: {}", cli::run("panel", &c).0);
                let _ = std::fs::remove_dir_all(&root);
            }
            println!("--json jobs: {problems} problems");
            std::process::exit(if problems == 0 { 0 } else { 1 });
        }
        [root, home, job] => {
            let (text, code) = cli::run(job, &ctx(std::path::Path::new(root), std::path::Path::new(home)));
            println!("{text}");
            std::process::exit(code);
        }
        _ => {
            eprintln!("json --self-test <home> | json <root> <home> <job>");
            std::process::exit(2);
        }
    }
}
