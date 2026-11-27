# Launcher

A fast, native desktop launcher for switching between launch profiles of games
you own or are making. Written in Rust with [egui](https://github.com/emilk/egui).

![Screenshot](docs/screenshot.png)

Each game has any number of profiles (vanilla, modded, debug build, an
emulator with different settings…), and switching is one click. A profile
says how to start the game:

- **Program or URL**: a game `.exe`, a mod loader, an emulator, or a launcher
  URL like `steam://rungameid/489830`.
- **Arguments**: typed like on the Windows command line. Quotes group words,
  backslashes are kept.
- **Working folder**: defaults to the program's folder.
- **Environment variables** for that profile only.
- **Mods** (optional): files in a mod folder are copied over the game folder
  before launch. Originals are backed up and put back when the game closes.
  If the launcher is closed or crashes first, it offers to put them back the
  next time it opens (or run `gamelauncher restore-mods`).

Placeholders `{exe_dir}` and `{profile}` and variables like `%APPDATA%` or
`$HOME` work in arguments, the working folder and env values.

The ★ profile is the game's default. Changes save automatically.

## Build

Needs [Rust](https://rustup.rs) (stable).

```
cargo run --release            # open the window
cargo build --release          # target/release/gamelauncher(.exe)
cargo test
```

## Command line and shortcuts

```
gamelauncher list
gamelauncher run "My Game"              # default profile
gamelauncher run "My Game" "Modded"     # a specific profile
gamelauncher restore-mods
```

To make a desktop shortcut for one profile, point it at
`gamelauncher.exe run "My Game" "Modded"`.

## Where settings live

`%APPDATA%\GameLauncher\library.json` on Windows, `~/.config/gamelauncher`
elsewhere. Set `GAMELAUNCHER_HOME` to keep it somewhere else, for example next
to the launcher for a portable setup.

## Notes

- Profiles that start from a URL hand the game to Steam/Epic, so the launcher
  can't tell when the game closes. Mod files for those profiles stay in place
  until you restore them.
