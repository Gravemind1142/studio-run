//! Finding the Roblox Studio executable.

use std::path::{Path, PathBuf};

/// Find Studio: an explicit path, else the install the updater keeps current.
pub fn studio_exe(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return resolve_user_path(&p).ok_or_else(|| format!("Studio not found at {}", p.display()));
    }
    discover().ok_or_else(|| {
        "couldn't find Roblox Studio; install it, or pass --studio / set STUDIO_EXE".to_string()
    })
}

/// A file path, or on macOS a `RobloxStudio.app` bundle.
fn resolve_user_path(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    #[cfg(target_os = "macos")]
    {
        let inner = path.join("Contents/MacOS/RobloxStudio");
        if inner.is_file() {
            return Some(inner);
        }
    }
    None
}

#[cfg(windows)]
fn discover() -> Option<PathBuf> {
    if let Some(p) = from_registry().filter(|p| p.is_file()) {
        return Some(p);
    }
    from_versions_dir()
}

/// The `roblox-studio:` protocol handler, kept current by Studio's updater,
/// else the newest install under `%LOCALAPPDATA%\Roblox\Versions`.
#[cfg(windows)]
fn from_registry() -> Option<PathBuf> {
    use std::ffi::c_void;

    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::PCWSTR;

    let key: Vec<u16> = r"Software\Classes\roblox-studio\shell\open\command"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut buf = vec![0u16; 2048];
    let mut len = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut len),
        )
        .ok()
        .ok()?;
    }
    let value = String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]);
    parse_command(&value)
}

/// `"C:\...\RobloxStudioBeta.exe" %1` -> `C:\...\RobloxStudioBeta.exe`
#[cfg(windows)]
fn parse_command(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let exe = match value.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => value.split_whitespace().next()?,
    };
    (!exe.is_empty()).then(|| PathBuf::from(exe))
}

#[cfg(windows)]
fn from_versions_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(r"Roblox\Versions");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("RobloxStudioBeta.exe"))
        .filter(|p| p.is_file())
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

#[cfg(target_os = "macos")]
fn discover() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from(
        "/Applications/RobloxStudio.app/Contents/MacOS/RobloxStudio",
    )];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home).join("Applications/RobloxStudio.app/Contents/MacOS/RobloxStudio"),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn parses_protocol_handler_command() {
        assert_eq!(
            parse_command(r#""C:\Roblox\Versions\v1\RobloxStudioBeta.exe" %1"#),
            Some(PathBuf::from(r"C:\Roblox\Versions\v1\RobloxStudioBeta.exe"))
        );
        assert_eq!(
            parse_command(r"C:\x\Studio.exe %1"),
            Some(PathBuf::from(r"C:\x\Studio.exe"))
        );
        assert_eq!(parse_command(""), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolves_an_app_bundle() {
        let dir = std::env::temp_dir().join(format!("studio-run-bundle-{}", std::process::id()));
        let exe = dir.join("Contents/MacOS/RobloxStudio");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        let found = resolve_user_path(&dir).expect("bundle");
        assert_eq!(found, exe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_a_missing_explicit_path() {
        let missing =
            std::env::temp_dir().join(format!("studio-run-missing-{}", std::process::id()));
        let err = studio_exe(Some(missing.clone())).unwrap_err();
        assert!(err.contains(&missing.display().to_string()));
    }
}
