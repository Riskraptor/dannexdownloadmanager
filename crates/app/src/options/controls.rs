//! The pages' own controls: the tab strip, page titles, headers and group boxes, read-only and
//! password fields, reports, and OK / Cancel.

use super::*;

/// A page title's room, and its rule's length: from the title's start (x 63) to the content's
/// right edge (406).
pub(super) const HEADER_W: f32 = 343.0;

pub(super) fn ok_cancel(ui: &mut Ui, o: Pos2, x: f32, y: f32, valid: bool) -> Outcome<()> {
    let kind = if valid {
        ButtonKind::Default
    } else {
        ButtonKind::Disabled
    };
    let ok = winui::button_light(ui, r(o, x, y, 73.0, 21.0), "OK", kind, false).clicked();
    let cancel = winui::button_light(
        ui,
        r(o, x + 81.0, y, 73.0, 21.0),
        "Cancel",
        ButtonKind::Normal,
        false,
    )
    .clicked();
    if cancel || winui::escape_pressed(ui) {
        Outcome::Closed
    } else if valid && (ok || winui::enter_pressed(ui)) {
        Outcome::Done(())
    } else {
        Outcome::Open
    }
}

/// A row of tabs as shown: IDM's measured tabs in English; a translation's sized by its labels
/// (each label centred, the room left over shared out), so the row keeps its span (x 8..419)
/// and no label is cut.
pub(super) fn tab_row(
    p: &egui::Painter,
    row: &[(usize, &'static str, f32, f32, f32)],
    measured: bool,
) -> Vec<(usize, &'static str, f32, f32, f32)> {
    // `measured`: the row's English positions are IDM's (a row of ours is sized as a translation).
    if measured && row.iter().all(|t| crate::i18n::t(t.1) == t.1) {
        return row.to_vec();
    }
    let (left, right) = (row[0].2, row[row.len() - 1].3);
    let widths: Vec<f32> = row
        .iter()
        .map(|t| winui::text_width(p, crate::i18n::t(t.1)).ceil())
        .collect();
    let total: f32 = widths.iter().map(|w| w + 16.0).sum();
    let spare = ((right - left - total) / row.len() as f32).floor();
    let padded: Vec<f32> = widths.iter().map(|w| w + 16.0).collect();
    let cap = winui::width_cap(&padded, right - left);
    let mut x0 = left;
    row.iter()
        .zip(&widths)
        .enumerate()
        .map(|(k, (t, w))| {
            let x1 = if k + 1 == row.len() {
                right
            } else if spare < 0.0 {
                // Too wide for the strip: the narrow tabs keep their width, the wide ones share
                // the rest (their labels cut where drawn).
                x0 + (w + 16.0).min(cap)
            } else {
                x0 + w + 16.0 + spare
            };
            let tab = (t.0, t.1, x0, x1, x0 + ((x1 - x0 - w) / 2.0).floor());
            x0 = x1;
            tab
        })
        .collect()
}

/// The two-row tab strip and the page frame; returns the page clicked.
pub(super) fn tabs(ui: &mut Ui, o: Pos2, page: usize) -> Option<usize> {
    // (page, label, x0, x1, label x): the rows never move sideways, only swap front and back.
    // Remote (#141, ours) joins the first row, which is sized by its labels since.
    const G: [(usize, &str, f32, f32, f32); 6] = [
        // i18n: tab title: general settings
        (0, crate::i18n::mark("General"), 8.0, 82.0, 26.0),
        // i18n: tab title
        (1, crate::i18n::mark("File types"), 82.0, 166.0, 101.0),
        // i18n: tab title: where downloads are saved
        (2, crate::i18n::mark("Save to"), 166.0, 241.0, 185.0),
        // i18n: tab title: download settings
        (3, crate::i18n::mark("Downloads"), 241.0, 330.0, 260.0),
        // i18n: tab title: connection settings
        (4, crate::i18n::mark("Connection"), 330.0, 419.0, 349.0),
        (
            REMOTE_PAGE,
            // i18n: tab title: control DDM from a phone or another computer on the local network
            crate::i18n::mark("Remote"),
            380.0,
            419.0,
            390.0,
        ),
    ];
    // IDM's Dial Up / VPN tab is gone (D-021: no modem support; it did nothing); ours, Regional
    // (how dates and times are written), Accounts (#44, debrid services) and Archives (#37) share
    // the row, sized by their labels.
    const P: [(usize, &str, f32, f32, f32); 6] = [
        // i18n: tab title
        (5, crate::i18n::mark("Proxy / Socks"), 8.0, 104.0, 20.0),
        // i18n: tab title: user names and passwords for sites
        (6, crate::i18n::mark("Sites Logins"), 104.0, 192.0, 116.0),
        // i18n: tab title: the user's debrid service accounts (Real-Debrid…)
        (7, crate::i18n::mark("Accounts"), 192.0, 265.0, 204.0),
        // i18n: tab title: sound settings
        (8, crate::i18n::mark("Sounds"), 265.0, 336.0, 279.0),
        // i18n: tab title: how dates and times are written (date order, 12- or 24-hour clock)
        (9, crate::i18n::mark("Regional"), 336.0, 380.0, 350.0),
        // i18n: tab title: extracting downloaded archives, and Link grabber
        (10, crate::i18n::mark("Archives"), 380.0, 419.0, 390.0),
    ];
    let p = ui.painter().clone();
    let border = winui::HEADER_LINE();
    let (g, pr) = (tab_row(&p, &G, false), tab_row(&p, &P, false));
    let first_row = page < 5 || page == REMOTE_PAGE;
    let (front, back): (&[_], &[_]) = if first_row { (&g, &pr) } else { (&pr, &g) };
    let mut clicked = None;
    let mut tab = |ui: &mut Ui,
                   (i, label, x0, x1, lx): (usize, &str, f32, f32, f32),
                   top: f32,
                   base: f32,
                   sel: bool| {
        let (x0, x1) = if sel {
            (x0 - if x0 <= 8.0 { 2.0 } else { 1.0 }, x1 + 2.0)
        } else {
            (x0, x1)
        };
        let bottom = if sel { 45.0 } else { top + 17.0 };
        // From the leading edge: the first tab at the right in RTL.
        let rc = winui::rect(Rect::from_min_max(
            o + vec2(x0, top),
            o + vec2(x1 + 1.0, bottom + 1.0),
        ));
        // The strip is one Tab stop, the chosen tab (Windows' tab control); the arrows move.
        let sense = if sel {
            Sense::click()
        } else {
            winui::pointer_only()
        };
        let resp = ui.interact(rc, egui::Id::unique(("opt-tab", i)), sense);
        winui::expose(
            &resp,
            egui::Role::Tab,
            crate::i18n::t(label),
            true,
            Some(sel),
        );
        let resp = winui::enter_is_not_a_click(ui, resp);
        if resp.clicked() && !sel {
            clicked = Some(i);
        }
        if sel {
            winui::keep_keys(ui, &resp, true, false, false);
            if resp.has_focus() {
                // Left / Right (mirrored in RTL), Home / End through the pages in order.
                const ORDER: [usize; 12] = [0, 1, 2, 3, 4, 11, 5, 6, 7, 8, 9, 10];
                let at = ORDER.iter().position(|p| *p == i).unwrap_or(0) as isize;
                let fwd = if winui::rtl() { -1 } else { 1 };
                let to = if winui::take_key(ui, egui::Key::ArrowRight) {
                    Some(at + fwd)
                } else if winui::take_key(ui, egui::Key::ArrowLeft) {
                    Some(at - fwd)
                } else if winui::take_key(ui, egui::Key::Home) {
                    Some(0)
                } else if winui::take_key(ui, egui::Key::End) {
                    Some(ORDER.len() as isize - 1)
                } else {
                    None
                };
                if let Some(to) = to {
                    let next = ORDER[to.clamp(0, ORDER.len() as isize - 1) as usize];
                    clicked = Some(next);
                    // The keys stay on the strip: the new tab takes the focus.
                    ui.memory_mut(|m| m.request_focus(egui::Id::unique(("opt-tab", next))));
                }
            }
        }
        let face = if sel {
            LIGHT_PAGE()
        } else if resp.hovered() {
            winui::TAB_HOT()
        } else {
            winui::TAB_UNSELECTED()
        };
        fill(&p, o.x + x0, o.y + top, o.x + x1, o.y + bottom, face);
        fill(
            &p,
            o.x + x0 + 1.0,
            o.y + top,
            o.x + x1 - 1.0,
            o.y + top,
            border,
        );
        fill(
            &p,
            o.x + x0,
            o.y + top + 1.0,
            o.x + x0,
            o.y + bottom,
            border,
        );
        fill(
            &p,
            o.x + x1,
            o.y + top + 1.0,
            o.x + x1,
            o.y + bottom,
            border,
        );
        // A translated row too wide for the strip has narrower tabs (`tab_row`): their labels are
        // cut with "..." and shown whole on hover.
        let full = crate::i18n::t(label);
        let shown = winui::ellipsize(&p, &winui::tahoma(), full, x1 - x0 - 6.0);
        if shown == full {
            text(&p, o.x + lx, o.y + base, full, LIGHT_TEXT());
        } else {
            let w = winui::text_width(&p, &shown);
            let lx = x0 + ((x1 - x0 - w) / 2.0).floor();
            let rc = text(&p, o.x + lx, o.y + base, &shown, LIGHT_TEXT());
            winui::full_on_hover(ui, rc, ("opt-tab-label", i), full);
        }
        winui::show_focus(ui, &resp, rc.shrink(2.0));
    };
    for t in back {
        tab(ui, *t, 9.0, 22.0, false);
    }
    for t in front.iter().filter(|t| t.0 != page) {
        tab(ui, *t, 27.0, 40.0, false);
    }
    // Page frame (x 6..421, y 45..468 and the pages' room more) with its shadow, then the
    // selected tab over its top.
    let foot = 468.0 + PAGE_GROW;
    fill(
        &p,
        o.x + 6.0,
        o.y + 45.0,
        o.x + 421.0,
        o.y + foot,
        LIGHT_PAGE(),
    );
    fill(&p, o.x + 6.0, o.y + 45.0, o.x + 421.0, o.y + 45.0, border);
    fill(&p, o.x + 6.0, o.y + foot, o.x + 421.0, o.y + foot, border);
    fill(&p, o.x + 6.0, o.y + 45.0, o.x + 6.0, o.y + foot, border);
    fill(&p, o.x + 421.0, o.y + 45.0, o.x + 421.0, o.y + foot, border);
    let shadow = winui::TAB_UNSELECTED();
    fill(
        &p,
        o.x + 422.0,
        o.y + 45.0,
        o.x + 423.0,
        o.y + foot + 1.0,
        shadow,
    );
    fill(
        &p,
        o.x + 6.0,
        o.y + foot + 1.0,
        o.x + 423.0,
        o.y + foot + 1.0,
        shadow,
    );
    if let Some(t) = front.iter().find(|t| t.0 == page) {
        tab(ui, *t, 25.0, 38.0, true);
    }
    clicked
}

/// Page header: the icon, the title in bold after it and the etched rule under the title, from
/// the title's start to the page's content edge (a long title is cut with "...", whole on hover).
pub(super) fn header(ui: &mut Ui, o: Pos2, icon: Icon, title: &str) {
    let p = ui.painter().clone();
    icon.image(true).paint_at(ui, r(o, 21.0, 54.0, 32.0, 32.0));
    let full = crate::i18n::t(title);
    let shown = winui::ellipsize(&p, &winui::tahoma_bold(), full, HEADER_W);
    let rc = winui::text_font(
        &p,
        winui::tahoma_bold(),
        o.x + 63.0,
        o.y + 69.0,
        &shown,
        LIGHT_TEXT(),
    );
    if shown != full {
        winui::full_on_hover(ui, rc, ("page-title", title), full);
    }
    winui::etched_h(&p, o.x + 63.0, o.y + 78.0, HEADER_W);
}

/// `english` translated, as a page title in `room` px: one line where it fits,
/// else two ([`winui::fit_lines`]); and how wide it is drawn.
pub(super) fn page_title_lines(
    p: &egui::Painter,
    font: egui::FontId,
    english: &str,
    room: f32,
) -> (Vec<String>, f32) {
    let lines = winui::fit_lines(p, font.clone(), crate::i18n::t(english), room);
    let w = lines
        .iter()
        .map(|l| winui::text_width_font(p, font.clone(), l))
        .fold(0.0, f32::max);
    (lines, w)
}

/// Group box (`#dcdcdc` frame) with its caption (translated here) at x + 9.
pub(super) fn group(ui: &Ui, o: Pos2, x: f32, y: f32, w: f32, h: f32, caption: &str) {
    let p = ui.painter();
    let full = crate::i18n::t(caption);
    // A caption longer than the box is cut with "..." before its right edge (whole on hover).
    let caption = winui::ellipsize(p, &winui::tahoma(), full, w - 18.0);
    let cw = winui::text_width(p, &caption);
    winui::group_box(
        p,
        r(o, x, y, w, h),
        Some((o.x + x + 7.0, o.x + x + 9.0 + cw + 2.0)),
    );
    let rc = text(p, o.x + x + 9.0, o.y + y + 4.0, &caption, LIGHT_TEXT());
    if caption != full {
        winui::full_on_hover(ui, rc, ("group", caption.as_str()), full);
    }
}

/// A label (translated here) that IDM greys out with the classic embossed look when its
/// controls are off.
pub(super) fn disabled_label(
    p: &egui::Painter,
    x: f32,
    base: f32,
    s: &str,
    enabled: bool,
    right: bool,
) {
    let s = crate::i18n::t(s);
    let x = if right {
        x - winui::text_width(p, s)
    } else {
        x
    };
    winui::label(p, x, base, s, enabled);
}

pub(crate) fn list_frame(p: &egui::Painter, rc: Rect) {
    p.rect_filled(rc, 0.0, winui::FIELD());
    p.rect_stroke(
        rc,
        0.0,
        egui::Stroke::new(1.0, winui::FIELD_EDGE()),
        egui::StrokeKind::Inside,
    );
}

/// A report list (header 25 px, rows 17 px): `cols` are (title, right edge in client x).
/// Returns true on a double click of a row.
#[allow(clippy::too_many_arguments)]
pub(super) fn report(
    ui: &mut Ui,
    o: Pos2,
    rc: Rect,
    cols: &[(&str, f32)],
    rows: &[Vec<String>],
    st: &mut winui::RowList,
    grid: bool,
    id: &str,
) -> (bool, bool) {
    let p = ui.painter().clone();
    list_frame(&p, rc);
    // Laid out in English, as `cols` are; each part made on screen.
    let rc = winui::rect(rc);
    let left = rc.left();
    let head = rc.top() + 1.0;
    let divider = winui::HEADER_LINE();
    fill(
        &p,
        left + 1.0,
        head,
        rc.right() - 2.0,
        head + 23.0,
        winui::LIST_HEADER(),
    );
    let mut x0 = left;
    for (i, (title, right)) in cols.iter().enumerate() {
        let title = crate::i18n::t(title);
        let right = o.x + right;
        let tx = if i == 0 { x0 + 8.0 } else { x0 + 7.0 };
        let (rect, cut) = winui::text_fit(&p, tx, head + 16.0, title, LIGHT_TEXT(), right - 3.0);
        if cut {
            winui::full_on_hover(ui, rect, (id, "head", i), title);
        }
        fill(&p, right, head + 1.0, right, head + 23.0, divider);
        x0 = right;
    }
    // The rows: the shared row list under the header (its frame on the list's).
    let body = Rect::from_min_max(
        pos2(rc.left() + 1.0, head + 25.0),
        pos2(rc.right() - 1.0, rc.bottom() - 1.0),
    );
    let gridc = winui::GRID();
    let ev = winui::row_list(
        ui,
        (id, "rows"),
        winui::rect(body.expand(1.0)),
        rows.len(),
        17.0,
        st,
        |ui, clip, k, line, look| {
            let top = line.top();
            // Read as its cells, joined.
            look.name = rows[k]
                .iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if grid {
                fill(
                    clip,
                    line.left(),
                    top + 16.0,
                    line.right(),
                    top + 16.0,
                    gridc,
                );
            }
            let mut cx = left;
            for (i, (_, right)) in cols.iter().enumerate() {
                let right = o.x + right;
                if grid {
                    fill(clip, right + 1.0, top, right + 1.0, top + 16.0, gridc);
                }
                if let Some(cell) = rows[k].get(i) {
                    winui::fit(
                        ui,
                        clip,
                        cx + 6.0,
                        top + 12.0,
                        cell,
                        look.ink,
                        right - 3.0,
                        (id, "cell", k, i),
                    );
                }
                cx = right;
            }
        },
    );
    if ev.focused {
        st.type_to(ui, id, rows.len(), |i| {
            rows[i].first().cloned().unwrap_or_default()
        });
    }
    // A double-click or Enter edits (Enter taken: not the dialog's OK too); Delete removes.
    if ev.entered.is_some() {
        winui::take_key(ui, egui::Key::Enter);
    }
    (
        ev.double_clicked.or(ev.entered).is_some(),
        ev.delete.is_some(),
    )
}

/// A multi-line edit that wraps its text (the shared one).
pub(super) fn multiline(ui: &mut Ui, rc: Rect, value: &mut String, id: &str, enabled: bool) {
    winui::multiline_light(ui, rc, value, id, enabled);
}

/// A read-only field (the page colour, as IDM's): the shared one, so it can be copied.
pub(super) fn readonly(ui: &mut Ui, rc: Rect, value: &str) {
    winui::readonly_field(
        ui,
        rc,
        value,
        LIGHT_PAGE(),
        false,
        ("options", rc.left() as i32, rc.top() as i32),
    );
}

/// [`readonly`] for an address: left to right in every language.
pub(super) fn readonly_ltr(ui: &mut Ui, rc: Rect, value: &str) {
    winui::readonly_field(
        ui,
        rc,
        value,
        LIGHT_PAGE(),
        true,
        ("options", rc.left() as i32, rc.top() as i32),
    );
}

/// A password edit (characters shown as bullets): the shared one, on the tab page.
pub(super) fn password(ui: &mut Ui, rc: Rect, value: &mut String, enabled: bool) {
    winui::password_light_on(ui, rc, value, enabled, LIGHT_PAGE());
}

/// Options → Accounts: where the sign-ins (and Sites Logins' passwords, the proxy's) are kept,
/// as `secrets::Vault::kept` found it. None while it is still looking.
pub(super) fn kept_text(kept: crate::secrets::Kept) -> Option<&'static str> {
    use crate::secrets::Kept;
    Some(match kept {
        Kept::Looking => return None,
        #[cfg(windows)]
        Kept::Keychain => {
            // i18n: Options → Accounts: where the debrid accounts' sign-ins and the other passwords are kept (Windows' own Credential Manager, in Control Panel)
            crate::i18n::mark("Sign-ins and passwords are kept in Windows Credential Manager.")
        }
        #[cfg(target_os = "macos")]
        Kept::Keychain => {
            // i18n: Options → Accounts: where the debrid accounts' sign-ins and the other passwords are kept (the macOS login keychain)
            crate::i18n::mark("Sign-ins and passwords are kept in the macOS keychain.")
        }
        #[cfg(target_os = "linux")]
        // i18n: Options → Accounts: where the debrid accounts' sign-ins and the other passwords are kept (Linux: GNOME Keyring, KeePassXC... through the Secret Service)
        Kept::Keychain => crate::i18n::mark(
            "Sign-ins and passwords are kept in the system keyring (Secret Service).",
        ),
        #[cfg(windows)]
        // i18n: Options → Accounts: a portable copy (it writes nothing outside its folder) keeps the sign-ins in its own settings, encrypted so that only this Windows account can read them
        Kept::Portable => crate::i18n::mark(
            "This portable copy keeps sign-ins and passwords in its own settings, encrypted for your Windows account.",
        ),
        #[cfg(not(windows))]
        // i18n: Options → Accounts: a portable copy (it writes nothing outside its folder) keeps the sign-ins in its own settings folder, which only the user's account may read
        Kept::Portable => crate::i18n::mark(
            "This portable copy keeps sign-ins and passwords in its own settings, which only you can read.",
        ),
        // i18n: Options → Accounts (Linux): the system keyring is locked (DDM never unlocks it: that would ask for its password), so the sign-ins stay in DDM's settings folder, which only the user's account may read
        Kept::Locked => crate::i18n::mark(
            "The system keyring is locked and DDM doesn't unlock it, so sign-ins and passwords are kept in DDM's settings, which only you can read.",
        ),
        #[cfg(windows)]
        // i18n: Options → Accounts: Windows Credential Manager didn't answer, so the sign-ins stay in DDM's settings, encrypted so that only this Windows account can read them
        Kept::Missing => crate::i18n::mark(
            "Windows Credential Manager didn't answer, so sign-ins and passwords are kept in DDM's settings, encrypted for your Windows account.",
        ),
        #[cfg(not(windows))]
        // i18n: Options → Accounts: no system keyring/keychain answered (none runs, e.g. a desktop without one), so the sign-ins stay in DDM's settings folder, which only the user's account may read
        Kept::Missing => crate::i18n::mark(
            "No system keyring answered, so sign-ins and passwords are kept in DDM's settings, which only you can read.",
        ),
    })
}
