//! The icons the main window draws: the themed and classic ones (`IconKey`), the shell's file icons
//! (`ShellIcons`), and how a picture is painted.

use super::*;

// ---- shell icons ---------------------------------------------------------------------------

/// What a 16 × 16 icon shows: a file type (by extension) or a folder.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum IconKey {
    Ext(String),
    /// 32 × 32 (Download File Info).
    ExtLarge(String),
    /// A FamFamFam Silk icon (CC BY 2.5, assets/icons/silk/), by name.
    Silk(&'static str),
    /// The theme icon set's icon for a need (`crate::themeicons`), else this Silk icon.
    Themed(&'static str, &'static str),
}

/// The small icons drawn by key: Silk's (the Categories tree, the Scheduler's queues) and the
/// file types', which fileicons.rs draws (the system's own).
#[derive(Default)]
pub struct ShellIcons;

impl ShellIcons {
    /// Paint the icon for `key` with its top-left at English point `min` (16 × 16, the large ones
    /// 32): placed where that is on screen, not mirrored. A generic page if unavailable.
    pub fn paint(&mut self, ui: &Ui, key: IconKey, min: Pos2) {
        self.paint_in(ui, ui.painter(), key, min);
    }

    /// `paint` through `painter`, so the icon keeps to its clip (a list scrolled sideways must
    /// not draw its icons over the pane beside it).
    pub fn paint_in(&mut self, ui: &Ui, painter: &egui::Painter, key: IconKey, min: Pos2) {
        let side = if matches!(key, IconKey::ExtLarge(_)) {
            32.0
        } else {
            16.0
        };
        let rc = winui::rect(Rect::from_min_size(min, vec2(side, side)));
        match &key {
            IconKey::Silk(name) => {
                if let Some(src) = silk(name) {
                    paint_image(ui, painter, src, rc);
                }
            }
            IconKey::Themed(need, name) => {
                if !crate::themeicons::paint(ui, painter, need, rc, true)
                    && let Some(src) = silk(name)
                {
                    paint_image(ui, painter, src, rc);
                }
            }
            IconKey::Ext(e) | IconKey::ExtLarge(e) => {
                // The key holds the extension alone (empty when there is none).
                let name = if e.is_empty() {
                    "file".to_string()
                } else {
                    format!("x.{e}")
                };
                crate::fileicons::paint(ui, painter, crate::fileicons::Kind::File(&name), rc);
            }
        }
    }
}

/// The embedded Silk icons (XP-era look, like IDM's tree icons).
pub(super) fn silk(name: &str) -> Option<egui::ImageSource<'static>> {
    macro_rules! icons {
        ($($n:literal),*) => {
            match name {
                $($n => Some(egui::include_image!(concat!("../../assets/icons/silk/", $n, ".png"))),)*
                _ => None,
            }
        };
    }
    icons!(
        "folder",
        "folder_go",
        "folder_page",
        "page_white_zip",
        "page_white",
        "music",
        "application_xp",
        "film",
        "tick",
        "page_white_stack",
        "arrow_down",
        "arrow_up",
        "cross",
        "folder_link",
        "folder_delete"
    )
}

/// An image source drawn in `rc` through `painter` (its clip), pixel-sharp.
pub fn paint_image(ui: &Ui, painter: &egui::Painter, src: egui::ImageSource<'static>, rc: Rect) {
    let img = egui::Image::new(src).texture_options(egui::TextureOptions::NEAREST);
    if let Ok(egui::load::TexturePoll::Ready { texture }) = img.load_for_size(ui.ctx(), rc.size()) {
        painter.image(
            texture.id,
            rc,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}
