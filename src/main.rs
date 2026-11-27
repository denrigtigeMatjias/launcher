// No console window when started from Explorer in release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod launch;
mod model;
mod mods;
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
  gamelauncher restore-mods             put back files replaced by mods";

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
            let (tx, rx) = std::sync::mpsc::channel();
            launch::launch(profile, &repos::load().0, true, move |info| {
                let _ = tx.send(info);
            })?;
            let info = rx.recv()?;
            if let Some(err) = info.restore_error {
                eprintln!("Mod files could not be restored: {err}. Run `gamelauncher restore-mods`.");
            }
            Ok(ExitCode::from(info.code.unwrap_or(0).clamp(0, 255) as u8))
        }
        "restore-mods" => {
            let pending = mods::pending_restores(&launch::backups_root());
            if pending.is_empty() {
                println!("Nothing to restore.");
            }
            for o in pending {
                mods::restore_overlay(&o)?;
                println!("Restored {}", o.target.display());
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
