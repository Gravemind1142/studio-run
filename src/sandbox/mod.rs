//! Launching Studio without it taking over the screen.
//!
//! On Windows, Studio runs on a private desktop inside a job object. On
//! macOS, it is started with `--background` in its own process group. Either
//! way, dropping [`Studio`] kills the process tree.

#[cfg(any(windows, target_os = "macos"))]
pub use platform::Studio;

/// A top-level window belonging to the Studio process tree.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub class: String,
    pub title: String,
}

#[cfg(windows)]
mod platform {
    pub use super::windows::Studio;
}

#[cfg(target_os = "macos")]
mod platform {
    pub use super::macos::Studio;
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
mod macos;
