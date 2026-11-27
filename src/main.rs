// No console window when started from Explorer in release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod fetch;
mod icons;
mod launch;
mod model;
mod repos;
mod storage;
mod ui;

use std::process::ExitCode;

const USAGE: &str = "\
Launcher: switch between launch profiles of your games.

Usage:
  gamelauncher                          open the window
  gamelauncher list                     list games and profiles
  gamelauncher run <game> [profile]     launch (default profile if none given)
  gamelauncher update [repo-id]         re-download repos (all of them if none given)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return match ui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(format!("Could not open the window: {e}")),
        };
    }
    attach_console();
    match cli(&args) {
        Ok(code) => code,
        Err(e) => fail(format!("{e:#}")),
    }
}

fn fail(msg: String) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

/// Download whatever the profile's repo needs, if it is not here yet.
fn fetch_for(profile: &model::Profile, repos: &[repos::Repo]) -> anyhow::Result<()> {
    let Some(repo) = repos.iter().find(|r| r.id == profile.repo_id) else {
        anyhow::bail!("'{}' has no repo set. Open the launcher and pick one.", profile.name);
    };
    fetch::ensure(repo, false, &|line| println!("{line}"))?;
    Ok(())
}

fn cli(args: &[String]) -> anyhow::Result<ExitCode> {
    match args[0].as_str() {
        "list" => {
            let library = storage::load()?;
            if library.games.is_empty() {
                println!("No games yet. Run gamelauncher without arguments to add some.");
            }
            for game in &library.games {
                println!("{}", game.name);
                let default = game.default_index();
                for (i, p) in game.profiles.iter().enumerate() {
                    println!("  - {}{}", p.name, if Some(i) == default { " (default)" } else { "" });
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let Some(game_name) = args.get(1) else { anyhow::bail!("Usage: gamelauncher run <game> [profile]") };
            let library = storage::load()?;
            let game = library.game(game_name).ok_or_else(|| anyhow::anyhow!("No game named '{game_name}'"))?;
            let profile = match args.get(2) {
                Some(p) => game.profile(p).ok_or_else(|| anyhow::anyhow!("{} has no profile named '{p}'", game.name))?,
                None => game.default().ok_or_else(|| anyhow::anyhow!("{} has no profiles", game.name))?,
            };
            let repos = repos::load().0;
            fetch_for(profile, &repos)?;
            let (tx, rx) = std::sync::mpsc::channel();
            launch::launch(profile, &repos, true, move |code| {
                let _ = tx.send(code);
            })?;
            Ok(ExitCode::from(rx.recv()?.unwrap_or(0).clamp(0, 255) as u8))
        }
        "update" => {
            let repos = repos::load().0;
            let wanted = args.get(1);
            let picked: Vec<&repos::Repo> =
                repos.iter().filter(|r| wanted.is_none_or(|w| r.id == *w)).collect();
            if picked.is_empty() {
                anyhow::bail!("No repo with id '{}'", wanted.map(String::as_str).unwrap_or(""));
            }
            // entries that share a url share a clone, so fetch it once
            let mut done = std::collections::HashSet::new();
            for repo in picked {
                if !done.insert(repo.url.trim()) {
                    println!("{} shares that clone", repo.name);
                    continue;
                }
                fetch::ensure(repo, true, &|line| println!("{line}"))?;
                println!("{} is up to date", repo.name);
            }
            Ok(ExitCode::SUCCESS)
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => anyhow::bail!("Unknown command '{other}'\n\n{USAGE}"),
    }
}

/// Release builds on Windows have no console of their own; borrow the one
/// we were started from so `list` and errors are visible in a terminal.
#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}
