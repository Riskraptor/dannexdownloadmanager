//! View → Theme → Choose a theme... (#45, beyond IDM's toolbar styles): every theme as a live
//! picture of the main window, its light side and its dark side side by side, in the accent
//! chosen for it; a click makes it DDM's at once. Under them the look's other choices, each
//! applied as it is made: light, dark or as the system is; the accent colour (each theme keeps
//! its own); the toolbar's icons (each theme keeps its own); the list's text size and density;
//! the interface size; and importing or exporting a theme file.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::dialogs::{Outcome, window};
use crate::icons::ToolbarIcons;
use crate::list::FontSize;
use crate::look::{Density, ThemeLook};
use crate::palette::{CaptionLook, IconStyle, Palette, Theme};
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, r, text};

pub const ID: &str = "theme-picker";

const W: f32 = 760.0;
const H: f32 = 552.0;
const M: f32 = 12.0;
/// The cards: five a row, two rows in view (more scroll).
const COLS: usize = 5;
const CARD_W: f32 = 136.0;
const PIC_H: f32 = 84.0;
const CARD_H: f32 = PIC_H + 24.0;
const GAP: f32 = 10.0;
const GRID_Y: f32 = 12.0;
const GRID_ROWS: usize = 2;
const GROUP_Y: f32 = GRID_Y + GRID_ROWS as f32 * (CARD_H + GAP) + 6.0;
const GROUP_H: f32 = 206.0;
const BUTTONS_Y: f32 = GROUP_Y + GROUP_H + 14.0;

/// Light, dark, or as the system is (View → Follow the system's dark mode, Always dark).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
    System,
}

/// What the window shows of the settings now.
pub struct State<'a> {
    pub theme: &'a str,
    pub mode: Mode,
    pub looks: &'a [ThemeLook],
    /// The toolbar's icons in use, and whether the theme has its own.
    pub toolbar: ToolbarIcons,
    pub own_icons: bool,
    pub font: FontSize,
    pub density: Density,
    /// View → Interface size, percent.
    pub size: u16,
}

/// What was chosen, for the main window to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Theme(&'static str),
    Mode(Mode),
    /// The theme's accent colour (None: its own).
    Accent(Option<Color32>),
    Toolbar(ToolbarIcons),
    Font(FontSize),
    Density(Density),
    Size(u16),
    Import,
    Export,
    Folder,
}

/// A card's two sides as last drawn, and the accent they were drawn in.
type Drawn = (Option<Color32>, Box<(Palette, Palette)>);

/// A theme's card: its id and name, the theme (a file's once read), and its two sides as last
/// drawn in an accent (made again when the accent changes).
struct Card {
    id: &'static str,
    name: String,
    theme: Option<Theme>,
    drawn: Option<Drawn>,
}

pub struct Picker {
    cards: Vec<Card>,
    /// Theme files being read beside the window (#743): each card's theme as it comes.
    reading: Option<std::sync::mpsc::Receiver<(usize, Theme)>>,
    /// The first row of cards in view.
    top: f32,
    /// The custom accent's box, and the theme it was filled for.
    hex: String,
    hex_for: Option<(String, Option<Color32>)>,
}

impl Picker {
    /// The built-in themes and those in the themes folder (`files`, View → Theme's list).
    pub fn new(ctx: &egui::Context, files: &[crate::themefile::Entry]) -> Picker {
        let mut cards: Vec<Card> = crate::palette::builtins()
            .into_iter()
            .map(|t| Card {
                id: crate::chrome::label(&t.id),
                name: shown_name(&t.id, &t.name),
                theme: Some(t.clone()),
                drawn: None,
            })
            .collect();
        // Native after Classic, as View → Theme lists it.
        let native = crate::native::theme(&crate::native::system(ctx));
        cards.insert(
            1,
            Card {
                id: "native",
                name: shown_name("native", "Native"),
                theme: Some(native),
                drawn: None,
            },
        );
        let first_file = cards.len();
        let mut paths = Vec::new();
        for e in files.iter().filter(|e| e.path.is_some()) {
            paths.push(e.id.clone());
            cards.push(Card {
                id: crate::chrome::label(&e.id),
                name: e.name.clone(),
                theme: None,
                drawn: None,
            });
        }
        let reading = (!paths.is_empty()).then(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let wake = ctx.clone();
            crate::launch::spawn("theme picker", move || {
                for (k, id) in paths.into_iter().enumerate() {
                    if let Ok(loaded) = crate::themefile::load(&id) {
                        let _ = tx.send((first_file + k, loaded.theme));
                        wake.request_repaint();
                    }
                }
            });
            rx
        });
        Picker {
            cards,
            reading,
            top: 0.0,
            hex: String::new(),
            hex_for: None,
        }
    }

    /// The window; false once it is closed. What was chosen goes in `acts`.
    pub fn show(&mut self, ctx: &egui::Context, st: &State, acts: &mut Vec<Act>) -> bool {
        if let Some(rx) = &self.reading {
            while let Ok((k, theme)) = rx.try_recv() {
                if let Some(c) = self.cards.get_mut(k) {
                    c.theme = Some(theme);
                    c.drawn = None;
                }
            }
        }
        let out = window(
            ctx,
            ID,
            // i18n: title of the theme picker (View → Theme → Choose a theme...)
            "Choose a theme",
            [W, H],
            true,
            LIGHT_BG(),
            |ui, o| self.body(ui, o, st, acts),
        );
        matches!(out, Outcome::Open)
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2, st: &State, acts: &mut Vec<Act>) -> Outcome<()> {
        self.grid(ui, o, st, acts);
        self.appearance(ui, o, st, acts);
        layout(ui, o, st, acts);
        let p = ui.painter().clone();
        let left: [(&str, Act); 3] = [
            // i18n: theme picker button: picks a theme file (or a Base16 or pywal colour scheme) and uses it
            (crate::i18n::mark("Import a theme…"), Act::Import),
            // i18n: theme picker button: saves the theme in use, its accent included, as a theme file
            (crate::i18n::mark("Export this theme…"), Act::Export),
            // i18n: theme picker button: opens the folder where theme files go
            (crate::i18n::mark("Open themes folder"), Act::Folder),
        ];
        let mut x = M;
        for (label, act) in left {
            let w = winui::button_width(&p, label, 73.0);
            if winui::button_light(
                ui,
                r(o, x, BUTTONS_Y, w, 23.0),
                label,
                ButtonKind::Normal,
                false,
            )
            .clicked()
            {
                acts.push(act);
            }
            x += w + 8.0;
        }
        let close = winui::buttons_light_right(
            ui,
            o,
            W - M,
            BUTTONS_Y,
            &[(crate::i18n::mark("Close"), ButtonKind::Default)],
        );
        if close[0].clicked() || winui::escape_pressed(ui) {
            Outcome::Closed
        } else {
            Outcome::Open
        }
    }

    /// The cards, five a row: each theme's picture and name; the one in use framed in the
    /// accent and ticked.
    fn grid(&mut self, ui: &mut Ui, o: Pos2, st: &State, acts: &mut Vec<Act>) {
        let p = ui.painter().clone();
        let rows = self.cards.len().div_ceil(COLS);
        let step = CARD_H + GAP;
        let area = Rect::from_min_size(
            o + vec2(M, GRID_Y),
            vec2(W - 2.0 * M, GRID_ROWS as f32 * step - GAP),
        );
        // More rows than fit: the wheel and a scroll bar move them a row at a time.
        let scrolls = rows > GRID_ROWS;
        if scrolls {
            if ui.rect_contains_pointer(winui::rect(area)) {
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                self.top = (self.top - dy / step).clamp(0.0, (rows - GRID_ROWS) as f32);
            }
            let track = Rect::from_min_max(
                pos2(area.right() - crate::scrollbar::THICK, area.top()),
                area.max,
            );
            crate::scrollbar::bar(
                ui,
                ui.scope_id().with("theme-grid-bar"),
                winui::rect(track),
                true,
                rows as f32,
                GRID_ROWS as f32,
                1.0,
                &mut self.top,
            );
        }
        let first = self.top.round() as usize;
        let clip = p.with_clip_rect(winui::rect(area));
        let accent_now = crate::theme::colors().accent;
        for (k, card) in self.cards.iter_mut().enumerate() {
            let (row, col) = (k / COLS, k % COLS);
            if row < first || row >= first + GRID_ROWS {
                continue;
            }
            let x = area.left()
                + col as f32
                    * (CARD_W + (area.width() - 22.0 - COLS as f32 * CARD_W) / (COLS - 1) as f32);
            let y = area.top() + (row - first) as f32 * step;
            let pic = Rect::from_min_size(pos2(x, y), vec2(CARD_W, PIC_H));
            let whole = Rect::from_min_size(pos2(x, y), vec2(CARD_W, CARD_H));
            let resp = ui.interact(
                winui::rect(whole),
                ui.scope_id().with(("theme-card", k)),
                Sense::click(),
            );
            let chosen = st.theme == card.id;
            // Its picture, in the accent chosen for it.
            let accent = crate::look::accent_of(st.looks, card.id);
            if let Some(theme) = &card.theme {
                if card.drawn.as_ref().is_none_or(|(a, _)| *a != accent) {
                    let made = match accent {
                        Some(a) => crate::look::with_accent(theme, a),
                        None => theme.clone(),
                    };
                    card.drawn = Some((accent, Box::new((made.light, made.dark))));
                }
                let (light, dark) = &*card.drawn.as_ref().expect("made above").1;
                let screen = winui::rect(pic);
                let half = screen.width() / 2.0;
                let left = Rect::from_min_size(screen.min, vec2(half, screen.height()));
                let right =
                    Rect::from_min_max(pos2(screen.left() + half, screen.top()), screen.max);
                miniature(
                    &clip.with_clip_rect(left.intersect(clip.clip_rect())),
                    screen,
                    light,
                    theme,
                );
                miniature(
                    &clip.with_clip_rect(right.intersect(clip.clip_rect())),
                    screen,
                    dark,
                    theme,
                );
                clip.line_segment(
                    [
                        pos2(right.left(), screen.top()),
                        pos2(right.left(), screen.bottom()),
                    ],
                    Stroke::new(1.0, Color32::from_black_alpha(60)),
                );
            } else {
                clip.rect_filled(winui::rect(pic), 6.0, winui::FIELD());
            }
            let edge = if chosen {
                Stroke::new(2.0, accent_now)
            } else if resp.hovered() {
                Stroke::new(1.0, winui::MUTED())
            } else {
                Stroke::new(1.0, winui::FIELD_EDGE())
            };
            clip.rect_stroke(winui::rect(pic), 6.0, edge, egui::StrokeKind::Outside);
            if chosen {
                // A tick in the accent at its corner.
                let c = winui::rect(pic).right_top() + vec2(-11.0, 11.0);
                clip.circle_filled(c, 8.0, accent_now);
                clip.line(
                    vec![
                        c + vec2(-3.5, 0.0),
                        c + vec2(-1.0, 2.5),
                        c + vec2(3.5, -2.5),
                    ],
                    Stroke::new(1.6, crate::derive::ink_on(accent_now)),
                );
            }
            let font = if chosen {
                winui::tahoma_bold()
            } else {
                winui::tahoma()
            };
            let name = winui::ellipsize(&clip, &font, &card.name, CARD_W - 4.0);
            let w = winui::text_width_font(&clip, font.clone(), &name);
            winui::text_font(
                &clip,
                font,
                x + (CARD_W - w) / 2.0,
                y + PIC_H + 16.0,
                &name,
                LIGHT_TEXT(),
            );
            winui::expose(
                &resp,
                egui::Role::RadioButton,
                &card.name,
                true,
                Some(chosen),
            );
            if resp.clicked() && !chosen {
                acts.push(Act::Theme(card.id));
            }
        }
    }

    /// Light, dark or as the system; the accent: the theme's own, one of the offered, or typed.
    fn appearance(&mut self, ui: &mut Ui, o: Pos2, st: &State, acts: &mut Vec<Act>) {
        let p = ui.painter().clone();
        let gw = (W - 2.0 * M - 12.0) / 2.0;
        // i18n: theme picker: the box with light or dark and the accent colour
        let caption = crate::i18n::t("Appearance");
        group(&p, o, M, gw, caption);
        let x0 = M + 10.0;
        let y0 = GROUP_Y + 20.0;
        let modes = [
            // i18n: theme picker: the theme's light side
            (Mode::Light, crate::i18n::mark("Light")),
            // i18n: theme picker: the theme's dark side
            (Mode::Dark, crate::i18n::mark("Dark")),
            // i18n: theme picker: light or dark as the system's own setting is (follows it)
            (Mode::System, crate::i18n::mark("As the system")),
        ];
        let mut x = x0;
        for (mode, label) in modes {
            if winui::radio_light(ui, o + vec2(x, y0), st.mode == mode, label, true).clicked()
                && st.mode != mode
            {
                acts.push(Act::Mode(mode));
            }
            x += 18.0 + winui::text_width(&p, crate::i18n::t(label)) + 22.0;
        }
        // The accent.
        // i18n: theme picker: a label before the accent colours (focus, selection, default buttons, progress)
        let label = crate::i18n::t("Accent colour:");
        text(&p, o.x + x0, o.y + y0 + 40.0, label, LIGHT_TEXT());
        let now = crate::look::accent_of(st.looks, st.theme);
        let own = self
            .cards
            .iter()
            .find(|c| c.id == st.theme)
            .and_then(|c| c.theme.as_ref())
            .map(|t| t.palette(crate::theme::dark()).accent)
            .unwrap_or(crate::theme::colors().accent);
        let swatches: Vec<(Option<Color32>, String)> = std::iter::once((
            None,
            // i18n: theme picker: tooltip of the first accent swatch: the theme's own accent colour
            crate::i18n::t("The theme's own").to_owned(),
        ))
        .chain(crate::look::ACCENTS.iter().map(|(name, rgb)| {
            (
                Some(crate::look::accent_color(*rgb)),
                crate::i18n::t(name).to_owned(),
            )
        }))
        .collect();
        let sy = y0 + 52.0;
        for (k, (color, name)) in swatches.iter().enumerate() {
            let cell = Rect::from_min_size(o + vec2(x0 + k as f32 * 28.0, sy), vec2(24.0, 24.0));
            let screen = winui::rect(cell);
            let resp = ui.interact(screen, ui.scope_id().with(("accent", k)), Sense::click());
            let fill = color.unwrap_or(own);
            p.circle_filled(screen.center(), 10.0, fill);
            if color.is_none() {
                // The theme's own: a ring inside, as "automatic" swatches show.
                p.circle_stroke(
                    screen.center(),
                    6.0,
                    Stroke::new(1.5, crate::derive::ink_on(fill)),
                );
            }
            let picked = *color == now;
            if picked {
                p.circle_stroke(screen.center(), 12.5, Stroke::new(2.0, LIGHT_TEXT()));
            } else if resp.hovered() {
                p.circle_stroke(screen.center(), 12.0, Stroke::new(1.0, winui::MUTED()));
            }
            winui::expose(&resp, egui::Role::RadioButton, name, true, Some(picked));
            if resp.on_hover_text(name.as_str()).clicked() && !picked {
                acts.push(Act::Accent(*color));
            }
        }
        // A colour of one's own, typed as #rrggbb.
        if self.hex_for.as_ref() != Some(&(st.theme.to_owned(), now)) {
            self.hex = now.map(crate::palette::to_hex).unwrap_or_default();
            self.hex_for = Some((st.theme.to_owned(), now));
        }
        // i18n: theme picker: a label before the box where an accent colour is typed (#rrggbb)
        let own_label = crate::i18n::t("Your own:");
        let cy = sy + 36.0;
        text(&p, o.x + x0, o.y + cy + 15.0, own_label, LIGHT_TEXT());
        let bx = x0 + winui::text_width(&p, own_label) + 8.0;
        let typed = winui::edit_light(ui, r(o, bx, cy, 90.0, 23.0), &mut self.hex, true, false);
        // i18n: what a screen reader calls the theme picker's box for an accent colour of one's own
        let name = crate::i18n::t("Accent colour, as #rrggbb");
        ui.ctx()
            .accesskit_node_builder(typed.id, |n| n.set_label(name));
        let parsed = crate::palette::parse_hex(&self.hex).filter(|c| c.a() == 255);
        if let Some(c) = parsed {
            let sw = Rect::from_min_size(o + vec2(bx + 98.0, cy + 2.0), vec2(19.0, 19.0));
            p.rect_filled(winui::rect(sw), 4.0, c);
        }
        if typed.changed()
            && let Some(c) = parsed
            && Some(c) != now
        {
            acts.push(Act::Accent(Some(c)));
            self.hex_for = Some((st.theme.to_owned(), Some(c)));
        }
        // i18n: theme picker, under the accent colours: each theme remembers its own accent and toolbar icons
        let note = crate::i18n::t("Each theme keeps its own accent colour and toolbar icons.");
        winui::fit(
            ui,
            &p,
            o.x + x0,
            o.y + GROUP_Y + GROUP_H - 14.0,
            note,
            winui::MUTED(),
            o.x + M + gw - 10.0,
            "theme-note",
        );
    }
}

/// The toolbar's icons, the list's text size and density, the interface size.
fn layout(ui: &mut Ui, o: Pos2, st: &State, acts: &mut Vec<Act>) {
    let p = ui.painter().clone();
    let gw = (W - 2.0 * M - 12.0) / 2.0;
    let gx = M + gw + 12.0;
    // i18n: theme picker: the box with the toolbar's icons, the text size and the density
    let caption = crate::i18n::t("Layout");
    group(&p, o, gx, gw, caption);
    let x0 = gx + 10.0;
    let y0 = GROUP_Y + 20.0;
    // i18n: theme picker: a label before the toolbar icons' choices
    text(
        &p,
        o.x + x0,
        o.y + y0 + 10.0,
        crate::i18n::t("Toolbar icons:"),
        LIGHT_TEXT(),
    );
    let styles = [
        // i18n: theme picker: the toolbar's icons as thin coloured lines
        (ToolbarIcons::Neon, crate::i18n::mark("Line"), true),
        // i18n: theme picker: the toolbar's icons as glossy 3D pictures
        (ToolbarIcons::ThreeD, crate::i18n::mark("3D"), true),
        // i18n: theme picker: the toolbar's icons the theme brings itself (a style theme's)
        (
            ToolbarIcons::Own,
            crate::i18n::mark("The theme's own"),
            st.own_icons,
        ),
    ];
    let mut x = x0;
    for (style, label, enabled) in styles {
        if winui::radio_light(
            ui,
            o + vec2(x, y0 + 20.0),
            st.toolbar == style,
            label,
            enabled,
        )
        .clicked()
            && st.toolbar != style
        {
            acts.push(Act::Toolbar(style));
        }
        x += 18.0 + winui::text_width(&p, crate::i18n::t(label)) + 22.0;
    }
    // The list's text size.
    let label_w = |s: &str| winui::text_width(&p, s);
    // i18n: theme picker: a label before the download list's text size
    let size_label = crate::i18n::t("Text size:");
    // i18n: theme picker: a label before the list rows' density (compact or comfortable)
    let density_label = crate::i18n::t("Density:");
    // i18n: theme picker: a label before View → Interface size's choices (100 % to 200 %)
    let zoom_label = crate::i18n::t("Interface size:");
    let bx = x0
        + label_w(size_label)
            .max(label_w(density_label))
            .max(label_w(zoom_label))
        + 10.0;
    let row = |n: f32| y0 + 54.0 + n * 34.0;
    text(
        &p,
        o.x + x0,
        o.y + row(0.0) + 15.0,
        size_label,
        LIGHT_TEXT(),
    );
    let sizes = [
        // i18n: theme picker: the download list's text smaller than the theme's
        (FontSize::Small, crate::i18n::mark("Small")),
        // i18n: theme picker: the download list's text at the theme's own size
        (FontSize::Normal, crate::i18n::mark("Normal")),
        // i18n: theme picker: the download list's text larger
        (FontSize::Large, crate::i18n::mark("Large")),
        // i18n: theme picker: the download list's text larger still
        (FontSize::ExtraLarge, crate::i18n::mark("Extra large")),
    ];
    let items: Vec<&str> = sizes.iter().map(|(_, l)| *l).collect();
    let at = sizes.iter().position(|(f, _)| *f == st.font).unwrap_or(1);
    if let Some(i) = winui::combo_light(
        ui,
        r(o, bx, row(0.0), 150.0, 23.0),
        &items,
        at,
        true,
        "theme-font",
    ) {
        acts.push(Act::Font(sizes[i].0));
    }
    text(
        &p,
        o.x + x0,
        o.y + row(1.0) + 15.0,
        density_label,
        LIGHT_TEXT(),
    );
    let densities = [
        // i18n: theme picker: the lists' rows as the theme draws them (the default)
        (Density::Compact, crate::i18n::mark("Compact")),
        // i18n: theme picker: the lists' rows taller, with more room around their text
        (Density::Comfortable, crate::i18n::mark("Comfortable")),
    ];
    let mut x = bx;
    for (d, label) in densities {
        if winui::radio_light(
            ui,
            o + vec2(x, row(1.0) + 5.0),
            st.density == d,
            label,
            true,
        )
        .clicked()
            && st.density != d
        {
            acts.push(Act::Density(d));
        }
        x += 18.0 + winui::text_width(&p, crate::i18n::t(label)) + 22.0;
    }
    text(
        &p,
        o.x + x0,
        o.y + row(2.0) + 15.0,
        zoom_label,
        LIGHT_TEXT(),
    );
    let steps = crate::scale::STEPS;
    let labels: Vec<String> = steps
        .iter()
        .map(|p| crate::i18n::percent(f64::from(*p) / 100.0, 0))
        .collect();
    let items: Vec<&str> = labels.iter().map(String::as_str).collect();
    let at = steps.iter().position(|s| *s == st.size).unwrap_or(0);
    if let Some(i) = winui::combo_light(
        ui,
        r(o, bx, row(2.0), 150.0, 23.0),
        &items,
        at,
        true,
        "theme-zoom",
    ) {
        acts.push(Act::Size(steps[i]));
    }
}

/// A group box with its caption at English x `x`, `w` wide.
fn group(p: &egui::Painter, o: Pos2, x: f32, w: f32, caption: &str) {
    let cw = winui::text_width(p, caption);
    winui::group_box(
        p,
        r(o, x, GROUP_Y, w, GROUP_H),
        Some((o.x + x + 7.0, o.x + x + 11.0 + cw)),
    );
    text(p, o.x + x + 9.0, o.y + GROUP_Y + 4.0, caption, LIGHT_TEXT());
}

/// A theme's name as the picker shows it: the built-in ones as View → Theme names them.
fn shown_name(id: &str, name: &str) -> String {
    match id {
        "classic" | "native" | "high-contrast" => crate::i18n::tc("menu", name).to_owned(),
        _ => name.to_owned(),
    }
}

/// A small picture of the main window in palette `c` of `theme`, filling `rc` (on screen):
/// its title bar when the theme draws one, the menu bar, the toolbar's icons, the categories and
/// the list with a selected row and a download's meter, and an accent button.
fn miniature(p: &egui::Painter, rc: Rect, c: &Palette, theme: &Theme) {
    let radius = 6.0;
    p.rect_filled(rc, radius, c.window);
    let w = rc.width();
    let mut y = rc.top();
    let dash = |x: f32, y: f32, len: f32, ink: Color32| {
        p.rect_filled(Rect::from_min_size(pos2(x, y), vec2(len, 2.0)), 1.0, ink);
    };
    if theme.controls.caption != CaptionLook::System {
        let bar = Rect::from_min_max(rc.min, pos2(rc.right(), y + 10.0));
        p.rect_filled(
            bar,
            egui::CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            },
            c.caption,
        );
        let mid = if matches!(
            theme.controls.caption,
            CaptionLook::Aqua | CaptionLook::Adwaita | CaptionLook::Metro | CaptionLook::Breeze
        ) {
            rc.center().x - 14.0
        } else {
            rc.left() + 8.0
        };
        dash(mid, y + 4.0, 28.0, c.caption_text);
        y += 10.0;
    }
    // The menu bar.
    let menu = Rect::from_min_max(pos2(rc.left(), y), pos2(rc.right(), y + 8.0));
    p.rect_filled(menu, 0.0, c.menubar);
    for k in 0..4 {
        dash(rc.left() + 6.0 + k as f32 * 14.0, y + 3.0, 9.0, c.menu_text);
    }
    y += 8.0;
    // The toolbar: its icons in the theme's ink.
    let inks = match theme.icons {
        IconStyle::Color => [c.accent, c.success, c.warning, c.error, c.link],
        IconStyle::Mono => [c.text; 5],
        IconStyle::Accent => [c.accent; 5],
    };
    for (k, ink) in inks.iter().enumerate() {
        let icon = Rect::from_min_size(
            pos2(rc.left() + 7.0 + k as f32 * 15.0, y + 3.0),
            vec2(9.0, 9.0),
        );
        p.rect_stroke(icon, 2.5, Stroke::new(1.5, *ink), egui::StrokeKind::Inside);
    }
    y += 15.0;
    p.line_segment(
        [pos2(rc.left(), y), pos2(rc.right(), y)],
        Stroke::new(1.0, c.toolbar_line),
    );
    // The categories.
    let body_bottom = rc.bottom() - 4.0;
    let tree = Rect::from_min_max(
        pos2(rc.left() + 3.0, y + 3.0),
        pos2(rc.left() + w * 0.27, body_bottom),
    );
    p.rect_filled(tree, 2.0, c.tree);
    p.rect_stroke(
        tree,
        2.0,
        Stroke::new(1.0, c.tree_frame),
        egui::StrokeKind::Inside,
    );
    for k in 0..5 {
        let ry = tree.top() + 4.0 + k as f32 * 7.0;
        if ry + 4.0 > tree.bottom() {
            break;
        }
        if k == 0 {
            p.rect_filled(
                Rect::from_min_size(
                    pos2(tree.left() + 2.0, ry - 2.0),
                    vec2(tree.width() - 4.0, 6.0),
                ),
                1.0,
                c.selection_inactive,
            );
        }
        dash(
            tree.left() + 4.0 + if k > 0 { 4.0 } else { 0.0 },
            ry,
            tree.width() * 0.5,
            c.text,
        );
    }
    // The list: its header, rows, one selected, one with its meter.
    let list = Rect::from_min_max(
        pos2(tree.right() + 3.0, y + 3.0),
        pos2(rc.right() - 3.0, body_bottom),
    );
    p.rect_filled(list, 2.0, c.list);
    p.rect_stroke(
        list,
        2.0,
        Stroke::new(1.0, c.list_frame),
        egui::StrokeKind::Inside,
    );
    let head = Rect::from_min_max(
        list.min + vec2(1.0, 1.0),
        pos2(list.right() - 1.0, list.top() + 7.0),
    );
    p.rect_filled(head, 0.0, c.header);
    dash(head.left() + 4.0, head.top() + 2.5, 16.0, c.muted);
    dash(
        head.left() + list.width() * 0.55,
        head.top() + 2.5,
        12.0,
        c.muted,
    );
    for k in 0..6 {
        let top = head.bottom() + k as f32 * 7.0;
        if top + 7.0 > list.bottom() - 1.0 {
            break;
        }
        let row = Rect::from_min_max(
            pos2(list.left() + 1.0, top),
            pos2(list.right() - 1.0, top + 7.0),
        );
        let ink = if k == 1 {
            p.rect_filled(row, 0.0, c.selection);
            c.selection_text
        } else {
            c.text
        };
        dash(row.left() + 4.0, top + 2.5, list.width() * 0.32, ink);
        if k == 2 {
            let meter = Rect::from_min_size(
                pos2(row.left() + list.width() * 0.55, top + 1.5),
                vec2(list.width() * 0.3, 4.0),
            );
            p.rect_filled(meter, 1.0, c.meter_track);
            p.rect_filled(
                Rect::from_min_size(meter.min, vec2(meter.width() * 0.62, meter.height())),
                1.0,
                c.meter_fill,
            );
        } else {
            let muted = if k == 1 { c.selection_muted } else { c.muted };
            dash(
                row.left() + list.width() * 0.55,
                top + 2.5,
                list.width() * 0.2,
                muted,
            );
        }
        p.line_segment(
            [
                pos2(row.left(), row.bottom()),
                pos2(row.right(), row.bottom()),
            ],
            Stroke::new(0.5, c.list_grid),
        );
    }
    // A default button in the accent.
    let button = Rect::from_min_size(
        pos2(list.right() - 26.0, list.bottom() - 10.0),
        vec2(22.0, 7.0),
    );
    p.rect_filled(button, 2.0, c.button_accent);
    dash(
        button.left() + 6.0,
        button.top() + 2.5,
        10.0,
        c.button_accent_text,
    );
}
