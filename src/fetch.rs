//! Getting each repo's files: cloning it, or downloading its newest release.
//!
//! Everything lands in `<settings folder>/repos/<repo id>`, which the
//! launcher owns. Nobody downloads anything by hand or points the launcher at
//! a folder: the first Play fetches what the repo needs, and the Update
//! button fetches it again.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::repos::{Method, Repo};
use crate::storage;

/// Sent to GitHub with every request. Deliberately says nothing about who is
/// running the launcher.
const AGENT: &str = concat!("gamelauncher/", env!("CARGO_PKG_VERSION"));

const MAX_JSON: u64 = 8 * 1024 * 1024;
const MAX_ASSET: u64 = 512 * 1024 * 1024;

/// Where a repo's files live once fetched.
///
/// Keyed by URL, not by repo id: a repo with several launch scripts gets an
/// entry per script, and they share one clone — and, for anything built from
/// source, one `target` folder.
pub fn repo_dir(url: &str) -> PathBuf {
    storage::data_dir().join("repos").join(folder_name(url))
}

/// A readable folder name for a URL, with a hash so that two repos of the
/// same name on different hosts stay apart.
fn folder_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or("");
    let slug: String = last
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let hash = trimmed.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let slug = if slug.is_empty() { "repo" } else { &slug };
    format!("{slug}-{hash:08x}")
}

/// What was fetched into a repo folder, and when.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stamp {
    /// A release tag, or the commit that was cloned.
    pub version: String,
    /// Seconds since the epoch.
    pub at: u64,
}

impl Stamp {
    fn path(dir: &Path) -> PathBuf {
        dir.join(".gamelauncher-fetch.json")
    }

    fn write(dir: &Path, version: String) -> Result<()> {
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let stamp = Self { version, at };
        std::fs::write(Self::path(dir), serde_json::to_vec_pretty(&stamp)?)?;
        Ok(())
    }

    /// "today", "yesterday", or "N days ago".
    pub fn when(&self) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        match now.saturating_sub(self.at) / 86_400 {
            0 => "today".into(),
            1 => "yesterday".into(),
            days => format!("{days} days ago"),
        }
    }
}

/// What was fetched for `repo`, if anything.
pub fn stamp(repo: &Repo) -> Option<Stamp> {
    stamp_in(&repo_dir(&repo.url))
}

fn stamp_in(dir: &Path) -> Option<Stamp> {
    serde_json::from_slice(&std::fs::read(Stamp::path(dir)).ok()?).ok()
}

/// True when everything the repo's method needs is already on disk.
pub fn is_ready(repo: &Repo) -> bool {
    ready_in(&repo_dir(&repo.url), &repo.method)
}

/// The stamp is written last, so a clone or a download that was cut short
/// (the launcher closed, the machine went off) is not mistaken for a
/// finished one: the next Play fetches it again.
fn ready_in(dir: &Path, method: &Method) -> bool {
    if stamp_in(dir).is_none() {
        return false;
    }
    match method {
        Method::Bat { .. } => dir.join(".git").is_dir(),
        Method::CopyAndRun { assets, .. } => {
            !assets.is_empty() && assets.iter().all(|a| dir.join(a).is_file())
        }
    }
}

/// Fetch what `repo` needs, unless it is already there and `force` is false.
///
/// `log` is called with a line per step, for the status line or the terminal.
pub fn ensure(repo: &Repo, force: bool, log: &dyn Fn(String)) -> Result<PathBuf> {
    let dir = repo_dir(&repo.url);
    if !force && is_ready(repo) {
        return Ok(dir);
    }
    if repo.url.trim().is_empty() {
        bail!("{} has no 'url' in repos.json, so there is nothing to download", repo.name);
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match &repo.method {
        Method::Bat { .. } => clone_or_update(repo, &dir, log)?,
        Method::CopyAndRun { assets, .. } => download_release(repo, assets, &dir, log)?,
    }
    Ok(dir)
}

// ---------- git ----------

fn git(args: &[&std::ffi::OsStr]) -> Result<String> {
    let out = Command::new("git").args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("Git is not installed, or not on PATH. Install it from https://git-scm.com.")
        } else {
            anyhow::anyhow!("Could not run git: {e}")
        }
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("git failed: {}", err.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn clone_or_update(repo: &Repo, dir: &Path, log: &dyn Fn(String)) -> Result<()> {
    let url = repo.url.trim();
    let dir_os = dir.as_os_str();
    if dir.join(".git").is_dir() {
        log(format!("Updating {}…", repo.name));
        git(&["-C".as_ref(), dir_os, "fetch".as_ref(), "--depth".as_ref(), "1".as_ref(), "origin".as_ref(), "HEAD".as_ref()])?;
        git(&["-C".as_ref(), dir_os, "reset".as_ref(), "--hard".as_ref(), "FETCH_HEAD".as_ref()])?;
    } else {
        log(format!("Cloning {}…", repo.name));
        if dir.exists() {
            std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))?;
        }
        git(&["clone".as_ref(), "--depth".as_ref(), "1".as_ref(), url.as_ref(), dir_os])?;
    }
    let commit = git(&["-C".as_ref(), dir_os, "rev-parse".as_ref(), "--short".as_ref(), "HEAD".as_ref()])
        .unwrap_or_else(|_| "unknown".into());
    Stamp::write(dir, commit)
}

// ---------- GitHub releases ----------

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// `owner/name` from a GitHub URL, for the releases API.
pub fn github_slug(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.strip_prefix("github.com/")?;
    let mut parts = rest.split('/');
    let owner = parts.next()?.trim();
    let name = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some((owner.to_string(), name.to_string()))
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>> {
    ureq::get(url)
        .header("User-Agent", AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("asking for {url}"))?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))
}

fn download_release(repo: &Repo, assets: &[String], dir: &Path, log: &dyn Fn(String)) -> Result<()> {
    let (owner, name) = github_slug(&repo.url)
        .ok_or_else(|| anyhow::anyhow!("{} needs a github.com 'url' to find its releases", repo.name))?;

    log(format!("Looking up the newest release of {owner}/{name}…"));
    let api = format!("https://api.github.com/repos/{owner}/{name}/releases/latest");
    let body = get(&api, MAX_JSON)?;
    let release: Release = serde_json::from_slice(&body)
        .with_context(|| format!("{owner}/{name} has no readable latest release"))?;

    std::fs::create_dir_all(dir)?;
    for want in assets {
        let found = release.assets.iter().find(|a| a.name == *want).ok_or_else(|| {
            let have: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            let have = if have.is_empty() { "nothing".into() } else { have.join(", ") };
            anyhow::anyhow!(
                "Release {} of {owner}/{name} has no file called '{want}'. It has: {have}",
                release.tag_name
            )
        })?;
        log(format!("Downloading {want} ({})…", release.tag_name));
        let bytes = get(&found.browser_download_url, MAX_ASSET)?;
        // Write beside the target first, so a failed download leaves no
        // half-written file that later looks like a finished one.
        let part = dir.join(format!("{want}.part"));
        std::fs::write(&part, &bytes).with_context(|| format!("writing {}", part.display()))?;
        std::fs::rename(&part, dir.join(want))?;
    }
    Stamp::write(dir, release.tag_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_github_slugs() {
        assert_eq!(
            github_slug("https://github.com/owner/tool"),
            Some(("owner".into(), "tool".into()))
        );
        assert_eq!(
            github_slug("https://github.com/owner/tool.git"),
            Some(("owner".into(), "tool".into()))
        );
        assert_eq!(
            github_slug("https://github.com/owner/tool/releases"),
            Some(("owner".into(), "tool".into()))
        );
        assert_eq!(github_slug("https://gitlab.com/owner/tool"), None);
        assert_eq!(github_slug("https://github.com/owner"), None);
    }

    #[test]
    fn entries_sharing_a_url_share_a_folder() {
        let a = repo_dir("https://git.example/owner/tool.git");
        assert_eq!(a, repo_dir("https://git.example/owner/tool"));
        // same name, different host: not the same folder
        assert_ne!(a, repo_dir("https://other.example/owner/tool"));
        assert!(a.to_string_lossy().contains("tool-"));
    }

    #[test]
    fn a_half_finished_fetch_is_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let method = Method::CopyAndRun {
            assets: vec!["loader.exe".into()],
            run: "loader.exe".into(),
            args: String::new(),
        };
        assert!(!ready_in(dir, &method));
        std::fs::write(dir.join("loader.exe"), "x").unwrap();
        // the file is there, but the fetch never finished
        assert!(!ready_in(dir, &method));
        Stamp::write(dir, "v1".into()).unwrap();
        assert!(ready_in(dir, &method));
    }

    #[test]
    fn days_ago_reads_plainly() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(Stamp { version: "v1".into(), at: now }.when(), "today");
        assert_eq!(Stamp { version: "v1".into(), at: now - 86_400 }.when(), "yesterday");
        assert_eq!(Stamp { version: "v1".into(), at: now - 3 * 86_400 }.when(), "3 days ago");
    }
}
