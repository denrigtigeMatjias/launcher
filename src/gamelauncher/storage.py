"""Where the library lives on disk, and loading/saving it."""

from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path

from .models import Library


def data_dir() -> Path:
    """Folder for the library file and mod backups.

    Windows: %APPDATA%\\GameLauncher. Elsewhere: ~/.config/gamelauncher.
    Set GAMELAUNCHER_HOME to use another folder (handy for a portable setup).
    """
    override = os.environ.get("GAMELAUNCHER_HOME")
    if override:
        path = Path(override)
    elif sys.platform == "win32":
        path = Path(os.environ.get("APPDATA", Path.home())) / "GameLauncher"
    else:
        path = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "gamelauncher"
    path.mkdir(parents=True, exist_ok=True)
    return path


def library_path() -> Path:
    return data_dir() / "library.json"


def load_library(path: Path | None = None) -> Library:
    path = path or library_path()
    if not path.exists():
        return Library()
    with open(path, encoding="utf-8") as fh:
        return Library.from_dict(json.load(fh))


def save_library(library: Library, path: Path | None = None) -> None:
    """Write atomically so a crash never leaves a half-written library."""
    path = path or library_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".library-", suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(library.to_dict(), fh, indent=2)
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise
