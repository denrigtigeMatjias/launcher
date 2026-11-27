"""Command line: `gamelauncher` opens the window; subcommands work without it.

    gamelauncher list
    gamelauncher run "My Game" ["Profile name"]
    gamelauncher restore-mods

`run` is meant for desktop shortcuts: one shortcut per profile.
"""

from __future__ import annotations

import argparse
import sys

from . import launch, mods
from .storage import load_library


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="gamelauncher", description="Profile-based game launcher")
    sub = parser.add_subparsers(dest="command")
    sub.add_parser("list", help="list games and their profiles")
    run = sub.add_parser("run", help="launch a game with a profile")
    run.add_argument("game")
    run.add_argument("profile", nargs="?", help="defaults to the game's default profile")
    run.add_argument("--no-wait", action="store_true", help="return right after starting the game")
    sub.add_parser("restore-mods", help="undo mod overlays that are still applied")
    args = parser.parse_args(argv)

    if args.command is None:
        from .gui import run_gui

        run_gui()
        return 0

    if args.command == "list":
        library = load_library()
        if not library.games:
            print("No games yet. Run `gamelauncher` to add some.")
        for game in library.games:
            print(game.name)
            for p in game.profiles:
                mark = " (default)" if game.default() is p else ""
                print(f"  - {p.name}{mark}")
        return 0

    if args.command == "run":
        game = load_library().game(args.game)
        if game is None:
            print(f"No game named '{args.game}'", file=sys.stderr)
            return 2
        profile = game.profile(args.profile) if args.profile else game.default()
        if profile is None:
            print(f"No profile named '{args.profile}' for {game.name}", file=sys.stderr)
            return 2
        result: dict = {}
        launch.launch(profile, on_exit=lambda code: result.update(code=code), wait=not args.no_wait)
        code = result.get("code")
        return code if isinstance(code, int) else 0

    if args.command == "restore-mods":
        pending = mods.pending_restores(launch.backups_root())
        for overlay in pending:
            mods.restore_overlay(overlay)
            print(f"Restored {overlay.target}")
        if not pending:
            print("Nothing to restore.")
        return 0

    return 1
