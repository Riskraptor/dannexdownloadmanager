//! View's submenus and what they set: themes (and theme files added), the interface size, the font,
//! the toolbar, the tray icon, how text is drawn and the graphics API.

use super::*;

impl App {
    /// View → Theme (#45): the built-in themes, the theme files in the themes folder, and a way
    /// to that folder. Picked, a theme applies at once.
    pub(super) fn theme_menu(&self) -> MenuEntry<Cmd> {
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let chosen = self.settings.theme.as_str();
        let mut items = Vec::new();
        let mut files = false;
        let on = |id: &str| chosen == id;
        // i18n: View → Theme: opens the theme picker (every theme's picture, the accent colour, the toolbar's icons, the density)
        items.push(I("&Choose a theme...", Cmd::ThemePicker, true, false));
        items.push(Sep);
        // i18n: View → Theme: DDM's own look, the default (the classic download manager's layout, Windows 11's controls)
        items.push(Radio("Classic", Cmd::Theme("classic"), true, on("classic")));
        // i18n: View → Theme: the system's own colours (its accent colour, its high contrast)
        items.push(Radio("Native", Cmd::Theme("native"), true, on("native")));
        // i18n: View → Theme: black and white with strong edges, for low vision
        items.push(Radio(
            "High contrast",
            Cmd::Theme("high-contrast"),
            true,
            on("high-contrast"),
        ));
        items.push(Sep);
        // i18n: View → Theme: a style theme's name (Windows 8's flat look); keep "Metro" unless your language writes it otherwise
        items.push(Radio("Metro", Cmd::Theme("metro"), true, on("metro")));
        // i18n: View → Theme: a style theme's name (GNOME's look, the Linux desktop), a name: keep "Adwaita" as written
        items.push(Radio("Adwaita", Cmd::Theme("adwaita"), true, on("adwaita")));
        // i18n: View → Theme: a style theme's name (KDE Plasma's look, the Linux desktop), a name: keep "Breeze" as written
        items.push(Radio("Breeze", Cmd::Theme("breeze"), true, on("breeze")));
        // i18n: View → Theme: a style theme's name (Windows XP's look), a name: keep "Luna" as written
        items.push(Radio("Luna", Cmd::Theme("luna"), true, on("luna")));
        // i18n: View → Theme: a style theme's name (Windows 7's glassy look), a name: keep "Aero" as written
        items.push(Radio("Aero", Cmd::Theme("aero"), true, on("aero")));
        // i18n: View → Theme: a style theme's name (Mac OS X's look), a name: keep "Aqua" as written
        items.push(Radio("Aqua", Cmd::Theme("aqua"), true, on("aqua")));
        // i18n: View → Theme: a style theme's name (a white canvas, emerald fills, 6-point corners), a name: keep "Emerald" as written
        items.push(Radio("Emerald", Cmd::Theme("emerald"), true, on("emerald")));
        let wal = crate::themefile::PYWAL;
        for e in self.theme_list.iter().filter(|e| e.path.is_some()) {
            if !files {
                items.push(Sep);
                files = true;
            }
            // pywal's colours, where `wal` writes them, followed as it writes them again.
            if e.id == wal && e.name == wal {
                // i18n: View → Theme: the colours of pywal (a program making colour schemes from the wallpaper), followed as they change; a name: keep "pywal" as written
                items.push(Radio("pywal", Cmd::Theme(wal), true, chosen == wal));
                continue;
            }
            // A file's theme by its own name, untranslated.
            let (id, name) = (crate::chrome::label(&e.id), crate::chrome::label(&e.name));
            items.push(Radio(name, Cmd::Theme(id), true, e.id == chosen));
        }
        items.push(Sep);
        // i18n: View → Theme: picks a theme file or a Base16 or pywal colour scheme, copies it into the themes folder and uses it
        items.push(I("Add a theme file…", Cmd::AddThemeFile, true, false));
        // i18n: View → Theme: saves the theme in use (its accent colour included) as a theme file
        items.push(I("Export this theme…", Cmd::ExportTheme, true, false));
        // i18n: View → Theme: opens the folder where theme files go (TOML or JSON)
        items.push(I("Open themes folder", Cmd::OpenThemesFolder, true, false));
        // i18n: View menu: submenu choosing DDM's look (colours, sizes, icons): Classic, Native…
        Sub("Theme", true, items)
    }

    /// View → Interface size (#262): every window larger, as a browser zooms (Ctrl + / Ctrl − /
    /// Ctrl 0 too). The size shown is the zoom drawn now, a test run's MDM_ZOOM too.
    pub(super) fn interface_size_menu(&self) -> MenuEntry<Cmd> {
        let now = crate::scale::of_zoom(self.zoom);
        let items = crate::scale::STEPS
            .into_iter()
            .map(|p| {
                // As the interface language writes a percentage: 125%, 125 %, %125.
                let label = crate::chrome::label(&crate::i18n::percent(f64::from(p) / 100.0, 0));
                MenuEntry::Radio(label, Cmd::InterfaceSize(p), true, p == now)
            })
            .collect();
        // i18n: View menu: submenu choosing how large DDM draws its windows and text (100 % to 200 %), as a browser's zoom
        MenuEntry::Sub("&Interface size", true, items)
    }

    /// View → Font: how large the download list writes its rows (its rows grow with the text),
    /// and its file names in bold. Applies at once; Interface size enlarges it all the same.
    pub(super) fn font_menu(&self) -> MenuEntry<Cmd> {
        use crate::list::FontSize;
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let now = self.settings.list_font;
        let items = vec![
            // i18n: View → Font: the download list's text smaller than the theme's
            Radio(
                "&Small",
                Cmd::ListFont(FontSize::Small),
                true,
                now == FontSize::Small,
            ),
            // i18n: View → Font: the download list's text at the theme's own size (the default)
            Radio(
                "&Normal",
                Cmd::ListFont(FontSize::Normal),
                true,
                now == FontSize::Normal,
            ),
            // i18n: View → Font: the download list's text larger, its rows taller
            Radio(
                "&Large",
                Cmd::ListFont(FontSize::Large),
                true,
                now == FontSize::Large,
            ),
            // i18n: View → Font: the download list's text larger still, its rows taller
            Radio(
                "E&xtra large",
                Cmd::ListFont(FontSize::ExtraLarge),
                true,
                now == FontSize::ExtraLarge,
            ),
            Sep,
            I(
                // i18n: View → Font: a check item; the File Name column's text in bold
                "&Bold file names",
                Cmd::ToggleBoldNames,
                true,
                self.settings.list_bold_names,
            ),
            Sep,
            Radio(
                // i18n: View → Font: the lists' rows as the theme draws them, the default (density)
                "Co&mpact rows",
                Cmd::Density(crate::look::Density::Compact),
                true,
                self.settings.density == crate::look::Density::Compact,
            ),
            Radio(
                // i18n: View → Font: the lists' rows taller, with more room around their text (density)
                "Com&fortable rows",
                Cmd::Density(crate::look::Density::Comfortable),
                true,
                self.settings.density == crate::look::Density::Comfortable,
            ),
        ];
        // i18n: noun: the list's font (submenu)
        Sub("Font", true, items)
    }

    /// View → Toolbar: large buttons (IDM's) or small ones, their labels shown or not, and which
    /// buttons show in what order (Customize...). Applies at once: the Categories pane and the
    /// list start under the band's new height (`chrome::pane_top`).
    pub(super) fn toolbar_menu(&self) -> MenuEntry<Cmd> {
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let small = self.settings.toolbar_small;
        let style = self.toolbar_style();
        let items = vec![
            // i18n: View → Toolbar: the toolbar's buttons large (32-point icons), the default
            Radio("&Large buttons", Cmd::ToolbarSmall(false), true, !small),
            // i18n: View → Toolbar: the toolbar's buttons small (16-point icons), in a lower toolbar
            Radio("&Small buttons", Cmd::ToolbarSmall(true), true, small),
            Sep,
            I(
                // i18n: View → Toolbar: a check item; each button's name shown with its icon (off: icons only, each named by its tooltip)
                "Show text &labels",
                Cmd::ToggleToolbarLabels,
                true,
                self.settings.toolbar_labels,
            ),
            Sep,
            Radio(
                // i18n: View → Toolbar: the buttons' icons as thin coloured lines, the default (a style's name)
                "L&ine style",
                Cmd::ToolbarIcons(crate::icons::ToolbarIcons::Neon),
                true,
                style == crate::icons::ToolbarIcons::Neon,
            ),
            Radio(
                // i18n: View → Toolbar: the buttons' icons as glossy, shaded, colourful pictures
                "&3D style",
                Cmd::ToolbarIcons(crate::icons::ToolbarIcons::ThreeD),
                true,
                style == crate::icons::ToolbarIcons::ThreeD,
            ),
            Radio(
                // i18n: View → Toolbar: the buttons' icons the theme brings itself (a style theme's: Luna, Aqua…); greyed for a theme without
                "The &theme's own icons",
                Cmd::ToolbarIcons(crate::icons::ToolbarIcons::Own),
                crate::icons::theme_has_toolbar_icons(),
                style == crate::icons::ToolbarIcons::Own,
            ),
            Sep,
            // i18n: View → Toolbar: opens a window choosing which buttons the toolbar shows, and their order
            I("&Customize...", Cmd::CustomizeToolbar, true, false),
        ];
        // i18n: noun: the toolbar (submenu of View with its settings)
        Sub("Toolbar", true, items)
    }

    /// View → Tray icon: the notification-area icon in full colour (3D style), as the flat mark
    /// in the taskbar's ink (Classic style), or not shown (Don't show: closing the window leaves
    /// it in the taskbar or the Dock, `tray`); the download speed in its tooltip, and on Windows
    /// the progress on the taskbar button.
    pub(super) fn tray_menu(&self) -> MenuEntry<Cmd> {
        use crate::tray::TrayStyle;
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let s = &self.settings;
        let style = s.tray_icon.then_some(s.tray_style);
        #[allow(unused_mut)]
        let mut items = vec![
            Radio(
                // i18n: View → Tray icon: DDM's icon in the notification area in full colour, the default (a style's name)
                "&3D style",
                Cmd::TrayIcon(Some(TrayStyle::ThreeD)),
                true,
                style == Some(TrayStyle::ThreeD),
            ),
            Radio(
                // i18n: View → Tray icon: DDM's icon in the notification area flat, in one colour that suits the taskbar (a style's name)
                "&Flat style",
                Cmd::TrayIcon(Some(TrayStyle::Classic)),
                true,
                style == Some(TrayStyle::Classic),
            ),
            Radio(
                // i18n: View → Tray icon: no icon in the notification area (closing the window minimizes it to the taskbar)
                "&Don't show",
                Cmd::TrayIcon(None),
                true,
                style.is_none(),
            ),
            Sep,
            I(
                // i18n: View → Tray icon: a check item; the tray icon's tooltip says how many downloads run and how fast, and each one's progress
                "Show download s&peed in the tray tooltip",
                Cmd::ToggleTraySpeed,
                s.tray_icon,
                s.tray_speed,
            ),
            I(
                // i18n: View → DDM tray icon (and the tray's menu): a check item; a small, quiet notification when a download completes
                "&Notify when downloads complete",
                Cmd::ToggleNotify,
                // Windows' notifications come from the icon; elsewhere the desktop's own.
                s.tray_icon || cfg!(not(windows)),
                s.notify_complete,
            ),
        ];
        // The taskbar button's green bar is Windows' (ITaskbarList3, win32.rs): macOS's Dock and
        // Linux's docks have no common way to show a program's progress, so there is no item.
        #[cfg(windows)]
        items.push(I(
            // i18n: View → Tray icon: a check item; every running download's progress as one bar on DDM's taskbar button (Windows)
            "Show progress on the &taskbar button",
            Cmd::ToggleTaskbarProgress,
            true,
            s.taskbar_progress,
        ));
        // i18n: View menu: submenu with the notification-area (tray) icon's settings
        Sub("DDM &tray icon", true, items)
    }

    /// The toolbar's icons for the theme in use: those chosen for it (View → Toolbar, the theme
    /// picker), else its own when it brings some, else the line or 3D style last chosen.
    pub(super) fn toolbar_style(&self) -> crate::icons::ToolbarIcons {
        crate::look::look_of(&self.settings.theme_looks, &self.settings.theme)
            .and_then(|l| l.toolbar)
            .unwrap_or_else(|| {
                if crate::icons::theme_has_toolbar_icons() {
                    crate::icons::ToolbarIcons::Own
                } else {
                    self.settings.toolbar_icons
                }
            })
    }

    /// The theme picker (View → Theme → Choose a theme...), while open: what is chosen in it
    /// runs as the menus' commands do.
    pub(super) fn theme_picker(&mut self, ctx: &egui::Context) {
        use crate::themepick::{Act, Mode};
        let Some(mut picker) = self.theme_picker.take() else {
            return;
        };
        let s = &self.settings;
        let mode = if s.always_dark {
            Mode::Dark
        } else if s.dark_mode_support {
            Mode::System
        } else {
            Mode::Light
        };
        let state = crate::themepick::State {
            theme: &s.theme,
            mode,
            looks: &s.theme_looks,
            toolbar: self.toolbar_style(),
            own_icons: crate::icons::theme_has_toolbar_icons(),
            font: s.list_font,
            density: s.density,
            size: crate::scale::of_zoom(self.zoom),
        };
        let mut acts = Vec::new();
        let open = picker.show(ctx, &state, &mut acts);
        if open {
            self.theme_picker = Some(picker);
        }
        for act in acts {
            let cmd = match act {
                Act::Theme(id) => Cmd::Theme(id),
                Act::Mode(m) => Cmd::Mode(m),
                Act::Accent(a) => Cmd::Accent(a),
                Act::Toolbar(t) => Cmd::ToolbarIcons(t),
                Act::Font(f) => Cmd::ListFont(f),
                Act::Density(d) => Cmd::Density(d),
                Act::Size(p) => Cmd::InterfaceSize(p),
                Act::Import => Cmd::AddThemeFile,
                Act::Export => Cmd::ExportTheme,
                Act::Folder => Cmd::OpenThemesFolder,
            };
            self.run(cmd, None, ctx);
        }
    }

    /// View → Text (Windows): how the letters are drawn. Smooth is DirectWrite's natural look
    /// (any size, each letter where it falls: easy to read on a dense screen); Classic is the
    /// hinted whole-pixel look of Windows' old controls. Applies at once.
    #[cfg(windows)]
    pub(super) fn text_menu(&self) -> MenuEntry<Cmd> {
        let smooth = self.settings.smooth_text;
        // i18n: View → Text: letters drawn smooth, as Windows' modern apps draw them, anti-aliased at any size
        let a = MenuEntry::Radio("&Smooth", Cmd::SmoothText(true), true, smooth);
        // i18n: View → Text: letters drawn the classic way, hinted to whole pixels (crisp, coarse on a dense screen)
        let b = MenuEntry::Radio("&Classic", Cmd::SmoothText(false), true, !smooth);
        // i18n: View menu: submenu choosing how the text is drawn (smooth or classic)
        MenuEntry::Sub("&Text", true, vec![a, b])
    }

    /// View → Draw with (#89, #206): OpenGL or the system's own graphics API (Direct3D on
    /// Windows, Vulkan on Linux, Metal on macOS), used from the next start. Greyed lines say when
    /// what draws now isn't the choice: the system's API failed here (and why), or the choice
    /// waits for the next start.
    #[cfg(gpu)]
    pub(super) fn draw_with_menu(&self) -> MenuEntry<Cmd> {
        use crate::renderer::{self, Api, Why};
        use MenuEntry::{Item as I, Radio, Sep, Sub};
        let off = |label| I(label, Cmd::None, false, false);
        let chosen = renderer::chosen(self.settings.renderer);
        let gpu = renderer::GPU.unwrap_or(Api::OpenGl);
        let on = chosen == gpu;
        let draw = Cmd::DrawWith(gpu);
        // Its choice, and the note when it failed here.
        let (choice, failed_note) = match gpu {
            Api::Direct3D => (
                // i18n: View → Draw with: a graphics system's name (Microsoft's), usually kept as is
                Radio("Direct3D", draw, true, on),
                // i18n: View → Draw with: a greyed note; Direct3D failed on this computer, so OpenGL draws the windows
                off("Direct3D didn't start here: OpenGL draws"),
            ),
            Api::Vulkan => (
                // i18n: View → Draw with: a graphics system's name (Khronos', used on Linux), usually kept as is
                Radio("Vulkan", draw, true, on),
                // i18n: View → Draw with: a greyed note; Vulkan failed on this computer, so OpenGL draws the windows
                off("Vulkan didn't start here: OpenGL draws"),
            ),
            _ => (
                // i18n: View → Draw with: a graphics system's name (Apple's), usually kept as is
                Radio("Metal", draw, true, on),
                // i18n: View → Draw with: a greyed note; Metal failed on this computer, so OpenGL draws the windows
                off("Metal didn't start here: OpenGL draws"),
            ),
        };
        let mut items = vec![
            // i18n: View → Draw with: a graphics system's name, usually kept as is
            Radio(
                "OpenGL",
                Cmd::DrawWith(Api::OpenGl),
                true,
                chosen == Api::OpenGl,
            ),
            choice,
        ];
        let failed = renderer::gpu_failed().filter(|_| on);
        if renderer::active().is_some_and(|a| a != chosen) {
            items.push(Sep);
            let last = match failed {
                Some(why) => {
                    items.push(failed_note);
                    // Why, under it ("it": Direct3D, Vulkan or Metal).
                    match why {
                        // i18n: View → Draw with: a greyed note under "… didn't start here"; why: no graphics card here has a driver for that graphics system ("it")
                        Why::NoAdapter => off("No graphics card here has a driver for it"),
                        // i18n: View → Draw with: a greyed note under "… didn't start here"; why: the window stayed empty, so DDM started again with OpenGL
                        Why::NoFirstFrame => off("Its first frame didn't show within 10 seconds"),
                        // i18n: View → Draw with: a greyed note under "… didn't start here"; why: DDM crashed or was closed while starting with it
                        Why::EndedEarly => off("Its last start ended before its first frame"),
                        // i18n: View → Draw with: a greyed note under "… didn't start here"; why: the graphics card stopped working twice (a driver update or reset)
                        Why::LostTwice => off("The graphics card was lost twice in 10 minutes"),
                        // i18n: View → Draw with: a greyed note under "… didn't start here"; why: another error, written in DDM's log
                        Why::Other => off("It couldn't start: the log says why"),
                    }
                }
                // i18n: View → Draw with: a greyed note under the choices; the choice takes effect when DDM is started again
                None => off("Applies the next time DDM starts"),
            };
            items.push(last);
        }
        // i18n: View menu: submenu choosing what draws DDM's windows: OpenGL, or Direct3D, Vulkan or Metal (graphics systems)
        Sub("Draw with", true, items)
    }

    /// View → Interface size (#262): its keys in the main window (the dialogs take them too), and
    /// a new size, from the menu or the keys in any window, saved; the windows that don't follow
    /// it by themselves are made to: the main window's minimum size, the drop target (kept where
    /// it is on the screen). The main window is brought inside the screen once after the start
    /// and after each change (a large size on a small screen), in the frame after it: egui
    /// measures the window at the old size in the frame a new one applies. False in that frame.
    pub(super) fn follow_zoom(&mut self, ctx: &egui::Context) -> bool {
        const MIN: egui::Vec2 = egui::vec2(600.0, 300.0);
        crate::scale::keys(ctx);
        let zoom = ctx.zoom_factor();
        let settled = zoom == self.zoom;
        if settled && !self.fitted && ctx.input(|i| i.viewport().outer_rect).is_some() {
            self.fitted = true;
            crate::scale::fit_window(ctx, MIN, None);
        }
        // Saved whatever changed it, and kept when Options, opened before, hands back its copy
        // of the settings.
        let percent = crate::scale::of_zoom(zoom);
        if crate::scale::test_zoom().is_none() && self.settings.interface_size != percent {
            self.settings.interface_size = percent;
            self.dirty = true;
        }
        if settled {
            return true;
        }
        let ratio = self.zoom / zoom;
        self.zoom = zoom;
        self.fitted = false;
        tracing::info!("interface size {percent}%");
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(MIN));
        if let Some((x, y)) = self.settings.drop_target_pos {
            let at = |v: i32| (v as f32 * ratio).round() as i32;
            self.settings.drop_target_pos = Some((at(x), at(y)));
        }
        if let Some(d) = &mut self.drop_target {
            d.rescale(ctx, ratio);
        }
        false
    }

    /// View → Theme → Add a theme file, answered: the file read and copied into the themes
    /// folder beside the window (#743: it may be on a drive slow to answer), then picked
    /// ([`Self::theme_added`]).
    pub(super) fn add_theme_file(&mut self, path: &std::path::Path) {
        let path = path.to_owned();
        self.ask_beside(move || Beside::ThemeAdded {
            added: crate::themefile::add(&path),
            changed: crate::themefile::changed(),
            list: crate::themefile::list(),
        });
    }

    /// A theme file added (`added`: its id), and the themes folder then (`changed`, `list`):
    /// picked, as clicking it in View → Theme does; a file that isn't a theme says why.
    pub(super) fn theme_added(
        &mut self,
        added: Result<String, String>,
        changed: Option<std::time::SystemTime>,
        list: Vec<crate::themefile::Entry>,
        ctx: &egui::Context,
    ) {
        match added {
            Ok(id) => {
                self.themes_changed = changed;
                self.theme_list = list;
                self.run(Cmd::Theme(crate::chrome::label(&id)), None, ctx);
            }
            Err(e) => {
                tracing::warn!("add a theme file: {e}");
                self.notice = Some((
                    // i18n: title of the message when View → Theme → Add a theme file fails
                    crate::i18n::mark("Add a theme file"),
                    crate::i18n::fmt(
                        // i18n: {error} is why, in English (a file that isn't a theme or a colour scheme, a colour missing)
                        crate::i18n::t("This file can't be used as a theme: {error}"),
                        &[("error", &e)],
                    ),
                ));
            }
        }
    }
}

/// View → Theme's files as they are now: the themes folder's time and whether pywal's colours
/// are there; the list read again when they aren't as `known`. Looks at the disk: for a thread
/// other than the window's (#743).
pub(super) fn themes_seen(known: (Option<std::time::SystemTime>, bool)) -> Beside {
    let changed = crate::themefile::changed();
    let wal = crate::themefile::pywal_file().is_some_and(|p| p.is_file());
    let list = ((changed, wal) != known).then(crate::themefile::list);
    Beside::Themes { changed, wal, list }
}
