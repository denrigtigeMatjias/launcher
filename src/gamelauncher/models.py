"""Data model: a library holds games, and each game holds launch profiles."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Optional


@dataclass
class Profile:
    """One way of starting a game.

    ``executable`` is either a path to a program (a game .exe, an emulator, a
    mod loader) or a launcher URL such as ``steam://rungameid/271590``.
    """

    name: str
    executable: str = ""
    arguments: str = ""
    working_dir: str = ""
    env: dict[str, str] = field(default_factory=dict)
    # Optional mod overlay: files in mod_source are copied over mod_target
    # before launch. Originals are backed up and put back after the game exits
    # when restore_mods is true.
    mod_source: str = ""
    mod_target: str = ""
    restore_mods: bool = True
    notes: str = ""

    @classmethod
    def from_dict(cls, data: dict) -> "Profile":
        known = {f for f in cls.__dataclass_fields__}
        clean = {k: v for k, v in data.items() if k in known}
        clean["env"] = {str(k): str(v) for k, v in (clean.get("env") or {}).items()}
        return cls(**clean)

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class Game:
    name: str
    profiles: list[Profile] = field(default_factory=list)
    default_profile: str = ""

    def profile(self, name: str) -> Optional[Profile]:
        return next((p for p in self.profiles if p.name == name), None)

    def default(self) -> Optional[Profile]:
        return self.profile(self.default_profile) or (self.profiles[0] if self.profiles else None)

    @classmethod
    def from_dict(cls, data: dict) -> "Game":
        return cls(
            name=data["name"],
            profiles=[Profile.from_dict(p) for p in data.get("profiles", [])],
            default_profile=data.get("default_profile", ""),
        )

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "default_profile": self.default_profile,
            "profiles": [p.to_dict() for p in self.profiles],
        }


@dataclass
class Library:
    games: list[Game] = field(default_factory=list)

    def game(self, name: str) -> Optional[Game]:
        return next((g for g in self.games if g.name == name), None)

    @classmethod
    def from_dict(cls, data: dict) -> "Library":
        return cls(games=[Game.from_dict(g) for g in data.get("games", [])])

    def to_dict(self) -> dict:
        return {"version": 1, "games": [g.to_dict() for g in self.games]}


def unique_name(base: str, taken: set[str]) -> str:
    """Return ``base`` or ``base (2)``, ``base (3)``... whichever is free."""
    if base not in taken:
        return base
    n = 2
    while f"{base} ({n})" in taken:
        n += 1
    return f"{base} ({n})"
