//! Turn a profile into a running process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

use crate::model::{EnvVar, LaunchKind, Profile};
use crate::repos::{Method, Repo};
use crate::{mods, storage};

pub fn is_url(executable: &str) -> bool {
    let s = executable.trim();
    let Some(idx) = s.find("://") else { return false };
    let scheme = &s[..idx];
    // Exclude things like "C://", which is a (strange) Windows path.
    scheme.len() > 1
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
}

/// Split an argument string the way a Windows user would type it: spaces
/// separate, double quotes group, backslashes are kept as they are.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = vec![];
    let mut current = String::new();
    let (mut in_quotes, mut has_token) = (false, false);
    for ch in text.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

/// Expand `%VAR%`, `$VAR` and `${VAR}` from the environment. Unknown
/// variables are left as written.
pub fn expand_env(s: &str, env: &HashMap<String, String>) -> String {
    let lookup = |name: &str| {
        env.get(name).cloned().or_else(|| {
            // Windows variable names are case-insensitive.
            env.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
        })
    };
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if !name.is_empty() && !name.contains(' ')
                    && let Some(v) = lookup(&name) {
                        out.push_str(&v);
                        i += end + 2;
                        continue;
                    }
            }
        } else if c == '$' {
            let (name, len) = if chars.get(i + 1) == Some(&'{') {
                match chars[i + 2..].iter().position(|&c| c == '}') {
                    Some(end) => (chars[i + 2..i + 2 + end].iter().collect::<String>(), end + 3),
                    None => (String::new(), 0),
                }
            } else {
                let name: String = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                    .collect();
                let len = name.chars().count() + 1;
                (name, len)
            };
            if !name.is_empty()
                && let Some(v) = lookup(&name) {
                    out.push_str(&v);
                    i += len;
                    continue;
                }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[derive(Debug, PartialEq)]
pub enum Plan {
    Url(String),
    Process { program: PathBuf, args: Vec<String>, cwd: PathBuf, env: HashMap<String, String> },
}

pub fn plan(profile: &Profile) -> Result<Plan> {
    let exe = profile.executable.trim();
    if exe.is_empty() {
        bail!("'{}' has no program or URL set", profile.name);
    }
    if is_url(exe) {
        return Ok(Plan::Url(exe.to_string()));
    }

    let mut env: HashMap<String, String> = std::env::vars().collect();
    let program = PathBuf::from(expand_env(exe, &env));
    let exe_dir = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let exe_dir_s = exe_dir.to_string_lossy().to_string();

    let fill = |s: &str, env: &HashMap<String, String>| {
        let s = s
            .replace("{exe_dir}", &exe_dir_s)
            .replace("{profile}", &profile.name)
            .replace("{game_dir}", profile.game_dir.trim())
            .replace("{repo_dir}", profile.repo_dir.trim());
        expand_env(&s, env)
    };

    let base_env = env.clone();
    for var in profile.env.iter().filter(|v| !v.key.trim().is_empty()) {
        env.insert(var.key.trim().to_string(), fill(&var.value, &base_env));
    }
    let cwd = if profile.working_dir.trim().is_empty() {
        exe_dir
    } else {
        PathBuf::from(fill(profile.working_dir.trim(), &env))
    };
    let args = split_args(&profile.arguments).iter().map(|a| fill(a, &env)).collect();
    Ok(Plan::Process { program, args, cwd, env })
}

/// A file to copy into the game folder before launch.
#[derive(Debug, PartialEq)]
pub struct Copy {
    pub from: PathBuf,
    pub to: PathBuf,
}

fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

/// For repo profiles: work out what to run from the repo's method. Returns a
/// profile that runs it directly, plus any files to copy first. Program
/// profiles come back unchanged.
pub fn resolve(profile: &Profile, repos: &[Repo]) -> Result<(Profile, Vec<Copy>)> {
    if profile.kind != LaunchKind::Repo {
        return Ok((profile.clone(), vec![]));
    }
    let repo = repos
        .iter()
        .find(|r| r.id == profile.repo_id)
        .ok_or_else(|| anyhow::anyhow!("Pick a repo for '{}'", profile.name))?;
    let game_dir = PathBuf::from(profile.game_dir.trim());
    if profile.game_dir.trim().is_empty() || !game_dir.is_dir() {
        bail!("Pick the game folder for '{}'", profile.name);
    }
    let repo_dir = PathBuf::from(profile.repo_dir.trim());
    let has_repo_dir = !profile.repo_dir.trim().is_empty();
    if has_repo_dir && !repo_dir.is_dir() {
        bail!("Repo folder not found: {}", repo_dir.display());
    }
    let fill_dirs = |s: &str| {
        s.replace("{game_dir}", profile.game_dir.trim()).replace("{repo_dir}", profile.repo_dir.trim())
    };

    let mut out = profile.clone();
    out.env.push(EnvVar { key: "GAME_DIR".into(), value: profile.game_dir.trim().into() });
    out.env.push(EnvVar { key: "REPO_DIR".into(), value: profile.repo_dir.trim().into() });
    let mut copies = vec![];

    match &repo.method {
        Method::Bat { file, args } => {
            let file = PathBuf::from(fill_dirs(file));
            let bat = if file.is_absolute() {
                file
            } else if has_repo_dir {
                repo_dir.join(file)
            } else {
                bail!("Pick the folder you downloaded {} to", repo.name);
            };
            if !bat.is_file() {
                bail!("{} not found", bat.display());
            }
            let bat_s = bat.to_string_lossy().into_owned();
            if cfg!(windows) {
                out.executable = bat_s;
                out.arguments = args.clone();
            } else {
                out.executable = "sh".into();
                out.arguments = format!("{} {args}", quote(&bat_s));
            }
            out.working_dir = bat.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        }
        Method::CopyAndRun { copy, run, args } => {
            if !has_repo_dir {
                bail!("Pick the folder you downloaded {} to", repo.name);
            }
            for rel in copy {
                let from = repo_dir.join(fill_dirs(rel));
                if !from.is_file() {
                    bail!("{} not found in the repo folder", rel);
                }
                let name = from.file_name().unwrap_or_default();
                copies.push(Copy { to: game_dir.join(name), from });
            }
            out.executable = game_dir.join(fill_dirs(run)).to_string_lossy().into_owned();
            out.arguments = args.clone();
            out.working_dir = profile.game_dir.trim().into();
        }
    }
    out.kind = LaunchKind::Program;
    Ok((out, copies))
}

/// Copy files into the game folder. A file that is already there is backed
/// up once as `<name>.launcher-backup` before it is replaced.
pub fn do_copies(copies: &[Copy]) -> Result<()> {
    for c in copies {
        if c.to.exists() {
            let mut backup = c.to.clone().into_os_string();
            backup.push(".launcher-backup");
            let backup = PathBuf::from(backup);
            if !backup.exists() && std::fs::read(&c.to).ok() != std::fs::read(&c.from).ok() {
                std::fs::copy(&c.to, &backup)?;
            }
        }
        std::fs::copy(&c.from, &c.to)
            .map_err(|e| anyhow::anyhow!("Could not copy {} into the game folder: {e}", c.from.display()))?;
    }
    Ok(())
}

pub fn backups_root() -> PathBuf {
    storage::data_dir().join("mod-backups")
}

/// What happened when a launched game finished.
#[derive(Debug, Clone)]
pub struct ExitInfo {
    /// `None` for launcher URLs, which give us no process to wait on.
    pub code: Option<i32>,
    pub restore_error: Option<String>,
}

/// Start the profile, applying its mod overlay first if it has one.
///
/// When the game exits, the overlay is undone (if `restore_mods` is set) and
/// `on_exit` is called. With `wait` this blocks until then, otherwise a
/// background thread waits.
pub fn launch(
    profile: &Profile,
    repos: &[Repo],
    wait: bool,
    on_exit: impl FnOnce(ExitInfo) + Send + 'static,
) -> Result<()> {
    let (resolved, copies) = resolve(profile, repos)?;
    let profile = &resolved;
    let plan = plan(profile)?;
    do_copies(&copies)?;

    let overlay = if profile.mod_source.trim().is_empty() {
        None
    } else {
        let target = if !profile.mod_target.trim().is_empty() {
            PathBuf::from(profile.mod_target.trim())
        } else if let Plan::Process { cwd, .. } = &plan {
            cwd.clone()
        } else {
            bail!("Set 'Copy mods into' for profiles that start from a URL");
        };
        Some(mods::apply_overlay(Path::new(profile.mod_source.trim()), &target, &backups_root())?)
    };

    let (program, args, cwd, env) = match plan {
        Plan::Url(url) => {
            // Steam/Epic take over from here, so there is nothing to wait on
            // and mod files stay in place until restored by hand.
            open::that_detached(&url)?;
            on_exit(ExitInfo { code: None, restore_error: None });
            return Ok(());
        }
        Plan::Process { program, args, cwd, env } => (program, args, cwd, env),
    };

    let mut cmd = Command::new(&program);
    cmd.args(&args).env_clear().envs(&env);
    if !cwd.as_os_str().is_empty() {
        cmd.current_dir(&cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Some(o) = &overlay {
                let _ = mods::restore_overlay(o);
            }
            bail!("Could not start {}: {e}", program.display());
        }
    };

    let restore = profile.restore_mods;
    let watch = move || {
        let code = child.wait().ok().and_then(|s| s.code());
        let restore_error = match (&overlay, restore) {
            (Some(o), true) => mods::restore_overlay(o).err().map(|e| e.to_string()),
            _ => None,
        };
        on_exit(ExitInfo { code, restore_error });
    };
    if wait {
        watch();
    } else {
        std::thread::spawn(watch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EnvVar;

    #[test]
    fn splits_like_windows() {
        assert_eq!(
            split_args(r#"-window -path "C:\My Mods\x" -fps=60"#),
            vec!["-window", "-path", r"C:\My Mods\x", "-fps=60"]
        );
        assert_eq!(split_args(r#""""#), vec![""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn detects_urls() {
        assert!(is_url("steam://rungameid/489830"));
        assert!(is_url("com.epicgames.launcher://apps/x?action=launch"));
        assert!(!is_url(r"C:\Games\game.exe"));
        assert!(!is_url("C://Games/game.exe"));
    }

    #[test]
    fn expands_env_styles() {
        let env: HashMap<String, String> =
            [("APPDATA".into(), "/ad".into()), ("X".into(), "1".into())].into();
        assert_eq!(expand_env("%appdata%/a $X ${X}b %NOPE% 100%", &env), "/ad/a 1 1b %NOPE% 100%");
    }

    #[test]
    fn plan_fills_placeholders() {
        let p = Profile {
            executable: "/games/g/bin/game".into(),
            arguments: r#"-cfg "{exe_dir}/cfg.ini" -p {profile}"#.into(),
            env: vec![EnvVar { key: "DIR".into(), value: "{exe_dir}".into() }],
            ..Profile::named("Mods")
        };
        let Plan::Process { program, args, cwd, env } = plan(&p).unwrap() else { panic!() };
        assert_eq!(program, PathBuf::from("/games/g/bin/game"));
        assert_eq!(args, vec!["-cfg", "/games/g/bin/cfg.ini", "-p", "Mods"]);
        assert_eq!(cwd, PathBuf::from("/games/g/bin"));
        assert_eq!(env["DIR"], "/games/g/bin");
    }

    #[test]
    fn plan_needs_a_program() {
        assert!(plan(&Profile::default()).is_err());
        assert_eq!(
            plan(&Profile { executable: "steam://rungameid/1".into(), ..Default::default() }).unwrap(),
            Plan::Url("steam://rungameid/1".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn runs_process_with_mods_then_restores() {
        let dir = tempfile::tempdir().unwrap();
        let (game, m) = (dir.path().join("game"), dir.path().join("mod"));
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(game.join("a.txt"), "orig").unwrap();
        std::fs::write(m.join("a.txt"), "mod").unwrap();
        // SAFETY: tests touching GAMELAUNCHER_HOME only run here.
        unsafe { std::env::set_var("GAMELAUNCHER_HOME", dir.path().join("home")) };

        let p = Profile {
            executable: "/bin/sh".into(),
            arguments: r#"-c "echo $(cat a.txt)-$MODE-$0 > seen.txt" hi"#.into(),
            working_dir: game.to_string_lossy().into(),
            env: vec![EnvVar { key: "MODE".into(), value: "modded".into() }],
            mod_source: m.to_string_lossy().into(),
            ..Profile::named("Test")
        };
        let (tx, rx) = std::sync::mpsc::channel();
        launch(&p, &[], true, move |info| tx.send(info).unwrap()).unwrap();
        let info = rx.recv().unwrap();
        assert_eq!(info.code, Some(0));
        assert!(info.restore_error.is_none());
        assert_eq!(std::fs::read_to_string(game.join("seen.txt")).unwrap().trim(), "mod-modded-hi");
        assert_eq!(std::fs::read_to_string(game.join("a.txt")).unwrap(), "orig");
    }

    #[cfg(unix)]
    #[test]
    fn repo_methods() {
        use crate::repos::{Method, Repo};
        let dir = tempfile::tempdir().unwrap();
        let (game, repo) = (dir.path().join("game"), dir.path().join("repo"));
        std::fs::create_dir_all(repo.join("bin")).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(repo.join("start.bat"), "echo \"$GAME_DIR|$1\" > \"$GAME_DIR/bat.txt\"").unwrap();
        std::fs::write(repo.join("bin/run.sh"), "new").unwrap();
        std::fs::write(game.join("run.sh"), "old").unwrap();
        let repos = vec![
            Repo { id: "b".into(), name: "B".into(), url: String::new(), instructions: String::new(),
                   method: Method::Bat { file: "start.bat".into(), args: "\"{game_dir}\"".into() } },
            Repo { id: "c".into(), name: "C".into(), url: String::new(), instructions: String::new(),
                   method: Method::CopyAndRun { copy: vec!["bin/run.sh".into()], run: "run.sh".into(), args: String::new() } },
        ];
        let mut p = Profile {
            kind: LaunchKind::Repo,
            repo_id: "b".into(),
            game_dir: game.to_string_lossy().into(),
            repo_dir: repo.to_string_lossy().into(),
            ..Profile::named("R")
        };
        launch(&p, &repos, true, |_| {}).unwrap();
        let g = game.to_string_lossy();
        assert_eq!(std::fs::read_to_string(game.join("bat.txt")).unwrap().trim(), format!("{g}|{g}"));

        p.repo_id = "c".into();
        let (resolved, copies) = resolve(&p, &repos).unwrap();
        assert_eq!(resolved.executable, game.join("run.sh").to_string_lossy());
        assert_eq!(copies, vec![Copy { from: repo.join("bin/run.sh"), to: game.join("run.sh") }]);
        do_copies(&copies).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("run.sh")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(game.join("run.sh.launcher-backup")).unwrap(), "old");

        p.repo_dir.clear();
        assert!(resolve(&p, &repos).is_err());
        p.repo_id = "missing".into();
        assert!(resolve(&p, &repos).is_err());
    }
}
