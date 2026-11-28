//! Release notes: Help → Check for updates' What's new, and the What's new note a new version
//! shows once after an update. The CHANGELOG's Markdown is read into blocks ([`parse`]) and drawn
//! as such ([`view`]): headings in bold, bullets whose wrapped lines stay under their text,
//! **bold** and `code` spans, links as their text, HTML dropped; in a box that scrolls (the
//! wheel, its scroll bar, the keys) and copies its text from its right-click menu.
//!
//! A GitHub release's notes are its CHANGELOG section with the release workflow's additions
//! around it (`.github/workflows/release.yml`: an invitation to try the Mac builds first,
//! `tools/contributors.py`'s "Thanks to" with its avatars last): [`release_section`] keeps the
//! section. The program carries its own CHANGELOG ([`CHANGELOG`]): the note after an update reads
//! its version's section ([`changelog_section`]) without asking the network.

use egui::{Color32, Rect, Ui, pos2};

use crate::dialogs::{Outcome, notice};
use crate::winui::{self, ButtonKind, r};

/// The CHANGELOG this program was built with.
pub const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// How a run of text is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Plain,
    Bold,
    Code,
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// A block of the notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading(Vec<Span>),
    /// A list item (`- `, `* `, `+ `, or `1. `: its `number`), `depth` levels in.
    Bullet {
        depth: usize,
        number: Option<u32>,
        spans: Vec<Span>,
    },
    Para(Vec<Span>),
}

// ---- reading ------------------------------------------------------------------------------------

/// A GitHub release's notes without what the release workflow puts around its CHANGELOG section:
/// the invitation to try the Mac builds (a paragraph starting "**On a Mac?**"), the "Thanks to"
/// section and everything after it, and paragraphs that are only HTML (avatars).
pub fn release_section(body: &str) -> String {
    let body = body.replace("\r\n", "\n");
    let mut kept: Vec<&str> = Vec::new();
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with('#')
            && t.trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case("thanks to")
        {
            break;
        }
        kept.push(line);
    }
    let text = kept.join("\n");
    text.split("\n\n")
        .filter(|p| {
            let t = p.trim();
            !t.is_empty() && !t.starts_with("**On a Mac?**") && !only_html(t)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_owned()
}

/// The notes of `version` in a CHANGELOG (`## [X.Y.Z] - date` up to the next `## `); none when it
/// has no section for it, or an empty one.
pub fn changelog_section(changelog: &str, version: &str) -> Option<String> {
    let head = format!("## [{version}]");
    let text = changelog.replace("\r\n", "\n");
    let mut lines = text.lines();
    lines.by_ref().find(|l| l.starts_with(&head))?;
    let body: Vec<&str> = lines.take_while(|l| !l.starts_with("## ")).collect();
    let s = body.join("\n").trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// The newest released version a CHANGELOG has notes for (not `[Unreleased]`), with its notes.
pub fn latest_section(changelog: &str) -> Option<(String, String)> {
    changelog
        .lines()
        .filter_map(|l| l.strip_prefix("## [")?.split(']').next())
        .filter(|v| v.starts_with(|c: char| c.is_ascii_digit()))
        .find_map(|v| changelog_section(changelog, v).map(|s| (v.to_owned(), s)))
}

/// Whether the What's new note is due at this start: the version whose notes were last shown
/// (`seen`, empty before the first) is another one than `current`, older; with none seen, only
/// for a DDM that ran before (`ran_before`: an update from a version without the note), never on
/// a first start.
pub fn whats_new_due(seen: &str, current: &str, ran_before: bool) -> bool {
    if seen == current {
        return false;
    }
    if seen.is_empty() {
        return ran_before;
    }
    crate::help::newer(current, seen)
}

/// Markdown into blocks: `#` headings, list items (nested by their indent, two spaces a level),
/// paragraphs; a line wrapped in the file goes on its block; lines of HTML alone are dropped.
pub fn parse(md: &str) -> Vec<Block> {
    enum Open {
        Heading,
        Bullet(usize, Option<u32>),
        Para,
    }
    let mut out = Vec::new();
    let mut open: Option<(Open, String)> = None;
    let flush = |open: &mut Option<(Open, String)>, out: &mut Vec<Block>| {
        if let Some((kind, text)) = open.take() {
            let spans = spans(&text);
            if spans.is_empty() {
                return;
            }
            out.push(match kind {
                Open::Heading => Block::Heading(spans),
                Open::Bullet(depth, number) => Block::Bullet {
                    depth,
                    number,
                    spans,
                },
                Open::Para => Block::Para(spans),
            });
        }
    };
    for raw in md.replace("\r\n", "\n").lines() {
        let indent: usize = raw
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum();
        let l = raw.trim();
        if l.is_empty() {
            flush(&mut open, &mut out);
            continue;
        }
        if only_html(l) {
            continue;
        }
        if l.starts_with('#') {
            flush(&mut open, &mut out);
            open = Some((Open::Heading, l.trim_start_matches('#').trim().to_owned()));
            flush(&mut open, &mut out);
            continue;
        }
        if let Some((number, rest)) = list_item(l) {
            flush(&mut open, &mut out);
            open = Some((Open::Bullet((indent / 2).min(3), number), rest.to_owned()));
            continue;
        }
        match &mut open {
            Some((_, text)) => {
                text.push(' ');
                text.push_str(l);
            }
            None => open = Some((Open::Para, l.to_owned())),
        }
    }
    flush(&mut open, &mut out);
    out
}

/// A list item's number (none for `-`, `*`, `+`) and its text.
fn list_item(l: &str) -> Option<(Option<u32>, &str)> {
    for mark in ["- ", "* ", "+ "] {
        if let Some(rest) = l.strip_prefix(mark) {
            return Some((None, rest.trim_start()));
        }
    }
    let digits = l.chars().take_while(char::is_ascii_digit).count();
    let rest = l[digits..].strip_prefix(". ")?;
    let n = l[..digits].parse().ok()?;
    Some((Some(n), rest.trim_start()))
}

/// A line that is only HTML tags (an avatar's `<a><img></a>`, a comment).
fn only_html(l: &str) -> bool {
    l.starts_with('<') && spans(l).iter().all(|s| s.text.trim().is_empty())
}

/// The HTML tag at the start of `s` (`<a href="…">`, `</a>`, `<!-- … -->`): its length.
fn tag_len(s: &str) -> Option<usize> {
    if let Some(rest) = s.strip_prefix("<!--") {
        return rest.find("-->").map(|i| 4 + i + 3);
    }
    let after = s[1..].chars().next()?;
    if !(after.is_ascii_alphabetic() || after == '/') {
        return None;
    }
    let end = s.find('>')?;
    // Not a tag across a sentence ("a < b and c > d").
    (!s[..end].contains("  ")).then_some(end + 1)
}

/// `&amp;` and the like, as their characters.
fn entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// A line's runs: `**bold**` (and `__bold__`), `` `code` `` (its text as it is), `[text](address)`
/// as its text, `<address>` as the address, `*emphasis*` without its marks, HTML tags dropped,
/// `\` escapes.
pub fn spans(line: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let push = |out: &mut Vec<Span>, text: &str, style: Style| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.style == style => last.text.push_str(text),
            _ => out.push(Span {
                text: text.to_owned(),
                style,
            }),
        }
    };
    let mut bold = false;
    let mut em = false;
    let mut i = 0;
    let mut plain = String::new();
    let flush = |out: &mut Vec<Span>, plain: &mut String, bold: bool| {
        let text = entities(plain);
        push(out, &text, if bold { Style::Bold } else { Style::Plain });
        plain.clear();
    };
    while i < line.len() {
        let rest = &line[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if c == '\\'
            && let Some(next) = rest[1..].chars().next()
            && next.is_ascii_punctuation()
        {
            plain.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        if c == '`'
            && let Some(end) = rest[1..].find('`')
        {
            flush(&mut out, &mut plain, bold);
            push(&mut out, &rest[1..1 + end], Style::Code);
            i += end + 2;
            continue;
        }
        if (rest.starts_with("**") || rest.starts_with("__"))
            && (bold || rest[2..].contains(&rest[..2]))
        {
            flush(&mut out, &mut plain, bold);
            bold = !bold;
            i += 2;
            continue;
        }
        // `*emphasis*`: its marks only (a lone `*`, `2 * 3`, stays).
        if c == '*'
            && (em
                || (rest[1..].starts_with(|n: char| !n.is_whitespace()) && rest[1..].contains('*')))
        {
            em = !em;
            i += 1;
            continue;
        }
        if c == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
        {
            plain.push_str(&rest[1..close]);
            i += close + 2 + end + 1;
            continue;
        }
        if c == '<' {
            // An address in angle brackets: the address.
            if let Some(end) = rest.find('>')
                && rest[1..end].contains("://")
                && !rest[1..end].contains(' ')
            {
                plain.push_str(&rest[1..end]);
                i += end + 1;
                continue;
            }
            if let Some(n) = tag_len(rest) {
                i += n;
                continue;
            }
        }
        plain.push(c);
        i += c.len_utf8();
    }
    flush(&mut out, &mut plain, bold);
    if let Some(first) = out.first_mut() {
        first.text = first.text.trim_start().to_owned();
    }
    if let Some(last) = out.last_mut() {
        last.text = last.text.trim_end().to_owned();
    }
    out.retain(|s| !s.text.is_empty());
    out
}

/// The notes as plain text (the clipboard's copy, a screen reader's): a line a block, `•` before a
/// bullet, a blank line between blocks other than two bullets.
pub fn to_plain(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut last_bullet = None;
    for b in blocks {
        let (text, bullet) = match b {
            Block::Heading(s) | Block::Para(s) => (joined(s), false),
            Block::Bullet {
                depth,
                number,
                spans,
            } => {
                let mark = number.map_or_else(|| "•".to_owned(), |n| format!("{n}."));
                (
                    format!("{}{mark} {}", "  ".repeat(*depth), joined(spans)),
                    true,
                )
            }
        };
        match last_bullet {
            None => {}
            Some(true) if bullet => out.push('\n'),
            Some(_) => out.push_str("\n\n"),
        }
        out.push_str(&text);
        last_bullet = Some(bullet);
    }
    out
}

fn joined(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

// ---- laying out ---------------------------------------------------------------------------------

/// Between two lines' baselines.
pub const LINE: f32 = 15.0;
/// Between two blocks; more before a heading.
const GAP: f32 = 5.0;
const HEAD_GAP: f32 = 4.0;
/// A list item's text, from its mark's place; and each level of nesting.
const INDENT: f32 = 13.0;
const DEPTH: f32 = 13.0;
/// Room each side of a code run's text, inside its shading.
pub const CODE_PAD: f32 = 2.0;
/// The first line's baseline, below the top.
const FIRST_BASELINE: f32 = 12.0;

/// A run of text where the notes put it: `x` from the room's left edge, `baseline` from its top.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub baseline: f32,
    pub text: String,
    pub style: Style,
    /// Its width as measured (a code run's shading).
    pub w: f32,
}

/// A list item's mark: a dot, or its number.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub x: f32,
    pub baseline: f32,
    pub number: Option<u32>,
}

/// The notes laid out in a room `width` wide.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    pub runs: Vec<Placed>,
    pub marks: Vec<Mark>,
    /// From the top of the first line to below the last.
    pub height: f32,
}

/// Lay `blocks` out in a room `width` wide, words measured by `measure` (a run's text and style;
/// a heading's runs are bold), wrapped at spaces; a word wider than the room breaks between its
/// characters. A list item's wrapped lines start under its text, not under its mark.
pub fn lay_out(blocks: &[Block], width: f32, measure: &dyn Fn(&str, Style) -> f32) -> Laid {
    let mut laid = Laid::default();
    let space = measure(" ", Style::Plain);
    let mut top = 0.0f32;
    for (k, block) in blocks.iter().enumerate() {
        if matches!(block, Block::Heading(_)) && k > 0 {
            top += HEAD_GAP;
        }
        let (left, spans, heading) = match block {
            Block::Heading(s) => (0.0, s, true),
            Block::Para(s) => (0.0, s, false),
            Block::Bullet {
                depth,
                number,
                spans,
            } => {
                let at = *depth as f32 * DEPTH;
                laid.marks.push(Mark {
                    x: at,
                    baseline: top + FIRST_BASELINE,
                    number: *number,
                });
                let left = at
                    + number.map_or(INDENT, |n| {
                        INDENT.max(measure(&format!("{n}."), Style::Plain) + 4.0)
                    });
                (left, spans, false)
            }
        };
        let room = (width - left).max(8.0);
        // Words: runs of text between spaces, whatever their styles ("**Add URL**," is two
        // words, the comma on the second).
        let mut words: Vec<Vec<(String, Style)>> = Vec::new();
        let mut gap_before = true;
        for s in spans {
            let style = if heading { Style::Bold } else { s.style };
            for (j, part) in s.text.split(' ').enumerate() {
                if j > 0 {
                    gap_before = true;
                }
                if part.is_empty() {
                    continue;
                }
                if gap_before || words.is_empty() {
                    words.push(Vec::new());
                }
                if let Some(w) = words.last_mut() {
                    w.push((part.to_owned(), style));
                }
                gap_before = false;
            }
        }
        let mut line = 0usize;
        let mut x = 0.0f32;
        let place = |laid: &mut Laid, line: usize, x: f32, text: String, style: Style, w: f32| {
            laid.runs.push(Placed {
                x: left + x,
                baseline: top + FIRST_BASELINE + LINE * line as f32,
                text,
                style,
                w,
            });
        };
        for word in words {
            let widths: Vec<f32> = word.iter().map(|(t, s)| measure(t, *s)).collect();
            let w: f32 = widths.iter().sum();
            let start = if x > 0.0 { x + space } else { 0.0 };
            if x > 0.0 && start + w > room {
                line += 1;
                x = 0.0;
            } else {
                x = start;
            }
            if w <= room {
                for ((t, s), pw) in word.into_iter().zip(widths) {
                    place(&mut laid, line, x, t, s, pw);
                    x += pw;
                }
                continue;
            }
            // Wider than the room (an address): broken between its characters.
            for (t, s) in word {
                let mut run = String::new();
                let mut rw = 0.0;
                for ch in t.chars() {
                    let cw = measure(&ch.to_string(), s);
                    if x + rw + cw > room && (x > 0.0 || !run.is_empty()) {
                        if !run.is_empty() {
                            place(&mut laid, line, x, std::mem::take(&mut run), s, rw);
                        }
                        line += 1;
                        x = 0.0;
                        rw = 0.0;
                    }
                    run.push(ch);
                    rw += cw;
                }
                if !run.is_empty() {
                    place(&mut laid, line, x, run, s, rw);
                    x += rw;
                }
            }
        }
        top += LINE * (line + 1) as f32 + GAP;
    }
    laid.height = (top - GAP).max(0.0) + 4.0;
    laid
}

// ---- drawing ------------------------------------------------------------------------------------

/// The font a style is drawn in, and whether it is drawn twice a pixel apart (bold without a bold
/// face, as the download list's names are).
fn font_of(style: Style) -> (egui::FontId, bool) {
    let body = winui::tahoma();
    match style {
        // The bold face at the text's own size (winui::tahoma_bold is a title's, larger).
        Style::Bold if winui::bold_face() => (
            egui::FontId::new(body.size, winui::tahoma_bold().family),
            false,
        ),
        Style::Bold => (body, true),
        Style::Plain | Style::Code => (body, false),
    }
}

/// A run's width in its style.
fn width_of(p: &egui::Painter, text: &str, style: Style) -> f32 {
    let (font, twice) = font_of(style);
    let w = winui::text_width_font(p, font, text);
    match style {
        Style::Code => w + 2.0 * CODE_PAD,
        _ if twice => w + 1.0,
        _ => w,
    }
}

/// Notes in `area` (English coordinates; its frame drawn by the caller), read left to right in
/// every language as data: scrolled by the wheel or a trackpad over it, its scroll bar when they
/// are longer than the box, and with `keys` the window's Up, Down, Page Up, Page Down, Home and
/// End; their text copied from its right-click menu. Where it is scrolled to is kept under `id`.
pub fn view(ui: &mut Ui, id: egui::Id, area: Rect, blocks: &[Block], ink: Color32, keys: bool) {
    use crate::scrollbar;
    const PAD: f32 = 7.0;
    let p = ui.painter().clone();
    let measure = |t: &str, s: Style| width_of(&p, t, s);
    let inner_h = area.height() - 6.0;
    let mut text_right = area.right() - PAD;
    let mut laid = lay_out(blocks, text_right - (area.left() + PAD), &measure);
    let bar = laid.height > inner_h;
    if bar {
        text_right = area.right() - 1.0 - scrollbar::THICK - 4.0;
        laid = lay_out(blocks, text_right - (area.left() + PAD), &measure);
    }
    let total = laid.height;
    let max_top = (total - inner_h).max(0.0);
    let mut top: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let resp = ui.interact(winui::rect(area), id.with("area"), winui::pointer_only());
    if resp.hovered() {
        top += winui::wheel_rows(ui, LINE, (inner_h / LINE).floor().max(1.0)) * LINE;
    }
    let plain = to_plain(blocks);
    winui::copy_menu(&resp, &plain);
    // What a screen reader reads of the box: the notes as text.
    ui.ctx().accesskit_node_builder(resp.id, |node| {
        node.set_role(egui::accesskit::Role::Document);
        node.set_value(plain.clone());
    });
    if keys {
        let page = (inner_h - LINE).max(LINE);
        ui.input(|i| {
            for (key, by) in [
                (egui::Key::ArrowUp, -LINE),
                (egui::Key::ArrowDown, LINE),
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
    if ui
        .data(|d| d.get_temp::<f32>(id))
        .is_some_and(|was| was != top)
    {
        ui.ctx().request_repaint();
    }
    let clip = p.with_clip_rect(winui::rect(area.shrink(2.0)));
    // Data keeps its direction: from the room's left edge on screen, in an RTL window too.
    let left = winui::data_left(area.left() + PAD, text_right);
    let ppp = ui.ctx().pixels_per_point();
    let scrolled = (top * ppp).round() / ppp;
    let y0 = area.top() + 3.0 - scrolled;
    let shown = |baseline: f32| baseline > area.top() - LINE && baseline < area.bottom() + LINE;
    let shade = ink.gamma_multiply(0.12);
    for run in &laid.runs {
        let baseline = y0 + run.baseline;
        if !shown(baseline) {
            continue;
        }
        let x = left + run.x;
        let (font, twice) = font_of(run.style);
        if run.style == Style::Code {
            clip.rect_filled(
                Rect::from_min_max(pos2(x, baseline - 11.0), pos2(x + run.w, baseline + 3.0)),
                2.0,
                shade,
            );
            winui::data_run(&clip, font, x + CODE_PAD, baseline, &run.text, ink);
            continue;
        }
        winui::data_run(&clip, font.clone(), x, baseline, &run.text, ink);
        if twice {
            winui::data_run(&clip, font, x + 1.0, baseline, &run.text, ink);
        }
    }
    for mark in &laid.marks {
        let baseline = y0 + mark.baseline;
        if !shown(baseline) {
            continue;
        }
        match mark.number {
            Some(n) => {
                winui::data_run(
                    &clip,
                    winui::tahoma(),
                    left + mark.x,
                    baseline,
                    &format!("{n}."),
                    ink,
                );
            }
            None => {
                clip.circle_filled(pos2(left + mark.x + 4.0, baseline - 3.5), 2.0, ink);
            }
        }
    }
    if bar {
        let track = winui::rect(Rect::from_min_max(
            pos2(area.right() - 1.0 - scrollbar::THICK, area.top() + 1.0),
            pos2(area.right() - 1.0, area.bottom() - 1.0),
        ));
        if scrollbar::bar(
            ui,
            id.with("bar"),
            track,
            true,
            total,
            inner_h,
            LINE,
            &mut top,
        ) {
            ui.ctx().request_repaint();
        }
    }
    ui.data_mut(|d| d.insert_temp(id, top));
}

// ---- the note after an update -------------------------------------------------------------------

/// The window's title.
// i18n: title of the window a new version shows once after an update, with what changed in it
pub const WHATS_NEW: &str = crate::i18n::mark("What's new");

/// What's new in this version, shown once at its first start after an update (app.rs decides
/// when: [`whats_new_due`]), from the CHANGELOG it was built with.
pub struct WhatsNew {
    version: String,
    blocks: Vec<Block>,
}

impl WhatsNew {
    /// This version's notes; none when the CHANGELOG has no section for it (a build between
    /// releases).
    pub fn new() -> Option<Self> {
        let text = changelog_section(CHANGELOG, crate::help::VERSION)?;
        Some(Self::of(crate::help::VERSION, &text))
    }

    /// The newest release's notes, for the window's pictures (`/show whats-new` in a build
    /// between releases).
    pub fn latest() -> Option<Self> {
        Self::new().or_else(|| {
            let (version, text) = latest_section(CHANGELOG)?;
            Some(Self::of(&version, &text))
        })
    }

    fn of(version: &str, text: &str) -> Self {
        WhatsNew {
            version: version.to_owned(),
            blocks: parse(text),
        }
    }

    /// `Closed` once read (OK, Esc, the window's ×). Shown without taking the keyboard from the
    /// program in front: it comes by itself.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let (version, blocks) = (self.version.clone(), &self.blocks);
        notice(
            ctx,
            "whats-new",
            WHATS_NEW,
            [440.0, 320.0],
            false,
            winui::BG(),
            false,
            |ui, o| {
                let p = ui.painter().clone();
                let ink = winui::TEXT();
                let right = o.x + 429.0;
                let headline = crate::i18n::fmt(
                    // i18n: headline of the window a new version shows once after an update; {version} like "1.2.0"
                    crate::i18n::t("What's new in Dannex Download Manager {version}"),
                    &[("version", &version)],
                );
                let shown =
                    winui::ellipsize(&p, &winui::tahoma_bold(), &headline, right - o.x - 11.0);
                let drawn = winui::text_font(
                    &p,
                    winui::tahoma_bold(),
                    o.x + 11.0,
                    o.y + 22.0,
                    &shown,
                    ink,
                );
                if shown != headline {
                    winui::full_on_hover(ui, drawn, "whats-new-headline", &headline);
                }
                let area = egui::Rect::from_min_max(
                    pos2(o.x + 11.0, o.y + 34.0),
                    pos2(right, o.y + 276.0),
                );
                winui::field_frame(&p, winui::rect(area), winui::FieldLook::ReadOnly);
                view(
                    ui,
                    egui::Id::unique("whats-new-notes"),
                    area,
                    blocks,
                    ink,
                    true,
                );
                let placed = winui::buttons_to_right(&p, 429.0, 9.0, &[("OK", 73.0)], 150.0);
                let ok = winui::button(
                    ui,
                    r(o, placed[0].0, 287.0, placed[0].1, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                    None,
                )
                .clicked();
                if ok || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
                    Outcome::Closed
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> Span {
        Span {
            text: s.into(),
            style: Style::Plain,
        }
    }
    fn bold(s: &str) -> Span {
        Span {
            text: s.into(),
            style: Style::Bold,
        }
    }
    fn code(s: &str) -> Span {
        Span {
            text: s.into(),
            style: Style::Code,
        }
    }

    /// 1.1.5's notes on GitHub (`gh release view v1.1.5`), as release.yml made them: the Mac
    /// builds' invitation, the CHANGELOG section; and the "Thanks to" a release with contributors
    /// ends with (tools/contributors.py thanks_markdown).
    const BODY_115: &str = "**On a Mac?** The `-experimental` files are fresh builds for Mac (Apple Silicon and Intel) that we haven't tried ourselves yet. Want to be among the first? Give them a go and tell us how it went in an [issue](https://github.com/Riskraptor/dannexdownloadmanager/issues).\n\n- **Verified Metalink downloads:** open a `.metalink` or `.meta4` address in **Add URL** and DDM\n  reads its publisher-provided file address, alternate mirrors and strongest checksum. If the\n  first server cannot answer, DDM can continue with a mirror only when that checksum makes the\n  switch safe; credentials never go to another mirror.\n- **HTTP/2 downloads:** HTTPS downloads negotiate HTTP/2 when the server offers it, and keep the\n  existing HTTP/1.1 path as a reliable fallback.\n";
    const THANKS: &str = "\n\n## Thanks to\n\nThe people who made this release possible with a pull request, an issue or a comment:\n\n<a href=\"https://github.com/someone\"><img src=\"https://avatars.githubusercontent.com/u/1?v=4&s=64\" width=\"32\" height=\"32\" alt=\"@someone\"></a>\n\n@someone\n";

    #[test]
    fn a_release_keeps_its_changelog_section_only() {
        let body = format!("{BODY_115}{THANKS}").replace('\n', "\r\n");
        let section = release_section(&body);
        assert!(
            section.starts_with("- **Verified Metalink downloads:**"),
            "{section}"
        );
        assert!(section.ends_with("reliable fallback."), "{section}");
        assert!(!section.contains("On a Mac"));
        assert!(
            !section.contains("Thanks to")
                && !section.contains("<img")
                && !section.contains("@someone")
        );
        // As CHANGELOG.md has it (its section for 1.1.5 is the same text).
        let ours = changelog_section(CHANGELOG, "1.1.5");
        if let Some(ours) = ours {
            assert_eq!(section, ours);
        }
        // Notes without the workflow's additions stay as they are.
        assert_eq!(release_section("- One\n- Two"), "- One\n- Two");
        // A paragraph of HTML alone goes, the text around it stays.
        assert_eq!(
            release_section("Text\n\n<p><img src=x></p>\n\nMore"),
            "Text\n\nMore"
        );
    }

    #[test]
    fn a_version_s_section_is_read_from_the_changelog() {
        let log = "# Changelog\n\nIntro.\n\n## [Unreleased]\n\n- Soon\n\n## [1.2.0] - 2026-12-01\n\n- **New:** a thing\n  wrapped.\n\n### Fixed\n\n- A bug\n\n## [1.1.5] - 2026-11-25\n\n- Older\n";
        assert_eq!(
            changelog_section(log, "1.2.0").as_deref(),
            Some("- **New:** a thing\n  wrapped.\n\n### Fixed\n\n- A bug")
        );
        assert_eq!(changelog_section(log, "1.1.5").as_deref(), Some("- Older"));
        assert_eq!(changelog_section(log, "1.1"), None, "1.1 isn't 1.1.5");
        assert_eq!(changelog_section(log, "9.9.9"), None);
        assert_eq!(
            changelog_section("## [1.0.0]\n\n## [0.9.0]\n- x", "1.0.0"),
            None,
            "empty"
        );
        assert_eq!(
            latest_section(log).map(|(v, _)| v).as_deref(),
            Some("1.2.0")
        );
        // The CHANGELOG built in has the running version's notes or, between releases, a newest.
        assert!(latest_section(CHANGELOG).is_some());
    }

    #[test]
    fn markdown_becomes_blocks() {
        let blocks = parse(
            "Fixes for\nthe Mac.\n\n### Fixed\n\n- **Menus** did\n  nothing.\n- Paste `x<y>` from [the guide](docs/a.md#b)\n  - nested\n1. first\n<img src=\"a.png\">\n\n## Thanks",
        );
        assert_eq!(
            blocks,
            vec![
                Block::Para(vec![plain("Fixes for the Mac.")]),
                Block::Heading(vec![plain("Fixed")]),
                Block::Bullet {
                    depth: 0,
                    number: None,
                    spans: vec![bold("Menus"), plain(" did nothing.")]
                },
                Block::Bullet {
                    depth: 0,
                    number: None,
                    spans: vec![plain("Paste "), code("x<y>"), plain(" from the guide")]
                },
                Block::Bullet {
                    depth: 1,
                    number: None,
                    spans: vec![plain("nested")]
                },
                Block::Bullet {
                    depth: 0,
                    number: Some(1),
                    spans: vec![plain("first")]
                },
                Block::Heading(vec![plain("Thanks")]),
            ]
        );
    }

    #[test]
    fn inline_marks_become_runs() {
        assert_eq!(
            spans("**Verified Metalink downloads:** open a `.metalink` in **Add URL**."),
            vec![
                bold("Verified Metalink downloads:"),
                plain(" open a "),
                code(".metalink"),
                plain(" in "),
                bold("Add URL"),
                plain("."),
            ]
        );
        assert_eq!(
            spans("Tell us in an [issue](https://x/issues)."),
            vec![plain("Tell us in an issue.")]
        );
        assert_eq!(
            spans("[not a link] and [x]"),
            vec![plain("[not a link] and [x]")]
        );
        assert_eq!(
            spans("<a href=\"x\">Avatar</a> &amp; *more*"),
            vec![plain("Avatar & more")]
        );
        assert_eq!(
            spans("See <https://x.test/a>"),
            vec![plain("See https://x.test/a")]
        );
        assert_eq!(
            spans("2 < 3 and 2 * 3 stay"),
            vec![plain("2 < 3 and 2 * 3 stay")]
        );
        assert_eq!(
            spans(r"a \*star\* and snake_case_name"),
            vec![plain("a *star* and snake_case_name")]
        );
        assert_eq!(
            spans("`<ddm-video-button>`"),
            vec![code("<ddm-video-button>")]
        );
    }

    #[test]
    fn notes_copy_as_plain_lines() {
        let blocks =
            parse("Fixes for\nthe Mac.\n\n### Fixed\n\n- **Menus** did\n  nothing.\n- Paste");
        assert_eq!(
            to_plain(&blocks),
            "Fixes for the Mac.\n\nFixed\n\n• Menus did nothing.\n• Paste"
        );
        assert_eq!(to_plain(&parse("1. a\n  - b")), "1. a\n  • b");
    }

    /// Each character 6 px, a space 3 px: a list item's wrapped lines start under its text.
    #[test]
    fn a_bullet_wraps_under_its_text() {
        let measure = |t: &str, s: Style| {
            let w = t.chars().count() as f32 * if t == " " { 3.0 } else { 6.0 };
            if s == Style::Code {
                w + 2.0 * CODE_PAD
            } else {
                w
            }
        };
        let blocks = parse("- aaaa bbbb cccc dddd\n\nNext");
        // Room for two words a line after the mark's indent: 13 + 24 + 3 + 24 = 64.
        let laid = lay_out(&blocks, 66.0, &measure);
        let xs: Vec<(f32, f32, &str)> = laid
            .runs
            .iter()
            .map(|r| (r.x, r.baseline, r.text.as_str()))
            .collect();
        assert_eq!(
            xs,
            vec![
                (13.0, 12.0, "aaaa"),
                (40.0, 12.0, "bbbb"),
                (13.0, 27.0, "cccc"),
                (40.0, 27.0, "dddd"),
                (0.0, 47.0, "Next"),
            ]
        );
        assert_eq!(
            laid.marks,
            vec![Mark {
                x: 0.0,
                baseline: 12.0,
                number: None
            }]
        );
        assert_eq!(laid.height, 2.0 * LINE + GAP + LINE + 4.0);
        // A word glued to the next run ("**URL**,") moves as one.
        let glued = lay_out(&parse("aaaaaaaa **bb**,"), 60.0, &measure);
        assert_eq!(glued.runs[1].baseline, glued.runs[2].baseline);
        assert_eq!(glued.runs[2].x, glued.runs[1].x + 12.0);
        // A word wider than the room breaks between its characters.
        let long = lay_out(&parse("abcdefghij"), 30.0, &measure);
        let texts: Vec<&str> = long.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["abcde", "fghij"]);
        // A heading's runs are bold, with more room above it.
        let head = lay_out(&parse("a\n\n## H"), 100.0, &measure);
        assert_eq!(head.runs[1].style, Style::Bold);
        assert_eq!(head.runs[1].baseline, 12.0 + LINE + GAP + HEAD_GAP);
    }

    #[test]
    fn the_note_shows_once_after_an_update_never_on_a_first_start() {
        // A first start: nothing seen, never ran.
        assert!(!whats_new_due("", "1.2.0", false));
        // An update from a version before the note (nothing seen, but it ran).
        assert!(whats_new_due("", "1.2.0", true));
        // An update from a version that had it.
        assert!(whats_new_due("1.1.5", "1.2.0", true));
        assert!(whats_new_due("1.2.0-rc.1", "1.2.0", true));
        // Seen already, or an older version put back.
        assert!(!whats_new_due("1.2.0", "1.2.0", true));
        assert!(!whats_new_due("1.3.0", "1.2.0", true));
    }
}
