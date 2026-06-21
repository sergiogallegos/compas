//! Controller-profile storage: load/list/save `ControllerProfile` JSON files from the user's
//! controller directory (and, later, a bundled read-only dir). See `docs/CONTROLLER-ARCHITECTURE.md`.
//!
//! A profile is pure data (bindings + optional script) — adding a controller is dropping a file
//! here, no recompile. This module is plain blocking I/O, called only from Tauri commands.

use std::fs;
use std::path::{Path, PathBuf};

use compas_core::ControllerProfile;

/// The user controller directory (`<app-data>/controllers`), created if missing.
pub fn profiles_dir(base: &Path) -> std::io::Result<PathBuf> {
    let dir = base.join("controllers");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Load and parse every `*.json` profile in `dir`, skipping any that fail to parse.
pub fn list_profiles(dir: &Path) -> Vec<ControllerProfile> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(p) = load_profile(&path) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Load a single profile file.
pub fn load_profile(path: &Path) -> Result<ControllerProfile, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Save (or overwrite) a profile as `<dir>/<id>.json`. Used by the guided learn editor.
pub fn save_profile(dir: &Path, profile: &ControllerProfile) -> Result<PathBuf, String> {
    let safe: String = profile
        .id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let id = if safe.is_empty() { "profile".to_string() } else { safe };
    let path = dir.join(format!("{id}.json"));
    let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_list_round_trips() {
        let tmp = std::env::temp_dir().join(format!("compas-ctrl-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let mut p = ControllerProfile {
            id: "vendor model v1!".into(), // sanitized to vendor-model-v1-
            name: "Vendor Model".into(),
            ..Default::default()
        };
        p.bindings.push(compas_core::Binding {
            channel: 0,
            input: compas_core::InputKind::Cc { cc: 7 },
            control: "deck.0.gain".into(),
            soft_takeover: false,
        });

        let path = save_profile(&tmp, &p).unwrap();
        assert!(path.exists());
        let listed = list_profiles(&tmp);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Vendor Model");
        assert_eq!(listed[0].bindings.len(), 1);

        // A malformed file is skipped, not fatal.
        fs::write(tmp.join("broken.json"), "{ not json").unwrap();
        assert_eq!(list_profiles(&tmp).len(), 1);

        let _ = fs::remove_dir_all(&tmp);
    }
}
