//! The toolbar: its buttons drawn in the chosen look, their labels fitted, and the drop target's
//! tip.

use super::*;

// ---- toolbar -------------------------------------------------------------------------------

pub enum ToolbarEvent {
    Click(usize),
    /// A drop-down button's arrow: its menu opens at this point (button left, under the lit
    /// button: y 85 with large buttons).
    Drop(usize, Pos2),
}

/// How the toolbar is drawn (View → Toolbar): large buttons, 32-point icons with their labels
/// under them (IDM's look, the default), or small ones, 16-point icons with their labels beside
/// them in a lower band; either with icons only, each named by its tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarLook {
    pub small: bool,
    pub labels: bool,
}

impl Default for ToolbarLook {
    fn default() -> Self {
        ToolbarLook {
            small: false,
            labels: true,
        }
    }
}

impl ToolbarLook {
    /// The band's height under the etch (y 20, 21): 68 with large buttons and their labels.
    pub fn band_h(self) -> f32 {
        match (self.small, self.labels) {
            (false, true) => 68.0,
            (false, false) => 46.0,
            (true, _) => 30.0,
        }
    }

    /// A lit button's height from the band's top: 3 px short of its bottom (65 in 68).
    pub(super) fn hot_h(self) -> f32 {
        self.band_h() - 3.0
    }

    /// The icon's side, in points.
    pub(super) fn icon(self) -> f32 {
        if self.small { 16.0 } else { 32.0 }
    }

    /// A button's least width: IDM's 66 px with a label under the icon, the icon and its margins
    /// without one; a small button with its label is as wide as the label makes it.
    pub(super) fn base_w(self) -> f32 {
        match (self.small, self.labels) {
            (false, true) => 66.0,
            (false, false) => 44.0,
            (true, true) => 0.0,
            (true, false) => 28.0,
        }
    }

    /// What a button takes besides its label: the margins (large), or the icon, its margin and
    /// the gap before the label too (small: 5 + 16 + 4, and 6 after the label).
    pub(super) fn pad(self) -> f32 {
        if self.small { 31.0 } else { 12.0 }
    }

    /// A drop-down's arrow part, after its button: wide enough to be seen and hit as a button
    /// of its own (a split button's second half).
    pub(super) fn drop_w(self) -> f32 {
        if self.small { 16.0 } else { 19.0 }
    }

    /// Its chevron's width and height (points).
    pub(super) fn chevron(self) -> (f32, f32) {
        if self.small { (7.0, 4.0) } else { (9.0, 5.0) }
    }
}

/// What a drop-down button's arrow half opens, said in its tooltip and to screen readers (the
/// same menus are in Tasks and Downloads, for the keyboard).
pub(super) fn drop_tip(icon: &Icon) -> &'static str {
    match icon {
        // i18n: tooltip of the arrow beside the toolbar's Add Torrent button: it opens a menu of the torrent commands
        Icon::AddTorrent => crate::i18n::mark(
            "More: open a torrent file, add a magnet link, RSS feeds, torrent settings",
        ),
        // i18n: tooltip of the arrow beside the toolbar's Start Queue button: a menu of the queues to start
        Icon::StartQueue => crate::i18n::mark("Choose the queue to start"),
        // i18n: tooltip of the arrow beside the toolbar's Stop Queue button: a menu of the queues to stop
        _ => crate::i18n::mark("Choose the queue to stop"),
    }
}

/// Client y where the panes (Categories, list) start, under the menu bar: below the etch and the
/// toolbar band (70 with large buttons and their labels), in both modes.
pub fn pane_top(look: ToolbarLook) -> f32 {
    2.0 + look.band_h()
}

/// Paint the etch (y 20, 21) and the toolbar band under it (large buttons: y 22–89), `buttons`
/// in the order given, as `look` says.
pub fn toolbar(
    ui: &mut Ui,
    o: Pos2,
    width: f32,
    look: ToolbarLook,
    buttons: &[(Icon, &str, bool)],
) -> Option<ToolbarEvent> {
    let p = ui.painter().clone();
    let top = o.y + MENU_H;
    let c = crate::theme::colors();
    fill(&p, o.x, top, o.x + width - 1.0, top, c.toolbar_line);
    fill(
        &p,
        o.x,
        top + 1.0,
        o.x + width - 1.0,
        top + 1.0,
        c.toolbar_line_light,
    );
    // Both modes (light-main.md; dark changes only colours): Tahoma labels; large buttons are
    // 66 px in a 68 px band, small ones as wide as their labels in a 30 px band.
    let font = winui::tahoma();
    let (band_h, base_w, pad, drop_w) = (look.band_h(), look.base_w(), look.pad(), look.drop_w());
    let band = Rect::from_min_size(pos2(o.x, top + 2.0), vec2(width, band_h));
    p.rect_filled(band, 0.0, WINDOW_BG());
    let mut x = o.x;
    let mut event = None;
    let is_dropdown =
        |icon: &Icon| matches!(icon, Icon::StartQueue | Icon::StopQueue | Icon::AddTorrent);
    // A toolbar wider than the window (long translations): the longest labels give way first,
    // cut with "..." and whole on hover, so every button stays on screen. Without labels every
    // button has its least width.
    let label_ext = |label: &str| {
        if look.labels {
            text_width_font(&p, font.clone(), label).round()
        } else {
            0.0
        }
    };
    let natural: Vec<(f32, f32)> = buttons
        .iter()
        .map(|(icon, label, _)| {
            let ext = label_ext(crate::i18n::tc("toolbar", label));
            (ext, if is_dropdown(icon) { drop_w } else { 0.0 })
        })
        .collect();
    let cap = if look.labels {
        label_cap(&natural, base_w, pad, width)
    } else {
        f32::INFINITY
    };
    for (i, (icon, label, enabled)) in buttons.iter().enumerate() {
        let full = crate::i18n::tc("toolbar", label);
        let dropdown = is_dropdown(icon);
        // IDM's buttons are 66 px; ours grow only when a full label needs it (user
        // decision), up to the cap that keeps the toolbar inside the window.
        let (bw, cut) = if look.labels {
            let full_ext = label_ext(full);
            let bw = base_w.max((full_ext + pad).min(cap));
            (bw, full_ext + pad > bw)
        } else {
            (base_w, false)
        };
        let shown = if cut {
            winui::ellipsize(&p, &font, full, bw - pad)
        } else {
            full.to_owned()
        };
        let label = shown.as_str();
        let ext = label_ext(label);
        let total = bw + if dropdown { drop_w } else { 0.0 };
        // Laid out in English from the leading edge; each part made where it is on screen (the
        // first button at the right in RTL, a drop-down's arrow after its button). A split button
        // (a drop-down) is two buttons of their own side by side, the command and the arrow that
        // opens its menu, each named for screen readers.
        let main_rc = winui::rect(Rect::from_min_size(pos2(x, band.top()), vec2(bw, band_h)));
        let resp = ui.interact(main_rc, egui::Id::unique(("tb", i)), winui::pointer_only());
        // Named for screen readers; the name shows on hover where the label is cut, or not shown
        // (icons only).
        winui::expose(&resp, egui::Role::Button, full, *enabled, None);
        let resp = if cut || !look.labels {
            resp.on_hover_text(full)
        } else {
            resp
        };
        let arrow_resp = dropdown.then(|| {
            let arrow_rc = winui::rect(Rect::from_min_size(
                pos2(x + bw, band.top()),
                vec2(total - bw, band_h),
            ));
            let tip = crate::i18n::t(drop_tip(icon));
            let r = ui.interact(
                arrow_rc,
                egui::Id::unique(("tb-drop", i)),
                winui::pointer_only(),
            );
            // Its menu lists every queue (or the torrent commands): never greyed.
            winui::expose(&r, egui::Role::Button, tip, true, None);
            r.on_hover_text(tip)
        });
        let on_arrow = arrow_resp.as_ref().is_some_and(egui::Response::hovered);
        let hovered = (*enabled && resp.hovered()) || on_arrow;
        let h = look.hot_h();
        if hovered {
            // Hover (IDM 6.42, captured 2026-10-30): the button's own width, from the band's top,
            // square. Dark: a flat #33414d. Light: Windows' hot button, #e5f3ff with a 1 px
            // #cce8ff border. As much room under the label as above the icon (user request
            // 2026-11-01: IDM's 61 px left the label on the bottom edge): 65 tall, inside the
            // band (3 px short of its bottom with small buttons and without labels too). Beyond
            // IDM (which lights only the button part, user
            // request 2026-10-29): a drop-down is a split button — the part under the pointer is
            // lit (pressed while the mouse is down), the other part faintly, a 1 px divider between.
            let (hot_c, faint_c, down_c, edge_c, border_c) = (
                c.toolbar_hot,
                c.toolbar_hot_faint,
                c.toolbar_pressed,
                c.toolbar_hot_edge,
                c.toolbar_hot_border,
            );
            let down = if on_arrow {
                arrow_resp
                    .as_ref()
                    .is_some_and(egui::Response::is_pointer_button_down_on)
            } else {
                resp.is_pointer_button_down_on()
            };
            let main = winui::rect(Rect::from_min_size(pos2(x, band.top()), vec2(bw, h)));
            // The arrow part starts on the button's last column: their borders share one line.
            let arrow = winui::rect(Rect::from_min_size(
                pos2(x + bw - 1.0, band.top()),
                vec2(total - bw + 1.0, h),
            ));
            let lit = |r: Rect, under: bool| {
                let c = match (under, down) {
                    (true, true) => down_c,
                    (true, false) => hot_c,
                    (false, _) => faint_c,
                };
                p.rect_filled(r, 0.0, c);
                // None in the dark look (a transparent border isn't drawn).
                p.rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, border_c),
                    egui::StrokeKind::Inside,
                );
            };
            // A greyed command stays unlit; its arrow still opens the menu (every queue).
            if *enabled {
                lit(main, !on_arrow);
            }
            if dropdown {
                lit(arrow, on_arrow);
                // The divider: the parts' shared edge, one line.
                winui::fill_l(
                    &p,
                    x + bw - 1.0,
                    band.top(),
                    x + bw - 1.0,
                    band.top() + h - 1.0,
                    edge_c,
                );
            }
        }
        // IDM's look (large buttons, labels under them) is placed as it always was; the others
        // centre their icon in the lit area on whole pixels.
        let classic = !look.small && look.labels;
        let side = look.icon();
        let icon_rect = if classic {
            Rect::from_center_size(pos2(x + bw / 2.0, top + 25.5), vec2(side, side))
        } else {
            let left = if look.small && look.labels {
                x + 5.0
            } else {
                x + ((bw - side) / 2.0).floor()
            };
            Rect::from_min_size(
                pos2(left, band.top() + ((h - side) / 2.0).floor()),
                vec2(side, side),
            )
        };
        // Placed, not mirrored: an icon keeps its look. Each is the picture made for its size on
        // screen where there is one (a theme's 16-pixel drawing for a small button, the 3D
        // style's 48-pixel one for a large button at 150 %).
        icon.image_px(*enabled, side * ui.ctx().pixels_per_point())
            .paint_at(ui, winui::rect(icon_rect));
        let ink = if *enabled { TEXT() } else { TB_DISABLED() };
        if classic {
            // Label: x = btn.x + floor((w − ext)/2), +1 for drop-downs, last ink row 58. Every
            // label on one line (IDM draws the drop-downs' 1 px lower: in one row it read as a
            // mistake).
            let mut lx = x + ((bw - ext) / 2.0).floor();
            if dropdown {
                lx += 1.0;
            }
            text_font(&p, font.clone(), lx, top + 58.0, label, ink);
        } else if look.labels {
            // Beside the icon, 4 px after it, its capitals centred on the icon's middle.
            let cap = (font.size * 0.72).round();
            let ly = icon_rect.top() + side / 2.0 + (cap / 2.0).ceil() - 1.0;
            text_font(&p, font.clone(), icon_rect.right() + 4.0, ly, label, ink);
        }
        if dropdown {
            // The chevron's middle: level with the icon's (large buttons with their labels:
            // where IDM's arrow is), else the lit area's.
            let cy = if classic {
                top + 31.5
            } else {
                band.top() + (h / 2.0).floor()
            };
            // The split's divider at rest (user choice 2026-10-29, "B"): a classic etched
            // toolbar separator — a shadow line and a highlight beside it — 24 px tall (shorter in
            // a small band), centred on the arrow; the hover draws the full divider instead.
            if !hovered {
                let (shadow, light) = (c.toolbar_split_shadow, c.toolbar_split_light);
                let half = ((h - 8.0) / 2.0).floor().min(12.0);
                let (y0, y1) = (cy - half, cy + half);
                winui::fill_l(&p, x + bw - 1.0, y0, x + bw - 1.0, y1, shadow);
                winui::fill_l(&p, x + bw, y0, x + bw, y1, light);
            }
            // A chevron rather than the 4-pixel triangle it was (the owner found it hidden):
            // drawn in the text's ink under the pointer, the arrow's at rest, at any scale.
            let (cw, ch) = look.chevron();
            let cx = x + bw + (drop_w / 2.0).floor() + 0.5;
            let ink = if on_arrow { TEXT() } else { TB_ARROW() };
            chevron_down(&p, winui::mx(cx), cy, cw, ch, ink);
        }
        // No right-click menu: the toolbar's looks are in View → Toolbar.
        // The arrow is a button of its own (a split button): its menu lists every queue, so it
        // works while the main one's button is greyed.
        if arrow_resp.as_ref().is_some_and(egui::Response::clicked) {
            // The drop-down menu's top-left is the button's left edge under the lit button
            // (client y 85 with large buttons); in RTL its top-right at the button's right edge.
            event = Some(ToolbarEvent::Drop(i, pos2(winui::mx(x), top + h)));
        } else if resp.clicked() && *enabled {
            event = Some(ToolbarEvent::Click(i));
        }
        x += total;
    }
    event
}

/// A downward chevron `w` × `h` points centred on screen x `cx`, its middle at `cy`: two strokes
/// meeting at the bottom, as Windows 11's split buttons draw theirs.
pub(super) fn chevron_down(p: &egui::Painter, cx: f32, cy: f32, w: f32, h: f32, ink: Color32) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    p.line(
        vec![
            pos2(cx - hw, cy - hh),
            pos2(cx, cy + hh),
            pos2(cx + hw, cy - hh),
        ],
        egui::Stroke::new(1.5, ink),
    );
}

/// The widest a toolbar button may grow for its label so that all of them fit in `width`:
/// unlimited when they fit whole; else the largest cap at which they do (never under `base_w`).
/// `buttons`: each label's width and its drop-down arrow's; `pad`, what a button takes besides
/// its label ([`ToolbarLook::pad`]).
pub(super) fn label_cap(buttons: &[(f32, f32)], base_w: f32, pad: f32, width: f32) -> f32 {
    let total = |cap: f32| -> f32 {
        buttons
            .iter()
            .map(|(ext, drop)| base_w.max((ext + pad).min(cap)) + drop)
            .sum()
    };
    if total(f32::INFINITY) <= width {
        return f32::INFINITY;
    }
    let (mut lo, mut hi) = (
        base_w,
        buttons.iter().map(|b| b.0 + pad).fold(base_w, f32::max),
    );
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if total(mid) <= width {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo.floor()
}

#[cfg(test)]
mod label_cap_tests {
    use super::{ToolbarLook, pane_top};

    #[test]
    fn labels_give_way_only_when_the_toolbar_is_too_wide() {
        for pad in [12.0, 31.0] {
            let b = [(40.0, 0.0), (200.0, 15.0), (300.0, 0.0)];
            // Room for all: no cap.
            assert!(super::label_cap(&b, 70.0, pad, 1000.0).is_infinite());
            // Too wide: the cap brings it inside, and the short label keeps its button.
            let cap = super::label_cap(&b, 70.0, pad, 450.0);
            let total: f32 = b
                .iter()
                .map(|(e, d)| 70f32.max((e + pad).min(cap)) + d)
                .sum();
            assert!(total <= 450.0 && cap >= 70.0, "{pad}: {cap} {total}");
        }
        // Small buttons with labels have no least width: each is as wide as its label makes it.
        let small = ToolbarLook {
            small: true,
            labels: true,
        };
        let b = [(30.0, 0.0), (60.0, 13.0)];
        assert!(super::label_cap(&b, small.base_w(), small.pad(), 200.0).is_infinite());
        let cap = super::label_cap(&b, small.base_w(), small.pad(), 120.0);
        assert!(cap < 60.0 + small.pad(), "{cap}");
    }

    /// The panes start under the band the look draws: IDM's 70 px with large buttons and their
    /// labels (the default, as before View → Toolbar), less without labels, least with small
    /// buttons; a lit button and its icon stay inside the band.
    #[test]
    fn the_panes_follow_the_toolbars_height() {
        let look = |small, labels| ToolbarLook { small, labels };
        assert_eq!(ToolbarLook::default(), look(false, true));
        assert_eq!(pane_top(ToolbarLook::default()), 70.0);
        assert!(pane_top(look(false, false)) < pane_top(look(false, true)));
        for labels in [true, false] {
            assert!(pane_top(look(true, labels)) < pane_top(look(false, false)));
            assert_eq!(look(true, labels).icon(), 16.0);
        }
        for small in [false, true] {
            for labels in [false, true] {
                let l = look(small, labels);
                assert!(
                    l.hot_h() < l.band_h() && l.icon() + 8.0 <= l.hot_h(),
                    "{l:?}"
                );
                // A button without a label is its icon and a margin each side.
                if !labels {
                    assert!(l.base_w() >= l.icon() + 8.0, "{l:?}");
                }
            }
        }
    }
}
