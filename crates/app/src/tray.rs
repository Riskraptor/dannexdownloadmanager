//! The notification-area icon (IDM: "IDM menu pops up when you right click on IDM icon"
//! [IDM-doc]). Left click shows the window again, or hides it there while it shows; right click
//! shows the menu. The menu's
//! contents are ours ([unknown] IDM's exact items, lab). Windows, macOS (menu bar) and Linux
//! (StatusNotifierItem over D-Bus: KDE, GNOME with its AppIndicator extension).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

mod watch;

pub use watch::Icon;

use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// View → DDM tray icon: how the notification-area icon looks while it is shown (Don't show is
/// `Settings::tray_icon` off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub enum TrayStyle {
    /// The app icon in full colour, its hand-tuned drawing for the tray's size (the default).
    #[default]
    #[serde(rename = "3d")]
    ThreeD,
    /// The mark alone, flat, in one colour for the taskbar: white on a dark one, dark on a light
    /// one (Windows: the taskbar's own light or dark setting; Linux: the panel's; macOS: a
    /// template image, which the menu bar inks itself).
    #[serde(rename = "classic")]
    Classic,
}

/// A settings file's style; one this DDM doesn't know (a later one's) reads as the default.
impl<'de> serde::Deserialize<'de> for TrayStyle {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let saved = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
        Ok(match saved.as_str() {
            Some("classic") => TrayStyle::Classic,
            _ => TrayStyle::ThreeD,
        })
    }
}

/// The running downloads, as the icon shows them: a slim bar along its foot, filled as far as
/// they are together (`Filled`, in pixels), or with a short piece going across in four steps,
/// two a second, while no size is known (`Sweep`). Changed only when the bar's pixels do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    None,
    Filled(u32),
    Sweep(u32),
}

/// What the icon's picture is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub style: TrayStyle,
    /// The picture's side in pixels: the tray's icon size at its screen's scale.
    pub side: u32,
    /// Classic: the bar it sits on is dark (the mark white) rather than light.
    pub dark_bar: bool,
    /// Classic, macOS: a template image (black, which the menu bar inks itself).
    pub template: bool,
    pub bar: Bar,
}

/// The bar's place in a picture `side` pixels square: its left and right (end excluded), its
/// top and bottom rows (end excluded). Along the foot, inside the tile.
fn bar_rect(side: u32) -> (u32, u32, u32, u32) {
    let h = (side / 10).max(2);
    let x0 = (side / 8).max(2);
    let y1 = side - (side / 16).max(1);
    (x0, side - x0, y1 - h, y1)
}

impl Bar {
    /// The bar for `activity` (None: nothing runs; Some(None): downloads run, no size known;
    /// Some(Some(done)): their part done) in a picture `side` pixels square; `since` how long the
    /// app has run, for the sweep's step.
    pub fn of(activity: Option<Option<f64>>, side: u32, since: std::time::Duration) -> Bar {
        let (x0, x1, ..) = bar_rect(side);
        match activity {
            None => Bar::None,
            Some(Some(done)) => {
                Bar::Filled((done.clamp(0.0, 1.0) * f64::from(x1 - x0)).round() as u32)
            }
            Some(None) => Bar::Sweep(((since.as_millis() / 500) % 4) as u32),
        }
    }
}

/// The icon's picture for `look`, unpremultiplied RGBA, `look.side` pixels square.
pub fn picture_rgba(look: Look) -> Vec<u8> {
    const INK_LIGHT: [u8; 3] = [255, 255, 255];
    const INK_DARK: [u8; 3] = [0x1b, 0x1b, 0x1f];
    let side = look.side.max(8);
    let ink = if look.template {
        [0, 0, 0]
    } else if look.dark_bar {
        INK_LIGHT
    } else {
        INK_DARK
    };
    let mut px = match look.style {
        TrayStyle::ThreeD => crate::logo::rgba(side),
        TrayStyle::Classic => crate::logo::mark_rgba(side, ink),
    };
    let (x0, x1, y0, y1) = bar_rect(side);
    let (track, fill): ([u8; 4], [u8; 4]) = match look.style {
        // On the night tile: a dim indigo groove (lighter than the sky, so what is left to do
        // shows), filled in the comet's amber.
        TrayStyle::ThreeD => ([0x4a, 0x58, 0xa8, 0xff], [0xff, 0xb2, 0x3e, 0xff]),
        // In the mark's one ink: faint for the track, whole for the part done.
        TrayStyle::Classic => (
            [ink[0], ink[1], ink[2], 0x60],
            [ink[0], ink[1], ink[2], 0xff],
        ),
    };
    let filled = |x: u32| -> bool {
        let w = x1 - x0;
        match look.bar {
            Bar::None => false,
            Bar::Filled(n) => x - x0 < n,
            Bar::Sweep(step) => {
                let piece = (w / 3).max(2);
                let at = step.min(3) * (w - piece) / 3;
                (at..at + piece).contains(&(x - x0))
            }
        }
    };
    if look.bar != Bar::None {
        let mut put = |x: u32, y: u32, c: [u8; 4]| {
            let i = ((y * side + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&c);
        };
        if look.style == TrayStyle::Classic {
            // The mark stops a pixel above the bar: one ink, so they'd run into each other.
            for y in y0.saturating_sub(1)..side {
                for x in 0..side {
                    put(x, y, [ink[0], ink[1], ink[2], 0]);
                }
            }
        }
        for y in y0..y1 {
            for x in x0..x1 {
                put(x, y, if filled(x) { fill } else { track });
            }
        }
    }
    px
}

/// What the tray asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Show,
    /// Left click on the icon: the window back when hidden or behind other
    /// windows, hidden there when frontmost.
    Toggle,
    AddUrl,
    Batch,
    LimiterOn,
    LimiterOff,
    Scheduler,
    StartQueue,
    StopQueue,
    Options,
    Exit,
    /// "Downloads in progress" → a download: its progress window, shown again (#63).
    ShowDownload(u64),
}

/// Whether a tray shows DDM's icon at all, so hiding a window there can be undone. Windows and
/// macOS (the menu bar) always have one; on Linux only when a StatusNotifierItem host runs (#59),
/// asked each time: it may come and go.
pub fn available() -> bool {
    #[cfg(target_os = "linux")]
    return crate::unix::tray_host();
    #[cfg(not(target_os = "linux"))]
    true
}

/// What closing the main window does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnClose {
    /// Hidden in the notification area (the menu bar on macOS); downloads go on.
    Hide,
    /// Left in the taskbar (the Dock on macOS), minimized: the tray has no icon to come back
    /// from; downloads go on.
    Minimize,
    Quit,
}

/// What closing the main window does: `keep_running` is Options' "Keep running when closing the
/// window", `tray` whether a tray object exists, `icon` what the shell last said of its icon
/// ([`Tray::icon`]; Linux: there when a tray host runs), `taskbar` whether a window can wait in the
/// taskbar or the Dock instead (not Linux: #59, a window there that merely minimized could never
/// be closed).
///
/// Never a hidden window with no icon to bring it back: the taskbar (or quitting) only when the
/// shell said the icon is gone. No answer, or an old one, hides the window as an icon that is
/// there does (#750, #718, #754: a shell that couldn't confirm the icon left the window in the
/// taskbar with the icon right there); such a window still comes back from the icon once the
/// shell answers, and a second start of DDM always brings it back.
pub fn on_close(keep_running: bool, tray: bool, icon: Icon, taskbar: bool) -> OnClose {
    if !(keep_running && tray) {
        OnClose::Quit
    } else if icon != Icon::Gone {
        OnClose::Hide
    } else if taskbar {
        OnClose::Minimize
    } else {
        OnClose::Quit
    }
}

/// The longest download name in the tray menu, in characters (the rest ends in "…").
const MENU_NAME_MAX: usize = 48;

pub struct Tray {
    limiter_on: CheckMenuItem,
    limiter_off: CheckMenuItem,
    /// Start and Stop main download queue: one of them enabled, as the queue runs.
    start_queue: MenuItem,
    stop_queue: MenuItem,
    queue_running: std::cell::Cell<Option<bool>>,
    /// The icon itself, kept for as long as the tray (Windows asks and tells the shell about it
    /// from the watcher's thread, through its hidden window).
    #[cfg_attr(windows, allow(dead_code))]
    tray: TrayIcon,
    /// The tip shown and when, so the shell is told at most once a second.
    tip: std::cell::RefCell<(String, Option<std::time::Instant>)>,
    rx: Receiver<TrayCmd>,
    /// "Downloads in progress": one entry per running download, rebuilt when they change.
    downloads: Submenu,
    shown_downloads: std::cell::RefCell<Vec<(u64, String)>>,
    download_ids: Arc<Mutex<Vec<(MenuId, u64)>>>,
    /// Every entry with its English text, to translate again when the language changes.
    labels: Vec<(Entry, &'static str)>,
    /// What [`Tray::icon`] last saw and when: the minimize path asks every frame, and the menu
    /// bar shouldn't be asked that often (a second of staleness is enough).
    #[cfg(target_os = "macos")]
    visible_checked: std::cell::Cell<Option<(Icon, std::time::Instant)>>,
    /// The menu bar's last answer, logged when it changes.
    #[cfg(target_os = "macos")]
    answer: std::cell::Cell<Option<bool>>,
    /// The thread that asks the shell about the icon and sets its tip (#747: never the window's).
    #[cfg(windows)]
    watch: watch::Watch,
    /// The last answer the window took from it, for a frame where the watcher is busy writing
    /// the next.
    #[cfg(windows)]
    seen: std::cell::Cell<Option<watch::Seen>>,
    /// The picture's look as last given to the shell.
    look: std::cell::Cell<Look>,
    /// The taskbar's icon size and whether it is dark, as last asked (every 2 s: the user can
    /// change either at any time), and when.
    place: std::cell::Cell<(u32, bool, std::time::Instant)>,
    /// When the icon was made: the sweep's clock.
    born: std::time::Instant,
    ctx: egui::Context,
}

/// A menu entry whose text is translated.
enum Entry {
    Item(MenuItem),
    Check(CheckMenuItem),
    Sub(Submenu),
}

impl Tray {
    /// What the shell says of the icon, as the window may act on it: closing, Minimize to the
    /// notification area and the start with the session ask it every frame. Windows: the last
    /// answer of the watcher's thread, never waiting for the shell; macOS: the menu bar item's
    /// frame (AppKit's, asked on the window's thread), a second old at most. Linux has no such
    /// question: its [`available`] stands for it.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub fn icon(&self) -> Icon {
        if let Some(v) = test_visible() {
            return v;
        }
        #[cfg(windows)]
        return watch::icon(self.seen(), std::time::Instant::now());
        #[cfg(target_os = "macos")]
        {
            if let Some((icon, checked)) = self.visible_checked.get()
                && checked.elapsed() < std::time::Duration::from_secs(1)
            {
                return icon;
            }
            self.icon_now()
        }
    }

    /// [`Tray::icon`], for closing: on macOS without the one-second cache (an icon gone since must
    /// not hide the window); on Windows the same last answer, whose age it weighs.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub fn icon_now(&self) -> Icon {
        if let Some(v) = test_visible() {
            return v;
        }
        #[cfg(windows)]
        return self.icon();
        #[cfg(target_os = "macos")]
        {
            // MDM_TEST_TRAY_RECT=0: the menu bar gives the item no place.
            let placed = self.tray.rect().is_some()
                && std::env::var_os("MDM_TEST_TRAY_RECT").is_none_or(|v| v != "0");
            if self.answer.replace(Some(placed)) != Some(placed) {
                tracing::info!("tray icon: placed {placed}");
            }
            let icon = if placed { Icon::There } else { Icon::Gone };
            self.visible_checked
                .set(Some((icon, std::time::Instant::now())));
            icon
        }
    }

    /// Whether a window hidden in the notification area should come back to the taskbar: the
    /// shell, running and answering, has said for some seconds that it has no icon of DDM's
    /// (`watch::rescue`). Never on no answer. Windows only: macOS leaves a window hidden in the
    /// menu bar as it was.
    #[cfg(windows)]
    pub fn rescue(&self) -> bool {
        test_visible().is_none() && watch::rescue(self.seen(), std::time::Instant::now())
    }

    /// The watcher's last answer, or the one the window took before while it is busy writing.
    #[cfg(windows)]
    fn seen(&self) -> Option<watch::Seen> {
        if let Some(seen) = self.watch.latest() {
            self.seen.set(Some(seen));
        }
        self.seen.get()
    }

    /// The icon and its menu, in `style`; events wake `ctx`. None if the shell refuses the icon
    /// (logged).
    pub fn new(ctx: &egui::Context, limiter: bool, style: TrayStyle) -> Option<Tray> {
        let menu = Menu::new();
        let mut items = Vec::new();
        let mut labels = Vec::new();
        // Labels are translated here and again by `relabel` when the language changes.
        let mut add = |m: &Menu, label: &'static str, cmd: TrayCmd| {
            let item = MenuItem::new(crate::i18n::t(label), true, None);
            items.push((item.id().clone(), cmd));
            m.append(&item).ok();
            labels.push((Entry::Item(item.clone()), label));
            item
        };
        add(&menu, crate::i18n::mark("Open DDM"), TrayCmd::Show);
        menu.append(&PredefinedMenuItem::separator()).ok();
        let mark = crate::i18n::mark;
        add(&menu, mark("Add new download"), TrayCmd::AddUrl);
        add(&menu, mark("Add batch download"), TrayCmd::Batch);
        // i18n: tray menu: a submenu of the downloads now running; choosing one shows its progress window
        let downloads_label = mark("Downloads in progress");
        let downloads = Submenu::new(crate::i18n::t(downloads_label), false);
        menu.append(&downloads).ok();
        menu.append(&PredefinedMenuItem::separator()).ok();
        let limiter_label = mark("Speed Limiter");
        let limiter_menu = Submenu::new(crate::i18n::t(limiter_label), true);
        // i18n: verb: turn the Speed Limiter on
        let on_label = mark("Turn on");
        // i18n: verb: turn the Speed Limiter off
        let off_label = mark("Turn off");
        let limiter_on = CheckMenuItem::new(crate::i18n::t(on_label), true, limiter, None);
        let limiter_off = CheckMenuItem::new(crate::i18n::t(off_label), true, !limiter, None);
        limiter_menu.append(&limiter_on).ok();
        limiter_menu.append(&limiter_off).ok();
        menu.append(&limiter_menu).ok();
        // i18n: noun: opens the Scheduler (the tool that starts queues on time)
        add(&menu, mark("Scheduler"), TrayCmd::Scheduler);
        let start_queue = add(
            &menu,
            mark("Start main download queue"),
            TrayCmd::StartQueue,
        );
        let stop_queue = add(&menu, mark("Stop main download queue"), TrayCmd::StopQueue);
        // i18n: plural noun: opens the Options window (settings)
        add(&menu, mark("Options"), TrayCmd::Options);
        menu.append(&PredefinedMenuItem::separator()).ok();
        // i18n: verb: quit the app
        add(&menu, mark("Exit"), TrayCmd::Exit);
        drop(add);
        items.push((limiter_on.id().clone(), TrayCmd::LimiterOn));
        items.push((limiter_off.id().clone(), TrayCmd::LimiterOff));

        let (side, dark_bar) = place(ctx);
        let look = Look {
            style,
            side,
            dark_bar,
            template: cfg!(target_os = "macos") && style == TrayStyle::Classic,
            bar: Bar::None,
        };
        let icon = match picture(look) {
            Ok(icon) => icon,
            Err(e) => {
                tracing::warn!("tray icon: no picture: {e}");
                return None;
            }
        };
        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(crate::MAIN_TITLE);
        // macOS: the classic mark as a template image, which the menu bar inks for its look.
        #[cfg(target_os = "macos")]
        let builder = if look.template {
            builder.with_icon_templated(icon)
        } else {
            builder.with_icon(icon)
        };
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icon);
        let tray = match builder.build() {
            Ok(tray) => tray,
            Err(e) => {
                tracing::warn!("tray icon: not made: {e}");
                return None;
            }
        };

        // Only the hidden window's handle crosses to the watcher's thread: TrayIcon stays here.
        #[cfg(windows)]
        let watch = {
            let wake = ctx.clone();
            watch::Watch::new(
                WindowsShell {
                    hwnd: tray.window_handle() as isize,
                    id: None,
                    // MDM_TEST_TRAY_RECT=0: the shell gives the icon no place, as the reporters'
                    // did for one behind ^ (the close test, with a real icon the shell holds).
                    rect_withheld: std::env::var_os("MDM_TEST_TRAY_RECT").is_some_and(|v| v == "0"),
                    logged: None,
                },
                move || wake.request_repaint(),
            )
        };

        let (tx, rx) = channel();
        let wake = |tx: &Sender<TrayCmd>, ctx: &egui::Context| (tx.clone(), ctx.clone());
        let (icon_tx, icon_ctx) = wake(&tx, ctx);
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                let _ = icon_tx.send(TrayCmd::Toggle);
                icon_ctx.request_repaint();
            }
        }));
        let (menu_tx, menu_ctx) = wake(&tx, ctx);
        let download_ids: Arc<Mutex<Vec<(MenuId, u64)>>> = Arc::default();
        let ids = download_ids.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let cmd = items
                .iter()
                .find(|(id, _): &&(MenuId, TrayCmd)| *id == e.id)
                .map(|(_, c)| *c)
                .or_else(|| {
                    ids.lock()
                        .ok()?
                        .iter()
                        .find(|(id, _)| *id == e.id)
                        .map(|(_, d)| TrayCmd::ShowDownload(*d))
                });
            if let Some(cmd) = cmd {
                let _ = menu_tx.send(cmd);
                menu_ctx.request_repaint();
            }
        }));
        labels.push((Entry::Sub(limiter_menu), limiter_label));
        labels.push((Entry::Sub(downloads.clone()), downloads_label));
        labels.push((Entry::Check(limiter_on.clone()), on_label));
        labels.push((Entry::Check(limiter_off.clone()), off_label));
        Some(Tray {
            tray,
            tip: Default::default(),
            limiter_on,
            limiter_off,
            start_queue,
            stop_queue,
            queue_running: std::cell::Cell::new(None),
            rx,
            downloads,
            shown_downloads: Default::default(),
            download_ids,
            labels,
            #[cfg(target_os = "macos")]
            visible_checked: Default::default(),
            #[cfg(target_os = "macos")]
            answer: Default::default(),
            #[cfg(windows)]
            watch,
            #[cfg(windows)]
            seen: Default::default(),
            look: std::cell::Cell::new(look),
            place: std::cell::Cell::new((side, dark_bar, std::time::Instant::now())),
            born: std::time::Instant::now(),
            ctx: ctx.clone(),
        })
    }

    /// Keep the icon's picture in step: View → DDM tray icon's style, the taskbar's icon size and
    /// light or dark (asked every 2 s), and the running downloads' bar (`activity`: None when
    /// none runs, else their part done when their sizes are known). The shell is told only when
    /// the picture changes: when the bar gains a pixel, or, with no size known, twice a second.
    /// Windows: through the watcher's thread, as the tip (#747).
    pub fn set_look(&self, style: TrayStyle, activity: Option<Option<f64>>) {
        let (mut side, mut dark_bar, asked) = self.place.get();
        if asked.elapsed() >= std::time::Duration::from_secs(2) {
            (side, dark_bar) = place(&self.ctx);
            self.place.set((side, dark_bar, std::time::Instant::now()));
        }
        let bar = Bar::of(activity, side, self.born.elapsed());
        if matches!(bar, Bar::Sweep(_)) {
            self.ctx
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        let look = Look {
            style,
            side,
            dark_bar,
            template: cfg!(target_os = "macos") && style == TrayStyle::Classic,
            bar,
        };
        if look == self.look.get() {
            return;
        }
        #[cfg(windows)]
        let told = self.watch.picture(watch::Picture {
            rgba: picture_rgba(look),
            side: look.side,
        });
        // Elsewhere set here, as the tray's own calls don't wait for the shell; one it refuses
        // is logged and not asked for again until the picture changes.
        #[cfg(not(windows))]
        let told = {
            let set = picture(look).and_then(|icon| {
                #[cfg(target_os = "macos")]
                if look.template {
                    return self
                        .tray
                        .set_icon_templated(Some(icon))
                        .map_err(|e| e.to_string());
                }
                self.tray.set_icon(Some(icon)).map_err(|e| e.to_string())
            });
            if let Err(e) = set {
                tracing::warn!("tray icon: picture not set: {e}");
            }
            true
        };
        if told {
            self.look.set(look);
        }
    }

    /// The commands since the last call.
    pub fn poll(&self) -> Vec<TrayCmd> {
        self.rx.try_iter().collect()
    }

    /// Translate the menu again (the interface language changed).
    pub fn relabel(&self) {
        for (entry, source) in &self.labels {
            let text = crate::i18n::t(source);
            match entry {
                Entry::Item(i) => i.set_text(text),
                Entry::Check(i) => i.set_text(text),
                Entry::Sub(i) => i.set_text(text),
            }
        }
    }

    /// Keep Start / Stop main download queue in step with the queue (the menu is only told
    /// when it changes).
    pub fn set_queue(&self, running: bool) {
        if self.queue_running.replace(Some(running)) != Some(running) {
            self.start_queue.set_enabled(!running);
            self.stop_queue.set_enabled(running);
        }
    }

    /// The icon's tip: the app's name, and while downloads run how many and how fast.
    pub fn set_tooltip(&self, tip: &str) {
        let mut last = self.tip.borrow_mut();
        let due = last
            .1
            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1));
        if last.0 != tip && due {
            // Windows: the watcher's thread sets it, as setting it waits for Explorer (#747).
            #[cfg(windows)]
            if !self.watch.tip(tip) {
                return;
            }
            #[cfg(not(windows))]
            let _ = self.tray.set_tooltip(Some(tip));
            *last = (tip.to_owned(), Some(std::time::Instant::now()));
        }
    }

    /// "Downloads in progress": the running downloads (id, name), the menu rebuilt only when they
    /// change; greyed when none runs.
    pub fn set_downloads(&self, running: &[(u64, String)]) {
        if *self.shown_downloads.borrow() == running {
            return;
        }
        for _ in 0..self.downloads.items().len() {
            let _ = self.downloads.remove_at(0);
        }
        let mut ids = Vec::with_capacity(running.len());
        for (id, name) in running {
            let item = MenuItem::new(short(name), true, None);
            ids.push((item.id().clone(), *id));
            self.downloads.append(&item).ok();
        }
        if let Ok(mut d) = self.download_ids.lock() {
            *d = ids;
        }
        self.downloads.set_enabled(!running.is_empty());
        *self.shown_downloads.borrow_mut() = running.to_vec();
    }

    /// Keep the Speed Limiter checks in step with the app.
    pub fn set_limiter(&self, on: bool) {
        self.limiter_on.set_checked(on);
        self.limiter_off.set_checked(!on);
    }
}

/// What the tests say of the icon, as off-screen tests can't make the shell answer reliably
/// (MDM_TEST_TRAY_VISIBLE: 1 there, anything else gone; the startup and close tests use it with
/// MDM_TEST_TRAY).
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn test_visible() -> Option<Icon> {
    std::env::var_os("MDM_TEST_TRAY_VISIBLE")
        .map(|v| if v == "1" { Icon::There } else { Icon::Gone })
}

/// Windows' shell, as the watcher's thread asks it: the tray's hidden window (one icon) and the
/// icon's id once found.
#[cfg(windows)]
struct WindowsShell {
    hwnd: isize,
    id: Option<u32>,
    rect_withheld: bool,
    /// The last picture's side and whether the shell took it, as logged.
    logged: Option<(bool, u32)>,
}

#[cfg(windows)]
impl watch::Port for WindowsShell {
    fn ask(&mut self) -> watch::Shell {
        use crate::win32::TrayAnswer;
        match crate::win32::tray_icon_ask(self.hwnd, self.id, self.rect_withheld) {
            TrayAnswer::NoTaskbar => watch::Shell::NoTaskbar,
            TrayAnswer::Silent => watch::Shell::Silent,
            TrayAnswer::Answered { held, placed } => {
                if held.is_some() {
                    self.id = held;
                }
                watch::Shell::Answered {
                    placed,
                    held: held.is_some(),
                }
            }
        }
    }

    fn tip(&mut self, tip: &str) -> bool {
        self.id
            .is_some_and(|id| crate::win32::tray_icon_tip(self.hwnd, id, tip))
    }

    fn picture(&mut self, picture: &watch::Picture) -> bool {
        let taken = self.id.is_some_and(|id| {
            crate::win32::tray_icon_picture(self.hwnd, id, &picture.rgba, picture.side)
        });
        // In the log when it changes (a new size, or the shell taking it or not), not at each
        // step of the downloads' bar.
        if self.logged.replace((taken, picture.side)) != Some((taken, picture.side)) {
            tracing::info!(
                "tray icon: picture of {} px {}",
                picture.side,
                if taken { "set" } else { "not taken" }
            );
        }
        taken
    }
}

/// A download's name as the tray menu shows it: at most MENU_NAME_MAX characters, the rest "…";
/// "&" doubled, which menus would read as a shortcut mark.
fn short(name: &str) -> String {
    let mut s: String = name.chars().take(MENU_NAME_MAX).collect();
    if name.chars().count() > MENU_NAME_MAX {
        s.push('…');
    }
    s.replace('&', "&&")
}

/// The icon's picture for `look` ([`picture_rgba`]), square. Never an empty one: tray-icon takes
/// a 0×0 picture, and macOS then fails to make the menu bar item of it (it encodes the picture as
/// a PNG, which can't be 0 wide), so before #529 macOS had no menu bar icon and closing the
/// window quit.
fn picture(look: Look) -> Result<tray_icon::Icon, String> {
    let rgba = picture_rgba(look);
    let side = look.side.max(8);
    if rgba.len() != (side * side * 4) as usize || !rgba.chunks(4).any(|p| p[3] > 0) {
        return Err(format!("{side}×{side} px, empty"));
    }
    tray_icon::Icon::from_rgba(rgba, side, side).map_err(|e| e.to_string())
}

/// The tray's icon size in pixels and whether what it sits on is dark (for the classic mark's
/// ink). Windows: the notification area's small-icon size at the taskbar's scale, and the
/// taskbar's own light or dark setting ("Choose your default Windows mode"), which can differ
/// from the apps'. Linux: a 32-pixel picture (the panel scales it to its size), white on GNOME's
/// panel, which is always dark, else on a panel as dark as the desktop says. macOS: the menu
/// bar's 18 points at 2×; the classic mark is a template there, inked by the menu bar.
fn place(ctx: &egui::Context) -> (u32, bool) {
    #[cfg(windows)]
    {
        let _ = ctx;
        (crate::win32::tray_icon_px(), !crate::win32::taskbar_light())
    }
    #[cfg(target_os = "linux")]
    {
        let gnome = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| {
            d.split(':').any(|d| {
                matches!(
                    d.to_ascii_lowercase().as_str(),
                    "gnome" | "ubuntu" | "unity"
                )
            })
        });
        (32, gnome || crate::unix::desktop_dark(ctx).unwrap_or(true))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = ctx;
        (36, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The close decision (#750, #718, #754, #545): an icon the shell says is there, or no answer
    /// at all, hides the window; only the shell saying the icon is gone leaves it in the taskbar.
    #[test]
    fn closing_hides_unless_the_shell_says_the_icon_is_gone() {
        for icon in [Icon::There, Icon::Unknown] {
            assert_eq!(on_close(true, true, icon, true), OnClose::Hide, "{icon:?}");
        }
        assert_eq!(on_close(true, true, Icon::Gone, true), OnClose::Minimize);
    }

    /// The rest of the close decision, on every system (#457, #59).
    #[test]
    fn closing_quits_with_the_option_off_no_tray_or_nowhere_to_wait() {
        // No icon and no taskbar to wait in (Linux with no tray host): quit.
        assert_eq!(on_close(true, true, Icon::Gone, false), OnClose::Quit);
        assert_eq!(on_close(true, true, Icon::There, false), OnClose::Hide);
        // The option off, or no tray object at all: closing quits, whatever the shell says.
        for icon in [Icon::There, Icon::Unknown, Icon::Gone] {
            assert_eq!(on_close(false, true, icon, true), OnClose::Quit);
            assert_eq!(on_close(true, false, icon, true), OnClose::Quit);
        }
    }

    #[test]
    fn menu_names_are_shortened_and_keep_their_ampersands() {
        assert_eq!(short("a & b.zip"), "a && b.zip");
        let s = short(&"x".repeat(60));
        assert_eq!(s.chars().count(), MENU_NAME_MAX + 1);
        assert!(s.ends_with('…'));
    }

    fn look(style: TrayStyle, side: u32, bar: Bar) -> Look {
        Look {
            style,
            side,
            dark_bar: true,
            template: false,
            bar,
        }
    }

    /// #529: the tray's picture is a real one on every system, macOS's menu bar included (where
    /// the window icon, `app_icon`, is empty on purpose), in each style, at every size a tray
    /// asks for.
    #[test]
    fn the_tray_picture_is_never_empty() {
        let p = crate::app_picture();
        assert_eq!((p.width, p.height), (256, 256));
        assert_eq!(p.rgba.len(), 256 * 256 * 4);
        assert!(p.rgba.chunks(4).any(|px| px[3] > 0), "a blank picture");
        for style in [TrayStyle::ThreeD, TrayStyle::Classic] {
            for side in [16, 20, 24, 28, 32, 36, 40, 48] {
                assert!(
                    picture(look(style, side, Bar::None)).is_ok(),
                    "{style:?} {side}"
                );
            }
        }
        #[cfg(target_os = "macos")]
        assert_eq!(
            crate::app_icon().width,
            0,
            "the Dock keeps the bundle's icon"
        );
    }

    /// The classic mark is one ink: white where the taskbar is dark, near black where it is
    /// light, black for a macOS template; the 3D style is the app icon's own colours.
    #[test]
    fn the_classic_mark_takes_the_taskbars_ink() {
        let inks = |l: Look| -> Vec<[u8; 3]> {
            let px = picture_rgba(l);
            let mut inks: Vec<[u8; 3]> = px
                .chunks(4)
                .filter(|p| p[3] > 0)
                .map(|p| [p[0], p[1], p[2]])
                .collect();
            inks.sort_unstable();
            inks.dedup();
            inks
        };
        let mut dark = look(TrayStyle::Classic, 16, Bar::None);
        assert_eq!(inks(dark), vec![[255, 255, 255]]);
        dark.dark_bar = false;
        assert_eq!(inks(dark), vec![[0x1b, 0x1b, 0x1f]]);
        dark.template = true;
        assert_eq!(inks(dark), vec![[0, 0, 0]]);
        assert!(inks(look(TrayStyle::ThreeD, 16, Bar::None)).len() > 20);
        // Its 16-pixel picture is the mark drawn for 16 pixels, not a smaller copy of a larger.
        let px = picture_rgba(look(TrayStyle::Classic, 16, Bar::None));
        let mark = crate::logo::mark_rgba(16, [255, 255, 255]);
        assert_eq!(px, mark);
    }

    /// Not a check: writes the tray's pictures (each style, for a dark and a light taskbar, at
    /// each size, idle, half done and with no size known) as PNGs into the folder MDM_TRAY_DUMP
    /// names, to look at (a PR's screenshots).
    #[test]
    #[ignore]
    fn dump_pictures() {
        let Some(dir) = std::env::var_os("MDM_TRAY_DUMP") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for style in [TrayStyle::ThreeD, TrayStyle::Classic] {
            for dark_bar in [true, false] {
                for side in [16, 20, 24, 32] {
                    let half = Bar::of(Some(Some(0.5)), side, std::time::Duration::ZERO);
                    for (what, bar) in [
                        ("idle", Bar::None),
                        ("half", half),
                        ("sweep", Bar::Sweep(1)),
                    ] {
                        let l = Look {
                            style,
                            side,
                            dark_bar,
                            template: false,
                            bar,
                        };
                        let name = format!(
                            "{}-{}-{side}-{what}.png",
                            if style == TrayStyle::ThreeD {
                                "3d"
                            } else {
                                "classic"
                            },
                            if dark_bar { "dark" } else { "light" },
                        );
                        image::save_buffer(
                            dir.join(name),
                            &picture_rgba(l),
                            side,
                            side,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }

    /// The downloads' bar: none while nothing runs; filled as far as they are, in whole pixels
    /// (so the picture changes only when the bar does); with no size known a short piece in one
    /// of four places, a step every half second. Drawn along the foot.
    #[test]
    fn the_bar_shows_the_running_downloads() {
        use std::time::Duration;
        let s = Duration::from_millis;
        assert_eq!(Bar::of(None, 16, s(0)), Bar::None);
        let (x0, x1, y0, y1) = bar_rect(16);
        assert_eq!((x0, x1, y0, y1), (2, 14, 13, 15));
        assert_eq!(Bar::of(Some(Some(0.0)), 16, s(0)), Bar::Filled(0));
        assert_eq!(Bar::of(Some(Some(0.5)), 16, s(0)), Bar::Filled(6));
        assert_eq!(Bar::of(Some(Some(0.52)), 16, s(0)), Bar::Filled(6));
        assert_eq!(Bar::of(Some(Some(1.5)), 16, s(0)), Bar::Filled(12));
        assert_eq!(Bar::of(Some(None), 16, s(0)), Bar::Sweep(0));
        assert_eq!(Bar::of(Some(None), 16, s(499)), Bar::Sweep(0));
        assert_eq!(Bar::of(Some(None), 16, s(500)), Bar::Sweep(1));
        assert_eq!(Bar::of(Some(None), 16, s(2000)), Bar::Sweep(0));
        for side in [16, 20, 24, 32, 40, 48] {
            let (x0, x1, y0, y1) = bar_rect(side);
            assert!(
                x0 >= 2 && x1 <= side - 2 && y1 < side && y1 - y0 >= 2,
                "{side}"
            );
        }
        for style in [TrayStyle::ThreeD, TrayStyle::Classic] {
            let half = picture_rgba(look(style, 16, Bar::Filled(6)));
            let at = |x: u32, y: u32| {
                let i = ((y * 16 + x) * 4) as usize;
                [half[i], half[i + 1], half[i + 2], half[i + 3]]
            };
            // Done to the left of the middle, to do to its right, the same on both rows.
            assert_eq!(at(2, 13), at(7, 14));
            assert_ne!(at(7, 13), at(8, 13), "{style:?}");
            assert_eq!(at(8, 13), at(13, 14));
            assert_eq!(at(7, 13)[3], 0xff);
        }
        // The classic mark stops a pixel above its bar.
        let classic = picture_rgba(look(TrayStyle::Classic, 16, Bar::Filled(6)));
        assert!((0..16).all(|x| classic[((12 * 16 + x) * 4 + 3) as usize] == 0));
        let pieces: Vec<Vec<u8>> = (0..4)
            .map(|step| picture_rgba(look(TrayStyle::ThreeD, 16, Bar::Sweep(step))))
            .collect();
        assert!(pieces.windows(2).all(|w| w[0] != w[1]));
    }
}
