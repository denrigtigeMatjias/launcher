"""Tkinter window: games on the left, their profiles in the middle, the
selected profile's settings on the right."""

from __future__ import annotations

import tkinter as tk
from tkinter import filedialog, messagebox, simpledialog, ttk

from . import launch, mods
from .models import Game, Library, Profile, unique_name
from .storage import load_library, save_library


def parse_env(text: str) -> dict[str, str]:
    env = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition("=")
        if not sep or not key.strip():
            raise ValueError(f"Environment line needs KEY=VALUE: {line}")
        env[key.strip()] = value.strip()
    return env


def format_env(env: dict[str, str]) -> str:
    return "\n".join(f"{k}={v}" for k, v in env.items())


class LauncherApp(ttk.Frame):
    def __init__(self, root: tk.Tk) -> None:
        super().__init__(root, padding=8)
        self.root = root
        self.library: Library = load_library()
        self.game: Game | None = None
        self.profile: Profile | None = None
        self.status = tk.StringVar(value="Ready")
        self._build()
        self._refresh_games()
        self.grid(sticky="nsew")
        root.columnconfigure(0, weight=1)
        root.rowconfigure(0, weight=1)
        self.after(200, self._offer_pending_restores)

    # ---------- layout ----------
    def _build(self) -> None:
        self.columnconfigure(2, weight=1)
        self.rowconfigure(0, weight=1)

        games = ttk.LabelFrame(self, text="Games", padding=6)
        games.grid(row=0, column=0, sticky="nsew", padx=(0, 6))
        games.rowconfigure(0, weight=1)
        self.games_list = tk.Listbox(games, exportselection=False, width=20)
        self.games_list.grid(row=0, column=0, columnspan=3, sticky="nsew")
        self.games_list.bind("<<ListboxSelect>>", lambda e: self._select_game())
        ttk.Button(games, text="Add", command=self._add_game).grid(row=1, column=0, pady=(6, 0))
        ttk.Button(games, text="Rename", command=self._rename_game).grid(row=1, column=1, pady=(6, 0))
        ttk.Button(games, text="Remove", command=self._remove_game).grid(row=1, column=2, pady=(6, 0))

        profiles = ttk.LabelFrame(self, text="Profiles", padding=6)
        profiles.grid(row=0, column=1, sticky="nsew", padx=(0, 6))
        profiles.rowconfigure(0, weight=1)
        self.profiles_list = tk.Listbox(profiles, exportselection=False, width=22)
        self.profiles_list.grid(row=0, column=0, columnspan=2, sticky="nsew")
        self.profiles_list.bind("<<ListboxSelect>>", lambda e: self._select_profile())
        self.profiles_list.bind("<Double-Button-1>", lambda e: self._launch())
        ttk.Button(profiles, text="New", command=self._new_profile).grid(row=1, column=0, sticky="ew", pady=(6, 0))
        ttk.Button(profiles, text="Duplicate", command=self._duplicate_profile).grid(row=1, column=1, sticky="ew", pady=(6, 0))
        ttk.Button(profiles, text="Set default", command=self._set_default).grid(row=2, column=0, sticky="ew")
        ttk.Button(profiles, text="Delete", command=self._delete_profile).grid(row=2, column=1, sticky="ew")

        form = ttk.LabelFrame(self, text="Profile settings", padding=6)
        form.grid(row=0, column=2, sticky="nsew")
        form.columnconfigure(1, weight=1)
        self.vars = {k: tk.StringVar() for k in
                     ("name", "executable", "arguments", "working_dir", "mod_source", "mod_target")}
        self.restore_var = tk.BooleanVar(value=True)

        def row(r, label, key, browse=None):
            ttk.Label(form, text=label).grid(row=r, column=0, sticky="w", pady=2)
            ttk.Entry(form, textvariable=self.vars[key]).grid(row=r, column=1, sticky="ew", pady=2)
            if browse:
                ttk.Button(form, text="…", width=3, command=browse).grid(row=r, column=2, padx=(4, 0))

        row(0, "Name", "name")
        row(1, "Program or URL", "executable", self._browse_exe)
        row(2, "Arguments", "arguments")
        row(3, "Working folder", "working_dir", lambda: self._browse_dir("working_dir"))
        ttk.Label(form, text="Environment\n(KEY=VALUE per line)").grid(row=4, column=0, sticky="nw", pady=2)
        self.env_text = tk.Text(form, height=5, width=30)
        self.env_text.grid(row=4, column=1, columnspan=2, sticky="nsew", pady=2)
        row(5, "Mod folder", "mod_source", lambda: self._browse_dir("mod_source"))
        row(6, "Copy mods into", "mod_target", lambda: self._browse_dir("mod_target"))
        ttk.Checkbutton(form, text="Undo mod files when the game closes",
                        variable=self.restore_var).grid(row=7, column=1, columnspan=2, sticky="w")
        ttk.Label(form, text="Notes").grid(row=8, column=0, sticky="nw", pady=2)
        self.notes_text = tk.Text(form, height=4, width=30)
        self.notes_text.grid(row=8, column=1, columnspan=2, sticky="nsew", pady=2)
        form.rowconfigure(8, weight=1)
        ttk.Label(form, foreground="gray", wraplength=290, justify="left", text=(
            "Placeholders: {exe_dir}, {profile}. %VARS% are expanded. "
            "Empty working folder = the program's folder. "
            "Empty 'Copy mods into' = the working folder."
        )).grid(row=9, column=1, columnspan=2, sticky="w")

        buttons = ttk.Frame(form)
        buttons.grid(row=10, column=0, columnspan=3, sticky="e", pady=(8, 0))
        ttk.Button(buttons, text="Save", command=self._save_profile).pack(side="left", padx=4)
        ttk.Button(buttons, text="▶ Launch", command=self._launch).pack(side="left")

        ttk.Label(self, textvariable=self.status, anchor="w").grid(
            row=1, column=0, columnspan=3, sticky="ew", pady=(6, 0))
        self._set_form_enabled(False)

    def _set_form_enabled(self, enabled: bool) -> None:
        state = "normal" if enabled else "disabled"
        for child in self.env_text, self.notes_text:
            child.configure(state=state)

    # ---------- refresh ----------
    def _refresh_games(self, select: str | None = None) -> None:
        self.games_list.delete(0, "end")
        for g in self.library.games:
            self.games_list.insert("end", g.name)
        names = [g.name for g in self.library.games]
        if select in names:
            i = names.index(select)
            self.games_list.selection_set(i)
            self.games_list.see(i)
        self._select_game()

    def _refresh_profiles(self, select: str | None = None) -> None:
        self.profiles_list.delete(0, "end")
        if not self.game:
            self._load_form(None)
            return
        default = self.game.default()
        for p in self.game.profiles:
            self.profiles_list.insert("end", f"★ {p.name}" if p is default else f"   {p.name}")
        names = [p.name for p in self.game.profiles]
        if select is None and default:
            select = default.name
        if select in names:
            self.profiles_list.selection_set(names.index(select))
        self._select_profile()

    def _select_game(self) -> None:
        sel = self.games_list.curselection()
        self.game = self.library.games[sel[0]] if sel else None
        self._refresh_profiles()

    def _select_profile(self) -> None:
        sel = self.profiles_list.curselection()
        self.profile = self.game.profiles[sel[0]] if (self.game and sel) else None
        self._load_form(self.profile)

    def _load_form(self, p: Profile | None) -> None:
        self._set_form_enabled(True)
        for key, var in self.vars.items():
            var.set(getattr(p, key) if p else "")
        self.restore_var.set(p.restore_mods if p else True)
        self.env_text.delete("1.0", "end")
        self.notes_text.delete("1.0", "end")
        if p:
            self.env_text.insert("1.0", format_env(p.env))
            self.notes_text.insert("1.0", p.notes)
        self._set_form_enabled(p is not None)

    def _persist(self) -> None:
        save_library(self.library)

    # ---------- games ----------
    def _add_game(self) -> None:
        name = simpledialog.askstring("Add game", "Game name:", parent=self.root)
        if not name or not name.strip():
            return
        name = unique_name(name.strip(), {g.name for g in self.library.games})
        game = Game(name=name, profiles=[Profile(name="Default")], default_profile="Default")
        self.library.games.append(game)
        self._persist()
        self._refresh_games(select=name)

    def _rename_game(self) -> None:
        if not self.game:
            return
        name = simpledialog.askstring("Rename game", "New name:", initialvalue=self.game.name, parent=self.root)
        if not name or not name.strip() or name.strip() == self.game.name:
            return
        self.game.name = unique_name(name.strip(), {g.name for g in self.library.games})
        self._persist()
        self._refresh_games(select=self.game.name)

    def _remove_game(self) -> None:
        if self.game and messagebox.askyesno("Remove game", f"Remove {self.game.name} and all its profiles?"):
            self.library.games.remove(self.game)
            self._persist()
            self._refresh_games()

    # ---------- profiles ----------
    def _new_profile(self) -> None:
        if not self.game:
            messagebox.showinfo("No game", "Add or pick a game first.")
            return
        name = unique_name("New profile", {p.name for p in self.game.profiles})
        self.game.profiles.append(Profile(name=name))
        self._persist()
        self._refresh_profiles(select=name)

    def _duplicate_profile(self) -> None:
        if not (self.game and self.profile):
            return
        copy = Profile.from_dict(self.profile.to_dict())
        copy.name = unique_name(f"{self.profile.name} copy", {p.name for p in self.game.profiles})
        self.game.profiles.insert(self.game.profiles.index(self.profile) + 1, copy)
        self._persist()
        self._refresh_profiles(select=copy.name)

    def _delete_profile(self) -> None:
        if self.game and self.profile and messagebox.askyesno("Delete profile", f"Delete '{self.profile.name}'?"):
            self.game.profiles.remove(self.profile)
            self._persist()
            self._refresh_profiles()

    def _set_default(self) -> None:
        if self.game and self.profile:
            self.game.default_profile = self.profile.name
            self._persist()
            self._refresh_profiles(select=self.profile.name)

    def _save_profile(self) -> bool:
        if not (self.game and self.profile):
            return False
        try:
            env = parse_env(self.env_text.get("1.0", "end"))
        except ValueError as exc:
            messagebox.showerror("Environment", str(exc))
            return False
        new_name = self.vars["name"].get().strip() or self.profile.name
        others = {p.name for p in self.game.profiles if p is not self.profile}
        if new_name in others:
            messagebox.showerror("Name taken", f"This game already has a profile called '{new_name}'.")
            return False
        if self.game.default_profile == self.profile.name:
            self.game.default_profile = new_name
        self.profile.name = new_name
        for key in ("executable", "arguments", "working_dir", "mod_source", "mod_target"):
            setattr(self.profile, key, self.vars[key].get().strip())
        self.profile.env = env
        self.profile.restore_mods = self.restore_var.get()
        self.profile.notes = self.notes_text.get("1.0", "end").rstrip()
        self._persist()
        self._refresh_profiles(select=new_name)
        self.status.set(f"Saved {self.game.name} / {new_name}")
        return True

    # ---------- launching ----------
    def _launch(self) -> None:
        if not self._save_profile():
            return
        profile, game = self.profile, self.game
        label = f"{game.name} / {profile.name}"

        def done(code):
            msg = f"{label} closed" + (f" (exit code {code})" if code is not None else "")
            self.after(0, lambda: self.status.set(msg))

        try:
            launch.launch(profile, on_exit=done)
        except Exception as exc:  # show any launch problem to the user
            messagebox.showerror("Could not launch", str(exc))
            self.status.set(f"Failed to launch {label}")
            return
        self.status.set(f"Running {label}")

    def _offer_pending_restores(self) -> None:
        pending = mods.pending_restores(launch.backups_root())
        if not pending:
            return
        targets = "\n".join(sorted({o.target for o in pending}))
        if messagebox.askyesno("Mod files still applied",
                               f"Mod files are still copied into:\n{targets}\n\nPut the original files back now?"):
            for overlay in pending:
                mods.restore_overlay(overlay)
            self.status.set("Original files restored")

    # ---------- browse ----------
    def _browse_exe(self) -> None:
        path = filedialog.askopenfilename(
            title="Pick the game, emulator or mod loader",
            filetypes=[("Programs", "*.exe *.bat *.cmd *.lnk"), ("All files", "*.*")])
        if path:
            self.vars["executable"].set(path)

    def _browse_dir(self, key: str) -> None:
        path = filedialog.askdirectory(title="Pick a folder")
        if path:
            self.vars[key].set(path)


def run_gui() -> None:
    root = tk.Tk()
    root.title("Game Launcher")
    root.geometry("1000x560")
    root.minsize(820, 460)
    LauncherApp(root)
    root.mainloop()
