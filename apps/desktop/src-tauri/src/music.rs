//! The catalog of bundled background music.
//!
//! Tracks live in `assets/music/` next to a `tracks.json` file describing
//! each one, including its license. Adding a track means dropping the audio
//! file in that folder and adding an entry to `tracks.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const CATALOG_FILE: &str = "tracks.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicTrack {
    pub id: String,
    pub title: String,
    pub file: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    tracks: Vec<MusicTrack>,
}

/// Reads the catalog, skipping entries whose audio file is missing.
pub fn load_catalog(dir: &Path) -> AppResult<Vec<MusicTrack>> {
    let path = dir.join(CATALOG_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|e| AppError::Music(format!("cannot read {}: {e}", path.display())))?;
    let catalog: Catalog = serde_json::from_str(&text)
        .map_err(|e| AppError::Music(format!("invalid {}: {e}", path.display())))?;
    Ok(catalog
        .tracks
        .into_iter()
        .filter(|t| is_safe_file_name(&t.file) && dir.join(&t.file).is_file())
        .collect())
}

/// The audio file of a track, if it is in the catalog.
pub fn resolve(dir: &Path, id: &str) -> AppResult<PathBuf> {
    load_catalog(dir)?
        .into_iter()
        .find(|t| t.id == id)
        .map(|t| dir.join(t.file))
        .ok_or_else(|| AppError::Music(format!("unknown track \"{id}\"")))
}

/// Catalog entries must name a file inside the music folder.
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_dir(name: &str, json: &str, files: &[&str]) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("codetake-music-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CATALOG_FILE), json).unwrap();
        for file in files {
            fs::write(dir.join(file), b"audio").unwrap();
        }
        dir
    }

    const JSON: &str = r#"{ "tracks": [
        { "id": "a", "title": "A", "file": "a.m4a", "author": "CodeTake", "license": "CC0-1.0" },
        { "id": "b", "title": "B", "file": "b.m4a", "author": "CodeTake", "license": "CC0-1.0" },
        { "id": "evil", "title": "E", "file": "../secret", "author": "x", "license": "x" }
    ] }"#;

    #[test]
    fn lists_tracks_whose_files_exist() {
        let dir = catalog_dir("list", JSON, &["a.m4a"]);
        let tracks = load_catalog(&dir).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "a");
        assert_eq!(tracks[0].license, "CC0-1.0");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_track_files_and_rejects_unknown_ids() {
        let dir = catalog_dir("resolve", JSON, &["a.m4a", "b.m4a"]);
        assert_eq!(resolve(&dir, "b").unwrap(), dir.join("b.m4a"));
        assert!(resolve(&dir, "missing").is_err());
        assert!(resolve(&dir, "evil").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_a_broken_catalog() {
        let dir = catalog_dir("broken", "{ not json", &[]);
        assert!(matches!(load_catalog(&dir), Err(AppError::Music(_))));
        fs::remove_dir_all(dir).unwrap();
    }
}
