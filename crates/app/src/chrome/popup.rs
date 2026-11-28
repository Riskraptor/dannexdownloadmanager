//! Popup menus (the menu bar's, the right-click menus): their entries, keys and mnemonics, their own
//! windows placed on the screen, and each level drawn and answered.

use super::*;

// ---- popup menus ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum MenuEntry<C> {
    /// Label (with `&` before the mnemonic letter), command, enabled, checked.
    Item(&'static str, C, bool, bool),
    /// Like `Item`, but checked shows a bullet (Win32 radio-check items).
    Radio(&'static str, C, bool, bool),
    /// Label, enabled, children.
    Sub(&'static str, bool, Vec<MenuEntry<C>>),
    Sep,
}

pub(super) const SLOT: f32 = 22.0;

pub(super) const SEP: f32 = 9.0;

pub enum PopupResult<C> {
    Open,
    Chosen(C),
    Dismissed,
    /// Left or Right on a menu of the bar: open the menu before (-1) or after (+1) it.
    Neighbor(i32),
}

/// What a popup menu is: how its window says so to the window system (X11's window types) and,
/// on X11, whether it shares its window with others ([`menu_viewport`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuKind {
    /// A menu of the menu bar, and its submenus.
    Bar,
    /// A context menu: a right click, the Menu key, a toolbar button's drop-down.
    Context,
    /// A drop-down list's choices: a combo box's, a text box's history.
    List,
}

/// How long the focus may be away from every window of ours before an open menu counts as left.
/// On Linux a window manager may give a menu's window the focus (KWin, sway, and GNOME under
/// Wayland give it to a submenu's as it opens). When that submenu closes (the pointer back on
/// another item of its parent), its window passes the focus on, and for a frame or more no
/// window of ours has it: counted as the app losing the focus, it closed the whole menu (#250).
pub(super) const FOCUS_GRACE: f64 = 0.4;

/// Whether an open menu still has the focus: the main window's or one of the menu's windows'.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct MenuFocus {
    /// One of ours has had the focus since the menu opened.
    pub(super) had: bool,
    /// Since when none of ours has had it (the app's clock, in seconds).
    pub(super) away_since: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FocusVerdict {
    /// Ours, or never had (a menu opened while another app was in front).
    Here,
    /// Gone for less than FOCUS_GRACE: look again in `wait` seconds.
    Away { wait: f64 },
    /// Gone for FOCUS_GRACE: another app has it, the menu closes.
    Left,
}

impl MenuFocus {
    /// After a pass at `now`: `focused` is whether one of our windows had the focus in it.
    pub(super) fn pass(&mut self, focused: bool, now: f64) -> FocusVerdict {
        if focused {
            self.had = true;
            self.away_since = None;
            return FocusVerdict::Here;
        }
        if !self.had {
            return FocusVerdict::Here;
        }
        let since = *self.away_since.get_or_insert(now);
        let away = now - since;
        if away >= FOCUS_GRACE {
            FocusVerdict::Left
        } else {
            FocusVerdict::Away {
                wait: FOCUS_GRACE - away,
            }
        }
    }
}

/// How a borderless window of ours looks and takes the pointer ([`style_popup`]).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PopupLook {
    /// A menu: never activated (the main window keeps the focus), small rounded corners and
    /// the menu's border, no open animation.
    Menu,
    /// A small floating window (the drop target): activated as any window, rounded corners.
    Floating,
}

/// A menu label made at run time (a queue's name): each distinct text is kept once for the
/// life of the app, so menus rebuilt every frame don't allocate or leak more than that.
pub fn label(text: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static LABELS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut set = LABELS.get_or_init(Default::default).lock().unwrap();
    if let Some(l) = set.get(text) {
        return l;
    }
    let l: &'static str = Box::leak(text.to_owned().into_boxed_str());
    set.insert(l);
    l
}

/// A menu title's access letter, lower case ("&Tasks": "t"; none marked: its first letter).
pub fn menu_letter(label: &str) -> String {
    let (shown, at) = plain(label);
    shown
        .chars()
        .nth(at.unwrap_or(0))
        .map(|c| c.to_lowercase().to_string())
        .unwrap_or_default()
}

pub(super) fn plain(label: &str) -> (String, Option<usize>) {
    let mut out = String::new();
    let mut mnemonic = None;
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' && chars.peek() == Some(&'&') {
            // "&&": an "&" in the text (a queue named "Tom & Jerry"), no access key.
            out.push(chars.next().unwrap());
        } else if c == '&' && chars.peek().is_some() {
            mnemonic = Some(out.chars().count());
            out.push(chars.next().unwrap());
        } else {
            out.push(c);
        }
    }
    (out, mnemonic)
}

/// A key or key combination as written in the shortcuts list ("Ctrl+N", "Shift+Delete"), as
/// this system shows it: macOS's symbols (⌘N, ⇧⌦), elsewhere the translated key names. It keeps
/// its own direction (a first-strong isolate): a combination reads left to right in a
/// right-to-left window too ("Ctrl++ / Ctrl+-" ran as "-+Ctrl++ / Ctrl"), an Arabic phrase
/// ("Drag on blank space") right to left.
pub fn key_label(spec: &'static str) -> String {
    let keys = if cfg!(target_os = "macos") {
        // Translated first ("Drag on blank space", "Home / End"...), then the Mac's symbols.
        crate::i18n::tc("key", spec)
            .replace("Ctrl+", "\u{2318}")
            .replace("Shift+", "\u{21e7}")
            .replace("Alt+", "\u{2325}")
            .replace("Delete", "\u{2326}")
            .replace("Enter", "\u{21a9}")
    } else {
        crate::i18n::tc("key", spec).to_owned()
    };
    if crate::i18n::rtl() {
        crate::i18n::isolate(&keys)
    } else {
        keys
    }
}

/// A menu label without the shortcut IDM writes into some labels ("Find (Ctrl-F)"), when the
/// menu shows the shortcut in its own column.
pub(super) fn without_shortcut(text: &str) -> &str {
    let t = text.trim_end();
    if let Some(open) = t.rfind(" (")
        && t.ends_with(')')
    {
        let inner = &t[open + 2..t.len() - 1];
        let keyish = inner.len() <= 12
            && !inner.contains(' ')
            && (inner.contains('-') || inner.contains('+') || inner.starts_with('F'));
        if keyish {
            return &t[..open];
        }
    }
    t
}

/// Open a menu from the keyboard: its first item is highlighted.
pub fn start_keyboard(ctx: &egui::Context, id: &str) {
    show_cues(ctx);
    let kb = egui::Id::unique(("popup-sub", id)).with("kb");
    ctx.data_mut(|d| d.insert_temp::<(Option<usize>, Option<usize>)>(kb, (Some(usize::MAX), None)));
}

/// Keyboard cues on: the user reached for the keyboard (Alt, F10, a menu opened by keys), so
/// light mode underlines access letters from now on, the Windows rule (dark mode always does).
pub fn show_cues(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("keyboard-cues"), true));
}

/// Whether access letters are underlined: always in dark mode, after [`show_cues`] in light.
pub fn cues(ctx: &egui::Context) -> bool {
    crate::theme::colors().access_keys_always
        || ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::unique("keyboard-cues"))
                .unwrap_or(false)
        })
}

/// The next item of `level` from `from` in direction `dir`, skipping separators (wraps).
pub(super) fn step<C>(level: &[MenuEntry<C>], from: Option<usize>, dir: i32) -> Option<usize> {
    let n = level.len() as i32;
    if n == 0 {
        return None;
    }
    let mut i = match from {
        Some(f) if f < level.len() => f as i32,
        _ if dir > 0 => -1,
        _ => n,
    };
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if !matches!(level[i as usize], MenuEntry::Sep) {
            return Some(i as usize);
        }
    }
    None
}

pub(super) fn entry_label<C>(e: &MenuEntry<C>) -> Option<(&'static str, bool)> {
    match e {
        MenuEntry::Item(l, _, en, _) | MenuEntry::Radio(l, _, en, _) | MenuEntry::Sub(l, en, _) => {
            Some((l, *en))
        }
        MenuEntry::Sep => None,
    }
}

/// The keys an open menu takes from `i` (without Ctrl or Command: those are shortcuts).
pub(super) fn menu_keys(i: &egui::InputState) -> Vec<egui::Key> {
    i.events
        .iter()
        .filter_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.command && !modifiers.ctrl => Some(*key),
            _ => None,
        })
        .collect()
}

/// Keys for an open menu (pressed in the main window or in the menu's own windows): Up and Down
/// move the highlight (Home / End to the first / last), Right opens a submenu or the next menu, Left closes a submenu or opens
/// the previous menu, Enter or an item's underlined letter runs it, Escape closes a submenu.
pub(super) fn keyboard<C: Clone>(
    ctx: &egui::Context,
    entries: &[MenuEntry<C>],
    sub_key: egui::Id,
) -> Option<PopupResult<C>> {
    let kb_key = sub_key.with("kb");
    let mut keys = ctx.input(menu_keys);
    // Pressed in the menu's own windows: popup_level noted them as those windows drew.
    keys.extend(
        ctx.data_mut(|d| d.remove_temp::<Vec<egui::Key>>(sub_key.with("window-keys")))
            .unwrap_or_default(),
    );
    let (mut hl0, mut hl1): (Option<usize>, Option<usize>) =
        ctx.data(|d| d.get_temp(kb_key)).unwrap_or_default();
    if hl0 == Some(usize::MAX) {
        hl0 = step(entries, None, 1);
        ctx.data_mut(|d| d.insert_temp(kb_key, (hl0, hl1)));
    }
    if keys.is_empty() {
        return None;
    }
    // The keys are the menu's: the list underneath doesn't move too.
    ctx.input_mut(|i| {
        for k in &keys {
            i.consume_key(egui::Modifiers::NONE, *k);
        }
    });
    let mut open_sub = ctx.data(|d| d.get_temp::<usize>(sub_key));
    let mut out = None;
    for key in keys {
        let children: Option<&[MenuEntry<C>]> = open_sub.and_then(|i| match entries.get(i) {
            Some(MenuEntry::Sub(_, _, c)) => Some(c.as_slice()),
            _ => None,
        });
        let in_sub = children.is_some() && hl1.is_some();
        let level = if in_sub {
            children.unwrap_or(&[])
        } else {
            entries
        };
        let hl = if in_sub { hl1 } else { hl0 };
        let mut target = None;
        match key {
            egui::Key::ArrowDown | egui::Key::ArrowUp => {
                let n = step(level, hl, if key == egui::Key::ArrowDown { 1 } else { -1 });
                if in_sub { hl1 = n } else { hl0 = n }
            }
            // Home / End: the first / last item, as Windows' menus have them.
            egui::Key::Home | egui::Key::End => {
                let n = step(level, None, if key == egui::Key::Home { 1 } else { -1 });
                if in_sub { hl1 = n } else { hl0 = n }
            }
            egui::Key::Escape if in_sub => {
                open_sub = None;
                hl1 = None;
            }
            egui::Key::Escape => out = Some(PopupResult::Dismissed),
            egui::Key::ArrowLeft if in_sub => {
                open_sub = None;
                hl1 = None;
            }
            egui::Key::ArrowLeft => out = Some(PopupResult::Neighbor(-1)),
            egui::Key::ArrowRight => match hl.and_then(|h| level.get(h)) {
                Some(MenuEntry::Sub(_, true, c)) if !in_sub => {
                    open_sub = hl;
                    hl1 = step(c, None, 1);
                }
                _ => out = Some(PopupResult::Neighbor(1)),
            },
            egui::Key::Enter | egui::Key::Space => target = hl,
            other => {
                let name = other.name().to_lowercase();
                if name.chars().count() == 1 {
                    target = level.iter().position(|e| {
                        entry_label(e).is_some_and(|(l, en)| {
                            let (t, m) = plain(crate::i18n::tc("menu", l));
                            en && m
                                .and_then(|m| t.chars().nth(m))
                                .is_some_and(|c| c.to_lowercase().to_string() == name)
                        })
                    });
                }
            }
        }
        if let Some(t) = target {
            if in_sub {
                hl1 = Some(t)
            } else {
                hl0 = Some(t)
            }
            match level.get(t) {
                Some(MenuEntry::Item(_, c, true, _) | MenuEntry::Radio(_, c, true, _)) => {
                    out = Some(PopupResult::Chosen(c.clone()));
                }
                Some(MenuEntry::Sub(_, true, c)) if !in_sub => {
                    open_sub = Some(t);
                    hl1 = step(c, None, 1);
                }
                _ => {}
            }
        }
        if out.is_some() {
            break;
        }
    }
    ctx.data_mut(|d| {
        d.insert_temp(kb_key, (hl0, hl1));
        match open_sub {
            Some(i) => {
                d.insert_temp(sub_key, i);
            }
            None => {
                d.remove::<usize>(sub_key);
            }
        }
    });
    out
}

/// Show a popup menu with its top-left at `pos` (main-window client coordinates, on screen) —
/// its top-right in a right-to-left interface, where menus open to the left.
///
/// Like a Win32 menu, each level is its own borderless top-level window (an immediate egui
/// viewport): it can extend past the main window, flips to stay on the monitor, never takes
/// activation (so Escape still reaches the main window), and clicks on it never reach the
/// widgets underneath. Call every frame while open, from the main window's `ui`.
pub fn popup<C: Clone>(
    ctx: &egui::Context,
    kind: MenuKind,
    id: &str,
    pos: Pos2,
    entries: &[MenuEntry<C>],
    hint: &dyn Fn(&C) -> Option<&'static str>,
) -> PopupResult<C> {
    let sub_key = egui::Id::unique(("popup-sub", id));
    // The control that has the keys while the menu is open (a drop-down list, a text box): the
    // menu's windows never take them, so Escape reached its window, and egui let them go there
    // (Escape drops the focus). Escape closing the menu gives them back.
    let opener = sub_key.with("opener");
    let focused_now = ctx.memory(|m| m.focused());
    if let Some(f) = focused_now {
        ctx.data_mut(|d| d.insert_temp(opener, f));
    }
    let give_back = |ctx: &egui::Context| {
        let had = ctx.data_mut(|d| {
            let had = d.get_temp::<egui::Id>(opener);
            d.remove::<egui::Id>(opener);
            had
        });
        if let Some(had) = had
            && focused_now.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            ctx.memory_mut(|m| m.request_focus(had));
        }
    };
    if let Some(r) = keyboard(ctx, entries, sub_key) {
        ctx.data_mut(|d| {
            d.remove::<usize>(sub_key);
            d.remove::<(Option<usize>, Option<usize>)>(sub_key.with("kb"));
        });
        give_back(ctx);
        return r;
    }
    let origin = ctx
        .input(|i| i.viewport().inner_rect)
        .map_or(Pos2::ZERO, |r| r.min);
    let mut result = None;
    // Set by the menu's windows as they draw (popup_level) when one of them has the focus.
    let window_focus = sub_key.with("window-focus");
    ctx.data_mut(|d| d.insert_temp(window_focus, false));
    popup_level(
        ctx,
        kind,
        id,
        origin + pos.to_vec2(),
        None,
        entries,
        sub_key,
        &mut result,
        0,
        hint,
    );
    if let Some(c) = result {
        ctx.data_mut(|d| {
            d.remove::<usize>(sub_key);
            d.remove::<(Option<usize>, Option<usize>)>(sub_key.with("kb"));
        });
        give_back(ctx);
        return PopupResult::Chosen(c);
    }
    // The menus are other windows: any press that reaches the main window, Escape, or the
    // app losing focus closes them (as Win32 menus do) — except on the frame the menu opened,
    // whose press may be the very click that opened it.
    let pass = ctx.cumulative_pass_nr();
    let shown = sub_key.with("shown");
    // Focus in the menu's own windows is still ours: on macOS and Linux a click in a menu window
    // gives that window the focus, and must not count as clicking away.
    let focused = ctx.input(|i| i.viewport().focused) == Some(true)
        || ctx.data(|d| d.get_temp::<bool>(window_focus).unwrap_or(false));
    // (first pass, last pass, the focus): focus is only *lost* if it was had, and only when no
    // window of ours gets it back within FOCUS_GRACE (#250).
    let now = ctx.input(|i| i.time);
    let (first, verdict) = ctx.data_mut(|d| {
        let (first, last, seen) = d.get_temp::<(u64, u64, MenuFocus)>(shown).unwrap_or((
            pass,
            pass,
            MenuFocus::default(),
        ));
        let (first, mut seen) = if last + 1 < pass {
            (pass, MenuFocus::default())
        } else {
            (first, seen)
        };
        let verdict = seen.pass(focused, now);
        d.insert_temp(shown, (first, pass, seen));
        (first == pass, verdict)
    });
    if let FocusVerdict::Away { wait } = verdict {
        // No event may come to look again (the pointer resting on the menu).
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    let lost_focus = verdict == FocusVerdict::Left;
    if !first
        && (lost_focus
            || ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.any_pressed()))
    {
        // Closed by a press: where it was, so the control that opened it doesn't take that
        // press's click (on release) as opening it again (winui::opener_click). Read before
        // the data lock (asking the context inside it deadlocks).
        let press = ctx.input(|i| i.pointer.press_origin().filter(|_| i.pointer.any_pressed()));
        let dismissed = dismissed_at_key(ctx);
        ctx.data_mut(|d| {
            d.remove::<usize>(sub_key);
            d.remove::<(Option<usize>, Option<usize>)>(sub_key.with("kb"));
            if let Some(at) = press {
                d.insert_temp(dismissed, at);
            }
        });
        give_back(ctx);
        return PopupResult::Dismissed;
    }
    PopupResult::Open
}

/// Where the last press that closed a popup went down, in this window.
pub fn dismissed_at_key(ctx: &egui::Context) -> egui::Id {
    egui::Id::unique(("popup-dismissed-at", ctx.viewport_id()))
}

/// Open a menu from the keyboard with item `i` highlighted (a drop-down list on its choice).
pub fn start_at(ctx: &egui::Context, id: &str, i: usize) {
    show_cues(ctx);
    let kb = egui::Id::unique(("popup-sub", id)).with("kb");
    ctx.data_mut(|d| d.insert_temp::<(Option<usize>, Option<usize>)>(kb, (Some(i), None)));
}

/// Open a menu with its entry `i`'s submenu open and the submenu's first entry highlighted, as
/// the keyboard leaves it (`/show menu:torrent`, for screenshots).
pub fn open_sub_at(ctx: &egui::Context, id: &str, i: usize) {
    show_cues(ctx);
    let sub_key = egui::Id::unique(("popup-sub", id));
    ctx.data_mut(|d| {
        d.insert_temp(sub_key, i);
        d.insert_temp::<(Option<usize>, Option<usize>)>(sub_key.with("kb"), (Some(i), Some(0)));
    });
}

/// The surface of a popup drawn inside a window (the date picker's calendar, a column's filter
/// box): the menus' colours and border, rounded as Windows 11's flyouts (they had their own).
pub fn popup_surface(p: &egui::Painter, rc: egui::Rect) {
    let radius = crate::theme::metrics().flyout_radius;
    p.rect_filled(rc, radius, POPUP_BG());
    p.rect_stroke(
        rc,
        radius,
        egui::Stroke::new(1.0, POPUP_BORDER()),
        egui::StrokeKind::Inside,
    );
}

/// A menu label's width as Win32 sizes menus: GDI's whole-pixel width (egui's advances differ
/// by up to a pixel either way: IDM's menus are 236, 291 wide where egui gives 237, 290), or the
/// width it is drawn at when that is wider by more (a script drawn in a fallback font, such as
/// Japanese).
pub(super) fn menu_width(p: &egui::Painter, label: &str) -> f32 {
    let drawn = text_width_font(p, winui::segoe(), label).ceil();
    #[cfg(windows)]
    {
        let gdi = crate::win32::menu_text_width(label) as f32;
        if drawn > gdi + 1.0 { drawn } else { gdi }
    }
    #[cfg(not(windows))]
    {
        drawn
    }
}

/// Size of a menu window: the check column (38), the widest label, then 30 to the edge (48 when an
/// item opens a submenu: its arrow), slots of 22 (separators 9), 3 px top and bottom. Fitted to
/// its texts, so they don't float in a wide window.
pub(super) fn menu_size<C>(
    ctx: &egui::Context,
    entries: &[MenuEntry<C>],
    hint: &dyn Fn(&C) -> Option<&'static str>,
) -> egui::Vec2 {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let width = |t: &str| menu_width(&painter, t);
    let widest = entries
        .iter()
        .filter_map(|e| match e {
            MenuEntry::Item(l, c, ..) | MenuEntry::Radio(l, c, ..) => Some(match hint(c) {
                Some(k) => {
                    width(without_shortcut(&plain(crate::i18n::tc("menu", l)).0))
                        + 28.0
                        + text_width_font(&painter, winui::segoe(), &key_label(k)).ceil()
                }
                None => width(&plain(crate::i18n::tc("menu", l)).0),
            }),
            MenuEntry::Sub(l, ..) => Some(width(&plain(crate::i18n::tc("menu", l)).0)),
            MenuEntry::Sep => None,
        })
        .fold(0.0, f32::max);
    let h: f32 = entries
        .iter()
        .map(|e| {
            if matches!(e, MenuEntry::Sep) {
                SEP
            } else {
                SLOT
            }
        })
        .sum();
    let has_sub = entries.iter().any(|e| matches!(e, MenuEntry::Sub(..)));
    vec2(38.0 + widest + if has_sub { 48.0 } else { 30.0 }, h + 6.0)
}

/// The builder of every borderless window of ours (a menu, the drop target): no title bar, not
/// resizable, no taskbar button, always on top. On Windows it is created hidden: its first pass
/// makes it a plain popup ([`style_popup`]) and [`show_popup`] shows it then.
pub fn popup_viewport(title: &str) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top()
        // Hidden until styled, then shown (show_popup).
        .with_visible(!cfg!(windows))
}

/// Call first in the pass of a borderless window titled `title` (made with [`popup_viewport`]),
/// before anything is drawn in it. On Windows it becomes a plain popup window, its client the
/// whole window (win32::popup_window): winit's borderless windows keep a caption style, which
/// on a Windows 11 Insider build (22635) put a black band a title bar tall across the top of
/// every menu and of the drop target, their picture moved down by as much (#208). Styled in
/// the window's own pass: its frame changes, and a picture drawn before that was lost with the
/// old frame (black until something made it draw again). True on the pass that styled it: show
/// it then ([`show_popup`]). Elsewhere nothing to do (false).
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn style_popup(title: &str, look: PopupLook) -> bool {
    #[cfg(windows)]
    {
        use crate::win32::Popup;
        let kind = match look {
            PopupLook::Menu => Popup::Menu {
                border: u32::from_le_bytes([
                    POPUP_BORDER().r(),
                    POPUP_BORDER().g(),
                    POPUP_BORDER().b(),
                    0,
                ]),
            },
            PopupLook::Floating => Popup::Floating,
        };
        crate::win32::popup_window(title, kind)
    }
    #[cfg(not(windows))]
    false
}

/// Show the borderless window `vid` [`style_popup`] just styled (from its parent's pass).
pub fn show_popup(ctx: &egui::Context, vid: egui::ViewportId) {
    ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Visible(true));
    // The command is carried out on the window's next frame: a menu opened from the keyboard
    // (Shift+F10, the Menu key) or a screen reader moves no pointer to bring one, and stayed
    // hidden (#113).
    ctx.request_repaint_of(vid);
    ctx.request_repaint();
}

/// Place a menu of `size` at screen point `at`, flipped to stay inside the monitor's work area:
/// up (or left) of the point, or for a submenu (`parent`: the parent menu's screen rect) to the
/// parent's left side. In a right-to-left interface `at` is the menu's top-right: it opens to
/// the left, and flips to the right (a submenu to the parent's right side). All in points: the
/// work area too (it was taken in pixels, which differ at a display or interface scale other
/// than 100 %, #262).
pub(super) fn place_menu(
    ctx: &egui::Context,
    at: Pos2,
    size: egui::Vec2,
    parent: Option<Rect>,
) -> Pos2 {
    // An off-screen test copy (MDM_OFFSCREEN) keeps its menus beside its window, off the
    // screens too (scale::work_area has none there): the nearest monitor's work area brought
    // them onto the screen, over the user's work.
    if let Some(area) = crate::scale::work_area(ctx, at) {
        let (l, t, r, b) = (area.min.x, area.min.y, area.max.x, area.max.y);
        let mut p = at;
        if crate::i18n::rtl() {
            p.x = at.x - size.x;
            if p.x < l {
                p.x = match parent {
                    Some(pr) => pr.right() - 4.0,
                    None => at.x,
                };
            }
        } else if p.x + size.x > r {
            p.x = match parent {
                Some(pr) => pr.left() - size.x + 4.0,
                None => at.x - size.x,
            };
        }
        if p.y + size.y > b {
            p.y = match parent {
                Some(_) => b - size.y,
                None => at.y - size.y,
            };
        }
        if crate::i18n::rtl() {
            p.x = p.x.min(r - size.x);
        }
        return pos2(p.x.max(l), p.y.max(t));
    }
    let _ = parent;
    if crate::i18n::rtl() {
        return pos2(at.x - size.x, at.y);
    }
    at
}

/// Whether the menu bar's menus share one window per level ([`menu_viewport`]): on X11 and
/// XWayland, where DDM places its menus itself.
pub(super) fn bar_shares_window() -> bool {
    #[cfg(target_os = "linux")]
    {
        !crate::unix::on_wayland()
    }
    // Windows and macOS: one window per menu, as before. Native Wayland (DDM_WAYLAND=1) too: a
    // program can't move its windows there (winit 0.30 has no popup windows on Wayland), so a
    // shared window would stay where the first menu opened.
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// The window menu `id`'s level `depth` is drawn in. With `bar_shared` (X11: [`bar_shares_window`])
/// every menu of the bar is drawn in one window per level, moved and resized as the pointer or
/// the arrow keys go from title to title, as a submenu's window already is from item to item.
/// A window of its own for each menu was mapped and unmapped at every switch, and a compositor
/// that animates new windows (Hyprland, picom with fading) faded each one in, and the last one
/// out, every time (#277). Other menus: a window of their own, as before.
pub(super) fn menu_viewport(
    kind: MenuKind,
    id: &str,
    depth: usize,
    bar_shared: bool,
) -> egui::ViewportId {
    let key = if bar_shared && kind == MenuKind::Bar {
        "menu-bar"
    } else {
        id
    };
    egui::ViewportId::from_hash_of(("menu", key, depth))
}

/// The entry under `p` in a menu drawn at `o`, `w` wide: its index, none on a separator or the
/// padding (the slots as `popup_level` lays them out).
pub(super) fn item_at<C>(entries: &[MenuEntry<C>], o: Pos2, w: f32, p: Pos2) -> Option<usize> {
    let mut y = o.y + 3.0;
    for (i, e) in entries.iter().enumerate() {
        if matches!(e, MenuEntry::Sep) {
            y += SEP;
            continue;
        }
        let slot = Rect::from_min_max(pos2(o.x + 6.0, y), pos2(o.x + w - 6.0, y + SLOT));
        if slot.contains(p) {
            return Some(i);
        }
        y += SLOT;
    }
    None
}

/// Whether the pointer at `p`, having left an open submenu's entry at `from`, is on its way into
/// the submenu at `sub` (#579): inside the triangle from `from` to the submenu's near edge, with
/// a pixel's slack. A pointer going up or down the menu instead is not.
pub(super) fn heading_into(from: Pos2, p: Pos2, sub: Rect) -> bool {
    let near = if sub.center().x >= from.x {
        sub.left()
    } else {
        sub.right()
    };
    let (a, b, c) = (from, pos2(near, sub.top()), pos2(near, sub.bottom()));
    let side = |p1: Pos2, p2: Pos2| (p2.x - p1.x) * (p.y - p1.y) - (p2.y - p1.y) * (p.x - p1.x);
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    let slack = 1.0 * (b - a).length().max(1.0);
    let neg = d1 < -slack || d2 < -slack || d3 < -slack;
    let pos = d1 > slack || d2 > slack || d3 > slack;
    !(neg && pos)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn popup_level<C: Clone>(
    ctx: &egui::Context,
    kind: MenuKind,
    id: &str,
    at: Pos2,
    parent: Option<Rect>,
    entries: &[MenuEntry<C>],
    sub_key: egui::Id,
    result: &mut Option<C>,
    depth: usize,
    hint: &dyn Fn(&C) -> Option<&'static str>,
) {
    let size = menu_size(ctx, entries, hint);
    let kb_key = sub_key.with("kb");
    let kb: Option<usize> = ctx
        .data(|d| d.get_temp::<(Option<usize>, Option<usize>)>(kb_key))
        .and_then(|(a, b)| if depth == 0 { a } else { b });
    let screen = Rect::from_min_size(place_menu(ctx, at, size, parent), size);
    if depth == 1 {
        // Where the submenu is, for its menu's next pass (`heading_into`, #579).
        ctx.data_mut(|d| d.insert_temp(sub_key.with("sub-rect"), screen));
    }
    let title = format!("dannexdownloadmanager menu {depth}"); // not translated: an id
    let builder = popup_viewport(&title)
        .with_active(false)
        .with_position(screen.min)
        .with_inner_size(size);
    // Linux: a menu window as X11 menus are (unix::menu_window).
    #[cfg(target_os = "linux")]
    let builder = crate::unix::menu_window(builder, kind);
    let open_sub = if depth == 0 {
        ctx.data(|d| d.get_temp::<usize>(sub_key))
    } else {
        None
    };
    // (index, slot top) of the submenu to show beside this menu.
    let mut sub_at: Option<(usize, f32)> = None;
    let vid = menu_viewport(kind, id, depth, bar_shares_window());
    let parent_vid = ctx.viewport_id();
    // Set when this pass made the new (hidden) window a menu window: shown below. (Elsewhere a
    // menu's window is created visible.)
    let mut styled = false;
    ctx.show_viewport_immediate(vid, builder, |ui, _| {
        // Made a menu window in its own pass, before anything is drawn in it (style_popup).
        styled = style_popup(&title, PopupLook::Menu);
        // What happens in this window, for popup() and keyboard(), which run in the parent's
        // pass: they must not ask egui about a menu window by its id, which creates one when it
        // isn't open (a submenu, mostly): an untitled copy of the main window, opened and closed
        // every frame (issues #4 and #5: fading windows; on a full-screen Mac, a new space).
        let (focused, keys) = ui.input(|i| (i.viewport().focused == Some(true), menu_keys(i)));
        ui.ctx().data_mut(|d| {
            if focused {
                d.insert_temp(sub_key.with("window-focus"), true);
            }
            if !keys.is_empty() {
                d.get_temp_mut_or_default::<Vec<egui::Key>>(sub_key.with("window-keys"))
                    .extend(keys.iter().copied());
            }
        });
        if !keys.is_empty() {
            // keyboard() takes them on the parent's next pass.
            ui.ctx().request_repaint_of(parent_vid);
        }
        let w = size.x;
        let painter = ui.painter().clone();
        let o = ui.max_rect().min;
        // Where the items are, for the tests that point at them.
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(sub_key.with(("origin", depth)), o));
        // Right to left: text right-aligned, check marks and bullets on the right, the submenu
        // arrow on the left pointing left.
        let _surface = winui::Surface::enter(o.x, o.x + w);
        let rtl = winui::rtl();
        // Body; Windows 11 rounds the window and draws the border and shadow (style_popup).
        fill(
            &painter,
            o.x,
            o.y,
            o.x + w - 1.0,
            o.y + size.y - 1.0,
            POPUP_BG(),
        );
        let pointer = ui.ctx().pointer_hover_pos();
        // The item under the pointer (none on a separator or the padding): one item is lit at a
        // time, so an open submenu's entry gives up its light to it (#579).
        let under = pointer.and_then(|p| item_at(entries, o, w, p));
        // An open submenu stays open while the pointer is on another item only as long as it
        // heads into the submenu (#579): from where it left the submenu's entry, inside the
        // triangle to the submenu's near edge. Its screen rect, from the pass that drew it, in
        // this window's points.
        let anchor_key = sub_key.with("anchor");
        let sub_rect = (depth == 0)
            .then(|| {
                ui.ctx()
                    .data(|d| d.get_temp::<Rect>(sub_key.with("sub-rect")))
            })
            .flatten()
            .map(|r| r.translate(o.to_vec2() - screen.min.to_vec2()));
        let clicked = ui.input(|i| i.pointer.primary_released());
        // The pointer moving over an item takes the highlight from the keyboard.
        let moving = ui.input(|i| i.pointer.is_moving());
        // For screen readers and UI Automation (#113): the window is a menu, its entries its
        // items (made in a child whose accessibility parent is the menu); invoking one chooses
        // it, as a click does.
        let menu_node = ui.interact(
            Rect::from_min_size(o, size),
            egui::Id::unique(("menu-node", id, depth)),
            egui::Sense::hover(),
        );
        winui::expose(&menu_node, egui::Role::Menu, "", true, None);
        let items_ui = ui.new_child(egui::UiBuilder::new().accessibility_parent(menu_node.id));
        let mut y = o.y + 3.0;
        for (i, e) in entries.iter().enumerate() {
            if matches!(e, MenuEntry::Sep) {
                winui::fill_l(
                    &painter,
                    o.x + 9.0,
                    y + 4.0,
                    o.x + w - 10.0,
                    y + 4.0,
                    POPUP_SEP(),
                );
                y += SEP;
                continue;
            }
            let (label, enabled, checked) = match e {
                MenuEntry::Item(l, _, en, ch) | MenuEntry::Radio(l, _, en, ch) => (*l, *en, *ch),
                MenuEntry::Sub(l, en, _) => (*l, *en, false),
                MenuEntry::Sep => unreachable!(),
            };
            let slot = Rect::from_min_max(pos2(o.x + 6.0, y), pos2(o.x + w - 6.0, y + SLOT));
            let hovered = pointer.is_some_and(|p| slot.contains(p));
            if hovered && moving && kb.is_some() && kb != Some(i) {
                ui.ctx().data_mut(|d| {
                    let (a, b) = d
                        .get_temp::<(Option<usize>, Option<usize>)>(kb_key)
                        .unwrap_or_default();
                    d.insert_temp(
                        kb_key,
                        if depth == 0 {
                            (Some(i), b)
                        } else {
                            (a, Some(i))
                        },
                    );
                });
            }
            let lit = match kb {
                Some(k) => k == i,
                None => hovered,
            };
            // An open submenu's entry stays lit while the pointer is on it, in the submenu or
            // off this menu; not while it is on another item, which is the lit one then (#579).
            let opened = open_sub == Some(i) && under.is_none_or(|u| u == i);
            if lit || opened {
                painter.rect_filled(
                    slot,
                    crate::theme::metrics().menu_item_radius,
                    POPUP_HOVER(),
                );
            } else {
                // Spec: an item's normal background is one level lighter than the padding.
                fill(
                    &painter,
                    slot.left(),
                    y,
                    slot.right() - 1.0,
                    y + SLOT - 1.0,
                    POPUP_ITEM(),
                );
            }
            let color = if enabled {
                TEXT()
            } else {
                TEXT_DISABLED_MENU()
            };
            // Bullets, check marks and arrows (light mode: dark cores, not pure black).
            let glyph = if enabled {
                GLYPH()
            } else {
                TEXT_DISABLED_MENU()
            };
            let (t, mn) = plain(crate::i18n::tc("menu", label));
            let keys = match e {
                MenuEntry::Item(_, c, ..) | MenuEntry::Radio(_, c, ..) => hint(c),
                _ => None,
            };
            let t = if keys.is_some() {
                without_shortcut(&t).to_owned()
            } else {
                t
            };
            // The entry's node: a menu item (a check or radio item when it is one), its name,
            // whether it can be used, and for a submenu that it opens one.
            let item = items_ui.interact(
                slot,
                egui::Id::unique(("menu-item", id, depth, i)),
                winui::pointer_only(),
            );
            let (role, on) = match e {
                MenuEntry::Radio(..) => (egui::Role::MenuItemRadio, Some(checked)),
                MenuEntry::Item(..) if checked => (egui::Role::MenuItemCheckBox, Some(true)),
                _ => (egui::Role::MenuItem, None),
            };
            winui::expose(&item, role, &t, enabled, on);
            if let MenuEntry::Sub(..) = e {
                let open = open_sub == Some(i);
                ui.ctx().accesskit_node_builder(item.id, |n| {
                    n.set_has_popup(egui::accesskit::HasPopup::Menu);
                    n.set_expanded(open);
                });
            }
            // Invoked by a screen reader or UI Automation (a real click is the pointer's,
            // below): an item is chosen, a submenu opens.
            // (A submenu's entry is opened with Expand too, its pattern being Expand / Collapse.)
            let expand_sub = matches!(e, MenuEntry::Sub(..))
                && ui.input(|x| {
                    x.has_accesskit_action_request(item.id, egui::accesskit::Action::Expand)
                });
            let invoked = (item.clicked() && !item.clicked_by(egui::PointerButton::Primary)
                || expand_sub)
                && enabled;
            match e {
                MenuEntry::Item(_, c, ..) | MenuEntry::Radio(_, c, ..) if invoked => {
                    *result = Some(c.clone());
                    ui.ctx().request_repaint_of(parent_vid);
                }
                MenuEntry::Sub(..) if invoked && depth == 0 => {
                    ui.ctx().data_mut(|d| d.insert_temp(sub_key, i));
                    sub_at = Some((i, y - o.y));
                    ui.ctx().request_repaint_of(parent_vid);
                }
                _ => {}
            }
            let drawn = text_font(&painter, winui::segoe(), o.x + 38.0, y + 14.0, &t, color);
            // The shortcut at the trailing edge, in the menu's quieter ink.
            if let Some(k) = keys {
                winui::text_right(
                    &painter,
                    o.x + w - 24.0,
                    y + 14.0,
                    &key_label(k),
                    TEXT_DISABLED_MENU(),
                );
            }
            // Underlined mnemonics: always in dark mode, once the keyboard is in use in light.
            if let Some(m) = mn
                && cues(ctx)
            {
                let (x0, x1) = winui::char_span(&painter, &winui::segoe(), &t, m, drawn);
                fill(&painter, x0, y + 16.0, x1, y + 16.0, color);
            }
            // The check box column: x 12..21 from the leading edge (the glyphs keep their
            // shape).
            let gx = |x: f32| if rtl { x + w - 33.0 } else { x };
            if checked && matches!(e, MenuEntry::Radio(..)) {
                // Measured 4×4 bullet at rel x 15..18, slot rows 9..12 (ink over #2c).
                const BULLET: [[u8; 4]; 4] = [
                    [0x54, 0xca, 0xd7, 0x54],
                    [0xd7, 0xff, 0xff, 0xca],
                    [0xca, 0xff, 0xff, 0xd7],
                    [0x54, 0xd7, 0xca, 0x54],
                ];
                for (dy, row) in BULLET.iter().enumerate() {
                    for (dx, v) in row.iter().enumerate() {
                        let a = ((*v as f32 - 44.0) / (255.0 - 44.0) * 255.0).round() as u8;
                        let c = Color32::from_rgba_unmultiplied(glyph.r(), glyph.g(), glyph.b(), a);
                        px(&painter, gx(o.x + 15.0 + dx as f32), y + 9.0 + dy as f32, c);
                    }
                }
            } else if checked {
                painter.line(
                    vec![
                        pos2(gx(o.x + 12.5), y + 11.5),
                        pos2(gx(o.x + 15.5), y + 14.5),
                        pos2(gx(o.x + 20.5), y + 8.5),
                    ],
                    Stroke::new(1.3, glyph),
                );
            }
            match e {
                MenuEntry::Sub(..) => {
                    // At the trailing edge, pointing where the submenu opens.
                    painter.line(
                        vec![
                            pos2(winui::mx(o.x + w - 20.5), y + 7.5),
                            pos2(winui::mx(o.x + w - 16.5), y + 11.5),
                            pos2(winui::mx(o.x + w - 20.5), y + 15.5),
                        ],
                        Stroke::new(1.2, glyph),
                    );
                    if depth == 0 && enabled && hovered && (kb.is_none() || moving) {
                        ui.ctx().data_mut(|d| d.insert_temp(sub_key, i));
                    }
                    if depth == 0
                        && hovered
                        && let Some(p) = pointer
                    {
                        ui.ctx().data_mut(|d| d.insert_temp(anchor_key, p));
                    }
                    if depth == 0 && (open_sub == Some(i) || (enabled && hovered && kb.is_none())) {
                        sub_at = Some((i, y - o.y));
                    }
                }
                MenuEntry::Item(_, c, ..) | MenuEntry::Radio(_, c, ..) => {
                    // The pointer on another item of the root closes an open submenu at once
                    // (#579), unless it is crossing the item on its way into the submenu
                    // (diagonally): then after Windows' menu delay, if it rests there.
                    let since_key = sub_key.with(("leave-since", i));
                    if depth == 0 && hovered && open_sub.is_some() {
                        let now = ui.input(|inp| inp.time);
                        let since = ui
                            .ctx()
                            .data_mut(|d| *d.get_temp_mut_or_insert_with(since_key, || now));
                        let anchor = ui.ctx().data(|d| d.get_temp::<Pos2>(anchor_key));
                        let heading = match (anchor, pointer, sub_rect) {
                            (Some(a), Some(p), Some(r)) => heading_into(a, p, r),
                            _ => false,
                        };
                        if !heading || now - since >= 0.35 {
                            ui.ctx().data_mut(|d| {
                                d.remove::<usize>(sub_key);
                                d.remove::<f64>(since_key);
                                d.remove::<Pos2>(anchor_key);
                                d.remove::<Rect>(sub_key.with("sub-rect"));
                            });
                            sub_at = None;
                        } else {
                            // The window that draws the menus (this one is drawn in its pass):
                            // a pointer resting on the item brings no event to close it by.
                            ui.ctx().request_repaint_after_for(
                                std::time::Duration::from_millis(60),
                                parent_vid,
                            );
                        }
                    } else if depth == 0 {
                        ui.ctx().data_mut(|d| d.remove::<f64>(since_key));
                    }
                    if hovered && clicked && enabled {
                        *result = Some(c.clone());
                    }
                }
                MenuEntry::Sep => {}
            }
            y += SLOT;
        }
    });
    if styled {
        show_popup(ctx, vid);
    }
    if let Some((i, slot_y)) = sub_at
        && let MenuEntry::Sub(_, _, children) = &entries[i]
    {
        // The submenu's top-left: parent right − 6, slot top − 3 (as measured); in RTL its
        // top-right at parent left + 6.
        let x = if crate::i18n::rtl() {
            screen.min.x + 6.0
        } else {
            screen.max.x - 6.0
        };
        let at = pos2(x, screen.min.y + slot_y - 3.0);
        popup_level(
            ctx,
            kind,
            id,
            at,
            Some(screen),
            children,
            sub_key,
            result,
            depth + 1,
            hint,
        );
    }
}
