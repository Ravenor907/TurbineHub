//! Prints what Turbine Hub detects on this machine: cargo run --example machine
fn main() {
    let efi = std::process::Command::new("efibootmgr").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let m = turbinehub_engine::hardware::Machine::read(std::path::Path::new("/"), &efi, &turbinehub_engine::hardware::device_of(&turbinehub_engine::home_dir()));
    println!("{} ({:?}), on TV: {}\n{m:#?}", m.name(), m.kind, m.on_tv());
}
