//! The per-download progress dialog, painted to IDM 6.42's pixels
//! (notes/ui-spec/progress-dialog.md). Coordinates are client pixels from the spec.

use egui::{Color32, Pos2, Ui, ViewportBuilder, ViewportId, vec2};
use mdm_engine::Snapshot;
use mdm_engine::download::ConnStatus;

use crate::dialogs::display_path;
use crate::model::{self, Item, Status, size3};
// Laid out in English pixels: `fill` and `px` make each one where it is on screen (bars fill
// from the right in a right-to-left interface), `text` starts at an English pen.
use crate::winui::{self, ButtonKind, fill_l as fill, px_l as px, r, text};

use crate::winui::caption::CaptionButton;

mod connections;
mod glide;
mod limits;
mod status;
mod torrent;

pub use self::glide::*;

// What the tabs share (each takes it, and this module's imports, with `use super::*`).
use self::connections::*;
use self::status::*;
use self::torrent::*;

// What the torrent's Files window uses too (torrentfiles.rs).
pub(crate) use self::torrent::{files_table, table_body_height};

/// The drawn title bar's height (#63): Windows only, where the frame is ours
/// (win32::progress_frame); Linux and macOS keep the system's decorations, which take no buttons
/// of ours (Wayland even places them itself), so the hide button sits in the tab row there.
const TITLE_H: f32 = if cfg!(windows) {
    crate::winui::caption::CAPTION_H
} else {
    0.0
};

/// Extra height added to the top tab pane for the deferred Rename row on Download status.
const RENAME_DROP: f32 = 28.0;

/// The speed graph's band (beyond IDM), between the tab page and the progress bar: the bar, the
/// buttons and the details under it are lower by this much, the window as much taller.
const GRAPH_BAND: f32 = crate::speedgraph::BAND;

/// The frame applied to a progress window (Windows): its client size, pixels per point × 1000
/// and its system menu's "Hide to the notification area" (translated, when there is one).
type Framed = ([f32; 2], u32, Option<String>);

/// The window's title: IDM truncates the percentage in it (44.51 % → "44%"); without a size to
/// take a share of, only the name. The name keeps its own direction (an Arabic one doesn't pull
/// the percentage after it).
fn window_title(frac: Option<f64>, name: &str) -> String {
    let name = crate::i18n::isolate(name);
    match frac {
        Some(f) => format!("{}% {name}", (f * 100.0).floor() as u32),
        None => name,
    }
}

/// How a download's progress window opens when the person starts or resumes it (Options →
/// Downloads → Customize "Download progress" dialog, as IDM's): shown, minimized, or hidden in
/// the notification area. A queue's downloads open none (they start hidden: the row's Show
/// progress or the tray brings one up), whatever this says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressView {
    #[default]
    Normal,
    Minimized,
    Hidden,
}

/// How a progress window just asked for opens ([`opens`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    Shown,
    Minimized,
    /// Hidden in the notification area: the tray's Downloads in progress, the row's Show
    /// progress or a double-click set to Open brings it up.
    Hidden,
}

/// How a progress window opens for start view `view`: hidden only where a tray shows DDM's icon
/// to bring it back from (`tray`), minimized otherwise, never out of reach.
pub fn opens(view: ProgressView, tray: bool) -> Opens {
    match view {
        ProgressView::Normal => Opens::Shown,
        ProgressView::Minimized => Opens::Minimized,
        ProgressView::Hidden if tray => Opens::Hidden,
        ProgressView::Hidden => Opens::Minimized,
    }
}

/// What the window's × (and the system's close: Alt+F4, the taskbar's) does, as IDM's: a
/// download under way is stopped (resumable, as Cancel does); a finished, stopped or failed one's
/// window just closes, as does a torrent's that shares after finishing (its sharing goes on).
/// Hiding the window while the download goes on is the title bar's "Hide to the notification
/// area" button, or Minimize.
fn closing_action(item: &Item) -> Action {
    if item.live.is_some() && item.status != Status::Complete {
        Action::Cancel
    } else {
        Action::Close
    }
}

/// The title bar's buttons, from the window's end: Close, Maximize (greyed: a fixed size, as
/// IDM's), Minimize, and with a tray our "hide to the notification area". None where the system
/// draws the title bar.
fn caption_buttons(tray: bool) -> Vec<(CaptionButton, bool)> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut b = vec![
        (CaptionButton::Close, true),
        (CaptionButton::Maximize, false),
        (CaptionButton::Minimize, true),
    ];
    if tray {
        b.push((CaptionButton::ToTray, true));
    }
    b
}

/// The title bar where it's ours (Windows), `width` points wide at `o`; the button pressed. Where
/// its buttons and icon are goes to the window's hit test (win32::set_caption_hits), in client
/// pixels, each frame (`hwnd`: the window, once its frame is applied).
fn draw_caption(
    ui: &mut Ui,
    o: Pos2,
    width: f32,
    title: &str,
    buttons: &[(CaptionButton, bool)],
    hwnd: Option<isize>,
) -> Option<CaptionButton> {
    if !cfg!(windows) {
        return None;
    }
    // The one drawn title bar of every window (dialogs.rs), with its hit test.
    crate::dialogs::drawn_title(ui, o, width, title, buttons, hwnd)
}

/// A greyed label (the limits' captions while off).
fn disabled() -> Color32 {
    crate::theme::colors().progress_label_disabled
}

/// The segment / piece bar's 13 rows, top to bottom: (not here yet, here, a connection's
/// start), from the theme (IDM's gradients in the classic look; its dark variant dims the
/// "not here yet" rows, a bright white strip in the dark window).
fn seg_rows() -> impl Iterator<Item = (Color32, Color32, Color32)> {
    let s = &crate::theme::colors().segments;
    (0..13).map(|i| (s.missing[i], s.done[i], s.start[i]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Status,
    Limiter,
    Completion,
}

impl Progress {
    /// Open on a tab by its English name in lower case: a top tab ("status", "limit", "done") or
    /// a torrent's details tab ("peers"; /show progress:peers, for the docs' screenshots and the
    /// fit audit); any other name leaves it as it is. A top tab hidden with "Hide tab" comes back.
    pub fn show_tab(&mut self, name: &str) {
        let top = match name {
            "status" => Some(Tab::Status),
            "limit" => Some(Tab::Limiter),
            "done" => Some(Tab::Completion),
            _ => None,
        };
        if let Some(t) = top {
            self.hidden_tabs.retain(|h| *h != t);
            self.tab = t;
            return;
        }
        let tab = match name {
            "files" => TTab::Files,
            "peers" => TTab::Peers,
            "trackers" => TTab::Trackers,
            "pieces" => TTab::Pieces,
            "general" => TTab::General,
            _ => return,
        };
        self.ttab = tab;
        self.details = true;
    }
}

/// A torrent's details tabs (M15, D-014), in the place of the connections table.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TTab {
    Files,
    Peers,
    Trackers,
    Pieces,
    General,
}

// "When done" (a download's, in its progress window; a queue's, in the Scheduler): the same words
// in both windows.
/// Ends the dial-up (RAS) connection, through Windows' rasdial.
// i18n: checkbox (progress window's When done tab, the Scheduler): end the dial-up connection once the download or the queue is done
pub const HANG_UP: &str = crate::i18n::mark("End the dial-up connection when done");
// i18n: checkbox (progress window's When done tab, the Scheduler): quit the program once the download or the queue is done
pub const EXIT_WHEN_DONE: &str = crate::i18n::mark("Close Dannex Download Manager when done");
// i18n: checkbox (progress window's When done tab, the Scheduler), followed by a drop-down: Shut down, Sleep or Hibernate
pub const TURN_OFF: &str = crate::i18n::mark("When done, turn off the computer");
// i18n: checkbox under "When done, turn off the computer": programs that would hold the shutdown up (asking to save) are closed anyway
pub const FORCE: &str = crate::i18n::mark("Don't wait for programs that hold it up");
// i18n: checkbox (Options → Downloads, the progress window's When done tab): show the Download complete window when a file finishes
pub const SHOW_COMPLETE: &str = crate::i18n::mark("Show the Download complete window");

// The Speed Limiter tab's "remember" checkbox, its caption in two lines.
// i18n: progress window's Speed Limiter tab: first line of a checkbox's caption (the second: "is stopped and resumed"): this limit stays with the file
const REMEMBER_1: &str = crate::i18n::mark("Keep this limit when the download");
// i18n: progress window's Speed Limiter tab: second line of the caption that starts "Keep this limit when the download"
const REMEMBER_2: &str = crate::i18n::mark("is stopped and resumed");

/// Why "End the dial-up connection when done" is greyed off Windows.
pub const NO_DIAL_UP: &str = crate::i18n::mark("This system has no dial-up connections to end");

pub struct Progress {
    pub id: u64,
    details: bool,
    tab: Tab,
    /// Tabs taken off the strip with their "Hide tab" button (#326), for this window.
    hidden_tabs: Vec<Tab>,
    ttab: TTab,
    limiter: bool,
    /// KBytes/sec typed on the Speed Limiter tab.
    limit_kb: String,
    /// The tab has been initialised from the download's current limit.
    limit_init: bool,
    /// A limit change to report to the app (`Action::Limit`), and whether to remember it.
    pending_limit: Option<(Option<u64>, bool)>,
    remember: bool,
    /// A torrent's own upload limit on the Speed Limiter tab: on, and its KBytes/sec box.
    up_on: bool,
    up_kb: String,
    /// An upload limit change to report (`Action::UploadLimit`).
    pending_upload: Option<Option<u64>>,
    /// Files ticked or unticked in a torrent's Files tab, to report (`Action::Choose`).
    pending_choose: Option<Action>,
    show_complete: bool,
    /// "Show download complete dialog" was ticked or unticked in this window: this file's own
    /// choice from then on, over Options'.
    complete_chosen: bool,
    /// Torrent settings' "Keep sharing torrents after they finish" (the General tab says so).
    pub keep_sharing: bool,
    /// The torrent session (the app's), for a finished torrent still shared without a `Run`:
    /// its tabs read the session directly.
    pub host: Option<crate::torrents::Host>,
    /// Hidden in the notification area (#63): not shown, its tab and choices kept for when it
    /// comes back (the tray's "Downloads in progress", the row's Show progress, or the download
    /// double-clicked with On Double click → Open).
    pub hidden: bool,
    /// Opens minimized (its start view, [`Opens::Minimized`]): minimized once it is shown.
    start_minimized: bool,
    /// A tray shows DDM's icon, so the window can be hidden there (set by the app each frame).
    pub tray: bool,
    /// Options → General → "Minimize to the notification area": Minimize hides it there too.
    pub minimize_to_tray: bool,
    /// DDM's main window is shown (set by the app each frame): the glide's frames draw it too.
    pub main_shown: bool,
    /// This download's speed second by second, for its speed graph (set by the app each frame;
    /// none before it has run in this session).
    pub speed: Option<crate::speedgraph::History>,
    /// When done, used when "Show download complete dialog" is off.
    hang_up: bool,
    exit: bool,
    turn_off_on: bool,
    turn_off: usize,
    force: bool,
    /// The bar and the segment map between figures.
    glide: Glide,
    /// Deferred rename on Download status: initialised from `Item::rename_on_complete`, saved
    /// by clicking Confirm, pressing Enter, or leaving the box idle for 3 seconds.
    rename_init: bool,
    rename_text: String,
    rename_saved: Option<String>,
    rename_edited_at: Option<std::time::Instant>,
    rename_show_saved: bool,
    pending_rename: Option<Option<String>>,
    /// Translated reason key when deferred rename is unavailable for this download (shared or
    /// save folder, split archive volume, or single-file torrent).
    pub rename_disabled_reason: Option<&'static str>,
    /// In-memory state stamp for `rename_disabled_reason`, so disk checks run only when the
    /// window opens, when a download completes, or when list/folder settings change.
    pub rename_guard_stamp: Option<u64>,
    /// Those disk checks, under way off the window's thread for a newer stamp (#743): a folder
    /// on a sleeping drive or a network share can take seconds to answer. The reason shown
    /// stays the last one meanwhile (the rename is checked again when it is carried out).
    pub rename_guard: Option<(u64, crate::launch::Asked<u64, Option<&'static str>>)>,
}

pub enum Action {
    None,
    Pause,
    Resume,
    /// Stop the download and close the dialog.
    Cancel,
    /// Download with the browser (#376): a download the site's bot check refused goes to the
    /// browser (crate::handback); the dialog closes.
    WithBrowser,
    /// A torrent's video or sound in the browser's player while it downloads (crate::player).
    Play,
    /// Set this download's speed limit (bytes/s, None = off).
    /// This download's limit (None: unlimited); `true`: remember it for the file.
    Limit(Option<u64>, bool),
    /// A torrent's own upload limit (bytes/s, None: off), kept with it (`TorrentSpec`).
    UploadLimit(Option<u64>),
    /// A torrent's files to download from now on (#39, its Files tab): indexes into its list,
    /// whether that is every file, and their size together.
    Choose {
        files: Vec<usize>,
        all: bool,
        size: u64,
    },
    /// Set or clear the deferred rename (`Item::rename_on_complete`).
    Rename(Option<String>),
    /// Close the dialog, keep downloading.
    Close,
    /// Hide the dialog in the notification area, keep downloading (#63).
    Hide,
}

impl Progress {
    /// `show_complete`: Options' "Show download complete dialog", this file's first state.
    pub fn new(id: u64, show_complete: bool) -> Self {
        Progress {
            id,
            details: true,
            tab: Tab::Status,
            hidden_tabs: Vec::new(),
            ttab: TTab::Files,
            limiter: false,
            limit_kb: "10".into(),
            limit_init: false,
            pending_limit: None,
            remember: false,
            up_on: false,
            up_kb: "50".into(),
            pending_upload: None,
            pending_choose: None,
            show_complete,
            complete_chosen: false,
            keep_sharing: false,
            host: None,
            hidden: false,
            start_minimized: false,
            tray: false,
            minimize_to_tray: false,
            main_shown: true,
            speed: None,
            hang_up: false,
            exit: false,
            turn_off_on: false,
            turn_off: 0,
            force: false,
            glide: Glide::default(),
            rename_init: false,
            rename_text: String::new(),
            rename_saved: None,
            rename_edited_at: None,
            rename_show_saved: false,
            pending_rename: None,
            rename_disabled_reason: None,
            rename_guard_stamp: None,
            rename_guard: None,
        }
    }

    /// Open as its start view says ([`opens`]): shown, minimized, or hidden in the notification
    /// area.
    pub fn open_as(&mut self, opens: Opens) {
        self.hidden = opens == Opens::Hidden;
        self.start_minimized = opens == Opens::Minimized;
    }

    /// Its optional tabs as Options → Downloads → Customize says (`limiter`: Speed Limiter shows,
    /// `completion`: When done shows); one left out starts hidden, as its "Hide
    /// tab" leaves it.
    pub fn with_tabs(mut self, limiter: bool, completion: bool) -> Self {
        if !limiter {
            self.hide(Tab::Limiter);
        }
        if !completion {
            self.hide(Tab::Completion);
        }
        self
    }

    fn status_drop(&self) -> f32 {
        let _ = self;
        RENAME_DROP
    }

    /// Synchronise the Rename box from `item.rename_on_complete` on first display and whenever
    /// `item` changes while idle.
    pub fn sync_rename_from_item(&mut self, item: &Item) {
        let idle = self.rename_edited_at.is_none() && self.pending_rename.is_none();
        if !self.rename_init || (idle && self.rename_saved != item.rename_on_complete) {
            self.rename_init = true;
            self.rename_saved = item.rename_on_complete.clone();
            self.rename_text = item.rename_on_complete.clone().unwrap_or_default();
            self.rename_show_saved = self.rename_saved.is_some();
        }
    }

    fn commit_rename(&mut self, norm: Option<String>) {
        self.rename_saved = norm.clone();
        self.rename_edited_at = None;
        self.rename_show_saved = norm.is_some();
        self.pending_rename = Some(norm);
    }

    fn on_rename_text_changed(&mut self, now: std::time::Instant) {
        match crate::deferred_rename::validate_name(&self.rename_text) {
            Ok(norm) => {
                if norm == self.rename_saved {
                    self.rename_edited_at = None;
                    self.rename_show_saved = self.rename_saved.is_some();
                } else {
                    self.rename_show_saved = false;
                    self.rename_edited_at = Some(now);
                }
            }
            Err(_) => {
                self.rename_show_saved = false;
                self.rename_edited_at = None;
            }
        }
    }

    /// Check whether the 3-second idle auto-save timer has elapsed; returns the remaining wait if
    /// still counting down so the UI schedules a repaint.
    fn poll_rename_autosave(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        let edited_at = self.rename_edited_at?;
        let Ok(norm) = crate::deferred_rename::validate_name(&self.rename_text) else {
            self.rename_edited_at = None;
            return None;
        };
        if norm == self.rename_saved {
            self.rename_edited_at = None;
            self.rename_show_saved = self.rename_saved.is_some();
            return None;
        }
        let elapsed = now.saturating_duration_since(edited_at);
        if elapsed >= crate::deferred_rename::AUTOSAVE_DELAY {
            self.commit_rename(norm);
            None
        } else {
            Some(crate::deferred_rename::AUTOSAVE_DELAY - elapsed)
        }
    }

    /// Flush any rename that was already confirmed (Confirm, Enter, assistive input, or the
    /// 3-second idle auto-save); half-typed unconfirmed text is never applied.
    pub fn take_confirmed_rename(&mut self) -> Option<Option<String>> {
        self.pending_rename.take()
    }

    pub fn show(&mut self, ctx: &egui::Context, item: &Item, snap: Option<&Snapshot>) -> Action {
        crate::crash::window("progress");
        let frac = item.fraction();
        let title = window_title(frac, &item.file_name);
        // The bar and the segment map glide between the figures (the text shows them as they
        // are): frames come faster for that only while the download runs and its figures move.
        let segments: Vec<(u64, u64)> = match snap {
            Some(s) if !s.segments.is_empty() => s
                .segments
                .iter()
                .map(|g| (g.start, g.cursor.saturating_sub(g.start)))
                .collect(),
            _ => vec![(0, item.downloaded)],
        };
        let drawn = self.glide.step(
            ctx.input(|i| i.time),
            item.downloaded,
            &segments,
            item.status == Status::Complete,
        );
        // Never past the fraction: a video's bar holds short of the end until it is finished,
        // as its percentage does ([`Item::fraction`]); no size, no fill.
        let bar = match (frac, item.size.filter(|t| *t > 0)) {
            (Some(f), Some(t)) => (drawn.done / t as f64).clamp(0.0, 1.0).min(f),
            _ => 0.0,
        };
        // The speed graph's band (beyond IDM) under the tab page: the rest moves down by it.
        let drop = self.status_drop() + GRAPH_BAND;
        let body = if self.details {
            [548.0, 412.0 + drop + torrent_drop(item)]
        } else {
            [548.0, 241.0 + drop]
        };
        // Windows: the title bar is ours (#63), drawn in the client above IDM's layout, so the
        // hide button sits in it, left of Minimize, as in IDM. Elsewhere the system draws it.
        let client = [body[0], body[1] + TITLE_H];
        // As large as the screen allows at the interface size (#262): the rest scrolls.
        let shown = crate::dialogs::shown_size(ctx, client);
        let vid = ViewportId::from_hash_of(("progress", self.id));
        let buttons = caption_buttons(self.tray);
        let key = egui::Id::unique(("progress-pos", self.id));
        let title_now = title.clone();
        // Hidden until dialogs::keep_theme has given it its theme.
        let mut builder = crate::quiet(ViewportBuilder::default())
            .with_visible(false)
            .with_title(title)
            .with_inner_size(shown)
            // IDM's progress window has 2 px borders and a fixed size. On Windows the sizing
            // frame is asked for and win32::progress_frame trims its borders, disables resizing
            // and gives the caption to the client, which draws it; elsewhere the window is simply
            // not resizable (its contents are laid out for one size: Show/Hide details switches
            // between two).
            .with_resizable(cfg!(windows))
            .with_icon(crate::app_icon())
            .with_maximize_button(false);
        if let Some(p) = crate::dialogs::place(ctx, key, [548.0, 412.0 + drop]) {
            builder = builder.with_position(p);
        }
        // Minimize hides it in the notification area when asked (and a tray can bring it back).
        let minimize_hides = self.tray && self.minimize_to_tray;
        // The window, once its frame is applied (Windows: its caption parts go to its hit test).
        let hwnd = ctx.data(|d| d.get_temp::<isize>(key.with("hwnd")));
        // The × stops a download under way, as IDM's does (`closing_action`): said in its
        // tooltip, where the title bar is ours.
        let x_stops = matches!(closing_action(item), Action::Cancel);
        let tray = self.tray;
        let action = ctx.show_viewport_immediate(vid, builder, |ui, _| {
            let _ui = crate::framestats::span("progress ui");
            // Painted in the pass that closes it too (see dialogs::close_requested).
            let close = crate::dialogs::close_requested(ui);
            if !close && minimize_hides && ui.ctx().input(|i| i.viewport().minimized == Some(true))
            {
                return Action::Hide;
            }
            if x_stops {
                // i18n: tooltip of the close box (×) of a running download's progress window: it stops the download (which can be resumed later) and closes the window
                winui::caption::close_tip(crate::i18n::mark(
                    "Stop the download and close (Resume continues it)",
                ));
            }
            let action = egui::CentralPanel::no_frame()
                .frame(egui::Frame::new().fill(winui::BG()))
                .show(ui, |ui| {
                    let o = ui.max_rect().min;
                    // The window's width, laid out from the right in a right-to-left
                    // interface (the title bar; the layout under it has its own).
                    let _surface = winui::Surface::enter(o.x, o.x + shown[0]);
                    let caption = draw_caption(ui, o, shown[0], &title_now, &buttons, hwnd);
                    // The layout under the title bar, scrolled when the window is smaller.
                    let below = egui::Rect::from_min_max(o + vec2(0.0, TITLE_H), ui.max_rect().max);
                    let mut rest = ui.new_child(egui::UiBuilder::new().max_rect(below));
                    let action = crate::dialogs::scroll(&mut rest, key, body, |ui, o| {
                        let _surface = winui::Surface::enter(o.x, o.x + body[0]);
                        let _laid = crate::framestats::span("progress layout");
                        self.body(ui, o, item, snap, frac, (bar, &drawn.segments))
                    });
                    // The system menu's "Hide to the notification area" (Alt+Space).
                    #[cfg(windows)]
                    if let Some(h) = hwnd
                        && crate::win32::take_hide_asked(h)
                    {
                        return Action::Hide;
                    }
                    match caption {
                        Some(CaptionButton::Close) => closing_action(item),
                        Some(CaptionButton::ToTray) => Action::Hide,
                        Some(CaptionButton::Minimize) if minimize_hides => Action::Hide,
                        Some(CaptionButton::Minimize) => {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            action
                        }
                        _ => action,
                    }
                })
                .inner;
            // The system's close (Alt+F4, the taskbar's, the window manager's ×): as our ×.
            if close {
                return closing_action(item);
            }
            // Escape puts the window away and the download goes on: hidden in the notification
            // area where a tray can bring it back, else closed (the row's Show progress opens it
            // again). Never a stop: a key pressed by mistake loses nothing.
            if matches!(action, Action::None) && winui::escape_pressed(ui) {
                if tray { Action::Hide } else { Action::Close }
            } else {
                action
            }
        });
        // Elsewhere the window manager is asked for its size at a new interface size (#262).
        crate::dialogs::follow_scale(ctx, vid, key, shown);
        // The glide's frames: while the download runs, its figures move and the window can be
        // seen (minimized, or hidden in the tray where it isn't drawn at all, it waits).
        let minimized = ctx.input_for(vid, |i| i.viewport().minimized) == Some(true);
        let ppp = ctx.input_for(vid, |i| i.pixels_per_point);
        if item.live.is_some()
            && !minimized
            && let Some(every) = glide_frame(drawn.rate, item.size, ppp, self.main_shown)
        {
            ctx.request_repaint_after(every);
        }
        // Apply the frame while the window is still hidden, and again whenever the client size
        // (details), the scale (another display, the interface size) or the system menu's item
        // changes.
        #[cfg(windows)]
        {
            let scale = ctx.input_for(vid, |i| i.pixels_per_point);
            let hide = buttons
                .iter()
                .any(|(b, _)| *b == CaptionButton::ToTray)
                .then(|| crate::i18n::t("Hide to the notification area").to_owned());
            let want: Framed = (shown, (scale * 1000.0).round() as u32, hide);
            if ctx
                .data(|d| d.get_temp::<Framed>(key.with("framed")))
                .as_ref()
                != Some(&want)
                && let Some(h) =
                    crate::win32::thin_frame(&title_now, shown, scale, want.2.as_deref())
            {
                ctx.data_mut(|d| {
                    d.insert_temp(key.with("framed"), want);
                    d.insert_temp(key.with("hwnd"), h);
                });
            }
        }
        let closing = matches!(
            action,
            Action::Close | Action::Cancel | Action::Hide | Action::WithBrowser
        );
        let _theme = crate::framestats::span("progress keep_theme");
        crate::dialogs::keep_theme(
            ctx,
            vid,
            key,
            &title_now,
            crate::theme::dark_surface(),
            !closing,
        );
        // Its start view minimized: in the pass keep_theme shows it, right after (the commands go
        // in order: shown, then minimized), never before (a hidden window that is then shown
        // comes up restored).
        if self.start_minimized && !closing && crate::dialogs::window_shown(ctx, key) {
            self.start_minimized = false;
            ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Minimized(true));
        }
        let action = match (action, self.pending_limit.take()) {
            (Action::None, Some((limit, remember))) => Action::Limit(limit, remember),
            (a, _) => a,
        };
        // An upload change waits a frame when a download one goes first (one action a frame);
        // so does a change of files.
        let action = match action {
            Action::None => self
                .pending_upload
                .take()
                .map_or(Action::None, Action::UploadLimit),
            a => a,
        };
        let action = match action {
            Action::None => self.pending_choose.take().unwrap_or(Action::None),
            a => a,
        };
        let action = match action {
            Action::None => self
                .pending_rename
                .take()
                .map_or(Action::None, Action::Rename),
            a => a,
        };
        if matches!(action, Action::Close | Action::Cancel | Action::WithBrowser) {
            crate::dialogs::forget_place(ctx, key);
        }
        // Gone (hidden keeps its place): the next window gets its thin frame again.
        if closing {
            #[cfg(windows)]
            if let Some(h) = hwnd {
                crate::win32::forget_caption(h);
            }
            ctx.data_mut(|d| {
                d.remove::<Framed>(key.with("framed"));
                d.remove::<isize>(key.with("hwnd"));
            });
        }
        action
    }

    /// `frac`: the download's fraction as it is (the text); `drawn`: the bar's fill and the
    /// segments as they glide ([`Glide`]).
    fn body(
        &mut self,
        ui: &mut Ui,
        o: Pos2,
        item: &Item,
        snap: Option<&Snapshot>,
        frac: Option<f64>,
        drawn: (f64, &[(u64, f64)]),
    ) -> Action {
        let p = ui.painter().clone();
        self.sync_rename_from_item(item);
        if let Some(wait) = self.poll_rename_autosave(std::time::Instant::now()) {
            ui.ctx()
                .request_repaint_after(wait + std::time::Duration::from_millis(15));
        }
        let hide = self.tabs(ui, o);
        let hide_tab = match self.tab {
            Tab::Status => {
                self.status_page(ui, o, item, snap, frac);
                false
            }
            Tab::Limiter => self.limiter_page(ui, o, item),
            Tab::Completion => self.completion_page(ui, o, item),
        };
        if hide_tab {
            self.hide(self.tab);
            // The strip above was drawn with the tab still on it: draw the window again at once,
            // without showing this pass, so the tab goes in one step. A download's window
            // otherwise waits for its next timed repaint (250 ms while it downloads) on the old page.
            ui.ctx().request_discard("Hide tab");
            ui.ctx().request_repaint();
        }

        // The speed graph (beyond IDM) in its band under the tab page; the rest under it.
        let band = o + vec2(0.0, self.status_drop());
        let runs = item.status == Status::Downloading;
        crate::speedgraph::progress_band(ui, &p, band, self.speed.as_ref(), runs, self.id);
        let lower = band + vec2(0.0, GRAPH_BAND);

        // The progress bar: IDM's green on the shared, themed track.
        winui::meter(&p, r(lower, 17.0, 180.0, 513.0, 16.0), drawn.0 as f32);

        let mut action = if hide { Action::Hide } else { Action::None };
        let details_label = if self.details {
            crate::i18n::mark("<< Hide details")
        } else {
            crate::i18n::mark("Show details >>")
        };
        let other_tab = self.tab != Tab::Status;
        let kind = if other_tab {
            ButtonKind::Default
        } else {
            ButtonKind::Normal
        };
        // The row as wide as its labels in this language need (#192), each laid out for the
        // longer of its two labels so it keeps its size when they change; Cancel stays at the
        // window's edge. A download the site's bot check refused (#376) has "Download with the
        // browser" between the details and Resume.
        let with_browser = crate::handback::offered(item);
        // A running torrent with a video or sound file the browser plays: its player, as the
        // list's right-click menu has it, in the same place.
        let playable = !with_browser
            && item.status == Status::Downloading
            && item
                .torrent
                .as_ref()
                .zip(self.host.as_ref())
                .is_some_and(|(spec, host)| {
                    crate::app::player::has_playable_file(host, &spec.hash)
                });
        let mut row = vec![
            (
                winui::widest_label(&p, &["<< Hide details", "Show details >>"]),
                30.0,
                138.0,
            ),
            (winui::widest_label(&p, &["Pause", "Resume"]), 311.0, 82.0),
            ("Cancel", 422.0, 73.0),
        ];
        if with_browser {
            row.insert(1, ("Download with the browser", 176.0, 127.0));
        } else if playable {
            row.insert(1, ("Play while downloading", 176.0, 127.0));
        }
        let mut placed = winui::button_row(&p, &row, 8.0, 30.0, 200.0);
        let browser_at = with_browser.then(|| placed.remove(1));
        let play_at = playable.then(|| placed.remove(1));
        let [details_at, pause_at, cancel_at] = placed[..] else {
            unreachable!("three buttons")
        };
        if winui::button(
            ui,
            r(lower, details_at.0, 206.0, details_at.1, 22.0),
            details_label,
            kind,
            false,
            None,
        )
        .clicked()
        {
            self.details = !self.details;
        }
        if let Some(at) = browser_at
            && winui::button(
                ui,
                r(lower, at.0, 206.0, at.1, 22.0),
                // i18n: button of a download the site's bot check (Cloudflare) refused: open its address in the web browser, which downloads the file itself (and keeps it); the same words as the right-click item
                "Download with the browser",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
        {
            action = Action::WithBrowser;
        }
        if let Some(at) = play_at
            && winui::button(
                ui,
                r(lower, at.0, 206.0, at.1, 22.0),
                // i18n: button of a running torrent's progress window: opens a browser media player that reads its video or sound while the pieces arrive (the same words as the right-click item)
                "Play while downloading",
                ButtonKind::Normal,
                false,
                None,
            )
            .clicked()
        {
            action = Action::Play;
        }
        let running = item.status == Status::Downloading;
        let can_resume = matches!(item.status, Status::Stopped | Status::Error);
        let (label, pkind) = if running {
            // i18n: verb: pause the download
            (crate::i18n::mark("Pause"), ButtonKind::Normal)
        } else if can_resume {
            // i18n: verb: continue the download
            (crate::i18n::mark("Resume"), ButtonKind::Normal)
        } else {
            (crate::i18n::mark("Pause"), ButtonKind::Disabled)
        };
        if winui::button(
            ui,
            r(lower, pause_at.0, 206.0, pause_at.1, 22.0),
            label,
            pkind,
            false,
            None,
        )
        .clicked()
        {
            action = if running {
                Action::Pause
            } else {
                Action::Resume
            };
        }
        if winui::button(
            ui,
            r(lower, cancel_at.0, 206.0, cancel_at.1, 22.0),
            "Cancel",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
        {
            action = Action::Cancel;
        }

        if self.details && item.torrent.is_some() {
            self.torrent_details(ui, &p, lower + vec2(0.0, torrent_drop(item)), item);
        } else if self.details {
            winui::text_in_window(
                ui,
                &p,
                lower.x + 137.0,
                lower.y + 242.0,
                // i18n: progress window: caption over the bar showing where each connection started in the file and how far it has got
                crate::i18n::t("Each connection's part of the file"),
                winui::TEXT(),
            );
            segment_bar(&p, lower, item, drawn.1);
            connections(ui, &p, lower, snap);
        }
        action
    }

    /// Classic 3-D tabs over a raised page frame (spec "Tab strip"); with a tray, the "hide to
    /// the notification area" button at the row's end. True when that button is pressed.
    fn tabs(&mut self, ui: &mut Ui, o: Pos2) -> bool {
        // The shared tab strip (the torrent tabs' too): one look in both themes, one Tab stop with
        // the arrows, sized to the translated labels. It had two of its own (classic 3-D in dark).
        let tabs: Vec<(Tab, &str)> = [
            // i18n: tab title: how the download is going
            (Tab::Status, crate::i18n::mark("Download status")),
            // i18n: tab title: the tool that limits the download speed
            (Tab::Limiter, crate::i18n::mark("Speed Limiter")),
            // i18n: tab title: what happens when the download finishes
            (Tab::Completion, crate::i18n::mark("When done")),
        ]
        .into_iter()
        .filter(|t| !self.hidden_tabs.contains(&t.0))
        .collect();
        let p = ui.painter().clone();
        let english: Vec<&str> = tabs.iter().map(|t| t.1).collect();
        // The hide button sits here where the system draws the title bar (Linux, macOS; on
        // Windows it's in ours); the tabs end before it (long translations shorten first).
        let tab_button = self.tray && !cfg!(windows);
        let right = if tab_button { 500.0 } else { 525.0 };
        let light: Vec<winui::LightTab> = winui::tab_row(&p, &english, 20.0, right, 2.0, 8.0)
            .into_iter()
            .zip(tabs.iter())
            .map(|((x0, x1, _), (_, label))| winui::LightTab {
                label,
                sel: (x0 - 2.0, x1 + 2.0),
                unsel: (x0, x1),
            })
            .collect();
        let current = tabs.iter().position(|t| t.0 == self.tab).unwrap_or(0);
        let drop = self.status_drop();
        // The page: x 18..529, y 28..(171 + drop) (tabs_light's pane starts 20 px under its top).
        if let Some(i) =
            winui::tabs_light(ui, o, r(o, 18.0, 8.0, 514.0, 165.0 + drop), &light, current)
            && tabs[i].0 != self.tab
        {
            self.tab = tabs[i].0;
        }
        self.tab_row_menu(ui, r(o, 18.0, 6.0, 514.0, 22.0));
        // DDM's (#63): IDM puts this in its title bar, which here is the system's own (it takes
        // no buttons of ours), so it sits at the tab row's end, over the page's corner.
        tab_button
            && winui::to_tray_button(
                ui,
                r(o, 507.0, 6.0, 22.0, 20.0),
                "progress-hide",
                // The title bar's own button's words (winui::caption).
                crate::i18n::mark("Hide to the notification area (the download goes on)"),
            )
            .clicked()
    }

    /// The tab row's right-click menu (#326): "Show hidden tabs" puts back the tabs hidden in
    /// this window, greyed while none is. From the keyboard too, on the focused tab: Shift+F10
    /// or the Menu key.
    fn tab_row_menu(&mut self, ui: &Ui, row: egui::Rect) {
        use crate::chrome::{MenuEntry, MenuKind, PopupResult};
        let ctx = ui.ctx();
        let key = egui::Id::unique(("progress-tab-menu", self.id));
        let by_pointer = ui.input(|i| {
            i.pointer
                .secondary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten()
                .filter(|p| row.contains(*p))
        });
        let by_keys = || {
            let tab = ctx
                .memory(|m| m.focused())
                .and_then(|f| ctx.read_response(f))
                .map(|r| r.rect)
                .filter(|r| row.contains(r.center()))?;
            ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Menu)
            })
            // Under the tab, from its leading edge (a menu's top-right in right to left).
            .then(|| {
                if winui::rtl() {
                    tab.right_bottom()
                } else {
                    tab.left_bottom()
                }
            })
        };
        if let Some(at) = by_pointer.or_else(by_keys) {
            ctx.data_mut(|d| d.insert_temp(key, at));
        }
        let Some(at) = ctx.data(|d| d.get_temp::<Pos2>(key)) else {
            return;
        };
        let entries = [MenuEntry::Item(
            // i18n: right-click menu of a download window's tab row: bring back the tabs taken off with "Hide tab"
            "&Show hidden tabs",
            (),
            !self.hidden_tabs.is_empty(),
            false,
        )];
        let id = format!("progress-tab-menu-{}", self.id);
        match crate::chrome::popup(ctx, MenuKind::Context, &id, at, &entries, &|_| None) {
            PopupResult::Open | PopupResult::Neighbor(_) => return,
            PopupResult::Chosen(()) => {
                self.show_hidden_tabs();
                ctx.request_repaint();
            }
            PopupResult::Dismissed => {}
        }
        ctx.data_mut(|d| d.remove::<Pos2>(key));
    }

    /// Puts every tab hidden with "Hide tab" back on the strip, in its own place.
    fn show_hidden_tabs(&mut self) {
        self.hidden_tabs.clear();
    }

    /// Takes `tab` off the strip (its "Hide tab" button, #326) and shows Download status, which
    /// has no such button and so always stays.
    fn hide(&mut self, tab: Tab) {
        if tab != Tab::Status && !self.hidden_tabs.contains(&tab) {
            self.hidden_tabs.push(tab);
        }
        self.tab = Tab::Status;
    }
}

#[cfg(test)]
mod tests;
