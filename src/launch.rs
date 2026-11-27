//! Turn a profile into a running process.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::fetch;
use crate::model::Profile;
use crate::repos::{self, Method, Repo};

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

/// A file to copy into the game folder before launch.
#[derive(Debug, PartialEq)]
pub struct Copy {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Everything needed to start a profile, worked out from its repo's method.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    /// Files to copy into the game folder before starting.
    pub copies: Vec<Copy>,
}

/// Work out what to run for `profile` from its repo's method.
///
/// The repo's files are wherever [`fetch`] put them, so this expects
/// [`fetch::ensure`] to have run. The program gets `GAME_DIR` and `REPO_DIR`,
/// and `{game_dir}`, `{repo_dir}`, `{profile}` and variables like `%APPDATA%`
/// are filled in wherever the method uses them.
pub fn plan(profile: &Profile, repos: &[Repo]) -> Result<Plan> {
    let repo = repos
        .iter()
        .find(|r| r.id == profile.repo_id)
        .ok_or_else(|| anyhow::anyhow!("Pick a repo for '{}'", profile.name))?;

    let game_dir_s = profile.game_dir.trim();
    let game_dir = PathBuf::from(game_dir_s);
    if game_dir_s.is_empty() || !game_dir.is_dir() {
        bail!("Pick the game folder for '{}'", profile.name);
    }
    let repo_dir = fetch::repo_dir(&repo.url);
    let repo_dir_s = repo_dir.to_string_lossy().into_owned();

    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("GAME_DIR".into(), game_dir_s.to_string());
    env.insert("REPO_DIR".into(), repo_dir_s.clone());

    let fill = |s: &str| {
        let s = s
            .replace("{game_dir}", game_dir_s)
            .replace("{repo_dir}", &repo_dir_s)
            .replace("{profile}", &profile.name);
        expand_env(&s, &env)
    };
    let fill_args = |s: &str| split_args(s).iter().map(|a| fill(a)).collect::<Vec<_>>();

    let (program, args, cwd, copies) = match &repo.method {
        Method::Bat { file, args } => {
            let named = fill(file);
            let path = PathBuf::from(&named);
            let bat = if path.is_absolute() {
                path
            } else {
                repos::script(&named).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{} runs '{named}', which is not in a scripts folder. Put it in {}",
                        repo.name,
                        repos::script_dirs()[0].display()
                    )
                })?
            };
            if !bat.is_file() {
                bail!("{} not found", bat.display());
            }
            // The script works on the clone, so start it there.
            if cfg!(windows) {
                (bat, fill_args(args), repo_dir.clone(), vec![])
            } else {
                let mut sh_args = vec![bat.to_string_lossy().into_owned()];
                sh_args.extend(fill_args(args));
                (PathBuf::from("sh"), sh_args, repo_dir.clone(), vec![])
            }
        }
        Method::CopyAndRun { assets, run, args } => {
            let mut copies = vec![];
            for name in assets {
                let from = repo_dir.join(name);
                if !from.is_file() {
                    bail!("{} has not been downloaded yet. Press Update for {}.", name, repo.name);
                }
                copies.push(Copy { to: game_dir.join(name), from });
            }
            let program = game_dir.join(fill(run));
            (program, fill_args(args), game_dir.clone(), copies)
        }
    };

    Ok(Plan { program, args, cwd, env, copies })
}

/// A file the launcher put in the game folder, and what to do about it when
/// the game closes.
#[derive(Debug, PartialEq)]
pub struct Restore {
    /// The file that was copied in.
    pub path: PathBuf,
    /// What stood there before, set aside as `<name>.launcher-backup`.
    pub backup: Option<PathBuf>,
}

/// Copy files into the game folder, setting aside anything already there.
///
/// Returns what to hand to [`undo_copies`] once the game has closed. An
/// existing `.launcher-backup` is left alone: that one is the real original,
/// from a run whose clean-up never happened.
pub fn do_copies(copies: &[Copy]) -> Result<Vec<Restore>> {
    let mut restores = vec![];
    for c in copies {
        let mut backup = None;
        if c.to.exists() {
            let mut name = c.to.clone().into_os_string();
            name.push(".launcher-backup");
            let saved = PathBuf::from(name);
            if !saved.exists() {
                std::fs::rename(&c.to, &saved)
                    .with_context(|| format!("setting aside {}", c.to.display()))?;
            }
            backup = Some(saved);
        }
        std::fs::copy(&c.from, &c.to)
            .map_err(|e| anyhow::anyhow!("Could not copy {} into the game folder: {e}", c.from.display()))?;
        restores.push(Restore { path: c.to.clone(), backup });
    }
    Ok(restores)
}

/// Put the game folder back as it was: take out what was copied in, and
/// return anything that was set aside.
///
/// Best effort, and deliberately quiet: a file the game still holds open is
/// left where it is, and the next launch copies over it anyway.
pub fn undo_copies(restores: &[Restore]) {
    for r in restores.iter().rev() {
        let _ = std::fs::remove_file(&r.path);
        if let Some(saved) = &r.backup {
            let _ = std::fs::rename(saved, &r.path);
        }
    }
}

/// Start the profile. When the game exits, anything copied into the game
/// folder is taken out again and `on_exit` is called with its exit code
/// (`None` if it was stopped by a signal). With `wait` this blocks until
/// then, otherwise a background thread waits.
pub fn launch(
    profile: &Profile,
    repos: &[Repo],
    wait: bool,
    on_exit: impl FnOnce(Option<i32>) + Send + 'static,
) -> Result<()> {
    let plan = plan(profile, repos)?;
    let restores = do_copies(&plan.copies)?;

    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args).env_clear().envs(&plan.env);
    if !plan.cwd.as_os_str().is_empty() {
        cmd.current_dir(&plan.cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            // nothing is going to run, so don't leave our files behind
            undo_copies(&restores);
            bail!("Could not start {}: {e}", plan.program.display());
        }
    };

    let watch = move || {
        let code = child.wait().ok().and_then(|s| s.code());
        undo_copies(&restores);
        on_exit(code);
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

    fn repo(id: &str, method: Method) -> Repo {
        Repo {
            id: id.into(),
            name: id.to_uppercase(),
            url: format!("https://github.com/test/{id}"),
            description: String::new(),
            instructions: String::new(),
            method,
        }
    }

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
    fn expands_env_styles() {
        let env: HashMap<String, String> =
            [("APPDATA".into(), "/ad".into()), ("X".into(), "1".into())].into();
        assert_eq!(expand_env("%appdata%/a $X ${X}b %NOPE% 100%", &env), "/ad/a 1 1b %NOPE% 100%");
    }

    /// The game folder is left exactly as it was found.
    #[test]
    fn copies_in_then_puts_the_game_folder_back() {
        let dir = tempfile::tempdir().unwrap();
        let (have, game) = (dir.path().join("have"), dir.path().join("game"));
        std::fs::create_dir_all(&have).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(have.join("victory.exe"), "downloaded").unwrap();
        let copies = vec![Copy { from: have.join("victory.exe"), to: game.join("victory.exe") }];

        // nothing there before: the file arrives, then leaves no trace
        let restores = do_copies(&copies).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("victory.exe")).unwrap(), "downloaded");
        undo_copies(&restores);
        assert!(std::fs::read_dir(&game).unwrap().next().is_none());

        // a file of the same name was there: it comes back, byte for byte
        std::fs::write(game.join("victory.exe"), "yours").unwrap();
        let restores = do_copies(&copies).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("victory.exe")).unwrap(), "downloaded");
        undo_copies(&restores);
        assert_eq!(std::fs::read_to_string(game.join("victory.exe")).unwrap(), "yours");
        assert_eq!(std::fs::read_dir(&game).unwrap().count(), 1);
    }

    /// A run whose clean-up never happened leaves a backup behind. The next
    /// run must not overwrite it with the copy we left there.
    #[test]
    fn an_older_backup_is_the_real_original() {
        let dir = tempfile::tempdir().unwrap();
        let (have, game) = (dir.path().join("have"), dir.path().join("game"));
        std::fs::create_dir_all(&have).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(have.join("victory.exe"), "downloaded").unwrap();
        std::fs::write(game.join("victory.exe"), "left behind").unwrap();
        std::fs::write(game.join("victory.exe.launcher-backup"), "yours").unwrap();

        let copies = vec![Copy { from: have.join("victory.exe"), to: game.join("victory.exe") }];
        let restores = do_copies(&copies).unwrap();
        undo_copies(&restores);
        assert_eq!(std::fs::read_to_string(game.join("victory.exe")).unwrap(), "yours");
        assert_eq!(std::fs::read_dir(&game).unwrap().count(), 1);
    }

    #[test]
    fn plan_needs_a_repo_a_game_folder_and_its_files() {
        assert!(plan(&Profile::default(), &[]).is_err());
        let repos = vec![repo("b", Method::Bat { file: "nothing-here.bat".into(), args: String::new() })];
        // the right repo, but no game folder yet
        let p = Profile { repo_id: "b".into(), ..Profile::named("R") };
        assert!(plan(&p, &repos).is_err());
        // a game folder, but the script it names is in no scripts folder
        let dir = tempfile::tempdir().unwrap();
        let p = Profile { game_dir: dir.path().to_string_lossy().into(), ..p };
        assert!(plan(&p, &repos).is_err());
    }

    /// Both methods, with the files where [`fetch`] would have put them.
    #[cfg(unix)]
    #[test]
    fn repo_methods() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        // SAFETY: tests touching GAMELAUNCHER_HOME only run here.
        unsafe { std::env::set_var("GAMELAUNCHER_HOME", &home) };

        let game = dir.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("run.sh"), "old").unwrap();

        // a script of yours, in the settings folder
        let scripts = home.join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(scripts.join("start.bat"), "echo \"$GAME_DIR|$REPO_DIR|$1|$2\" > \"$GAME_DIR/bat.txt\"").unwrap();

        let repos = vec![
            repo("b", Method::Bat { file: "start.bat".into(), args: "\"{game_dir}\" {profile}".into() }),
            repo(
                "c",
                Method::CopyAndRun {
                    assets: vec!["run.sh".into()],
                    run: "run.sh".into(),
                    args: String::new(),
                },
            ),
        ];
        // the cloned repo, and the downloaded release asset
        let cloned = fetch::repo_dir("https://github.com/test/b");
        std::fs::create_dir_all(&cloned).unwrap();
        let downloaded = fetch::repo_dir("https://github.com/test/c");
        std::fs::create_dir_all(&downloaded).unwrap();
        std::fs::write(downloaded.join("run.sh"), "new").unwrap();

        let mut p = Profile {
            repo_id: "b".into(),
            game_dir: game.to_string_lossy().into(),
            ..Profile::named("R")
        };

        let (tx, rx) = std::sync::mpsc::channel();
        launch(&p, &repos, true, move |code| tx.send(code).unwrap()).unwrap();
        assert_eq!(rx.recv().unwrap(), Some(0));
        let (g, r) = (game.to_string_lossy(), cloned.to_string_lossy());
        assert_eq!(std::fs::read_to_string(game.join("bat.txt")).unwrap().trim(), format!("{g}|{r}|{g}|R"));

        p.repo_id = "c".into();
        let plan = plan(&p, &repos).unwrap();
        assert_eq!(plan.program, game.join("run.sh"));
        assert_eq!(plan.copies, vec![Copy { from: downloaded.join("run.sh"), to: game.join("run.sh") }]);
        do_copies(&plan.copies).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("run.sh")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(game.join("run.sh.launcher-backup")).unwrap(), "old");

        // nothing downloaded yet, and an id that is not in the list
        std::fs::remove_file(downloaded.join("run.sh")).unwrap();
        assert!(plan(&p, &repos).is_err());
        p.repo_id = "missing".into();
        assert!(plan(&p, &repos).is_err());
    }
}
