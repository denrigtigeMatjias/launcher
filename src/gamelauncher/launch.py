"""Turn a profile into a running process."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Optional

from . import mods
from .models import Profile
from .storage import data_dir

URL_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*://")


def is_url(executable: str) -> bool:
    return bool(URL_RE.match(executable.strip()))


def split_args(text: str) -> list[str]:
    """Split an argument string the way a Windows user would type it.

    Spaces separate arguments, double quotes group them, and backslashes are
    kept as they are, so ``-path "C:\\My Mods\\"`` works as expected.
    """
    args, current, in_quotes, has_token = [], [], False, False
    for ch in text:
        if ch == '"':
            in_quotes = not in_quotes
            has_token = True
        elif ch.isspace() and not in_quotes:
            if has_token:
                args.append("".join(current))
                current, has_token = [], False
        else:
            current.append(ch)
            has_token = True
    if has_token:
        args.append("".join(current))
    return args


def expand(value: str, variables: dict[str, str]) -> str:
    """Fill in {placeholders} and %ENV%/$ENV variables."""
    for key, val in variables.items():
        value = value.replace("{" + key + "}", val)
    return os.path.expandvars(value)


@dataclass
class LaunchPlan:
    argv: list[str]
    cwd: Optional[str]
    env: dict[str, str]
    url: Optional[str] = None


def plan(profile: Profile) -> LaunchPlan:
    exe = profile.executable.strip()
    if not exe:
        raise ValueError(f"Profile '{profile.name}' has no executable or URL")

    if is_url(exe):
        return LaunchPlan(argv=[], cwd=None, env={}, url=exe)

    exe = os.path.expandvars(os.path.expanduser(exe))
    exe_dir = str(Path(exe).parent)
    variables = {"exe_dir": exe_dir, "profile": profile.name}

    env = dict(os.environ)
    for key, val in profile.env.items():
        env[key] = expand(val, variables)

    cwd = expand(profile.working_dir, variables) if profile.working_dir.strip() else exe_dir
    args = [expand(a, variables) for a in split_args(profile.arguments)]
    return LaunchPlan(argv=[exe, *args], cwd=cwd or None, env=env)


def open_url(url: str) -> None:
    if sys.platform == "win32":
        os.startfile(url)  # type: ignore[attr-defined]
    elif sys.platform == "darwin":
        subprocess.Popen(["open", url])
    else:
        subprocess.Popen(["xdg-open", url])


def backups_root() -> Path:
    return data_dir() / "mod-backups"


def launch(
    profile: Profile,
    on_exit: Optional[Callable[[Optional[int]], None]] = None,
    wait: bool = False,
) -> Optional[subprocess.Popen]:
    """Start the profile. Applies its mod overlay first, if it has one.

    When the game exits, the overlay is undone (if restore_mods is set) and
    ``on_exit`` is called with the exit code. With ``wait=True`` this blocks
    until then, which is what the command line uses.
    """
    p = plan(profile)

    overlay = None
    if profile.mod_source.strip():
        target = profile.mod_target.strip() or (p.cwd or "")
        if not target:
            raise ValueError("Set 'Copy mods into' for profiles that launch a URL")
        overlay = mods.apply_overlay(profile.mod_source, target, backups_root())

    if p.url:
        # Launcher URLs hand off to Steam/Epic/etc., so there is no process to
        # wait on; a mod overlay stays in place until restored by hand.
        open_url(p.url)
        if on_exit:
            on_exit(None)
        return None

    try:
        kwargs = {}
        if sys.platform == "win32":
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP  # type: ignore[attr-defined]
        proc = subprocess.Popen(p.argv, cwd=p.cwd, env=p.env, **kwargs)
    except BaseException:
        if overlay:
            mods.restore_overlay(overlay)
        raise

    def watch() -> None:
        code = proc.wait()
        if overlay and profile.restore_mods:
            mods.restore_overlay(overlay)
        if on_exit:
            on_exit(code)

    if wait:
        watch()
    else:
        threading.Thread(target=watch, daemon=True).start()
    return proc
