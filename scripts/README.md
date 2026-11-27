# Scripts

The `.bat` files for repos that use the `bat` method. A repo entry in
[`repos.json`](../repos.json) names one:

```json
"method": { "type": "bat", "file": "victory.bat", "args": "Play-VIctory.cmd" }
```

On Play the launcher clones that repo into its own folder, then runs the named
script **from here**, with the clone as the working folder. Drop your scripts
in this folder and name them in `repos.json`.

There are two worked examples here: [`victory.bat`](victory.bat) for the
cool_fox repo and [`victory-belobug.bat`](victory-belobug.bat) for the Belobug
fork, which copies the fork's `victory.example.toml` and starts its
`scripts\play.ps1`. Each does by hand what you would type yourself.

```
git clone https://git.gtahub.st/github/DoubleGhosty--cool_fox.git
cd cool_fox
cargo build --release --locked -p victory
set VICTORY_GAME_DIR=D:\path\to\your\title
.\Play-VIctory.cmd
```

The clone is the launcher's job; the script does the rest.

## What a script gets

| | |
| --- | --- |
| `%GAME_DIR%` | the game folder picked in the launcher |
| `%REPO_DIR%` | the clone, which is also the working folder |
| `%1`, `%2`… | whatever `args` says, with `{game_dir}`, `{repo_dir}` and `{profile}` filled in |

A repo that reads its own variable (VIctory wants `VICTORY_GAME_DIR`) gets it
from a line in the script: `set "VICTORY_GAME_DIR=%GAME_DIR%"`.

## Two things worth getting right

**Wait for the game.** The launcher watches the script's process, so a script
that starts the game and exits immediately makes the launcher think the game
closed at once. Use `call` for another `.bat`/`.cmd`, and `start /wait "" x.exe`
for a program:

```bat
call "%REPO_DIR%\Play-VIctory.cmd"
exit /b %ERRORLEVEL%
```

**Pass the exit code on.** `exit /b %ERRORLEVEL%` lets the launcher tell a
clean exit from a crash, and say so in the status bar.

For a PowerShell script in the repo, `powershell -NoProfile -ExecutionPolicy
Bypass -File ...` is the way in: a freshly cloned `.ps1` is unsigned, and
`-File` keeps the launcher waiting on it. Start with `cd /d "%REPO_DIR%"` if
the script builds anything, so `cargo` sees the clone's workspace rather than
whichever folder it was started from.

## One script, several entries

`args` is the cheap way to vary one script. The three VIctory entries all run
`victory.bat` and differ only in which `Play-*.cmd` they pass, so they share a
clone and a build: fetching happens once per repo URL, not once per entry.

## Where the launcher looks

In order, first hit wins:

1. `%APPDATA%\GameLauncher\scripts\` — yours, no rebuild needed
2. `scripts\` next to `gamelauncher.exe` — shipped with a build
3. this folder — while running from the source tree
