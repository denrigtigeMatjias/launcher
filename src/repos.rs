//! The fixed list of repos offered in the "Repo method" dropdown.
//!
//! The list lives in `repos.json` at the root of the source tree and is built
//! into the program. A `repos.json` in the data folder (next to library.json)
//! replaces it without rebuilding, which is handy while writing entries.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const BUILT_IN: &str = include_str!("../repos.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Method {
    /// Open a .bat file. `file` is relative to the repo folder unless it is
    /// absolute or starts with `{game_dir}`.
    Bat {
        file: String,
        #[serde(default)]
        args: String,
    },
    /// Copy files from the repo folder into the game folder, then run `run`
    /// (relative to the game folder).
    CopyAndRun {
        copy: Vec<String>,
        run: String,
        #[serde(default)]
        args: String,
    },
}

impl Method {
    pub fn describe(&self) -> String {
        match self {
            Method::Bat { file, .. } => format!("Opens {file} from the repo folder"),
            Method::CopyAndRun { copy, run, .. } => {
                format!("Copies {} into the game folder, then runs {run}", copy.join(", "))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    /// Stable id saved in profiles. Change `name` freely, but keep `id`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    pub method: Method,
    #[serde(default)]
    pub instructions: String,
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
        if let Method::CopyAndRun { copy, run, .. } = &r.method {
            if copy.is_empty() || run.trim().is_empty() {
                bail!("'{}' needs at least one file in 'copy' and a 'run' file", r.name);
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

    #[test]
    fn rejects_duplicates_and_typos() {
        let dup = r#"{"repos":[
            {"id":"a","name":"A","method":{"type":"bat","file":"x.bat"}},
            {"id":"a","name":"B","method":{"type":"bat","file":"y.bat"}}]}"#;
        assert!(parse(dup).is_err());
        let typo = r#"{"repos":[{"id":"a","name":"A","method":{"type":"bat","fiel":"x.bat"}}]}"#;
        assert!(parse(typo).is_err());
        let bad_method = r#"{"repos":[{"id":"a","name":"A","method":{"type":"exe"}}]}"#;
        assert!(parse(bad_method).is_err());
    }
}
