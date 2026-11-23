//! Cloud settings, from the command line.

pub const DEFAULT_KEY_ENV: &str = "ROBLOX_API_KEY";

/// Settings from the command line.
#[derive(Debug, Default, Clone)]
pub struct Flags {
    pub universe_id: Option<u64>,
    pub place_id: Option<u64>,
    pub api_key_env: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Settings {
    pub universe_id: u64,
    pub place_id: u64,
    pub api_key_env: String,
}

/// Checks the flags and fills in defaults.
pub fn resolve(flags: &Flags) -> Result<Settings, String> {
    let universe_id = flags.universe_id.ok_or("no universe: pass --universe-id")?;
    let place_id = flags.place_id.ok_or("no place: pass --place-id")?;
    Ok(Settings {
        universe_id,
        place_id,
        api_key_env: flags
            .api_key_env
            .clone()
            .unwrap_or_else(|| DEFAULT_KEY_ENV.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_flags() {
        let flags = Flags {
            universe_id: Some(10),
            place_id: Some(2),
            api_key_env: Some("FLAG".into()),
        };
        let s = resolve(&flags).unwrap();
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
        assert_eq!(resolve(&flags).unwrap().api_key_env, DEFAULT_KEY_ENV);
    }

    #[test]
    fn missing_ids_name_the_flag() {
        let e = resolve(&Flags::default()).unwrap_err();
        assert!(e.contains("--universe-id"), "{e}");
        let e = resolve(&Flags {
            universe_id: Some(1),
            ..Default::default()
        })
        .unwrap_err();
        assert!(e.contains("--place-id"), "{e}");
    }
}
