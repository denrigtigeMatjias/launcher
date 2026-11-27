"""Mod overlays: copy a mod folder over a game folder, and undo it later.

Every change is recorded in a manifest saved next to the backups, so an
overlay can still be undone if the launcher was closed or crashed while the
game ran (see ``pending_restores``).
"""

from __future__ import annotations

import json
import shutil
import time
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class Overlay:
    target: str
    backup_dir: str
    created: list[str] = field(default_factory=list)   # files that did not exist before
    replaced: list[str] = field(default_factory=list)  # files whose originals are in backup_dir

    @property
    def manifest_path(self) -> Path:
        return Path(self.backup_dir) / "manifest.json"

    def save(self) -> None:
        self.manifest_path.write_text(json.dumps(self.__dict__, indent=2), encoding="utf-8")

    @classmethod
    def load(cls, manifest: Path) -> "Overlay":
        return cls(**json.loads(manifest.read_text(encoding="utf-8")))


def apply_overlay(source: str | Path, target: str | Path, backups_root: Path) -> Overlay:
    source, target = Path(source), Path(target)
    if not source.is_dir():
        raise FileNotFoundError(f"Mod folder not found: {source}")
    if not target.is_dir():
        raise FileNotFoundError(f"Mod target folder not found: {target}")
    if source.resolve() == target.resolve():
        raise ValueError("Mod folder and target folder are the same")

    backup = backups_root / time.strftime("%Y%m%d-%H%M%S")
    n = 1
    while backup.exists():
        n += 1
        backup = backups_root / f"{time.strftime('%Y%m%d-%H%M%S')}-{n}"
    (backup / "files").mkdir(parents=True)

    overlay = Overlay(target=str(target), backup_dir=str(backup))
    overlay.save()  # save early: a partial overlay can still be undone
    try:
        for src in sorted(p for p in source.rglob("*") if p.is_file()):
            rel = src.relative_to(source)
            dest = target / rel
            if dest.exists():
                saved = backup / "files" / rel
                saved.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(dest, saved)
                overlay.replaced.append(rel.as_posix())
            else:
                dest.parent.mkdir(parents=True, exist_ok=True)
                overlay.created.append(rel.as_posix())
            overlay.save()
            shutil.copy2(src, dest)
    except BaseException:
        restore_overlay(overlay)
        raise
    return overlay


def restore_overlay(overlay: Overlay) -> None:
    target = Path(overlay.target)
    backup = Path(overlay.backup_dir)
    for rel in reversed(overlay.created):
        path = target / rel
        if path.exists():
            path.unlink()
        # remove folders the overlay created, if they are now empty
        parent = path.parent
        while parent != target and parent.is_dir() and not any(parent.iterdir()):
            parent.rmdir()
            parent = parent.parent
    for rel in overlay.replaced:
        saved = backup / "files" / rel
        if saved.exists():
            shutil.copy2(saved, target / rel)
    shutil.rmtree(backup, ignore_errors=True)


def pending_restores(backups_root: Path) -> list[Overlay]:
    """Overlays that were applied but never undone."""
    if not backups_root.is_dir():
        return []
    return [Overlay.load(m) for m in sorted(backups_root.glob("*/manifest.json"))]
