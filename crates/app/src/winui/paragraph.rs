//! Paragraphs: text wrapped to a width (CJK and hard breaks too), sentences made of pieces, data
//! paragraphs, scrolling text and the first-start windows' banner.

use super::*;

/// Where the text before a line's first input box goes in a [`sentence`].
#[derive(Clone, Copy, Debug)]
pub enum Lead {
    /// Starts at this x; the box moves right when the text is too long for the room.
    Left(f32),
    /// Ends at this x (a column of labels right-aligned to their boxes); longer text grows left.
    Right(f32),
}

/// One line of a [`sentence`], placed as the English has it.
#[derive(Clone, Copy, Debug)]
pub struct SentenceLine {
    pub lead: Lead,
    pub lead_baseline: f32,
    /// The top-left of the line's first box.
    pub boxes: Pos2,
    /// From a box to the text after it.
    pub gap: f32,
    /// The baseline of the text after the first box.
    pub baseline: f32,
}

/// A piece of a [`sentence`] template.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece<'a> {
    Text(&'a str),
    /// A `{name}` that is one of the sentence's input boxes.
    Slot(&'a str),
}

/// The banner atop a first-start window (the browser extension's offer, the video helpers):
/// DDM's logo at the start side, a bold headline, and a muted subline that wraps. 84 px tall, and
/// taller by what a long translation's extra lines need: that extra is returned, for the window
/// to add to its height and to move what is below the banner down by (it is known from the frame
/// before: [`crate::dialogs::grow_extra`] under `key`).
#[allow(clippy::too_many_arguments)]
pub fn banner(
    ui: &Ui,
    p: &Painter,
    o: Pos2,
    w: f32,
    key: &str,
    headline: &str,
    subline: &str,
) -> f32 {
    const PLANNED: f32 = 84.0;
    let extra = crate::dialogs::grow_extra(ui.ctx(), key);
    let h = PLANNED + extra;
    // `r` gives the rect where it is on screen (mirrored in RTL): the logo at the start side.
    p.rect_filled(r(o, 0.0, 0.0, w, h), 0.0, FIELD());
    fill_l(p, o.x, o.y + h, o.x + w - 1.0, o.y + h, FIELD_EDGE());
    // The app icon drawn for its size on screen (48 points: 72 pixels at 150 %).
    egui::Image::new(crate::logo::source(48.0 * ui.ctx().pixels_per_point()))
        .paint_at(ui, r(o, 18.0, 18.0, 48.0, 48.0));
    // The headline on one line: cut with "..." at the far edge (a long translation), whole on
    // hover.
    let bold = tahoma_title();
    let shown = ellipsize(p, &bold, headline, w - 18.0 - 80.0);
    let drawn = text_font(p, bold, o.x + 80.0, o.y + 36.0, &shown, TEXT());
    if shown != headline {
        full_on_hover(ui, drawn, (key, "headline"), headline);
    }
    let lines = paragraph(
        p,
        o.x + 80.0,
        o.y + 57.0,
        subline,
        MUTED(),
        o.x + w - 18.0,
        14.0,
    );
    // As planned, two lines: the second one's baseline at 71, then 13 px of descent and margin.
    let need = 57.0 + 14.0 * (lines.max(2) as f32 - 1.0) + 13.0;
    crate::dialogs::grow_px(ui.ctx(), key, need - PLANNED);
    extra
}

/// A separator dot between two parts of a line ("Germany · about 35 ms"), at English `x`: a
/// round 3 px dot on the middle of the lowercase letters. The font's "·" is a thin tick sitting
/// high at small sizes, which reads as an apostrophe. Its rect on screen.
pub fn separator_dot(p: &Painter, x: f32, baseline: f32, color: Color32) -> Rect {
    let rc = rect(Rect::from_center_size(
        pos2(x + 1.5, baseline - 2.5),
        vec2(3.0, 3.0),
    ));
    p.circle_filled(rc.center(), 1.5, color);
    rc
}

/// `s` with its single line breaks joined (a space, or nothing between letters of scripts
/// written without spaces: Chinese, Japanese, Thai…), blank lines kept as paragraph breaks.
pub(super) fn reflow(s: &str) -> String {
    let spaceless = |c: char| {
        matches!(c as u32,
            0x0E00..=0x0EFF   // Thai, Lao
            | 0x1780..=0x17FF // Khmer
            | 0x3040..=0x30FF // Hiragana, Katakana
            | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF // Han
            | 0x3000..=0x303F | 0xFF00..=0xFFEF) // CJK punctuation, full-width forms
    };
    s.split("\n\n")
        .map(|para| {
            let mut out = String::new();
            for (k, line) in para.split('\n').enumerate() {
                let line = line.trim();
                if k > 0 && !out.is_empty() && !line.is_empty() {
                    let before = out.chars().last().is_some_and(spaceless);
                    let after = line.chars().next().is_some_and(spaceless);
                    if !(before && after) {
                        out.push(' ');
                    }
                }
                out.push_str(line);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Chinese and Japanese (Han, kana, their punctuation and full-width forms): broken between any
/// two characters.
pub(super) fn cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x3000..=0x303F | 0xFF00..=0xFFEF)
}

/// Closing punctuation that a line broken between characters must not start with.
pub(super) fn no_line_start(c: char) -> bool {
    "、。，．・：；？！ー）」』】〕｝〉》々ゝゞぁぃぅぇぉっゃゅょァィゥェォッャュョ,.:;?!)]}%"
        .contains(c)
}

/// Read-only data that scrolls inside `area` (English coordinates, the box's own frame drawn by
/// the caller): the wheel or a trackpad over it, its scroll bar when the text is longer than the
/// box, and with `keys` the window's Up, Down, Page Up, Page Down, Home and End. Where it is
/// scrolled to is kept under `id`. The text is data (a crash report, release notes): its line
/// breaks are kept and it reads left to right in every language ([`data_paragraph`]).
#[allow(clippy::too_many_arguments)]
pub fn scroll_text(
    ui: &mut Ui,
    id: egui::Id,
    area: Rect,
    s: &str,
    color: Color32,
    line_h: f32,
    keys: bool,
) {
    use crate::scrollbar;
    const PAD: f32 = 6.0;
    let p = ui.painter().clone();
    let visible = ((area.height() - 6.0) / line_h).floor().max(1.0);
    // Laid out first where nothing shows: the bar takes room only when the text needs it.
    let measure = p.with_clip_rect(Rect::NOTHING);
    let lines =
        |right: f32| data_paragraph(&measure, area.left() + PAD, 0.0, s, color, right, line_h);
    let mut total = lines(area.right() - PAD) as f32;
    let bar = total > visible;
    let text_right = if bar {
        let right = area.right() - 1.0 - scrollbar::THICK - 3.0;
        total = lines(right) as f32;
        right
    } else {
        area.right() - PAD
    };
    let max_top = (total - visible).max(0.0);
    let mut top: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    // The wheel scrolls the system's lines a notch (three unless changed), as text boxes do.
    let area_resp = ui.interact(rect(area), id.with("area"), pointer_only());
    if area_resp.hovered() {
        top += wheel_rows(ui, line_h, visible);
    }
    // The whole text to the clipboard from its right-click menu.
    copy_menu(&area_resp, s);
    if keys {
        let page = (visible - 1.0).max(1.0);
        ui.input(|i| {
            for (key, by) in [
                (egui::Key::ArrowUp, -1.0),
                (egui::Key::ArrowDown, 1.0),
                (egui::Key::PageUp, -page),
                (egui::Key::PageDown, page),
                (egui::Key::Home, -total),
                (egui::Key::End, total),
            ] {
                if i.key_pressed(key) {
                    top += by;
                }
            }
        });
    }
    top = top.clamp(0.0, max_top);
    // Moved by the wheel or a key: draw the new place now, not at the next event.
    if ui
        .data(|d| d.get_temp::<f32>(id))
        .is_some_and(|was| was != top)
    {
        ui.ctx().request_repaint();
    }
    let clip = p.with_clip_rect(rect(area.shrink(2.0)));
    data_paragraph(
        &clip,
        area.left() + PAD,
        area.top() + 16.0 - top * line_h,
        s,
        color,
        text_right,
        line_h,
    );
    if bar {
        let track = rect(Rect::from_min_max(
            pos2(area.right() - 1.0 - scrollbar::THICK, area.top() + 1.0),
            pos2(area.right() - 1.0, area.bottom() - 1.0),
        ));
        if scrollbar::bar(
            ui,
            id.with("bar"),
            track,
            true,
            total,
            visible,
            1.0,
            &mut top,
        ) {
            ui.ctx().request_repaint();
        }
    }
    ui.data_mut(|d| d.insert_temp(id, top));
}

/// A paragraph from `x` to `right`, first baseline at `baseline`, `line_h` apart; returns the
/// lines drawn. English keeps its line breaks (broken where IDM breaks it) while each of its
/// lines fits the room; a translation's copy of them, and English with a line too long, is
/// flowed again (`reflow`). A line too long for the room wraps at spaces, or between any two
/// characters in scripts written without spaces (Chinese, Japanese, Thai…). (Data, whose line
/// breaks are its own: [`data_paragraph`].)
pub fn paragraph(
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    right: f32,
    line_h: f32,
) -> usize {
    let room = (right - x).max(1.0);
    // IDM's English is hard-wrapped; a translation's copy of those breaks is flowed again (a
    // blank line still starts a new paragraph). English keeps its own lines, unless one of
    // them is longer than the room: wrapped alone, it would leave its last word on a line of
    // its own.
    let flowed;
    let s = if crate::i18n::code() == "en" && s.lines().all(|l| text_width(p, l) <= room) {
        s
    } else {
        flowed = reflow(s);
        flowed.as_str()
    };
    let lines = wrap_lines(p, s, room);
    for (i, l) in lines.iter().enumerate() {
        text(p, x, baseline + line_h * i as f32, l, color);
    }
    lines.len()
}

/// Data shown as it is (a crash report, release notes), from English `x` to `right`, first
/// baseline at `baseline`, `line_h` apart; returns the lines drawn. Its line breaks are its own
/// in every language (a line longer than the room wraps, as in [`paragraph`]), and it reads left
/// to right from the left edge of its room, inside a right-to-left window too: data keeps its
/// direction there (the RTL notes above).
pub fn data_paragraph(
    p: &Painter,
    x: f32,
    baseline: f32,
    s: &str,
    color: Color32,
    right: f32,
    line_h: f32,
) -> usize {
    let lines = wrap_lines(p, s, (right - x).max(1.0));
    let left = data_left(x, right);
    for (i, l) in lines.iter().enumerate() {
        let mut job = egui::text::LayoutJob::simple(l.clone(), tahoma(), color, f32::INFINITY);
        job.direction = egui::epaint::text::TextDirection::LeftToRight;
        paint_galley(
            p,
            left,
            baseline + line_h * i as f32,
            p.layout_job(job),
            color,
        );
    }
    lines.len()
}

/// A run of data (a word of release notes, crate::notes) in `font`, read left to right in every
/// language, its left at screen x `left` and its baseline at `baseline`; its rect on screen.
pub fn data_run(
    p: &Painter,
    font: FontId,
    left: f32,
    baseline: f32,
    s: &str,
    color: Color32,
) -> Rect {
    let mut job = egui::text::LayoutJob::simple(s.to_owned(), font, color, f32::INFINITY);
    job.direction = egui::epaint::text::TextDirection::LeftToRight;
    paint_galley(p, left, baseline, p.layout_job(job), color)
}

/// Where data from English `x` to `right` starts on screen: its room's left edge (the mirror of
/// its English right edge in RTL).
pub fn data_left(x: f32, right: f32) -> f32 {
    rect(Rect::from_min_max(pos2(x, 0.0), pos2(right, 0.0)))
        .left()
        .round()
}

/// `s` broken into the lines it shows in `room` px: at its own line breaks, and where a line is
/// too long, at spaces, or between any two characters in scripts written without spaces.
pub(super) fn wrap_lines(p: &Painter, s: &str, room: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for given in s.split('\n') {
        // A Windows line end (a report written there) breaks the line too.
        let given = given.strip_suffix('\r').unwrap_or(given);
        let mut line = String::new();
        for word in given.split_inclusive(' ') {
            let candidate = format!("{line}{word}");
            if line.is_empty()
                || text_width(p, candidate.trim_end()) <= room
                // Chinese and Japanese fill the line: broken between characters below.
                || word.chars().next().is_some_and(cjk)
            {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line).trim_end().to_owned());
                line = word.to_owned();
            }
            // Wider than the room (a script without spaces), wherever it stands in the line:
            // break between characters, a letter and its marks kept together.
            while text_width(p, line.trim_end()) > room {
                let g: Vec<&str> = line.graphemes(true).collect();
                let mut n = 0;
                let mut w = String::new();
                for (k, gr) in g.iter().enumerate() {
                    w.push_str(gr);
                    if text_width(p, &w) > room {
                        break;
                    }
                    n = k + 1;
                }
                let latin = |k: usize| g[k].chars().all(|c| c.is_ascii_alphanumeric());
                // Not inside a Latin word ("Dannex Download Manager" in Japanese): before it.
                if n > 0
                    && n < g.len()
                    && latin(n - 1)
                    && latin(n)
                    && let Some(space) = g[..n].iter().rposition(|gr| *gr == " ")
                {
                    n = space + 1;
                }
                // A closing mark never starts a line (Japanese kinsoku): the character before
                // it goes down with it.
                if n > 1 && g.get(n).is_some_and(|gr| gr.chars().all(no_line_start)) {
                    n -= 1;
                }
                if n == 0 {
                    break;
                }
                lines.push(g[..n].concat().trim_end().to_owned());
                line = g[n..].concat().trim_start().to_owned();
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines
}

/// `template`'s lines (`\n`), each split at the `{name}` placeholders that are in `slots`;
/// text pieces trimmed (the layout spaces them), empty ones dropped.
pub fn sentence_pieces<'a>(template: &'a str, slots: &[&str]) -> Vec<Vec<Piece<'a>>> {
    template
        .split('\n')
        .map(|line| {
            let mut out = Vec::new();
            let (mut from, mut at) = (0, 0);
            while let Some(open) = line[at..].find('{').map(|i| at + i) {
                let Some(close) = line[open..].find('}').map(|i| open + i) else {
                    break;
                };
                let name = &line[open + 1..close];
                if slots.contains(&name) {
                    out.push(Piece::Text(line[from..open].trim()));
                    out.push(Piece::Slot(name));
                    from = close + 1;
                }
                at = close + 1;
            }
            out.push(Piece::Text(line[from..].trim()));
            out.retain(|p| *p != Piece::Text(""));
            out
        })
        .collect()
}

/// Where [`sentence`] puts things: (x, baseline, text) for each text piece, and each slot's
/// rect in `slots` order. `width` measures a text. A right-aligned lead starts no further out
/// than `near` (the window's edge); the box after it moves on instead.
pub fn sentence_layout(
    template: &str,
    lines: &[SentenceLine],
    slots: &[(&str, egui::Vec2)],
    width: impl Fn(&str) -> f32,
    near: f32,
) -> (Vec<(f32, f32, String)>, Vec<Rect>) {
    let names: Vec<&str> = slots.iter().map(|(n, _)| *n).collect();
    let mut rows = sentence_pieces(template, &names);
    // More lines than the English has: the rest joins the last one.
    if rows.len() > lines.len() && !lines.is_empty() {
        let extra: Vec<Piece> = rows.drain(lines.len()..).flatten().collect();
        rows[lines.len() - 1].extend(extra);
    }
    let mut texts = Vec::new();
    let mut rects: Vec<Option<Rect>> = vec![None; slots.len()];
    // Where the last line ends: (x, box top, gap).
    let mut end = (0.0, 0.0, 8.0);
    for (row, spec) in rows.iter().zip(lines) {
        let mut x = None; // the next x after a box (or text after one)
        let mut first_box = true;
        let mut lead_end = None;
        for piece in row {
            match *piece {
                Piece::Text(s) => {
                    let w = width(s);
                    let (tx, base) = match x {
                        Some(x) => (x, spec.baseline),
                        None => match spec.lead {
                            Lead::Left(l) => (l, spec.lead_baseline),
                            Lead::Right(r) => ((r - w).max(near), spec.lead_baseline),
                        },
                    };
                    if x.is_none() {
                        lead_end = Some(tx + w);
                    }
                    texts.push((tx, base, s.to_owned()));
                    x = Some(tx + w + spec.gap);
                }
                Piece::Slot(name) => {
                    let i = names.iter().position(|n| *n == name).unwrap_or(0);
                    let size = slots[i].1;
                    let left = if first_box {
                        // Room for the lead text, at least 3 px before the box (a right-aligned
                        // one only needs it when it reached `near`).
                        lead_end.map_or(spec.boxes.x, |e| spec.boxes.x.max(e + 3.0))
                    } else {
                        x.unwrap_or(spec.boxes.x)
                    };
                    first_box = false;
                    let rc = Rect::from_min_size(pos2(left, spec.boxes.y), size);
                    if rects[i].is_none() {
                        rects[i] = Some(rc);
                    }
                    x = Some(rc.right() + spec.gap);
                }
            }
        }
        end = (x.unwrap_or(spec.boxes.x), spec.boxes.y, spec.gap);
    }
    // A box the translation left out still gets a place: after the last line.
    let rects = rects
        .into_iter()
        .zip(slots)
        .map(|(rc, (_, size))| {
            rc.unwrap_or_else(|| {
                let rc = Rect::from_min_size(pos2(end.0, end.1), *size);
                end.0 = rc.right() + end.2;
                rc
            })
        })
        .collect();
    (texts, rects)
}

/// A translated sentence with input boxes in it ("Download {files} files at the same time"):
/// the text pieces drawn with `draw(painter, x, baseline, text)` around the boxes, in the
/// translation's order, so a translator can move a box. `lines` place each line as the
/// English has it (one entry per `\n`-separated line); `slots` name the boxes and their sizes.
/// Returns each box's rect on screen, in `slots` order, for the caller to draw its edit there.
/// `draw` gets English pens (draw with [`text`]); `lines` are English too: in RTL the sentence
/// runs from the right, its first piece at the mirrored lead. Nothing starts or ends outside the
/// window: a text that would run past its far edge is cut with "..." and shown whole on hover.
pub fn sentence(
    ui: &Ui,
    p: &Painter,
    template: &str,
    lines: &[SentenceLine],
    slots: &[(&str, egui::Vec2)],
    mut draw: impl FnMut(&Painter, f32, f32, &str),
) -> Vec<Rect> {
    let (near, far) = window_edges(p);
    let (texts, rects) = sentence_layout(template, lines, slots, |s| text_width(p, s), near);
    for (i, (x, baseline, s)) in texts.iter().enumerate() {
        let shown = ellipsize(p, &tahoma(), s, (far - x).max(0.0));
        draw(p, *x, *baseline, &shown);
        if shown != *s {
            let w = text_width(p, &shown);
            let at = rect(Rect::from_min_max(
                pos2(*x, baseline - 11.0),
                pos2(x + w, baseline + 3.0),
            ));
            full_on_hover(ui, at, ("sentence", template, i), s);
        }
    }
    rects.into_iter().map(rect).collect()
}

#[cfg(test)]
mod reflow_tests {
    #[test]
    fn hard_breaks_flow_again() {
        assert_eq!(
            super::reflow("one two\nthree\n\nnew"),
            "one two three\n\nnew"
        );
        assert_eq!(super::reflow("日本語の\n文章"), "日本語の文章");
    }
}
