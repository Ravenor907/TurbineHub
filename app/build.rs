use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    // Every .qml file in qml/ belongs to the TurbineHub module (Main.qml plus one file per component and page).
    let mut files: Vec<String> = std::fs::read_dir("qml")
        .expect("qml folder")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("qml"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    files.sort();
    for f in &files {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-changed=qml");
    // The version comes from the build (the Flatpak manifest sets it from VERSION).
    println!("cargo:rerun-if-env-changed=TURBINEHUB_VERSION");
    CxxQtBuilder::new_qml_module(QmlModule::new("TurbineHub").qml_files(files)).qt_module("Quick").qt_module("Gui").files(["src/backend.rs"]).cpp_file("src/qtglue.cpp").build();
}
