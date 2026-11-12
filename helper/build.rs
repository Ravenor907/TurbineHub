// The helper names the Turbine Hub version it was built with (`status`). Cargo only rebuilds for a new version when
// it is told the version comes from the environment.
fn main() {
    println!("cargo:rerun-if-env-changed=TURBINEHUB_VERSION");
}
