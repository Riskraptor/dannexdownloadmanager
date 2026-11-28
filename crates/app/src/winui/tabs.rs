//! Tab controls: the light dialogs' tab row, each tab's width shared from the row's.

use super::*;

/// One tab of a light tab control: its x range (inclusive, from the client origin) when
/// selected and when not (Windows draws the selected tab wider).
pub struct LightTab {
    pub label: &'static str,
    pub sel: (f32, f32),
    pub unsel: (f32, f32),
}

/// A translated row of tabs, sized to their labels as Windows sizes them: each `pad` px either
/// side of its (translated) label, from English x `start`, `gap` px apart. A row longer than
/// `right` caps the widest tabs to share the room and cuts their labels with "...".
/// (x0, x1, label shown) per tab, English x.
pub fn tab_row(
    p: &Painter,
    labels: &[&str],
    start: f32,
    right: f32,
    gap: f32,
    pad: f32,
) -> Vec<(f32, f32, String)> {
    let widths: Vec<f32> = labels
        .iter()
        .map(|s| text_width(p, s).ceil() + 2.0 * pad)
        .collect();
    let room = right - start - gap * labels.len().saturating_sub(1) as f32;
    let cap = width_cap(&widths, room);
    let mut x0 = start;
    labels
        .iter()
        .zip(widths)
        .map(|(s, w)| {
            let w = w.min(cap);
            let shown = ellipsize(p, &tahoma(), s, w - 2.0 * pad);
            let tab = (x0, x0 + w, shown);
            x0 += w + gap;
            tab
        })
        .collect()
}

/// The largest whole width `c` with Σ min(wᵢ, c) ≤ `room` (infinite when everything fits): the
/// narrow ones keep their width, the wide ones share what is left.
pub fn width_cap(widths: &[f32], room: f32) -> f32 {
    let mut sorted = widths.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut used = 0.0;
    for (k, w) in sorted.iter().enumerate() {
        let rest = (sorted.len() - k) as f32;
        if used + w * rest > room {
            return ((room - used) / rest).floor();
        }
        used += w;
    }
    f32::INFINITY
}

/// Light tab control in `rc` (client rect of the whole control): the pane and the tabs.
/// Returns the tab clicked this frame.
pub fn tabs_light(
    ui: &mut Ui,
    o: Pos2,
    rc: Rect,
    tabs: &[LightTab],
    selected: usize,
) -> Option<usize> {
    let p = ui.painter().clone();
    let border = HEADER_LINE();
    let shadow = TAB_UNSELECTED();
    // Pane: x rc.left ..= right − 3, y top + 20 ..= bottom − 2, then a 2 px / 1 px shadow.
    let (l, t, rr, b) = (
        rc.left(),
        rc.top() + 20.0,
        rc.right() - 3.0,
        rc.bottom() - 2.0,
    );
    fill(&p, l, t, rr, b, LIGHT_PAGE());
    hline(&p, l, rr, t, border);
    hline(&p, l, rr, b, border);
    vline(&p, l, t, b, border);
    vline(&p, rr, t, b, border);
    fill(&p, rr + 1.0, t, rr + 2.0, b + 1.0, shadow);
    hline(&p, l, rr + 2.0, b + 1.0, shadow);
    // (selected range, unselected range, label x, label shown), from the client origin.
    type Placed = ((f32, f32), (f32, f32), f32, String);
    let placed: Vec<Placed> = if tabs.iter().all(|t| crate::i18n::t(t.label) == t.label) {
        tabs.iter()
            .map(|t| (t.sel, t.unsel, t.sel.0 + 8.0, t.label.to_owned()))
            .collect()
    } else {
        // A translation: tabs sized to their labels from the first tab's place, within the pane
        // (the selected one 2 px wider each side).
        let labels: Vec<&str> = tabs.iter().map(|t| crate::i18n::t(t.label)).collect();
        let (start, left) = tabs.first().map_or((0.0, 0.0), |t| (t.unsel.0, t.sel.0));
        tab_row(&p, &labels, start, left + rc.width() - 3.0 - 2.0, 2.0, 8.0)
            .into_iter()
            .map(|(x0, x1, shown)| {
                let lx = x0 + ((x1 - x0 - text_width(&p, &shown)) / 2.0).floor();
                ((x0 - 2.0, x1 + 2.0), (x0, x1), lx, shown)
            })
            .collect()
    };
    let mut clicked = None;
    let mut draw = |i: usize, tab: &LightTab, sel: bool| {
        let (sel_x, unsel_x, label_x, shown) = &placed[i];
        let (x0, x1) = if sel { *sel_x } else { *unsel_x };
        // English pixel columns; the tabs run from the leading edge.
        let (a, b) = (mcol(o.x + x0), mcol(o.x + x1));
        let (x0, x1) = (a.min(b), a.max(b));
        let y0 = if sel { rc.top() } else { rc.top() + 2.0 };
        let hit = Rect::from_min_max(pos2(x0, y0), pos2(x1 + 1.0, t));
        // Keyed by the control's place and the tab's index: its x changes when it is chosen,
        // and the focus must stay on it.
        // One Tab stop, the chosen tab (Windows' tab control); the arrows choose.
        let sense = if sel { Sense::click() } else { pointer_only() };
        let id_of =
            |ui: &Ui, k: usize| widget_id(ui, ("tab-light", rc.min.x as i32, rc.min.y as i32, k));
        let resp = ui.interact(hit, id_of(ui, i), sense);
        focus_on_press(&resp);
        expose(
            &resp,
            egui::Role::Tab,
            crate::i18n::t(tab.label),
            true,
            Some(sel),
        );
        let resp = enter_is_not_a_click(ui, resp);
        if resp.clicked() && !sel {
            clicked = Some(i);
        }
        if sel {
            keep_keys(ui, &resp, true, false, false);
            if resp.has_focus() {
                let last = tabs.len().saturating_sub(1) as isize;
                let fwd = if rtl() { -1 } else { 1 };
                let to = if take_key(ui, egui::Key::ArrowRight) {
                    Some(i as isize + fwd)
                } else if take_key(ui, egui::Key::ArrowLeft) {
                    Some(i as isize - fwd)
                } else if take_key(ui, egui::Key::Home) {
                    Some(0)
                } else if take_key(ui, egui::Key::End) {
                    Some(last)
                } else {
                    None
                };
                if let Some(to) = to.map(|t| t.clamp(0, last) as usize).filter(|t| *t != i) {
                    clicked = Some(to);
                    let next = id_of(ui, to);
                    ui.memory_mut(|m| m.request_focus(next));
                }
            }
        }
        let face = if sel {
            LIGHT_PAGE()
        } else if resp.hovered() {
            TAB_HOT()
        } else {
            shadow
        };
        fill(&p, x0, y0, x1, t, face);
        hline(&p, x0 + 1.0, x1 - 1.0, y0, border);
        vline(&p, x0, y0 + 1.0, t, border);
        vline(&p, x1, y0 + 1.0, t, border);
        if sel {
            // Open at the bottom: the pane's top line stops at the tab.
            hline(&p, x0 + 1.0, x1 - 1.0, t, LIGHT_PAGE());
        } else {
            hline(&p, x0, x1, t, border);
        }
        let base = if sel {
            rc.top() + 13.0
        } else {
            rc.top() + 15.0
        };
        let full = crate::i18n::t(tab.label);
        let drawn = text(&p, o.x + label_x, base, shown, LIGHT_TEXT());
        // Over the tab's face (drawn before it, the face hid it).
        show_focus(ui, &resp, hit.shrink(2.0));
        if shown != full {
            full_on_hover(ui, drawn, ("tab-light-label", i), full);
        }
    };
    for (i, tab) in tabs.iter().enumerate().filter(|(i, _)| *i != selected) {
        draw(i, tab, false);
    }
    if let Some(tab) = tabs.get(selected) {
        draw(selected, tab, true);
    }
    clicked
}

#[cfg(test)]
mod tab_row_tests {
    #[test]
    fn the_widest_share_what_is_left() {
        assert_eq!(super::width_cap(&[50.0, 60.0], 200.0), f32::INFINITY);
        // 40 keeps its width; 100 and 120 share the remaining 120.
        assert_eq!(super::width_cap(&[100.0, 40.0, 120.0], 160.0), 60.0);
    }
}
