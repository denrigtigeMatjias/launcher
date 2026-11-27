//! Where the library lives on disk, and loading/saving it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::Library;

/// Folder for the library, the repo list, your scripts, and the repos the
/// launcher has fetched.
///
/// Windows: `%APPDATA%\GameLauncher`. Elsewhere: `~/.config/gamelauncher`.
/// `GAMELAUNCHER_HOME` overrides both (handy for a portable setup).
pub fn data_dir() -> PathBuf {
    let dir = if let Some(p) = std::env::var_os("GAMELAUNCHER_HOME") {
        PathBuf::from(p)
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join("GameLauncher")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
            .join("gamelauncher")
    };
    let _ = fs::create_dir_all(&dir);
    dir
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn library_path() -> PathBuf {
    data_dir().join("library.json")
}

pub fn load_from(path: &Path) -> Result<Library> {
    if !path.exists() {
        return Ok(Library::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Writes to a temp file and renames it, so a crash never leaves half a file.
pub fn save_to(library: &Library, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string_pretty(library)?.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub fn load() -> Result<Library> {
    load_from(&library_path())
}

pub fn save(library: &Library) -> Result<()> {
    save_to(library, &library_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Game, Profile};

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib.json");
        let mut game = Game::new("G");
        game.profiles.push(Profile {
            repo_id: "example-bat".into(),
            game_dir: r"C:\Games\G".into(),
            ..Profile::named("B")
        });
        game.default_profile = "B".into();
        let lib = Library { games: vec![game], ..Default::default() };
        save_to(&lib, &path).unwrap();
        assert_eq!(load_from(&path).unwrap(), lib);
        assert_eq!(load_from(&dir.path().join("missing.json")).unwrap(), Library::default());
    }
}
