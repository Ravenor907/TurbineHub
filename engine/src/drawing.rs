//! How the window is drawn where there is no graphics card to draw with (WSL, a virtual machine without one, a
//! container): Mesa is told to draw in software from the start, instead of trying each graphics driver first and
//! printing a screen of errors before it ends up there anyway. Qt Quick keeps drawing through OpenGL, so the
//! interface looks the same, only slower. The app reads the facts and sets the one variable; the decision is here.

use std::path::Path;

/// The variable that tells Mesa to draw in software.
pub const SOFTWARE_ENV: &str = "LIBGL_ALWAYS_SOFTWARE";

/// What the decision needs to know about this run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    /// The player set how to draw already (LIBGL_ALWAYS_SOFTWARE or QT_QUICK_BACKEND): that is kept.
    pub chosen: bool,
    /// Nothing is drawn on a screen (QT_QPA_PLATFORM is offscreen or minimal: the build's pictures).
    pub offscreen: bool,
    /// The kernel's own name for itself (/proc/sys/kernel/osrelease).
    pub kernel: String,
    /// The names in /dev/dri, None when the folder can't be read.
    pub dri: Option<Vec<String>>,
}

/// Running under WSL (Windows' Linux): its kernel names itself "microsoft". WSL has no graphics card Mesa inside
/// a Flatpak can use.
pub fn kernel_is_wsl(release: &str) -> bool {
    let r = release.to_ascii_lowercase();
    r.contains("microsoft") || r.contains("wsl")
}

/// Is there a graphics card to draw with (a render node in /dev/dri)?
pub fn has_render_node(dri: Option<&[String]>) -> bool {
    dri.is_some_and(|names| names.iter().any(|n| n.starts_with("renderD")))
}

/// Why this run draws in software, when it should; None to leave drawing as it is.
pub fn software_reason(seen: &Seen) -> Option<&'static str> {
    if seen.chosen || seen.offscreen {
        None
    } else if kernel_is_wsl(&seen.kernel) {
        Some("WSL has no graphics card Mesa can use")
    } else if !has_render_node(seen.dri.as_deref()) {
        Some("no graphics card to draw with in /dev/dri")
    } else {
        None
    }
}

/// Reads the facts from this machine. `env`: the value of an environment variable ("" when unset).
pub fn look(root: &Path, env: &dyn Fn(&str) -> String) -> Seen {
    let set = |k: &str| !env(k).is_empty();
    let platform = env("QT_QPA_PLATFORM").to_ascii_lowercase();
    Seen {
        chosen: set(SOFTWARE_ENV) || set("QT_QUICK_BACKEND"),
        offscreen: platform.starts_with("offscreen") || platform.starts_with("minimal"),
        kernel: std::fs::read_to_string(root.join("proc/sys/kernel/osrelease")).unwrap_or_default(),
        dri: std::fs::read_dir(root.join("dev/dri")).ok().map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(kernel: &str, dri: Option<&[&str]>) -> Seen {
        Seen { kernel: kernel.into(), dri: dri.map(|d| d.iter().map(|x| x.to_string()).collect()), ..Seen::default() }
    }

    #[test]
    fn software_drawing_only_where_there_is_no_graphics_card() {
        // A handheld or PC with its card: nothing changes.
        assert_eq!(software_reason(&seen("6.11.11-valve27-1-neptune-611", Some(&["card0", "renderD128", "by-path"]))), None);
        // No render node, or no /dev/dri at all.
        assert_eq!(software_reason(&seen("6.8.0-generic", Some(&["card0"]))), Some("no graphics card to draw with in /dev/dri"));
        assert_eq!(software_reason(&seen("6.8.0-generic", None)), Some("no graphics card to draw with in /dev/dri"));
        // WSL, even though it shows a render node of its own.
        assert_eq!(software_reason(&seen("5.15.167.4-microsoft-standard-WSL2", Some(&["renderD128"]))), Some("WSL has no graphics card Mesa can use"));
        // The player's own choice and the build's pictures are left alone.
        assert_eq!(software_reason(&Seen { chosen: true, ..seen("x", None) }), None);
        assert_eq!(software_reason(&Seen { offscreen: true, ..seen("x", None) }), None);
    }

    #[test]
    fn the_facts_are_read_from_the_machine() {
        let root = std::env::temp_dir().join(format!("turbinehub-drawing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("dev/dri")).unwrap();
        std::fs::create_dir_all(root.join("proc/sys/kernel")).unwrap();
        std::fs::write(root.join("dev/dri/renderD128"), "").unwrap();
        std::fs::write(root.join("proc/sys/kernel/osrelease"), "6.11.11-valve27\n").unwrap();
        let none = |_: &str| String::new();
        let s = look(&root, &none);
        assert!(!s.chosen && !s.offscreen && has_render_node(s.dri.as_deref()) && software_reason(&s).is_none());
        let offscreen = |k: &str| if k == "QT_QPA_PLATFORM" { "offscreen".to_string() } else { String::new() };
        assert!(look(&root, &offscreen).offscreen);
        let chosen = |k: &str| if k == "QT_QUICK_BACKEND" { "software".to_string() } else { String::new() };
        assert!(look(&root, &chosen).chosen);
        std::fs::remove_file(root.join("dev/dri/renderD128")).unwrap();
        assert!(software_reason(&look(&root, &none)).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
