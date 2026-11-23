//! `studio-run.toml`: cloud settings, so `--cloud` needs no IDs on the command line.
//!
//! ```toml
//! [cloud]
//! universe-id = 111
//! place-id = 444
//! api-key-env = "TEST_KEY"
//! upload = true   # this place is disposable; --place may upload to it
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const FILE_NAME: &str = "studio-run.toml";
pub const DEFAULT_KEY_ENV: &str = "ROBLOX_API_KEY";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    cloud: Section,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Section {
    pub universe_id: Option<u64>,
    pub place_id: Option<u64>,
    pub api_key_env: Option<String>,
    pub upload: Option<bool>,
}

/// Settings from the command line, which win over the file.
#[derive(Debug, Default, Clone)]
pub struct Flags {
    pub universe_id: Option<u64>,
    pub place_id: Option<u64>,
    pub api_key_env: Option<String>,
    pub allow_upload: bool,
}

#[derive(Debug, PartialEq)]
pub struct Settings {
    pub universe_id: u64,
    pub place_id: u64,
    pub api_key_env: String,
    pub upload_allowed: bool,
    /// The config file used, if any.
    pub file: Option<PathBuf>,
}

/// Loads the config file (`explicit`, or `studio-run.toml` in the current
/// directory or a parent) and combines it with the flags.
pub fn load(explicit: Option<&Path>, flags: &Flags) -> Result<Settings, String> {
    let path = match explicit {
        Some(p) if !p.is_file() => return Err(format!("config file not found: {}", p.display())),
        Some(p) => Some(p.to_path_buf()),
        None => std::env::current_dir().ok().and_then(|d| find_upward(&d)),
    };
    let section = match &path {
        Some(p) => parse(
            &std::fs::read_to_string(p)
                .map_err(|e| format!("couldn't read {}: {e}", p.display()))?,
        )
        .map_err(|e| format!("{}: {e}", p.display()))?,
        None => Section::default(),
    };
    resolve(flags, &section, path)
}

fn find_upward(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|d| d.join(FILE_NAME))
        .find(|p| p.is_file())
}

fn parse(text: &str) -> Result<Section, String> {
    toml::from_str::<File>(text)
        .map(|f| f.cloud)
        .map_err(|e| e.message().to_string())
}

fn resolve(flags: &Flags, file: &Section, path: Option<PathBuf>) -> Result<Settings, String> {
    let where_ = || match &path {
        Some(p) => format!("pass --{{}} or set it in {}", p.display()),
        None => format!("pass --{{}} or set it in {FILE_NAME}"),
    };
    let universe_id = flags
        .universe_id
        .or(file.universe_id)
        .ok_or_else(|| format!("no universe: {}", where_().replace("{}", "universe-id")))?;
    let place_id = flags
        .place_id
        .or(file.place_id)
        .ok_or_else(|| format!("no place: {}", where_().replace("{}", "place-id")))?;
    Ok(Settings {
        universe_id,
        place_id,
        api_key_env: flags
            .api_key_env
            .clone()
            .or_else(|| file.api_key_env.clone())
            .unwrap_or_else(|| DEFAULT_KEY_ENV.to_string()),
        upload_allowed: flags.allow_upload || file.upload == Some(true),
        file: path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> Section {
        parse(text).unwrap()
    }

    #[test]
    fn reads_the_cloud_section() {
        let s = file(
            "[cloud]\nuniverse-id = 111\nplace-id = 444\napi-key-env = \"K\"\nupload = true\n",
        );
        assert_eq!(s.universe_id, Some(111));
        assert_eq!(s.place_id, Some(444));
        assert_eq!(s.api_key_env.as_deref(), Some("K"));
        assert_eq!(s.upload, Some(true));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(parse("[cloud]\nuniverse = 1\n").is_err());
        assert!(parse("[clod]\n").is_err());
    }

    #[test]
    fn flags_win_over_the_file() {
        let f = file("[cloud]\nuniverse-id = 1\nplace-id = 2\napi-key-env = \"FILE\"\n");
        let flags = Flags {
            universe_id: Some(10),
            api_key_env: Some("FLAG".into()),
            ..Default::default()
        };
        let s = resolve(&flags, &f, None).unwrap();
        assert_eq!((s.universe_id, s.place_id), (10, 2));
        assert_eq!(s.api_key_env, "FLAG");
    }

    #[test]
    fn defaults() {
        let flags = Flags {
            universe_id: Some(1),
            place_id: Some(2),
            ..Default::default()
        };
        let s = resolve(&flags, &Section::default(), None).unwrap();
        assert_eq!(s.api_key_env, DEFAULT_KEY_ENV);
        assert!(!s.upload_allowed);
    }

    #[test]
    fn upload_needs_the_flag_or_the_file() {
        let base = Flags {
            universe_id: Some(1),
            place_id: Some(2),
            ..Default::default()
        };
        let allowed =
            |flags: &Flags, f: &str| resolve(flags, &file(f), None).unwrap().upload_allowed;
        assert!(!allowed(&base, "[cloud]\nupload = false\n"));
        assert!(allowed(&base, "[cloud]\nupload = true\n"));
        let flag = Flags {
            allow_upload: true,
            ..base.clone()
        };
        assert!(allowed(&flag, ""));
    }

    #[test]
    fn missing_ids_say_where_to_set_them() {
        let e = resolve(&Flags::default(), &Section::default(), None).unwrap_err();
        assert!(e.contains("--universe-id") && e.contains(FILE_NAME), "{e}");
    }

    #[test]
    fn finds_the_file_in_a_parent_directory() {
        let root = std::env::temp_dir().join(format!("studio-run-config-{}", std::process::id()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join(FILE_NAME), "[cloud]\n").unwrap();
        assert_eq!(find_upward(&nested), Some(root.join(FILE_NAME)));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
