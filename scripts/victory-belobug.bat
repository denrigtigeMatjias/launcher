@echo off
rem victory-belobug.bat - builds the Belobug AMD fork and starts the title.
rem
rem The repo's own steps (README.md, "How to run it"):
rem   git clone <repo>
rem   cd VIctory-7900XTX
rem   copy victory.example.toml victory.toml
rem   cargo build --locked --profile diag -p victory --features diagnostics
rem   scripts\play.ps1 -Exe <path of your copy's executable>
rem
rem The launcher does the clone and makes it the working folder, and gives us
rem %GAME_DIR% and %REPO_DIR%. %1 is the title's executable name inside the
rem game folder, from "args" in repos.json.
setlocal

rem cargo must run inside the clone, or it builds whatever workspace the
rem working folder happens to sit in. The launcher starts us there already;
rem this makes the script stand on its own.
cd /d "%REPO_DIR%" || exit /b 1

set "exe=%~1"
if "%exe%"=="" set "exe=game_scarlett_master.exe"
if not exist "%GAME_DIR%\%exe%" (
    echo %exe% is not in "%GAME_DIR%".
    echo Point the game folder at the folder holding your copy of the title.
    exit /b 2
)

rem The fork reads victory.toml from the repo root. Its paths must point
rem outside the repo, and VICTORY_GAME_DIR below takes precedence over the
rem file's game_dir, so the copied example is enough to start from.
if not exist "%REPO_DIR%\victory.toml" (
    copy /y "%REPO_DIR%\victory.example.toml" "%REPO_DIR%\victory.toml" >nul
    echo Made %REPO_DIR%\victory.toml. Edit it for the optional paths ^(FSR, cache^).
)

echo Building victory, diag profile ^(the first run takes a while^)...
cargo build --locked --profile diag -p victory --features diagnostics
if errorlevel 1 (
    echo Build failed, see above. No cargo at all? Install Rust: https://rustup.rs
    exit /b 1
)

rem play.ps1 keeps VICTORY_GAME_DIR and the other path variables, and replaces
rem the rest with the settings this fork is played with.
set "VICTORY_GAME_DIR=%GAME_DIR%"

rem -ExecutionPolicy Bypass: play.ps1 is a freshly cloned file, so it is not
rem signed and the default policy would refuse it. -File keeps the launcher
rem waiting on PowerShell, which waits on the game.
powershell -NoProfile -ExecutionPolicy Bypass -File "%REPO_DIR%\scripts\play.ps1" -Exe "%GAME_DIR%\%exe%"
exit /b %ERRORLEVEL%
