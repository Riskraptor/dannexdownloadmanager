//! The menu bar: its menus' titles, the bar drawn and clicked, and the keyboard moving along it.

use super::*;

// ---- menu bar --------------------------------------------------------------------------------

/// Top-level menus: label, item rect x and width, text x (measured, "Ta" kerned by the font):
/// the standard Win32 bar (text at item.x + 7, width = text + 14; light-main.md), in both modes
/// (user decision 2026-11-01: dark mode only changes colours, never sizes or places; IDM's dark
/// bar was wider).
pub(super) const MENUS: [(&str, f32, f32, f32); 5] = [
    ("&Tasks", 0.0, 42.0, 7.0),
    ("&File", 42.0, 32.0, 49.0),
    ("&Downloads", 74.0, 73.0, 81.0),
    ("&View", 147.0, 39.0, 154.0),
    ("&Help", 186.0, 39.0, 193.0),
];

/// What happened to the menu bar this frame, for [`menubar_next`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MenuBarEvent {
    /// Title `i` clicked (`released`: the click's release, or a screen reader's Collapse), or
    /// opened by a screen reader's Expand.
    Title { i: usize, released: bool },
    /// The pointer over title `i`, no click: while another menu is open, that one's menu opens
    /// instead (Windows' menu bars, and IDM's); otherwise nothing.
    Hover(usize),
    /// The open menu shut itself (Escape, a press outside it, the app losing the focus);
    /// `press_on_title`: the press that shut it went down on a title of the bar.
    Dismissed { press_on_title: bool },
    /// An item was chosen.
    Chosen,
    /// Left or Right, or the keyboard (Alt, F10, an access letter), opened menu `i`.
    Open(usize),
}

/// The menu bar in the interface language: English keeps the measured rects; a translation is
/// laid out by the same rule (Win32's: text at item.x + 7, width = text + 14) from the translated label's width: GDI's, or the
/// width it is drawn at when wider (a script drawn in a fallback font, such as Japanese).
pub fn menus(p: &egui::Painter) -> Vec<(&'static str, f32, f32, f32)> {
    let table = &MENUS;
    let translated: Vec<&'static str> = table
        .iter()
        .map(|(l, ..)| crate::i18n::tc("menubar", l))
        .collect();
    if translated
        .iter()
        .zip(table.iter())
        .all(|(t, (l, ..))| t == l)
    {
        return table.to_vec();
    }
    let (pad, inset) = (14.0, 7.0);
    let mut x = 0.0;
    translated
        .into_iter()
        .map(|label| {
            let w = menu_width(p, &plain(label).0) + pad;
            let item = (label, x, w, x + inset);
            x += w;
            item
        })
        .collect()
}

/// Paint the menu bar; returns what happened to its titles this frame, for [`menubar_next`]: a
/// title clicked (or opened and shut by a screen reader), or the pointer over one (which only
/// moves to it from another open menu).
pub fn menu_bar(ui: &mut Ui, o: Pos2, width: f32, open: Option<usize>) -> Option<MenuBarEvent> {
    let p = ui.painter().clone();
    fill(
        &p,
        o.x,
        o.y,
        o.x + width - 1.0,
        o.y + MENU_H - 2.0,
        MENUBAR_BG(),
    );
    // The bar's last row (light: #f2f2f2; dark: the bar's own).
    fill(
        &p,
        o.x,
        o.y + MENU_H - 2.0,
        o.x + width - 1.0,
        o.y + MENU_H - 2.0,
        MENUBAR_BOTTOM(),
    );
    fill(
        &p,
        o.x,
        o.y + MENU_H - 1.0,
        o.x + width - 1.0,
        o.y + MENU_H - 1.0,
        MENUBAR_LINE(),
    );
    let mut hit = None;
    // For screen readers and UI Automation (#113): a menu bar whose titles are menu items that
    // open their menus (Click, as with the mouse).
    let bar = ui.interact(
        winui::rect(Rect::from_min_size(o, vec2(width, MENU_H))),
        egui::Id::unique("menubar-node"),
        egui::Sense::hover(),
    );
    winui::expose(&bar, egui::Role::MenuBar, "", true, None);
    let titles_ui = ui.new_child(egui::UiBuilder::new().accessibility_parent(bar.id));
    let released = ui.input(|x| x.pointer.any_released());
    for (i, (label, x, w, tx)) in menus(&p).iter().enumerate() {
        // From the leading edge: the first menu at the right in RTL.
        let rc = winui::rect(Rect::from_min_size(o + vec2(*x, 0.0), vec2(*w, MENU_H)));
        let resp = titles_ui.interact(rc, egui::Id::unique(("menubar", i)), winui::pointer_only());
        winui::expose(&resp, egui::Role::MenuItem, &plain(label).0, true, None);
        ui.ctx().accesskit_node_builder(resp.id, |n| {
            n.set_has_popup(egui::accesskit::HasPopup::Menu);
            n.set_expanded(open == Some(i));
        });
        if open == Some(i) {
            p.rect_filled(rc, 0.0, MENUBAR_OPEN());
        } else if open.is_none() && resp.hovered() {
            let hot = Rect::from_min_max(rc.min + vec2(0.0, 1.0), rc.max - vec2(0.0, 1.0));
            p.rect_filled(hot, 0.0, MENUBAR_HOT());
        }
        // The title without its "&", its access letter underlined (each language marks its
        // own: French Tâches and Téléchargements both had T).
        let (shown, letter) = plain(label);
        let drawn = text_font(&p, winui::segoe(), o.x + tx, o.y + 13.0, &shown, TEXT());
        // Dark mode always underlines mnemonics (1 px under the letter's advance); light mode
        // follows the Windows rule: once the keyboard is in use (`cues`).
        if cues(ui.ctx()) {
            let (x0, x1) =
                winui::char_span(&p, &winui::segoe(), &shown, letter.unwrap_or(0), drawn);
            fill(&p, x0, o.y + 15.0, x1, o.y + 15.0, TEXT());
        }
        // A screen reader or UI Automation opens a title's menu with Expand (or Click) and
        // shuts it with Collapse.
        let (expand, collapse) = ui.input(|x| {
            (
                x.has_accesskit_action_request(resp.id, egui::accesskit::Action::Expand),
                x.has_accesskit_action_request(resp.id, egui::accesskit::Action::Collapse),
            )
        });
        if collapse && open == Some(i) {
            hit = Some(MenuBarEvent::Title { i, released: true });
        } else if resp.clicked() || (expand && open != Some(i)) {
            hit = Some(MenuBarEvent::Title { i, released });
        } else if resp.hovered() && open.is_some_and(|o| o != i) {
            hit = Some(MenuBarEvent::Hover(i));
        }
        if resp.clicked() || expand || collapse {
            // No pointer moves to bring the next frame: the menu is drawn (or shut) on it.
            ui.ctx().request_repaint();
        }
    }
    hit
}

/// The menu-bar title under `pos` (main-window coordinates), as [`menu_bar`] lays them out at
/// `o`: the first menu at the right in RTL.
pub fn menubar_title_at(p: &egui::Painter, o: Pos2, pos: Pos2) -> Option<usize> {
    menus(p).iter().position(|(_, x, w, _)| {
        winui::rect(Rect::from_min_size(o + vec2(*x, 0.0), vec2(*w, MENU_H))).contains(pos)
    })
}

/// The menu bar's open menu after `event` (`open`: the one open before it).
///
/// A menu that shut itself is closed here too, unless the press that shut it went down on a
/// title: that press's release is the title's click, which shuts the menu (its own title) or
/// opens another one. Escape, or the app losing the focus, with the pointer resting on the bar
/// (where the click that opened the menu left it) used to leave the menu marked open with
/// nothing shown, so the next click on its title only "shut" it, and nothing opened.
///
/// The pointer alone never opens a menu: it only moves from an open one to another (#277). Once
/// a menu closes in any way (a command chosen, Escape, a click elsewhere, another window taking
/// the focus), the bar waits for a click.
pub fn menubar_next(open: Option<usize>, event: MenuBarEvent) -> Option<usize> {
    match event {
        MenuBarEvent::Hover(i) => open.map(|_| i),
        MenuBarEvent::Title { i, released } => {
            if open == Some(i) && released {
                None
            } else {
                Some(i)
            }
        }
        MenuBarEvent::Dismissed {
            press_on_title: true,
        } => open,
        MenuBarEvent::Dismissed {
            press_on_title: false,
        }
        | MenuBarEvent::Chosen => None,
        MenuBarEvent::Open(i) => Some(i),
    }
}
