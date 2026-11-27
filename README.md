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

Instead of a program, a profile can use a **repo method**: pick the game
folder, choose a repo from the dropdown, pick the folder you downloaded that
repo to, and follow the instructions shown for it. See [Repo list](#repo-list).

The ★ profile is the game's default. Changes save automatically.

## Build

Needs [Rust](https://rustup.rs) (stable).

```
cargo run --release            # open the window
cargo build --release          # target/release/gamelauncher(.exe)
cargo test
```

## Repo list

The dropdown is filled from [`repos.json`](repos.json), which is built into
the program. Each entry looks like this:

```json
{
  "id": "my-tool",
  "name": "My Tool",
  "url": "https://github.com/owner/my-tool",
  "method": { "type": "bat", "file": "launch.bat", "args": "\"{game_dir}\"" },
  "instructions": "1. Download the repo.\n2. Pick its folder.\n3. Press Play."
}
```

- `id`: saved in profiles, so keep it the same once people use it. Must be unique.
- `name`: shown in the dropdown. `url`: shown as an "Open on GitHub" link.
- `instructions`: shown under the dropdown. Use `\n` for new lines.
- `method`: one of the two below.

| Method | Fields | What happens on Play |
| --- | --- | --- |
| `bat` | `file`, `args` (optional) | Opens `file` (relative to the repo folder) with `args`. |
| `copy_and_run` | `copy` (list), `run`, `args` (optional) | Copies each file in `copy` from the repo folder into the game folder, then runs `run` from the game folder. |

`{game_dir}` and `{repo_dir}` work in `file`, `copy`, `run` and `args`, and the
program gets `GAME_DIR` and `REPO_DIR` environment variables. When
`copy_and_run` replaces a file that is already in the game folder, the
original is kept once as `<name>.launcher-backup`.

To try changes without rebuilding, put your `repos.json` in the settings
folder (see below). It then replaces the built-in list. If it has a mistake,
the launcher says so and uses the built-in list.

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
