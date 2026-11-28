//! Text: laid out in the interface's direction, drawn at a baseline, measured, fitted and cut with
//! an ellipsis; labels and captions, links, copyable and right-aligned text.

use super::*;

/// `caption` as it fits from its English pen `x` to the far edge of what the painter may draw on
/// (the window, or the box it sits in), keeping a 6 px margin, and ending 5 px before English x
/// `until` (a control after the caption): whole, or cut with "..." (true).
pub(super) fn fit_caption_until(
    p: &Painter,
    x: f32,
    caption: &str,
    until: Option<f32>,
) -> (String, bool) {
    let clip = p.clip_rect();
    // The far edge in English coordinates: the screen's left edge when mirrored.
    let edge = if rtl() { mx(clip.left()) } else { clip.right() };
    let end = until.map_or(edge - 6.0, |u| (u - 5.0).min(edge - 6.0));
    let room = end - x;
    if text_width(p, caption) <= room {
        (caption.to_owned(), false)
    } else {
        (ellipsize(p, &tahoma(), caption, room.max(0.0)), true)
    }
}

/// `s` laid out on one line in the interface's direction.
pub fn galley(p: &Painter, font: FontId, s: &str, color: Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(s.to_owned(), font, color, f32::INFINITY);
    job.direction = direction();
    p.layout_job(job)
}

/// Draw `s` with its pen (its start) at English `x` and its baseline (last cap row) at
/// `baseline`. Returns the text's advance rect on screen.
pub fn text(p: &Painter, x: f32, baseline: f32, s: &str, color: Color32) -> Rect {
    let drawn = text_font(p, tahoma(), x, baseline, s, color);
    note_caption(p, s, drawn);
    drawn
}

/// The caption painted last in this window (text, where, which pass): the next text box or
/// drop-down beside or under it takes it as its name for screen readers ([`caption_for`]).
pub(super) fn caption_key(ctx: &egui::Context) -> egui::Id {
    egui::Id::unique(("last-caption", ctx.viewport_id()))
}

pub(super) fn note_caption(p: &Painter, s: &str, drawn: Rect) {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > 80 {
        return;
    }
    let ctx = p.ctx();
    // The key first: asking the context inside its lock deadlocks.
    let (key, pass) = (caption_key(ctx), ctx.cumulative_pass_nr());
    ctx.data_mut(|d| d.insert_temp(key, (s.to_owned(), drawn, pass)));
}

/// The caption that names `control` (screen rect): painted this pass just before it, at its
/// leading side on the same row or right above it. Taken once.
pub(super) fn caption_for(ctx: &egui::Context, control: Rect) -> Option<String> {
    let key = caption_key(ctx);
    let (s, at, pass) = ctx.data(|d| d.get_temp::<(String, Rect, u64)>(key))?;
    if pass != ctx.cumulative_pass_nr() {
        return None;
    }
    // In English (a right-to-left window has its captions at the right).
    let (c, b) = (rect(control), rect(at));
    let same_row = b.right() <= c.left() + 4.0 && b.bottom() > c.top() && b.top() < c.bottom();
    let above = b.bottom() <= c.top() + 3.0 && b.bottom() >= c.top() - 24.0 && b.left() < c.right();
    if !(same_row || above) {
        return None;
    }
    ctx.data_mut(|d| d.remove::<(String, Rect, u64)>(key));
    Some(s.trim_end_matches(':').trim().replace('&', ""))
}

/// A text box's or drop-down's name for screen readers, from its caption ([`caption_for`]).
pub(super) fn name_from_caption(resp: &Response) {
    if let Some(name) = caption_for(&resp.ctx, resp.rect) {
        resp.ctx
            .accesskit_node_builder(resp.id, |node| node.set_label(name));
    }
}

/// Like [`text`] in any font.
pub fn text_font(
    p: &Painter,
    font: FontId,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
) -> Rect {
    let g = galley(p, font, s, color);
    // The text's box in English, then on screen (its start at the mirrored edge in RTL).
    let left = rect(Rect::from_min_size(
        pos2(x.round(), 0.0),
        vec2(g.size().x, 0.0),
    ))
    .left();
    paint_galley(p, left, baseline, g, color)
}

/// Like [`text_font`] with its start at screen x `start` (a widget's inside, measured from its
/// leading edge): the text runs right from it, or left in RTL.
pub fn text_start(
    p: &Painter,
    font: FontId,
    start: f32,
    baseline: f32,
    s: &str,
    color: Color32,
) -> Rect {
    let g = galley(p, font, s, color);
    let left = if rtl() { start - g.size().x } else { start };
    paint_galley(p, left.round(), baseline, g, color)
}

/// Like [`text_font`] with its left at screen x `left` (text centred in a screen rect).
pub fn text_at(
    p: &Painter,
    font: FontId,
    left: f32,
    baseline: f32,
    s: &str,
    color: Color32,
) -> Rect {
    paint_galley(p, left, baseline, galley(p, font, s, color), color)
}

/// Paint `g` with its left at screen x `left` and its baseline (last cap row) at `baseline`.
pub(super) fn paint_galley(
    p: &Painter,
    left: f32,
    baseline: f32,
    g: std::sync::Arc<egui::Galley>,
    color: Color32,
) -> Rect {
    let asc = g
        .rows
        .first()
        .map(|r| r.pos.y + r.row.glyphs.first().map_or(0.0, |g| g.pos.y))
        .unwrap_or(0.0);
    // The baseline line sits under the last cap row.
    let top = (baseline + 1.0 - asc).round();
    let size = g.size();
    let left = left.round();
    let drawn = Rect::from_min_size(pos2(left, top), size);
    fit_audit(p, drawn, g.text());
    p.galley(pos2(left, top), g, color);
    drawn
}

/// `MDM_FIT_AUDIT=<file>` (a check for translations, beyond IDM): every distinct text drawn past
/// the edge of what its painter may show (the window, a box), and every two texts drawn over each
/// other in one window, are appended to the file, so a run through the windows in each language
/// lists what doesn't fit. Off otherwise (one env lookup).
pub(super) fn fit_audit(p: &Painter, drawn: Rect, text: &str) {
    use std::collections::{HashMap, HashSet};
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    /// What one window (viewport) has drawn in its current pass: (layer, visible rect, text).
    type Pass = (u64, Vec<(egui::LayerId, Rect, String)>);
    /// The audit file, the lines already in it, and each window's texts this pass.
    type Audit = Mutex<(
        std::fs::File,
        HashSet<String>,
        HashMap<egui::ViewportId, Pass>,
    )>;
    static AUDIT: OnceLock<Option<Audit>> = OnceLock::new();
    let Some(audit) = AUDIT.get_or_init(|| {
        let path = std::env::var_os("MDM_FIT_AUDIT")?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Mutex::new((file, Default::default(), Default::default())))
    }) else {
        return;
    };
    let mut audit = audit.lock().unwrap();
    let (file, seen, passes) = &mut *audit;
    let lang = crate::i18n::code();
    let clip = p.clip_rect();
    if (drawn.left() < clip.left() - 1.0 || drawn.right() > clip.right() + 1.0)
        && seen.insert(text.to_owned())
    {
        let _ = writeln!(
            file,
            "{}\t{:.0}..{:.0} in {:.0}..{:.0}\t{}", // not translated: a check's log line
            lang,
            drawn.left(),
            drawn.right(),
            clip.left(),
            clip.right(),
            text.replace('\n', " / ")
        );
    }
    // Over another text: what shows of each (inside its clip) crosses by more than 2 px across
    // and half a row down (rows a line apart may cross by their padding). The same text twice
    // (an embossed disabled label) is one text.
    let ctx = p.ctx();
    let viewport = ctx.viewport_id();
    let pass_nr = ctx.cumulative_pass_nr_for(viewport);
    let pass = passes.entry(viewport).or_insert((pass_nr, Vec::new()));
    if pass.0 != pass_nr {
        *pass = (pass_nr, Vec::new());
    }
    let shown = drawn.intersect(clip);
    if shown.width() <= 0.0 || shown.height() <= 0.0 {
        return;
    }
    for (layer, other, other_text) in &pass.1 {
        let both = shown.intersect(*other);
        if *layer == p.layer_id()
            && other_text != text
            && both.width() > 2.0
            && both.height() > 0.5 * shown.height().min(other.height())
        {
            let line = format!(
                "{lang}\toverlap\t{} | {}", // not translated: a check's log line
                other_text.replace('\n', " / "),
                text.replace('\n', " / ")
            );
            if seen.insert(line.clone()) {
                let _ = writeln!(file, "{line}");
            }
        }
    }
    pass.1.push((p.layer_id(), shown, text.to_owned()));
}

/// The screen x range of the `n`-th character of `s` drawn in `drawn` (a mnemonic's underline):
/// by the width of what comes before it in a left-to-right interface (as Windows does), by the
/// glyph itself in RTL, where the letters before it are to its right.
pub fn char_span(p: &Painter, font: &FontId, s: &str, n: usize, drawn: Rect) -> (f32, f32) {
    let before: String = s.chars().take(n).collect();
    let ch: String = s.chars().skip(n).take(1).collect();
    let cw = text_width_font(p, font.clone(), &ch);
    if !crate::i18n::rtl() {
        let x0 = drawn.left() + text_width_font(p, font.clone(), &before);
        return (x0, x0 + cw - 1.0);
    }
    // The glyph itself, in the form it takes in the word (a joined Arabic letter is narrower
    // than alone); the row keeps its glyphs in reading order.
    let g = galley(p, font.clone(), s, TEXT());
    let c = ch.chars().next();
    let glyph = g.rows.first().and_then(|r| {
        let k = before.chars().filter(|b| Some(*b) == c).count();
        r.row.glyphs.iter().filter(|gl| Some(gl.chr) == c).nth(k)
    });
    match glyph {
        Some(gl) => {
            let x0 = drawn.left() + gl.pos.x;
            (x0, x0 + gl.advance_width.round().max(1.0) - 1.0)
        }
        None => (drawn.left(), drawn.left() + cw - 1.0),
    }
}

pub fn text_width(p: &Painter, s: &str) -> f32 {
    text_width_font(p, tahoma(), s)
}

pub fn text_width_font(p: &Painter, font: FontId, s: &str) -> f32 {
    p.layout_no_wrap(s.to_owned(), font, TEXT()).size().x
}

/// Where the fields of a column of labelled rows start (offset from the dialog's left), for
/// labels drawn at `label_x`: `field_x` as laid out for English, or further right when a
/// translated label is longer, so no label runs under its field. Every row shares it.
pub fn label_column(p: &Painter, label_x: f32, labels: &[&str], field_x: f32) -> f32 {
    let widest = labels
        .iter()
        .map(|l| text_width(p, l))
        .fold(0.0_f32, f32::max);
    field_x.max((label_x + widest + 6.0).ceil())
}

/// `s` cut with "..." (three periods, as Windows' DT_END_ELLIPSIS) to fit `max` px.
pub fn ellipsize(p: &Painter, font: &FontId, s: &str, max: f32) -> String {
    if text_width_font(p, font.clone(), s) <= max {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let cut = |n: usize| chars[..n].iter().collect::<String>() + "...";
    // The longest prefix that fits, by binary search (widths grow with the prefix).
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if text_width_font(p, font.clone(), &cut(mid)) <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(lo)
}

/// `s` cut with "..." in the middle to fit `max` px (Windows' DT_PATH_ELLIPSIS): for a path or
/// an address, whose start (the folder) and end (the file's name) both matter. The end keeps
/// three parts in five of what fits.
pub fn ellipsize_middle(p: &Painter, font: &FontId, s: &str, max: f32) -> String {
    if text_width_font(p, font.clone(), s) <= max {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let cut = |n: usize| {
        let head = n * 2 / 5;
        let tail = n - head;
        chars[..head].iter().collect::<String>()
            + "..."
            + &chars[chars.len() - tail..].iter().collect::<String>()
    };
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if text_width_font(p, font.clone(), &cut(mid)) <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(lo)
}

/// [`text`] that never runs past `right`: cut with "..." when too long. Returns the drawn
/// rect and whether the text was cut (then show it whole with [`full_on_hover`]).
pub fn text_fit(
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    right: f32,
) -> (Rect, bool) {
    let shown = ellipsize(p, &tahoma(), s, (right - x).max(0.0));
    let cut = shown != s;
    (text(p, x, baseline, &shown, color), cut)
}

/// [`text`] that never runs past the window's far edge: cut with "..." and shown whole on hover.
pub fn text_in_window(
    ui: &Ui,
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
) -> Rect {
    let (rc, cut) = text_fit(p, x, baseline, s, color, window_edges(p).1);
    if cut {
        full_on_hover(ui, rc, ("in-window", s), s);
    }
    rc
}

/// A label that leads into a control whose box starts at English `before`: at its pen `x` (IDM's
/// place) when it fits, else ending 5 px before the box and starting earlier, down to `min_x`;
/// cut with "..." only past that. Returns the drawn rect and whether it was cut.
pub fn label_into(
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    before: f32,
    min_x: f32,
) -> (Rect, bool) {
    let end = before - 5.0;
    let w = text_width(p, s);
    if x + w <= end {
        return (text(p, x, baseline, s, color), false);
    }
    let start = (end - w).max(min_x);
    text_fit(p, start, baseline, s, color, end)
}

/// [`label_into`] shown whole on hover when it had to be cut: the caption of a box starting at
/// English x `before`, never under it.
#[allow(clippy::too_many_arguments)]
pub fn label_before(
    ui: &Ui,
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    before: f32,
    min_x: f32,
) -> Rect {
    let (rect, cut) = label_into(p, x, baseline, s, color, before, min_x);
    if cut {
        full_on_hover(
            ui,
            rect,
            ("label-before", before as i32, baseline as i32),
            s,
        );
    }
    rect
}

/// A tooltip with the whole of a cut text while the pointer is over it (as Windows shows
/// for truncated list items).
pub fn full_on_hover(
    ui: &Ui,
    rect: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    full: &str,
) {
    let id = ui.scope_id().with(("full-text", salt));
    ui.interact(rect, id, egui::Sense::hover())
        .on_hover_text(full);
}

/// Text that opens or does something when clicked: link colour, underlined under the pointer.
/// `label` is English (translated here).
pub fn link(ui: &Ui, p: &Painter, x: f32, baseline: f32, label: &str, salt: &str) -> bool {
    let rect = text(p, x, baseline, crate::i18n::t(label), LINK());
    let resp = ui
        .interact(rect, ui.scope_id().with(("link", salt)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    expose(&resp, egui::Role::Link, crate::i18n::t(label), true, None);
    // A Tab stop shows that it has the keys.
    show_focus(ui, &resp, rect.expand(2.0));
    if resp.hovered() {
        let y = rect.bottom() + 0.5;
        p.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            egui::Stroke::new(1.0, LINK()),
        );
    }
    resp.clicked()
}

/// A disabled control says why on hover: `reason` (English, translated here) while `disabled`.
pub fn why_disabled(resp: Response, disabled: bool, reason: &str) -> Response {
    if disabled {
        resp.on_hover_text(crate::i18n::t(reason))
    } else {
        resp
    }
}

/// [`text_fit`] that shows the whole text on hover whenever it cut it: the one way to draw a
/// line that may not fit (no caller can forget the hover). `salt` tells the lines apart.
#[allow(clippy::too_many_arguments)]
pub fn fit(
    ui: &Ui,
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    right: f32,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> Rect {
    let (rect, cut) = text_fit(p, x, baseline, s, color, right);
    if cut {
        full_on_hover(ui, rect, salt, s);
    }
    rect
}

/// [`fit`] for a path or an address: shortened in the middle ([`ellipsize_middle`]) so the
/// file's name stays in sight, shown whole on hover when cut.
#[allow(clippy::too_many_arguments)]
pub fn fit_middle(
    ui: &Ui,
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    right: f32,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> Rect {
    let shown = ellipsize_middle(p, &tahoma(), s, (right - x).max(0.0));
    let rect = text(p, x, baseline, &shown, color);
    if shown != s {
        full_on_hover(ui, rect, salt, s);
    }
    rect
}

/// Text already drawn in `rect` that the user may want to copy (an address, a hash, a path, a
/// version): its right-click menu copies it whole ([`copy_menu`]), and it shows whole on hover
/// when `cut`. Taken by the pointer only (labels are no Tab stops).
pub fn copyable(
    ui: &Ui,
    rect: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    cut: bool,
) -> Response {
    let resp = ui.interact(rect, ui.scope_id().with(("copyable", salt)), pointer_only());
    copy_menu(&resp, text);
    if cut { resp.on_hover_text(text) } else { resp }
}

/// Right-aligned label whose ink ends at `right`.
pub fn text_right(p: &Painter, right: f32, baseline: f32, s: &str, color: Color32) -> (Rect, bool) {
    // A text longer than the room to the window's near edge (6 px margin) is cut with "...":
    // it never starts outside the window. The caller shows it whole on hover when cut.
    text_right_within(p, right, baseline, s, color, window_edges(p).0)
}

/// [`text_right`] that starts no further out than English x `near` (inside a group box).
pub fn text_right_within(
    p: &Painter,
    right: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    near: f32,
) -> (Rect, bool) {
    let room = right + 1.0 - near;
    let (shown, cut) = if text_width(p, s) <= room {
        (s.to_owned(), false)
    } else {
        (ellipsize(p, &tahoma(), s, room.max(0.0)), true)
    };
    let w = text_width(p, &shown);
    (text(p, right + 1.0 - w, baseline, &shown, color), cut)
}

/// A control's caption or a label beside it: the text colour, or the one disabled grey (no
/// classic embossed shadow: Windows 11 has none).
pub fn label(p: &Painter, x: f32, baseline: f32, s: &str, enabled: bool) -> Rect {
    text(
        p,
        x,
        baseline,
        s,
        if enabled {
            LIGHT_TEXT()
        } else {
            TEXT_DISABLED()
        },
    )
}

/// The baseline (last row of the capitals) of Tahoma −11 text centred in a control `height` tall
/// at `top`: its 8-row capitals start at `top + floor((height − 8) / 2)`. At 21 px that is the
/// measured `top + 13` of Windows' controls; every painter that centres a line of text in its box
/// (buttons, text boxes, read-only fields) uses it, so they agree at any height (a fixed `+ 13`
/// put text 1–3 px high in the 22–26 px ones).
pub fn baseline_in(top: f32, height: f32) -> f32 {
    top + ((height - 8.0) / 2.0).floor() + 7.0
}
