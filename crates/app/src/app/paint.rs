//! Drawing the main window: its title bar where the theme draws one, the menu bar, toolbar,
//! Categories pane, list, an empty list's message and the status strip.

use super::*;

/// The status strip's height, under the list and the Categories pane.
pub(super) const STRIP_H: f32 = 22.0;

impl App {
    /// An empty list's message, centred in its rows' area `body` (English): an icon, what is
    /// empty, why, and the one thing to do about it (clear the filters, show every download,
    /// add one). Returns that command when it is clicked.
    pub(super) fn empty_state(&self, ui: &Ui, body: egui::Rect) -> Option<Cmd> {
        let p = ui.painter();
        let filtered = self.list.filters.iter().any(Option::is_some) && !self.view_ids().is_empty();
        // A grabber project's node: the way to its files found.
        let project = match self.view {
            View::Grabber(id) if !filtered => self.grab_empty_state(id),
            // Empty because a project keeps its downloads under its node: the way there.
            v if !filtered => self.grab_apart_state(v),
            _ => None,
        };
        let (title, why, action, cmd) = if let Some(project) = project {
            project
        } else if filtered {
            (
                // i18n: title in the empty download list, when the column filters hide every download
                "No downloads match the filters",
                // i18n: in the empty download list, under its title
                "The column filters hide every download in this view.",
                "Clear all filters",
                Cmd::ClearFilters,
            )
        } else if !self.items.is_empty() {
            (
                // i18n: title in the empty download list (this category or queue has no downloads)
                "No downloads here",
                // i18n: in the empty download list, under its title
                "This category or queue holds no downloads yet.",
                // i18n: link in the empty download list: go to the All Downloads view
                "Show all downloads",
                Cmd::ShowAll,
            )
        } else {
            (
                // i18n: title in the empty download list when there are no downloads at all
                "No downloads yet",
                // i18n: in the empty download list, under its title
                "The downloads you add, or drop on this window, show here.",
                // i18n: link in the empty download list: opens Add URL
                "Add new download",
                Cmd::AddUrl,
            )
        };
        let (title, why) = (crate::i18n::t(title), crate::i18n::t(why));
        let title_font = winui::tahoma_bold();
        let tw = winui::text_width_font(p, title_font.clone(), title);
        let ww = winui::text_width(p, why);
        let aw = winui::text_width(p, crate::i18n::t(action));
        // The block: icon 40, title, reason, link; in the middle, or under the header when
        // the list is short.
        let block_h = 40.0 + 14.0 + 16.0 + 8.0 + 13.0 + 14.0 + 13.0;
        let top = (body.center().y - block_h / 2.0).max(body.top() + 16.0);
        let cx = body.center().x;
        // A download glyph in the muted ink (outlined, as the toolbar's icons are).
        let ink = egui::Stroke::new(1.5, winui::MUTED());
        let c = winui::rect(egui::Rect::from_center_size(
            pos2(cx, top + 20.0),
            egui::vec2(1.0, 1.0),
        ))
        .center();
        p.circle_stroke(c, 19.0, ink);
        p.line_segment([c + egui::vec2(0.0, -10.0), c + egui::vec2(0.0, 9.0)], ink);
        p.line_segment([c + egui::vec2(-7.0, 2.0), c + egui::vec2(0.0, 9.0)], ink);
        p.line_segment([c + egui::vec2(7.0, 2.0), c + egui::vec2(0.0, 9.0)], ink);
        let y = top + 40.0 + 14.0 + 12.0;
        winui::text_font(p, title_font, cx - tw / 2.0, y, title, winui::TEXT());
        let y = y + 8.0 + 13.0;
        winui::text(p, cx - ww / 2.0, y, why, winui::MUTED());
        let y = y + 14.0 + 13.0;
        winui::link(ui, p, cx - aw / 2.0, y, action, "empty-action").then_some(cmd)
    }

    /// The strip under the list: how many downloads are shown and selected, how many run and
    /// how fast in all (beyond IDM, which has no status bar).
    /// The status strip under the panes; true when its "Clear filters" was clicked.
    pub(super) fn status_strip(&self, ui: &Ui, o: Pos2, w: f32, h: f32, shown: usize) -> bool {
        let p = ui.painter();
        let top = o.y + h - STRIP_H;
        // Centred in the band it reads as: the strip and the 5 px gap above it, under the panes.
        let baseline = winui::baseline_in(top - 5.0, STRIP_H + 5.0);
        let n = shown as u64;
        let mut left = crate::i18n::tn(n, "{n} download", "{n} downloads");
        let selected = self.selection.ids().len() as u64;
        if selected > 1 {
            left += "   ";
            left += &crate::i18n::tn(selected, "{n} selected", "{n} selected");
        }
        // A column filter hides rows: said here, with the way back (a header's funnel shows
        // only under the pointer, or on the filtered column).
        let filtered = self.list.filtered();
        if filtered {
            left += "   ";
            // i18n: status strip: the list shows only the downloads that pass a column's filter
            left += crate::i18n::t("(filtered)");
        }
        let drawn = winui::fit(
            ui,
            p,
            o.x + 8.0,
            baseline,
            &left,
            winui::MUTED(),
            o.x + w / 2.0,
            "strip-left",
        );
        let cleared = filtered
            && winui::link(
                ui,
                p,
                (o.x + 8.0 + drawn.width()).min(o.x + w / 2.0) + 8.0,
                baseline,
                // i18n: status strip: a link that turns every column filter off (the list shows all its downloads again)
                crate::i18n::mark("Clear filters"),
                "strip-clear-filters",
            );
        if let Some(right) = self.activity() {
            let (drawn, _) = winui::text_right(p, o.x + w - 10.0, baseline, &right, winui::MUTED());
            // The last minute's speed before it (beyond IDM), clear of the counts at the start.
            crate::speedgraph::status_sparkline(
                ui,
                p,
                o.x + w - 10.0 - drawn.width() - 8.0,
                baseline,
                o.x + w / 2.0 + 8.0,
                self.speeds.total(),
            );
        } else if let Some(note) = crate::autohelpers::note(ui.ctx()) {
            // The video helpers DDM gets by itself: coming, then ready for a moment. In the
            // strip's right half, clear of the counts.
            let note = winui::ellipsize(p, &winui::tahoma(), &note, w / 2.0 - 18.0);
            winui::text_right(p, o.x + w - 10.0, baseline, &note, winui::MUTED());
        }
        cleared
    }

    /// "2 downloading, 1.25 MB/sec" while downloads run (the status strip, the tray's tip).
    pub(super) fn activity(&self) -> Option<String> {
        let running: Vec<&Item> = self
            .items
            .iter()
            .filter(|i| i.live.is_some() && i.listed())
            .collect();
        if running.is_empty() {
            return None;
        }
        let speed: f64 = running.iter().map(|i| i.rate()).sum();
        Some(crate::i18n::fmt(
            // i18n: {count} = "2 downloading"; {speed} = all of them together, like "1.25 MB/sec"
            crate::i18n::t("{count}, {speed}"),
            &[
                (
                    "count",
                    &crate::i18n::tn(running.len() as u64, "{n} downloading", "{n} downloading"),
                ),
                ("speed", &model::rate(speed)),
            ],
        ))
    }

    pub(super) fn open_popup(&mut self, kind: Popup, at: Pos2) {
        self.popup_serial += 1;
        self.popup = Some((kind, at, self.popup_serial));
    }

    /// The main window's title bar where the theme draws its own (Windows only: elsewhere the
    /// window manager owns the title bar): the frame is switched to ours or back as the theme
    /// changes, the bar drawn at the top of `full`; returns what is left for the layout.
    pub(super) fn drawn_caption(&mut self, ui: &mut Ui, full: egui::Rect) -> egui::Rect {
        #[cfg(windows)]
        {
            use crate::winui::caption::{self, CAPTION_H, CaptionButton};
            let want =
                crate::theme::current().controls.caption != crate::palette::CaptionLook::System;
            if self.drawn_frame != Some(want)
                && let Some(h) = crate::win32::drawn_frame(crate::MAIN_TITLE, want)
            {
                self.drawn_frame = Some(want);
                self.main_hwnd = Some(h);
                ui.ctx().request_repaint();
            }
            if !want || self.drawn_frame != Some(true) {
                return full;
            }
            let rect = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), CAPTION_H));
            if let Some(h) = self.main_hwnd {
                let m = crate::win32::max_button_state(h);
                caption::frame_maximize(m.hot, m.pressed);
            }
            let buttons = [
                (CaptionButton::Close, true),
                (CaptionButton::Maximize, true),
                (CaptionButton::Minimize, true),
            ];
            let bar = caption::title_bar(ui, rect, crate::MAIN_TITLE, &buttons);
            if let Some(h) = self.main_hwnd {
                // The sizing frame above the bar (Windows 11, #208) in the bar's colour.
                crate::win32::caption_colour(h, [bar.top.r(), bar.top.g(), bar.top.b()]);
                let ppp = ui.ctx().pixels_per_point();
                let px = |r: egui::Rect| {
                    let (a, b) = ((r.min - full.min) * ppp, (r.max - full.min) * ppp);
                    [
                        a.x.round() as i32,
                        a.y.round() as i32,
                        b.x.round() as i32,
                        b.y.round() as i32,
                    ]
                };
                crate::win32::set_caption_hits(
                    h,
                    crate::win32::CaptionHits {
                        strip_h: (CAPTION_H * ppp).round() as i32,
                        icon: px(bar.icon),
                        buttons: bar.buttons.iter().map(|r| px(*r)).collect(),
                        max_button: Some(1),
                    },
                );
            }
            let ctx = ui.ctx();
            match bar.pressed {
                Some(CaptionButton::Close) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Some(CaptionButton::Minimize) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                }
                Some(CaptionButton::Maximize) => {
                    let max = ctx.input(|i| i.viewport().maximized) == Some(true);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                _ => {}
            }
            egui::Rect::from_min_max(egui::pos2(full.min.x, full.min.y + CAPTION_H), full.max)
        }
        #[cfg(not(windows))]
        {
            let _ = ui;
            full
        }
    }

    pub(super) fn toggle(&mut self, b: Branch) {
        match b {
            Branch::All => self.all_open = !self.all_open,
            Branch::Unfinished => self.unfinished_open = !self.unfinished_open,
            Branch::Finished => self.finished_open = !self.finished_open,
            Branch::Queues => self.queues_open = !self.queues_open,
            Branch::Grabber => self.grab_toggle(),
            Branch::Leaf => {}
        }
    }

    pub(super) fn paint(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        // The client area, laid out from the right in a right-to-left interface (winui).
        let _surface = winui::Surface::enter(full.min.x, full.max.x);
        // A style theme's title bar, drawn above the layout (Windows; #45), mirrored right to
        // left as the progress window's.
        let full = self.drawn_caption(ui, full);
        let o = full.min;
        let (w, h) = (full.width(), full.height());
        let mut chosen: Option<(Cmd, Option<u64>)> = None;

        // The menu bar from the keyboard: F10 or Alt pressed alone opens its first menu, Alt and a
        // menu's underlined letter that menu (Windows and Linux); on macOS, whose apps have no
        // Alt keys, Ctrl+F2 (the system's key for the menu bar).
        // A focused button or list (Tab, a screen reader, #328) leaves these keys to the menu bar;
        // only a text box keeps them.
        let typing = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(&ctx, id).is_some());
        if self.open_menu.is_none() && self.popup.is_none() && !typing {
            let labels: Vec<String> = chrome::menus(ui.painter())
                .iter()
                .map(|(l, ..)| chrome::menu_letter(l))
                .collect();
            let (alt, other, f10, ctrl_f2, letter) = ctx.input(|i| {
                let mut letter = None;
                let mut other = false;
                for e in &i.events {
                    match e {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            other = true;
                            if modifiers.alt && !modifiers.ctrl && !modifiers.command {
                                letter = Some(key.name().to_lowercase());
                            }
                        }
                        egui::Event::PointerButton { pressed: true, .. } => other = true,
                        // A letter of another script (Cyrillic, Arabic...) typed with Alt.
                        egui::Event::Text(t) if i.modifiers.alt && !i.modifiers.ctrl => {
                            letter = Some(t.to_lowercase());
                        }
                        _ => {}
                    }
                }
                (
                    i.modifiers.alt,
                    other,
                    i.key_pressed(egui::Key::F10) && i.modifiers.is_none(),
                    i.key_pressed(egui::Key::F2) && i.modifiers.ctrl,
                    letter,
                )
            });
            let mut open = None;
            if cfg!(target_os = "macos") {
                if ctrl_f2 {
                    open = Some(0);
                }
            } else {
                if alt && !self.alt_down {
                    self.alt_alone = true;
                    // The keyboard is in use: access letters show (light mode too).
                    crate::chrome::show_cues(&ctx);
                }
                if alt && other {
                    self.alt_alone = false;
                }
                if !alt && self.alt_down && self.alt_alone {
                    open = Some(0);
                }
                if f10 {
                    open = Some(0);
                }
                if let Some(l) = letter {
                    open = labels.iter().position(|m| *m == l).or(open);
                }
            }
            self.alt_down = alt;
            if let Some(i) = open {
                // The key that opened the menu is spent: the menu, drawn in this same pass, took
                // it as one of its own items' letters too (Alt+D opened Downloads and chose its
                // "&Delete All Completed", Alt+F File's first "F" item).
                ctx.input_mut(|inp| {
                    inp.events.retain(|e| {
                        !matches!(
                            e,
                            egui::Event::Key { pressed: true, .. } | egui::Event::Text(_)
                        )
                    })
                });
                self.alt_alone = false;
                self.open_menu = chrome::menubar_next(self.open_menu, MenuBarEvent::Open(i));
                chrome::start_keyboard(&ctx, &format!("menu-{i}"));
            }
        } else {
            self.alt_down = false;
            self.alt_alone = false;
        }

        // Menu bar and its popups: which menu is open is chrome::menubar_next's to say.
        if let Some(event) = chrome::menu_bar(ui, o, w, self.open_menu) {
            self.open_menu = chrome::menubar_next(self.open_menu, event);
        }
        // A menu opened by Left or Right is drawn in the same pass as the one it replaces (a
        // second turn): on X11 the bar's menus share one window (chrome::popup), and a pass
        // without a menu closed it, a new window opening for the next (#277).
        for _ in 0..2 {
            let Some(i) = self.open_menu else { break };
            let x = chrome::menus(ui.painter())[i].1;
            let entries = self.menu_entries(i);
            // Under the item's leading edge (the menu's top-right in RTL).
            let at = pos2(winui::mx(o.x + x), o.y + chrome::MENU_H - 1.0);
            let event = match chrome::popup(
                &ctx,
                MenuKind::Bar,
                &format!("menu-{i}"),
                at,
                &entries,
                &|c: &Cmd| c.hint(),
            ) {
                PopupResult::Open => None,
                PopupResult::Neighbor(d) => {
                    let n = chrome::menus(ui.painter()).len() as i32;
                    let j = (i as i32 + d).rem_euclid(n) as usize;
                    chrome::start_keyboard(&ctx, &format!("menu-{j}"));
                    Some(MenuBarEvent::Open(j))
                }
                PopupResult::Chosen(c) => {
                    chosen = Some((c, None));
                    Some(MenuBarEvent::Chosen)
                }
                PopupResult::Dismissed => {
                    // Shut by a press on a title: its click (on release) is `menu_bar`'s. Not
                    // by Escape with the pointer merely resting on the bar.
                    let press =
                        ui.input(|x| x.pointer.press_origin().filter(|_| x.pointer.any_pressed()));
                    let press_on_title = press
                        .is_some_and(|at| chrome::menubar_title_at(ui.painter(), o, at).is_some());
                    Some(MenuBarEvent::Dismissed { press_on_title })
                }
            };
            let neighbor = matches!(event, Some(MenuBarEvent::Open(_)));
            if let Some(event) = event {
                self.open_menu = chrome::menubar_next(self.open_menu, event);
            }
            if !neighbor {
                break;
            }
        }

        // Toolbar — full labels (IDM truncates "Delete Co...", "Start Qu...": user decision).
        let picked = self.chosen(None);
        let running = picked.iter().any(|i| i.live.is_some());
        let resumable = picked.iter().any(|i| i.can_start());
        let deletable = picked.iter().any(|i| i.live.is_none());
        let any_running = self.items.iter().any(|i| i.live.is_some() && i.listed());
        let any_complete = self
            .items
            .iter()
            .any(|i| i.status == Status::Complete && i.listed());
        // Each button's state and command; their labels and first order are toolbar.rs's, the
        // order and the buttons shown View → Toolbar → Customize's.
        let state = |icon: Icon| match icon {
            Icon::AddUrl => (true, Cmd::AddUrl),
            Icon::AddTorrent => (true, Cmd::OpenTorrentFile),
            Icon::Resume => (resumable, Cmd::Resume),
            Icon::Stop => (running, Cmd::Stop),
            Icon::StopAll => (any_running, Cmd::StopAll),
            Icon::Delete => (deletable, Cmd::Remove),
            Icon::DeleteCompleted => (any_complete, Cmd::DeleteCompleted),
            Icon::Options => (true, Cmd::Options),
            Icon::Scheduler => (true, Cmd::Scheduler),
            // The button acts on the main queue (the others: its arrow, the menus).
            Icon::StartQueue => (
                self.queues.first().is_some_and(|q| !q.running),
                Cmd::StartQueue(0),
            ),
            Icon::StopQueue => (
                self.queues.first().is_some_and(|q| q.running),
                Cmd::StopQueue(0),
            ),
            // Tasks → Run site grabber...: while a grab runs, its window is brought forward.
            Icon::Grabber => (true, Cmd::SiteGrabber),
            _ => (false, Cmd::None),
        };
        let buttons: Vec<(Icon, &str, bool, Cmd)> =
            crate::toolbar::shown(&self.settings.toolbar_buttons)
                .into_iter()
                .map(|icon| {
                    let (enabled, cmd) = state(icon);
                    (icon, crate::toolbar::label(icon), enabled, cmd)
                })
                .collect();
        // View → Toolbar → Line / 3D style / the theme's own, for every button's icon from this
        // frame on: as chosen for the theme in use.
        crate::icons::set_toolbar_icons(self.toolbar_style());
        let look = chrome::ToolbarLook {
            small: self.settings.toolbar_small,
            labels: self.settings.toolbar_labels,
        };
        let pane_top = chrome::pane_top(look);
        let tb: Vec<(Icon, &str, bool)> = buttons.iter().map(|(i, l, e, _)| (*i, *l, *e)).collect();
        match chrome::toolbar(ui, o, w, look, &tb) {
            Some(ToolbarEvent::Click(k)) => chosen = Some((buttons[k].3, None)),
            Some(ToolbarEvent::Drop(k, at)) => match buttons[k].0 {
                Icon::AddTorrent => self.open_popup(Popup::TorrentDrop, at),
                icon => self.open_popup(Popup::QueueDrop(matches!(icon, Icon::StartQueue)), at),
            },
            None => {}
        }

        // Categories pane.
        let list_left = if self.show_categories {
            let rows = self.tree_rows();
            let tree: Vec<TreeRow<'_>> = rows.iter().map(|(r, ..)| r.clone()).collect();
            // The splitter: the 4 px gap between the pane and the list, dragged sideways.
            let max_w = (w * 0.6).max(100.0);
            let cat_w = (self.settings.categories_width as f32).clamp(100.0, max_w);
            let right = o.x + 6.0 + cat_w;
            let split = winui::rect(egui::Rect::from_min_max(
                pos2(right, o.y + chrome::MENU_H + pane_top),
                pos2(right + 4.0, o.y + h - STRIP_H - 5.0),
            ));
            let sr = ui.interact(
                split,
                egui::Id::unique("cat-splitter"),
                winui::pointer_drag(),
            );
            if sr.hovered() || sr.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if sr.dragged()
                && let Some(p) = sr.interact_pointer_pos()
            {
                // The pointer stays on the splitter's middle (the pane's width from its leading
                // edge: the right in RTL).
                let nw = (winui::mx(p.x) - o.x - 6.0 - 2.0)
                    .round()
                    .clamp(100.0, max_w);
                self.settings.categories_width = nw as u32;
            }
            if sr.drag_stopped() {
                self.dirty = true;
            }
            // The tree's keys while it has them (Tab, or F6 as in Explorer, moves between it and
            // the list): Up / Down through the nodes, Left / Right close and open a branch (on
            // a shut or childless node Left goes to its parent, on an open one Right to its
            // first child, as a Windows tree view), Home / End.
            let free = self.popup.is_none()
                && self.open_menu.is_none()
                && ctx.memory(|m| m.focused().is_none());
            if free
                && ctx.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                        || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)
                        || i.consume_key(egui::Modifiers::NONE, egui::Key::F6)
                        || i.consume_key(egui::Modifiers::SHIFT, egui::Key::F6)
                })
            {
                self.tree_keys = !self.tree_keys;
                self.list.set_keys(!self.tree_keys);
                crate::chrome::show_cues(&ctx);
            }
            let mut tree_event = None;
            if free && self.tree_keys {
                let at = rows
                    .iter()
                    .position(|(_, v, _)| *v == Some(self.view))
                    .unwrap_or(0);
                let with_view: Vec<usize> =
                    (0..rows.len()).filter(|k| rows[*k].1.is_some()).collect();
                let pos = with_view.iter().position(|k| *k == at).unwrap_or(0);
                let key = |k| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
                let to = if key(egui::Key::ArrowDown) {
                    with_view.get(pos + 1).copied()
                } else if key(egui::Key::ArrowUp) {
                    pos.checked_sub(1).and_then(|p| with_view.get(p).copied())
                } else if key(egui::Key::Home) {
                    with_view.first().copied()
                } else if key(egui::Key::End) {
                    with_view.last().copied()
                } else {
                    None
                };
                if let Some(k) = to {
                    tree_event = Some(TreeEvent::Select(k));
                }
                let (left, right) = if winui::rtl() {
                    (egui::Key::ArrowRight, egui::Key::ArrowLeft)
                } else {
                    (egui::Key::ArrowLeft, egui::Key::ArrowRight)
                };
                let levels: Vec<(usize, Option<bool>, bool)> = rows
                    .iter()
                    .map(|(r, v, _)| (r.level, r.expander, v.is_some()))
                    .collect();
                let side = if key(left) {
                    Some(false)
                } else if key(right) {
                    Some(true)
                } else {
                    None
                };
                if let Some(right) = side {
                    tree_event = match tree_side_step(&levels, at, right) {
                        Some(TreeStep::Toggle) => Some(TreeEvent::Toggle(at)),
                        Some(TreeStep::Select(k)) => Some(TreeEvent::Select(k)),
                        None => tree_event,
                    };
                }
            }
            if let Some(ev) = chrome::categories(
                ui,
                &mut self.icons,
                o,
                look,
                o.y + h - STRIP_H,
                right,
                &tree,
                self.tree_keys && free,
            )
            .or(tree_event)
            {
                // A click in the tree gives it the keys.
                self.tree_keys = true;
                self.list.set_keys(false);
                match ev {
                    TreeEvent::Close => self.show_categories = false,
                    TreeEvent::Select(k) => match rows[k].1 {
                        Some(v) => self.view = v,
                        None => self.toggle(rows[k].2),
                    },
                    TreeEvent::Toggle(k) => self.toggle(rows[k].2),
                    TreeEvent::Context(k, at) => {
                        // Leaves and nodes with a view; "Queues" (a branch without one) has
                        // the queue menu, all greyed.
                        let node = rows[k].1;
                        if node.is_some() || rows[k].2 == Branch::Queues {
                            self.open_popup(Popup::Node(node), at);
                        }
                    }
                }
            }
            6.0 + cat_w + 4.0
        } else {
            6.0
        };

        // Download list: each row's texts made only when it is drawn (LazyRows), unless a
        // column filter had to look at them all.
        let (ids, order) = self.list_order();
        let queued = self.queued_ids();
        self.selection.retain(&ids);
        let (selected, focus) = self.selection.rows(&ids);
        let outer = egui::Rect::from_min_max(
            pos2(o.x + list_left, o.y + chrome::MENU_H + pane_top),
            pos2(o.x + w - 7.0, o.y + h - STRIP_H - 4.0),
        );
        self.list.menu_open = self.popup.is_some() || self.open_menu.is_some();
        // View → Font, as the settings have it.
        self.list.font = self.settings.list_font;
        self.list.bold_names = self.settings.list_bold_names;
        self.list.density = self.settings.density;
        // Another node shows from its top (its rows aren't the last one's).
        if self.list_view != self.view {
            self.list.scroll = 0.0;
            self.list_view = self.view;
        }
        // MDM_BENCH_SCROLL (developers): the list scrolls by itself, timed (framestats.rs).
        if let Some(step) = crate::framestats::bench_scroll(&ctx) {
            self.list.bench_scroll(step, ids.len());
        }
        // The Status cell's tip, the download's recent events: made for the row under the
        // pointer only (twelve dated lines for every row, every frame, before).
        let items = &self.items;
        let tip = |i: usize| {
            ids.get(i)
                .and_then(|id| items.iter().find(|it| it.id == *id))
                .map_or(String::new(), |it| it.log_lines(12))
        };
        let lazy;
        let source: &dyn list::Rows = match &order {
            Ok(order) => {
                lazy = LazyRows {
                    items,
                    order,
                    queued: &queued,
                    extracts: &self.extracts,
                };
                &lazy
            }
            Err(rows) => rows,
        };
        let r = list::show(
            ui,
            outer,
            &mut self.list,
            source,
            &selected,
            focus,
            &mut self.icons,
            &tip,
        );
        if self.list.has_keys() {
            self.tree_keys = false;
        }
        if let Some(pick) = r.pick {
            self.selection.apply(pick, &ids);
        }
        if let Some(i) = r.properties {
            self.properties(ids[i]);
        }
        // A finished download: Open or Properties (IDM's default), as the context menu sets.
        // Enter on the focused row does what a double-click does (as Explorer's Enter runs a
        // row's default command); the item menu's "Enter" moves with it.
        if let Some(i) = r.double_clicked.or(r.enter) {
            let id = ids[i];
            if self.enter_opens() {
                self.open_item(id);
            } else {
                self.properties(id);
            }
        }
        // Something droppable over the window: the list is where it goes.
        let dragging = self.drops.over() || ctx.input(|i| !i.raw.hovered_files.is_empty());
        if dragging {
            ui.painter().rect_stroke(
                winui::rect(outer),
                0.0,
                egui::Stroke::new(2.0, winui::accent()),
                egui::StrokeKind::Inside,
            );
        }
        if ids.is_empty()
            && let Some(cmd) = self.empty_state(ui, r.body.unwrap_or(outer))
        {
            chosen = Some((cmd, None));
        }
        if self.status_strip(ui, o, w, h, ids.len()) {
            chosen = Some((Cmd::ClearFilters, None));
        }
        if let Some((i, pos)) = r.context {
            self.open_popup(Popup::Item(i.map(|i| ids[i])), pos);
        }
        let layout = self.list.layout();
        if self.last_layout.as_ref() != Some(&layout) {
            self.dirty |= self.last_layout.is_some();
            self.last_layout = Some(layout);
        }
        if let Some(pos) = r.header_context {
            self.open_popup(Popup::Columns(r.header_col), pos);
        }
        if let Some((kind, pos, serial)) = self.popup {
            let (entries, target) = match kind {
                // Column chooser (IDM: right-click on the header). File Name always stays.
                Popup::Columns(col) => {
                    // A column by the header's own words, the narrow Q by its whole name.
                    let title = |i: usize| {
                        if list::TITLES[i] == "Q" {
                            crate::i18n::tc("column", "Queue")
                        } else {
                            crate::i18n::tc("column", list::TITLES[i])
                        }
                    };
                    let mut e: Vec<MenuEntry<Cmd>> = Vec::new();
                    // The right-clicked column's filter (its funnel shows only under the
                    // pointer): ticked while it filters. And every filter off.
                    if let Some(c) = col.filter(|c| self.list.is_shown(*c)) {
                        let label = crate::i18n::fmt(
                            // i18n: list header's right-click menu: opens the filter box of the column right-clicked; {column} its title ("Status")
                            crate::i18n::t("F&ilter \"{column}\"..."),
                            &[("column", &title(c).replace('&', "&&"))],
                        );
                        e.push(MenuEntry::Item(
                            crate::chrome::label(&label),
                            Cmd::FilterColumn(c),
                            true,
                            self.list.filters[c].is_some(),
                        ));
                    }
                    e.push(MenuEntry::Item(
                        "Clear all &filters",
                        Cmd::ClearFilters,
                        self.list.filtered(),
                        false,
                    ));
                    e.push(MenuEntry::Sep);
                    // Column chooser (IDM: right-click on the header). File Name always stays.
                    e.extend((0..list::COLS).map(|i| {
                        MenuEntry::Item(
                            crate::chrome::label(&title(i).replace('&', "&&")),
                            Cmd::ToggleColumn(i),
                            i != 0,
                            self.list.is_shown(i),
                        )
                    }));
                    e.push(MenuEntry::Sep);
                    // The columns' window (View → Choose columns): their order, which show,
                    // and Reset, as IDM's header offers it.
                    e.push(MenuEntry::Item(
                        // i18n: list header's right-click menu: opens the window choosing which columns show and in what order
                        "&Columns...",
                        Cmd::CustomizeColumns,
                        true,
                        false,
                    ));
                    e.push(MenuEntry::Item(
                        // i18n: list header's right-click menu: makes every shown column as wide as its title and its cells need
                        "Fit columns to con&tent",
                        Cmd::FitColumns,
                        true,
                        false,
                    ));
                    // i18n: header menu: put the columns back as they first were (order, widths, all shown)
                    e.push(MenuEntry::Item(
                        "&Reset columns",
                        Cmd::ResetColumns,
                        true,
                        false,
                    ));
                    (e, None)
                }
                Popup::Item(None) => (self.blank_entries(), None),
                Popup::Item(id) => (self.item_entries(id), id),
                Popup::QueueDrop(start) => (self.queue_drop(start), None),
                Popup::TorrentDrop => (
                    vec![
                        MenuEntry::Item("Open &torrent file...", Cmd::OpenTorrentFile, true, false),
                        MenuEntry::Item("Add &magnet link...", Cmd::AddMagnet, true, false),
                        MenuEntry::Item("&RSS feeds...", Cmd::RssFeeds(false), true, false),
                        MenuEntry::Sep,
                        MenuEntry::Item("Torrent setti&ngs...", Cmd::TorrentSettings, true, false),
                    ],
                    None,
                ),
                Popup::Node(node) => (self.node_entries(node), None),
                Popup::DropTarget => (
                    vec![
                        MenuEntry::Item("Show DDM", Cmd::ShowMain, true, false),
                        MenuEntry::Item("&Add new download", Cmd::AddUrl, true, false),
                        MenuEntry::Sep,
                        MenuEntry::Item("Hide drop target", Cmd::ToggleDropTarget, true, false),
                    ],
                    None,
                ),
            };
            // Open says "Enter" only where Enter opens (On Double click → Open); Enter shows
            // Properties otherwise, as Alt+Enter does.
            let enter_opens = self.enter_opens();
            match chrome::popup(
                &ctx,
                MenuKind::Context,
                &format!("popup-{serial}"),
                pos,
                &entries,
                &|c: &Cmd| match c {
                    Cmd::Open if !enter_opens => None,
                    c => c.hint(),
                },
            ) {
                PopupResult::Open | PopupResult::Neighbor(_) => {}
                PopupResult::Chosen(c) => {
                    chosen = Some((c, target));
                    self.popup = None;
                }
                PopupResult::Dismissed => self.popup = None,
            }
        }
        // Ctrl+F finds, F3 finds the next match (Downloads menu); Ctrl+N adds a download (not
        // in IDM). The full list: Help → Keyboard shortcuts (`shortcuts.rs`).
        if chosen.is_none() && self.popup.is_none() && self.open_menu.is_none() {
            let (find, next, add) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::F),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::F3),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                )
            });
            // Ctrl+M as IDM; F2 as renaming in Explorer.
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::M)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::F2)
            }) {
                chosen = Some((Cmd::MoveRename, None));
            }
            if find {
                chosen = Some((Cmd::Find, None));
            } else if next {
                chosen = Some((Cmd::FindNext, None));
            } else if add {
                chosen = Some((Cmd::AddUrl, None));
            }
        }
        // Delete removes the selected downloads, Shift+Delete with their files (both ask first);
        // not while a text box (Find, a filter) has the keys.
        if chosen.is_none()
            && self.popup.is_none()
            && self.open_menu.is_none()
            && ctx.memory(|m| m.focused().is_none())
        {
            // On Windows egui reports Shift+Delete as Cut (Ctrl+X is Cut too, without Shift).
            let (del, shift_del) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::Delete) && !i.modifiers.shift,
                    // Windows reports it as Cut; macOS and Linux as the key.
                    i.modifiers.shift
                        && !i.modifiers.command
                        && (i.events.iter().any(|e| matches!(e, egui::Event::Cut))
                            || i.key_pressed(egui::Key::Delete)),
                )
            });
            if shift_del {
                chosen = Some((Cmd::RemoveWithFiles, None));
            } else if del {
                chosen = Some((Cmd::Remove, None));
            }
        }
        if let Some((c, target)) = chosen {
            self.run(c, target, &ctx);
        }
    }

    pub(super) fn paint_frame(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // The freeze watchdog times every frame's drawing, as it times its logic (crash.rs).
        crate::crash::frame_begin();
        // MDM_FRAME_STATS: what this pass costs and why it came (framestats.rs).
        let stats = crate::framestats::begin();
        // A lost graphics device is noticed before this frame paints (renderer.rs).
        crate::renderer::frame_begin();
        // Off-screen test runs: no window of ours may become the foreground one (win32.rs). Linux
        // and macOS need no such pass: their off-screen windows open inactive (main.rs `quiet`)
        // and nothing raises them then (dialogs::raise).
        #[cfg(windows)]
        if crate::offscreen() {
            crate::win32::keep_from_foreground();
        }
        crate::crash::window("main");
        let ctx = ui.ctx().clone();
        // Not in the frame the interface size changed in: the window's rect is measured at the
        // old one then.
        if self.follow_zoom(&ctx) {
            dialogs::publish_main_rect(&ctx);
        }
        // Incoming commands and the clock (downloads, queues, Scheduler, saving…): `logic`, just
        // before, which runs minimized too.
        self.poll_picks(&ctx);
        self.drop_target(&ctx);
        self.main_drops(&ctx);
        self.remember_layout(&ctx);
        // Light or dark: IDM follows Windows' app mode (checked every 2 s) unless View → Dark
        // Mode support is off. Switches live (IDM asks to restart). On Linux the desktop's own
        // change wakes a frame, and the check runs in it rather than at its next 2 s (#156).
        let desktop_changed = crate::theme::system_changed();
        if desktop_changed
            || self
                .theme_checked
                .is_none_or(|t| t.elapsed() > Duration::from_secs(2))
        {
            self.theme_checked = Some(Instant::now());
            // A theme file added, removed or renamed, pywal's colours written or taken away:
            // View → Theme lists it. Looked at beside the window (#743, `themes_seen`), one look
            // at a time; the list follows when the answer comes (`Beside::Themes`).
            if !self.themes_looking {
                self.themes_looking = true;
                let known = (self.themes_changed, self.pywal_there);
                self.ask_beside(move || themes_seen(known));
            }
            let changed = crate::theme::update(
                self.settings.dark_mode_support,
                self.settings.always_dark,
                &ctx,
            );
            // Native follows the system's accent and contrast as they change, a theme made from
            // pywal's colours the colours `wal` writes for each new wallpaper.
            let changed = if crate::theme::follow_system(&ctx) || crate::theme::follow_pywal() {
                crate::theme::apply_theme(&ctx);
                true
            } else {
                changed
            };
            // The caption follows (first check: the window may not exist yet; retried). Only
            // then: asking DWM every 2 s caught the window's thread in a stalled system (#633).
            if changed || !self.caption_set {
                crate::theme::apply_mode(&ctx);
                #[cfg(windows)]
                {
                    self.caption_set =
                        crate::win32::caption_dark(crate::MAIN_TITLE, crate::theme::dark_surface());
                    // The menus Windows draws (the tray icon's, the window menu) too (#682).
                    crate::win32::menus_dark(crate::theme::dark_surface());
                }
                // Linux: the tray's menu is drawn by the desktop's StatusNotifierItem host (ksni
                // sends only its items), in the desktop's theme; no app can colour it. macOS: the
                // menu bar item's menu follows the menu bar's appearance, the system's.
                #[cfg(not(windows))]
                {
                    self.caption_set = true;
                }
            }
            if changed {
                ctx.request_repaint();
            }
        }
        {
            let _main = crate::framestats::span("main");
            egui::CentralPanel::no_frame()
                .frame(egui::Frame::new().fill(chrome::WINDOW_BG()))
                .show(ui, |ui| self.paint(ui));
        }
        let _dialogs = crate::framestats::span("windows");
        self.dialogs(&ctx);
        drop(_dialogs);
        crate::framestats::pass_done(&ctx, stats);
        crate::crash::frame_end();
        crate::renderer::frame_done(&ctx);
    }
}
