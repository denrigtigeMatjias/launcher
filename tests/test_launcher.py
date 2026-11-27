import sys
from pathlib import Path

import pytest

from gamelauncher import launch, mods
from gamelauncher.cli import main
from gamelauncher.models import Game, Library, Profile, unique_name
from gamelauncher.storage import load_library, save_library


@pytest.fixture(autouse=True)
def home(tmp_path, monkeypatch):
    monkeypatch.setenv("GAMELAUNCHER_HOME", str(tmp_path / "home"))
    return tmp_path / "home"


def test_split_args_keeps_windows_paths():
    assert launch.split_args(r'-window -path "C:\My Mods\x" -fps=60') == [
        "-window", "-path", r"C:\My Mods\x", "-fps=60"]
    assert launch.split_args('""') == [""]
    assert launch.split_args("   ") == []


def test_url_detection():
    assert launch.is_url("steam://rungameid/271590")
    assert launch.is_url("com.epicgames.launcher://apps/foo?action=launch")
    assert not launch.is_url(r"C:\Games\game.exe")


def test_plan_fills_placeholders_and_env(tmp_path, monkeypatch):
    monkeypatch.setenv("MYVAR", "hello")
    exe = tmp_path / "bin" / "game.exe"
    p = Profile(name="Mods", executable=str(exe), arguments='-cfg "{exe_dir}/cfg.ini" -p {profile}',
                env={"GREETING": "$MYVAR", "DIR": "{exe_dir}"})
    plan = launch.plan(p)
    assert plan.argv == [str(exe), "-cfg", f"{exe.parent}/cfg.ini", "-p", "Mods"]
    assert plan.cwd == str(exe.parent)
    assert plan.env["GREETING"] == "hello"
    assert plan.env["DIR"] == str(exe.parent)


def test_plan_requires_executable():
    with pytest.raises(ValueError):
        launch.plan(Profile(name="empty"))


def test_library_round_trip(tmp_path):
    lib = Library(games=[Game(name="G", default_profile="B",
                              profiles=[Profile(name="A"), Profile(name="B", env={"X": "1"})])])
    path = tmp_path / "lib.json"
    save_library(lib, path)
    loaded = load_library(path)
    assert loaded == lib
    assert loaded.games[0].default().name == "B"


def test_unknown_fields_are_ignored():
    p = Profile.from_dict({"name": "x", "future_field": 1})
    assert p.name == "x"


def test_unique_name():
    assert unique_name("A", set()) == "A"
    assert unique_name("A", {"A", "A (2)"}) == "A (3)"


def make_tree(root: Path, files: dict[str, str]) -> None:
    for rel, text in files.items():
        f = root / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(text)


def read_tree(root: Path) -> dict[str, str]:
    return {p.relative_to(root).as_posix(): p.read_text() for p in root.rglob("*") if p.is_file()}


def test_overlay_apply_and_restore(tmp_path):
    game, mod = tmp_path / "game", tmp_path / "mod"
    make_tree(game, {"data/a.txt": "orig a", "b.txt": "orig b"})
    make_tree(mod, {"data/a.txt": "mod a", "new/c.txt": "mod c"})
    before = read_tree(game)

    overlay = mods.apply_overlay(mod, game, tmp_path / "backups")
    assert read_tree(game) == {"data/a.txt": "mod a", "b.txt": "orig b", "new/c.txt": "mod c"}
    assert len(mods.pending_restores(tmp_path / "backups")) == 1

    mods.restore_overlay(overlay)
    assert read_tree(game) == before
    assert not (game / "new").exists()
    assert mods.pending_restores(tmp_path / "backups") == []


def test_pending_restore_survives_restart(tmp_path):
    game, mod = tmp_path / "game", tmp_path / "mod"
    make_tree(game, {"a.txt": "orig"})
    make_tree(mod, {"a.txt": "mod"})
    mods.apply_overlay(mod, game, tmp_path / "backups")
    (pending,) = mods.pending_restores(tmp_path / "backups")  # as if loaded after a crash
    mods.restore_overlay(pending)
    assert (game / "a.txt").read_text() == "orig"


def test_launch_runs_process_and_restores_mods(tmp_path):
    game, mod = tmp_path / "game", tmp_path / "mod"
    make_tree(game, {"a.txt": "orig"})
    make_tree(mod, {"a.txt": "mod"})
    # The "game" is a Python one-liner that records what it sees.
    script = "import os,sys; open('seen.txt','w').write(open('a.txt').read()+'|'+os.environ['MODE']+'|'+sys.argv[1])"
    p = Profile(name="Test", executable=sys.executable, arguments=f'-c "{script}" hi',
                working_dir=str(game), env={"MODE": "modded"}, mod_source=str(mod))
    codes = []
    launch.launch(p, on_exit=codes.append, wait=True)
    assert codes == [0]
    assert (game / "seen.txt").read_text() == "mod|modded|hi"
    assert (game / "a.txt").read_text() == "orig"


def test_cli_run_and_list(tmp_path, capsys):
    game_dir = tmp_path / "g"
    game_dir.mkdir()
    lib = Library(games=[Game(name="My Game", default_profile="Vanilla", profiles=[
        Profile(name="Vanilla", executable=sys.executable, arguments='-c "open(\'v\',\'w\')"', working_dir=str(game_dir)),
        Profile(name="Other", executable=sys.executable, arguments='-c "open(\'o\',\'w\')"', working_dir=str(game_dir)),
    ])])
    save_library(lib)
    assert main(["list"]) == 0
    assert "Vanilla (default)" in capsys.readouterr().out
    assert main(["run", "My Game"]) == 0
    assert (game_dir / "v").exists() and not (game_dir / "o").exists()
    assert main(["run", "My Game", "Other"]) == 0
    assert (game_dir / "o").exists()
    assert main(["run", "Nope"]) == 2
