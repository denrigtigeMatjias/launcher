//! Mod overlays: copy a mod folder over a game folder, and undo it later.
//!
//! Every change is written to a manifest next to the backups as it happens,
//! so an overlay can still be undone after the launcher was closed or crashed
//! while the game ran (see [`pending_restores`]).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Overlay {
    pub target: PathBuf,
    pub backup_dir: PathBuf,
    /// Files that did not exist before (relative to target).
    pub created: Vec<PathBuf>,
    /// Files whose originals were saved in `backup_dir/files`.
    pub replaced: Vec<PathBuf>,
}

impl Overlay {
    fn manifest(&self) -> PathBuf {
        self.backup_dir.join("manifest.json")
    }

    fn save(&self) -> Result<()> {
        fs::write(self.manifest(), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

fn walk_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

pub fn apply_overlay(source: &Path, target: &Path, backups_root: &Path) -> Result<Overlay> {
    if !source.is_dir() {
        bail!("Mod folder not found: {}", source.display());
    }
    if !target.is_dir() {
        bail!("Folder to copy mods into not found: {}", target.display());
    }
    if fs::canonicalize(source)? == fs::canonicalize(target)? {
        bail!("The mod folder and the game folder are the same");
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut backup = backups_root.join(stamp.to_string());
    let mut n = 1;
    while backup.exists() {
        n += 1;
        backup = backups_root.join(format!("{stamp}-{n}"));
    }
    fs::create_dir_all(backup.join("files"))?;

    let mut overlay = Overlay {
        target: target.to_path_buf(),
        backup_dir: backup.clone(),
        created: vec![],
        replaced: vec![],
    };
    overlay.save()?; // saved early: a partial overlay can still be undone

    let mut files = vec![];
    walk_files(source, &mut files)?;
    files.sort();

    let result = (|| -> Result<()> {
        for src in &files {
            let rel = src.strip_prefix(source)?.to_path_buf();
            let dest = target.join(&rel);
            if dest.exists() {
                let saved = backup.join("files").join(&rel);
                fs::create_dir_all(saved.parent().unwrap())?;
                fs::copy(&dest, &saved)?;
                overlay.replaced.push(rel);
            } else {
                fs::create_dir_all(dest.parent().unwrap())?;
                overlay.created.push(rel);
            }
            overlay.save()?;
            fs::copy(src, &dest).with_context(|| format!("copying {}", src.display()))?;
        }
        Ok(())
    })();

    if let Err(e) = result {
        let _ = restore_overlay(&overlay);
        return Err(e);
    }
    Ok(overlay)
}

pub fn restore_overlay(overlay: &Overlay) -> Result<()> {
    let target = &overlay.target;
    for rel in overlay.created.iter().rev() {
        let path = target.join(rel);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        // Remove folders the overlay created, if they are now empty.
        let mut parent = path.parent().map(Path::to_path_buf);
        while let Some(dir) = parent {
            if &dir == target || fs::remove_dir(&dir).is_err() {
                break;
            }
            parent = dir.parent().map(Path::to_path_buf);
        }
    }
    for rel in &overlay.replaced {
        let saved = overlay.backup_dir.join("files").join(rel);
        if saved.exists() {
            fs::copy(&saved, target.join(rel))?;
        }
    }
    let _ = fs::remove_dir_all(&overlay.backup_dir);
    Ok(())
}

/// Overlays that were applied but never undone.
pub fn pending_restores(backups_root: &Path) -> Vec<Overlay> {
    let Ok(entries) = fs::read_dir(backups_root) else { return vec![] };
    let mut out: Vec<Overlay> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| fs::read(e.path().join("manifest.json")).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect();
    out.sort_by(|a, b| a.backup_dir.cmp(&b.backup_dir));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn make(root: &Path, files: &[(&str, &str)]) {
        for (rel, text) in files {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
    }

    fn read(root: &Path) -> BTreeMap<String, String> {
        let mut files = vec![];
        walk_files(root, &mut files).unwrap();
        files
            .into_iter()
            .map(|p| {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                (rel, fs::read_to_string(&p).unwrap())
            })
            .collect()
    }

    #[test]
    fn apply_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let (game, m, backups) = (dir.path().join("game"), dir.path().join("mod"), dir.path().join("b"));
        make(&game, &[("data/a.txt", "orig a"), ("b.txt", "orig b")]);
        make(&m, &[("data/a.txt", "mod a"), ("new/c.txt", "mod c")]);
        let before = read(&game);

        let overlay = apply_overlay(&m, &game, &backups).unwrap();
        let after = read(&game);
        assert_eq!(after["data/a.txt"], "mod a");
        assert_eq!(after["new/c.txt"], "mod c");
        assert_eq!(after["b.txt"], "orig b");
        assert_eq!(pending_restores(&backups).len(), 1);

        restore_overlay(&overlay).unwrap();
        assert_eq!(read(&game), before);
        assert!(!game.join("new").exists());
        assert!(pending_restores(&backups).is_empty());
    }

    #[test]
    fn restore_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (game, m, backups) = (dir.path().join("game"), dir.path().join("mod"), dir.path().join("b"));
        make(&game, &[("a.txt", "orig")]);
        make(&m, &[("a.txt", "mod")]);
        apply_overlay(&m, &game, &backups).unwrap();
        let pending = pending_restores(&backups); // as if loaded after a crash
        restore_overlay(&pending[0]).unwrap();
        assert_eq!(fs::read_to_string(game.join("a.txt")).unwrap(), "orig");
    }

    #[test]
    fn rejects_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(apply_overlay(dir.path(), dir.path(), &dir.path().join("b")).is_err());
    }
}
