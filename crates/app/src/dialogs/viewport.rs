//! The dialogs' windows: one immediate viewport each, placed and kept on the screen, following the
//! interface size and the theme, its title drawn where the theme draws one, raised or left behind
//! the program in front (`Kind`), and the plain question and notice boxes.

use super::*;

pub(super) const MAIN_RECT: &str = "mdm-main-rect";

/// The dialogs later downloads wait behind (`Download File Info`, the duplicate question): they
/// must be on the screen, even while the main window is minimized (Windows hides a window's owned
/// windows with it, leaving the downloads waiting behind one nobody can see, #440).
#[cfg(windows)]
pub(super) const QUEUE_GATES: [&str; 2] = ["file-info", "duplicate"];

/// How a dialog's window behaves towards the others.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Plain,
    /// Something DDM tells (`message`): it may come up by itself (a failure in a queue, a save
    /// that failed), so it takes the keyboard only when DDM was in front as it came
    /// (`asked_in_ddm`); otherwise it shows above the other windows without taking it.
    Told,
    /// Above the app's windows, taking the focus when it opens (`question`).
    Question,
    /// Never takes the focus by itself; above every program's windows while `on_top` (`notice`).
    Notice {
        on_top: bool,
    },
}

/// The main window publishes its outer rect each frame so dialogs can centre on it, with the
/// zoom it is measured at (not in the frame the interface size changes: its rect is still
/// measured at the old one then, scale::settled).
pub fn publish_main_rect(ctx: &egui::Context) {
    if let Some(rc) = ctx.input(|i| i.viewport().outer_rect) {
        let zoom = ctx.zoom_factor();
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique(MAIN_RECT), (rc, zoom)));
    }
}

/// The main window's outer rect as last published, in points at the zoom drawn now (#262: the
/// same place on the screen is fewer points at a larger interface size).
pub(super) fn main_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    let (rc, zoom) = ctx.data(|d| d.get_temp::<(egui::Rect, f32)>(egui::Id::unique(MAIN_RECT)))?;
    let k = zoom / ctx.zoom_factor();
    Some(egui::Rect::from_min_max(
        (rc.min.to_vec2() * k).to_pos2(),
        (rc.max.to_vec2() * k).to_pos2(),
    ))
}

/// Where a dialog opens: centred over the main window (as IDM's are), inside the screen's work
/// area (its title bar first, for one larger than the screen), decided once when the dialog
/// first shows and kept until `forget_place` — so the user can still move it. Placed again when
/// the interface size changes (#262): the place is in points, which that changes.
pub fn place(ctx: &egui::Context, key: egui::Id, client: [f32; 2]) -> Option<Pos2> {
    let zoom = ctx.zoom_factor();
    if let Some((p, z)) = ctx.data(|d| d.get_temp::<(Pos2, f32)>(key))
        && z == zoom
    {
        return Some(p);
    }
    let main = main_rect(ctx)?;
    // A fixed-frame dialog's outer size is client + (6, 29) (spec "Conventions").
    let outer = vec2(client[0] + 6.0, client[1] + 29.0);
    let mut p = (main.center() - outer / 2.0).round();
    if let Some(area) = crate::scale::work_area(ctx, main.center()) {
        let whole = vec2(client[0], client[1]) + crate::scale::frame(ctx);
        p = crate::scale::keep_inside(p, whole, area).round();
    }
    ctx.data_mut(|d| d.insert_temp(key, (p, zoom)));
    Some(p)
}

pub fn forget_place(ctx: &egui::Context, key: egui::Id) {
    ctx.data_mut(|d| d.remove::<(Pos2, f32)>(key));
}

/// A fixed-size window's client of `size` points as it can be shown on the screen around the
/// main window: no larger than the work area at the interface size (#262), [`scroll`]ed where
/// it is smaller.
pub fn shown_size(ctx: &egui::Context, size: [f32; 2]) -> [f32; 2] {
    let at = main_rect(ctx).map_or(Pos2::ZERO, |m| m.center());
    let fit = crate::scale::shown(vec2(size[0], size[1]), crate::scale::room(ctx, at));
    [fit.x, fit.y]
}

/// A window's whole client, `size` points laid out from the origin `body` gets, drawn in what
/// the window has: scrolled when that is smaller ([`shown_size`]; scale::pane). Interface size
/// keys (Ctrl + / Ctrl − / Ctrl 0) pressed in the window are taken here too.
pub fn scroll<R>(
    ui: &mut Ui,
    id: egui::Id,
    size: [f32; 2],
    body: impl FnOnce(&mut Ui, Pos2) -> R,
) -> R {
    crate::scale::keys(ui.ctx());
    crate::scale::pane(ui, id, vec2(size[0], size[1]), body)
}

/// The interface size changed while window `vid` is open (#262): its size, `shown` points, at
/// the new scale. Windows' fixed frames are sized again by their owner from the new scale
/// (`dialog_frame`, `thin_frame`); elsewhere the window manager is asked. True when it changed
/// since the last call (the caller frames it again).
pub fn follow_scale(ctx: &egui::Context, vid: ViewportId, key: egui::Id, shown: [f32; 2]) -> bool {
    let scale = ctx.input_for(vid, |i| i.pixels_per_point);
    let k = key.with("scale-seen");
    let seen = ctx.data(|d| d.get_temp::<f32>(k));
    ctx.data_mut(|d| d.insert_temp(k, scale));
    let changed = seen.is_some_and(|s| s != scale);
    if changed && !cfg!(windows) {
        ctx.send_viewport_cmd_to(
            vid,
            egui::ViewportCommand::InnerSize(vec2(shown[0], shown[1])),
        );
    }
    changed
}

/// Window chrome shared by every dialog and the progress window: the caption's theme, and when
/// the window may be seen. winit gives a new window the *system* theme, so windows are created
/// hidden (`hidden_builder`), get their theme on the first frame and are shown on the next,
/// already right — no dark flash. The theme is re-sent whenever the wish changes. Call every
/// frame with `open` = false once the window is gone.
pub fn keep_theme(
    ctx: &egui::Context,
    vid: ViewportId,
    key: egui::Id,
    title: &str,
    dark: bool,
    open: bool,
) {
    let theme_key = key.with("theme");
    let shown_key = key.with("shown");
    let front_key = key.with("front");
    let caption_key = key.with("caption");
    let icons_key = key.with("icons");
    // Not drawn in the last pass, the window is gone and this is a new one, hidden until it is
    // told otherwise, whatever the last one was told (#440).
    if !open || window_was_gone(ctx, key.with("theme-pass")) {
        ctx.data_mut(|d| {
            d.remove::<bool>(theme_key);
            d.remove::<bool>(shown_key);
            d.remove::<u32>(front_key);
            d.remove::<bool>(caption_key);
            d.remove::<bool>(icons_key);
        });
    }
    if !open {
        return;
    }
    let sent = ctx.data(|d| d.get_temp::<bool>(theme_key));
    if sent != Some(dark) {
        ctx.send_viewport_cmd_to(
            vid,
            egui::ViewportCommand::SetTheme(crate::theme::system_theme(dark)),
        );
        ctx.data_mut(|d| d.insert_temp(theme_key, dark));
        // The window is revealed on the next frame: make sure there is one, at once though the
        // main window is hidden in the notification area (#3).
        ctx.request_repaint();
        crate::starttime::nudge();
    }
    // The theme went out in an earlier frame: show the window now.
    let shown = ctx.data(|d| d.get_temp::<bool>(shown_key).unwrap_or(false));
    if sent.is_some() && !shown {
        ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Visible(true));
        ctx.data_mut(|d| d.insert_temp(shown_key, true));
        // A dialog's: how long from being asked for to being shown (#3: a download handed over
        // shows Download File Info at once).
        if let Some(at) = ctx.data(|d| d.get_temp::<std::time::Instant>(key.with("asked-at"))) {
            tracing::info!(
                "start time: window {title:?} shown {} ms after it was asked for",
                crate::starttime::ms(at)
            );
        }
        // The next frame sees where it went (below).
        ctx.request_repaint();
        crate::starttime::nudge();
    }
    // Shown (in an earlier frame), once: in front of the other programs' windows, where Windows
    // didn't give it the foreground (#208: Download File Info behind the browser that asked for
    // it). Not in off-screen test runs, whose windows stay out of the way. Elsewhere the window
    // manager places a new window and decides on its focus; an app has no say there.
    // Looked for in a few frames at most (its title may change meanwhile: a progress window's).
    #[cfg(windows)]
    {
        const TRIES: u32 = 30;
        let tries = ctx.data(|d| d.get_temp::<u32>(front_key).unwrap_or(0));
        if shown && !crate::offscreen() && tries < TRIES {
            let tries = match crate::win32::come_forward(title) {
                Some(lifted) => {
                    if lifted {
                        tracing::info!("window {title:?} shown behind another program's: lifted");
                    }
                    TRIES
                }
                None => {
                    ctx.request_repaint();
                    tries + 1
                }
            };
            ctx.data_mut(|d| d.insert_temp(front_key, tries));
        }
    }
    #[cfg(not(windows))]
    let _ = front_key;
    // Belt and braces, once per window and theme (until the window is found): asked every frame,
    // the window's thread waited on DWM, or on a busy window of this process while looking for
    // its own, in a stalled system (#633, #583).
    #[cfg(windows)]
    if ctx.data(|d| d.get_temp::<bool>(caption_key)) != Some(dark)
        && crate::win32::caption_dark(title, dark)
    {
        ctx.data_mut(|d| d.insert_temp(caption_key, dark));
    }
    // The title bar's and the taskbar's icons drawn for their size, once per window (until it is
    // found): it keeps them from then on, at any scale.
    #[cfg(windows)]
    if ctx.data(|d| d.get_temp::<bool>(icons_key)).is_none() && crate::win32::crisp_icons(title) {
        ctx.data_mut(|d| d.insert_temp(icons_key, true));
    }
    #[cfg(not(windows))]
    let _ = (title, caption_key, icons_key);
}

/// Whether `keep_theme` has shown the window `key` (it is created hidden, given its theme, then
/// shown): a command that needs it on the screen (Minimized) goes after that.
pub fn window_shown(ctx: &egui::Context, key: egui::Id) -> bool {
    ctx.data(|d| d.get_temp::<bool>(key.with("shown")).unwrap_or(false))
}

/// Whether the window `key` is drawn again after a pass that didn't draw it: egui destroys a
/// viewport a pass doesn't draw, so what was kept about its window went with it (a dialog that
/// ended without a closing pass, as Download File Info behind the Create folder question, #440).
/// False while it is drawn pass after pass, whatever the time between them (a minimized main
/// window runs its passes seconds apart). `key` is this caller's alone.
pub(super) fn window_was_gone(ctx: &egui::Context, key: egui::Id) -> bool {
    let pass = ctx.cumulative_pass_nr();
    let last = ctx.data(|d| d.get_temp::<u64>(key));
    ctx.data_mut(|d| d.insert_temp(key, pass));
    last.is_none_or(|l| pass.saturating_sub(l) > 1)
}

/// A window that must be on the screen and isn't (hidden, not minimized): shown again after a
/// moment, however it got that way, directly: winit believes it is still visible and ignores
/// `Visible(true)`. The other platforms have nothing to ask: DDM's windows have no owner there,
/// and the window manager doesn't say whether it shows a window.
#[cfg(windows)]
pub(super) fn keep_on_screen(ctx: &egui::Context, key: egui::Id, title: &str) {
    const AFTER_SECONDS: f64 = 1.5;
    let since_key = key.with("hidden-since");
    let shown = ctx.data(|d| d.get_temp::<bool>(key.with("shown")).unwrap_or(false));
    if !shown || !crate::win32::window_hidden(title) {
        ctx.data_mut(|d| d.remove::<f64>(since_key));
        return;
    }
    let now = ctx.input(|i| i.time);
    let since = ctx.data(|d| d.get_temp::<f64>(since_key));
    if since.is_some_and(|s| now - s >= AFTER_SECONDS) {
        tracing::warn!("window {title:?} was hidden: shown again");
        crate::win32::show_hidden(title);
        ctx.data_mut(|d| d.remove::<f64>(since_key));
    } else if since.is_none() {
        ctx.data_mut(|d| d.insert_temp(since_key, now));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
}

/// A fixed-size dialog window whose client area is exactly `client` pixels; `body` gets the
/// client origin and paints at absolute spec coordinates.
pub(crate) fn window<T>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    client: [f32; 2],
    minimize: bool,
    bg: egui::Color32,
    body: impl FnMut(&mut Ui, Pos2) -> Outcome<T>,
) -> Outcome<T> {
    dialog_window(ctx, id, title, client, minimize, bg, Kind::Plain, body)
}

/// A question that must be answered (Yes / No, a countdown): a `window` that stays above the
/// app's other windows and takes the focus when it opens, so a click on the dialog that asked
/// it, or on the main window, never hides it behind them (on Windows it is also owned by the
/// main window; macOS and Linux have no owner to keep it in front).
pub(crate) fn question<T>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    client: [f32; 2],
    minimize: bool,
    bg: egui::Color32,
    body: impl FnMut(&mut Ui, Pos2) -> Outcome<T>,
) -> Outcome<T> {
    dialog_window(ctx, id, title, client, minimize, bg, Kind::Question, body)
}

/// News that must not go unseen (Download complete, as IDM shows it): a `window` shown above
/// every other program's while `on_top`, without taking the keyboard focus from the one being
/// typed in. The caller drops `on_top` once it has been clicked (`notice_seen`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn notice<T>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    client: [f32; 2],
    minimize: bool,
    bg: egui::Color32,
    on_top: bool,
    body: impl FnMut(&mut Ui, Pos2) -> Outcome<T>,
) -> Outcome<T> {
    dialog_window(
        ctx,
        id,
        title,
        client,
        minimize,
        bg,
        Kind::Notice { on_top },
        body,
    )
}

/// Whether notice `id` has the keyboard focus (it was clicked, or activated): seen.
pub(crate) fn notice_seen(ctx: &egui::Context, id: &str) -> bool {
    ctx.input_for(ViewportId::from_hash_of(id), |i| i.viewport().focused) == Some(true)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dialog_window<T>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    client: [f32; 2],
    minimize: bool,
    bg: egui::Color32,
    kind: Kind,
    mut body: impl FnMut(&mut Ui, Pos2) -> Outcome<T>,
) -> Outcome<T> {
    let modal = kind == Kind::Question;
    let key = egui::Id::unique(("dialog-pos", id));
    // The title in the interface language: drawn so in our title bar, and given to the system
    // as its title bar reads it (`i18n::window_title`, right to left in an RTL interface). Win32
    // finds the window by that text, so it is used as is from here on.
    let caption = crate::i18n::t(title);
    let native = crate::i18n::window_title(caption);
    let title = native.as_str();
    // A style theme's title bar is ours (Windows, #45): drawn above the layout, which keeps its
    // size; the frame (win32::thin_frame) gives the caption to the client.
    let drawn = drawn_captions();
    let caption_h = if drawn {
        crate::winui::caption::CAPTION_H
    } else {
        0.0
    };
    let outer = [client[0], client[1] + caption_h];
    // As large as the screen allows at the interface size (#262): the rest scrolls.
    let shown = shown_size(ctx, outer);
    let vid = ViewportId::from_hash_of(id);
    // A new window: nothing kept about the last one applies to it (#440). Whether the main
    // window owns it is decided now, as it is asked for: by the program in front then (the
    // browser for its download, DDM for its own menu), not by its own window once shown.
    if window_was_gone(ctx, key.with("window-pass")) {
        forget_window(ctx, vid, key);
        // When it was asked for: `keep_theme` logs how long it took to show (#3).
        ctx.data_mut(|d| d.insert_temp(key.with("asked-at"), std::time::Instant::now()));
        #[cfg(windows)]
        ctx.data_mut(|d| d.insert_temp(key.with("owned"), crate::win32::dialog_owned_by_main()));
    }
    // Hidden until `keep_theme` has given it its theme (see there).
    let mut builder = ViewportBuilder::default()
        .with_visible(false)
        .with_title(title)
        .with_inner_size(shown)
        // The thin frame trims the sizing borders and keeps the size fixed.
        .with_resizable(drawn)
        .with_minimize_button(minimize)
        .with_maximize_button(false);
    // Every dialog carries the app icon: without one Windows draws its generic placeholder.
    builder = crate::quiet(builder.with_icon(crate::app_icon()));
    if modal {
        builder = builder.with_always_on_top();
    }
    if let Kind::Notice { on_top } = kind {
        // Shown without activating it (winit: SW_SHOWNOACTIVATE), and the level always given,
        // so that dropping `on_top` turns topmost off (a missing level changes nothing).
        builder = builder.with_active(false).with_window_level(if on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        });
    }
    // Told while the person works elsewhere: shown without activating it (keep_theme lifts it
    // above the other windows), as Download complete is.
    if kind == Kind::Told && !asked_in_ddm(ctx, key) {
        builder = builder.with_active(false);
    }
    if let Some(p) = place(ctx, key, shown) {
        builder = builder.with_position(p);
    }
    crate::crash::window(id);
    let out = ctx.show_viewport_immediate(vid, builder, |ui, _| {
        // Painted in the pass that closes it too (see close_requested).
        let close = close_requested(ui);
        let out = egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(bg))
            .show(ui, |ui| {
                let origin = ui.max_rect().min;
                // The window's width, laid out from the right in a right-to-left interface (the
                // title bar; the layout under it has its own, scrolled with it).
                let _surface = winui::Surface::enter(origin.x, origin.x + shown[0]);
                // Our title bar's ×: like the window manager's, the window is painted in its
                // closing pass too (#207), then closes.
                let mut caption_close = false;
                if drawn {
                    use crate::winui::caption::CaptionButton;
                    let mut buttons = vec![(CaptionButton::Close, true)];
                    if minimize {
                        buttons.push((CaptionButton::Minimize, true));
                    }
                    let hwnd = ui.ctx().data(|d| d.get_temp::<isize>(key.with("hwnd")));
                    match drawn_title(ui, origin, shown[0], caption, &buttons, hwnd) {
                        Some(CaptionButton::Close) => caption_close = true,
                        Some(CaptionButton::Minimize) => {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        _ => {}
                    }
                }
                // The layout under the title bar, scrolled when the window is smaller.
                let below = egui::Rect::from_min_max(
                    origin + egui::vec2(0.0, caption_h),
                    ui.max_rect().max,
                );
                let mut rest = ui.new_child(egui::UiBuilder::new().max_rect(below));
                let out = scroll(&mut rest, key, client, |ui, o| {
                    let _surface = winui::Surface::enter(o.x, o.x + client[0]);
                    body(ui, o)
                });
                if caption_close { Outcome::Closed } else { out }
            })
            .inner;
        if close { Outcome::Closed } else { out }
    });
    let rescaled = follow_scale(ctx, vid, key, shown);
    // Elsewhere follow_scale asked the window manager itself.
    #[cfg(not(windows))]
    let _ = rescaled;
    // Give the window IDM's modal-dialog frame (3 px borders, 26 px caption) once it exists,
    // and again at a new size (a dialog that grows: the frame is sized from the client).
    #[cfg(windows)]
    {
        let framed = key.with("framed");
        let scale = ctx.input_for(vid, |i| i.pixels_per_point);
        // The size and whose title bar it is: either changing frames it again, as does a new
        // scale (the interface size, #262; another monitor's).
        let want = [shown[0], shown[1] + if drawn { 1e6 } else { 0.0 }];
        if rescaled || ctx.data(|d| d.get_temp::<[f32; 2]>(framed)) != Some(want) {
            if drawn {
                if let Some(h) = crate::win32::thin_frame(title, shown, scale, None) {
                    ctx.data_mut(|d| {
                        d.insert_temp(framed, want);
                        d.insert_temp(key.with("hwnd"), h);
                    });
                }
            } else {
                let owned = ctx.data(|d| d.get_temp::<bool>(key.with("owned")).unwrap_or(false));
                let light = !crate::theme::dark_surface();
                if crate::win32::dialog_frame(title, shown, light, scale, owned) {
                    ctx.data_mut(|d| d.insert_temp(framed, want));
                }
            }
        }
    }
    keep_theme(
        ctx,
        ViewportId::from_hash_of(id),
        key,
        title,
        // Every dialog follows the mode (the light dialogs too), its caption the theme's surface.
        crate::theme::dark_surface(),
        matches!(out, Outcome::Open),
    );
    // A question takes the focus once it is shown (not while hidden for its theme), and so does
    // Download File Info outside Windows (#484): asked for from the browser, it opened behind
    // it. Windows lifts it in keep_theme instead. On Windows a question is made to take it only
    // when it was asked with DDM in front (`asked_in_ddm`): one that comes up by itself while
    // the person types elsewhere (a queue's turn-off countdown, a copied link's duplicate)
    // shows above every window without taking their keys; Windows still gives it the focus
    // itself when DDM may have it (the browser passed it on with its download).
    let focused = key.with("focus-sent");
    let takes_focus =
        (modal && asked_in_ddm(ctx, key)) || (cfg!(not(windows)) && id == "file-info");
    if takes_focus && matches!(out, Outcome::Open) && !crate::offscreen() {
        let shown = ctx.data(|d| d.get_temp::<bool>(key.with("shown")).unwrap_or(false));
        if shown && !ctx.data(|d| d.get_temp::<bool>(focused).unwrap_or(false)) {
            ctx.send_viewport_cmd_to(ViewportId::from_hash_of(id), egui::ViewportCommand::Focus);
            ctx.data_mut(|d| d.insert_temp(focused, true));
        }
    }
    #[cfg(windows)]
    if matches!(out, Outcome::Open) && QUEUE_GATES.contains(&id) {
        keep_on_screen(ctx, key, title);
    }
    if !matches!(out, Outcome::Open) {
        forget_window(ctx, vid, key);
    }
    out
}

/// Whether dialog `key` was asked for with DDM in front (Windows: one of its windows had the
/// foreground and the main window was on the screen as it was asked for,
/// `win32::dialog_owned_by_main`): the person is working in DDM, and the dialog may take the
/// keyboard. Elsewhere the window manager decides on the focus: always true.
pub(super) fn asked_in_ddm(ctx: &egui::Context, key: egui::Id) -> bool {
    #[cfg(windows)]
    return ctx.data(|d| d.get_temp::<bool>(key.with("owned")).unwrap_or(false));
    #[cfg(not(windows))]
    {
        let _ = (ctx, key);
        true
    }
}

/// What a dialog's window keeps while it is open, forgotten: the next opening is a new one (its
/// text boxes start afresh).
pub(super) fn forget_window(ctx: &egui::Context, vid: ViewportId, key: egui::Id) {
    let opened = opening_key_of(vid);
    ctx.data_mut(|d| {
        let n = d.get_temp::<u64>(opened).unwrap_or(0);
        d.insert_temp(opened, n + 1);
        d.remove::<bool>(key.with("focus-sent"));
    });
    forget_place(ctx, key);
    #[cfg(windows)]
    ctx.data_mut(|d| {
        d.remove::<[f32; 2]>(key.with("framed"));
        d.remove::<isize>(key.with("hwnd"));
    });
}

/// Whether the windows' title bars are ours: a style theme's on Windows (#45). Elsewhere the
/// window manager draws them, which takes no style of ours (Wayland places its own buttons).
pub fn drawn_captions() -> bool {
    cfg!(windows) && crate::theme::current().controls.caption != crate::palette::CaptionLook::System
}

/// A title bar we draw (where the window's frame is ours: win32::thin_frame), `width` points
/// wide at `o`, with `buttons` from the window's end; the button pressed. Where its buttons and
/// icon are goes to the window's hit test (win32::set_caption_hits), in client pixels, each
/// frame (`hwnd`: the window, once its frame is applied).
pub fn drawn_title(
    ui: &mut Ui,
    o: Pos2,
    width: f32,
    title: &str,
    buttons: &[(crate::winui::caption::CaptionButton, bool)],
    hwnd: Option<isize>,
) -> Option<crate::winui::caption::CaptionButton> {
    use crate::winui::caption::CAPTION_H;
    let rect = egui::Rect::from_min_size(o, egui::vec2(width, CAPTION_H));
    let bar = crate::winui::caption::title_bar(ui, rect, title, buttons);
    #[cfg(windows)]
    if let Some(hwnd) = hwnd {
        let ppp = ui.ctx().pixels_per_point();
        let px = |r: egui::Rect| {
            let (a, b) = ((r.min - o) * ppp, (r.max - o) * ppp);
            [
                a.x.round() as i32,
                a.y.round() as i32,
                b.x.round() as i32,
                b.y.round() as i32,
            ]
        };
        crate::win32::set_caption_hits(
            hwnd,
            crate::win32::CaptionHits {
                strip_h: (CAPTION_H * ppp).round() as i32,
                icon: px(bar.icon),
                buttons: bar.buttons.iter().map(|r| px(*r)).collect(),
                max_button: None,
            },
        );
    }
    #[cfg(not(windows))]
    let _ = hwnd;
    bar.pressed
}

/// Whether the window manager asked this window to close (its title bar's ×, Alt+F4, a
/// `WM_DELETE_WINDOW`). The window must still be painted in that pass, and the request honoured
/// after it: egui clears a dialog's window to transparent black before each pass, so a pass that
/// paints nothing leaves a black window on screen until the window is destroyed (#207: one to
/// several seconds on Linux).
pub(crate) fn close_requested(ui: &Ui) -> bool {
    ui.ctx().input(|i| i.viewport().close_requested())
}

/// Bring dialog `id` to the front with the keyboard focus once it shows (a window that opens by
/// itself with news, such as a new version found at start, as apps announce theirs). Call every
/// frame while it should; true once done. Off-screen test runs never take the focus.
pub fn focus_when_shown(ctx: &egui::Context, id: &str) -> bool {
    let key = egui::Id::unique(("dialog-pos", id));
    let shown = ctx.data(|d| d.get_temp::<bool>(key.with("shown")).unwrap_or(false));
    if !shown {
        return false;
    }
    if !crate::offscreen() {
        raise(ctx, ViewportId::from_hash_of(id));
    }
    true
}

/// How many times the current window has closed (its text boxes' ids include it).
pub fn opening_key(ctx: &egui::Context) -> egui::Id {
    opening_key_of(ctx.viewport_id())
}

pub(super) fn opening_key_of(viewport: ViewportId) -> egui::Id {
    egui::Id::unique(("dialog-opening", viewport))
}

/// A size as the dialogs show it: the same text as the list, the progress window and the tray
/// (`60.00 MB`: one space before the unit, as the language writes it).
pub fn dialog_size(b: u64) -> String {
    model::size(b)
}

// ---- Paragraphs that grow their dialog -------------------------------------------------------

/// The height a dialog adds for its paragraph `key` (13 px a line beyond the lines its layout
/// planned), as the paragraph measured it last frame ([`grow_to`]): a long translation or name
/// moves what is under it down and the window grows, instead of running over the controls.
pub fn grow_extra(ctx: &egui::Context, key: &str) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(egui::Id::unique(("grow", key))))
        .unwrap_or(0.0)
}

/// Records that dialog `key` needs `extra` px more than its layout planned ([`grow_extra`]):
/// for what grows by other steps than whole lines (a row of links that wraps).
pub fn grow_px(ctx: &egui::Context, key: &str, extra: f32) {
    let extra = extra.max(0.0).ceil();
    if extra != grow_extra(ctx, key) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("grow", key)), extra));
        ctx.request_repaint();
    }
}

/// Records that paragraph `key` took `lines` lines where `planned` fit (see [`grow_extra`]); a
/// change draws again at once, at the new size.
pub fn grow_to(ctx: &egui::Context, key: &str, lines: usize, planned: usize) {
    let extra = lines.saturating_sub(planned) as f32 * 13.0;
    if extra != grow_extra(ctx, key) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("grow", key)), extra));
        ctx.request_repaint();
    }
}

// ---- Message (light) --------------------------------------------------------------------------

/// A notice with OK, sized to its text (crate::i18n::t("Cannot find …")): one line when it
/// fits, wrapped and taller when it doesn't (a long path or reason is never cut off).
pub fn message(ctx: &egui::Context, id: &str, title: &str, text: &str) -> Outcome<()> {
    // Tahoma −11 averages about 5.5 px a character; the box is kept between 260 and 480 px.
    let width = (text.chars().count() as f32 * 5.6 + 40.0).clamp(260.0, 480.0);
    let extra = grow_extra(ctx, id);
    // It may come up by itself: the keyboard stays where the person types unless DDM was in
    // front (Kind::Told).
    dialog_window(
        ctx,
        id,
        title,
        [width, 80.0 + extra],
        false,
        winui::LIGHT_BG(),
        Kind::Told,
        |ui, o| {
            let p = ui.painter().clone();
            let lines = winui::paragraph(
                &p,
                o.x + 16.0,
                o.y + 24.0,
                text,
                winui::LIGHT_TEXT(),
                o.x + width - 16.0,
                13.0,
            );
            grow_to(ui.ctx(), id, lines, 1);
            let ok = winui::button_light(
                ui,
                r(o, (width - 73.0) / 2.0, 47.0 + extra, 73.0, 21.0),
                "OK",
                ButtonKind::Default,
                false,
            )
            .clicked();
            let keys =
                ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
            if ok || keys {
                Outcome::Closed
            } else {
                Outcome::Open
            }
        },
    )
}

/// Select the file in Explorer (Finder, the Linux file manager). A file that is gone: its
/// folder, if that is still there.
/// Bring an open window to the front (and back from the taskbar): asked for again, it came
/// to nothing while it sat behind the main window.
pub fn raise(ctx: &egui::Context, viewport: ViewportId) {
    ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Minimized(false));
    // Off-screen test runs never take the focus (main.rs `offscreen`), as the other paths here.
    if !crate::offscreen() {
        ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
    }
}

/// A dialog's window ([`window`], [`question`]) by its id.
pub fn dialog_viewport(id: &str) -> ViewportId {
    ViewportId::from_hash_of(id)
}
