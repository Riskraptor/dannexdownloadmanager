#![cfg_attr(not(windows), allow(dead_code))]
use super::*;

crate::theme::themed! {
    /// A title bar we draw ourselves (Windows 11's caption, both themes).
    pub CAPTION_BG => caption;
    pub CAPTION_INK => caption_text;
    /// A caption button's hover and pressed tints over the bar: WinUI's caption tokens, as
    /// Windows Terminal ships them (#0FFFFFFF / #09000000, #0AFFFFFF / #06000000).
    pub CAPTION_HOVER => caption_hot;
    pub CAPTION_PRESSED => caption_pressed;
    /// A button at rest, where the style shows one (GNOME's round buttons).
    CAPTION_BUTTON => caption_button;
    /// Close's hover red, and its glyph on it.
    CLOSE_HOVER => caption_close;
    CLOSE_INK => caption_close_text;
}
/// The title and glyphs while the window isn't the active one, and a greyed button's: the
/// one disabled ink of every control.
fn caption_ink_inactive() -> Color32 {
    crate::theme::colors().caption_text_inactive
}
/// Close's hover red with a white glyph; pressed, the red at 0.9 and the glyph at 0.7
/// (WinUI's, both themes).
fn close_pressed() -> Color32 {
    CLOSE_HOVER().gamma_multiply(0.9)
}

/// A drawn title bar's height and one caption button's width, in points (Windows 11's).
pub const CAPTION_H: f32 = 32.0;
pub const CAPTION_BUTTON_W: f32 = 46.0;
/// Where the window's icon ends: the system menu's place in the title bar.
pub const CAPTION_ICON_W: f32 = 32.0;

/// How the theme's title bar is laid out (`palette::CaptionLook`).
#[derive(Clone, Copy)]
struct Style {
    /// One button's width (Close's its own: Windows 7's is wider), and the room after the
    /// last one at the window's end.
    button_w: f32,
    close_w: f32,
    end: f32,
    /// GNOME's round buttons (a circle this wide, tinted at rest), else Windows' tiles the
    /// bar's height.
    circle: Option<f32>,
    /// The window's icon at the start (the system menu's place).
    icon: bool,
    /// The title centred in the bar (else after the icon), and in bold.
    centred: bool,
    bold: bool,
    /// A button under the pointer or pressed draws its glyph in the bar's colour (KDE).
    invert: bool,
    /// The era themes (#45): the bar and its buttons shaded in the era's way, each button
    /// a tile of this size (a zero width: its place's) with a face of its own at rest, hung
    /// from the bar's top (`hang`, Windows 7) or centred in it.
    era: Option<Shine>,
    tile: egui::Vec2,
    hang: bool,
}

fn style() -> Style {
    use crate::palette::CaptionLook;
    let windows = Style {
        button_w: CAPTION_BUTTON_W,
        close_w: CAPTION_BUTTON_W,
        end: 0.0,
        circle: None,
        icon: true,
        centred: false,
        bold: false,
        invert: false,
        era: None,
        tile: egui::Vec2::ZERO,
        hang: false,
    };
    match crate::theme::current().controls.caption {
        CaptionLook::System => windows,
        CaptionLook::Metro => Style {
            centred: true,
            ..windows
        },
        // libadwaita's header bar: 24-point round buttons 6 apart, no icon, a bold title.
        CaptionLook::Adwaita => Style {
            button_w: 30.0,
            close_w: 30.0,
            end: 4.0,
            circle: Some(24.0),
            icon: false,
            centred: true,
            bold: true,
            ..windows
        },
        // KDE's Breeze: the icon, the title centred, 18-point round buttons shown only
        // under the pointer, in the title's colour with the glyph in the bar's.
        CaptionLook::Breeze => Style {
            button_w: 26.0,
            close_w: 26.0,
            end: 4.0,
            circle: Some(18.0),
            centred: true,
            invert: true,
            ..windows
        },
        // Windows XP: the icon and a bold title, 21-point square buttons with a light edge.
        CaptionLook::Luna => Style {
            button_w: 23.0,
            close_w: 23.0,
            end: 5.0,
            bold: true,
            era: Some(Shine::Luna),
            tile: vec2(21.0, 21.0),
            ..windows
        },
        // Windows 7: the icon and the title, buttons 20 points high hung from the bar's
        // top, side by side, Close wider.
        CaptionLook::Aero => Style {
            button_w: 27.0,
            close_w: 46.0,
            end: 6.0,
            era: Some(Shine::Aero),
            tile: vec2(0.0, 20.0),
            hang: true,
            ..windows
        },
        // Mac OS X: no icon, the title centred, round 15-point buttons.
        CaptionLook::Aqua => Style {
            button_w: 23.0,
            close_w: 23.0,
            end: 8.0,
            icon: false,
            centred: true,
            era: Some(Shine::Aqua),
            tile: vec2(15.0, 15.0),
            ..windows
        },
    }
}

/// The bar's colour: the active one, or the theme's inactive one while the window isn't
/// the active one (where it has one).
fn bar_colour(active: bool) -> Color32 {
    let inactive = crate::theme::colors().caption_inactive;
    if !active && inactive.a() > 0 {
        inactive
    } else {
        CAPTION_BG()
    }
}

/// An era's bar, shaded from its top: XP's light edge on a deep colour, Windows 7's glassy
/// halves, Mac OS X's light grey falling to the bar's colour.
fn bar_stops(era: Shine, bar: Color32, g: Color32) -> Vec<(f32, Color32)> {
    use crate::derive::{mix, shade};
    match era {
        Shine::Luna => vec![
            (0.0, mix(bar, g, 0.45)),
            (0.12, mix(bar, g, 0.12)),
            (0.5, bar),
            (1.0, shade(bar, -0.06)),
        ],
        Shine::Aero => vec![
            (0.0, mix(bar, g, 0.45)),
            (0.5, mix(bar, g, 0.2)),
            (0.5, bar),
            (1.0, mix(bar, g, 0.1)),
        ],
        Shine::Aqua => vec![(0.0, mix(bar, g, 0.6)), (1.0, bar)],
    }
}

/// A caption button of a title bar we draw ourselves, from the window's end inwards: Close,
/// Maximize, Minimize, then ours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionButton {
    Close,
    Maximize,
    Minimize,
    /// Hide the window in the notification area (#63).
    ToTray,
}

impl CaptionButton {
    /// Its accessible name, English (translated where shown).
    fn name(self) -> &'static str {
        match self {
            // i18n: a window's title-bar button: closes the window
            CaptionButton::Close => crate::i18n::mark("Close"),
            // i18n: a window's title-bar button: makes the window fill the screen
            CaptionButton::Maximize => crate::i18n::mark("Maximize"),
            // i18n: a window's title-bar button: minimizes the window
            CaptionButton::Minimize => crate::i18n::mark("Minimize"),
            // i18n: tooltip of a button in a download's progress window: hides the window to the notification area (system tray); the download goes on
            CaptionButton::ToTray => crate::i18n::mark("Hide to the notification area"),
        }
    }

    /// The theme icon set's name for its glyph (`crate::themeicons`).
    fn need(self, maximized: bool) -> &'static str {
        match self {
            CaptionButton::Close => "ui-close",
            CaptionButton::Maximize if maximized => "ui-restore",
            CaptionButton::Maximize => "ui-maximize",
            CaptionButton::Minimize => "ui-minimize",
            CaptionButton::ToTray => "ui-to-tray",
        }
    }

    /// Its glyph: the theme's icon set's where it has one, else Segoe Fluent's shapes drawn
    /// as our own vectors, 10 px across and 1 px wide at `size` 1 and `width` 1 (Restore's
    /// two squares while maximized).
    #[allow(clippy::too_many_arguments)]
    fn glyph(
        self,
        ui: &Ui,
        p: &Painter,
        c: Pos2,
        ink: Color32,
        maximized: bool,
        size: f32,
        width: f32,
    ) {
        let px = 16.0 * size * ui.ctx().pixels_per_point();
        if let Some((src, tint)) = crate::themeicons::source(self.need(maximized), px, ink, true) {
            let rc = Rect::from_center_size(c, vec2(16.0, 16.0) * size);
            let img = egui::Image::new(src);
            if let Ok(egui::load::TexturePoll::Ready { texture }) =
                img.load_for_size(ui.ctx(), rc.size())
            {
                p.image(
                    texture.id,
                    rc,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    tint,
                );
                return;
            }
        }
        let s = Stroke::new(width, ink);
        let v = |x: f32, y: f32| c + vec2(x, y) * size;
        if self == CaptionButton::Maximize && maximized {
            p.rect_stroke(
                Rect::from_center_size(v(-1.0, 1.0), vec2(8.0, 8.0) * size),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
            p.line(
                vec![
                    v(-1.5, -3.5),
                    v(-1.5, -4.5),
                    v(4.5, -4.5),
                    v(4.5, 1.5),
                    v(3.5, 1.5),
                ],
                s,
            );
            return;
        }
        match self {
            CaptionButton::Minimize => {
                p.line_segment([v(-5.0, 0.5), v(5.0, 0.5)], s);
            }
            CaptionButton::Maximize => {
                p.rect_stroke(
                    Rect::from_center_size(c, vec2(9.0, 9.0) * size),
                    1.0,
                    s,
                    egui::StrokeKind::Inside,
                );
            }
            CaptionButton::Close => {
                p.line_segment([v(-5.0, -5.0), v(5.0, 5.0)], s);
                p.line_segment([v(-5.0, 5.0), v(5.0, -5.0)], s);
            }
            CaptionButton::ToTray => to_tray_glyph_sized(p, c, s, rtl(), size),
        }
    }
}

/// Windows keeps the window's own title bar in UI Automation (its "TitleBar", built from the
/// window's styles, whatever is drawn): its Minimize, Maximize and Close are spoken, and work,
/// through the system. Ours would say them a second time, so they stay out of the
/// accessibility tree there; only our own button (Hide) is exposed. Brave and VS Code have no
/// such TitleBar only because Chromium's own provider replaces the window's; AccessKit's
/// doesn't, and the system menu, Alt+Space and taskbar minimize need the styles kept.
const SYSTEM_SPEAKS: bool = cfg!(windows);

thread_local! {
    /// The next title bar's Maximize as the window's frame sees it (hot, pressed): where the
    /// frame answers for it (Snap Layouts, win32::drawn_frame), egui never sees the pointer
    /// there.
    static FRAME_MAX: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

/// Tell the next [`title_bar`] how its window's frame sees Maximize: under the pointer,
/// pressed.
pub fn frame_maximize(hot: bool, pressed: bool) {
    FRAME_MAX.with(|c| c.set((hot, pressed)));
}

thread_local! {
    /// The next title bar's Close tooltip, where its × does more than close the window (a
    /// progress window's stops its download): English, translated where shown.
    static CLOSE_TIP: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}

/// Give the next [`title_bar`]'s × a tooltip `tip` (English, a marked text): what it does
/// besides closing the window. The system's own buttons say nothing more than "Close".
pub fn close_tip(tip: &'static str) {
    CLOSE_TIP.with(|c| c.set(Some(tip)));
}

/// Keep `resp`'s node out of the accessibility tree (screen readers hear the system's).
fn unspoken(ui: &Ui, resp: &Response) {
    ui.ctx().accesskit_node_builder(resp.id, |n| n.set_hidden());
}

/// What a caption button's tooltip says: ours (Hide) says the download goes on; Close says
/// what its window gave (`close_tip`: a progress window's × stops its download); the others
/// none, as the system's.
fn caption_tip(b: CaptionButton, close_tip: Option<&'static str>) -> Option<&'static str> {
    match b {
        // i18n: tooltip of the title-bar button of a download's progress window that hides it to the notification area (system tray); the download goes on
        CaptionButton::ToTray => Some(crate::i18n::mark(
            "Hide to the notification area (the download goes on)",
        )),
        CaptionButton::Close => close_tip,
        _ => None,
    }
}

/// One caption button: Windows 11's 46 × 32, a hover and a pressed tint (Close turns red, its
/// glyph white), greyed when `enabled` is false, its glyph grey while the window is inactive.
/// Exposed as a button with its name; a tooltip for ours, and for a Close that does more than
/// close (`close_tip`): the system's have none.
fn caption_button(
    ui: &mut Ui,
    rc: Rect,
    b: CaptionButton,
    enabled: bool,
    active: bool,
    close_tip: Option<&'static str>,
) -> bool {
    let tip = caption_tip(b, close_tip).map(crate::i18n::t);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rc, widget_id(ui, ("caption-button", b.name())), sense);
    let name = crate::i18n::t(b.name());
    if b == CaptionButton::ToTray || !SYSTEM_SPEAKS {
        expose(&resp, egui::Role::Button, name, enabled, None);
    } else {
        unspoken(ui, &resp);
    }
    let p = ui.painter();
    let (frame_hot, frame_pressed) = if b == CaptionButton::Maximize {
        FRAME_MAX.with(|c| c.get())
    } else {
        (false, false)
    };
    let hover = enabled && (resp.hovered() || frame_hot);
    let pressed = enabled && (resp.is_pointer_button_down_on() || frame_pressed);
    let close = b == CaptionButton::Close;
    let st = style();
    let maximized = ui.ctx().input(|i| i.viewport().maximized) == Some(true);
    if let Some(era) = st.era {
        era_button(
            ui,
            &resp,
            rc,
            b,
            era,
            (hover, pressed, enabled, active),
            maximized,
        );
        let clicked = resp.clicked();
        if let Some(tip) = tip {
            resp.on_hover_text(tip);
        }
        return clicked;
    }
    let circle = st.circle;
    let fill = match (pressed, hover, close) {
        (true, _, true) => Some(close_pressed()),
        (false, true, true) => Some(CLOSE_HOVER()),
        (true, _, false) => Some(CAPTION_PRESSED()),
        (false, true, false) => Some(CAPTION_HOVER()),
        // GNOME's round buttons show at rest too.
        _ if circle.is_some() && enabled => Some(CAPTION_BUTTON()),
        _ => None,
    };
    if let Some(f) = fill {
        match circle {
            Some(d) => {
                p.circle_filled(rc.center(), d / 2.0, f);
            }
            None => {
                p.rect_filled(rc, 0.0, f);
            }
        }
    }
    let ink = if st.invert && (hover || pressed) {
        CAPTION_BG()
    } else if close && pressed {
        CLOSE_INK().gamma_multiply(0.7)
    } else if close && hover {
        CLOSE_INK()
    } else if !enabled || !active {
        caption_ink_inactive()
    } else {
        CAPTION_INK()
    };
    b.glyph(ui, p, rc.center().round(), ink, maximized, 1.0, 1.0);
    let ring = match circle {
        Some(d) => Rect::from_center_size(rc.center(), vec2(d, d)),
        None => rc.shrink(3.0),
    };
    show_focus(ui, &resp, ring);
    let clicked = resp.clicked();
    if let Some(tip) = tip {
        resp.on_hover_text(tip);
    }
    clicked
}

/// An era theme's caption button (#45) in its place `rc`: a shaded tile with an edge, its
/// own face at rest (Close red; Mac OS X's graphite until the pointer is on it), lighter
/// under the pointer and darker pressed, faded while greyed or the window inactive; the
/// glyph light on it (Windows 7's with a shadow), or Mac OS X's dark one shown only under
/// the pointer. There, our Hide button is a plain picture in the bar, as a toolbar's.
fn era_button(
    ui: &Ui,
    resp: &Response,
    rc: Rect,
    b: CaptionButton,
    era: Shine,
    (hover, pressed, enabled, active): (bool, bool, bool, bool),
    maximized: bool,
) {
    use crate::derive::{mix, shade};
    let p = ui.painter();
    let c = crate::theme::colors();
    let st = style();
    let close = b == CaptionButton::Close;
    let bar = bar_colour(active);
    let at = rc.center().round();
    if era == Shine::Aqua && b == CaptionButton::ToTray {
        if hover || pressed {
            let tint = if pressed {
                CAPTION_PRESSED()
            } else {
                CAPTION_HOVER()
            };
            p.rect_filled(Rect::from_center_size(at, vec2(22.0, 22.0)), 4.0, tint);
        }
        let ink = if enabled && active {
            CAPTION_INK()
        } else {
            caption_ink_inactive()
        };
        b.glyph(ui, p, at, ink, maximized, 1.0, 1.0);
        show_focus(ui, resp, Rect::from_center_size(at, vec2(22.0, 22.0)));
        return;
    }
    let size = vec2(
        if st.tile.x > 0.0 {
            st.tile.x
        } else {
            rc.width()
        },
        st.tile.y,
    );
    let tile = if st.hang {
        Rect::from_min_size(pos2(at.x - size.x / 2.0, rc.top()), size)
    } else {
        Rect::from_center_size(at, size)
    };
    let base = if close && (era != Shine::Aqua || hover || pressed) {
        CLOSE_HOVER()
    } else {
        CAPTION_BUTTON()
    };
    let base = if pressed {
        shade(base, -0.1)
    } else if hover {
        mix(base, c.gloss, 0.3)
    } else {
        base
    };
    let base = if enabled && active {
        base
    } else {
        mix(base, bar, 0.55)
    };
    let (top, bottom) = match era {
        Shine::Luna => (3.0, 3.0),
        Shine::Aero => (0.0, 4.0),
        Shine::Aqua => (size.y / 2.0, size.y / 2.0),
    };
    fill_shaded_corners(p, tile, top, bottom, &era.stops(base, c.gloss));
    let edge = match era {
        Shine::Luna => c.gloss.gamma_multiply(0.9),
        Shine::Aero => shade(bar, -0.35),
        Shine::Aqua => shade(base, -0.2),
    };
    let corners = egui::CornerRadius {
        nw: top as u8,
        ne: top as u8,
        sw: bottom as u8,
        se: bottom as u8,
    };
    p.rect_stroke(tile, corners, Stroke::new(1.0, edge), StrokeKind::Inside);
    let (size, width) = match era {
        Shine::Luna => (0.8, 2.0),
        Shine::Aero => (0.9, 1.5),
        Shine::Aqua => (0.55, 1.4),
    };
    let at = tile.center().round();
    if era == Shine::Aqua {
        // Mac OS X shows its buttons' glyphs only under the pointer.
        if hover || pressed {
            b.glyph(ui, p, at, CAPTION_INK(), maximized, size, width);
        }
    } else {
        let ink = if enabled && active {
            CLOSE_INK()
        } else {
            CLOSE_INK().gamma_multiply(0.7)
        };
        if era == Shine::Aero {
            let shadow = Color32::from_black_alpha(110);
            b.glyph(ui, p, at + vec2(1.0, 1.0), shadow, maximized, size, width);
        }
        b.glyph(ui, p, at, ink, maximized, size, width);
    }
    show_focus(ui, resp, tile);
}

/// What a drawn title bar did: the button pressed, and where its buttons and its icon are (in
/// points), for the window's hit test (win32::set_caption_hits).
pub struct TitleBar {
    pub pressed: Option<CaptionButton>,
    pub buttons: Vec<Rect>,
    pub icon: Rect,
    /// The colour along the bar's top edge (the sizing frame above the main window's bar
    /// takes it: win32::caption_colour). Only Windows draws title bars (elsewhere unread).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub top: Color32,
}

/// A title bar we draw ourselves (where the window's frame is ours: win32::thin_frame): the
/// window's icon and `title`, then `buttons` (each with its enabled state) from the window's end
/// inwards, mirrored right to left. The title is exposed as the bar's label. The rest of the bar
/// is the caption the system drags, snaps and opens the system menu from (win32's hit test):
/// egui only gets the buttons.
pub fn title_bar(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    buttons: &[(CaptionButton, bool)],
) -> TitleBar {
    // MDM_TEST_ACTIVE: every bar drawn as the active window's (pictures of windows off the
    // screens, which never have the focus).
    static ALWAYS_ACTIVE: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var_os("MDM_TEST_ACTIVE").is_some());
    let active = *ALWAYS_ACTIVE || ui.ctx().input(|i| i.viewport().focused) != Some(false);
    let rtl = rtl();
    let p = ui.painter().clone();
    let st = style();
    let bar = bar_colour(active);
    let top = match st.era {
        Some(era) => {
            let stops = bar_stops(era, bar, crate::theme::colors().gloss);
            fill_shaded(&p, rect, 0.0, &stops);
            stops.first().map_or(bar, |s| s.1)
        }
        None => {
            p.rect_filled(rect, 0.0, bar);
            bar
        }
    };
    let mut pressed = None;
    let mut rects = Vec::with_capacity(buttons.len());
    // This bar's own Close tooltip, if its window gave one (`close_tip`): for this bar only.
    let close_tip = CLOSE_TIP.with(std::cell::Cell::take);
    // From the window's end inwards, each as wide as its kind is.
    let mut from_end = st.end;
    for (b, enabled) in buttons {
        let bw = if *b == CaptionButton::Close {
            st.close_w
        } else {
            st.button_w
        };
        let x0 = if rtl {
            rect.left() + from_end
        } else {
            rect.right() - from_end - bw
        };
        from_end += bw;
        let rc = Rect::from_min_size(pos2(x0, rect.top()), vec2(bw, rect.height()));
        let tip = close_tip.filter(|_| *b == CaptionButton::Close);
        if caption_button(ui, rc, *b, *enabled, active, tip) {
            pressed = Some(*b);
        }
        rects.push(rc);
    }
    // The frame's word on Maximize holds for this bar only.
    FRAME_MAX.with(|c| c.set((false, false)));
    let buttons_w = from_end;
    // Without an icon, the title keeps a little room from the bar's start.
    let icon_w = if st.icon { CAPTION_ICON_W } else { 12.0 };
    if st.icon {
        let icon_c = if rtl {
            pos2(rect.right() - 16.0, rect.center().y)
        } else {
            pos2(rect.left() + 16.0, rect.center().y)
        };
        // The 16-pixel drawing at 100 % (not the 32 scaled down: a blur), 24 at 150 %.
        egui::Image::new(crate::logo::source(16.0 * ui.ctx().pixels_per_point()))
            .paint_at(ui, Rect::from_center_size(icon_c.round(), vec2(16.0, 16.0)));
    }
    let font = if st.bold {
        FontId::new(segoe().size, egui::FontFamily::Name("tahoma-bold".into()))
    } else {
        segoe()
    };
    let room = rect.width() - icon_w - buttons_w - 12.0;
    let shown = ellipsize(&p, &font, title, room.max(0.0));
    let ink = if active {
        CAPTION_INK()
    } else {
        caption_ink_inactive()
    };
    let (anchor, x) = if rtl {
        (egui::Align2::RIGHT_CENTER, rect.right() - icon_w)
    } else {
        (egui::Align2::LEFT_CENTER, rect.left() + icon_w)
    };
    // Windows 8 and GNOME centre the title in the bar (kept clear of the icon and the
    // buttons).
    let (anchor, x) = if st.centred {
        let half = text_width_font(&p, font.clone(), &shown) / 2.0;
        let (lo, hi) = if rtl {
            (rect.left() + buttons_w + half, rect.right() - icon_w - half)
        } else {
            (rect.left() + icon_w + half, rect.right() - buttons_w - half)
        };
        (
            egui::Align2::CENTER_CENTER,
            rect.center().x.clamp(lo.min(hi), hi.max(lo)),
        )
    } else {
        (anchor, x)
    };
    let galley = p.layout_no_wrap(shown.clone(), font.clone(), ink);
    let drawn = anchor.anchor_size(pos2(x, rect.center().y), galley.size());
    let c = crate::theme::colors();
    match st.era {
        // Windows XP: a dark shadow under the title.
        Some(Shine::Luna) => {
            let shadow = crate::derive::shade(bar, -0.25);
            let g = p.layout_no_wrap(shown.clone(), font.clone(), shadow);
            p.galley(drawn.min + vec2(1.0, 1.0), g, shadow);
        }
        // Windows 7: the title on a soft glow (the light variant's dark title).
        Some(Shine::Aero) if !c.dark => {
            let glow = egui::epaint::Shadow {
                offset: [0, 0],
                blur: 14,
                spread: 1,
                color: c.gloss.gamma_multiply(0.75),
            };
            p.add(glow.as_shape(drawn.expand2(vec2(6.0, 0.0)), 8));
        }
        // Mac OS X: a light line under the title, as if pressed into the bar.
        Some(Shine::Aqua) if !c.dark => {
            let light = c.gloss.gamma_multiply(0.7);
            let g = p.layout_no_wrap(shown.clone(), font.clone(), light);
            p.galley(drawn.min + vec2(0.0, 1.0), g, light);
        }
        _ => {}
    }
    p.galley(drawn.min, galley, ink);
    let label = ui.interact(drawn, widget_id(ui, "caption-title"), Sense::hover());
    // The window's own name says the title to screen readers (where the system speaks for
    // the title bar); elsewhere the label does.
    if SYSTEM_SPEAKS {
        unspoken(ui, &label);
    } else {
        expose(&label, egui::Role::Label, title, true, None);
    }
    if shown != title {
        label.on_hover_text(title);
    }
    // Without an icon, no system-menu place (Alt+Space and a right-click on the bar still
    // open the system menu).
    let icon = if !st.icon {
        Rect::from_min_size(rect.min, egui::Vec2::ZERO)
    } else if rtl {
        Rect::from_min_max(pos2(rect.right() - CAPTION_ICON_W, rect.top()), rect.max)
    } else {
        Rect::from_min_size(rect.min, vec2(CAPTION_ICON_W, rect.height()))
    };
    TitleBar {
        pressed,
        buttons: rects,
        icon,
        top,
    }
}
