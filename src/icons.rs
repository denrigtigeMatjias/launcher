//! Icons, drawn with the Fluent icon font that ships with Windows.
//!
//! Windows 11 has `Segoe Fluent Icons`, Windows 10 has `Segoe MDL2 Assets`,
//! and the codepoints used here are the same in both. The font is read from
//! the system at startup, so nothing is bundled. If neither font is there
//! every icon falls back to a plain character instead of an empty box.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Align, Color32, FontData, FontFamily, FontId, FontInsert, FontPriority, InsertFontFamily,
    TextFormat, text::LayoutJob,
};

const FAMILY: &str = "fluent";
static LOADED: AtomicBool = AtomicBool::new(false);

/// One icon: a glyph from the icon font, and what to show without it.
#[derive(Clone, Copy)]
pub struct Icon {
    glyph: &'static str,
    plain: &'static str,
}

/// No icon, for buttons that are text only.
pub const NONE: Icon = Icon { glyph: "", plain: "" };
pub const GAME: Icon = Icon { glyph: "\u{E7FC}", plain: "#" };
pub const PLAY: Icon = Icon { glyph: "\u{E768}", plain: ">" };
pub const ADD: Icon = Icon { glyph: "\u{E710}", plain: "+" };
pub const BACK: Icon = Icon { glyph: "\u{E72B}", plain: "<" };
pub const STAR: Icon = Icon { glyph: "\u{E734}", plain: "*" };
pub const STAR_FILL: Icon = Icon { glyph: "\u{E735}", plain: "*" };
pub const DUPLICATE: Icon = Icon { glyph: "\u{E8C8}", plain: "" };
pub const DELETE: Icon = Icon { glyph: "\u{E74D}", plain: "" };
pub const OPEN_IN_NEW: Icon = Icon { glyph: "\u{E8A7}", plain: "^" };
pub const DOT: Icon = Icon { glyph: "\u{E91F}", plain: "o" };
pub const REFRESH: Icon = Icon { glyph: "\u{E72C}", plain: "" };
pub const FOLDER: Icon = Icon { glyph: "\u{E8B7}", plain: "" };

/// Read the icon font from the system and add it as its own font family.
///
/// Call once, before the first frame.
pub fn install(ctx: &egui::Context) {
    let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    for file in ["SegoeIcons.ttf", "segmdl2.ttf"] {
        let path = std::path::Path::new(&windows).join("Fonts").join(file);
        let Ok(bytes) = std::fs::read(&path) else { continue };
        ctx.add_font(FontInsert::new(
            FAMILY,
            FontData::from_owned(bytes),
            vec![InsertFontFamily {
                family: FontFamily::Name(FAMILY.into()),
                priority: FontPriority::Highest,
            }],
        ));
        LOADED.store(true, Ordering::Relaxed);
        return;
    }
}

fn loaded() -> bool {
    LOADED.load(Ordering::Relaxed)
}

/// The font to draw icons in at `size`. Icons are square and centred on
/// their own box, so this is also the height they take up.
pub fn font(size: f32) -> FontId {
    if loaded() {
        FontId::new(size, FontFamily::Name(FAMILY.into()))
    } else {
        FontId::proportional(size)
    }
}

impl Icon {
    pub fn as_str(self) -> &'static str {
        if loaded() { self.glyph } else { self.plain }
    }

    /// The icon followed by text, both centred on the same line. Use for
    /// button and chip labels, where the two must not drift apart.
    pub fn with_text(self, size: f32, text: &str, text_font: FontId, color: Color32) -> LayoutJob {
        let mut job = LayoutJob::default();
        part(&mut job, self.as_str(), font(size), color, 0.0);
        let gap = if self.as_str().is_empty() { 0.0 } else { size * 0.5 };
        part(&mut job, text, text_font, color, gap);
        job
    }
}

/// Add one run of text or icon to `job`, on the same centre line as the rest.
///
/// Icons and text have different heights, so a label that mixes them has to
/// be one laid-out line: side by side as separate widgets they drift apart.
pub fn part(job: &mut LayoutJob, text: &str, font: FontId, color: Color32, gap: f32) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        gap,
        TextFormat { font_id: font, color, valign: Align::Center, ..Default::default() },
    );
}
