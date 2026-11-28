//! The main window's chrome painted to IDM 6.42's pixels (notes/ui-spec/main-window.md,
//! notes/ui-spec/menus.md): menu bar, popup menus, toolbar, Categories tree, shell icons.
//!
//! Our client area also holds the menu bar (IDM's is non-client), so every main-window
//! spec y is shifted by `MENU_H` = 20. Main-window spec "baseline" is origin + ascent (the row
//! under the glyphs); `winui::text_font` takes the last ink row, i.e. spec baseline − 1.

use egui::{Color32, Pos2, Rect, Stroke, Ui, pos2, vec2};

use crate::icons::Icon;
use crate::scrollbar;
use crate::winui::{self, text_font, text_width_font};

mod categories;
mod icons;
mod menu_bar;
mod popup;
mod toolbar;

pub use self::categories::*;
pub use self::icons::*;
pub use self::menu_bar::*;
pub use self::popup::*;
pub use self::toolbar::*;

/// Height of our menu bar band incl. its #f0f0f0 bottom line (IDM: non-client y −20…−1).
pub const MENU_H: f32 = 20.0;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
use crate::theme::themed;
// The active theme's colours (crate::palette, #45).
themed! {
    pub WINDOW_BG => window;
    MENUBAR_BG => menubar;
    /// The bar's last row (light: #f2f2f2).
    MENUBAR_BOTTOM => menubar_bottom;
    /// The open menu's item. Light: #f9f9f9, the popup's own background (captured 2026-10-30).
    MENUBAR_OPEN => menubar_open;
    /// An item under the pointer, no menu open (captured 2026-10-30): dark #505050 from 1 px
    /// below the bar's top; light #f5f5f5, 1 px inside the bar's top and bottom.
    MENUBAR_HOT => menubar_hot;
    MENUBAR_LINE => menubar_line;
    POPUP_BG => menu;
    // The native menu window's frame (Windows draws menus in their own window), and every
    // in-window popup's (popup_surface).
    POPUP_BORDER => menu_border;
    POPUP_SEP => menu_sep;
    /// Light: #f0f0f0, rounded, captured 2026-10-30.
    POPUP_HOVER => menu_hot;
    POPUP_ITEM => menu_item;
    TEXT => menu_text;
    /// Bullets and submenu arrows (light: dark cores #1a1a1a / #282828).
    GLYPH => menu_glyph;
    TEXT_DISABLED_MENU => menu_text_disabled;
    TB_DISABLED => toolbar_text_disabled;
    /// A drop-down's arrow (dark: light on the dark band, where #282828 could not be seen).
    TB_ARROW => toolbar_arrow;
    CAT_FRAME => tree_frame;
    TREE_BG => tree;
    TREE_LINES => tree_lines;
    /// The tree's chevrons (dark look).
    TREE_EXPANDER => tree_expander;
}

use crate::winui::{fill, px};

#[cfg(test)]
mod tests;
