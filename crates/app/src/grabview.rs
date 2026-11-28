//! Link grabber's window (#37), drawn from its model (`grabber.rs`): a search box and the
//! buttons that tick what is shown; a button per kind of file, each with its count and a tick
//! for all of that kind; the links in columns (name, the text the page shows for it, site, size,
//! type, what looking it up found), in their packages or not, sorted by any column; the box of
//! the selected package (its name, category, folder, archive password and what becomes of its
//! archives); and what Download now or Add to queue would add.
//!
//! The keys: Ctrl+F the search box (Escape empties it, Enter goes back to the list), Ctrl+A
//! ticks every link shown, Ctrl+Shift+A unticks them, Ctrl+I turns them round, Alt+1 to Alt+8
//! show one kind of file (Alt+1 all), Space ticks the selected row, Delete takes it out, Left and
//! Right close and open a package (or a file's second links), Ctrl+C copies the selected link's
//! address, Ctrl+V adds the addresses in the clipboard, Enter adds the ticked links.

use egui::{Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::dialogs::{Outcome, window};
use crate::grabber::{Col, Grabber, Kind, LinkState, Row};
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, fill_l as fill, r, text};

/// The window's client size.
const W: f32 = 960.0;
const H: f32 = 604.0;
const M: f32 = 12.0;
/// The rows' tops: the search box and the tick buttons, the kinds' buttons, the list, the line
/// under it, the package's box, the last row of buttons.
const TOP: f32 = 12.0;
const CHIPS_Y: f32 = 45.0;
const LIST_Y: f32 = 80.0;
const LIST_H: f32 = 326.0;
const UNDER_Y: f32 = LIST_Y + LIST_H + 8.0;
const BOX_Y: f32 = UNDER_Y + 28.0;
const BOX_H: f32 = 112.0;
const BUTTONS_Y: f32 = BOX_Y + BOX_H + 14.0;
const HEADER_H: f32 = 24.0;
const CHIP_H: f32 = 24.0;
const SEARCH_W: f32 = 270.0;

crate::theme::themed! {
    LIST_HEAD => header;
    LIST_LINE => table_line;
    HEADER_SEP => header_sep;
    SORTED => sorted;
    SORT_ARROW => sort_arrow;
    ACCENT => accent;
    ON_ACCENT => button_accent_text;
    CHIP => button;
    CHIP_EDGE => button_border;
    WARNING => warning;
}

/// A column's place in the list (English x from the list's inner left), its title and whether
/// its text ends at its right (sizes).
struct Column {
    col: Col,
    left: f32,
    right: f32,
    title: &'static str,
    end: bool,
}

/// The columns, laid out for the list's inner width `w` (the last one takes what is left).
fn columns(w: f32) -> [Column; 6] {
    [
        Column {
            col: Col::Name,
            left: 0.0,
            right: 320.0,
            // i18n: column title: file name
            title: crate::i18n::mark("Name"),
            end: false,
        },
        Column {
            col: Col::Text,
            left: 320.0,
            right: 505.0,
            // i18n: Link grabber: column title: the words the page shows for the link (its text, a picture's description)
            title: crate::i18n::mark("Link text"),
            end: false,
        },
        Column {
            col: Col::Site,
            left: 505.0,
            right: 630.0,
            // i18n: Link grabber: column title: the web site the link points to (example.com)
            title: crate::i18n::mark("Site"),
            end: false,
        },
        Column {
            col: Col::Size,
            left: 630.0,
            right: 712.0,
            // i18n: column title: file size
            title: crate::i18n::mark("Size"),
            end: true,
        },
        Column {
            col: Col::Type,
            left: 712.0,
            right: 768.0,
            // i18n: Link grabber: column title: the file's type (its extension: PDF, ZIP)
            title: crate::i18n::mark("Type"),
            end: false,
        },
        Column {
            col: Col::Status,
            left: 768.0,
            right: w,
            // i18n: column title: Link grabber: what looking the link up found (Online, Offline…)
            title: crate::i18n::mark("Status"),
            end: false,
        },
    ]
}

/// The window: `Done(true)` for Download now, `Done(false)` for Add to queue.
pub fn show(g: &mut Grabber, ctx: &egui::Context) -> Outcome<bool> {
    window(
        ctx,
        "grabber",
        "Link grabber",
        [W, H],
        true,
        LIGHT_BG(),
        |ui, o| body(g, ui, o),
    )
}

fn body(g: &mut Grabber, ui: &mut Ui, o: Pos2) -> Outcome<bool> {
    let p = ui.painter().clone();
    // Ctrl+V: the addresses in the pasted text, as Import finds them in a file; not while a box
    // has the keys (the text goes there).
    let pasted: Option<String> = ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Paste(t) => Some(t.clone()),
            _ => None,
        })
    });
    if let Some(t) = pasted.filter(|_| !g.editing) {
        let found = crate::transfer::import(&t);
        // i18n: Link grabber: where links came from: the clipboard (pasted with Ctrl+V)
        let from = crate::i18n::t("Pasted").to_owned();
        g.add(found, from, |_| false);
    }
    let typing = g.editing;
    let keys = shortcuts(ui, typing);

    // Row 1: the search box, where the links came from, the tick buttons.
    let search_rc = r(o, M, TOP, SEARCH_W, 23.0);
    let mut search = g.search.clone();
    let typed = winui::edit_light(ui, search_rc, &mut search, true, false);
    // i18n: what a screen reader calls Link grabber's search box
    let search_name = crate::i18n::t("Search links");
    ui.ctx()
        .accesskit_node_builder(typed.id, |n| n.set_label(search_name));
    if std::mem::take(&mut g.focus_search) || keys.find {
        typed.request_focus();
    }
    if typed.has_focus() {
        // Escape empties it first (the window closes on the next one); Enter goes to the list.
        if !search.is_empty() && winui::take_key(ui, egui::Key::Escape) {
            search.clear();
        }
        if winui::take_key(ui, egui::Key::Enter) {
            typed.surrender_focus();
        }
    }
    if search.is_empty() && !typed.has_focus() {
        // i18n: Link grabber: the empty search box's hint (Ctrl+F is its shortcut)
        let hint = crate::i18n::t("Search links (Ctrl+F)");
        winui::fit(
            ui,
            &p,
            o.x + M + 6.0,
            o.y + TOP + 16.0,
            hint,
            winui::MUTED(),
            o.x + M + SEARCH_W - 6.0,
            "grabber-search-hint",
        );
    }
    g.set_search(search);
    let ticks = winui::buttons_light_right(
        ui,
        o,
        W - M,
        TOP,
        &[
            // i18n: Link grabber button: tick every link shown
            (crate::i18n::mark("Tick all"), ButtonKind::Normal),
            // i18n: Link grabber button: untick every link shown
            (crate::i18n::mark("Tick none"), ButtonKind::Normal),
            // i18n: Link grabber button: tick the unticked links shown and untick the ticked ones
            (crate::i18n::mark("Invert"), ButtonKind::Normal),
        ],
    );
    let tips = [
        // i18n: tooltip of Link grabber's Tick all button
        crate::i18n::t("Tick every link shown (Ctrl+A)"),
        // i18n: tooltip of Link grabber's Tick none button
        crate::i18n::t("Untick every link shown (Ctrl+Shift+A)"),
        // i18n: tooltip of Link grabber's Invert button
        crate::i18n::t("Tick the links shown that aren't, untick those that are (Ctrl+I)"),
    ];
    let mut tick_left = W - M;
    for (b, tip) in ticks.iter().zip(tips) {
        b.clone().on_hover_text(tip);
        tick_left = tick_left.min(winui_left(o, b.rect));
    }
    if ticks[0].clicked() || keys.all {
        g.tick_shown(true);
    }
    if ticks[1].clicked() || keys.none {
        g.tick_shown(false);
    }
    if ticks[2].clicked() || keys.invert {
        g.invert_shown();
    }
    if !g.source().is_empty() {
        let line = crate::i18n::fmt(
            // i18n: Link grabber: {source} is where the links came from ("Links from the browser", a file's name, "Pasted")
            crate::i18n::t("Source: {source}"),
            &[("source", g.source())],
        );
        winui::fit(
            ui,
            &p,
            o.x + M + SEARCH_W + 14.0,
            o.y + TOP + 16.0,
            &line,
            winui::MUTED(),
            o.x + tick_left - 12.0,
            "grabber-source",
        );
    }

    // Row 2: a button per kind of file.
    chips(g, ui, o, keys.kind);

    // The list.
    list(
        g,
        ui,
        Rect::from_min_size(o + vec2(M, LIST_Y), vec2(W - 2.0 * M, LIST_H)),
    );

    // Under the list: Hide web pages, Group into packages, the look-ups' count.
    let mut hide = g.hide_pages;
    // i18n: Link grabber check box: links to web pages (HTML files) are not shown
    winui::checkbox_light(
        ui,
        o + vec2(M, UNDER_Y + 3.0),
        &mut hide,
        "Hide web pages",
        true,
    );
    g.set_hide_pages(hide);
    let after = M + 18.0 + winui::text_width(&p, crate::i18n::t("Hide web pages")) + 24.0;
    let mut grouped = g.grouped;
    winui::checkbox_light(
        ui,
        o + vec2(after, UNDER_Y + 3.0),
        &mut grouped,
        // i18n: Link grabber check box: links shown in their packages (a split archive's volumes, a page's links); unticked, one list
        "Group into packages",
        true,
    );
    g.set_grouped(grouped);
    looking_up(g, ui, o);

    let boxes = package_box(g, ui, o);
    g.editing = boxes || typed.has_focus();

    // The last row: what would be added, and the buttons.
    let (ticked, total, bytes, unknown) = g.summary();
    let mut line = crate::i18n::fmt(
        // i18n: Link grabber, under its list: {n} links ticked of {total} (the ticked ones are added)
        crate::i18n::t("{n} of {total} links ticked"),
        &[
            ("n", &crate::i18n::integer(ticked as u64)),
            ("total", &crate::i18n::integer(total as u64)),
        ],
    );
    if ticked > 0 && bytes > 0 {
        line = crate::regional::list(&[&line, &crate::dialogs::dialog_size(bytes)]);
    }
    if ticked > 0 && unknown > 0 {
        let more = crate::i18n::tn(
            unknown as u64,
            // i18n: Link grabber: after the ticked links' size: {n} of them have no size known (yet)
            "{n} of unknown size",
            "{n} of unknown size",
        );
        line = crate::regional::list(&[&line, &more]);
    }
    let any = ticked > 0;
    let kind = |default: bool| match (any, default) {
        (false, _) => ButtonKind::Disabled,
        (true, true) => ButtonKind::Default,
        (true, false) => ButtonKind::Normal,
    };
    let row = winui::buttons_light_right(
        ui,
        o,
        W - M,
        BUTTONS_Y,
        &[
            // i18n: Link grabber button: add the ticked links to the download queue without starting them
            (crate::i18n::mark("Add to queue"), kind(false)),
            // i18n: Link grabber button: download the ticked links now
            (crate::i18n::mark("Download now"), kind(true)),
            (crate::i18n::mark("Cancel"), ButtonKind::Normal),
        ],
    );
    let left = row.iter().map(|b| winui_left(o, b.rect)).fold(W, f32::min);
    winui::fit(
        ui,
        &p,
        o.x + M,
        o.y + BUTTONS_Y + 15.0,
        &line,
        LIGHT_TEXT(),
        o.x + left - 12.0,
        "grabber-summary",
    );
    let (later, start, cancel) = (row[0].clicked(), row[1].clicked(), row[2].clicked());
    // Escape and Enter as in any dialog (the search box takes its own first).
    if cancel || winui::escape_pressed(ui) {
        Outcome::Closed
    } else if any && (start || winui::enter_pressed(ui)) {
        Outcome::Done(true)
    } else if any && later {
        Outcome::Done(false)
    } else {
        Outcome::Open
    }
}

/// A button's left edge on screen as an English x from the window's origin (its right in
/// right-to-left).
fn winui_left(o: Pos2, rc: Rect) -> f32 {
    winui::rect(rc).left() - o.x
}

/// The window's own keys this frame.
#[derive(Default)]
struct Keys {
    find: bool,
    all: bool,
    none: bool,
    invert: bool,
    /// Alt+1 (all) to Alt+8: the kind to show.
    kind: Option<Option<Kind>>,
}

fn shortcuts(ui: &Ui, typing: bool) -> Keys {
    use egui::{Key, Modifiers};
    let mut k = Keys::default();
    ui.input_mut(|i| {
        k.find = i.consume_key(Modifiers::COMMAND, Key::F);
        if !typing {
            k.none = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
            k.all = i.consume_key(Modifiers::COMMAND, Key::A);
            k.invert = i.consume_key(Modifiers::COMMAND, Key::I);
        }
        let digits = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
        ];
        for (n, key) in digits.into_iter().enumerate() {
            if i.consume_key(Modifiers::ALT, key) {
                k.kind = Some(n.checked_sub(1).map(|n| Kind::ALL[n]));
            }
        }
    });
    k
}

/// The kinds' buttons: "All" and each kind with its count; a click shows that kind only, its
/// box ticks (or unticks) every link of that kind the search finds.
fn chips(g: &mut Grabber, ui: &mut Ui, o: Pos2, key: Option<Option<Kind>>) {
    let p = ui.painter().clone();
    let counts = g.kind_counts();
    let all: usize = counts.iter().map(|c| c.0).sum();
    // i18n: Link grabber: the filter button showing links of every kind
    let all_label = crate::i18n::t("All");
    let items: Vec<(Option<Kind>, &str, usize, usize)> = std::iter::once((None, all_label, all, 0))
        .chain(Kind::ALL.iter().map(|k| {
            (
                Some(*k),
                crate::i18n::t(k.label()),
                counts[*k as usize].0,
                counts[*k as usize].1,
            )
        }))
        .collect();
    let count_text = |n: usize| crate::i18n::integer(n as u64);
    // Each one's width: its box (not All's), its name and its count, with room around them;
    // made narrower together when a language's names don't fit in one row.
    let natural = |pad: f32| -> Vec<f32> {
        items
            .iter()
            .map(|(k, label, n, _)| {
                let check = if k.is_some() && *n > 0 { 19.0 } else { 0.0 };
                (pad * 2.0
                    + check
                    + winui::text_width(&p, label)
                    + 6.0
                    + winui::text_width(&p, &count_text(*n)))
                .ceil()
            })
            .collect()
    };
    let room = W - 2.0 * M;
    let gap = 6.0;
    let mut pad = 10.0;
    let mut widths = natural(pad);
    let need = |w: &[f32]| w.iter().sum::<f32>() + gap * (w.len() - 1) as f32;
    if need(&widths) > room {
        pad = 6.0;
        widths = natural(pad);
    }
    let squeeze = if need(&widths) > room {
        (room - gap * (widths.len() - 1) as f32) / widths.iter().sum::<f32>()
    } else {
        1.0
    };
    let mut x = o.x + M;
    let y = o.y + CHIPS_Y;
    for (n, ((kind, label, count, ticked), w)) in items.iter().zip(widths).enumerate() {
        let w = if squeeze < 1.0 {
            (w * squeeze).floor()
        } else {
            w
        };
        let chip = Rect::from_min_size(pos2(x, y), vec2(w, CHIP_H));
        x += w + gap;
        let chosen = g.filter == *kind;
        let empty = *count == 0 && kind.is_some();
        // The box: its own click; the rest of the button shows the kind.
        let check_w = if kind.is_some() && !empty { 19.0 } else { 0.0 };
        let face = Rect::from_min_max(pos2(chip.left() + pad + check_w, chip.top()), chip.max);
        let id = ui.scope_id().with(("grabber-chip", n));
        let resp = ui.interact(
            winui::rect(face),
            id,
            if empty {
                Sense::hover()
            } else {
                Sense::click()
            },
        );
        let state = winui::State::of(&resp, !empty);
        let (fill_c, ink, count_ink) = if chosen {
            (ACCENT(), ON_ACCENT(), ON_ACCENT())
        } else {
            let c = crate::theme::colors();
            let f = winui::subtle_fill(state, c).unwrap_or(CHIP());
            let ink = if empty {
                winui::LIGHT_DISABLED()
            } else {
                LIGHT_TEXT()
            };
            (f, ink, winui::MUTED())
        };
        let shape = winui::rect(chip);
        p.rect_filled(shape, CHIP_H / 2.0, fill_c);
        if !chosen {
            p.rect_stroke(
                shape,
                CHIP_H / 2.0,
                egui::Stroke::new(1.0, CHIP_EDGE()),
                egui::StrokeKind::Inside,
            );
        }
        let base = y + 16.0;
        let mut tx = chip.left() + pad;
        if let Some(k) = kind
            && !empty
        {
            let glyph = pos2(tx, y + 5.5);
            let mut on = *ticked == *count && *count > 0;
            let what = crate::i18n::fmt(
                // i18n: what a screen reader calls the box on a Link grabber filter button; {kind} is its kind of file (Video, Images…)
                crate::i18n::t("Tick every {kind} link"),
                &[("kind", label)],
            );
            if winui::row_check(ui, &p, glyph, &mut on, &what, ("grabber-chip-tick", n)) {
                g.tick_kind(*k);
            }
            if *ticked > 0 && ticked < count {
                winui::row_check_mixed(&p, glyph);
            }
        }
        tx += check_w;
        let label_w = winui::text_width(&p, label);
        let count_s = count_text(*count);
        let count_w = winui::text_width(&p, &count_s);
        let label_room = (chip.right() - pad - count_w - 6.0 - tx).max(10.0);
        let shown = if label_w > label_room + 0.5 {
            winui::ellipsize(&p, &winui::tahoma(), label, label_room)
        } else {
            (*label).to_owned()
        };
        let drawn = text(&p, tx, base, &shown, ink);
        let after = tx + drawn.width().min(label_room) + 6.0;
        text(&p, after, base, &count_s, count_ink);
        let tip = match kind {
            Some(_) => crate::i18n::fmt(
                // i18n: tooltip of a Link grabber filter button; {kind} is its kind of file (Video, Images…); its box ticks them all
                crate::i18n::t("Show only {kind} links (Alt+{n}); its box ticks them all"),
                &[("kind", label), ("n", &(n + 1).to_string())],
            ),
            // i18n: tooltip of Link grabber's All filter button
            None => crate::i18n::t("Show links of every kind (Alt+1)").to_owned(),
        };
        winui::expose(&resp, egui::Role::RadioButton, label, !empty, Some(chosen));
        if resp.clone().on_hover_text(tip).clicked() || key == Some(*kind) {
            g.set_filter(*kind);
        }
    }
}

/// The links, in their packages or not: a package's arrow (a double-click, Left / Right) opens
/// and closes it, its box ticks all its links; a link's row has its box, its file's icon and
/// name (with what is known of it: in the list, on the disk, other links to the same file), the
/// text the page shows for it, its site, size, type and status.
fn list(g: &mut Grabber, ui: &mut Ui, list: Rect) {
    let p = ui.painter().clone();
    winui::solid(&p, winui::rect(list), winui::FIELD_EDGE());
    let inner = list.shrink(1.0);
    p.rect_filled(winui::rect(inner), 0.0, winui::FIELD());
    let head = Rect::from_min_size(inner.min, vec2(inner.width(), HEADER_H));
    p.rect_filled(winui::rect(head), 0.0, LIST_HEAD());
    fill(
        &p,
        head.left(),
        head.bottom() - 1.0,
        head.right() - 1.0,
        head.bottom() - 1.0,
        LIST_LINE(),
    );
    let lx = inner.left();
    let cols = columns(inner.width() - crate::scrollbar::THICK);
    // The titles: each a button that sorts by its column (again: the other way; a third time:
    // the page's order).
    for (n, c) in cols.iter().enumerate() {
        let cell = Rect::from_min_max(
            pos2(lx + c.left, head.top()),
            pos2(lx + c.right, head.bottom() - 1.0),
        );
        let resp = ui.interact(
            winui::rect(cell),
            ui.scope_id().with(("grabber-head", n)),
            winui::pointer_only(),
        );
        let sorted = g.sort.filter(|(s, _)| *s == c.col);
        let state = winui::State::of(&resp, true);
        if sorted.is_some() {
            p.rect_filled(winui::rect(cell), 0.0, SORTED());
        }
        if let Some(f) = winui::subtle_fill(state, crate::theme::colors()) {
            p.rect_filled(winui::rect(cell), 0.0, f);
        }
        let title = crate::i18n::t(c.title);
        let base = head.top() + 16.0;
        let arrow_room = if sorted.is_some() { 16.0 } else { 0.0 };
        if c.end {
            winui::text_right(
                &p,
                cell.right() - 8.0 - arrow_room,
                base,
                title,
                winui::TEXT(),
            );
        } else {
            let left = if c.col == Col::Name {
                cell.left() + 28.0
            } else {
                cell.left() + 8.0
            };
            let shown = winui::ellipsize(
                &p,
                &winui::tahoma(),
                title,
                cell.right() - left - 8.0 - arrow_room,
            );
            text(&p, left, base, &shown, winui::TEXT());
        }
        if let Some((_, down)) = sorted {
            let cx = cell.right() - 10.0;
            let mid = head.top() + (HEADER_H / 2.0).floor() - 2.0;
            for i in 0..4 {
                let y = mid + if down { 3.0 - i as f32 } else { i as f32 };
                let half = i as f32;
                fill(&p, cx - half, y, cx + half, y, SORT_ARROW());
            }
        }
        if n + 1 < cols.len() {
            fill(
                &p,
                cell.right(),
                head.top() + 4.0,
                cell.right(),
                head.bottom() - 5.0,
                HEADER_SEP(),
            );
        }
        // i18n: tooltip of a Link grabber column title
        let tip = crate::i18n::t("Sort by this column (again: the other way, then as on the page)");
        winui::expose(&resp, egui::Role::ColumnHeader, title, true, None);
        if resp.on_hover_text(tip).clicked() {
            g.sort_by(c.col);
        }
    }
    let body = Rect::from_min_max(pos2(inner.left(), head.bottom()), inner.max);
    let rows: Vec<Row> = g.rows().to_vec();
    if g.links.is_empty() || rows.is_empty() {
        let note = if g.links.is_empty() {
            // i18n: Link grabber, while it is empty
            crate::i18n::t("Paste links here (Ctrl+V), or send a page's links from the browser.")
        } else {
            // i18n: Link grabber, when no link is found by the search or shown by the kind chosen
            crate::i18n::t("No links match the search or the kind of file chosen.")
        };
        let w = winui::text_width(&p, note);
        text(
            &p,
            body.center().x - w / 2.0,
            body.center().y + 4.0,
            note,
            winui::MUTED(),
        );
        return;
    }
    // The selection follows its row through changes (sorting, filtering, links arriving).
    if let Some(sel) = g.selected_row {
        g.rows.selected = rows.iter().position(|r| *r == sel);
    }
    // i18n: what a screen reader calls Link grabber's list
    g.rows.name = crate::i18n::t("Links");
    let row_h = g.row_h;
    let mut toggle = None;
    let mut flip = None;
    let mut copies = None;
    let mut st = g.rows;
    let grouped = g.grouped;
    // The painter reads the model as it is (the rows were made above), only for the rows it
    // draws.
    let model: &Grabber = g;
    let ev = winui::row_list(
        ui,
        "grabber-list",
        winui::rect(body.expand(1.0)),
        rows.len(),
        row_h,
        &mut st,
        |ui, clip, k, row, look| {
            let f = &facts_of(model, rows[k]);
            let top = row.top();
            let base = top + (row_h / 2.0).floor() + 4.0;
            let icon_y = top + ((row_h - 16.0) / 2.0).floor();
            let check_y = top + ((row_h - 13.0) / 2.0).floor();
            let muted = if look.selected {
                look.ink
            } else {
                winui::MUTED()
            };
            look.name = f.spoken.clone();
            match rows[k] {
                Row::Pkg(n) => {
                    let x = row.left() + 6.0;
                    let arrow = Rect::from_min_size(pos2(x, top), vec2(14.0, row_h));
                    if ui
                        .interact(
                            winui::rect(arrow),
                            egui::Id::unique(("grabber-arrow", n)),
                            winui::pointer_only(),
                        )
                        .clicked()
                    {
                        flip = Some(rows[k]);
                    }
                    winui::tree_expander(
                        clip,
                        pos2(x + 2.0, top + (row_h / 2.0).floor() - 2.0),
                        f.open,
                    );
                    let glyph = pos2(x + 16.0, check_y);
                    let mut on = f.on == Some(true);
                    if winui::row_check(ui, clip, glyph, &mut on, &f.name, ("grabber-pkg", n)) {
                        toggle = Some(rows[k]);
                    }
                    if f.on.is_none() {
                        winui::row_check_mixed(clip, glyph);
                    }
                    crate::fileicons::paint_at(
                        ui,
                        clip,
                        crate::fileicons::Kind::Folder,
                        pos2(glyph.x + 19.0, icon_y),
                        16.0,
                    );
                    let left = glyph.x + 19.0 + crate::fileicons::INDENT;
                    let name = crate::i18n::isolate(&f.name);
                    let room = row.right() - 240.0;
                    // Bold at the rows' own size.
                    let bold = egui::FontId::new(
                        crate::theme::metrics().font_dialog,
                        egui::FontFamily::Name("tahoma-bold".into()),
                    );
                    let label = winui::ellipsize(clip, &bold, &name, room - left);
                    let shown_w = winui::text_width_font(clip, bold.clone(), &label);
                    winui::text_font(clip, bold, left, base, &label, look.ink);
                    winui::fit(
                        ui,
                        clip,
                        left + shown_w + 10.0,
                        base,
                        &f.text,
                        muted,
                        row.right() - 8.0,
                        ("grabber-pkg-what", n),
                    );
                }
                Row::Link(i) => {
                    let indent = if grouped { 22.0 } else { 0.0 } + if f.copy { 20.0 } else { 0.0 };
                    let glyph = pos2(row.left() + 8.0 + indent, check_y);
                    let mut on = f.on == Some(true);
                    if winui::row_check(ui, clip, glyph, &mut on, &f.name, ("grabber-link", i)) {
                        toggle = Some(rows[k]);
                    }
                    crate::fileicons::paint_at(
                        ui,
                        clip,
                        crate::fileicons::Kind::File(&f.name),
                        pos2(glyph.x + 19.0, icon_y),
                        16.0,
                    );
                    let left = glyph.x + 19.0 + crate::fileicons::INDENT;
                    let name_right = row.left() + cols[1].left - 10.0;
                    // The marks after the name: measured first, so the name leaves them room.
                    let marks: Vec<(String, bool)> = f.marks.clone();
                    let mark_w: f32 = marks
                        .iter()
                        .map(|(m, _)| winui::text_width(clip, m) + 14.0)
                        .sum();
                    let name = crate::i18n::isolate(&f.name);
                    let name_room = (name_right - left - mark_w).max(40.0);
                    let ink = if f.copy || f.dim { muted } else { look.ink };
                    let drawn = winui::fit(
                        ui,
                        clip,
                        left,
                        base,
                        &name,
                        ink,
                        left + name_room,
                        ("grabber-name", i),
                    );
                    let mut mx = left + drawn.width().min(name_room) + 6.0;
                    for (n, (m, button)) in marks.iter().enumerate() {
                        let w = winui::text_width(clip, m) + 10.0;
                        let pill = Rect::from_min_size(
                            pos2(mx, top + ((row_h - 16.0) / 2.0).floor()),
                            vec2(w, 16.0),
                        );
                        let pill_s = winui::rect(pill);
                        let hot = *button
                            && ui
                                .interact(
                                    pill_s,
                                    egui::Id::unique(("grabber-copies", i)),
                                    winui::pointer_only(),
                                )
                                .on_hover_text(crate::i18n::t(
                                    // i18n: tooltip of the mark on a Link grabber row that has other links to the same file
                                    "Other links to the same file: show or hide them",
                                ))
                                .clicked();
                        if hot {
                            copies = Some(i);
                        }
                        let (fill_c, mink) = if *button {
                            (
                                ACCENT().gamma_multiply(0.18),
                                if look.selected { look.ink } else { ACCENT() },
                            )
                        } else {
                            (LIST_HEAD(), muted)
                        };
                        clip.rect_filled(pill_s, 8.0, fill_c);
                        text(clip, pill.left() + 5.0, base - 1.0, m, mink);
                        mx += w + 4.0;
                        let _ = n;
                    }
                    let cell = |c: usize| {
                        (
                            row.left() + cols[c].left + 8.0,
                            row.left() + cols[c].right - 8.0,
                        )
                    };
                    let (a, b) = cell(1);
                    winui::fit(ui, clip, a, base, &f.text, muted, b, ("grabber-text", i));
                    let (a, b) = cell(2);
                    winui::fit(ui, clip, a, base, &f.site, muted, b, ("grabber-site", i));
                    let (_, b) = cell(3);
                    winui::text_right(clip, b, base, &f.size, look.ink);
                    let (a, b) = cell(4);
                    winui::fit(ui, clip, a, base, &f.kind, look.ink, b, ("grabber-type", i));
                    let (a, b) = cell(5);
                    if !f.status.is_empty() {
                        let dot = if look.selected {
                            look.ink
                        } else {
                            f.status_ink
                        };
                        let center = winui::rect(Rect::from_center_size(
                            pos2(a + 3.5, top + row_h / 2.0),
                            vec2(7.0, 7.0),
                        ))
                        .center();
                        if f.dot {
                            clip.circle_filled(center, 3.5, dot);
                        }
                        let tx = if f.dot { a + 12.0 } else { a };
                        let ink = if look.selected {
                            look.ink
                        } else {
                            f.status_ink
                        };
                        winui::fit(ui, clip, tx, base, &f.status, ink, b, ("grabber-state", i));
                    }
                }
            }
        },
    );
    g.rows = st;
    if let Some(k) = ev.space.or(ev.double_clicked) {
        match rows[k] {
            Row::Pkg(_) if ev.double_clicked == Some(k) => flip = Some(rows[k]),
            row => toggle = Some(row),
        }
    }
    if let Some(k) = ev.delete {
        g.remove(rows[k]);
    }
    g.selected_row = g.rows.selected.and_then(|k| rows.get(k).copied());
    if let Some(row) = g.selected_row {
        g.current = match row {
            Row::Pkg(n) => n,
            Row::Link(i) => g.links[i].pkg_index(),
        };
    }
    if ev.focused {
        if let Some(row) = g.selected_row {
            let right = winui::take_key(ui, egui::Key::ArrowRight);
            let left = winui::take_key(ui, egui::Key::ArrowLeft);
            match row {
                Row::Pkg(n) => {
                    let open = g.package_open(n);
                    if (right && !open) || (left && open) {
                        flip = Some(row);
                    }
                }
                Row::Link(i) if g.copies(i) > 0 => {
                    let open = g.copies_open(i);
                    if (right && !open) || (left && open) {
                        copies = Some(i);
                    }
                }
                Row::Link(_) => {}
            }
            let copy = ui.input_mut(|inp| inp.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
            if copy && let Row::Link(i) = row {
                ui.ctx().copy_text(g.links[i].address().to_string());
            }
        }
        // Each row's name read only when something was typed (thousands of rows).
        let (packages, links) = (&g.packages, &g.links);
        let name = |k: usize| match rows[k] {
            Row::Pkg(n) => packages[n].name.clone(),
            Row::Link(i) => links[i].name.clone(),
        };
        if g.rows.type_to(ui, "grabber-list", rows.len(), name) {
            g.selected_row = g.rows.selected.and_then(|k| rows.get(k).copied());
        }
    }
    match flip {
        Some(Row::Pkg(n)) => g.flip_package(n),
        Some(Row::Link(_)) | None => {}
    }
    if let Some(i) = copies {
        g.flip_copies(i);
    }
    match toggle {
        Some(Row::Pkg(n)) => g.tick_package(n),
        Some(Row::Link(i)) => g.toggle_link(i),
        None => {}
    }
}

/// What a row shows, read from the model before it is drawn.
#[derive(Default, Clone)]
struct RowFacts {
    name: String,
    /// Ticked (Some(true)), not (Some(false)), some of a package's links (None).
    on: Option<bool>,
    open: bool,
    /// A package's "3 files, 1.2 GB"; a link's text on the page.
    text: String,
    site: String,
    size: String,
    kind: String,
    status: String,
    status_ink: egui::Color32,
    dot: bool,
    /// The marks after a link's name, each with whether it is a button (show its copies).
    marks: Vec<(String, bool)>,
    /// A second link to a file (shown under the first, its name dimmed).
    copy: bool,
    /// Its name dimmed: in the list or on the disk already.
    dim: bool,
    /// What a screen reader says for the row.
    spoken: String,
}

fn facts_of(g: &Grabber, row: Row) -> RowFacts {
    match row {
        Row::Pkg(n) => {
            let (all, on, size) = g.package_counts_now(n);
            // i18n: Link grabber: a package's row: {n} is its number of links
            let files = crate::i18n::tn(all as u64, "{n} file", "{n} files");
            let text = match size {
                Some(b) => crate::regional::list(&[&files, &crate::dialogs::dialog_size(b)]),
                None => files.clone(),
            };
            let name = g.packages[n].name.clone();
            RowFacts {
                spoken: crate::regional::list(&[&name, &text]),
                name,
                on: if on == all && all > 0 {
                    Some(true)
                } else if on == 0 {
                    Some(false)
                } else {
                    None
                },
                open: g.package_open(n),
                text,
                ..Default::default()
            }
        }
        Row::Link(i) => {
            let copies = g.copies_now(i);
            let open = g.copies_open(i);
            let l = &g.links[i];
            let mut marks = Vec::new();
            if l.known() {
                // i18n: Link grabber: a mark after a link's name: the download list has it already
                marks.push((crate::i18n::t("In the list").to_owned(), false));
            } else if l.on_disk() {
                // i18n: Link grabber: a mark after a link's name: its file is in the folder already
                marks.push((crate::i18n::t("On disk").to_owned(), false));
            }
            if l.dup_of().is_some() {
                // i18n: Link grabber: a mark after a link's name: another link above leads to the same file
                marks.push((crate::i18n::t("Same file").to_owned(), false));
            }
            if copies > 0 {
                let m = crate::i18n::tn(
                    copies as u64,
                    // i18n: Link grabber: a button after a link's name: {n} other links lead to the same file (click: show them)
                    "+{n} link",
                    "+{n} links",
                );
                let _ = open;
                marks.push((m, true));
            }
            let (status, status_ink, dot) = match &l.state {
                LinkState::Unchecked => (String::new(), winui::MUTED(), false),
                LinkState::Checking => {
                    (crate::grabber::state_text(&l.state), winui::MUTED(), false)
                }
                LinkState::Online => (crate::grabber::state_text(&l.state), winui::SUCCESS(), true),
                LinkState::NeedsLogin | LinkState::Busy => {
                    (crate::grabber::state_text(&l.state), WARNING(), true)
                }
                LinkState::Offline(_) | LinkState::Failed(_) => {
                    (crate::grabber::state_text(&l.state), winui::ERROR(), true)
                }
            };
            let size = l.size.map(crate::dialogs::dialog_size).unwrap_or_default();
            let spoken = [l.name.as_str(), l.text(), &size, &status]
                .iter()
                .chain(
                    marks
                        .iter()
                        .map(|(m, _)| m.as_str())
                        .collect::<Vec<_>>()
                        .iter(),
                )
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            RowFacts {
                name: l.name.clone(),
                on: Some(l.on),
                open,
                text: l.text().to_owned(),
                site: l.site().to_owned(),
                size,
                kind: l.type_text(),
                status,
                status_ink,
                dot,
                marks,
                copy: l.dup_of().is_some(),
                dim: l.known() || l.on_disk(),
                spoken,
            }
        }
    }
}

/// Under the list, on the right: how far looking the links up has got.
fn looking_up(g: &mut Grabber, ui: &Ui, o: Pos2) {
    if !g.look_up {
        return;
    }
    let all = g.to_look();
    if all == 0 {
        return;
    }
    let p = ui.painter().clone();
    let done = g.looked.min(all);
    let line = if done < all {
        crate::i18n::fmt(
            // i18n: Link grabber, under its list: links being looked up (their name, size and whether they are there): {n} of {total} so far
            crate::i18n::t("Looking up sizes: {n} of {total}"),
            &[
                ("n", &crate::i18n::integer(done as u64)),
                ("total", &crate::i18n::integer(all as u64)),
            ],
        )
    } else {
        crate::i18n::fmt(
            // i18n: Link grabber, under its list: every link has been looked up; {total} is how many
            crate::i18n::t("Sizes looked up: {total}"),
            &[("total", &crate::i18n::integer(all as u64))],
        )
    };
    let bar_w = 90.0;
    let right = W - M;
    let base = o.y + UNDER_Y + 15.0;
    if done < all {
        let bar = winui::r(o, right - bar_w, UNDER_Y + 7.0, bar_w, 6.0);
        winui::meter(&p, bar, done as f32 / all as f32);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    let text_right = if done < all {
        right - bar_w - 8.0
    } else {
        right
    };
    winui::text_right(&p, o.x + text_right, base, &line, winui::MUTED());
}

/// The selected package's name, category, folder, password and what happens to its archives.
/// True while one of its boxes has the keys.
fn package_box(g: &mut Grabber, ui: &mut Ui, o: Pos2) -> bool {
    let p = ui.painter().clone();
    // A package with links shown (its count made with the rows).
    g.rows();
    let has = |g: &Grabber, k: usize| g.package_counts_now(k).0 > 0;
    if !has(g, g.current) {
        g.current = (0..g.packages.len()).find(|k| has(g, *k)).unwrap_or(0);
    }
    let enabled = has(g, g.current);
    let current = g.current;
    // i18n: Link grabber: the box that edits the selected package
    let caption = crate::i18n::t("Package");
    let cw = winui::text_width(&p, caption);
    winui::group_box(
        &p,
        r(o, M, BOX_Y, W - 2.0 * M, BOX_H),
        Some((o.x + M + 7.0, o.x + M + 11.0 + cw)),
    );
    text(&p, o.x + M + 9.0, o.y + BOX_Y + 4.0, caption, LIGHT_TEXT());
    let ink = if enabled {
        LIGHT_TEXT()
    } else {
        winui::LIGHT_DISABLED()
    };
    // Labels before their boxes: a column as wide as this language's longest (up to 160 px).
    // i18n: Link grabber: the package's name (a label before its box)
    let name_label = crate::i18n::t("Name:");
    // i18n: Link grabber: the folder the package's files go to (a label before its box)
    let folder_label = crate::i18n::t("Save to:");
    // i18n: Link grabber: the package's archives' password (a label before its box)
    let pw_label = crate::i18n::t("Password:");
    // i18n: Link grabber: the category the package's downloads go in (a label before its drop-down)
    let cat_label = crate::i18n::t("Category:");
    let wide = |s: &str| winui::text_width(&p, s);
    let x0 = M + 10.0;
    let bx = (x0 + wide(name_label).max(wide(folder_label)) + 8.0).clamp(x0 + 60.0, x0 + 150.0);
    let label = |ui: &Ui, x: f32, y: f32, s: &str, right: f32| {
        winui::fit(
            ui,
            &p,
            o.x + x,
            o.y + y,
            s,
            ink,
            o.x + right,
            ("grabber-label", y as i32, x as i32),
        );
    };
    let row1 = BOX_Y + 18.0;
    let row2 = BOX_Y + 49.0;
    let row3 = BOX_Y + 82.0;
    let right = W - M - 10.0;
    let mut blank = (String::new(), String::new(), String::new(), false, false);
    let (named, pw, typed, chose, was, extract_now) = {
        let pk = g.packages.get_mut(current).filter(|_| enabled);
        let (name, folder, password, extract, delete) = match pk {
            Some(pk) => (
                &mut pk.name,
                &mut pk.folder,
                &mut pk.password,
                &mut pk.extract,
                &mut pk.delete,
            ),
            None => (
                &mut blank.0,
                &mut blank.1,
                &mut blank.2,
                &mut blank.3,
                &mut blank.4,
            ),
        };
        // Row 1: the name, the category, the password.
        label(ui, x0, row1 + 15.0, name_label, bx - 4.0);
        let cat_x = 430.0;
        let named = winui::edit_light(
            ui,
            r(o, bx, row1, cat_x - 14.0 - bx, 23.0),
            name,
            enabled,
            false,
        );
        let pw_x = 700.0;
        let pbx = (pw_x + wide(pw_label) + 8.0).min(790.0);
        label(ui, pw_x, row1 + 15.0, pw_label, pbx - 4.0);
        let pw = winui::password_light(ui, r(o, pbx, row1, right - pbx, 23.0), password, enabled);
        // i18n: what a screen reader calls Link grabber's password box
        let pw_name = crate::i18n::t("Archive password");
        ui.ctx().accesskit_node_builder(pw.id, |n| {
            n.set_label(pw_name);
            // UI Automation's id: tools tell the one password box apart by it (its name is
            // not read out to them).
            n.set_author_id("grabber-password");
        });
        // Row 2: the folder, and Browse.
        label(ui, x0, row2 + 15.0, folder_label, bx - 4.0);
        // i18n: verb: choose a folder
        let bw = winui::button_width(&p, "Browse", 73.0);
        let typed = winui::edit_light(
            ui,
            r(o, bx, row2, right - bw - 8.0 - bx, 23.0),
            folder,
            enabled,
            false,
        );
        if enabled && folder.is_empty() && !typed.has_focus() {
            // i18n: Link grabber, in the empty "Save to" box: each file goes to its category's folder (Options → Save to)
            let each = crate::i18n::t("Each file's category folder");
            winui::fit(
                ui,
                &p,
                o.x + bx + 5.0,
                o.y + row2 + 15.0,
                each,
                winui::MUTED(),
                o.x + right - bw - 12.0,
                "grabber-each",
            );
        }
        let browse = winui::button_light(
            ui,
            r(o, right - bw, row2 + 1.0, bw, 21.0),
            "Browse",
            if enabled {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        )
        .clicked();
        if browse {
            let from = folder.clone();
            winui::start_pick(ui.ctx(), "grabber-folder", move || {
                rfd::FileDialog::new().set_directory(from).pick_folder()
            });
        }
        let mut chose = false;
        if let Some(dir) = winui::picked(ui.ctx(), "grabber-folder") {
            *folder = dir.display().to_string();
            chose = true;
        }
        // Row 3: what becomes of its archives.
        let was = *extract;
        // i18n: Link grabber check box: extract the package's archives once they are downloaded
        let extract_label = crate::i18n::mark("Extract when done");
        winui::checkbox_light(ui, o + vec2(x0, row3), extract, extract_label, enabled);
        let extract_now = *extract;
        let next = x0 + 18.0 + wide(crate::i18n::t(extract_label)) + 24.0;
        // i18n: Link grabber check box: after extracting, the archives go to the Recycle Bin
        winui::checkbox_light(
            ui,
            o + vec2(next, row3),
            delete,
            "Delete archives after extracting",
            enabled && extract_now,
        );
        (named, pw, typed, chose, was, extract_now)
    };
    // The category: Automatic (each file's own), or one for all its downloads.
    let cat_x = 430.0;
    let cbx = (cat_x + wide(cat_label) + 8.0).min(540.0);
    label(ui, cat_x, row1 + 15.0, cat_label, cbx - 4.0);
    let cats = crate::model::Category::all();
    let names: Vec<String> = cats.iter().map(|c| c.shown()).collect();
    // i18n: Link grabber: the package's category: each file's own, by its type (the first choice of the drop-down)
    let auto = crate::i18n::t("Automatic (by file type)");
    let items: Vec<&str> = std::iter::once(auto)
        .chain(names.iter().map(String::as_str))
        .collect();
    let now = g
        .packages
        .get(current)
        .and_then(|p| p.category)
        .and_then(|c| cats.iter().position(|x| *x == c))
        .map_or(0, |i| i + 1);
    let pw_x = 700.0;
    if let Some(i) = winui::combo_light(
        ui,
        r(o, cbx, row1, pw_x - 14.0 - cbx, 23.0),
        &items,
        now,
        enabled,
        "grabber-category",
    ) && enabled
    {
        g.set_category(current, i.checked_sub(1).map(|i| cats[i]));
    }
    if enabled {
        let folder_changed = typed.changed() || chose;
        g.package_edited(current, folder_changed, was != extract_now);
    }
    named.has_focus() || typed.has_focus() || pw.has_focus()
}
