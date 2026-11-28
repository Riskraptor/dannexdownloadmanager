//! The look's choices beyond a theme's own (#45, beyond IDM, whose toolbar styles are its only
//! choice): how dense the lists are, and each theme's own accent colour and toolbar icons, kept
//! by its id and taken back when it is picked again. The accent is laid over the theme's
//! colours (`with_accent`): every colour of its accent's family takes the new hue, the rest
//! (text, greys, errors, warnings, success) stays, and text on the accent stays readable.

use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::derive::{from_oklch, oklch, readable};
use crate::palette::{Palette, ROLES, Theme};

/// View → Density: how much room the lists' rows have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    /// The theme's own row height (the classic list's 17 points), the default.
    #[default]
    Compact,
    /// Taller rows, with more air around their text.
    Comfortable,
}

impl Density {
    /// Points added to the download list's rows.
    pub fn row_grow(self) -> f32 {
        match self {
            Density::Compact => 0.0,
            Density::Comfortable => 6.0,
        }
    }
}

/// A settings file's density; one this DDM doesn't know (a later one's) reads as the default.
impl<'de> Deserialize<'de> for Density {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = <serde_json::Value as Deserialize>::deserialize(d)?;
        Ok(match saved.as_str() {
            Some("comfortable") => Density::Comfortable,
            _ => Density::Compact,
        })
    }
}

/// A theme's own choices, by its id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeLook {
    pub theme: String,
    /// Its accent colour (`#rrggbb`); None: the theme's own.
    pub accent: Option<String>,
    /// Its toolbar's icons; None: the theme's own set when it has one, else the style last
    /// chosen for a theme without one.
    pub toolbar: Option<crate::icons::ToolbarIcons>,
}

/// Theme `theme`'s choices, when it has any.
pub fn look_of<'a>(looks: &'a [ThemeLook], theme: &str) -> Option<&'a ThemeLook> {
    looks.iter().find(|l| l.theme == theme)
}

/// Theme `theme`'s choices, made when it has none yet.
pub fn look_mut<'a>(looks: &'a mut Vec<ThemeLook>, theme: &str) -> &'a mut ThemeLook {
    match looks.iter().position(|l| l.theme == theme) {
        Some(i) => &mut looks[i],
        None => {
            looks.push(ThemeLook {
                theme: theme.to_owned(),
                ..Default::default()
            });
            looks.last_mut().expect("just pushed")
        }
    }
}

/// Theme `theme`'s accent colour, when one was chosen for it.
pub fn accent_of(looks: &[ThemeLook], theme: &str) -> Option<Color32> {
    look_of(looks, theme)
        .and_then(|l| l.accent.as_deref())
        .and_then(crate::palette::parse_hex)
}

/// The accent colours offered, each with its name (Windows' and the desktops' own accents).
pub const ACCENTS: [(&str, u32); 10] = [
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Blue"), 0x0078d4),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Teal"), 0x038387),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Green"), 0x107c10),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Olive"), 0x6b8e23),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Gold"), 0xc19c00),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Orange"), 0xca5010),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Red"), 0xd13438),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Pink"), 0xc239b3),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Purple"), 0x8764b8),
    // i18n: an accent colour's name (View → Theme → Choose a theme...)
    (crate::i18n::mark("Graphite"), 0x69797e),
];

/// An offered accent's colour.
pub fn accent_color(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// `theme` with its accent made `accent`, both its sides ([`recolor`]).
pub fn with_accent(theme: &Theme, accent: Color32) -> Theme {
    let mut t = theme.clone();
    t.light = recolor(&theme.light, accent);
    t.dark = recolor(&theme.dark, accent);
    t
}

/// Roles that keep their colour whatever the accent: what they say is their colour.
const KEEP: [&str; 9] = [
    "error",
    "error.strong",
    "warning",
    "success",
    "reach.near",
    "reach.medium",
    "reach.far",
    "caption.close",
    "flag.edge",
];

/// A text colour of a palette to keep readable, and the fill it is on.
type InkOn = fn(&mut Palette) -> (&mut Color32, Color32);

/// How near a colour's hue must be to the accent's to be of its family (radians: about 34°).
const FAMILY: f32 = 0.6;

/// A colour without its transparency, and that transparency.
fn opaque(c: Color32) -> (Color32, u8) {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    (Color32::from_rgb(r, g, b), a)
}

/// Palette `p` with its accent made `to`: every colour of the accent's family (near its hue,
/// coloured enough to have one) takes the new hue and the new accent's colourfulness; the
/// strong ones its lightness too, the faint tints (a row under the pointer) keep theirs. The
/// accent itself, links and the text on accent-filled parts are then kept readable. A palette
/// whose accent is grey (High contrast's) has only its accent's own roles changed.
pub fn recolor(p: &Palette, to: Color32) -> Palette {
    let (to, _) = opaque(to);
    let (l0, c0, h0) = oklch(opaque(p.accent).0);
    let (l1, c1, h1) = oklch(to);
    let mut out = *p;
    let grey = c0 < 0.03;
    let ratio = if grey { 1.0 } else { (c1 / c0).clamp(0.0, 2.5) };
    let shift = |c: Color32| -> Option<Color32> {
        let (rgb, a) = opaque(c);
        if a == 0 {
            return None;
        }
        if grey {
            return (rgb == opaque(p.accent).0)
                .then(|| Color32::from_rgba_unmultiplied(to.r(), to.g(), to.b(), a));
        }
        let (l, ch, h) = oklch(rgb);
        let d = (h - h0).rem_euclid(std::f32::consts::TAU);
        let d = d.min(std::f32::consts::TAU - d);
        if ch < 0.025 || d > FAMILY {
            return None;
        }
        let strength = (ch / c0).min(1.0);
        let made = from_oklch(l + (l1 - l0) * strength, ch * ratio, h + (h1 - h0));
        Some(Color32::from_rgba_unmultiplied(
            made.r(),
            made.g(),
            made.b(),
            a,
        ))
    };
    for (key, _) in ROLES {
        if KEEP.contains(key) {
            continue;
        }
        if let Some(c) = p.role(key)
            && let Some(n) = shift(c)
        {
            out.set_role(key, n);
        }
    }
    for (made, &c) in out.segments.done.iter_mut().zip(&p.segments.done) {
        if let Some(n) = shift(c) {
            *made = n;
        }
    }
    // Readable as before: the accent on the window, links, text on what the accent fills.
    out.accent = readable(out.accent, out.window, 3.0);
    if out.link != p.link {
        out.link = readable(out.link, out.window, 4.5);
    }
    let pairs: [(InkOn, f32); 6] = [
        (|q| (&mut q.selection_text, q.selection), 4.5),
        (|q| (&mut q.selection_muted, q.selection), 3.0),
        (|q| (&mut q.button_accent_text, q.button_accent), 4.5),
        (
            |q| (&mut q.button_accent_text_pressed, q.button_accent_pressed),
            4.5,
        ),
        (|q| (&mut q.check_mark, q.check_on), 3.0),
        (|q| (&mut q.caption_text, q.caption), 4.5),
    ];
    for (pair, ratio) in pairs {
        let (ink, fill) = pair(&mut out);
        if fill.a() == 255 {
            *ink = readable(*ink, fill, ratio);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::contrast;

    #[test]
    fn a_new_accent_recolours_its_family_and_keeps_the_rest() {
        let classic = &crate::palette::CLASSIC;
        let green = accent_color(0x107c10);
        for p in [&classic.light, &classic.dark] {
            let q = recolor(p, green);
            // The accent's family is green now...
            let (_, c, h) = oklch(q.accent);
            let (_, _, hg) = oklch(green);
            assert!(c > 0.05 && (h - hg).abs() < 0.2, "{:?}", q.accent);
            assert_ne!(q.selection, p.selection);
            assert_ne!(q.check_on, p.check_on);
            // ... the rest as it was.
            for key in [
                "text", "window", "field", "error", "warning", "success", "muted",
            ] {
                if let Some(c) = p.role(key) {
                    assert_eq!(q.role(key), Some(c), "{key}");
                }
            }
            // Readable.
            assert!(contrast(q.accent, q.window) >= 3.0);
            assert!(contrast(q.selection_text, q.selection) >= 4.5);
            assert!(contrast(q.button_accent_text, q.button_accent) >= 4.5);
        }
    }

    /// A light accent on a light theme: text on it turns dark to stay readable.
    #[test]
    fn text_on_a_light_accent_stays_readable() {
        let classic = &crate::palette::CLASSIC;
        let q = recolor(&classic.light, accent_color(0xffd700));
        assert!(contrast(q.button_accent_text, q.button_accent) >= 4.5);
        assert!(contrast(q.check_mark, q.check_on) >= 3.0);
        assert!(contrast(q.accent, q.window) >= 3.0);
    }

    /// Every built-in theme takes every offered accent, readably.
    #[test]
    fn every_theme_takes_every_accent() {
        for t in crate::palette::builtins() {
            for (name, rgb) in ACCENTS {
                let made = with_accent(t, accent_color(rgb));
                for p in [&made.light, &made.dark] {
                    assert!(
                        contrast(p.accent, p.window) >= 2.9,
                        "{} {name}: {:?} on {:?}",
                        t.id,
                        p.accent,
                        p.window
                    );
                }
            }
        }
    }

    #[test]
    fn looks_are_kept_by_theme() {
        let mut looks = Vec::new();
        look_mut(&mut looks, "luna").accent = Some("#107c10".into());
        look_mut(&mut looks, "classic").toolbar = Some(crate::icons::ToolbarIcons::ThreeD);
        look_mut(&mut looks, "luna").toolbar = Some(crate::icons::ToolbarIcons::Neon);
        assert_eq!(looks.len(), 2);
        assert_eq!(accent_of(&looks, "luna"), Some(accent_color(0x107c10)));
        assert_eq!(accent_of(&looks, "classic"), None);
        let json = serde_json::to_string(&Density::Comfortable).unwrap();
        assert_eq!(json, "\"comfortable\"");
        assert_eq!(
            serde_json::from_str::<Density>("\"roomy\"").unwrap(),
            Density::Compact
        );
    }
}
