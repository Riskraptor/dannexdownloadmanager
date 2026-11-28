//! The Categories pane: its tree's rows drawn, counted and answered, their expanders.

use super::*;

// ---- Categories pane -----------------------------------------------------------------------

#[derive(Clone)]
pub struct TreeRow<'a> {
    pub level: usize,
    pub label: std::borrow::Cow<'a, str>,
    /// None: leaf; Some(open): has an expander.
    pub expander: Option<bool>,
    pub selected: bool,
    pub icon: IconKey,
    /// A small Silk icon overlaid at the icon's bottom-right (Finished ✓, queue stacks).
    pub badge: Option<&'static str>,
    /// How many downloads the node holds, shown after its label (none when empty).
    pub count: Option<usize>,
}

pub enum TreeEvent {
    Select(usize),
    Toggle(usize),
    Close,
    /// Right-click: the row whose menu opens (the selection does not move) and where.
    Context(usize, Pos2),
}

/// A tree node's count, after its label: "(7)".
pub(super) fn count_text(n: usize) -> String {
    format!("({})", crate::i18n::integer(n as u64))
}

/// The Categories pane: frame x 6 .. `right` (IDM: 148, resizable by the splitter), caption, close "x", tree (rows 16 px, indent 19).
/// Its frame's top is under the toolbar `look` draws ([`pane_top`]).
#[allow(clippy::too_many_arguments)]
pub fn categories(
    ui: &mut Ui,
    icons: &mut ShellIcons,
    o: Pos2,
    look: ToolbarLook,
    bottom: f32,
    right: f32,
    rows: &[TreeRow<'_>],
    // The tree has the keys (its chosen node shows the focus).
    keys: bool,
) -> Option<TreeEvent> {
    let p = ui.painter().clone();
    let y0 = o.y + pane_top(look) + MENU_H; // frame top
    // Laid out in English (frame x 6 .. right); every part made where it is on screen — the pane
    // on the right in RTL, its tree indenting from the right.
    // Its foot level with the download list's (`bottom` - 4), its tree's white with the list's.
    let frame = winui::rect(Rect::from_min_max(
        pos2(o.x + 6.0, y0),
        pos2(right, bottom - 4.0),
    ));
    p.rect_stroke(
        frame,
        0.0,
        Stroke::new(1.0, CAT_FRAME()),
        egui::StrokeKind::Inside,
    );
    winui::fill_l(
        &p,
        o.x + 6.0,
        y0 + 18.0,
        right - 1.0,
        y0 + 18.0,
        CAT_FRAME(),
    ); // caption underline
    // i18n: title of the side pane that lists the categories
    let caption = crate::i18n::t("Categories");
    // Short of the close box: in a narrow pane or a long translation, "..." and whole on hover.
    let shown = winui::ellipsize(&p, &winui::tahoma(), caption, right - 21.0 - (o.x + 12.0));
    let drawn = text_font(&p, winui::tahoma(), o.x + 12.0, y0 + 13.0, &shown, TEXT());
    if shown != caption {
        winui::full_on_hover(ui, drawn, "categories-caption", caption);
    }
    let mut event = None;
    let close = winui::rect(Rect::from_min_size(
        pos2(right - 19.0, y0 + 2.0),
        vec2(16.0, 15.0),
    ));
    let cr = ui
        .interact(close, egui::Id::unique("cat-close"), winui::pointer_only())
        // i18n: tooltip of the × that closes the Categories pane
        .on_hover_text(crate::i18n::t("Close"));
    if let Some(c) = winui::subtle_fill(winui::State::of(&cr, true), crate::theme::colors()) {
        p.rect_filled(close, crate::theme::metrics().part_radius, c);
    }
    // The close mark: a × drawn as two strokes, crisp at any scale (it was the letter x).
    let mark = Rect::from_center_size(close.center(), vec2(7.0, 7.0));
    let stroke = Stroke::new(1.3, TEXT());
    p.line_segment([mark.left_top(), mark.right_bottom()], stroke);
    p.line_segment([mark.right_top(), mark.left_bottom()], stroke);
    if cr.clicked() {
        event = Some(TreeEvent::Close);
    }
    let tree = Rect::from_min_max(pos2(o.x + 8.0, y0 + 18.0), pos2(right - 3.0, bottom - 6.0));
    p.rect_filled(winui::rect(tree), 0.0, TREE_BG());

    // Scroll bars (notes/ui-spec/scrollbars.md "Categories tree"): by rows vertically, by
    // pixels horizontally; each shows when the content overflows (Win32 rule).
    let content_h = rows.len() as f32 * 16.0;
    let content_w = rows
        .iter()
        .map(|r| {
            let text_x = if r.level == 0 { 51.0 } else { 70.0 };
            let count = r.count.map_or(0.0, |n| {
                5.0 + text_width_font(&p, winui::tahoma(), &count_text(n))
            });
            text_x - 8.0
                + text_width_font(&p, winui::tahoma(), crate::i18n::t(&r.label))
                + count
                + 5.0
        })
        .fold(0.0, f32::max)
        .ceil();
    let (mut need_v, mut need_h) = (false, false);
    for _ in 0..2 {
        need_h = content_w > tree.width() - if need_v { scrollbar::THICK } else { 0.0 };
        need_v = content_h > tree.height() - if need_h { scrollbar::THICK } else { 0.0 };
    }
    let area = Rect::from_min_max(
        tree.min,
        pos2(
            tree.right() - if need_v { scrollbar::THICK } else { 0.0 },
            tree.bottom() - if need_h { scrollbar::THICK } else { 0.0 },
        ),
    );
    let scroll_id = egui::Id::unique("tree-scroll");
    // Where the tree is (px down, px across), its wheel's glide, and the row selected last frame.
    let (mut vpx, mut hpos, mut glide, last_selected) = ui
        .ctx()
        .data(|d| d.get_temp::<(f32, f32, winui::WheelGlide, Option<usize>)>(scroll_id))
        .unwrap_or_default();
    let page_rows = (area.height() / 16.0).floor().max(1.0);
    let max_v = (rows.len() as f32 - page_rows).max(0.0) * 16.0;
    // As the download list: the wheel glides the tree a pixel at a time to where its notches
    // take it (a touchpad's travel at once); the scroll bar still moves it by whole rows.
    let (wheel, direct) = if ui.rect_contains_pointer(winui::rect(tree)) {
        hpos -= ui.input(|i| i.smooth_scroll_delta.x);
        winui::wheel_travel(ui, 16.0, page_rows)
    } else {
        (0.0, 0.0)
    };
    vpx += direct * 16.0;
    glide.step(ui.ctx(), &mut vpx, wheel * 16.0, max_v);
    // A newly selected row (the keys, a click on one cut off at an edge) is scrolled wholly into
    // view, as a Win32 tree view does.
    let selected = rows.iter().position(|r| r.selected);
    if let Some(k) = selected
        && selected != last_selected
    {
        let top = k as f32 * 16.0;
        if top < vpx {
            vpx = top;
        } else if top + 16.0 > vpx + page_rows * 16.0 {
            vpx = top + 16.0 - page_rows * 16.0;
        }
        glide.stop(vpx.clamp(0.0, max_v));
    }
    vpx = vpx.clamp(0.0, max_v);
    hpos = hpos.clamp(0.0, (content_w - area.width()).max(0.0)).round();
    // Drawn to the screen pixel (a glide part-way between rows), so the text stays sharp.
    let ppp = ui.ctx().pixels_per_point();
    let shown_v = (vpx * ppp).round() / ppp;
    // Everything below is laid out from a horizontally scrolled origin.
    let o = pos2(o.x - hpos, o.y);
    let tp = p.with_clip_rect(winui::rect(area));
    let row_top = |k: usize| area.top() + 16.0 * k as f32 - shown_v;
    // Blank space (registered first, so the rows sit above it): the selected node's menu, at
    // the pointer (IDM places it beside the node; user decision: at the pointer).
    let blank = ui.interact(
        winui::rect(area),
        egui::Id::unique("tree-blank"),
        winui::pointer_only(),
    );
    if blank.secondary_clicked()
        && let Some(k) = rows.iter().position(|r| r.selected)
        && let Some(at) = blank.interact_pointer_pos()
    {
        event = Some(TreeEvent::Context(k, at));
    }
    // For screen readers and UI Automation (#113): the pane is a tree named as its caption, its
    // rows the tree's items (made in a child whose accessibility parent is the tree).
    winui::expose(
        &blank,
        egui::Role::Tree,
        crate::i18n::t("Categories"),
        true,
        None,
    );
    let rows_ui = ui.new_child(egui::UiBuilder::new().accessibility_parent(blank.id));
    // Where the selected row is, for its menu from the keyboard (Shift+F10, the Menu key).
    let mut selected_at: Option<(usize, Pos2)> = None;

    // Connector dots (#6d6d6d, every other pixel), painted first; expander boxes cover them.
    let roots: Vec<usize> = (0..rows.len()).filter(|&k| rows[k].level == 0).collect();
    if let (Some(&first), Some(&last)) = (roots.first(), roots.last()) {
        // Every other pixel counted from the tree's first row, so the dots move with the rows.
        let mut y = row_top(first);
        while y <= row_top(last) + 8.0 {
            if (y - row_top(0)).round() as i32 % 2 == 0 {
                winui::px_l(&tp, o.x + 19.0, y, TREE_LINES());
            }
            y += 1.0;
        }
        for &k in &roots {
            let mut x = o.x + 19.0;
            while x <= o.x + 29.0 {
                winui::px_l(&tp, x, row_top(k) + 8.0, TREE_LINES());
                x += 2.0;
            }
        }
    }
    for (k, r) in rows.iter().enumerate() {
        if r.level == 1 {
            let mut x = o.x + 38.0;
            while x <= o.x + 48.0 {
                winui::px_l(&tp, x, row_top(k) + 8.0, TREE_LINES());
                x += 2.0;
            }
            let is_last = rows.get(k + 1).is_none_or(|n| n.level == 0);
            let start = row_top(k);
            let end = if is_last {
                row_top(k) + 8.0
            } else {
                row_top(k) + 15.0
            };
            let mut y = start;
            while y <= end {
                if (y - tree.top()) as i32 % 2 == 0 {
                    winui::px_l(&tp, o.x + 38.0, y, TREE_LINES());
                }
                y += 1.0;
            }
        }
    }

    for (k, r) in rows.iter().enumerate() {
        let top = row_top(k);
        // Rows wholly outside the viewport; one cut off at an edge (while the wheel glides) is
        // drawn, clipped by `tp`.
        if top + 16.0 <= area.top() || top >= area.bottom() {
            continue;
        }
        let icon_x = o.x + if r.level == 0 { 30.0 } else { 49.0 };
        let text_x = o.x + if r.level == 0 { 51.0 } else { 70.0 };
        let ext = text_width_font(&p, winui::tahoma(), crate::i18n::t(&r.label));
        if r.selected {
            // The selection in its colour while the tree has the keys (Tab, F6, a click on it),
            // as the list's: which of the two the keys move is seen at a glance. Else the
            // inactive selection grey, in both modes (#f0f0f0 on the white tree could not be
            // seen).
            winui::fill_l(
                &tp,
                text_x - 2.0,
                top,
                text_x + ext + 1.0,
                top + 15.0,
                if keys {
                    winui::ROW_SELECTED()
                } else {
                    winui::SELECTED_INACTIVE()
                },
            );
        }
        if let Some(open) = r.expander {
            if crate::theme::colors().tree_chevrons {
                winui::fill_l(&tp, o.x + 11.0, top, o.x + 26.0, top + 15.0, TREE_BG());
                chevron(&tp, o.x, top, open, r.selected);
            } else {
                // Classic box; nothing opaque around it, so connectors run up to it.
                expander_box(&tp, o.x, top, open);
            }
        }
        // Icons placed, not mirrored.
        icons.paint_in(ui, &tp, r.icon.clone(), pos2(icon_x, top));
        if let Some(name) = r.badge {
            let rc = winui::rect(Rect::from_min_size(
                pos2(icon_x + 7.0, top + 7.0),
                vec2(9.0, 9.0),
            ));
            // The check badge from the theme's icon set, where it has one.
            let themed =
                name == "tick" && crate::themeicons::paint(ui, &tp, "misc-emblem-ok", rc, true);
            if !themed && let Some(src) = silk(name) {
                paint_image(ui, &tp, src, rc);
            }
        }
        text_font(
            &tp,
            winui::tahoma(),
            text_x,
            top + 11.0,
            crate::i18n::t(&r.label),
            TEXT(),
        );
        if keys && r.selected {
            winui::focus_ring(
                &tp,
                winui::rect(Rect::from_min_max(
                    pos2(text_x - 3.0, top),
                    pos2(text_x + ext + 3.0, top + 16.0),
                )),
            );
        }
        if let Some(n) = r.count {
            let count = count_text(n);
            text_font(
                &tp,
                winui::tahoma(),
                text_x + ext + 5.0,
                top + 11.0,
                &count,
                winui::MUTED(),
            );
        }

        let row = winui::rect(
            Rect::from_min_size(pos2(area.left(), top), vec2(area.width(), 16.0)).intersect(area),
        );
        let resp = rows_ui.interact(
            row,
            egui::Id::unique(("tree-row", k)),
            winui::pointer_only(),
        );
        // A tree item: its name (with its count), selected, open or shut, its level; Click
        // selects it (egui's own), Expand / Collapse open and shut it, Show context menu opens
        // its menu.
        {
            use egui::accesskit::Action;
            let name = match r.count {
                Some(n) => format!("{} {}", crate::i18n::t(&r.label), count_text(n)),
                None => crate::i18n::t(&r.label).to_owned(),
            };
            winui::expose(&resp, egui::Role::TreeItem, &name, true, None);
            let (expand, collapse, menu) = ui.input(|i| {
                (
                    i.has_accesskit_action_request(resp.id, Action::Expand),
                    i.has_accesskit_action_request(resp.id, Action::Collapse),
                    i.has_accesskit_action_request(resp.id, Action::ShowContextMenu),
                )
            });
            let (level, selected, open) = (r.level, r.selected, r.expander);
            ui.ctx().accesskit_node_builder(resp.id, |n| {
                n.set_selected(selected);
                n.set_level(level + 1);
                n.add_action(Action::ShowContextMenu);
                if let Some(open) = open {
                    n.set_expanded(open);
                    n.add_action(if open {
                        Action::Collapse
                    } else {
                        Action::Expand
                    });
                }
            });
            let under = resp.rect.left_bottom() + vec2(24.0, 0.0);
            if r.selected {
                selected_at = Some((k, under));
            }
            if (expand && r.expander == Some(false)) || (collapse && r.expander == Some(true)) {
                event = Some(TreeEvent::Toggle(k));
            }
            if menu {
                event = Some(TreeEvent::Context(k, under));
            }
            // An assistive action moves no pointer: the frame showing its result is asked for.
            if expand || collapse || menu {
                ui.ctx().request_repaint();
            }
        }
        if resp.clicked() {
            let on_expander = resp.interact_pointer_pos().is_some_and(|q| {
                let x = winui::mx(q.x);
                x >= o.x + 11.0 && x <= o.x + 26.0
            });
            event = Some(if on_expander && r.expander.is_some() {
                TreeEvent::Toggle(k)
            } else {
                TreeEvent::Select(k)
            });
        }
        if resp.double_clicked() && r.expander.is_some() {
            event = Some(TreeEvent::Toggle(k));
        }
        if resp.secondary_clicked()
            && let Some(at) = resp.interact_pointer_pos()
        {
            event = Some(TreeEvent::Context(k, at));
        }
    }
    // The selected node's menu from the keyboard, under the node: Shift+F10 or the Menu key.
    if keys
        && let Some((k, at)) = selected_at
        && ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Menu)
        })
    {
        event = Some(TreeEvent::Context(k, at));
    }

    if need_v {
        // At the trailing edge: the left in RTL.
        let track = winui::rect(Rect::from_min_max(
            pos2(area.right(), tree.top()),
            pos2(tree.right(), area.bottom()),
        ));
        let total = rows.len() as f32;
        let mut vpos = vpx / 16.0;
        if scrollbar::bar(
            ui,
            scroll_id.with("v"),
            track,
            true,
            total,
            page_rows,
            1.0,
            &mut vpos,
        ) {
            vpx = (vpos.round() * 16.0).clamp(0.0, max_v);
            glide.stop(vpx);
            ui.ctx().request_repaint();
        }
    }
    if need_h {
        let track = winui::rect(Rect::from_min_max(
            pos2(tree.left(), area.bottom()),
            pos2(area.right(), tree.bottom()),
        ));
        if scrollbar::bar(
            ui,
            scroll_id.with("h"),
            track,
            false,
            content_w,
            area.width(),
            15.0,
            &mut hpos,
        ) {
            hpos = hpos.round();
            ui.ctx().request_repaint();
        }
    }
    if need_v && need_h {
        scrollbar::corner(ui, area.right(), area.bottom());
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(scroll_id, (vpx, hpos, glide, selected)));
    event
}

/// Light mode's classic 9×9 +/- box at x 15..23, y top+4..top+12 (light-main.md grids).
pub(super) fn expander_box(p: &egui::Painter, ox: f32, top: f32, open: bool) {
    const MINUS: [[u32; 9]; 9] = [
        [
            0xbabbbc, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191,
            0xbabbbc,
        ],
        [
            0x919191, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb,
            0x919191,
        ],
        [
            0x919191, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb,
            0x919191,
        ],
        [
            0x919191, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0xfafbfb,
            0x919191,
        ],
        [
            0x919191, 0xededec, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0xededec,
            0x919191,
        ],
        [
            0x919191, 0xededec, 0xededec, 0xededec, 0xededec, 0xededec, 0xededec, 0xededec,
            0x919191,
        ],
        [
            0x919191, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3,
            0x919191,
        ],
        [
            0x919191, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3,
            0x919191,
        ],
        [
            0xbabbbc, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191,
            0xbabbbc,
        ],
    ];
    const PLUS: [[u32; 9]; 9] = [
        [
            0xbabbbc, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191,
            0xbabbbc,
        ],
        [
            0x919191, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc,
            0x919191,
        ],
        [
            0x919191, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc, 0x294272, 0xfcfcfc, 0xfcfcfc, 0xfcfcfc,
            0x919191,
        ],
        [
            0x919191, 0xfafbfb, 0xfafbfb, 0xfafbfb, 0x294272, 0xfafbfb, 0xfafbfb, 0xfafbfb,
            0x919191,
        ],
        [
            0x919191, 0xfafbfb, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0x4b63a7, 0xfafbfb,
            0x919191,
        ],
        [
            0x919191, 0xededec, 0xededec, 0xededec, 0x294272, 0xededec, 0xededec, 0xededec,
            0x919191,
        ],
        [
            0x919191, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0x294272, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3,
            0x919191,
        ],
        [
            0x919191, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3, 0xe3e3e3,
            0x919191,
        ],
        [
            0xbabbbc, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191, 0x919191,
            0xbabbbc,
        ],
    ];
    let grid = if open { &MINUS } else { &PLUS };
    for (dy, row) in grid.iter().enumerate() {
        for (dx, v) in row.iter().enumerate() {
            let c = rgb((v >> 16) as u8, (v >> 8) as u8, *v as u8);
            winui::px_l(p, ox + 15.0 + dx as f32, top + 4.0 + dy as f32, c);
        }
    }
}

/// Explorer dark-theme chevrons, pixel for pixel (spec "Expander chevrons").
pub(super) fn chevron(p: &egui::Painter, ox: f32, top: f32, open: bool, bright: bool) {
    const COLLAPSED: [[u8; 6]; 9] = [
        [0xc9, 0x95, 0x23, 0x20, 0x20, 0x20],
        [0x8d, 0xd9, 0x97, 0x23, 0x20, 0x20],
        [0x21, 0x8d, 0xd9, 0x97, 0x23, 0x20],
        [0x20, 0x21, 0x8d, 0xd9, 0x97, 0x22],
        [0x20, 0x20, 0x22, 0xbb, 0xd9, 0x4a],
        [0x20, 0x21, 0x8d, 0xd9, 0x8d, 0x21],
        [0x21, 0x8d, 0xd9, 0x8d, 0x21, 0x20],
        [0x8d, 0xd9, 0x8d, 0x21, 0x20, 0x20],
        [0xc9, 0x8d, 0x21, 0x20, 0x20, 0x20],
    ];
    const EXPANDED: [[u8; 9]; 6] = [
        [0xec, 0xad, 0x23, 0x20, 0x20, 0x20, 0x21, 0xa3, 0xec],
        [0xa3, 0xff, 0xb0, 0x23, 0x20, 0x21, 0xa4, 0xff, 0xa3],
        [0x21, 0xa4, 0xff, 0xa8, 0x22, 0xa4, 0xff, 0xa4, 0x21],
        [0x20, 0x21, 0xa4, 0xff, 0xdb, 0xff, 0xa4, 0x21, 0x20],
        [0x20, 0x20, 0x21, 0xa4, 0xff, 0xa4, 0x21, 0x20, 0x20],
        [0x20, 0x20, 0x20, 0x21, 0x53, 0x21, 0x20, 0x20, 0x20],
    ];
    let _ = bright;
    // Measured over the dark tree (#20) in white: coverage, so the theme's tree and ink show.
    let (bg, ink) = (TREE_BG(), TREE_EXPANDER());
    let grey = |v: u8| {
        let a = (v as f32 - 32.0) / 223.0;
        let mix = |b: u8, i: u8| (b as f32 + (i as f32 - b as f32) * a).round() as u8;
        rgb(
            mix(bg.r(), ink.r()),
            mix(bg.g(), ink.g()),
            mix(bg.b(), ink.b()),
        )
    };
    if open {
        for (dy, row) in EXPANDED.iter().enumerate() {
            for (dx, v) in row.iter().enumerate() {
                if *v != 0x20 {
                    winui::px_l(p, ox + 14.0 + dx as f32, top + 5.0 + dy as f32, grey(*v));
                }
            }
        }
    } else {
        for (dy, row) in COLLAPSED.iter().enumerate() {
            for (dx, v) in row.iter().enumerate() {
                if *v != 0x20 {
                    winui::px_l(p, ox + 16.0 + dx as f32, top + 4.0 + dy as f32, grey(*v));
                }
            }
        }
    }
}
