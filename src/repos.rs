//! The fixed list of repos a profile can pick from.
//!
//! The list lives in `repos.json` at the root of the source tree and is built
//! into the program. A `repos.json` in the data folder (next to library.json)
//! replaces it without rebuilding, which is handy while writing entries.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const BUILT_IN: &str = include_str!("../repos.json");

/// How a repo starts a game. Set here, in the repo list, and never in the
/// window: both methods fetch what they need on their own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Method {
    /// Clone the repo, then run `file`: a .bat from the scripts folder (see
    /// [`script_dirs`]), or an absolute path.
    Bat {
        file: String,
        #[serde(default)]
        args: String,
    },
    /// Download `assets` from the repo's newest GitHub release, copy them
    /// into the game folder, then run `run` from there.
    CopyAndRun {
        assets: Vec<String>,
        run: String,
        #[serde(default)]
        args: String,
    },
}

impl Method {
    pub fn describe(&self) -> String {
        match self {
            Method::Bat { file, .. } => format!("Clones the repo, then runs {file}"),
            Method::CopyAndRun { assets, run, .. } => {
                format!("Downloads {} from the newest release, then runs {run}", assets.join(", "))
            }
        }
    }
}

/// Where the .bat files for the `bat` method are looked for, in order: your
/// own in the settings folder, then the ones shipped next to the program,
/// then the source tree while developing.
pub fn script_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![crate::storage::data_dir().join("scripts")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.join("scripts"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts"));
    dirs
}

/// The first scripts folder that has `file`.
pub fn script(file: &str) -> Option<PathBuf> {
    script_dirs().into_iter().map(|d| d.join(file)).find(|p| p.is_file())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    /// Stable id saved in profiles. Change `name` freely, but keep `id`.
    pub id: String,
    pub name: String,
    /// One line about what the repo is, shown beside its name in the dropdown.
    /// Say what it does, not how it starts: picking a repo is not picking a
    /// method. How it starts is `method`, set here and never in the window.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub url: String,
    pub method: Method,
    #[serde(default)]
    pub instructions: String,
}

impl Repo {
    /// The line beside the name in the repo dropdown.
    pub fn summary(&self) -> String {
        if self.description.trim().is_empty() {
            self.method.describe()
        } else {
            self.description.clone()
        }
    }
}

#[derive(Deserialize)]
struct RepoFile {
    #[serde(default, rename = "_help")]
    _help: Option<String>,
    repos: Vec<Repo>,
}

pub fn parse(text: &str) -> Result<Vec<Repo>> {
    let file: RepoFile = serde_json::from_str(text)?;
    let mut seen = std::collections::HashSet::new();
    for r in &file.repos {
        if r.id.trim().is_empty() {
            bail!("'{}' has an empty id", r.name);
        }
        if !seen.insert(r.id.as_str()) {
            bail!("Two repos use the id '{}'", r.id);
        }
        // Both methods download for themselves, so every repo needs a URL.
        if r.url.trim().is_empty() {
            bail!("'{}' needs a 'url' to download from", r.name);
        }
        if let Method::CopyAndRun { assets, run, .. } = &r.method {
            if assets.is_empty() || run.trim().is_empty() {
                bail!("'{}' needs at least one name in 'assets' and a 'run' file", r.name);
            }
            if crate::fetch::github_slug(&r.url).is_none() {
                bail!("'{}' downloads a release, so its 'url' must be a github.com repo", r.name);
            }
            // Asset names are file names in a release, not paths.
            for a in assets.iter().chain(std::iter::once(run)) {
                if a.contains('/') || a.contains('\\') || a.trim() != a || a.contains("..") {
                    bail!("'{}' has an odd release file name: '{a}'", r.name);
                }
            }
        }
    }
    Ok(file.repos)
}

pub fn override_path() -> PathBuf {
    crate::storage::data_dir().join("repos.json")
}

/// The repo list, plus a message if the override file could not be used.
pub fn load() -> (Vec<Repo>, Option<String>) {
    let built_in = parse(BUILT_IN).expect("built-in repos.json is valid (checked by tests)");
    let path = override_path();
    if !path.exists() {
        return (built_in, None);
    }
    match std::fs::read_to_string(&path)
        .context("reading")
        .and_then(|t| parse(&t))
    {
        Ok(list) => (list, None),
        Err(e) => (built_in, Some(format!("{} has a mistake, using the built-in list: {e:#}", path.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_list_is_valid() {
        assert!(!parse(BUILT_IN).unwrap().is_empty());
    }

    const URL: &str = r#""url":"https://github.com/owner/tool","#;

    #[test]
    fn rejects_duplicates_and_typos() {
        let dup = format!(
            r#"{{"repos":[
            {{"id":"a","name":"A",{URL}"method":{{"type":"bat","file":"x.bat"}}}},
            {{"id":"a","name":"B",{URL}"method":{{"type":"bat","file":"y.bat"}}}}]}}"#
        );
        assert!(parse(&dup).is_err());
        let typo = format!(r#"{{"repos":[{{"id":"a","name":"A",{URL}"method":{{"type":"bat","fiel":"x.bat"}}}}]}}"#);
        assert!(parse(&typo).is_err());
        let bad_method = format!(r#"{{"repos":[{{"id":"a","name":"A",{URL}"method":{{"type":"exe"}}}}]}}"#);
        assert!(parse(&bad_method).is_err());
    }

    #[test]
    fn repos_must_say_where_to_download_from() {
        let no_url = r#"{"repos":[{"id":"a","name":"A","method":{"type":"bat","file":"x.bat"}}]}"#;
        assert!(parse(no_url).is_err());
        let not_github = r#"{"repos":[{"id":"a","name":"A","url":"https://gitlab.com/o/t",
            "method":{"type":"copy_and_run","assets":["x.exe"],"run":"x.exe"}}]}"#;
        assert!(parse(not_github).is_err());
        // a clone can come from anywhere, only releases need github.com
        let gitlab_bat = r#"{"repos":[{"id":"a","name":"A","url":"https://gitlab.com/o/t",
            "method":{"type":"bat","file":"x.bat"}}]}"#;
        assert!(parse(gitlab_bat).is_ok());
        let path_asset = format!(
            r#"{{"repos":[{{"id":"a","name":"A",{URL}"method":{{"type":"copy_and_run","assets":["bin/x.exe"],"run":"x.exe"}}}}]}}"#
        );
        assert!(parse(&path_asset).is_err());
    }
}
