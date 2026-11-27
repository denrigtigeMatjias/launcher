# Game Launcher

A small desktop launcher for switching between launch profiles of games you own
or are making. Each game has any number of profiles, and each profile says how
to start it:

- **Program or URL**: a game `.exe`, a mod loader, an emulator, or a launcher
  URL like `steam://rungameid/489830`.
- **Arguments**: typed like on the Windows command line. Quotes group words,
  backslashes are kept.
- **Working folder**: defaults to the program's folder.
- **Environment variables**: `KEY=VALUE`, one per line.
- **Mod overlay** (optional): files in a mod folder are copied over the game
  folder before launch. Originals are backed up and put back when the game
  closes. If the launcher is closed or crashes first, it offers to put them
  back the next time it opens (or run `gamelauncher restore-mods`).

Placeholders `{exe_dir}` and `{profile}` and environment variables
(`%APPDATA%`, `$HOME`) work in arguments, the working folder and env values.

![Screenshot](docs/screenshot.png)

## Running it

Needs Python 3.10 or newer. The Windows installer from python.org includes
Tkinter, which the window uses. There are no other dependencies.

```
pip install .
gamelauncher            # opens the window
```

Or without installing: `python -m gamelauncher` from the `src` folder.

## Command line and shortcuts

```
gamelauncher list
gamelauncher run "My Game"                  # default profile (★ in the window)
gamelauncher run "My Game" "Modded"         # a specific profile
gamelauncher restore-mods
```

To make a desktop shortcut for one profile, point it at
`pythonw -m gamelauncher run "My Game" "Modded"`.

## Where settings live

`%APPDATA%\GameLauncher\library.json` on Windows, `~/.config/gamelauncher`
elsewhere. Set `GAMELAUNCHER_HOME` to keep it somewhere else, for example next
to the launcher for a portable setup.

## Notes

- Profiles that launch a URL hand the game to Steam/Epic, so the launcher
  cannot tell when the game closes. Mod files for those profiles stay in place
  until you restore them.
- Tests: `pip install pytest && pytest`.
