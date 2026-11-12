//! Runs one `--json` job against this machine: cargo run --example machine -- checkup
fn main() {
    let job = std::env::args().nth(1).unwrap_or_else(|| "machine".into());
    let efibootmgr = std::process::Command::new("efibootmgr").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let home = turbinehub_engine::home_dir();
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(Into::into).unwrap_or_else(|| home.join(".config"));
    let ctx = turbinehub_engine::cli::Ctx { root: "/".into(), home, config_home, efibootmgr, version: "dev".into() };
    let (text, code) = turbinehub_engine::cli::run(&job, &ctx);
    println!("{text}");
    std::process::exit(code);
}
