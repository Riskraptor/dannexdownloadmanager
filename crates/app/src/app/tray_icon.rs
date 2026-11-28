//! The notification area: the tray icon and its menu, the window hidden there and shown again, the
//! memory given back while hidden, and where a start with the session puts the window.

use super::*;

/// Started with the session (#226), the window waits hidden until the notification area is
/// known: DDM's own tray icon created and a tray there to show it. A desktop's tray can come
/// after DDM (GNOME's AppIndicator extension at sign-in answered 11.5 s after DDM's icon, on
/// the lab's VM), and DDM's icon after its own first look: this long they are waited for,
/// counted from DDM's icon (from the start while it has none), the window still hidden,
/// before it shows minimized.
pub(super) const TRAY_GRACE: Duration = Duration::from_secs(30);

/// How long after the main window loses focus a tray toggle still counts it as in
/// front (#520): clicking the icon makes the taskbar the foreground window first,
/// so `focused` is always false by the time Toggle runs on Windows. Unknown focus
/// (`None`) never counts: unsure means show, never hide.
pub(super) const TRAY_TOGGLE_GRACE: Duration = Duration::from_millis(500);

/// Where a start with the session (#226) leaves the window, decided here alone.
#[derive(Debug, PartialEq)]
pub(super) enum AtLogin {
    /// Not known yet: hidden, looked at again in a second.
    Wait,
    /// DDM's icon is in a notification area: the window stays hidden there.
    Tray,
    /// No notification area within [`TRAY_GRACE`]: shown, minimized.
    Minimize,
}

impl App {
    /// The main window into the tray: downloads, queues and schedules go on. Wayland can't hide a
    /// window, so there it goes to the dock / task list instead; so it does with no tray icon
    /// wanted (View → Tray icon), as there would be no icon to bring it back.
    pub(super) fn hide_to_tray(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "linux")]
        let wayland = crate::unix::on_wayland();
        #[cfg(not(target_os = "linux"))]
        let wayland = false;
        if wayland || !self.settings.tray_icon {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden = true;
        self.start_trimming();
    }

    /// Hidden, the window's memory sits unused until it shows again: give it back to the system
    /// (#72), on the schedule of `next_trim`. Windows pages it back in from RAM when it's touched
    /// again. Both ways into the tray (closing the window, and Minimize when it goes there).
    pub(super) fn start_trimming(&mut self) {
        self.trims_since_hidden = 0;
        self.trim_when_hidden = Some(Instant::now() + next_trim(0));
    }

    /// The window back from the tray (or from minimized), in front.
    pub(super) fn show_window(&mut self, ctx: &egui::Context) {
        // Asked for (a second launch, the tray icon): no start with the session to settle.
        self.at_login = None;
        self.trim_when_hidden = None;
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        if !crate::offscreen() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// What the shell says of the tray icon (Linux: there when a tray host is, asked each time:
    /// the host may come and go). On Windows and macOS the icon itself is asked: the shell can
    /// accept it and still show nothing, and a window hidden with no icon to click has no way
    /// back. Windows' answer comes from a thread of its own and is never waited for (#747);
    /// macOS's is a second old at most, for the per-frame minimize path.
    pub(super) fn tray_icon(&self) -> crate::tray::Icon {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let Some(tray) = &self.tray {
            return tray.icon();
        }
        Self::tray_host()
    }

    /// [`tray_icon`] for the close decision: on macOS asked afresh, as an icon can disappear
    /// between the cached answer and the close, and hiding then would be a ghost window.
    pub(super) fn tray_icon_now(&self) -> crate::tray::Icon {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let Some(tray) = &self.tray {
            return tray.icon_now();
        }
        Self::tray_host()
    }

    /// Whether a tray shows icons at all, as [`crate::tray::available`] says.
    pub(super) fn tray_host() -> crate::tray::Icon {
        if crate::tray::available() {
            crate::tray::Icon::There
        } else {
            crate::tray::Icon::Gone
        }
    }

    /// Started with the session (#226): once a second, until it is known, where the window goes
    /// ([`at_login`]): it stays hidden in the notification area, or shows minimized.
    pub(super) fn settle_at_login(&mut self, ctx: &egui::Context) {
        let Some(started) = self.at_login else {
            return;
        };
        ctx.request_repaint_after(Duration::from_secs(1));
        if self
            .at_login_checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.at_login_checked = Some(Instant::now());
        if self.tray.is_some() && self.at_login_icon.is_none() {
            self.at_login_icon = Some(Instant::now());
        }
        let icon = self.at_login_icon.map(|t| t.elapsed());
        let since = started.elapsed();
        let shown = icon.is_some() && self.tray_icon() == crate::tray::Icon::There;
        // No tray icon wanted (View → Tray icon): nothing to wait for, the window waits minimized.
        let decided = if self.settings.tray_icon {
            at_login(since, icon, shown)
        } else {
            AtLogin::Minimize
        };
        match decided {
            AtLogin::Wait => {}
            AtLogin::Tray => {
                tracing::info!(
                    "started hidden: in the notification area ({} ms on)",
                    since.as_millis()
                );
                self.at_login = None;
            }
            AtLogin::Minimize => {
                if self.settings.tray_icon {
                    tracing::info!(
                        "started hidden: no notification area within {} s of the icon: minimized ({} ms on)",
                        TRAY_GRACE.as_secs(),
                        since.as_millis()
                    );
                } else {
                    tracing::info!("started hidden: no tray icon (View → Tray icon): minimized");
                }
                self.at_login = None;
                self.trim_when_hidden = None;
                self.hidden = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
    }

    /// The tray icon: created once, its commands run, and closing the window hides it there.
    pub(super) fn tray(&mut self, ctx: &egui::Context) {
        // Off-screen test copies have none, but for MDM_TEST_TRAY=1 (#226's check: a start with
        // the session shows the icon, and no window).
        let wanted =
            !crate::offscreen() || std::env::var_os("MDM_TEST_TRAY").is_some_and(|v| v == "1");
        // View → Tray icon → Show tray icon off: the icon goes (it is made again when it is
        // turned back on), and everything below acts as with no tray: closing and Minimize leave
        // the window in the taskbar.
        if !self.settings.tray_icon {
            if self.tray.take().is_some() {
                tracing::info!("tray icon hidden (View → Tray icon)");
            }
            self.tray_limiter = None;
        }
        let wanted = wanted && self.settings.tray_icon;
        if self.tray.is_none() && self.tray_limiter.is_none() && wanted {
            let s = &self.settings;
            self.tray = crate::tray::Tray::new(
                ctx,
                (s.speed_limit_on, s.speed_limit_kb),
                s.notify_complete,
                s.tray_style,
            );
            self.tray_limiter = Some(self.settings.speed_limit_on);
            if self.tray.is_some() {
                tracing::info!("tray icon shown");
            }
        }
        if let Some(t) = &self.tray {
            let s = &self.settings;
            t.set_limiter(s.speed_limit_on, s.speed_limit_kb);
            t.set_notify(s.notify_complete);
            self.tray_limiter = Some(s.speed_limit_on);
        }
        if let (Some(t), Some(q)) = (&self.tray, self.queues.first()) {
            t.set_queue(q.running);
        }
        // Completed downloads' notification, once they stop coming in quick succession.
        if self.tray_kept.notifier.waiting() {
            if let Some((title, body)) = self.tray_kept.notifier.due(Instant::now()) {
                tracing::info!("notification: {title}");
                match &self.tray {
                    Some(t) => t.notify(&title, &body),
                    // Linux and macOS notify without the icon; Windows' come from it.
                    #[cfg(not(windows))]
                    None => crate::unix::notify(&title, &body),
                    #[cfg(windows)]
                    None => {}
                }
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if let Some(t) = &self.tray {
            // "Downloads in progress": each running download, to show its progress window (#63).
            let live: Vec<&Item> = self
                .items
                .iter()
                .filter(|i| i.live.is_some() && i.listed())
                .collect();
            let running: Vec<(u64, String)> =
                live.iter().map(|i| (i.id, i.file_name.clone())).collect();
            // The name, and (View → Tray icon → Show download speed in the tray tooltip) how many
            // downloads run and how fast, then each with its part done and speed; the shell is
            // told at most once a second (`set_tooltip`).
            let tip = if self.settings.tray_speed {
                let rows: Vec<crate::tray::TipRow> = live
                    .iter()
                    .map(|i| crate::tray::TipRow {
                        name: i.file_name.clone(),
                        done: i.fraction(),
                        rate: i.rate(),
                    })
                    .collect();
                crate::tray::tip_text(crate::MAIN_TITLE, &rows)
            } else {
                crate::MAIN_TITLE.to_owned()
            };
            t.set_tooltip(&tip);
            // Pause all while downloads run; Resume all while some it paused wait.
            let paused = self.tray_kept.paused.iter().any(|id| {
                self.items
                    .iter()
                    .any(|i| i.id == *id && i.live.is_none() && i.can_start())
            }) || !self.tray_kept.paused_queues.is_empty();
            t.set_pause(!running.is_empty(), paused);
            t.set_downloads(&running);
            // View → DDM tray icon's style, and while downloads run a bar along the icon's foot.
            let activity = (!running.is_empty()).then(|| self.overall_progress());
            t.set_look(self.settings.tray_style, activity);
        }
        self.settle_at_login(ctx);
        // Hidden in the notification area, and the shell, running and answering, has said for
        // some seconds that it has no icon of DDM's (lost, and not added back): the window comes
        // back to the taskbar, minimized. Never on no answer, or while Explorer isn't running
        // (it gives the icon back as it starts again).
        #[cfg(windows)]
        if self.hidden && self.at_login.is_none() && self.tray.as_ref().is_some_and(|t| t.rescue())
        {
            tracing::info!(
                "hidden, and the shell has no tray icon of DDM's: back in the taskbar, minimized"
            );
            self.hidden = false;
            self.trim_when_hidden = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        // When the window was last focused, for Toggle below. Kept every frame:
        // by the time a tray click arrives the taskbar already took the focus.
        if ctx.input(|i| i.viewport().focused) == Some(true) {
            self.tray_last_focused = Some(Instant::now());
        }
        let cmds = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for c in cmds {
            use crate::tray::TrayCmd;
            tracing::info!("tray: {c:?}");
            let cmd = match c {
                TrayCmd::Show => {
                    self.show_window(ctx);
                    continue;
                }
                TrayCmd::Toggle => {
                    let minimized = ctx.input(|i| i.viewport().minimized == Some(true));
                    // Behind other windows it still reads as shown: only hide when
                    // frontmost, otherwise bring it forward. On Windows the tray
                    // click itself takes the focus first, so a window focused just
                    // now still counts as in front (#520).
                    let focus = ctx.input(|i| i.viewport().focused);
                    let since_focused = self.tray_last_focused.map(|t| t.elapsed());
                    // With the icon under the taskbar's ^, the flyout took the focus well
                    // before the click: Windows is asked whether anything covers the window.
                    #[cfg(windows)]
                    let in_front = crate::win32::main_window_in_front() == Some(true);
                    // Elsewhere focus is all there is to go by (sway, KDE's tray).
                    #[cfg(not(windows))]
                    let in_front = false;
                    if tray_toggle_hide(self.hidden, minimized, focus, since_focused)
                        || (!self.hidden && !minimized && in_front)
                    {
                        self.hide_to_tray(ctx);
                    } else {
                        self.show_window(ctx);
                    }
                    continue;
                }
                TrayCmd::AddUrl => Cmd::AddUrl,
                TrayCmd::Batch => Cmd::Batch(false),
                TrayCmd::PauseAll => {
                    // What runs now, to resume: the downloads and the queues.
                    self.tray_kept.paused = self
                        .items
                        .iter()
                        .filter(|i| i.live.is_some() && i.listed())
                        .map(|i| i.id)
                        .collect();
                    self.tray_kept.paused_queues = (0..self.queues.len())
                        .filter(|q| self.queues[*q].running)
                        .collect();
                    Cmd::StopAll
                }
                TrayCmd::ResumeAll => {
                    for id in std::mem::take(&mut self.tray_kept.paused) {
                        if self
                            .item(id)
                            .is_some_and(|i| i.live.is_none() && i.can_start())
                        {
                            self.stopped_by_hand.remove(&id);
                            self.start(id, true);
                        }
                    }
                    for q in std::mem::take(&mut self.tray_kept.paused_queues) {
                        if q < self.queues.len() && !self.queues[q].running {
                            self.start_queue(q);
                        }
                    }
                    continue;
                }
                TrayCmd::LimiterOn => Cmd::LimiterOn,
                TrayCmd::LimiterOff => Cmd::LimiterOff,
                TrayCmd::LimiterAt(kb) => {
                    self.settings.speed_limit_kb = kb;
                    Cmd::LimiterOn
                }
                TrayCmd::LimiterSettings => Cmd::LimiterSettings,
                TrayCmd::ToggleNotify => Cmd::ToggleNotify,
                TrayCmd::Scheduler => Cmd::Scheduler,
                TrayCmd::StartQueue => Cmd::StartQueue(0),
                TrayCmd::StopQueue => Cmd::StopQueue(0),
                TrayCmd::Options => Cmd::Options,
                TrayCmd::Exit => Cmd::Exit,
                TrayCmd::ShowDownload(id) => {
                    self.open_item(id);
                    continue;
                }
            };
            self.run(cmd, None, ctx);
        }
        // Closing hides the window in the tray; downloads and queues go on. When the shell says it
        // has no icon (Windows, macOS: a tray that exists, and the shell says it has none) the
        // window goes to the taskbar or the Dock instead: still reachable, downloads still going.
        // No answer from the shell is not such a "no" (#750, `tray::on_close`). Everywhere else
        // closing closes, as any window's does: Options' "Keep running when closing the window"
        // off, no tray at all, or Linux with no tray host (#59: a hidden window couldn't come
        // back, and one that merely minimized could never be closed).
        if !self.exiting && ctx.input(|i| i.viewport().close_requested()) {
            use crate::tray::OnClose;
            let keep = self.settings.close_to_tray;
            // No tray icon wanted (View → Tray icon): closing goes as when the shell has none,
            // the window minimized to the taskbar or the Dock (Linux: it quits, #59).
            let unwanted = !self.settings.tray_icon;
            let tray = self.tray.is_some() || unwanted;
            // The shell's answer, only when it matters (macOS: asked afresh).
            let icon = if keep && tray && !unwanted {
                self.tray_icon_now()
            } else {
                crate::tray::Icon::Gone
            };
            let taskbar = cfg!(not(target_os = "linux"));
            match crate::tray::on_close(keep, tray, icon, taskbar) {
                OnClose::Hide => {
                    tracing::info!("close: hidden to the notification area (tray icon: {icon:?})");
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.hide_to_tray(ctx);
                }
                OnClose::Minimize => {
                    tracing::info!("close: no visible tray icon; minimizing to the taskbar");
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                OnClose::Quit => {
                    tracing::info!("close: quitting");
                    self.exiting = true;
                }
            }
        }
        // Options → General → "Minimize to the notification area" (#63): Minimize hides the
        // window there too, where a tray shows the icon. Wayland can't hide a window: there it
        // stays minimized, in the dock, as it was.
        #[cfg(target_os = "linux")]
        let can_hide = !crate::unix::on_wayland();
        #[cfg(not(target_os = "linux"))]
        let can_hide = true;
        let wanted = self.tray.is_some()
            && self.settings.minimize_to_tray
            && can_hide
            && !self.exiting
            && self.tray_icon() != crate::tray::Icon::Gone;
        // Windows: the minimize command itself is caught (win32), so the window goes straight to
        // the tray, never minimized (an idle app runs no frame while minimized: it wouldn't see).
        #[cfg(windows)]
        let minimize = crate::win32::main_minimize_to_tray(ctx, wanted);
        // Elsewhere: on the change to minimized (brought back, the window may still read
        // minimized for a frame or two). It is seen at the next frame the app runs; macOS doesn't
        // report it at all while running (egui#3494), so there Minimize stays in the Dock.
        #[cfg(not(windows))]
        let minimize = {
            let minimized = ctx.input(|i| i.viewport().minimized == Some(true));
            let was = egui::Id::unique("main-was-minimized");
            let was_minimized = ctx.data(|d| d.get_temp::<bool>(was)).unwrap_or(false);
            ctx.data_mut(|d| d.insert_temp(was, minimized));
            wanted && minimized && !was_minimized
        };
        if minimize && !self.hidden {
            tracing::info!("minimized to the notification area");
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hidden = true;
            self.start_trimming();
        }
        // Hidden, the window gets no paint messages: keep the clock (queues, scheduler,
        // download limits) running.
        if self.hidden {
            ctx.request_repaint_after(Duration::from_secs(1));
            if self.trim_when_hidden.is_some_and(|t| Instant::now() >= t) {
                trim_memory();
                self.trims_since_hidden += 1;
                self.trim_when_hidden = Some(Instant::now() + next_trim(self.trims_since_hidden));
            }
        }
    }

    /// Whether a window can be hidden in the notification area and brought back from it: the
    /// tray shows DDM's icon (off-screen test copies, which have none, as if they had one unless
    /// View → Tray icon is off).
    pub(super) fn tray_available(&self) -> bool {
        (self.tray.is_some() || (crate::offscreen() && self.settings.tray_icon))
            && crate::tray::available()
    }
}

/// Hidden to the tray, the window's memory is given back 2 s after it hid (its last frames done),
/// again 2 minutes after it hid (the clock's ticks page some back in, about 5 MB in the first
/// minute, and the process takes 4 MB more once, about 2 minutes after start), then every
/// 10 minutes while it stays hidden. `done` is how many times it was given back since hiding.
pub(super) fn next_trim(done: u32) -> Duration {
    match done {
        0 => Duration::from_secs(2),
        1 => Duration::from_secs(118),
        _ => Duration::from_secs(600),
    }
}

/// Whether a tray left-click hides the window instead of showing it: hidden or
/// minimized never hides; a focused window does; an unfocused one does only when
/// it just lost the focus (the click itself took it), else it is behind other
/// windows and comes forward. Unknown focus never hides.
pub(super) fn tray_toggle_hide(
    hidden: bool,
    minimized: bool,
    focused: Option<bool>,
    since_focused: Option<Duration>,
) -> bool {
    if hidden || minimized {
        return false;
    }
    if focused == Some(true) {
        return true;
    }
    focused == Some(false) && since_focused.is_some_and(|d| d < TRAY_TOGGLE_GRACE)
}

/// `since` the start, and since DDM's tray `icon` was created (None: not yet): whether a tray
/// `host` shows it (Windows, macOS: always one; Linux: a StatusNotifierWatcher, #59). The
/// grace counts from the icon, so a slow start of DDM's own doesn't eat the desktop's time.
pub(super) fn at_login(since: Duration, icon: Option<Duration>, host: bool) -> AtLogin {
    if icon.is_some() && host {
        AtLogin::Tray
    } else if icon.unwrap_or(since) >= TRAY_GRACE {
        AtLogin::Minimize
    } else {
        AtLogin::Wait
    }
}

/// Hand the memory the process isn't using now back to the system (#72).
pub(super) fn trim_memory() {
    #[cfg(windows)]
    crate::win32::trim_working_set();
    #[cfg(unix)]
    crate::unix::trim_heap();
}
