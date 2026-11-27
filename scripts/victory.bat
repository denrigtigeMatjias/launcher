@echo off
rem victory.bat - builds cool_fox and starts the title through VIctory.
rem
rem Used by the VIctory entries in repos.json. The launcher has already cloned
rem the repo and made it the working folder, and gives us:
rem   %GAME_DIR%  the game folder picked in the launcher
rem   %REPO_DIR%  the clone (same as the working folder)
rem   %1          which Play script to run, from "args" in repos.json
rem
rem Doing by hand what this does:
rem   git clone https://git.gtahub.st/github/DoubleGhosty--cool_fox.git
rem   cd cool_fox
rem   cargo build --release --locked -p victory
rem   set VICTORY_GAME_DIR=D:\path\to\your\title
rem   .\Play-VIctory.cmd
setlocal

rem cargo must run inside the clone, or it builds whatever workspace the
rem working folder happens to sit in. The launcher starts us there already;
rem this makes the script stand on its own.
cd /d "%REPO_DIR%" || exit /b 1

set "play=%~1"
if "%play%"=="" set "play=Play-VIctory.cmd"
if not exist "%REPO_DIR%\%play%" (
    echo %play% is not in the repo. It has:
    dir /b "%REPO_DIR%\Play-*.cmd"
    exit /b 2
)

rem The first build is a long one; later Plays reuse the target folder.
echo Building victory (first run takes a while)...
cargo build --release --locked -p victory
if errorlevel 1 (
    echo Build failed, see above. No cargo at all? Install Rust: https://rustup.rs
    exit /b 1
)

rem VIctory finds the title through this; Play-VIctory.cmd refuses without it.
set "VICTORY_GAME_DIR=%GAME_DIR%"

rem "call", not "start": the launcher watches this script, so it must stay
rem alive as long as the game does, and pass the game's exit code back.
call "%REPO_DIR%\%play%"
exit /b %ERRORLEVEL%
