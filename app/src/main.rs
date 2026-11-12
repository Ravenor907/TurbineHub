mod agent;
mod backend;
mod pad;
mod run;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

/// The version from the repository's VERSION file, set by the build (0.1, 0.2 ...).
pub const VERSION: &str = match option_env!("TURBINEHUB_VERSION") {
    Some(v) => v,
    None => "dev",
};

unsafe extern "C" {
    // qtglue.cpp
    fn turbinehub_window_identity();
}

/// In Game Mode the app draws through X11 (Xwayland), the path gamescope shows every app through. Elsewhere, and
/// when QT_QPA_PLATFORM is already set (the screenshot run sets offscreen), nothing changes.
fn choose_platform() {
    let set = std::env::var_os("QT_QPA_PLATFORM").is_some_and(|v| !v.is_empty());
    let x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
    if !set && x11 && run::in_game_mode() {
        // SAFETY: main calls this before it starts any thread, so nothing else reads the environment yet.
        unsafe { std::env::set_var("QT_QPA_PLATFORM", "xcb") };
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // The list of pictures for the screenshot run, then done.
    if args.iter().any(|a| a == "--shots") {
        for (name, shot_args) in turbinehub_engine::ui::Ui::shots() {
            println!("{name} {shot_args}");
        }
        return;
    }
    // A question from the quick panel or a script: answered without the window.
    if let Some(i) = args.iter().position(|a| a == "--json") {
        let (text, code) = turbinehub_engine::cli::run(args.get(i + 1).map(String::as_str).unwrap_or(""), &backend::real_ctx());
        println!("{text}");
        std::process::exit(code);
    }
    // The background helper: the same program without a window, started by its systemd user unit.
    if args.iter().any(|a| a == "--agent") {
        std::process::exit(agent::main());
    }
    choose_platform();
    pad::clear_steam_filters();
    let mut app = QGuiApplication::new();
    if let Some(mut a) = app.as_mut() {
        a.as_mut().set_organization_name(&QString::from("Turbine Hub"));
        a.as_mut().set_application_name(&QString::from("Turbine Hub"));
    }
    // SAFETY: after QGuiApplication, on the main thread; takes and keeps nothing from this side.
    unsafe { turbinehub_window_identity() };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/TurbineHub/qml/Main.qml"));
    }
    let code = app.as_mut().map(|a| a.exec()).unwrap_or(1);
    std::process::exit(code);
}
