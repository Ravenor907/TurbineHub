mod agent;
mod backend;
mod log;
mod pad;
mod run;
mod single;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

/// The version from the repository's VERSION file, set by the build (0.1, 0.2 ...).
pub const VERSION: &str = match option_env!("TURBINEHUB_VERSION") {
    Some(v) => v,
    None => "dev",
};

unsafe extern "C" {
    // qtglue.cpp
    fn turbinehub_window_identity();
    fn turbinehub_install_qt_handler(sink: extern "C" fn(i32, *const std::os::raw::c_char, *const std::os::raw::c_char));
}

/// The player asked for a restart (Desktop Mode, after an update or a newly allowed folder): once the window has
/// closed, Turbine Hub starts itself again. Set and cleared by the backend with the dialog that asks.
pub static RESTART: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Software drawing where there is no graphics card to draw with (the engine's `drawing` decides). Answers why,
/// when it was chosen.
fn choose_drawing() -> Option<&'static str> {
    use turbinehub_engine::drawing;
    let why = drawing::software_reason(&drawing::look(std::path::Path::new("/"), &|k| std::env::var(k).unwrap_or_default()))?;
    // SAFETY: main calls this before it starts any thread, so nothing else reads the environment yet.
    unsafe { std::env::set_var(drawing::SOFTWARE_ENV, "1") };
    Some(why)
}

/// The logs, Qt's messages into them, and Turbine Hub's own library art brought up to date: only for a window on
/// the real machine (pictures and made-up machines write nothing outside their own folder).
fn start_logs(software: Option<&'static str>) {
    let real = backend::option("screenshot").is_none() && matches!(backend::option("machine").as_deref(), None | Some("real"));
    if !real {
        return;
    }
    log::start(turbinehub_engine::applog::dir(&backend::data_dir()));
    log::line("app", &format!("Turbine Hub {VERSION} starting, {}{}", if run::in_game_mode() { "Game Mode" } else { "Desktop Mode" }, if run::in_flatpak() { "" } else { ", not in its Flatpak" }));
    if let Some(p) = std::env::var_os("QT_QPA_PLATFORM").filter(|v| !v.is_empty()) {
        log::line("app", &format!("Drawing through {}", p.to_string_lossy()));
    }
    if let Some(why) = software {
        log::line("app", &format!("Software drawing ({why})"));
    }
    // SAFETY: the sink is a plain function that lives for the whole run.
    unsafe { turbinehub_install_qt_handler(log::qt_message) };
    // Turbine Hub's own Steam library art, at every start and so after every update; never over the player's own.
    std::thread::spawn(|| {
        let ctx = backend::real_ctx();
        if let Some(words) = turbinehub_engine::ui::selftile::art_at_start(&ctx.home, &ctx.config_home.join("turbinehub"), &run::share_dir()) {
            log::line("art", &words);
        }
    });
}

/// Starts Turbine Hub again on the host, for the restart the player asked for. The new one outlives this one.
fn start_again() {
    log::line("app", "Restarting");
    let line = if run::in_flatpak() { turbinehub_engine::selfupdate::start_again(turbinehub_engine::ui::selftile::FLATPAK_ID) } else { std::env::args().take(1).collect() };
    if let Some((program, args)) = line.split_first()
        && let Err(e) = std::process::Command::new(program).args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()
    {
        log::line("app", &format!("Restart failed: {e}"));
    }
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
    // One Turbine Hub at a time: a second launch brings the open one to the front and ends, before it touches the
    // logs (it must not read as a run that ended without closing). Pictures and trial runs on a made-up machine
    // start regardless.
    let real = backend::option("screenshot").is_none() && matches!(backend::option("machine").as_deref(), None | Some("real"));
    if real && single::other_is_open() {
        return;
    }
    choose_platform();
    let software = choose_drawing();
    pad::clear_steam_filters();
    start_logs(software);
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
    log::clean_exit();
    backend::remove_sample_dir();
    // Before a restart: the copy started next must find nobody answering and become the Turbine Hub that listens.
    single::stop();
    if RESTART.load(std::sync::atomic::Ordering::Relaxed) {
        start_again();
    }
    std::process::exit(code);
}
