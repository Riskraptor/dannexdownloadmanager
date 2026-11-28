//! Windows dark-dialog controls, painted to IDM 6.42's measured pixels
//! (notes/ui-spec/dialogs.md "Dark dialog control styles"). Coordinates are client pixels;
//! "baseline" is the last pixel row of cap glyphs, as in the spec.

use egui::{
    Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};
use unicode_segmentation::UnicodeSegmentation;

mod a11y;
mod bars;
mod buttons;
mod checks;
mod combo;
mod edit;
mod fonts;
mod keys;
mod paint;
mod paragraph;
mod pick;
mod rows;
mod surface;
mod tabs;
mod text;
mod wheel;

pub use self::a11y::*;
pub use self::bars::*;
pub use self::buttons::*;
pub use self::checks::*;
pub use self::combo::*;
pub use self::edit::*;
pub use self::fonts::*;
pub use self::keys::*;
pub use self::paint::*;
pub use self::paragraph::*;
pub use self::pick::*;
pub use self::rows::*;
pub use self::surface::*;
pub use self::tabs::*;
pub use self::text::*;
pub use self::wheel::*;

// Every colour comes from the active theme (crate::palette, #45): these names are its roles.
crate::theme::themed! {
    /// Dialog background (light: light-dialogs.md).
    pub BG => window;
    pub TEXT => text;
    /// A group box's frame (it was #dcdcdc in dark too: brighter than the separators in it).
    pub GROUP_LINE => group_line;
    /// A list's column headers (the main list's): every list and table has the same (they had
    /// four: none, #f3f3f3, white).
    pub LIST_HEADER => header;
    /// Disabled text, the one grey of every control and label (there were five).
    pub TEXT_DISABLED => text_disabled;
    /// A greyed text box's text.
    pub EDIT_TEXT_DISABLED => text_disabled;
    /// A selection's fill (rows, a chosen day, selected text): Windows' accent by default.
    pub SELECTION => selection;
    /// Text on the selection.
    pub SELECTION_TEXT => selection_text;
}

// ---- light dialogs (Speed Limiter, Scheduler, Options) ------------------------------------
// notes/ui-spec/scheduler-and-misc.md "Light dialog control styles".

crate::theme::themed! {
    /// The "light dialogs" (Options, Scheduler, File Properties…): IDM draws them light in both
    /// modes; here they follow the mode, dark the dark dialogs' colours.
    pub LIGHT_BG => window;
    pub LIGHT_TEXT => text;
    pub LIGHT_DISABLED => text_disabled;
    /// A text box's or a list's inside.
    pub FIELD => field;
    /// A list's or a box's outline.
    pub FIELD_EDGE => list_frame;
    /// Grid lines between a list's rows, and header dividers.
    pub GRID => grid;
    pub HEADER_LINE => header_line;
    /// Etched separators: the shadow line and the highlight beside it.
    pub ETCH_SHADOW => etch_shadow;
    pub ETCH_LIGHT => etch_light;
    /// A selected row in a focused list ([`row_list`]; the download list's colours), white
    /// text on it.
    pub ROW_SELECTED => selection;
    /// Text that opens something ([`link`]).
    pub LINK => link;
    /// A selected row in a list without the focus.
    pub SELECTED_INACTIVE => selection_inactive;
    /// Error text.
    pub ERROR => error;
    /// Secondary text (notes, sizes, versions).
    pub MUTED => muted;
    /// Success text (a matching checksum).
    pub SUCCESS => success;
}

crate::theme::themed! {
    /// Tab page body.
    pub LIGHT_PAGE => tab_page;
    /// An unselected tab, and the page's shadow.
    pub TAB_UNSELECTED => tab;
    pub TAB_HOT => tab_hot;
    CHEVRON => chevron;
    CHEVRON_DISABLED => chevron_disabled;
}

/// A title bar we draw ourselves, with Windows 11's caption buttons (#63). Used on Windows,
/// where the progress window's frame is ours (win32::progress_frame); Linux and macOS keep the
/// system's decorations, whose title bars take no buttons of ours (and Wayland places them
/// itself), so the hide button sits in the tab row there.
pub mod caption;

#[cfg(test)]
mod tests;
