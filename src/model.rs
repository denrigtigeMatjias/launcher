//! Data model: a library holds games, and each game holds launch profiles.

use serde::{Deserialize, Serialize};

/// One way of starting a game: a repo from the repo list, and the folder the
/// game is installed in. The repo's own files are fetched and kept by the
/// launcher, so a profile never points at them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub repo_id: String,
    pub game_dir: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            repo_id: String::new(),
            game_dir: String::new(),
        }
    }
}

impl Profile {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Game {
    pub name: String,
    pub default_profile: String,
    pub profiles: Vec<Profile>,
}

impl Game {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_profile: "Default".into(),
            profiles: vec![Profile::default()],
        }
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn default_index(&self) -> Option<usize> {
        if self.profiles.is_empty() {
            return None;
        }
        Some(self.profiles.iter().position(|p| p.name == self.default_profile).unwrap_or(0))
    }

    pub fn default(&self) -> Option<&Profile> {
        self.default_index().map(|i| &self.profiles[i])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub games: Vec<Game>,
}

impl Default for Library {
    fn default() -> Self {
        Self { version: 1, games: Vec::new() }
    }
}

impl Library {
    pub fn game(&self, name: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.name == name)
    }
}

/// `base`, or `base (2)`, `base (3)`... whichever is not taken.
pub fn unique_name<'a>(base: &str, taken: impl IntoIterator<Item = &'a str> + Clone) -> String {
    let is_taken = |n: &str| taken.clone().into_iter().any(|t| t == n);
    if !is_taken(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} ({i})")).find(|n| !is_taken(n)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names() {
        assert_eq!(unique_name("A", ["B"]), "A");
        assert_eq!(unique_name("A", ["A", "A (2)"]), "A (3)");
    }

    #[test]
    fn default_profile_falls_back_to_first() {
        let mut g = Game::new("G");
        g.profiles.push(Profile::named("Modded"));
        g.default_profile = "Modded".into();
        assert_eq!(g.default().unwrap().name, "Modded");
        g.default_profile = "gone".into();
        assert_eq!(g.default().unwrap().name, "Default");
    }

    /// Libraries written by older versions carry fields that are gone now.
    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let p: Profile =
            serde_json::from_str(r#"{"name":"x","kind":"program","arguments":"-w","future":1}"#).unwrap();
        assert_eq!(p.name, "x");
        assert!(p.repo_id.is_empty());
    }
}
