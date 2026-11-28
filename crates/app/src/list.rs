//! The download list, painted to IDM 6.42's pixels (notes/ui-spec/main-window.md "List view"),
//! with IDM's header behaviour: drag a header to reorder columns, drag a border to resize, click
//! to sort, right-click to choose columns.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Ui, pos2};

use crate::chrome::{IconKey, ShellIcons};
use crate::scrollbar;
use crate::selection::Pick;
use crate::theme::themed;
// The list is laid out in English pixels; `fill` makes each one where it is on screen (the
// first column at the right in a right-to-left interface).
use crate::winui::{self, fill_l as fill, text_font};

/// The column headers' height (the theme's size; 24 in the classic look).
pub fn header_h() -> f32 {
    crate::theme::metrics().list_header
}

/// View → Font: how large the rows write their text, from the theme's own size (Normal). The
/// header keeps the theme's; the rows grow with the text, as a list view's do with its font.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontSize {
    Small,
    #[default]
    Normal,
    Large,
    ExtraLarge,
}

impl FontSize {
    pub const ALL: [FontSize; 4] = [
        FontSize::Small,
        FontSize::Normal,
        FontSize::Large,
        FontSize::ExtraLarge,
    ];

    /// Its name in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            FontSize::Small => "small",
            FontSize::Normal => "normal",
            FontSize::Large => "large",
            FontSize::ExtraLarge => "extra-large",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }

    /// Points added to the theme's text size (11 in the classic look: 10, 11, 13, 15).
    pub fn grow(self) -> f32 {
        match self {
            FontSize::Small => -1.0,
            FontSize::Normal => 0.0,
            FontSize::Large => 2.0,
            FontSize::ExtraLarge => 4.0,
        }
    }

    /// Points added to the theme's row height. Never fewer: a file's 16-point icon and the grid
    /// line under it take the classic row whole (a list view's rows don't get lower than their
    /// icons either).
    pub fn row_grow(self) -> f32 {
        match self {
            FontSize::Small | FontSize::Normal => 0.0,
            FontSize::Large => 3.0,
            FontSize::ExtraLarge => 6.0,
        }
    }

    /// How far the text's baseline moves down in its row: the larger text stays in the middle
    /// of the taller row, the smaller one in the middle of the same row.
    fn baseline_shift(self) -> f32 {
        match self {
            FontSize::Small => -1.0,
            FontSize::Normal => 0.0,
            FontSize::Large => 2.0,
            FontSize::ExtraLarge => 4.0,
        }
    }

    /// How far the icons move down in the taller row (to its middle).
    fn icon_shift(self) -> f32 {
        (self.row_grow() / 2.0).floor()
    }
}

/// Read by its name; one this build doesn't know (a later DDM's size) is Normal, rather than
/// the settings it is saved with failing to read.
impl<'de> serde::Deserialize<'de> for FontSize {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
        Ok(saved
            .as_str()
            .and_then(FontSize::from_key)
            .unwrap_or_default())
    }
}

// The active theme's colours (crate::palette, #45).
themed! {
    /// The list's border (it was #828790 in dark too): the lists' and tables' one edge.
    FRAME => list_frame;
    /// Inner ring and row background.
    BG => list;
    HEADER_BG => header;
    HEADER_SEP => header_sep;
    /// Sort indicator (an addition: IDM's header shows none).
    SORTED_BG => sorted;
    SORT_ARROW => sort_arrow;
    /// Light: the one grid colour, 1 px lines (light-main.md).
    GRID_H => list_grid;
    GRID_V_RIGHT => list_grid;
    /// Selected row (list focused): Windows' accent, as every list's (one blue across DDM), with
    /// white text and a dotted focus outline in its inverse, #ff9933.
    SELECTED => selection;
    TEXT => text;
    /// A filtered column's funnel.
    FILTER_ON => accent;
}

/// Columns, by their fixed data slot in [`Row::cells`]: IDM's eight, then the user's additions:
/// "Added" (2026-10-30: when the download was added; sorts in order of addition), "Source"
/// (D-014: a Web or Torrent chip, first by default), and a torrent's "Peers" and "Share ratio"
/// (#357: how many peers it has connected and what it has given back). "Upload speed" (#357: what
/// it sends) is the last slot: a saved layout is indexed by slot, so it must not take a slot an
/// older build already meant something else by.
pub const COLS: usize = 13;
pub const TITLES: [&str; COLS] = [
    "File Name",
    // i18n: column title: one letter for Queue (the column is narrow)
    "Q",
    // i18n: noun: the file's size
    "Size",
    // i18n: noun: the download's state or progress
    "Status",
    "Time left",
    "Transfer rate",
    "Last Try Date",
    // i18n: noun: the download's description
    "Description",
    // i18n: past participle: when the download was added
    "Added",
    // i18n: noun: where the download comes from (Web or Torrent)
    "Source",
    // i18n: column title: how many peers a torrent is connected to now (seeders and leechers)
    "Peers",
    // i18n: column title: what a torrent has sent to peers against what it took
    "Share ratio",
    // i18n: column title: what a torrent sends to peers, bytes per second
    "Upload speed",
];
/// View → Arrange files' orders that have no column of their own: by the folder a download is
/// saved in, by its Referer, by the web page it belongs to. Sort slots after the columns' (a
/// layout saved with one reads as unsorted in a DDM from before them).
pub const BY_SAVE_PATH: usize = 100;
pub const BY_REFERER: usize = 101;
pub const BY_PAGE: usize = 102;

/// A sort slot the list knows: a column's, or one of Arrange files' own.
pub fn sort_slot(col: usize) -> bool {
    col < COLS || (BY_SAVE_PATH..=BY_PAGE).contains(&col)
}

/// The "Added" column's data slot.
pub const ADDED: usize = 8;
/// The "Source" column's data slot: its cell is "Web" or "Torrent", painted as a chip.
pub const SOURCE: usize = 9;
/// The "Peers" column's data slot (a torrent's connected peers; blank for anything else).
pub const PEERS: usize = 10;
/// The "Share ratio" column's data slot (a torrent's sent over taken; blank otherwise).
pub const RATIO: usize = 11;
/// The "Upload speed" column's data slot (what a torrent sends; blank for anything else). The
/// last slot, so a layout saved before it existed still means what it meant.
pub const UPLOAD: usize = 12;

/// IDM's default column widths (and ours for Added, room for a 12-hour clock's
/// `Oct 28 9:05:01 AM 2026`, Source, Peers, Share ratio and Upload speed).
pub const DEFAULT_WIDTHS: [f32; COLS] = [
    150.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 200.0, 145.0, 62.0, 64.0, 92.0, 100.0,
];
/// Default display order: Source first, then IDM's, a torrent's three columns after its rate, and
/// Added after Last Try Date.
pub const DEFAULT_ORDER: [usize; COLS] =
    [SOURCE, 0, 1, 2, 3, 4, 5, UPLOAD, PEERS, RATIO, 6, ADDED, 7];
/// The columns shown by default: IDM's own set (File Name, Q, Size, Status, Time left, Transfer
/// rate, Last Try Date, Description). Added and Source are offered (the header's menu, the
/// Columns window), off; the torrent columns follow the view ([`DEFAULT_AUTO`]).
pub const DEFAULT_VISIBLE: [bool; COLS] = [
    true, true, true, true, true, true, true, true, false, false, false, false, false,
];
/// The columns that follow the view by default: Peers, Share ratio and Upload speed show in the
/// Torrents category's view, empty as they are for every web download elsewhere, until the
/// person shows or hides one (from then on it is theirs, in every view).
pub const DEFAULT_AUTO: [bool; COLS] = [
    false, false, false, false, false, false, false, false, false, false, true, true, true,
];

const MIN_WIDTH: f32 = 16.0;

/// The narrowest column `col` may be dragged to (its divider snaps there): enough for its icon
/// (Q), its chip's start (Source), or the start of a title and "..."; never unreadable or gone.
pub fn min_width(col: usize) -> f32 {
    match col {
        1 => DEFAULT_WIDTHS[1],
        0 => 60.0,
        SOURCE => 40.0,
        _ => 36.0,
    }
    .max(MIN_WIDTH)
}

/// What a header cell `width` wide draws besides its title (`title_w` wide): its filter funnel
/// and its sort arrow, each where there is room. The funnel shows while the pointer is over the
/// header (`hovered`) and always on a filtered column or one whose filter box is open
/// (`filtered`); the arrow on the sorted column. The title has the room first: what gives way
/// when it lacks some is a funnel only hovered, then the arrow, then a filtered column's funnel
/// (its header stays tinted); only then is the title cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderParts {
    pub funnel: bool,
    pub arrow: bool,
}

/// Room a funnel or the sort arrow takes in a header, and a header's padding (6 before the
/// title, 4 after the last part).
const PART_W: f32 = 14.0;
const HEADER_PAD: f32 = 10.0;

pub fn header_parts(
    width: f32,
    title_w: f32,
    sorted: bool,
    filtered: bool,
    hovered: bool,
) -> HeaderParts {
    let room = width - HEADER_PAD;
    let need = |p: HeaderParts| {
        title_w + if p.funnel { PART_W } else { 0.0 } + if p.arrow { PART_W } else { 0.0 }
    };
    let mut p = HeaderParts {
        funnel: filtered || hovered,
        arrow: sorted,
    };
    if need(p) > room && p.funnel && !filtered {
        p.funnel = false;
    }
    if need(p) > room && p.arrow {
        p.arrow = false;
    }
    if need(p) > room && p.funnel {
        p.funnel = false;
    }
    p
}
/// How far from a separator a press grabs it for resizing.
const GRIP: f32 = 4.0;
/// Pointer travel before a header press becomes a column drag.
const DRAG_START: f32 = 5.0;

/// A column filter (beyond IDM, user 2026-10-29): a few fixed values to keep, or text the cell
/// must contain (case-insensitive).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Filter {
    Values(Vec<&'static str>),
    Text(String),
}

/// The values a column filters by, for columns that have a few (None: a text search).
pub fn filter_values(col: usize) -> Option<&'static [&'static str]> {
    match col {
        // i18n: where a download comes from: Web (web or FTP server) or Torrent (BitTorrent)
        SOURCE => Some(&["Web", "Torrent"]),
        1 => Some(&["In a queue", "Not in a queue"]),
        // i18n: a download's status (state it is in, not a command)
        3 => Some(&["Downloading", "Stopped", "Waiting", "Complete", "Error"]),
        _ => None,
    }
}

/// The value a row has for a value-list column.
fn row_value(row: &Row, col: usize) -> &str {
    match col {
        1 if row.queued => "In a queue",
        1 => "Not in a queue",
        3 => row.status_group,
        c => &row.cells[c],
    }
}

/// Does the row pass every column filter? A hidden column's filter is kept but doesn't filter
/// (nothing on screen would say why rows are missing).
pub fn matches(st: &ListState, row: &Row) -> bool {
    st.filters.iter().enumerate().all(|(c, f)| match f {
        _ if !st.is_shown(c) => true,
        None => true,
        Some(Filter::Values(keep)) => keep.contains(&row_value(row, c)),
        Some(Filter::Text(t)) => row.cells[c].to_lowercase().contains(&t.to_lowercase()),
    })
}

#[derive(Clone, Copy)]
enum HeaderDrag {
    /// Resizing column `col` (data slot) from `width` at pointer `x0`.
    Resize {
        col: usize,
        width: f32,
        x0: f32,
    },
    /// A press on column `col` at `x0`; becomes a move once it travels.
    Press {
        col: usize,
        x0: f32,
    },
    Move {
        col: usize,
        grab: f32,
    },
}

pub struct ListState {
    /// Display order: data slots, left to right.
    pub order: Vec<usize>,
    pub widths: [f32; COLS],
    /// Shown, for a column the person chose for ([`ListState::auto`] off).
    pub visible: [bool; COLS],
    /// The column follows the view (the torrent columns, [`DEFAULT_AUTO`]): shown in the
    /// Torrents category's view only, until the person shows or hides it.
    pub auto: [bool; COLS],
    /// The Torrents category's view is shown (set by the main window before it draws the list).
    pub torrent_view: bool,
    /// Sorted column (data slot) and ascending?
    pub sort: Option<(usize, bool)>,
    /// Vertical scroll in px: whole rows from the keys, Find and the scroll bar (as a Win32
    /// list view scrolls), anywhere between them while the wheel glides it (`glide`).
    pub scroll: f32,
    /// The wheel's glide towards where its notches take the list.
    glide: winui::WheelGlide,
    /// Horizontal scroll in px.
    pub hscroll: f32,
    drag: Option<HeaderDrag>,
    /// The list has the keys: the selection in its colour (grey while it hasn't).
    focused: bool,
    /// Bring this row into view on the next frame (Find).
    pub scroll_to: Option<usize>,
    /// Column filters, by data slot (kept for the session).
    pub filters: Vec<Option<Filter>>,
    /// The open filter box: column and its top-left.
    filter_open: Option<(usize, Pos2)>,
    /// Open this column's filter box at the next frame (the header's right-click menu: "Filter
    /// this column...", the keyboard's way to it).
    filter_for: Option<usize>,
    /// Fit every shown column to its title and cells at the next frame (the header's
    /// right-click menu: "Fit columns to content").
    fit_all: bool,
    /// The sideways scroll when it opened (it closes when the list moves).
    filter_hscroll: f32,
    /// A selection box being dragged: where it started (x in content, y in rows' space) and
    /// whether Ctrl adds it to the selection.
    marquee: Option<(Pos2, bool)>,
    /// Type-ahead: the letters typed so far and when the last one came (seconds).
    typed: String,
    /// A menu is open: its keys (and letters) are its own, not the list's.
    pub menu_open: bool,
    typed_at: f64,
    /// View → Font: the rows' text size, and the File Name column in bold (the settings', set
    /// by the main window before it draws the list).
    pub font: FontSize,
    pub bold_names: bool,
    /// View → Density: the rows taller, their text and icons in their middle.
    pub density: crate::look::Density,
    /// The row height the list was last drawn with (0 before it was).
    drawn_row_h: f32,
    /// Where the scroll benchmark last put the list ([`ListState::bench_scroll`]).
    bench_at: f32,
}

impl Default for ListState {
    fn default() -> Self {
        ListState {
            order: DEFAULT_ORDER.to_vec(),
            widths: DEFAULT_WIDTHS,
            visible: DEFAULT_VISIBLE,
            auto: DEFAULT_AUTO,
            torrent_view: false,
            sort: None,
            scroll: 0.0,
            glide: winui::WheelGlide::default(),
            hscroll: 0.0,
            drag: None,
            // The list has the keys from the start (as Explorer's does), not after a click.
            focused: true,
            scroll_to: None,
            filters: vec![None; COLS],
            filter_open: None,
            filter_for: None,
            fit_all: false,
            filter_hscroll: 0.0,
            marquee: None,
            typed: String::new(),
            menu_open: false,
            typed_at: 0.0,
            font: FontSize::Normal,
            density: Default::default(),
            bold_names: false,
            drawn_row_h: 0.0,
            bench_at: 0.0,
        }
    }
}

/// The part of the column state that is saved with the download list.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    pub order: Vec<usize>,
    /// Lists (not fixed arrays), so layouts saved before a column was added still load.
    pub widths: Vec<f32>,
    pub visible: Vec<bool>,
    pub sort: Option<(usize, bool)>,
    /// Which columns follow the view ([`ListState::auto`]); empty in a layout saved before.
    #[serde(default)]
    pub auto: Vec<bool>,
}

impl ListState {
    /// Does the list have the keys? (A click elsewhere, or Tab to the Categories tree,
    /// takes them.)
    pub fn has_keys(&self) -> bool {
        self.focused
    }

    /// Give the list the keys, or take them (the tree has them).
    pub fn set_keys(&mut self, on: bool) {
        self.focused = on;
    }

    /// The scroll benchmark's step (framestats.rs, developers): `rows` further down a list of
    /// `n`, from the top again past its end; 0 starts at the top.
    pub fn bench_scroll(&mut self, rows: f32, n: usize) {
        let row_h = self.row_h();
        // The list stops at its end (show clamps it): stopped there, it starts over.
        let stuck = self.scroll < self.bench_at;
        self.scroll = if rows == 0.0 || stuck || n == 0 {
            0.0
        } else {
            self.scroll + rows * row_h
        };
        self.bench_at = self.scroll;
    }

    /// Columns as they first were: order, widths, every one shown.
    pub fn reset_columns(&mut self) {
        let fresh = ListState::default();
        self.order = fresh.order;
        self.widths = fresh.widths;
        self.visible = fresh.visible;
        self.auto = fresh.auto;
        self.hscroll = 0.0;
        self.drag = None;
    }

    /// The columns as View → Choose columns leaves them: `order` names every data slot once
    /// (a hidden column keeps its place in it, where it shows again), `visible` which show, File
    /// Name always; `reset_widths` after its Reset (the header's Reset columns does as much).
    /// An order that doesn't name every column once changes nothing.
    pub fn set_columns(&mut self, order: &[usize], visible: [bool; COLS], reset_widths: bool) {
        let mut seen = order.to_vec();
        seen.sort_unstable();
        if seen != (0..COLS).collect::<Vec<_>>() {
            return;
        }
        self.order = order.to_vec();
        // Only what changed becomes the person's choice: a torrent column left as it shows
        // keeps following the view.
        for (c, on) in visible.into_iter().enumerate() {
            if on != self.is_shown(c) {
                self.set_visible(c, on);
            }
        }
        if reset_widths {
            self.widths = DEFAULT_WIDTHS;
            self.hscroll = 0.0;
        }
    }

    /// Fit every shown column to its title and its cells, at the next frame (the header's
    /// "Fit columns to content"; a double-click on a divider fits one).
    pub fn fit_columns(&mut self) {
        self.fit_all = true;
    }

    /// Open column `col`'s filter box under its header at the next frame (the header's
    /// "Filter this column...", from the keyboard too); a hidden column has none.
    pub fn open_filter(&mut self, col: usize) {
        if col < COLS && self.is_shown(col) {
            self.filter_for = Some(col);
        }
    }

    /// Whether a column filter hides rows now (a shown column's: a hidden one has none).
    pub fn filtered(&self) -> bool {
        self.filters
            .iter()
            .enumerate()
            .any(|(c, f)| f.is_some() && self.is_shown(c))
    }

    /// Whether column `col` shows now: the person's choice, or for one that follows the view
    /// ([`ListState::auto`]) whether the Torrents view is shown. File Name always.
    pub fn is_shown(&self, col: usize) -> bool {
        col == 0
            || if self.auto.get(col).copied().unwrap_or(false) {
                self.torrent_view
            } else {
                self.visible.get(col).copied().unwrap_or(false)
            }
    }

    /// Show or hide column `col` (File Name always shows). The others keep their widths and
    /// close up, the room left at the right of the last one, as IDM's list does. A column that
    /// goes takes its sort and its filter with it: the rows would be ordered or missing by
    /// something no longer on the screen (the filter's box closes too); shown again, it starts
    /// unsorted and unfiltered.
    pub fn set_visible(&mut self, col: usize, on: bool) {
        if col >= COLS {
            return;
        }
        let on = on || col == 0;
        self.visible[col] = on;
        self.auto[col] = false;
        if !on {
            if self.sort.is_some_and(|(c, _)| c == col) {
                self.sort = None;
            }
            self.filters[col] = None;
            if self.filter_open.is_some_and(|(c, _)| c == col) {
                self.filter_open = None;
            }
            if matches!(
                self.drag,
                Some(HeaderDrag::Resize { col: c, .. } | HeaderDrag::Press { col: c, .. } | HeaderDrag::Move { col: c, .. }) if c == col
            ) {
                self.drag = None;
            }
        }
    }

    /// A row's height: the theme's (17 in the classic look), taller for a larger View → Font and
    /// a comfortable View → Density.
    pub fn row_h(&self) -> f32 {
        crate::theme::metrics().list_row + self.font.row_grow() + self.density.row_grow()
    }

    /// How far the rows' text and icons move down for View → Density (to the taller row's
    /// middle).
    fn density_shift(&self) -> f32 {
        (self.density.row_grow() / 2.0).floor()
    }

    /// The rows' text (the dialogs' Tahoma at the theme's size, as large as View → Font says);
    /// `name`: the File Name column's, bold when View → Font says so and a bold face is there
    /// (else [`ListState::overstrike`] makes it bold).
    fn row_font(&self, name: bool) -> egui::FontId {
        let size = crate::theme::metrics().font_dialog + self.font.grow();
        let family = if name && self.bold_names && winui::bold_face() {
            "tahoma-bold"
        } else {
            "tahoma"
        };
        egui::FontId::new(size, egui::FontFamily::Name(family.into()))
    }

    /// Whether a column's text is made bold by drawing it twice, a pixel apart (as Windows does
    /// for a font without a bold face): a bold file name where no bold Tahoma is installed
    /// (Linux, macOS).
    fn overstrike(&self, col: usize) -> bool {
        col == 0 && self.bold_names && !winui::bold_face()
    }

    pub fn layout(&self) -> Layout {
        Layout {
            order: self.order.clone(),
            widths: self.widths.to_vec(),
            visible: self.visible.to_vec(),
            sort: self.sort,
            auto: self.auto.to_vec(),
        }
    }

    /// Restore a saved layout. One saved before "Added" existed gets it after Last Try Date,
    /// one saved before "Source" gets it first, one saved before the torrent's columns gets them
    /// after Transfer rate, in this order: Upload speed, Peers, Share ratio. A layout that
    /// doesn't name every column exactly once is ignored.
    pub fn apply(&mut self, mut l: Layout) {
        // Never changed by the person (every column shown, at its first width, in its first
        // order: the defaults of the version that saved it): today's defaults, IDM's calm set.
        let untouched = untouched(&l);
        if !l.order.contains(&SOURCE) {
            l.order.insert(0, SOURCE);
        }
        if !l.order.contains(&ADDED) {
            let at = l
                .order
                .iter()
                .position(|c| *c == 6)
                .map_or(l.order.len(), |p| p + 1);
            l.order.insert(at, ADDED);
        }
        if !l.order.contains(&PEERS) {
            let at = l
                .order
                .iter()
                .position(|c| *c == 5)
                .map_or(l.order.len(), |p| p + 1);
            l.order.insert(at, PEERS);
        }
        if !l.order.contains(&RATIO) {
            let at = l
                .order
                .iter()
                .position(|c| *c == PEERS)
                .map_or(l.order.len(), |p| p + 1);
            l.order.insert(at, RATIO);
        }
        // Last, and so before the two already there: a layout from before #357's second half
        // keeps Transfer rate, then Upload speed, Peers, Share ratio.
        if !l.order.contains(&UPLOAD) {
            let at = l
                .order
                .iter()
                .position(|c| *c == 5)
                .map_or(l.order.len(), |p| p + 1);
            l.order.insert(at, UPLOAD);
        }
        let mut seen = l.order.clone();
        seen.sort_unstable();
        if seen != (0..COLS).collect::<Vec<_>>() {
            return;
        }
        self.order = l.order;
        for (c, default) in DEFAULT_WIDTHS.iter().enumerate() {
            self.widths[c] = l
                .widths
                .get(c)
                .copied()
                .unwrap_or(*default)
                .max(min_width(c));
            // A column the layout didn't know yet comes as today's default has it.
            self.visible[c] = l.visible.get(c).copied().unwrap_or(DEFAULT_VISIBLE[c]);
            self.auto[c] = l
                .auto
                .get(c)
                .copied()
                .unwrap_or(c >= l.visible.len() && DEFAULT_AUTO[c]);
        }
        self.visible[0] = true;
        if untouched {
            self.visible = DEFAULT_VISIBLE;
            self.auto = DEFAULT_AUTO;
        }
        // Never sorted by a column the person hid (a layout saved before hiding cleared it);
        // Arrange files' own orders have no column to hide.
        self.sort = l.sort.filter(|(c, _)| match *c < COLS {
            true => self.visible[*c] || self.auto[*c],
            false => sort_slot(*c),
        });
    }

    /// Visible columns in display order.
    fn shown(&self) -> Vec<usize> {
        self.order
            .iter()
            .copied()
            .filter(|c| self.is_shown(*c))
            .collect()
    }
}

/// Whether a saved layout is as the version that saved it first had it (the person never
/// changed a column): every column shown, each at its first width (Added's was 125 before it
/// was 145), in the first order. A layout from before the columns followed the view; a later
/// one says so itself ([`Layout::auto`]).
fn untouched(l: &Layout) -> bool {
    let first: Vec<usize> = DEFAULT_ORDER
        .iter()
        .copied()
        .filter(|c| l.order.contains(c))
        .collect();
    l.auto.is_empty()
        && l.order == first
        && l.visible.iter().take(l.order.len()).all(|v| *v)
        && l.order.iter().all(|&c| {
            let w = l.widths.get(c).copied().unwrap_or(DEFAULT_WIDTHS[c]);
            w == DEFAULT_WIDTHS[c] || (c == ADDED && w == 125.0)
        })
}

#[derive(Clone)]
pub struct Row {
    /// What the Status filter sees: Downloading, Stopped, Waiting, Complete or Error.
    pub status_group: &'static str,
    pub icon: IconKey,
    /// In a queue: the Q column shows a queue icon (IDM: stacked files).
    pub queued: bool,
    pub cells: [String; COLS],
}

/// The rows a list shows, each made when it is needed: a frame makes the rows in view (and
/// those a screen reader reaches), not every row's texts. A list of 10,000 downloads took about
/// 40 ms a frame to make them all, at every frame (MDM_FRAME_STATS, framestats.rs).
pub trait Rows {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Row `i`, for `i < len()`.
    fn row(&self, i: usize) -> Row;
    /// Row `i`'s file name (all type-ahead looks at).
    fn name(&self, i: usize) -> String {
        self.row(i).cells[0].clone()
    }
}

impl Rows for Vec<Row> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn row(&self, i: usize) -> Row {
        self[i].clone()
    }
    fn name(&self, i: usize) -> String {
        self[i].cells[0].clone()
    }
}

#[derive(Default)]
pub struct ListResponse {
    /// The rows' area (under the header, inside the scroll bars), English: where an empty
    /// list's message goes.
    pub body: Option<Rect>,
    /// How the selection changes (a click, a key, a selection box).
    pub pick: Option<Pick>,
    /// Double-click on this row.
    pub double_clicked: Option<usize>,
    /// Enter on the focused row: what a double-click does (On Double click: Open or Properties).
    pub enter: Option<usize>,
    /// Alt+Enter: the focused row's Properties.
    pub properties: Option<usize>,
    /// Right-click: the row (None = blank space) and where its menu opens.
    pub context: Option<(Option<usize>, Pos2)>,
    /// The header was right-clicked here (column chooser), and the column under the pointer.
    pub header_context: Option<Pos2>,
    pub header_col: Option<usize>,
    /// The sort column or direction changed.
    pub sort_changed: bool,
}

/// A Windows list view's keys while the list has the focus (and no text box does), with its
/// multiple selection. Up / Down, Page Up / Page Down (a page of rows) and Home / End move the
/// focus: alone they select that row, with Shift they select from the anchor to it, with Ctrl
/// they only move the focus, with Ctrl+Shift they add that range. Space selects the focused row,
/// Ctrl+Space puts it in or out, Ctrl+A selects all (also before the list was clicked). Enter is
/// a double-click on the focused row, Alt+Enter opens its Properties. Typing selects the next
/// file whose name starts with the letters typed (within a second of each other; one letter
/// again and again goes through the names starting with it). Returns true for Shift+F10 / the
/// Menu key: the focused row's context menu.
fn keyboard(
    ui: &Ui,
    st: &mut ListState,
    rows: &dyn Rows,
    focus: Option<usize>,
    client_h: f32,
    out: &mut ListResponse,
) -> bool {
    let n = rows.len();
    if n == 0 || st.menu_open || ui.ctx().memory(|m| m.focused().is_some()) {
        return false;
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
        st.focused = true;
        out.pick = Some(Pick::All);
        return false;
    }
    if !st.focused {
        return false;
    }
    let row_h = st.row_h();
    let page = ((client_h - row_h) / row_h).floor().max(1.0) as isize;
    // The first row wholly in view (the wheel may leave the top one part-way out).
    let top = (st.scroll / row_h - 0.01).ceil() as isize;
    let last = n as isize - 1;
    let cur = focus.map(|s| s as isize);
    let (to, mods) = ui.input(|i| {
        let k = |key| i.key_pressed(key) && !i.modifiers.alt;
        let to = if k(egui::Key::ArrowDown) {
            Some(cur.map_or(0, |c| c + 1))
        } else if k(egui::Key::ArrowUp) {
            Some(cur.map_or(0, |c| c - 1))
        // As Explorer: first to the last (first) row in view, then a page on.
        } else if k(egui::Key::PageDown) {
            let bottom = top + page - 1;
            Some(cur.map_or(0, |c| if c < bottom { bottom } else { c + page }))
        } else if k(egui::Key::PageUp) {
            Some(cur.map_or(0, |c| if c > top { top } else { c - page }))
        } else if k(egui::Key::Home) {
            Some(0)
        } else if k(egui::Key::End) {
            Some(last)
        } else {
            None
        };
        (to, i.modifiers)
    });
    if let Some(to) = to {
        let to = to.clamp(0, last) as usize;
        out.pick = Some(match (mods.command, mods.shift) {
            (false, false) => Pick::Only(to),
            (true, false) => Pick::Focus(to),
            (ctrl, true) => Pick::Range { to, add: ctrl },
        });
        st.scroll_to = Some(to);
        st.typed.clear();
        return false;
    }
    let (menu, enter, alt_enter, space) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Menu)
                || (i.key_pressed(egui::Key::F10) && i.modifiers.shift_only()),
            i.key_pressed(egui::Key::Enter) && i.modifiers.is_none(),
            i.key_pressed(egui::Key::Enter) && i.modifiers.alt,
            i.key_pressed(egui::Key::Space),
        )
    });
    if let Some(f) = focus {
        if enter {
            out.enter = Some(f);
        } else if alt_enter {
            out.properties = Some(f);
        }
    }
    // Type-ahead (a Space while typing is part of the name).
    let now = ui.input(|i| i.time);
    if now - st.typed_at > 1.0 {
        st.typed.clear();
    }
    let typed: String = ui.input(|i| {
        if i.modifiers.command || i.modifiers.alt {
            return String::new();
        }
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    });
    let typing = !typed.is_empty() && !(typed == " " && st.typed.is_empty());
    if typing {
        st.typed.push_str(&typed.to_lowercase());
        st.typed_at = now;
        let one_letter = st.typed.chars().all(|c| st.typed.starts_with(c));
        let (prefix, from) = if one_letter {
            // The same letter again: the next name starting with it.
            (
                st.typed.chars().take(1).collect::<String>(),
                focus.map_or(0, |f| f + 1),
            )
        } else {
            (st.typed.clone(), focus.unwrap_or(0))
        };
        if let Some(i) = (0..n)
            .map(|k| (from + k) % n)
            .find(|i| rows.name(*i).to_lowercase().starts_with(&prefix))
        {
            out.pick = Some(Pick::Only(i));
            st.scroll_to = Some(i);
        }
    } else if space && let Some(f) = focus {
        out.pick = Some(match (mods.command, mods.shift) {
            (true, _) => Pick::Toggle(f),
            (false, true) => Pick::Range { to: f, add: false },
            (false, false) => Pick::Only(f),
        });
    }
    menu
}

/// Paint the list inside `outer` (its 1 px #828790 border rect). `selected`: each row's
/// selection; `focus`: the focused row (the dotted outline, where keys move from).
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    outer: Rect,
    st: &mut ListState,
    source: &dyn Rows,
    selected: &[bool],
    focus: Option<usize>,
    icons: &mut ShellIcons,
    // The Status cell's tip for row `i` (asked for the row under the pointer only).
    status_tip: &dyn Fn(usize) -> String,
) -> ListResponse {
    let mut out = ListResponse::default();
    // `outer` and everything below are English (the main window's layout): each part is made where
    // it is on screen (`screen`, `fill`, `text_font`), and the pointer is read in English to find
    // the column or row under it.
    let screen = winui::rect;
    let english = |q: Pos2| pos2(winui::mx(q.x), q.y);
    // Focus follows the last press: inside the list focuses it, anywhere else unfocuses it.
    if ui.input(|i| i.pointer.any_pressed()) {
        st.focused = ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|p| screen(outer).contains(p));
    }
    let p = ui.painter_at(screen(outer));
    let (l, t, r, b) = (
        outer.left(),
        outer.top(),
        outer.right() - 1.0,
        outer.bottom() - 1.0,
    );
    winui::solid(&p, screen(outer), FRAME());
    fill(&p, l + 1.0, t + 1.0, r - 1.0, b - 1.0, BG());
    let (cl, ct, cr, cb) = (l + 2.0, t + 2.0, r - 2.0, b - 2.0);
    // The header's text; the rows' as large as View → Font says, the names bold if it says so.
    let font = winui::tahoma();
    let row_h = st.row_h();
    // Rows of another height (View → Font, another theme) keep the same first row in view.
    if st.drawn_row_h > 0.0 && st.drawn_row_h != row_h {
        st.scroll = (st.scroll / st.drawn_row_h).round() * row_h;
    }
    st.drawn_row_h = row_h;
    let (cell_font, name_font) = (st.row_font(false), st.row_font(true));
    // Where the rows put their text's baseline and their icons, below a row's top.
    let baseline = 12.0 + st.font.baseline_shift() + st.density_shift();
    let icon_y = st.font.icon_shift() + st.density_shift();
    let overstrike_names = st.overstrike(0);
    // "Fit columns to content": each shown column as wide as its title and cells need.
    if std::mem::take(&mut st.fit_all) {
        for c in st.shown() {
            let cells = if c == 0 { &name_font } else { &cell_font };
            st.widths[c] = fit_width(&p, &font, cells, c, source);
        }
    }

    // Scroll bars (notes/ui-spec/scrollbars.md), decided like Win32: each one shows when the
    // content overflows the room the other one leaves.
    let content_w: f32 = st.shown().iter().map(|c| st.widths[*c]).sum();
    let (client_w, client_h) = (cr - cl + 1.0, cb - ct + 1.0);
    // How many rows there are; each one's texts are made only once it is drawn (`made`, below).
    let n = source.len();
    let rows_h = n as f32 * row_h;
    let menu_key = keyboard(ui, st, source, focus, client_h, &mut out);
    let (mut need_v, mut need_h) = (false, false);
    for _ in 0..2 {
        // An empty list has nothing to scroll to (its empty state is centred in the view).
        need_h = !source.is_empty()
            && content_w > client_w - if need_v { scrollbar::THICK } else { 0.0 };
        need_v = rows_h > client_h - header_h() - if need_h { scrollbar::THICK } else { 0.0 };
    }
    // Inclusive right edge of the header and rows, and bottom of the rows.
    let rr = cr - if need_v { scrollbar::THICK } else { 0.0 };
    let rb = cb - if need_h { scrollbar::THICK } else { 0.0 };
    let row_w = rr - cl + 1.0;
    st.hscroll = st.hscroll.clamp(0.0, (content_w - row_w).max(0.0)).round();

    // Column spans in display order: (slot, x, right) with the separator on the last pixel.
    let spans = |st: &ListState| {
        let mut v = Vec::new();
        let mut x = cl - st.hscroll;
        for c in st.shown() {
            v.push((c, x, x + st.widths[c] - 1.0));
            x += st.widths[c];
        }
        v
    };

    // ---- header interaction ----------------------------------------------------------------
    let header = Rect::from_min_max(pos2(cl, ct), pos2(rr + 1.0, ct + header_h()));
    let hresp = ui.interact(
        screen(header),
        egui::Id::unique("list-header"),
        winui::pointer_drag(),
    );
    let pointer = ui.ctx().pointer_interact_pos().map(english);
    let cols = spans(st);
    let near_sep = |x: f32| {
        cols.iter()
            .find(|(_, _, x1)| (x - (x1 + 0.5)).abs() <= GRIP)
            .map(|s| s.0)
    };
    if let Some(pp) = hresp.hover_pos().map(english)
        && near_sep(pp.x).is_some()
    {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    // Resize or move is decided where the button went down: a drag is only recognised after the
    // pointer has travelled a few pixels, by then off a border's grip (user report 2026-10-29:
    // dragging a border moved the column instead of resizing it).
    if hresp.drag_started()
        && let Some(pp) = ui
            .input(|i| i.pointer.press_origin())
            .map(english)
            .or(pointer)
    {
        st.drag = match near_sep(pp.x) {
            Some(col) => Some(HeaderDrag::Resize {
                col,
                width: st.widths[col],
                x0: pp.x,
            }),
            None => cols
                .iter()
                .find(|(_, x0, x1)| pp.x >= *x0 && pp.x <= *x1)
                .map(|(col, _, _)| HeaderDrag::Press {
                    col: *col,
                    x0: pp.x,
                }),
        };
    }
    if let (Some(drag), Some(pp)) = (st.drag, pointer) {
        match drag {
            HeaderDrag::Resize { col, width, x0 } => {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                st.widths[col] = (width + pp.x - x0).round().max(min_width(col));
            }
            HeaderDrag::Press { col, x0 } if (pp.x - x0).abs() >= DRAG_START => {
                let start = cols.iter().find(|s| s.0 == col).map_or(x0, |s| s.1);
                st.drag = Some(HeaderDrag::Move {
                    col,
                    grab: x0 - start,
                });
            }
            _ => {}
        }
    }
    // A double-click on a border fits the column to the widest of its title and cells (as in
    // Explorer).
    if hresp.double_clicked()
        && let Some(col) = pointer.and_then(|pp| near_sep(pp.x))
    {
        let cells = if col == 0 { &name_font } else { &cell_font };
        st.widths[col] = fit_width(&p, &font, cells, col, source);
        st.drag = None;
    }
    if hresp.drag_stopped() {
        if let (Some(HeaderDrag::Move { col, .. }), Some(pp)) = (st.drag, pointer) {
            move_column(st, &cols, col, pp.x);
        }
        st.drag = None;
    }
    // What each header shows besides its title (`header_parts`): the funnel only under the
    // pointer or on a filtered column, the sort arrow on the sorted one, each where the title
    // leaves room.
    let hover_x = hresp.hover_pos().map(english).map(|h| h.x);
    let parts_of = |st: &ListState, c: usize, x0: f32, x1: f32| {
        let hovered = hover_x.is_some_and(|h| h >= x0 && h <= x1) && st.drag.is_none();
        let title = crate::i18n::tc("column", TITLES[c]);
        let filtered = st.filters[c].is_some() || st.filter_open.is_some_and(|(fc, _)| fc == c);
        header_parts(
            x1 - x0 + 1.0,
            winui::text_width_font(&p, font.clone(), title),
            st.sort.is_some_and(|(sc, _)| sc == c),
            filtered,
            hovered,
        )
    };
    // A click on a header's funnel (where it shows) opens its filter box instead of sorting.
    let funnel_hit = |st: &ListState, pp: Pos2| {
        cols.iter()
            .filter(|(c, x0, x1)| parts_of(st, *c, *x0, *x1).funnel)
            .map(|(c, x0, x1)| {
                (
                    *c,
                    funnel_rect(*x0, *x1, ct, parts_of(st, *c, *x0, *x1).arrow),
                )
            })
            .find(|(_, fr)| fr.contains(pp))
            .map(|(c, fr)| (c, fr.left_bottom()))
    };
    let funnel_click = hresp
        .clicked()
        .then_some(pointer)
        .flatten()
        .and_then(|pp| funnel_hit(st, pp));
    if let Some((col, at)) = funnel_click {
        st.filter_open = match st.filter_open {
            Some((c, _)) if c == col => None,
            _ => Some((
                col,
                pos2(at.x.min(outer.right() - FILTER_W), ct + header_h()),
            )),
        };
        st.filter_hscroll = st.hscroll;
    }
    if hresp.clicked()
        && funnel_click.is_none()
        && let Some(pp) = pointer
        && near_sep(pp.x).is_none()
        && let Some((col, ..)) = cols.iter().find(|(_, x0, x1)| pp.x >= *x0 && pp.x <= *x1)
    {
        // Ascending → descending → unsorted (order of addition).
        st.sort = match st.sort {
            Some((c, true)) if c == *col => Some((c, false)),
            Some((c, false)) if c == *col => None,
            _ => Some((*col, true)),
        };
        out.sort_changed = true;
    }
    if hresp.secondary_clicked() {
        out.header_context = ui.ctx().pointer_interact_pos();
        out.header_col = pointer.and_then(|pp| {
            cols.iter()
                .find(|(_, x0, x1)| pp.x >= *x0 && pp.x <= *x1)
                .map(|c| c.0)
        });
    }
    // "Filter this column..." (the header's menu, the keyboard's way): its box under its header,
    // as its funnel opens it.
    if let Some(col) = st.filter_for.take()
        && let Some((_, x0, x1)) = cols.iter().find(|c| c.0 == col).copied()
    {
        let fr = funnel_rect(x0, x1, ct, st.sort.is_some_and(|(c, _)| c == col));
        let at = fr.left().max(cl).min(outer.right() - FILTER_W);
        st.filter_open = Some((col, pos2(at, ct + header_h())));
        st.filter_hscroll = st.hscroll;
    }

    // ---- header painting -------------------------------------------------------------------
    let cols = spans(st);
    let p_all = p.clone();
    let p = p.with_clip_rect(screen(header));
    fill(&p, cl, ct, rr, ct + header_h() - 1.0, HEADER_BG());
    for (c, x0, x1) in &cols {
        let hovered = hresp
            .hover_pos()
            .map(english)
            .is_some_and(|h| h.x >= *x0 && h.x <= *x1 && st.drag.is_none());
        // The column under the pointer, and pressed while the mouse button is held on it (as
        // Windows' headers); the shared tint (winui::subtle_fill).
        let state = match (hovered, hresp.is_pointer_button_down_on()) {
            (false, _) => winui::State::Normal,
            (true, false) => winui::State::Hot,
            (true, true) => winui::State::Pressed,
        };
        let tint = winui::subtle_fill(state, crate::theme::colors());
        let parts = parts_of(st, *c, *x0, *x1);
        let sorted = st.sort.filter(|(sc, _)| sc == c);
        let filtered = st.filters[*c].is_some();
        // The sorted column's header is highlighted (and a filtered one's tinted, so a filtered
        // list is plain to see), under the pointer's tint.
        if sorted.is_some() {
            fill(
                &p,
                *x0,
                ct,
                x1 - 1.0,
                ct + header_h() - 1.0,
                tint.unwrap_or(SORTED_BG()),
            );
        } else if let Some(t) = tint {
            fill(&p, *x0, ct, x1 - 1.0, ct + header_h() - 1.0, t);
        }
        if filtered {
            fill(
                &p,
                *x0,
                ct,
                x1 - 1.0,
                ct + header_h() - 1.0,
                FILTER_ON().gamma_multiply(0.16),
            );
        }
        // The sort arrow at the cell's right edge (▲ ascending, ▼ descending), vertically
        // centred, where the title leaves it room.
        if let Some((_, asc)) = sorted
            && parts.arrow
        {
            let cx = x1 - 9.0;
            let mid = ct + (header_h() / 2.0).floor() - 2.0;
            for i in 0..4 {
                let y = mid + if asc { i as f32 } else { 3.0 - i as f32 };
                let half = i as f32;
                fill(&p, cx - half, y, cx + half, y, SORT_ARROW());
            }
        }
        fill(&p, *x1, ct, *x1, ct + header_h() - 1.0, HEADER_SEP());
        // The funnel (beyond IDM, whose headers show only their titles): under the pointer, and
        // always on a filtered column (in the filter's colour) or one whose box is open; never
        // over the title (`header_parts`).
        let open = st.filter_open.is_some_and(|(fc, _)| fc == *c);
        if parts.funnel {
            let fr = funnel_rect(*x0, *x1, ct, parts.arrow);
            let color = if filtered {
                FILTER_ON()
            } else if hovered || open {
                SORT_ARROW()
            } else {
                SORT_ARROW().gamma_multiply(0.45)
            };
            funnel(&p, fr, color);
        }
        let taken =
            if parts.funnel { PART_W } else { 0.0 } + if parts.arrow { PART_W } else { 0.0 };
        let full = crate::i18n::tc("column", TITLES[*c]);
        let label = winui::ellipsize(&p, &font, full, x1 - x0 + 1.0 - HEADER_PAD - taken);
        // A cut title shows whole under the pointer; the one-letter Q says what it is.
        if hovered && st.drag.is_none() {
            let tip = if TITLES[*c] == "Q" {
                // i18n: tooltip of the narrow "Q" column's title
                Some(crate::i18n::tc("column", "Queue"))
            } else {
                (label != full).then_some(full)
            };
            if let Some(tip) = tip {
                hresp.clone().on_hover_text_at_pointer(tip);
            }
        }
        let clip = p.with_clip_rect(screen(Rect::from_x_y_ranges(
            *x0..=*x1,
            ct..=ct + header_h(),
        )));
        text_font(&clip, font.clone(), x0 + 6.0, ct + 15.0, &label, TEXT());
    }

    // ---- body ------------------------------------------------------------------------------
    let p = p_all;
    let body_top = ct + header_h();
    let body = Rect::from_min_max(pos2(cl, body_top), pos2(rr + 1.0, rb + 1.0));
    out.body = Some(body);
    let resp = ui.interact(
        screen(body),
        egui::Id::unique("download-list"),
        winui::pointer_drag(),
    );
    // For screen readers: the list, its rows below, and the row with the dotted outline
    // as the one they read (the list keeps its keys itself, not egui's focus).
    winui::expose(
        &resp,
        egui::Role::ListBox,
        crate::i18n::t("Downloads"),
        true,
        None,
    );
    if let Some(f) = focus {
        let row = egui::Id::unique(("download-row", f));
        ui.ctx()
            .accesskit_node_builder(resp.id, |n| n.set_active_descendant(row.accesskit_id()));
    }
    let bp = p.with_clip_rect(screen(body));
    // Fully visible rows; the list scrolls until the last one is fully shown.
    let page_rows = (body.height() / row_h).floor().max(1.0);
    let max_scroll = (n as f32 - page_rows).max(0.0) * row_h;
    // The wheel glides the list to where its notches take it, a pixel at a time over a few
    // frames (a touchpad's own small moves at once), where a Win32 list view jumps three rows
    // a notch; the keys, Find and the scroll bar still move it at once, by whole rows.
    let (glide, direct) = if resp.hovered() {
        st.hscroll -= ui.input(|i| i.smooth_scroll_delta.x);
        winui::wheel_travel(ui, row_h, page_rows)
    } else {
        (0.0, 0.0)
    };
    st.scroll += direct * row_h;
    st.glide
        .step(ui.ctx(), &mut st.scroll, glide * row_h, max_scroll);
    // Rows out of view are there for screen readers too (#111), as many as `reachable_rows`
    // gives around the view; one asked to scroll into view shows from this frame on.
    let row_id = |idx: usize| egui::Id::unique(("download-row", idx));
    // The number of rows is the list's (UI Automation reads a row's "of M" from its list).
    let a11y = ui
        .ctx()
        .accesskit_node_builder(resp.id, |node| node.set_size_of_set(n))
        .is_some();
    let page = (body.height() / row_h).floor().max(1.0) as usize;
    let reach = winui::reachable_rows(n, (st.scroll / row_h).round() as usize, page);
    if a11y && let Some(idx) = winui::row_asked_for(ui, reach.clone(), row_id) {
        st.scroll_to = Some(idx);
    }
    if let Some(row) = st.scroll_to.take() {
        let top = row as f32 * row_h;
        if top < st.scroll {
            st.scroll = top;
        } else if top + row_h > st.scroll + page_rows * row_h {
            st.scroll = top + row_h - page_rows * row_h;
        }
        st.glide.stop(st.scroll);
    }
    st.scroll = st.scroll.clamp(0.0, max_scroll);
    st.hscroll = st.hscroll.clamp(0.0, (content_w - row_w).max(0.0)).round();
    // Drawn where the list is to the pixel (a glide part-way between rows): the rows where
    // they are at a whole row, less whole screen pixels, so the text stays as sharp.
    let ppp = ui.ctx().pixels_per_point();
    let first = (st.scroll / row_h + 1e-4).floor() as usize;
    let offset = ((st.scroll - first as f32 * row_h) * ppp).round().max(0.0) / ppp;
    let shown_scroll = first as f32 * row_h + offset;
    let visible = (body.height() / row_h).ceil() as usize + 1;
    let last_right = cols.last().map_or(cl, |s| s.2);
    let mut cut_under_pointer = None;
    let keys_in_use = winui::keyboard_in_use(ui.ctx());
    // A screen reader's row: the name, then Status and Size, selected or not, its place among all
    // the rows, where it is (out of view: above or below the list).
    // The rows are the list's children (UI Automation reads "12 of 40" from the list, and finds
    // them under it): made as widgets of a Ui whose parent is the list, clipped to nothing so
    // they take no pointer from it.
    let mut rows_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(screen(body))
            .accessibility_parent(resp.id),
    );
    rows_ui.set_clip_rect(Rect::NOTHING);
    let expose_row = |idx: usize, row: &Row, picked: bool| {
        let top = body_top + (idx as f32 - first as f32) * row_h - offset;
        let bounds = screen(Rect::from_min_size(
            pos2(cl, top),
            egui::vec2(rr - cl, row_h),
        ));
        rows_ui.interact(bounds, row_id(idx), egui::Sense::hover());
        let spoken = [0usize, 3, 2]
            .iter()
            .map(|c| row.cells[*c].as_str())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        ui.ctx().accesskit_node_builder(row_id(idx), |node| {
            node.set_role(egui::Role::ListBoxOption);
            node.set_label(spoken);
            node.set_selected(picked);
            // AccessKit counts from 0 (UI Automation and AT-SPI say idx + 1).
            node.set_position_in_set(idx);
            node.set_size_of_set(n);
            node.add_action(egui::accesskit::Action::ScrollIntoView);
            node.set_bounds(egui::accesskit::Rect {
                x0: (bounds.left() * ppp) as f64,
                y0: (bounds.top() * ppp) as f64,
                x1: (bounds.right() * ppp) as f64,
                y1: (bounds.bottom() * ppp) as f64,
            });
        });
    };
    // In the list's order: those above the view, those in it (below, as they are drawn), then
    // those under it.
    let reach = if a11y {
        winui::reachable_rows(n, first, page)
    } else {
        0..0
    };
    // The rows this frame needs, made now: those in view, and those a screen reader reaches.
    let in_view = first..(first + visible).min(n);
    let needed = if reach.is_empty() {
        in_view
    } else {
        reach.start.min(in_view.start)..reach.end.max(in_view.end)
    };
    let made: Vec<Row> = needed.clone().map(|i| source.row(i)).collect();
    let rows = |idx: usize| idx.checked_sub(needed.start).and_then(|k| made.get(k));
    let expose_out_of_view = |range: std::ops::Range<usize>| {
        for idx in range {
            if let Some(row) = rows(idx) {
                expose_row(idx, row, selected.get(idx).copied().unwrap_or(false));
            }
        }
    };
    expose_out_of_view(reach.start..reach.end.min(first));
    for k in 0..visible {
        let idx = first + k;
        // On a whole screen pixel, each row: rows 21.25 px apart at 125 % drew every fourth
        // grid line two pixels thick (winui::on_pixel).
        let top = winui::on_pixel(ppp, body_top + k as f32 * row_h - offset);
        if let Some(row) = rows(idx) {
            let picked = selected.get(idx).copied().unwrap_or(false);
            expose_row(idx, row, picked);
            let sel = picked && st.focused;
            // The focused row's dotted outline only once the keyboard is in use, as every
            // control's focus (winui::keyboard_in_use).
            let has_focus = focus == Some(idx) && st.focused && keys_in_use;
            if sel || has_focus {
                // The row above its grid line; the focused row has a dotted outline (every other
                // pixel), in the fill's inverse on a selected row. Both modes alike.
                let (right, bottom) = (last_right - 1.0, top + row_h - 2.0);
                if sel {
                    fill(&bp, cl, top, right, bottom, SELECTED());
                }
                if has_focus {
                    let dot = if sel {
                        crate::theme::colors().selection_focus
                    } else {
                        TEXT()
                    };
                    dotted(&bp, cl, top, right, bottom, dot);
                }
            } else if picked {
                // Unfocused (Find, Properties, a menu has the keys): still shown, in the inactive
                // selection grey, as Explorer shows it, so what a command acts on stays visible.
                let bottom = top + row_h - 2.0;
                fill(
                    &bp,
                    cl,
                    top,
                    last_right - 1.0,
                    bottom,
                    winui::SELECTED_INACTIVE(),
                );
            }
            for (c, x0, x1) in &cols {
                // The file-type icon sits in the File Name column, wherever it is.
                let tx = if *c == 0 {
                    icons.paint_in(ui, &bp, row.icon.clone(), pos2(x0 + 4.0, top + icon_y));
                    x0 + 22.0
                } else {
                    x0 + 6.0
                };
                if *c == 1 && row.queued {
                    icons.paint_in(
                        ui,
                        &bp,
                        IconKey::Silk("page_white_stack"),
                        pos2(x0 + 2.0, top + icon_y),
                    );
                }
                let cell = Rect::from_min_max(pos2(*x0, top), pos2(*x1 - 1.0, top + row_h));
                if *c == SOURCE {
                    let cut = chip(
                        &bp.with_clip_rect(screen(cell.intersect(body))),
                        &cell_font,
                        (*x0, *x1),
                        (top, row_h, baseline),
                        &row.cells[*c],
                    );
                    let hovered = resp
                        .hover_pos()
                        .map(english)
                        .is_some_and(|h| cell.contains(h));
                    if cut && hovered {
                        cut_under_pointer = Some(crate::i18n::t(&row.cells[*c]).to_owned());
                    }
                    continue;
                }
                let value = cell_text(*c, &row.cells[*c]);
                let (cf, bold) = if *c == 0 {
                    (&name_font, overstrike_names)
                } else {
                    (&cell_font, false)
                };
                // An overstruck name is a pixel wider.
                let room = x1 - tx - 3.0 - if bold { 1.0 } else { 0.0 };
                let s = winui::ellipsize(&bp, cf, &value, room);
                // A cut cell shows its whole text on hover (Windows' list label tip); the
                // Status cell shows the download's recent events.
                if resp
                    .hover_pos()
                    .map(english)
                    .is_some_and(|h| cell.contains(h))
                {
                    let tip = if *c == 3 {
                        status_tip(idx)
                    } else {
                        String::new()
                    };
                    if !tip.is_empty() {
                        cut_under_pointer = Some(tip);
                    } else if s != value {
                        cut_under_pointer = Some(row.cells[*c].clone());
                    }
                }
                let clip = bp.with_clip_rect(screen(cell.intersect(body)));
                // A failed download's Status and Description (the reason) in the error colour.
                let failed = row.status_group == "Error" && (*c == 3 || *c == 7);
                let ink = if sel {
                    winui::SELECTION_TEXT()
                } else if failed {
                    winui::ERROR()
                } else {
                    TEXT()
                };
                text_font(&clip, cf.clone(), tx, top + baseline, &s, ink);
                if bold {
                    text_font(&clip, cf.clone(), tx + 1.0, top + baseline, &s, ink);
                }
            }
        }
        // The grid line is the item's own last row (top + 16), so icons are not clipped; in
        // both modes (dark changes only colours).
        let gy = top + row_h - 1.0;
        // An empty list has no grid (the empty state stands alone).
        if n > 0 {
            fill(&bp, cl, gy, rr, gy, GRID_H());
        }
    }
    expose_out_of_view(reach.start.max(first + visible)..reach.end);
    if let Some(full) = cut_under_pointer {
        // At the pointer: the list is one widget, and a tip placed by it showed under its
        // bottom edge, far from the row.
        resp.clone().on_hover_text_at_pointer(full);
    }
    filter_box(ui, st);
    for (_, _, x1) in cols.iter().filter(|_| n > 0) {
        // One line at col.right + 1.
        fill(&bp, x1 + 1.0, body_top, x1 + 1.0, rb, GRID_V_RIGHT());
    }
    // While a column is dragged: a translucent copy of its header follows the pointer, and a
    // 2 px mark shows where it will land.
    if let (Some(HeaderDrag::Move { col, grab }), Some(pp)) = (st.drag, pointer) {
        let w = st.widths[col];
        let ghost = Rect::from_min_size(pos2(pp.x - grab, ct), egui::vec2(w, header_h()));
        let top = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("col-ghost"),
        ));
        top.rect_filled(screen(ghost), 0.0, crate::theme::colors().drag_ghost);
        text_font(
            &top,
            font.clone(),
            ghost.left() + 6.0,
            ct + 15.0,
            crate::i18n::tc("column", TITLES[col]),
            TEXT(),
        );
        let mark = drop_x(&cols, pp.x);
        fill(
            &top,
            mark - 1.0,
            ct,
            mark,
            ct + header_h() - 1.0,
            winui::accent(),
        );
    }

    // ---- scroll bars -----------------------------------------------------------------------
    // Vertical: beside the header too (client top to the corner), in rows.
    if need_v {
        // At the trailing edge: the left in RTL.
        let track = screen(Rect::from_min_max(
            pos2(rr + 1.0, ct),
            pos2(cr + 1.0, rb + 1.0),
        ));
        let mut pos_rows = shown_scroll / row_h;
        if scrollbar::bar(
            ui,
            egui::Id::unique("list-vbar"),
            track,
            true,
            n as f32,
            page_rows,
            1.0,
            &mut pos_rows,
        ) {
            st.scroll = pos_rows.round() * row_h;
            ui.ctx().request_repaint();
        }
    }
    // Horizontal: under the rows, in pixels.
    if need_h {
        let track = screen(Rect::from_min_max(
            pos2(cl, rb + 1.0),
            pos2(rr + 1.0, cb + 1.0),
        ));
        let mut x = st.hscroll;
        if scrollbar::bar(
            ui,
            egui::Id::unique("list-hbar"),
            track,
            false,
            content_w,
            row_w,
            15.0,
            &mut x,
        ) {
            st.hscroll = x.round();
            ui.ctx().request_repaint();
        }
    }
    if need_v && need_h {
        scrollbar::corner(ui, rr + 1.0, rb + 1.0);
    }

    // ---- row hit testing -------------------------------------------------------------------
    let row_at = |pos: Pos2| -> Option<usize> {
        if !body.contains(pos) || pos.x > last_right {
            return None;
        }
        let idx = ((pos.y - body_top + shown_scroll) / row_h).floor() as usize;
        (idx < n).then_some(idx)
    };
    let mods = ui.input(|i| i.modifiers);
    if let Some(on_screen) = resp.interact_pointer_pos()
        && let pos = english(on_screen)
        && body.contains(pos)
    {
        if resp.clicked() {
            // Windows: Ctrl+click puts a row in or out, Shift+click selects from the anchor,
            // Ctrl+Shift+click adds that range; a click on blank space clears (not with Ctrl).
            out.pick = match (row_at(pos), mods.command, mods.shift) {
                (Some(i), false, false) => Some(Pick::Only(i)),
                (Some(i), true, false) => Some(Pick::Toggle(i)),
                (Some(to), ctrl, true) => Some(Pick::Range { to, add: ctrl }),
                (None, false, _) => Some(Pick::Clear),
                (None, true, _) => None,
            };
        }
        if resp.double_clicked() {
            out.double_clicked = row_at(pos);
        }
        if resp.secondary_clicked() {
            // A right-click inside the selection keeps it (the menu acts on all of it); outside,
            // the row alone is selected; on blank space the selection is cleared. The menu opens
            // at the pointer (IDM opens the blank-space one at the list's centre; user decision).
            let row = row_at(pos);
            out.pick = Some(match row {
                Some(i) if selected.get(i).copied().unwrap_or(false) => Pick::Focus(i),
                Some(i) => Pick::Only(i),
                None => Pick::Clear,
            });
            out.context = Some((row, on_screen));
        }
    }
    // Shift+F10 / the Menu key: the focused row's menu, under it.
    if menu_key {
        let at = focus.map_or(body.left_top() + egui::vec2(20.0, 4.0), |f| {
            pos2(
                cl + 20.0,
                (body_top + (f as f32 + 1.0) * row_h - shown_scroll)
                    .clamp(body.top(), body.bottom()),
            )
        });
        // `at` is English; the menu opens at its screen point.
        out.context = Some((focus, pos2(winui::mx(at.x), at.y)));
    }

    // A selection box, dragged from blank space (below the rows or right of the columns), as in
    // Explorer: the rows it touches are selected, Ctrl adds them; dragging past the top or bottom
    // scrolls.
    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin()).map(english)
        && body.contains(origin)
        && row_at(origin).is_none()
    {
        st.marquee = Some((
            pos2(origin.x + st.hscroll, origin.y - body_top + shown_scroll),
            mods.command,
        ));
    }
    if let Some((start, add)) = st.marquee {
        if let Some(pp) = pointer {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            let step = 30.0 * row_h * dt;
            if pp.y < body.top() {
                st.scroll = (st.scroll - step).max(0.0);
                ui.ctx().request_repaint();
            } else if pp.y > body.bottom() {
                st.scroll = (st.scroll + step).min(max_scroll);
                ui.ctx().request_repaint();
            }
            let now = pos2(pp.x + st.hscroll, pp.y - body_top + st.scroll);
            let (y0, y1) = (start.y.min(now.y), start.y.max(now.y));
            let (x0, x1) = (start.x.min(now.x), start.x.max(now.x));
            let touches_columns = x0 <= last_right + st.hscroll && x1 >= cl + st.hscroll;
            let picked: Vec<usize> = if touches_columns {
                let first = (y0.max(0.0) / row_h).floor() as usize;
                let last = (y1.max(0.0) / row_h).floor() as usize;
                (first..=last.min(n.saturating_sub(1)))
                    .filter(|i| *i < n)
                    .collect()
            } else {
                Vec::new()
            };
            out.pick = Some(Pick::Rows { rows: picked, add });
            let screen = Rect::from_min_max(
                pos2(x0 - st.hscroll, y0 + body_top - st.scroll),
                pos2(x1 - st.hscroll, y1 + body_top - st.scroll),
            );
            let bp = p.with_clip_rect(winui::rect(body));
            let screen = winui::rect(screen);
            bp.rect_filled(screen, 0.0, crate::theme::colors().marquee);
            bp.rect_stroke(
                screen,
                0.0,
                egui::Stroke::new(1.0, crate::theme::colors().marquee_border),
                egui::StrokeKind::Inside,
            );
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            st.marquee = None;
            out.pick = Some(Pick::BoxDone);
        }
    }
    out
}

/// A cell's text as drawn in column `col`: data keeps its own direction in the interface's — a
/// file name or a description as a first-strong isolate (an Arabic name reads right to left in
/// an English window, a Latin one left to right in an Arabic window), a date left to right. The
/// cut happens inside the isolate, so the "..." ends the name.
/// The width that shows column `col`'s title and every cell whole (its icon, its padding).
fn fit_width(
    p: &egui::Painter,
    font: &egui::FontId,
    cell_font: &egui::FontId,
    col: usize,
    rows: &dyn Rows,
) -> f32 {
    let measure = |f: &egui::FontId, s: &str| winui::text_width_font(p, f.clone(), s);
    // The title, with room for the sort arrow and the filter funnel.
    let title = measure(font, crate::i18n::tc("column", TITLES[col])) + 6.0 + 24.0;
    let lead = if col == 0 { 22.0 } else { 6.0 };
    // The cells in their own font (View → Font): every row's, made for this once.
    let cells = (0..rows.len())
        .map(|i| {
            let r = rows.row(i);
            lead + measure(cell_font, &cell_text(col, crate::i18n::t(&r.cells[col]))) + 8.0
        })
        .fold(0.0, f32::max);
    match col {
        // The queue icon and the source chip have their own sizes.
        1 | SOURCE => DEFAULT_WIDTHS[col],
        _ => title.max(cells).ceil().max(min_width(col)),
    }
}

fn cell_text(col: usize, s: &str) -> String {
    match col {
        0 | 7 => crate::i18n::isolate(s),
        // Dates, a peer count and a ratio read left to right.
        // Dates, a rate, a peer count and a ratio read left to right.
        6 | ADDED | UPLOAD | PEERS | RATIO => crate::i18n::ltr(s),
        _ => s.to_owned(),
    }
}

/// A focus outline: every other pixel of the rectangle's edge (inclusive pixel bounds).
fn dotted(p: &egui::Painter, l: f32, t: f32, r: f32, b: f32, color: Color32) {
    let mut x = l;
    while x <= r {
        fill(p, x, t, x, t, color);
        fill(p, x, b, x, b, color);
        x += 2.0;
    }
    let mut y = t;
    while y <= b {
        fill(p, l, y, l, y, color);
        fill(p, r, y, r, y, color);
        y += 2.0;
    }
}

/// A Source chip: a rounded pill, green "Web" or purple "Torrent", white text, at the cell's
/// left of the cell `x0..x1`, in the row at `top`, `row_h` high, whose text sits on `baseline`
/// below its top. A label too long for the cell (a translation in a narrow column) is cut with
/// "..."; returns whether it was.
fn chip(
    p: &egui::Painter,
    font: &egui::FontId,
    (x0, x1): (f32, f32),
    (top, row_h, baseline): (f32, f32, f32),
    label: &str,
) -> bool {
    if label.is_empty() {
        return false;
    }
    let fill = if label == "Torrent" {
        crate::theme::colors().chip_torrent
    } else {
        crate::theme::colors().chip_web
    };
    // The cell holds the English key (filters match on it); the chip shows it translated.
    let full = crate::i18n::t(label);
    // Room: the pill starts 5 px in, pads its text 6 px each side, and ends 3 px before the cell.
    let label = &winui::ellipsize(p, font, full, (x1 - x0 - 5.0 - 12.0 - 3.0).max(0.0));
    let w = winui::text_width_font(p, font.clone(), label).round();
    let pill = Rect::from_min_size(pos2(x0 + 5.0, top + 2.0), egui::vec2(w + 12.0, row_h - 4.0));
    p.rect_filled(winui::rect(pill), (row_h - 4.0) / 2.0, fill);
    text_font(
        p,
        font.clone(),
        pill.left() + 6.0,
        top + baseline,
        label,
        crate::theme::colors().chip_text,
    );
    label != full
}

/// Where a column's funnel sits in its header: at the right, left of the sort chevron.
fn funnel_rect(x0: f32, x1: f32, ct: f32, arrow: bool) -> Rect {
    let right = x1 - if arrow { 18.0 } else { 4.0 };
    let left = (right - 12.0).max(x0 + 2.0);
    Rect::from_min_max(pos2(left, ct + 6.0), pos2(right, ct + header_h() - 6.0))
}

/// A small funnel: a triangle over a stem, in English rect `rc`.
fn funnel(p: &egui::Painter, rc: Rect, color: Color32) {
    let rc = winui::rect(rc);
    let (cx, top) = (rc.center().x, rc.top() + 1.0);
    p.add(egui::Shape::convex_polygon(
        vec![
            pos2(cx - 5.0, top),
            pos2(cx + 5.0, top),
            pos2(cx + 1.0, top + 5.0),
            pos2(cx - 1.0, top + 5.0),
        ],
        color,
        egui::Stroke::NONE,
    ));
    winui::fill(p, cx - 1.0, top + 5.0, cx, top + 10.0, color);
}

const FILTER_W: f32 = 230.0;

/// The open filter box under a header: a checkbox per value, or a "contains" search box; it
/// filters as you change it. Escape or a click elsewhere closes it.
fn filter_box(ui: &mut Ui, st: &mut ListState) {
    let Some((col, at)) = st.filter_open else {
        return;
    };
    let values = filter_values(col);
    let h = match values {
        Some(v) => 58.0 + 18.0 * v.len() as f32,
        None => 92.0,
    };
    // Laid out in English under its header (`at`); shown where that is on screen.
    let rect = Rect::from_min_size(at, egui::vec2(FILTER_W, h));
    let shown = winui::rect(rect);
    let r = winui::rect;
    let mut close = ui.input(|i| i.key_pressed(egui::Key::Escape));
    // A press anywhere else closes it (the toolbar, the menu bar, another header too), except
    // on its own funnel, whose click toggles it; so does the list scrolling sideways (it
    // stayed where the column was).
    let on_funnel =
        |pp: Pos2| pp.y <= at.y && pp.y >= at.y - header_h() && (pp.x - at.x).abs() <= 24.0;
    if ui.input(|i| i.pointer.any_pressed())
        && ui
            .ctx()
            .pointer_interact_pos()
            .is_some_and(|pp| !shown.contains(pp) && !on_funnel(pp))
    {
        close = true;
    }
    if st.hscroll != st.filter_hscroll {
        close = true;
    }
    egui::Area::new(egui::Id::unique(("list-filter", col)))
        .order(egui::Order::Foreground)
        .fixed_pos(shown.min)
        .show(ui.ctx(), |ui| {
            let (_, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
            let p = ui.painter().clone();
            crate::chrome::popup_surface(&p, shown);
            let o = rect.min;
            match values {
                Some(vals) => {
                    // i18n: verb: label before the filter's list, i.e. show the rows that…
                    winui::text(
                        &p,
                        o.x + 10.0,
                        o.y + 20.0,
                        crate::i18n::t("Show"),
                        winui::TEXT(),
                    );
                    let mut keep: Vec<&'static str> = match &st.filters[col] {
                        Some(Filter::Values(k)) => k.clone(),
                        _ => vals.to_vec(),
                    };
                    for (i, v) in vals.iter().enumerate() {
                        let mut on = keep.contains(v);
                        let y = o.y + 28.0 + 18.0 * i as f32;
                        if winui::checkbox(ui, pos2(o.x + 12.0, y), &mut on, v, true).clicked() {
                            keep.retain(|k| k != v);
                            if on {
                                keep.push(v);
                            }
                        }
                    }
                    st.filters[col] = (keep.len() < vals.len()).then(|| {
                        Filter::Values(vals.iter().copied().filter(|v| keep.contains(v)).collect())
                    });
                    // As wide as its label in this language (#192), short of Close.
                    let w = winui::button_fit_width(
                        ui.painter(),
                        "Show all",
                        90.0,
                        FILTER_W - 90.0 - 8.0 - 10.0,
                    );
                    if winui::button(
                        ui,
                        r(Rect::from_min_size(
                            pos2(o.x + 10.0, o.y + h - 28.0),
                            egui::vec2(w, 21.0),
                        )),
                        "Show all",
                        winui::ButtonKind::Normal,
                        false,
                        None,
                    )
                    .clicked()
                    {
                        st.filters[col] = None;
                    }
                }
                None => {
                    let label = crate::i18n::fmt(
                        crate::i18n::t("{column} contains"),
                        &[("column", crate::i18n::tc("column", TITLES[col]))],
                    );
                    winui::text(&p, o.x + 10.0, o.y + 20.0, &label, winui::TEXT());
                    let mut t = match &st.filters[col] {
                        Some(Filter::Text(t)) => t.clone(),
                        _ => String::new(),
                    };
                    let edit = winui::edit(
                        ui,
                        r(Rect::from_min_size(
                            pos2(o.x + 10.0, o.y + 28.0),
                            egui::vec2(FILTER_W - 20.0, 21.0),
                        )),
                        &mut t,
                        true,
                        false,
                    );
                    if !edit.has_focus() && !ui.ctx().memory(|m| m.focused().is_some()) {
                        edit.request_focus();
                    }
                    st.filters[col] = (!t.is_empty()).then_some(Filter::Text(t));
                    if winui::button(
                        ui,
                        r(Rect::from_min_size(
                            pos2(o.x + 10.0, o.y + h - 30.0),
                            egui::vec2(90.0, 21.0),
                        )),
                        // i18n: verb: clears this filter
                        "Clear",
                        winui::ButtonKind::Normal,
                        false,
                        None,
                    )
                    .clicked()
                    {
                        st.filters[col] = None;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        close = true;
                    }
                }
            }
            if winui::button(
                ui,
                r(Rect::from_min_size(
                    pos2(
                        o.x + FILTER_W - 90.0,
                        o.y + h - if values.is_some() { 28.0 } else { 30.0 },
                    ),
                    egui::vec2(80.0, 21.0),
                )),
                // i18n: verb: closes the filter box
                "Close",
                winui::ButtonKind::Default,
                false,
                None,
            )
            .clicked()
            {
                close = true;
            }
        });
    if close {
        st.filter_open = None;
    }
}

/// The x of the column boundary nearest to `x` (where a dragged column would be inserted).
fn drop_x(cols: &[(usize, f32, f32)], x: f32) -> f32 {
    let mut best = cols.first().map_or(x, |c| c.1);
    for (_, x0, x1) in cols {
        for edge in [*x0, x1 + 1.0] {
            if (edge - x).abs() < (best - x).abs() {
                best = edge;
            }
        }
    }
    best
}

/// Move column `col` to the boundary nearest `x` (within the visible columns' order).
fn move_column(st: &mut ListState, cols: &[(usize, f32, f32)], col: usize, x: f32) {
    let edge = drop_x(cols, x);
    // Insertion index among visible columns: number of columns whose start is left of the edge.
    let mut target = cols.iter().filter(|(_, x0, _)| *x0 < edge).count();
    let from = cols.iter().position(|c| c.0 == col).unwrap_or(0);
    if target > from {
        target -= 1;
    }
    let mut shown: Vec<usize> = cols.iter().map(|c| c.0).collect();
    let moved = shown.remove(from);
    shown.insert(target.min(shown.len()), moved);
    // Hidden columns keep their places beside the visible column before them (they move with
    // it), those before the first visible one stay first: each shows again where it was.
    let mut leading = Vec::new();
    let mut after: Vec<(usize, Vec<usize>)> = Vec::new();
    for &c in &st.order {
        if st.is_shown(c) {
            after.push((c, Vec::new()));
        } else if let Some((_, tail)) = after.last_mut() {
            tail.push(c);
        } else {
            leading.push(c);
        }
    }
    let mut order = leading;
    for v in shown {
        order.push(v);
        if let Some((_, tail)) = after.iter().find(|(c, _)| *c == v) {
            order.extend(tail);
        }
    }
    if order.len() == st.order.len() {
        st.order = order;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_cells_keep_their_own_direction() {
        const FSI: char = '\u{2068}';
        const LRI: char = '\u{2066}';
        const PDI: char = '\u{2069}';
        // A file name and a description: first-strong isolates, whatever their script.
        assert_eq!(cell_text(0, "تقرير.pdf"), format!("{FSI}تقرير.pdf{PDI}"));
        assert_eq!(cell_text(0, "report.zip"), format!("{FSI}report.zip{PDI}"));
        assert_eq!(cell_text(7, "ملاحظة"), format!("{FSI}ملاحظة{PDI}"));
        // Dates read left to right.
        assert_eq!(
            cell_text(ADDED, "Oct 29 2026 20:00:00"),
            format!("{LRI}Oct 29 2026 20:00:00{PDI}")
        );
        assert_eq!(cell_text(6, "Oct 29"), format!("{LRI}Oct 29{PDI}"));
        // Interface text (a status) is laid out in the interface's direction as it is.
        assert_eq!(cell_text(3, "Complete"), "Complete");
        // A cut keeps the "..." inside the isolate: the closing mark goes, the dots follow the
        // name, not the paragraph.
        let cut: String = cell_text(0, "abcdef").chars().take(4).collect::<String>() + "...";
        assert_eq!(cut, format!("{FSI}abc..."));
    }

    fn spans(st: &ListState) -> Vec<(usize, f32, f32)> {
        let mut x = 0.0;
        st.shown()
            .into_iter()
            .map(|c| {
                let s = (c, x, x + st.widths[c] - 1.0);
                x += st.widths[c];
                s
            })
            .collect()
    }

    #[test]
    fn moving_a_column_right_and_left() {
        let mut st = ListState::default();
        let cols = spans(&st);
        // Drop "File Name" (slot 0) at the boundary after "Size" (x = 150 + 20 + 75 = 245); Source,
        // hidden by default, stays first.
        move_column(&mut st, &cols, 0, 246.0);
        assert_eq!(st.order[..5], [SOURCE, 1, 2, 0, 3]);
        let cols = spans(&st);
        // And to the very left of the shown ones.
        move_column(&mut st, &cols, 0, 1.0);
        assert_eq!(st.order[..3], [SOURCE, 0, 1]);
        assert_eq!(st.shown()[0], 0);
    }

    fn row(source: &str, status: &'static str, name: &str, queued: bool) -> Row {
        let mut cells: [String; COLS] = Default::default();
        cells[0] = name.into();
        cells[SOURCE] = source.into();
        Row {
            status_group: status,
            icon: IconKey::Silk("page_white"),
            queued,
            cells,
        }
    }

    /// A list of 10,000 rows makes only the rows it draws each frame (it made every row's
    /// texts, 40 ms a frame), and its wheel glides it a part of a row at a time, drawn there.
    #[test]
    fn a_long_list_makes_only_the_rows_in_view() {
        struct Counted {
            n: usize,
            made: std::cell::Cell<usize>,
        }
        impl Rows for Counted {
            fn len(&self) -> usize {
                self.n
            }
            fn row(&self, i: usize) -> Row {
                self.made.set(self.made.get() + 1);
                row("Web", "Complete", &format!("file-{i:05}.zip"), false)
            }
        }
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        winui::install_fonts(&mut defs, None);
        ctx.set_fonts(defs);
        let rows = Counted {
            n: 10_000,
            made: std::cell::Cell::new(0),
        };
        let selected = vec![false; rows.n];
        let mut st = ListState::default();
        let mut t = 0.0;
        let mut frame = |st: &mut ListState, events: Vec<egui::Event>| {
            t += 1.0 / 60.0;
            rows.made.set(0);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    egui::vec2(900.0, 600.0),
                )),
                time: Some(t),
                predicted_dt: 1.0 / 60.0,
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let outer = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(800.0, 400.0));
                show(
                    ui,
                    outer,
                    st,
                    &rows,
                    &selected,
                    None,
                    &mut ShellIcons,
                    &|_| String::new(),
                );
            });
            out.textures_delta.clear();
            rows.made.get()
        };
        let made = frame(&mut st, vec![egui::Event::PointerMoved(pos2(300.0, 200.0))]);
        // The rows' room: the box less its frame and the header.
        let page = ((400.0 - 4.0 - header_h()) / st.row_h()).ceil() as usize;
        assert!(
            (page..=page + 3).contains(&made),
            "{made} rows made for a view of about {page}"
        );
        // A notch of the wheel: the list is part of the way there after a frame (between rows),
        // all of it after a few, on a whole row.
        let notch = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        };
        frame(&mut st, vec![notch]);
        let row_h = st.row_h();
        assert!(st.scroll > 0.0 && st.scroll % row_h != 0.0, "{}", st.scroll);
        for _ in 0..40 {
            assert!(frame(&mut st, vec![]) <= page + 3);
        }
        assert!(st.scroll > 0.0 && st.scroll % row_h == 0.0, "{}", st.scroll);
    }

    #[test]
    fn filters_by_values_and_text() {
        let mut st = ListState::default();
        // Source shown (off by default): a hidden column's filter filters nothing.
        st.set_visible(SOURCE, true);
        let rows = [
            row("Web", "Complete", "Setup.EXE", false),
            row("Torrent", "Downloading", "debian.iso", true),
            row("Web", "Error", "notes.pdf", false),
        ];
        let shown = |st: &ListState| rows.iter().filter(|r| matches(st, r)).count();
        assert_eq!(shown(&st), 3);
        st.filters[SOURCE] = Some(Filter::Values(vec!["Torrent"]));
        assert_eq!(shown(&st), 1);
        st.filters[SOURCE] = None;
        st.filters[3] = Some(Filter::Values(vec!["Complete", "Error"]));
        assert_eq!(shown(&st), 2);
        st.filters[0] = Some(Filter::Text("setup".into()));
        assert_eq!(shown(&st), 1, "case-insensitive, combined with the others");
        st.filters = vec![None; COLS];
        st.filters[1] = Some(Filter::Values(vec!["In a queue"]));
        assert_eq!(shown(&st), 1);
    }

    /// The slots are fixed, so a title inserted in the middle of [`TITLES`] would name the wrong
    /// column — the screenshots caught exactly that.
    #[test]
    fn each_titles_slot_names_its_own_column() {
        assert_eq!(TITLES[0], "File Name");
        assert_eq!(TITLES[6], "Last Try Date");
        assert_eq!(TITLES[7], "Description");
        assert_eq!(TITLES[ADDED], "Added");
        assert_eq!(TITLES[SOURCE], "Source");
        assert_eq!(TITLES[UPLOAD], "Upload speed");
        assert_eq!(TITLES[PEERS], "Peers");
        assert_eq!(TITLES[RATIO], "Share ratio");
    }

    /// View → Font: each size larger than the one before, its rows never lower than the theme's
    /// (the 16-point icons fill them), and Normal exactly the theme's look.
    #[test]
    fn font_sizes_grow_in_order_and_rows_with_them() {
        let theme = crate::theme::metrics().list_row;
        let mut st = ListState::default();
        assert_eq!(st.font, FontSize::Normal);
        assert_eq!(st.row_h(), theme);
        assert_eq!(FontSize::Normal.grow(), 0.0);
        assert_eq!(FontSize::Normal.baseline_shift(), 0.0);
        assert_eq!(FontSize::Normal.icon_shift(), 0.0);
        let mut last: Option<(f32, f32)> = None;
        for size in FontSize::ALL {
            st.font = size;
            assert!(st.row_h() >= theme, "{size:?}");
            // The text stays inside its row, above the grid line.
            assert!(12.0 + size.baseline_shift() < st.row_h() - 1.0, "{size:?}");
            assert!(size.icon_shift() + 16.0 <= st.row_h() - 1.0, "{size:?}");
            if let Some((grow, row)) = last {
                assert!(size.grow() > grow && st.row_h() >= row, "{size:?}");
            }
            last = Some((size.grow(), st.row_h()));
            assert_eq!(FontSize::from_key(size.key()), Some(size));
        }
        assert_eq!(FontSize::from_key("huge"), None);
        // Comfortable rows are taller, their text and icons still in their middle.
        st.font = FontSize::Normal;
        st.density = crate::look::Density::Comfortable;
        assert_eq!(st.row_h(), theme + 6.0);
        assert_eq!(st.density_shift(), 3.0);
        assert!(12.0 + st.density_shift() < st.row_h() - 1.0);
    }

    #[test]
    fn hidden_columns_keep_their_place() {
        let mut st = ListState::default();
        st.visible[1] = false; // hide "Q"
        let cols = spans(&st);
        move_column(&mut st, &cols, 7, 0.0); // Description to the front
        assert_eq!(st.shown()[0], 7);
        assert!(st.order.contains(&1));
        assert_eq!(st.order.len(), COLS);
        // Q kept its place after File Name: shown again, it is back there.
        let q = st.order.iter().position(|c| *c == 1).unwrap();
        assert_eq!(st.order[q - 1], 0);
        assert_eq!(st.order[q + 1], 2);
    }

    /// The owner's report: "when I remove the columns IDM behaves better". Hidden columns close
    /// up with their widths kept (nothing else resizes), and take their sort and filter with
    /// them; File Name always shows; the layout keeps them hidden after a restart.
    #[test]
    fn hiding_columns_closes_them_up_and_drops_their_sort_and_filter() {
        let mut st = ListState {
            sort: Some((5, true)),
            ..ListState::default()
        };
        st.filters[4] = Some(Filter::Text("min".into()));
        st.filter_open = Some((4, pos2(0.0, 0.0)));
        let widths = st.widths;
        for c in [4, 5, 7] {
            st.set_visible(c, false);
        }
        assert_eq!(st.widths, widths, "no width changes");
        assert_eq!(st.sort, None, "not sorted by a hidden column");
        assert_eq!(st.filters[4], None);
        assert_eq!(st.filter_open, None);
        // The others close up, left to right, in their order, each its own width.
        let cols = spans(&st);
        let shown: Vec<usize> = cols.iter().map(|c| c.0).collect();
        assert_eq!(shown, [0, 1, 2, 3, 6]);
        for w in cols.windows(2) {
            assert_eq!(
                w[0].2 + 1.0,
                w[1].1,
                "no gap between {:?} and {:?}",
                w[0],
                w[1]
            );
        }
        for (c, x0, x1) in &cols {
            assert_eq!(x1 - x0 + 1.0, widths[*c]);
        }
        // File Name can't be hidden.
        st.set_visible(0, false);
        assert!(st.visible[0]);
        // Kept with the list: hidden after a restart, sort and all.
        let mut back = ListState::default();
        back.apply(st.layout());
        assert_eq!(back.visible, st.visible);
        assert_eq!(back.widths, st.widths);
        assert_eq!(back.order, st.order);
        // A sort saved on a column that is hidden isn't taken back.
        let mut l = st.layout();
        l.sort = Some((4, false));
        back.apply(l);
        assert_eq!(back.sort, None);
    }

    #[test]
    fn a_layout_saved_before_the_added_columns_still_loads() {
        let old = Layout {
            order: vec![0, 2, 1, 3, 4, 5, 6, 7],
            widths: vec![200.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 300.0],
            visible: vec![true; 8],
            sort: Some((2, false)),
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(old);
        // Source first, the three torrent columns after Transfer rate, Added after Last Try Date.
        assert_eq!(
            st.order,
            [SOURCE, 0, 2, 1, 3, 4, 5, UPLOAD, PEERS, RATIO, 6, ADDED, 7]
        );
        assert_eq!(st.widths[0], 200.0, "the saved widths stay");
        assert_eq!(st.widths[ADDED], DEFAULT_WIDTHS[ADDED]);
        // A layout saved before they existed (and changed: Q moved) gets their defaults: Added
        // and Source offered but off, the torrent columns following the view.
        assert!(!st.is_shown(ADDED) && !st.is_shown(SOURCE));
        assert_eq!(st.widths[UPLOAD], DEFAULT_WIDTHS[UPLOAD]);
        assert_eq!(st.widths[PEERS], DEFAULT_WIDTHS[PEERS]);
        assert_eq!(st.widths[RATIO], DEFAULT_WIDTHS[RATIO]);
        assert!(st.auto[UPLOAD] && st.auto[PEERS] && st.auto[RATIO]);
        assert!(
            (0..8).all(|c| st.is_shown(c)),
            "its own columns as it had them"
        );
        assert_eq!(st.sort, Some((2, false)));
    }

    /// View → Arrange files → By Save Path / Referer / Parent Web Page: kept with the layout and
    /// read back; a slot the list doesn't know reads as unsorted.
    #[test]
    fn the_arrange_orders_without_a_column_are_kept() {
        let mut st = ListState::default();
        for (saved, read) in [
            (Some((BY_PAGE, true)), Some((BY_PAGE, true))),
            (Some((BY_SAVE_PATH, false)), Some((BY_SAVE_PATH, false))),
            (Some((99, true)), None),
        ] {
            let mut l = st.layout();
            l.sort = saved;
            st.apply(l);
            assert_eq!(st.sort, read);
        }
    }

    /// #357: a layout saved between "Source" and the torrent columns gets them in the same
    /// place, and a layout that names a column twice is still ignored.
    #[test]
    fn a_layout_saved_before_the_torrent_columns_gets_them() {
        let old = Layout {
            order: vec![SOURCE, 0, 1, 2, 3, 4, 5, 6, ADDED, 7],
            widths: vec![
                62.0, 150.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 125.0, 300.0,
            ],
            visible: vec![true; 10],
            sort: None,
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(old);
        assert_eq!(
            st.order,
            [SOURCE, 0, 1, 2, 3, 4, 5, UPLOAD, PEERS, RATIO, 6, ADDED, 7]
        );
        assert!(st.widths[UPLOAD] > 0.0 && st.widths[PEERS] > 0.0 && st.widths[RATIO] > 0.0);
        // None of the torrent columns were in that layout, so all three get their defaults.
        assert_eq!(st.widths[UPLOAD], DEFAULT_WIDTHS[UPLOAD]);
        // A layout that names one column twice (and so misses another) is ignored whole.
        let bad = Layout {
            order: vec![SOURCE, 0, 1, 2, 3, 4, 5, 6, ADDED, 7, PEERS, PEERS],
            widths: vec![62.0; 12],
            visible: vec![true; 12],
            sort: None,
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(bad);
        assert_eq!(st.order, DEFAULT_ORDER.to_vec(), "the default stands");
    }

    /// A layout saved by 0.5.2 (12 slots: Peers at 10, Share ratio at 11, sorted by Peers) must
    /// keep meaning what it meant: Peers is still Peers, with its own width and its sort. Upload
    /// speed takes the new, free slot and gets its default.
    #[test]
    fn a_layout_saved_by_052_keeps_peers_as_peers() {
        let old = Layout {
            order: vec![SOURCE, 0, 1, 2, 3, 4, 5, PEERS, RATIO, 6, ADDED, 7],
            widths: vec![
                150.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 300.0, 145.0, 62.0, 64.0, 92.0,
            ],
            visible: vec![true; 12],
            sort: Some((PEERS, true)),
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(old);
        assert_eq!(
            st.order,
            [SOURCE, 0, 1, 2, 3, 4, 5, UPLOAD, PEERS, RATIO, 6, ADDED, 7]
        );
        assert_eq!(
            st.widths[PEERS], 64.0,
            "Peers keeps the width 0.5.2 saved for it"
        );
        assert_eq!(st.widths[RATIO], 92.0, "Share ratio keeps its own");
        assert_eq!(
            st.widths[UPLOAD], DEFAULT_WIDTHS[UPLOAD],
            "the new slot gets its default"
        );
        assert_eq!(
            st.sort,
            Some((PEERS, true)),
            "a list sorted by Peers is still sorted by Peers"
        );
        assert_eq!(st.widths[SOURCE], 62.0, "the saved widths stay");
        assert_eq!(st.widths[ADDED], 145.0);
    }

    /// The lead's finding: the default list showed thirteen columns, the torrent ones empty for
    /// every web download. IDM's calm set by default; the torrent columns in the Torrents view.
    #[test]
    fn the_default_columns_are_idms_and_torrent_ones_follow_the_view() {
        let mut st = ListState::default();
        assert_eq!(st.shown(), [0, 1, 2, 3, 4, 5, 6, 7]);
        st.torrent_view = true;
        assert_eq!(st.shown(), [0, 1, 2, 3, 4, 5, UPLOAD, PEERS, RATIO, 6, 7]);
        // Hidden in the Torrents view: the person's choice, in every view from then on.
        st.set_visible(PEERS, false);
        assert!(!st.is_shown(PEERS));
        st.torrent_view = false;
        assert!(!st.is_shown(PEERS));
        // Shown elsewhere: shown everywhere; the other two still follow the view.
        st.set_visible(RATIO, true);
        assert!(st.is_shown(RATIO) && !st.is_shown(UPLOAD));
        // Kept with the list.
        let mut back = ListState::default();
        back.apply(st.layout());
        assert_eq!((back.visible, back.auto), (st.visible, st.auto));
        // Reset: today's defaults, the torrent columns following the view again.
        st.reset_columns();
        assert_eq!((st.visible, st.auto), (DEFAULT_VISIBLE, DEFAULT_AUTO));
    }

    /// Who gets the new defaults: a layout the person never changed (0.6.5's own: every column
    /// shown at its first width, in the first order; sorting doesn't count) does; one they
    /// changed (a width, the order, a column hidden) keeps theirs, every column as it showed.
    #[test]
    fn untouched_layouts_get_the_new_defaults_changed_ones_stay() {
        let v065 = Layout {
            order: DEFAULT_ORDER.to_vec(),
            widths: DEFAULT_WIDTHS.to_vec(),
            visible: vec![true; COLS],
            sort: Some((2, true)),
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(v065.clone());
        assert_eq!((st.visible, st.auto), (DEFAULT_VISIBLE, DEFAULT_AUTO));
        assert_eq!(st.sort, Some((2, true)), "its sort stays");
        // An older list, before Source and the torrent columns (Added 125 wide then).
        let older = Layout {
            order: vec![0, 1, 2, 3, 4, 5, 6, ADDED, 7],
            widths: vec![150.0, 20.0, 75.0, 65.0, 75.0, 95.0, 65.0, 200.0, 125.0],
            visible: vec![true; 9],
            sort: None,
            auto: Vec::new(),
        };
        let mut st = ListState::default();
        st.apply(older);
        assert_eq!(st.shown(), [0, 1, 2, 3, 4, 5, 6, 7]);
        // Changed: a width, the order, a column hidden. Each keeps every column as it was.
        let mut wider = v065.clone();
        wider.widths[0] = 260.0;
        let mut moved = v065.clone();
        moved.order.swap(1, 2);
        let mut hidden = v065.clone();
        hidden.visible[4] = false;
        for l in [wider, moved, hidden.clone()] {
            let mut st = ListState::default();
            st.apply(l.clone());
            assert!(st.auto.iter().all(|a| !a), "nothing follows the view");
            assert_eq!(st.visible.to_vec(), l.visible);
            assert!(st.is_shown(SOURCE) && st.is_shown(PEERS));
        }
        // A layout this version saved says itself what follows the view: kept as it is.
        let mut st = ListState::default();
        st.apply(hidden);
        let saved = st.layout();
        let mut back = ListState::default();
        back.apply(saved);
        assert_eq!((back.visible, back.auto), (st.visible, st.auto));
    }

    /// The owner's request: no funnel at rest, the title first. The funnel shows under the
    /// pointer or on a filtered column; what gives way when room lacks: a funnel only hovered,
    /// then the sort arrow, then a filtered column's funnel; the title is cut only after.
    #[test]
    fn header_parts_give_the_title_the_room_first() {
        let title = 50.0;
        let roomy = 120.0;
        // At rest: the title alone, or with its arrow on the sorted column.
        assert_eq!(
            header_parts(roomy, title, false, false, false),
            HeaderParts {
                funnel: false,
                arrow: false
            }
        );
        assert_eq!(
            header_parts(roomy, title, true, false, false),
            HeaderParts {
                funnel: false,
                arrow: true
            }
        );
        // Under the pointer, or filtered: the funnel too.
        assert_eq!(
            header_parts(roomy, title, true, false, true),
            HeaderParts {
                funnel: true,
                arrow: true
            }
        );
        assert_eq!(
            header_parts(roomy, title, false, true, false),
            HeaderParts {
                funnel: true,
                arrow: false
            }
        );
        // Room for the title and one part: a hovered funnel gives way to the arrow; a filtered
        // column's funnel stays and the arrow goes.
        let one = title + HEADER_PAD + PART_W;
        assert_eq!(
            header_parts(one, title, true, false, true),
            HeaderParts {
                funnel: false,
                arrow: true
            }
        );
        assert_eq!(
            header_parts(one, title, true, true, false),
            HeaderParts {
                funnel: true,
                arrow: false
            }
        );
        // Room for the title alone: nothing beside it; never over it.
        let bare = title + HEADER_PAD;
        assert_eq!(
            header_parts(bare, title, true, true, true),
            HeaderParts {
                funnel: false,
                arrow: false
            }
        );
        // A drag can't make a column unreadable: each has its least width.
        assert!((0..COLS).all(|c| min_width(c) >= 20.0));
        assert!(min_width(0) >= 60.0);
    }
}
