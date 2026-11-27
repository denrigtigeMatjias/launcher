//! The window: a sidebar of games, and for the selected game a header with a
//! big Play button, a row of profile chips, and the selected profile's settings.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText,
    ScrollArea, Sense, Stroke, TextEdit, TextStyle, Ui, Vec2, vec2,
};

use crate::launch::{self, ExitInfo};
use crate::model::{EnvVar, Game, LaunchKind, Library, Profile, unique_name};
use crate::repos::{self, Repo};
use crate::{mods, storage};

// ---------- palette ----------
const BG: Color32 = Color32::from_rgb(13, 15, 20);
const CARD: Color32 = Color32::from_rgb(24, 28, 37);
const FIELD: Color32 = Color32::from_rgb(15, 18, 24);
const BORDER: Color32 = Color32::from_rgb(38, 44, 57);
const TEXT: Color32 = Color32::from_rgb(230, 233, 240);
const MUTED: Color32 = Color32::from_rgb(140, 148, 165);
const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
const ACCENT_HOVER: Color32 = Color32::from_rgb(145, 118, 255);
const GREEN: Color32 = Color32::from_rgb(52, 199, 123);
const RED: Color32 = Color32::from_rgb(240, 90, 100);
const AMBER: Color32 = Color32::from_rgb(245, 180, 70);

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Launcher")
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([860.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native("Launcher", options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))))
}

pub fn apply_theme(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        use FontId as F;
        style.text_styles = [
            (TextStyle::Heading, F::proportional(28.0)),
            (TextStyle::Body, F::proportional(15.0)),
            (TextStyle::Button, F::proportional(15.0)),
            (TextStyle::Small, F::proportional(12.5)),
            (TextStyle::Monospace, F::monospace(14.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(14.0, 7.0);
        style.spacing.interact_size.y = 30.0;

        let v = &mut style.visuals;
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = FIELD;
        v.text_edit_bg_color = Some(FIELD);
        v.faint_bg_color = CARD;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = ACCENT_HOVER;
        v.window_corner_radius = CornerRadius::same(12);
        let r = CornerRadius::same(8);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = r;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 39, 51);
        v.widgets.inactive.bg_fill = Color32::from_rgb(34, 39, 51);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 50, 65);
        v.widgets.hovered.bg_fill = Color32::from_rgb(44, 50, 65);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(70, 78, 98));
        v.widgets.active.weak_bg_fill = Color32::from_rgb(52, 58, 75);
        v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Info,
    Good,
    Bad,
}

struct Toast {
    text: String,
    tone: Tone,
    at: Instant,
}

/// A game that is running right now, so the UI can show it.
struct Running {
    game: String,
    profile: String,
}

enum Event {
    Exited { game: String, profile: String, info: ExitInfo },
}

enum Action {
    AddGame,
    SelectGame(usize),
    PlayGame(usize),
    ShowLibrary,
    DeleteGame,
    SelectProfile(usize),
    NewProfile,
    DuplicateProfile,
    DeleteProfile,
    MakeDefault,
    Play,
    RestorePending,
}

pub struct App {
    library: Library,
    game: Option<usize>,
    profile: usize,
    dirty_since: Option<Instant>,
    toast: Option<Toast>,
    running: Vec<Running>,
    events_tx: Sender<Event>,
    events_rx: Receiver<Event>,
    pending: Vec<mods::Overlay>,
    repos: Vec<Repo>,
    confirm_delete_game: bool,
    confirm_delete_profile: bool,
    focus_game_name: bool,
    show_library: bool,
    ctx: egui::Context,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        apply_theme(ctx);
        let (library, toast) = match storage::load() {
            Ok(l) => (l, None),
            Err(e) => (
                Library::default(),
                Some(Toast { text: format!("Could not read your library: {e:#}"), tone: Tone::Bad, at: Instant::now() }),
            ),
        };
        let (repo_list, repo_error) = repos::load();
        let toast = toast.or(repo_error.map(|text| Toast { text, tone: Tone::Bad, at: Instant::now() }));
        let (events_tx, events_rx) = channel();
        let mut app = Self {
            game: if library.games.is_empty() { None } else { Some(0) },
            library,
            profile: 0,
            dirty_since: None,
            toast,
            running: vec![],
            events_tx,
            events_rx,
            pending: mods::pending_restores(&launch::backups_root()),
            repos: repo_list,
            confirm_delete_game: false,
            confirm_delete_profile: false,
            focus_game_name: false,
            show_library: true,
            ctx: ctx.clone(),
        };
        app.select_game(app.game);
        app
    }

    /// For tests and screenshots: start from a given library.
    #[allow(dead_code)]
    pub fn with_library(ctx: &egui::Context, library: Library, game: usize, profile: usize) -> Self {
        let mut app = Self::new(ctx);
        app.library = library;
        app.pending.clear();
        app.select_game(Some(game));
        app.profile = profile;
        app
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some(Toast { text: text.into(), tone, at: Instant::now() });
    }

    fn mark_dirty(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    fn save_now(&mut self) {
        self.dirty_since = None;
        if let Err(e) = storage::save(&self.library) {
            self.toast(format!("Could not save: {e:#}"), Tone::Bad);
        }
    }

    fn select_game(&mut self, idx: Option<usize>) {
        self.game = idx.filter(|&i| i < self.library.games.len());
        self.profile = self.game.and_then(|g| self.library.games[g].default_index()).unwrap_or(0);
        self.confirm_delete_game = false;
        self.confirm_delete_profile = false;
    }

    fn is_running(&self, game: &str, profile: Option<&str>) -> bool {
        self.running.iter().any(|r| r.game == game && profile.is_none_or(|p| r.profile == p))
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::AddGame => {
                let name = unique_name("New game", self.library.games.iter().map(|g| g.name.as_str()));
                self.library.games.push(Game::new(name));
                self.select_game(Some(self.library.games.len() - 1));
                self.focus_game_name = true;
                self.show_library = false;
                self.mark_dirty();
            }
            Action::SelectGame(i) => {
                self.select_game(Some(i));
                self.show_library = false;
            }
            Action::PlayGame(i) => {
                self.select_game(Some(i));
                self.play();
            }
            Action::ShowLibrary => {
                self.show_library = true;
                self.confirm_delete_game = false;
            }
            Action::DeleteGame => {
                if let Some(g) = self.game {
                    let removed = self.library.games.remove(g);
                    self.select_game(if self.library.games.is_empty() { None } else { Some(g.saturating_sub(1)) });
                    self.toast(format!("Removed {}", removed.name), Tone::Info);
                    self.show_library = true;
                    self.mark_dirty();
                }
            }
            Action::SelectProfile(i) => {
                self.profile = i;
                self.confirm_delete_profile = false;
            }
            Action::NewProfile | Action::DuplicateProfile => {
                let Some(g) = self.game else { return };
                let game = &mut self.library.games[g];
                let mut p = match action {
                    Action::DuplicateProfile => game.profiles.get(self.profile).cloned().unwrap_or_default(),
                    _ => Profile::default(),
                };
                let base = if matches!(action, Action::DuplicateProfile) { format!("{} copy", p.name) } else { "New profile".into() };
                p.name = unique_name(&base, game.profiles.iter().map(|p| p.name.as_str()));
                let at = (self.profile + 1).min(game.profiles.len());
                game.profiles.insert(at, p);
                self.profile = at;
                self.mark_dirty();
            }
            Action::DeleteProfile => {
                let Some(g) = self.game else { return };
                let game = &mut self.library.games[g];
                if game.profiles.len() > 1 {
                    game.profiles.remove(self.profile);
                    self.profile = self.profile.min(game.profiles.len() - 1);
                    self.confirm_delete_profile = false;
                    self.mark_dirty();
                }
            }
            Action::MakeDefault => {
                let Some(g) = self.game else { return };
                let game = &mut self.library.games[g];
                if let Some(p) = game.profiles.get(self.profile) {
                    game.default_profile = p.name.clone();
                    self.mark_dirty();
                }
            }
            Action::Play => self.play(),
            Action::RestorePending => {
                let mut failed = 0;
                for o in std::mem::take(&mut self.pending) {
                    if mods::restore_overlay(&o).is_err() {
                        failed += 1;
                    }
                }
                if failed == 0 {
                    self.toast("Original game files restored", Tone::Good);
                } else {
                    self.toast(format!("{failed} folder(s) could not be restored"), Tone::Bad);
                }
            }
        }
    }

    fn play(&mut self) {
        let Some(g) = self.game else { return };
        let game = &self.library.games[g];
        let Some(profile) = game.profiles.get(self.profile).cloned() else { return };
        let game_name = game.name.clone();
        self.save_now();

        let tx = self.events_tx.clone();
        let ctx = self.ctx.clone();
        let (gn, pn) = (game_name.clone(), profile.name.clone());
        let result = launch::launch(&profile, &self.repos, false, move |info| {
            let _ = tx.send(Event::Exited { game: gn, profile: pn, info });
            ctx.request_repaint();
        });
        match result {
            Ok(()) => {
                self.running.push(Running { game: game_name.clone(), profile: profile.name.clone() });
                self.toast(format!("Starting {game_name} · {}", profile.name), Tone::Good);
            }
            Err(e) => self.toast(format!("{e:#}"), Tone::Bad),
        }
    }

    fn handle_events(&mut self) {
        while let Ok(Event::Exited { game, profile, info }) = self.events_rx.try_recv() {
            if let Some(i) = self.running.iter().position(|r| r.game == game && r.profile == profile) {
                self.running.remove(i);
            }
            if let Some(err) = info.restore_error {
                self.toast(format!("{game} closed, but mod files could not be restored: {err}"), Tone::Bad);
                self.pending = mods::pending_restores(&launch::backups_root());
            } else if let Some(code) = info.code.filter(|c| *c != 0) {
                self.toast(format!("{game} closed with exit code {code}"), Tone::Bad);
            } else if info.code.is_some() {
                self.toast(format!("{game} closed"), Tone::Info);
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.dirty_since.is_some() {
            let _ = storage::save(&self.library);
        }
    }
}

// ---------- small widgets ----------

fn avatar_color(name: &str) -> Color32 {
    let hash = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32 / 360.0;
    egui::ecolor::Hsva::new(hue, 0.6, 0.58, 1.0).into()
}

fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_alphanumeric));
    let a = words.next().and_then(|w| w.chars().next());
    let b = words.next().and_then(|w| w.chars().next());
    match (a, b) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

fn paint_avatar(ui: &Ui, rect: egui::Rect, name: &str) {
    let r = (rect.width() * 0.28) as u8;
    ui.painter().rect_filled(rect, CornerRadius::same(r), avatar_color(name));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials(name),
        FontId::proportional(rect.height() * 0.42),
        Color32::WHITE,
    );
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn section_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(17.0).strong());
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(MUTED).small());
    }
    ui.add_space(4.0);
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).color(MUTED).small().strong());
}

fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add(
        TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(Color32::from_rgb(90, 97, 115)))
            .margin(Margin::symmetric(10, 8))
            .desired_width(width),
    )
}

enum Browse {
    File,
    Folder,
}

/// A text field with a Browse button. Returns true if the value changed.
fn path_field(ui: &mut Ui, value: &mut String, hint: &str, browse: Browse) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let w = ui.available_width() - 96.0;
        changed |= text_field(ui, value, hint, w).changed();
        if ui.add(Button::new("Browse").min_size(vec2(88.0, 34.0))).clicked() {
            let picked = match browse {
                Browse::File => rfd::FileDialog::new()
                    .add_filter("Programs", &["exe", "bat", "cmd", "lnk"])
                    .add_filter("All files", &["*"])
                    .pick_file(),
                Browse::Folder => rfd::FileDialog::new().pick_folder(),
            };
            if let Some(p) = picked {
                *value = p.to_string_lossy().into_owned();
                changed = true;
            }
        }
    });
    changed
}

/// A big clickable card with a title and a short description, used to pick
/// one option out of several. Returns the click response.
fn option_card(ui: &mut Ui, selected: bool, title: &str, subtitle: &str, size: Vec2) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let fill = if selected {
        Color32::from_rgb(36, 31, 66)
    } else if hovered {
        Color32::from_rgb(30, 35, 46)
    } else {
        FIELD
    };
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else if hovered {
        Stroke::new(1.0, Color32::from_rgb(70, 78, 98))
    } else {
        Stroke::new(1.0, BORDER)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(12), fill);
    painter.rect_stroke(rect, CornerRadius::same(12), stroke, egui::StrokeKind::Inside);

    // radio dot in the top-right corner
    let dot = egui::pos2(rect.right() - 20.0, rect.top() + 20.0);
    painter.circle_stroke(dot, 8.0, Stroke::new(1.5, if selected { ACCENT } else { BORDER }));
    if selected {
        painter.circle_filled(dot, 4.5, ACCENT);
    }

    let inner = rect.shrink2(vec2(16.0, 14.0));
    let title_galley = painter.layout(
        ellipsize(title, 40),
        FontId::proportional(16.0),
        if selected { Color32::WHITE } else { TEXT },
        inner.width() - 28.0,
    );
    let title_h = title_galley.size().y;
    painter.galley(inner.min, title_galley, TEXT);
    let sub_galley = painter.layout(subtitle.to_string(), FontId::proportional(12.5), MUTED, inner.width());
    painter.galley(inner.min + vec2(0.0, title_h + 6.0), sub_galley, MUTED);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Width for each card so `n` of them fill a row (2 or 3 per row).
fn card_width(ui: &Ui, n: usize) -> (f32, usize) {
    let full = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let cols = if full > 760.0 { 3 } else { 2 }.min(n.max(1));
    ((full - gap * (cols as f32 - 1.0)) / cols as f32 - 0.5, cols)
}

fn primary_button(text: impl Into<String>, size: Vec2) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE).strong().size(16.0))
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(12))
        .min_size(size)
}

fn ghost_button(text: impl Into<String>, color: Color32) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(color)).frame(false)
}

fn add_tile(ui: &mut Ui, size: Vec2, actions: &mut Vec<Action>) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let color = if hovered { ACCENT } else { BORDER };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(14), if hovered { Color32::from_rgb(26, 24, 44) } else { BG });
    painter.rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.5, color), egui::StrokeKind::Inside);
    painter.circle_stroke(rect.center() - vec2(0.0, 14.0), 22.0, Stroke::new(1.5, color));
    painter.text(rect.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, "＋", FontId::proportional(22.0), if hovered { TEXT } else { MUTED });
    painter.text(rect.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Add game", FontId::proportional(14.0), if hovered { TEXT } else { MUTED });
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        actions.push(Action::AddGame);
    }
}

/// Darken from `top` to `bottom` alpha over the lower part of `rect`.
fn paint_vertical_fade(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    let start = rect.top() + rect.height() * 0.35;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(egui::pos2(rect.left(), start), top);
    mesh.colored_vertex(egui::pos2(rect.right(), start), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A wide banner that fades from the game's colour into the card colour.
fn paint_banner(ui: &Ui, rect: egui::Rect, color: Color32) {
    let painter = ui.painter();
    let dim = |c: u8| (c as f32 * 0.45) as u8;
    let left = Color32::from_rgb(dim(color.r()), dim(color.g()), dim(color.b()));
    let r = 16.0;
    painter.rect_filled(rect, CornerRadius::same(16), CARD);
    painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(r * 3.0, rect.height())), CornerRadius::same(16), left);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(egui::pos2(rect.left() + r, rect.top()), left);
    mesh.colored_vertex(egui::pos2(rect.right() - r, rect.top()), CARD);
    mesh.colored_vertex(egui::pos2(rect.right() - r, rect.bottom()), CARD);
    mesh.colored_vertex(egui::pos2(rect.left() + r, rect.bottom()), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    painter.rect_stroke(rect, CornerRadius::same(16), Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
}

// ---------- panels ----------

impl App {
    fn pending_banner(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        if !self.pending.is_empty() {
            Frame::new()
                .fill(Color32::from_rgb(52, 40, 18))
                .stroke(Stroke::new(1.0, Color32::from_rgb(110, 80, 30)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let folders: Vec<String> =
                            self.pending.iter().map(|o| o.target.to_string_lossy().into_owned()).collect();
                        ui.label(RichText::new("⚠").color(AMBER).size(18.0));
                        ui.label(RichText::new(format!("Mod files are still applied in {}", folders.join(", "))).color(AMBER));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Restore originals").clicked() {
                                actions.push(Action::RestorePending);
                            }
                        });
                    });
                });
            ui.add_space(10.0);
        }

    }

    fn library_view(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(10), ACCENT);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "▶", FontId::proportional(16.0), Color32::WHITE);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Library").size(26.0));
                let n = self.library.games.len();
                ui.label(RichText::new(format!("{n} game{}", if n == 1 { "" } else { "s" })).color(MUTED).small());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.library.games.is_empty() && ui.add(primary_button("＋  Add game", vec2(150.0, 42.0))).clicked() {
                    actions.push(Action::AddGame);
                }
            });
        });
        ui.add_space(16.0);
        self.pending_banner(ui, actions);

        if self.library.games.is_empty() {
            self.empty_state(ui, actions);
            return;
        }

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let gap = 18.0;
            let full = ui.available_width();
            let cols = ((full + gap) / (220.0 + gap)).floor().max(1.0) as usize;
            let w = (full - gap * (cols as f32 - 1.0)) / cols as f32 - 0.5;
            let h = w * 0.62 + 70.0;
            let count = self.library.games.len() + 1; // + the "add game" tile
            for row in 0..count.div_ceil(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for col in 0..cols {
                        let i = row * cols + col;
                        if i < self.library.games.len() {
                            self.game_tile(ui, i, vec2(w, h), actions);
                        } else if i == self.library.games.len() {
                            add_tile(ui, vec2(w, h), actions);
                        }
                    }
                });
                ui.add_space(gap - ui.spacing().item_spacing.y);
            }
        });
    }

    fn game_tile(&self, ui: &mut Ui, i: usize, size: Vec2, actions: &mut Vec<Action>) {
        let game = &self.library.games[i];
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let hovered = resp.hovered();
        let painter = ui.painter();
        let color = avatar_color(&game.name);
        let r = 14u8;

        // lift on hover
        let rect = if hovered { rect.translate(vec2(0.0, -3.0)) } else { rect };
        painter.rect_filled(rect.translate(vec2(0.0, 6.0)), CornerRadius::same(r), Color32::from_black_alpha(70));
        painter.rect_filled(rect, CornerRadius::same(r), CARD);

        // artwork area
        let art = egui::Rect::from_min_size(rect.min, vec2(rect.width(), rect.height() - 70.0));
        painter.rect_filled(art, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, color);
        paint_vertical_fade(painter, art, Color32::from_black_alpha(0), Color32::from_black_alpha(110));
        painter.text(
            art.center(),
            Align2::CENTER_CENTER,
            initials(&game.name),
            FontId::proportional(art.height() * 0.36),
            Color32::from_white_alpha(235),
        );

        let running = self.is_running(&game.name, None);
        if running {
            let badge = egui::Rect::from_min_size(art.min + vec2(10.0, 10.0), vec2(84.0, 24.0));
            painter.rect_filled(badge, CornerRadius::same(12), Color32::from_black_alpha(170));
            painter.text(badge.center(), Align2::CENTER_CENTER, "● Running", FontId::proportional(12.5), GREEN);
        }

        // quick play button on hover
        let play_center = art.center();
        let play_r = 28.0;
        let over_play = resp.hover_pos().is_some_and(|p| p.distance(play_center) <= play_r + 4.0);
        if hovered && !running {
            painter.rect_filled(art, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, Color32::from_black_alpha(90));
            painter.circle_filled(play_center, play_r, if over_play { ACCENT_HOVER } else { ACCENT });
            painter.text(play_center + vec2(2.0, 0.0), Align2::CENTER_CENTER, "▶", FontId::proportional(22.0), Color32::WHITE);
        }

        // name and details
        painter.text(
            egui::pos2(rect.left() + 14.0, art.bottom() + 22.0),
            Align2::LEFT_CENTER,
            ellipsize(&game.name, ((rect.width() - 28.0) / 8.5) as usize),
            FontId::proportional(16.0),
            TEXT,
        );
        let default = game.default().map(|p| p.name.as_str()).unwrap_or("-");
        let n = game.profiles.len();
        painter.text(
            egui::pos2(rect.left() + 14.0, art.bottom() + 46.0),
            Align2::LEFT_CENTER,
            ellipsize(&format!("★ {default} · {n} profile{}", if n == 1 { "" } else { "s" }), ((rect.width() - 28.0) / 6.5) as usize),
            FontId::proportional(12.5),
            MUTED,
        );

        if hovered {
            painter.rect_stroke(rect, CornerRadius::same(r), Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
        }

        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            if over_play && !running {
                actions.push(Action::PlayGame(i));
            } else {
                actions.push(Action::SelectGame(i));
            }
        }
    }

    fn empty_state(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.28);
            let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(18), ACCENT);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "▶", FontId::proportional(28.0), Color32::WHITE);
            ui.add_space(8.0);
            ui.heading("Add your first game");
            ui.label(
                RichText::new(
                    "Give each game as many launch profiles as you like: vanilla, modded,\n\
                     debug builds, an emulator with different settings. Switch with one click.",
                )
                .color(MUTED),
            );
            ui.add_space(16.0);
            if ui.add(primary_button("＋  Add game", vec2(180.0, 46.0))).clicked() {
                actions.push(Action::AddGame);
            }
        });
    }

    fn game_view(&mut self, ui: &mut Ui, g: usize, actions: &mut Vec<Action>) {
        let mut changed = false;

        // ----- header -----
        let game_name_before = self.library.games[g].name.clone();
        let profile_count = self.library.games[g].profiles.len();
        let default_idx = self.library.games[g].default_index();
        let running_game = self.is_running(&game_name_before, None);
        let current_profile_name =
            self.library.games[g].profiles.get(self.profile).map(|p| p.name.clone()).unwrap_or_default();
        let running_profile = self.is_running(&game_name_before, Some(&current_profile_name));

        if ui.add(ghost_button("←  Library", MUTED)).clicked() {
            actions.push(Action::ShowLibrary);
        }
        ui.add_space(4.0);
        let (banner, _) = ui.allocate_exact_size(vec2(ui.available_width(), 150.0), Sense::hover());
        paint_banner(ui, banner, avatar_color(&game_name_before));
        ui.scope_builder(egui::UiBuilder::new().max_rect(banner.shrink2(vec2(28.0, 0.0))), |ui| {
            ui.horizontal_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                paint_avatar(ui, rect, &game_name_before);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(30.0);
                    let game = &mut self.library.games[g];
                    let resp = ui.add(
                        TextEdit::singleline(&mut game.name)
                            .font(FontId::proportional(32.0))
                            .frame(Frame::NONE)
                            .background_color(Color32::TRANSPARENT)
                            .hint_text("Game name")
                            .desired_width(ui.available_width() - 230.0),
                    );
                    if self.focus_game_name {
                        resp.request_focus();
                        self.focus_game_name = false;
                    }
                    if resp.changed() {
                        changed = true;
                    }
                    let status = if running_game {
                        RichText::new("● Running").color(GREEN)
                    } else {
                        let default = default_idx.map(|i| game.profiles[i].name.clone()).unwrap_or_default();
                        RichText::new(format!(
                            "{profile_count} profile{} · default: {default}",
                            if profile_count == 1 { "" } else { "s" }
                        ))
                        .color(Color32::from_rgb(200, 205, 215))
                    };
                    ui.label(status);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if running_profile { "●  Running" } else { "▶  Play" };
                    let btn = primary_button(label, vec2(170.0, 54.0));
                    let btn = if running_profile { btn.fill(Color32::from_rgb(40, 120, 85)) } else { btn };
                    let resp = ui
                        .add_enabled(!running_profile, btn)
                        .on_hover_text(format!("Launch with “{current_profile_name}”"));
                    if resp.clicked() {
                        actions.push(Action::Play);
                    }
                });
            });
        });
        if self.library.games[g].name.trim().is_empty() {
            ui.label(RichText::new("Give the game a name").color(AMBER).small());
        }

        ui.add_space(12.0);

        // ----- profile chips -----
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("PROFILES").color(MUTED).small().strong());
            ui.add_space(4.0);
            let game = &self.library.games[g];
            for (i, p) in game.profiles.iter().enumerate() {
                let star = if Some(i) == default_idx { "★ " } else { "" };
                let dot = if self.is_running(&game.name, Some(&p.name)) { "  ●" } else { "" };
                let selected = i == self.profile;
                let text = RichText::new(format!("{star}{}{dot}", ellipsize(&p.name, 28)))
                    .color(if selected { Color32::WHITE } else { TEXT });
                let resp = ui.add(
                    Button::new(text)
                        .fill(if selected { ACCENT } else { CARD })
                        .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER }))
                        .corner_radius(CornerRadius::same(17))
                        .min_size(vec2(0.0, 34.0)),
                );
                if resp.clicked() {
                    actions.push(Action::SelectProfile(i));
                }
                if resp.double_clicked() {
                    actions.push(Action::Play);
                }
            }
            if ui
                .add(Button::new(RichText::new("＋ New").color(MUTED)).corner_radius(CornerRadius::same(17)).min_size(vec2(0.0, 34.0)))
                .clicked()
            {
                actions.push(Action::NewProfile);
            }
        });

        ui.add_space(8.0);

        // ----- editor -----
        let is_default = Some(self.profile) == default_idx;
        let can_delete = profile_count > 1;
        let names: Vec<String> = self.library.games[g].profiles.iter().map(|p| p.name.clone()).collect();
        let confirm_profile = self.confirm_delete_profile;
        let confirm_game = self.confirm_delete_game;
        let mut set_confirm_profile = None;
        let mut set_confirm_game = None;

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let game = &mut self.library.games[g];
            let was_default_name = game.default_profile.clone();
            let Some(p) = game.profiles.get_mut(self.profile) else { return };
            let old_name = p.name.clone();

            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| section_title(ui, "Launch", "What to start and how"));
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if can_delete {
                            if confirm_profile {
                                if ui.add(Button::new(RichText::new("Really delete?").color(Color32::WHITE)).fill(RED)).clicked() {
                                    actions.push(Action::DeleteProfile);
                                }
                            } else if ui.add(ghost_button("Delete", RED)).clicked() {
                                set_confirm_profile = Some(true);
                            }
                        }
                        if ui.add(ghost_button("Duplicate", MUTED)).clicked() {
                            actions.push(Action::DuplicateProfile);
                        }
                        if is_default {
                            ui.label(RichText::new("★ Default").color(AMBER));
                        } else if ui.add(ghost_button("☆ Make default", MUTED)).clicked() {
                            actions.push(Action::MakeDefault);
                        }
                    });
                });

                let full = ui.available_width();
                field_label(ui, "PROFILE NAME");
                changed |= text_field(ui, &mut p.name, "e.g. Modded, Vanilla, Debug", full).changed();
                if names.iter().enumerate().any(|(i, n)| i != self.profile && *n == p.name) {
                    ui.label(RichText::new("Another profile already has this name").color(AMBER).small());
                }
                field_label(ui, "HOW TO START IT");
                ui.horizontal(|ui| {
                    let (w, _) = card_width(ui, 2);
                    let w = w.max(200.0).min((ui.available_width() - 8.0) / 2.0);
                    for (kind, title, sub) in [
                        (LaunchKind::Program, "Program or URL", "Run a game .exe, mod loader, emulator, or a Steam/Epic link"),
                        (LaunchKind::Repo, "Repo method", "Pick your game folder and a repo from the list, then follow its instructions"),
                    ] {
                        if option_card(ui, p.kind == kind, title, sub, vec2(w, 80.0)).clicked() && p.kind != kind {
                            p.kind = kind;
                            changed = true;
                        }
                    }
                });

                match p.kind {
                    LaunchKind::Program => {
                        field_label(ui, "PROGRAM OR URL");
                        changed |= path_field(ui, &mut p.executable, r"C:\Games\MyGame\game.exe  or  steam://rungameid/…", Browse::File);
                        field_label(ui, "ARGUMENTS");
                        changed |= text_field(ui, &mut p.arguments, r#"-windowed -config "{exe_dir}\mods.ini""#, full).changed();
                        field_label(ui, "WORKING FOLDER");
                        changed |= path_field(ui, &mut p.working_dir, "Leave empty to use the program's folder", Browse::Folder);
                        ui.label(
                            RichText::new("Placeholders {exe_dir} and {profile}, and variables like %APPDATA%, work in every field.")
                                .color(MUTED)
                                .small(),
                        );
                    }
                    LaunchKind::Repo => {
                        field_label(ui, "GAME FOLDER");
                        changed |= path_field(ui, &mut p.game_dir, r"C:\Games\MyGame", Browse::Folder);

                        field_label(ui, "CHOOSE A REPO");
                        if !p.repo_id.is_empty() && !self.repos.iter().any(|r| r.id == p.repo_id) {
                            ui.label(
                                RichText::new(format!("This profile used '{}', which is no longer in the list. Pick another.", p.repo_id))
                                    .color(AMBER)
                                    .small(),
                            );
                        }
                        let (w, cols) = card_width(ui, self.repos.len());
                        for row in self.repos.chunks(cols) {
                            ui.horizontal(|ui| {
                                for r in row {
                                    let sub = r.method.describe();
                                    if option_card(ui, p.repo_id == r.id, &r.name, &sub, vec2(w, 82.0)).clicked()
                                        && p.repo_id != r.id
                                    {
                                        p.repo_id = r.id.clone();
                                        changed = true;
                                    }
                                }
                            });
                        }

                        if let Some(repo) = self.repos.iter().find(|r| r.id == p.repo_id) {
                            field_label(ui, "REPO FOLDER (WHERE YOU DOWNLOADED IT)");
                            changed |= path_field(ui, &mut p.repo_dir, r"C:\Tools\repo-name", Browse::Folder);

                            ui.add_space(6.0);
                            Frame::new()
                                .fill(FIELD)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(Margin::same(14))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&repo.name).strong().size(16.0));
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if !repo.url.is_empty() {
                                                ui.hyperlink_to("Open on GitHub ↗", &repo.url);
                                            }
                                        });
                                    });
                                    ui.label(RichText::new(repo.method.describe()).color(ACCENT_HOVER).small());
                                    ui.add_space(6.0);
                                    ui.label(RichText::new("INSTRUCTIONS").color(MUTED).small().strong());
                                    if repo.instructions.trim().is_empty() {
                                        ui.label(RichText::new("No instructions for this repo yet.").color(MUTED));
                                    } else {
                                        ui.label(&repo.instructions);
                                    }
                                });
                        } else if self.repos.is_empty() {
                            ui.label(RichText::new("The repo list is empty. Add repos to repos.json.").color(AMBER).small());
                        }
                    }
                }
            });

            ui.add_space(12.0);
            card(ui, |ui| {
                section_title(ui, "Environment variables", "Set only for this profile");
                let mut remove = None;
                for (i, var) in p.env.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 50.0) / 2.0;
                        changed |= text_field(ui, &mut var.key, "NAME", w * 0.8).changed();
                        ui.label(RichText::new("=").color(MUTED));
                        changed |= text_field(ui, &mut var.value, "value", w * 1.2 - 16.0).changed();
                        if ui.add(ghost_button("✕", MUTED)).on_hover_text("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    p.env.remove(i);
                    changed = true;
                }
                if ui.button("＋ Add variable").clicked() {
                    p.env.push(EnvVar::default());
                    changed = true;
                }
            });

            ui.add_space(12.0);
            card(ui, |ui| {
                section_title(
                    ui,
                    "Mods",
                    "Copy a folder of mod files into the game before it starts. Originals are backed up.",
                );
                field_label(ui, "MOD FOLDER");
                changed |= path_field(ui, &mut p.mod_source, "Leave empty for no mods", Browse::Folder);
                field_label(ui, "COPY MODS INTO");
                changed |= path_field(ui, &mut p.mod_target, "Leave empty to use the working folder", Browse::Folder);
                ui.add_space(4.0);
                changed |= ui.checkbox(&mut p.restore_mods, "Put the original files back when the game closes").changed();
            });

            ui.add_space(12.0);
            card(ui, |ui| {
                section_title(ui, "Notes", "");
                changed |= ui
                    .add(
                        TextEdit::multiline(&mut p.notes)
                            .hint_text("Anything worth remembering about this setup")
                            .margin(Margin::symmetric(10, 8))
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
            });

            // keep the default pointing at this profile if it was renamed
            if p.name != old_name && was_default_name == old_name {
                game.default_profile = game.profiles[self.profile].name.clone();
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if confirm_game {
                    ui.label(RichText::new(format!("Remove {} and all its profiles?", game.name)).color(RED));
                    if ui.add(Button::new(RichText::new("Remove game").color(Color32::WHITE)).fill(RED)).clicked() {
                        actions.push(Action::DeleteGame);
                    }
                    if ui.button("Cancel").clicked() {
                        set_confirm_game = Some(false);
                    }
                } else if ui.add(ghost_button("Remove this game…", RED)).clicked() {
                    set_confirm_game = Some(true);
                }
            });
            ui.add_space(20.0);
        });

        if let Some(v) = set_confirm_profile {
            self.confirm_delete_profile = v;
        }
        if let Some(v) = set_confirm_game {
            self.confirm_delete_game = v;
        }
        if changed {
            self.mark_dirty();
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(t) = &self.toast {
                let color = match t.tone {
                    Tone::Info => MUTED,
                    Tone::Good => GREEN,
                    Tone::Bad => RED,
                };
                ui.label(RichText::new(&t.text).color(color));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = if self.dirty_since.is_some() { "Saving…" } else { "All changes saved" };
                ui.label(RichText::new(text).color(MUTED).small());
                if !self.running.is_empty() {
                    ui.label(RichText::new(format!("● {} running", self.running.len())).color(GREEN).small());
                }
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.handle_events();

        // Errors stay up longer than other messages.
        if let Some(t) = &self.toast {
            let ttl = if t.tone == Tone::Bad { 12.0 } else { 5.0 };
            if t.at.elapsed().as_secs_f32() > ttl {
                self.toast = None;
            }
        }

        let mut actions = vec![];

        egui::Panel::bottom("status")
            .exact_size(34.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(24, 6)))
            .show(ui, |ui| self.status_bar(ui));

        egui::CentralPanel::no_frame()
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(28, 22)))
            .show(ui, |ui| match self.game {
                Some(g) if !self.show_library => {
                    self.pending_banner(ui, &mut actions);
                    self.game_view(ui, g, &mut actions)
                }
                _ => self.library_view(ui, &mut actions),
            });

        for a in actions {
            self.apply(a);
        }

        // Save shortly after the last edit.
        if let Some(since) = self.dirty_since {
            let wait = Duration::from_millis(400);
            if since.elapsed() >= wait {
                self.save_now();
            } else {
                ui.ctx().request_repaint_after(wait - since.elapsed());
            }
        }
        if self.toast.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
    }
}
