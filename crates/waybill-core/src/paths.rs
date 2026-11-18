//! Where Waybill keeps things. Every location can be moved with an
//! environment variable, which tests and development builds use to stay away
//! from an installed copy.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands a leading `~`.
pub fn expand(s: &str) -> PathBuf {
    let s = s.trim();
    if s == "~" {
        home()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(s)
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn xdg_config() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

pub fn config_dir() -> PathBuf {
    env_dir("WAYBILL_CONFIG_DIR").unwrap_or_else(|| xdg_config().join("waybill"))
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// The user's own stylesheet, layered over the theme.
pub fn style_file() -> PathBuf {
    config_dir().join("style.css")
}

/// Per-user directory for claims and the drawer's socket. It is emptied by a
/// reboot, which is the point: a claim does not outlive the session's machine.
pub fn runtime_dir() -> PathBuf {
    if let Some(d) = env_dir("WAYBILL_RUNTIME_DIR") {
        return d;
    }
    if let Some(d) = env_dir("XDG_RUNTIME_DIR") {
        return d.join("waybill");
    }
    let base = env_dir("TMPDIR").unwrap_or_else(|| PathBuf::from("/tmp"));
    #[cfg(unix)]
    // SAFETY: getuid has no failure mode.
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = 0;
    base.join(format!("waybill-{uid}"))
}

/// Waybar's config directory.
pub fn waybar_dir() -> PathBuf {
    xdg_config().join("waybar")
}
