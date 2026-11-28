//! Toolbar icons: Lucide outlines (ISC licence, assets/icons/lucide/, v1.63.0) composed into
//! IDM 6.42's toolbar roles, stroked with the two-tone diagonal gradients measured from IDM's
//! "Neon" toolbar (tools/pixel/gradients.py). IDM's own images are never used (D-001).
//!
//! View → Toolbar → 3D style draws the toolbar's buttons with DDM's own glossy pictures instead
//! (assets/icons/toolbar3d, drawn by tools/icon/toolbar_3d.py), at the size each is shown at.

use std::sync::atomic::{AtomicU8, Ordering};

use egui::{Image, ImageSource};

/// View → Toolbar: how the toolbar's buttons are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub enum ToolbarIcons {
    /// Thin coloured line icons (the outlines here), the default.
    #[default]
    #[serde(rename = "neon")]
    Neon,
    /// Glossy, shaded, colourful pictures (assets/icons/toolbar3d).
    #[serde(rename = "3d")]
    ThreeD,
    /// The theme's own icon set's (a style theme's), where it has one; else the line icons.
    #[serde(rename = "own")]
    Own,
}

#[cfg(test)]
impl ToolbarIcons {
    /// Its name in the settings file.
    fn key(self) -> &'static str {
        match self {
            ToolbarIcons::Neon => "neon",
            ToolbarIcons::ThreeD => "3d",
            ToolbarIcons::Own => "own",
        }
    }
}

/// A settings file's style; one this DDM doesn't know (a later one's) reads as the default.
impl<'de> serde::Deserialize<'de> for ToolbarIcons {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
        Ok(match saved.as_str() {
            Some("3d") => ToolbarIcons::ThreeD,
            Some("own") => ToolbarIcons::Own,
            _ => ToolbarIcons::Neon,
        })
    }
}

/// The style the toolbar is drawn in now: the app sets it from its settings every frame, before
/// the toolbar, so every place that draws a toolbar button's icon follows it at once.
static TOOLBAR_ICONS: AtomicU8 = AtomicU8::new(0);

pub fn set_toolbar_icons(style: ToolbarIcons) {
    TOOLBAR_ICONS.store(style as u8, Ordering::Relaxed);
}

pub fn toolbar_icons() -> ToolbarIcons {
    match TOOLBAR_ICONS.load(Ordering::Relaxed) {
        n if n == ToolbarIcons::ThreeD as u8 => ToolbarIcons::ThreeD,
        n if n == ToolbarIcons::Own as u8 => ToolbarIcons::Own,
        _ => ToolbarIcons::Neon,
    }
}

/// Whether the active theme brings toolbar icons of its own (a style theme's set).
pub fn theme_has_toolbar_icons() -> bool {
    crate::themeicons::source("tb-add-url", 32.0, crate::themeicons::ink(true), true).is_some()
}

/// The 3D style's sizes, in pixels: the small buttons' 16 and the large ones' 32, and the sizes
/// they are at on screens at 125 %, 150 % and 200 %.
const THREE_D_SIZES: [u32; 7] = [16, 20, 24, 32, 40, 48, 64];

macro_rules! three_d {
    ($($need:literal),* $(,)?) => {
        &[$(($need, [
            include_bytes!(concat!("../assets/icons/toolbar3d/16/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/20/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/24/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/32/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/40/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/48/", $need, ".png")) as &[u8],
            include_bytes!(concat!("../assets/icons/toolbar3d/64/", $need, ".png")) as &[u8],
        ])),*]
    };
}

/// Each toolbar button's 3D picture at every size, by the theme icon sets' name for it.
static THREE_D: &[(&str, [&[u8]; 7])] = three_d!(
    "tb-add-url",
    "tb-add-torrent",
    "tb-resume",
    "tb-stop",
    "tb-stop-all",
    "tb-delete",
    "tb-delete-completed",
    "tb-options",
    "tb-scheduler",
    "tb-start-queue",
    "tb-stop-queue",
    "tb-grabber",
);

/// Which 3D size to draw for `px` screen pixels: the smallest drawn for at least that size (a
/// picture is never scaled up), else the largest.
fn three_d_size(px: f32) -> usize {
    let px = px.round();
    THREE_D_SIZES
        .iter()
        .position(|s| *s as f32 >= px)
        .unwrap_or(THREE_D_SIZES.len() - 1)
}

/// A 3D picture greyed for a disabled button: grey by its lightness, its contrast halved toward
/// a middle grey, half as opaque (as tools/icon/toolbar_3d.py's preview shows it). The bytes as
/// they are when they aren't a PNG.
fn greyed(png: &[u8]) -> std::sync::Arc<[u8]> {
    let Ok(img) = image::load_from_memory_with_format(png, image::ImageFormat::Png) else {
        return std::sync::Arc::from(png);
    };
    let mut img = img.to_rgba8();
    for px in img.pixels_mut() {
        let [r, g, b, a] = px.0;
        let lum = (u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) >> 8;
        let v = ((lum + 150) / 2) as u8;
        px.0 = [v, v, v, (u32::from(a) * 55 / 100) as u8];
    }
    let mut out = Vec::new();
    match img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png) {
        Ok(()) => std::sync::Arc::from(out),
        Err(_) => std::sync::Arc::from(png),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    AddUrl,
    /// Add Torrent (ours, M15): Lucide "magnet".
    AddTorrent,
    Resume,
    Stop,
    StopAll,
    Delete,
    DeleteCompleted,
    Options,
    Scheduler,
    StartQueue,
    StopQueue,
    /// Grabber (ours): the site grabber, Tasks → Run site grabber. DDM's own globe and arrow.
    Grabber,
    /// Download complete's drag handle: drag the file out (IDM's hand-and-arrows grip). Windows
    /// only, as the handle is (dialogs.rs).
    #[cfg_attr(not(windows), allow(dead_code))]
    DragOut,
    /// Options page headers (IDM draws its own artwork there; ours are Lucide outlines).
    PageGeneral,
    PageFileTypes,
    PageSaveTo,
    PageDownloads,
    PageConnection,
    PageProxy,
    PageLogins,
    PageSounds,
    PageRegional,
    PageArchives,
    PageRemote,
}

impl Icon {
    /// SVG body (24 × 24 user space, Lucide path data).
    fn body(self) -> &'static str {
        match self {
            // An outlined plus, like IDM's Add URL.
            Icon::AddUrl => r#"<path d="M9.5 2.5h5v7h7v5h-7v7h-5v-7h-7v-5h7z"/>"#,
            Icon::AddTorrent => concat!(
                r#"<path d="m6 15-4-4 6.75-6.77a7.79 7.79 0 0 1 11 11L13 22l-4-4 6.39-6.36a2.14 2.14 0 0 0-3-3L6 15"/>"#,
                r#"<path d="m5 8 4 4"/><path d="m12 15 4 4"/>"#,
            ),
            Icon::Resume => {
                r#"<circle cx="12" cy="12" r="10"/><path d="M12 7v10"/><path d="m8 13 4 4 4-4"/>"#
            }
            Icon::Stop => {
                r#"<circle cx="12" cy="12" r="10"/><rect x="8.5" y="8.5" width="7" height="7" rx="1.5"/>"#
            }
            Icon::StopAll => concat!(
                r#"<path d="M18 11V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2"/>"#,
                r#"<path d="M14 10V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v2"/>"#,
                r#"<path d="M10 10.5V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2v8"/>"#,
                r#"<path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15"/>"#,
            ),
            Icon::Delete => concat!(
                r#"<path d="M10 11v6"/><path d="M14 11v6"/>"#,
                r#"<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/>"#,
                r#"<path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#,
            ),
            Icon::DeleteCompleted => concat!(
                r#"<path d="m9 14 2 2 4-4.5"/>"#,
                r#"<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/>"#,
                r#"<path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#,
            ),
            Icon::Options => concat!(
                r#"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/>"#,
                r#"<circle cx="12" cy="12" r="3"/>"#,
            ),
            Icon::Scheduler => concat!(
                r#"<circle cx="12" cy="13" r="8"/><path d="M5 3 2 6"/><path d="m22 6-3-3"/>"#,
                r#"<path d="M6.38 18.7 4 21"/><path d="M17.64 18.67 20 21"/><path d="m9 13 2 2 4-4"/>"#,
            ),
            Icon::StartQueue => concat!(
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/>"#,
                r#"<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#,
                r#"<path d="M15 11v7"/><path d="m12 15.5 3 3 3-3"/>"#,
            ),
            Icon::StopQueue => concat!(
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/>"#,
                r#"<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#,
                r#"<rect width="6" height="6" x="12" y="12" rx="1"/>"#,
            ),
            // Ours, in Lucide's manner: a globe (a web site) with an arrow down beside it.
            Icon::Grabber => concat!(
                r#"<circle cx="9.5" cy="9.5" r="7.5"/><path d="M2 9.5h15"/>"#,
                r#"<path d="M9.5 2a10.9 10.9 0 0 0 0 15 10.9 10.9 0 0 0 0-15"/>"#,
                r#"<path d="M19 12v10"/><path d="m15.5 18.5 3.5 3.5 3.5-3.5"/>"#,
            ),
            // Lucide "move": four arrows.
            Icon::DragOut => concat!(
                r#"<path d="M12 2v20"/><path d="m15 19-3 3-3-3"/><path d="m19 9 3 3-3 3"/>"#,
                r#"<path d="M2 12h20"/><path d="m5 9-3 3 3 3"/><path d="m9 5 3-3 3 3"/>"#,
            ),
            // Lucide "globe".
            Icon::PageGeneral => concat!(
                r#"<circle cx="12" cy="12" r="10"/><path d="M2 12h20"/>"#,
                r#"<path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/>"#,
            ),
            // Lucide "file-type".
            Icon::PageFileTypes => concat!(
                r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/>"#,
                r#"<path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M9 13v-1h6v1"/><path d="M12 12v6"/><path d="M11 18h2"/>"#,
            ),
            // Lucide "folder-open".
            Icon::PageSaveTo => {
                r#"<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>"#
            }
            Icon::PageDownloads => {
                r#"<circle cx="12" cy="12" r="10"/><path d="M12 7v10"/><path d="m8 13 4 4 4-4"/>"#
            }
            // Lucide "network".
            Icon::PageConnection => concat!(
                r#"<rect x="16" y="16" width="6" height="6" rx="1"/><rect x="2" y="16" width="6" height="6" rx="1"/>"#,
                r#"<rect x="9" y="2" width="6" height="6" rx="1"/>"#,
                r#"<path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3"/><path d="M12 12V8"/>"#,
            ),
            // Lucide "shield".
            Icon::PageProxy => {
                r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#
            }
            // Lucide "key-round".
            Icon::PageLogins => concat!(
                r#"<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/>"#,
                r#"<circle cx="16.5" cy="7.5" r=".5"/>"#,
            ),
            // Lucide "volume-2".
            // Lucide "calendar-clock".
            Icon::PageRegional => concat!(
                r#"<path d="M16 14v2.2l1.6 1"/><path d="M16 2v3"/><path d="M8 2v3"/>"#,
                r#"<path d="M21 7.338V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h2.338"/>"#,
                r#"<path d="M3 9h5.859"/><circle cx="16" cy="16" r="6"/>"#,
            ),
            // Lucide "package".
            Icon::PageArchives => concat!(
                r#"<path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z"/>"#,
                r#"<path d="M12 22V12"/><path d="m3.3 7 7.703 4.734a2 2 0 0 0 1.994 0L20.7 7"/><path d="m7.5 4.27 9 5.15"/>"#,
            ),
            // Lucide "smartphone" (Options → Remote, #141).
            Icon::PageRemote => {
                r#"<rect width="14" height="20" x="5" y="2" rx="2" ry="2"/><path d="M12 18h.01"/>"#
            }
            Icon::PageSounds => concat!(
                r#"<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/>"#,
                r#"<path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.364 18.364a9 9 0 0 0 0-12.728"/>"#,
            ),
        }
    }

    /// The enabled gradient, top-left → bottom-right (measured from IDM 6.42).
    fn gradient(self) -> (&'static str, &'static str) {
        match self {
            Icon::AddUrl => ("#9ad9b0", "#7fb4c0"),
            // The Torrent pill's purple.
            Icon::AddTorrent => ("#b07ad9", "#7d5fd0"),
            Icon::Resume | Icon::Stop | Icon::StopAll | Icon::Delete => ("#e2e2e2", "#b4b4b4"),
            Icon::DeleteCompleted => ("#df6a9c", "#8180c0"),
            Icon::Options => ("#7883c2", "#9b84d0"),
            Icon::Scheduler => ("#a986d2", "#c0957a"),
            Icon::StartQueue => ("#93b6f2", "#90c6b6"),
            Icon::StopQueue => ("#a2c2f2", "#c4688f"),
            // Ours: the queues' blue to Add URL's green.
            Icon::Grabber => ("#8fb8f2", "#8fd0a8"),
            // Ours: IDM's grip has orange arrows.
            Icon::DragOut => ("#f5a623", "#d9661c"),
            Icon::PageGeneral | Icon::PageConnection => ("#3d8ee6", "#2bb3a3"),
            Icon::PageFileTypes | Icon::PageDownloads => ("#3fa864", "#2f7fcf"),
            Icon::PageSaveTo => ("#e6a817", "#c77a12"),
            Icon::PageProxy => ("#5b6fd6", "#8a5fd0"),
            Icon::PageLogins => ("#d9a514", "#9a7b2e"),
            Icon::PageSounds => ("#8a5fd0", "#d05f9a"),
            Icon::PageRegional => ("#2f7fcf", "#3fa864"),
            Icon::PageArchives => ("#c77a12", "#8a5fd0"),
            Icon::PageRemote => ("#3d8ee6", "#5b6fd6"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Icon::AddUrl => "add-url",
            Icon::AddTorrent => "add-torrent",
            Icon::Resume => "resume",
            Icon::Stop => "stop",
            Icon::StopAll => "stop-all",
            Icon::Delete => "delete",
            Icon::DeleteCompleted => "delete-completed",
            Icon::Options => "options",
            Icon::Scheduler => "scheduler",
            Icon::StartQueue => "start-queue",
            Icon::StopQueue => "stop-queue",
            Icon::Grabber => "grabber",
            Icon::DragOut => "drag-out",
            Icon::PageGeneral => "page-general",
            Icon::PageFileTypes => "page-file-types",
            Icon::PageSaveTo => "page-save-to",
            Icon::PageDownloads => "page-downloads",
            Icon::PageConnection => "page-connection",
            Icon::PageProxy => "page-proxy",
            Icon::PageLogins => "page-logins",
            Icon::PageSounds => "page-sounds",
            Icon::PageRegional => "page-regional",
            Icon::PageArchives => "page-archives",
            Icon::PageRemote => "page-remote",
        }
    }

    /// The ink's two stops, top-left to bottom-right: enabled, its gradient, one colour or
    /// the accent as the theme's icon style says; disabled, the theme's flat grey (as IDM).
    fn stops(self, enabled: bool) -> (String, String) {
        use crate::palette::{IconStyle, to_hex};
        let c = crate::theme::colors();
        if !enabled {
            return (to_hex(c.icon_disabled), to_hex(c.icon_disabled_end));
        }
        match crate::theme::current().icons {
            IconStyle::Color => {
                let (a, b) = self.gradient();
                (a.to_owned(), b.to_owned())
            }
            IconStyle::Mono => (to_hex(c.text), to_hex(c.text)),
            IconStyle::Accent => (to_hex(c.accent), to_hex(c.accent)),
        }
    }

    /// The SVG for this icon, inked from `a` to `b`.
    fn svg(self, a: &str, b: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="url(#g)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="2" y1="2" x2="22" y2="22"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient></defs>{}</svg>"#,
            self.body()
        )
    }

    /// The theme icon set's name for this icon (`crate::themeicons`), for the ones a set can
    /// replace: the toolbar's. The 3D style's pictures go by it too. Grabber's (`tb-grabber`)
    /// is the 3D picture's only: it isn't one of the needs a set can fill yet
    /// (`themeicons::NEEDS`; none of the built-in sets draws it), so a theme with a set shows
    /// DDM's own Grabber icon among its own.
    fn need(self) -> Option<&'static str> {
        Some(match self {
            Icon::AddUrl => "tb-add-url",
            Icon::AddTorrent => "tb-add-torrent",
            Icon::Resume => "tb-resume",
            Icon::Stop => "tb-stop",
            Icon::StopAll => "tb-stop-all",
            Icon::Delete => "tb-delete",
            Icon::DeleteCompleted => "tb-delete-completed",
            Icon::Options => "tb-options",
            Icon::Scheduler => "tb-scheduler",
            Icon::StartQueue => "tb-start-queue",
            Icon::StopQueue => "tb-stop-queue",
            Icon::Grabber => "tb-grabber",
            _ => return None,
        })
    }

    /// The icon, enabled or greyed: the theme's own where its icon set has one (a one-colour
    /// icon in the text colour), else the classic outline.
    pub fn image(self, enabled: bool) -> Image<'static> {
        self.image_px(enabled, 32.0)
    }

    /// [`Icon::image`] drawn about `px` pixels on screen: a theme's icon set gives its file made
    /// for that size (the small toolbar's 16-point icons take a set's 16-pixel drawings). The
    /// classic outline is a drawing, rasterized at the size it is painted.
    ///
    /// View → Toolbar → 3D style: a toolbar button's 3D picture drawn for that size, where the
    /// theme has no icon of its own for it and inks icons in their own colours (the classic look,
    /// Luna's, Aero's, Aqua's and themes like them). A theme with its own icon set keeps it, and
    /// one that inks icons in one colour (High contrast, Metro...) keeps its outlines in that ink:
    /// chosen for legibility, which colour pictures would undo.
    pub fn image_px(self, enabled: bool, px: f32) -> Image<'static> {
        let ink = crate::themeicons::ink(enabled);
        // The theme's own icons, unless the line or 3D style was chosen for its toolbar.
        let style = toolbar_icons();
        if style == ToolbarIcons::Own
            && let Some((src, tint)) = self
                .need()
                .and_then(|need| crate::themeicons::source(need, px, ink, enabled))
        {
            return Image::new(src).tint(tint);
        }
        if style == ToolbarIcons::ThreeD
            && crate::theme::current().icons == crate::palette::IconStyle::Color
            && let Some(src) = self.three_d(enabled, px)
        {
            return Image::new(src);
        }
        Image::new(self.source(enabled))
    }

    /// The 3D picture for about `px` screen pixels, greyed when not `enabled`; None for an icon
    /// that isn't a toolbar button's. Made once per icon, size and state.
    fn three_d(self, enabled: bool, px: f32) -> Option<ImageSource<'static>> {
        use std::cell::RefCell;
        use std::collections::HashMap;
        thread_local! {
            static MADE: RefCell<HashMap<(Icon, usize, bool), ImageSource<'static>>> =
                RefCell::default();
        }
        let need = self.need()?;
        let files = THREE_D.iter().find(|(n, _)| *n == need)?.1;
        let i = three_d_size(px);
        Some(MADE.with(|made| {
            made.borrow_mut()
                .entry((self, i, enabled))
                .or_insert_with(|| {
                    // Kept for the program's life, as its texture is.
                    let uri: &'static str = Box::leak(
                        format!(
                            "bytes://mdm/toolbar3d/{}/{need}-{}.png",
                            THREE_D_SIZES[i],
                            if enabled { "on" } else { "off" }
                        )
                        .into_boxed_str(),
                    );
                    let bytes = if enabled {
                        egui::load::Bytes::Static(files[i])
                    } else {
                        egui::load::Bytes::Shared(greyed(files[i]))
                    };
                    ImageSource::Bytes {
                        uri: uri.into(),
                        bytes,
                    }
                })
                .clone()
        }))
    }

    /// The icon as an image source (fileicons.rs shows Add Torrent's magnet for a .torrent).
    /// Made once per icon and ink (a theme's change makes new ones): drawing it again each frame
    /// allocates nothing.
    pub fn source(self, enabled: bool) -> ImageSource<'static> {
        use crate::palette::IconStyle;
        use std::cell::RefCell;
        use std::collections::HashMap;
        type Key = (Icon, bool, IconStyle, [u8; 4], [u8; 4]);
        thread_local! {
            static MADE: RefCell<HashMap<Key, ImageSource<'static>>> = RefCell::default();
        }
        let c = crate::theme::colors();
        let style = crate::theme::current().icons;
        let (x, y) = match (enabled, style) {
            (false, _) => (c.icon_disabled, c.icon_disabled_end),
            (true, IconStyle::Color) => (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT),
            (true, IconStyle::Mono) => (c.text, c.text),
            (true, IconStyle::Accent) => (c.accent, c.accent),
        };
        let key = (self, enabled, style, x.to_array(), y.to_array());
        MADE.with(|made| {
            made.borrow_mut()
                .entry(key)
                .or_insert_with(|| {
                    let (a, b) = self.stops(enabled);
                    // Keyed by its ink too: another theme's icon is another texture. Kept for
                    // the program's life, as its texture is (a few per theme).
                    let uri: &'static str = Box::leak(
                        format!(
                            "bytes://mdm/toolbar/{}-{}-{}{}.svg",
                            self.name(),
                            if enabled { "on" } else { "off" },
                            a.trim_start_matches('#'),
                            b.trim_start_matches('#'),
                        )
                        .into_boxed_str(),
                    );
                    ImageSource::Bytes {
                        uri: uri.into(),
                        bytes: egui::load::Bytes::Shared(self.svg(&a, &b).into_bytes().into()),
                    }
                })
                .clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every toolbar button has a 3D picture at every size, square at that size, and nothing
    /// else has one.
    #[test]
    fn every_toolbar_button_has_its_3d_picture_at_every_size() {
        for (icon, ..) in crate::toolbar::BUTTONS {
            let need = icon.need().unwrap();
            let (_, files) = THREE_D.iter().find(|(n, _)| *n == need).unwrap();
            for (bytes, size) in files.iter().zip(THREE_D_SIZES) {
                let img = image::load_from_memory(bytes).unwrap();
                assert_eq!((img.width(), img.height()), (size, size), "{need}");
                assert!(
                    img.to_rgba8().pixels().any(|p| p.0[3] == 255),
                    "{need} {size}"
                );
            }
        }
        assert_eq!(THREE_D.len(), crate::toolbar::BUTTONS.len());
        assert!(Icon::PageGeneral.three_d(true, 32.0).is_none());
    }

    /// A size takes the picture drawn for it, else the next larger (none is scaled up): the
    /// small buttons' 16 points at 175 % (28 pixels) take the 32-pixel one.
    #[test]
    fn a_3d_picture_is_picked_by_its_size_on_screen() {
        let size = |px: f32| THREE_D_SIZES[three_d_size(px)];
        assert_eq!(size(16.0), 16);
        assert_eq!(size(20.0), 20);
        assert_eq!(size(24.0), 24);
        assert_eq!(size(28.0), 32);
        assert_eq!(size(32.0), 32);
        assert_eq!(size(48.0), 48);
        assert_eq!(size(56.0), 64);
        assert_eq!(size(96.0), 64);
    }

    /// Greyed: no colour left, half as opaque.
    #[test]
    fn a_disabled_3d_picture_is_grey_and_faded() {
        let png = THREE_D[0].1[3];
        let before = image::load_from_memory(png).unwrap().to_rgba8();
        let after = image::load_from_memory(&greyed(png)).unwrap().to_rgba8();
        for (b, a) in before.pixels().zip(after.pixels()) {
            assert!(a.0[0] == a.0[1] && a.0[1] == a.0[2]);
            assert_eq!(u32::from(a.0[3]), u32::from(b.0[3]) * 55 / 100);
        }
    }

    /// The style reads back from the settings file; an unknown one (a later DDM's) is Neon.
    #[test]
    fn the_style_reads_back_and_an_unknown_one_is_neon() {
        for style in [ToolbarIcons::Neon, ToolbarIcons::ThreeD, ToolbarIcons::Own] {
            let text = serde_json::to_string(&style).unwrap();
            assert_eq!(text, format!("\"{}\"", style.key()));
            assert_eq!(serde_json::from_str::<ToolbarIcons>(&text).unwrap(), style);
        }
        assert_eq!(
            serde_json::from_str::<ToolbarIcons>("\"glass\"").unwrap(),
            ToolbarIcons::Neon
        );
        assert_eq!(
            serde_json::from_str::<ToolbarIcons>("7").unwrap(),
            ToolbarIcons::Neon
        );
    }
}
