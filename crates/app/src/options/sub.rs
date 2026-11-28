//! The windows Options opens over itself (`Sub`): each one shown, and its answer taken back into
//! the draft settings.

use super::*;

/// A small entry dialog opened from a page.
pub(super) enum Sub {
    /// Sites Logins → New / Edit: (row being edited, site/path, user, password).
    Login(Option<usize>, String, String, String),
    /// Connection → New / Edit exception: (row, server, number).
    Exception(Option<usize>, String, String),
    /// General → Download panels → Edit...: the button over videos, as edited.
    Panels(Box<PanelsEdit>),
    /// General → Keys...: the keys a click is held with, and "Also take files the browser
    /// would open".
    Keys(KeysEdit),
    /// General → Customize DDM menu items... → Edit...: the browsers' right-click items shown
    /// (`mdm_ipc::capture::MENU_ITEMS`' order).
    Menu([bool; 5]),
    /// General → Test browser integration...: the checks, as found when it opened.
    Test(Vec<crate::extension::Check>),
    /// File types → "Don't start downloading automatically from the following addresses" →
    /// Edit list...: one address pattern per line.
    Addresses(String),
    /// Connection → DNS → Settings...: how the DNS is asked, its servers, a test.
    Dns(DnsEdit),
    /// Accounts → Add...: the service, a pasted token, and the sign-in running (#44).
    Account(AccountEdit),
    /// Remote → Pair a device...: the code shown, until a device pairs or it is closed.
    Pair,
    /// General → Add browser...: (the name it is listed by, as typed; its program).
    AddBrowser(String, String),
    /// Proxy / Socks → Advanced / Socks...: the proxy's kind, address and login, as edited.
    Socks(SocksEdit),
    /// Downloads → Virus checking settings → Edit...: as edited.
    Virus(VirusEdit),
    /// Downloads → Customize "Download progress" dialog → Edit...: how a progress window opens,
    /// and which of its tabs show.
    Progress(ProgressEdit),
}

impl OptionsDialog {
    pub(super) fn sub_dialog(&mut self, ctx: &egui::Context) {
        let Some(sub) = self.sub.take() else { return };
        match sub {
            Sub::Pair => {
                if self.pair_window(ctx) {
                    self.sub = Some(Sub::Pair);
                } else if let Some(r) = &self.remote {
                    r.stop_pairing();
                }
            }
            Sub::Login(row, mut site, mut user, mut pass) => {
                let title = if row.is_some() {
                    crate::i18n::mark("Edit login")
                } else {
                    crate::i18n::mark("New login")
                };
                let out = window(
                    ctx,
                    "opt-login",
                    title,
                    [300.0, 150.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| {
                        let p = ui.painter().clone();
                        // i18n: label before a web address (site and folder)
                        winui::text_right(
                            &p,
                            o.x + 70.0,
                            o.y + 26.0,
                            crate::i18n::t("Site/path:"),
                            LIGHT_TEXT(),
                        );
                        winui::edit_light(
                            ui,
                            r(o, 78.0, 12.0, 210.0, 23.0),
                            &mut site,
                            true,
                            false,
                        );
                        // i18n: noun: label before the user name
                        winui::text_right(
                            &p,
                            o.x + 70.0,
                            o.y + 57.0,
                            crate::i18n::t("User:"),
                            LIGHT_TEXT(),
                        );
                        winui::edit_light(
                            ui,
                            r(o, 78.0, 43.0, 210.0, 23.0),
                            &mut user,
                            true,
                            false,
                        );
                        winui::text_right(
                            &p,
                            o.x + 70.0,
                            o.y + 88.0,
                            crate::i18n::t("Password:"),
                            LIGHT_TEXT(),
                        );
                        password(ui, r(o, 78.0, 74.0, 210.0, 23.0), &mut pass, true);
                        ok_cancel(ui, o, 136.0, 117.0, !site.trim().is_empty())
                    },
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Login(row, site, user, pass)),
                    Outcome::Done(()) => {
                        let entry = (site.trim().to_owned(), user, pass);
                        match row {
                            Some(i) => self.s.site_logins[i] = entry,
                            None => self.s.site_logins.push(entry),
                        }
                    }
                    Outcome::Closed => {}
                }
            }
            Sub::Exception(row, mut server, mut number) => {
                let limit = self.s.connection_limit();
                let title = if row.is_some() {
                    crate::i18n::mark("Edit exception")
                } else {
                    crate::i18n::mark("New exception")
                };
                let out = window(
                    ctx,
                    "opt-exception",
                    title,
                    [300.0, 118.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| {
                        let p = ui.painter().clone();
                        // i18n: noun: label before a server name
                        winui::text_right(
                            &p,
                            o.x + 70.0,
                            o.y + 26.0,
                            crate::i18n::t("Server:"),
                            LIGHT_TEXT(),
                        );
                        winui::edit_light(
                            ui,
                            r(o, 78.0, 12.0, 210.0, 23.0),
                            &mut server,
                            true,
                            false,
                        );
                        // i18n: label before the number of connections allowed
                        winui::text_right(
                            &p,
                            o.x + 70.0,
                            o.y + 57.0,
                            crate::i18n::t("Connections:"),
                            LIGHT_TEXT(),
                        );
                        winui::spin_box(
                            ui,
                            r(o, 78.0, 43.0, 60.0, 23.0),
                            &mut number,
                            1,
                            u64::from(limit),
                            true,
                            "opt-exception-number",
                        );
                        let in_range = number
                            .parse::<u32>()
                            .is_ok_and(|n| (1..=limit).contains(&n));
                        ok_cancel(ui, o, 136.0, 85.0, !server.trim().is_empty() && in_range)
                    },
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Exception(row, server, number)),
                    Outcome::Done(()) => {
                        let entry = (
                            server.trim().to_owned(),
                            number.parse().unwrap_or(1).clamp(1, limit),
                        );
                        match row {
                            Some(i) => self.s.conn_exceptions[i] = entry,
                            None => self.s.conn_exceptions.push(entry),
                        }
                    }
                    Outcome::Closed => {}
                }
            }
            Sub::Panels(mut e) => {
                let out = window(
                    ctx,
                    "opt-panels",
                    // i18n: window title: the download button the extension shows over videos
                    "Download panels",
                    [400.0, 292.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| panels_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Panels(e)),
                    Outcome::Done(()) => (*e).save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Dns(mut e) => {
                let out = window(
                    ctx,
                    "opt-dns",
                    // i18n: window title: the settings of the DNS chosen in Options → Connection
                    "DNS settings",
                    [400.0, 312.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| dns_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Dns(e)),
                    Outcome::Done(()) => e.save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Account(mut e) => {
                let out = window(
                    ctx,
                    "opt-account",
                    // i18n: window title: adding a debrid service account (Real-Debrid…)
                    "Add an account",
                    [
                        420.0,
                        300.0 + crate::dialogs::grow_extra(ctx, "opt-account"),
                    ],
                    false,
                    LIGHT_BG(),
                    |ui, o| account_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Account(e)),
                    Outcome::Done(account) => {
                        let mut account = account;
                        account.id = self
                            .s
                            .debrid_accounts
                            .iter()
                            .map(|a| a.id)
                            .max()
                            .unwrap_or(0)
                            + 1;
                        self.s.debrid_accounts.push(account);
                    }
                    Outcome::Closed => e
                        .signin
                        .stop
                        .store(true, std::sync::atomic::Ordering::Relaxed),
                }
            }
            Sub::Addresses(mut list) => {
                let out = window(
                    ctx,
                    "opt-addresses",
                    // i18n: window title: the addresses whose downloads stay with the browser
                    "Addresses left to the browser",
                    [420.0, 230.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| {
                        let p = ui.painter().clone();
                        winui::paragraph(
                            &p,
                            o.x + 14.0,
                            o.y + 20.0,
                            crate::i18n::t(
                                "One address per line; * stands for any text: example.com/private/*",
                            ),
                            LIGHT_TEXT(),
                            o.x + 406.0,
                            14.0,
                        );
                        multiline(
                            ui,
                            r(o, 14.0, 44.0, 392.0, 130.0),
                            &mut list,
                            "addresses",
                            true,
                        );
                        ok_cancel(ui, o, 252.0, 192.0, true)
                    },
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Addresses(list)),
                    Outcome::Done(()) => {
                        self.s.excluded_addresses = list
                            .lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                            .map(str::to_owned)
                            .collect();
                    }
                    Outcome::Closed => {}
                }
            }
            Sub::Keys(mut e) => {
                let out = window(
                    ctx,
                    "opt-keys",
                    // i18n: window title: the keys that force or prevent downloading with DDM
                    "Keys",
                    [440.0, 246.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| keys_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Keys(e)),
                    Outcome::Done(()) => e.save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Menu(mut shown) => {
                let out = window(
                    ctx,
                    "opt-menu",
                    // i18n: window title: the commands DDM adds to the browsers' right-click menu
                    "Browser menu items",
                    [400.0, 214.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| menu_dialog(ui, o, &mut shown),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Menu(shown)),
                    Outcome::Done(()) => {
                        self.s.browser_menu_hidden = mdm_ipc::capture::MENU_ITEMS
                            .iter()
                            .zip(shown)
                            .filter(|(_, on)| !on)
                            .map(|(item, _)| (*item).to_owned())
                            .collect();
                    }
                    Outcome::Closed => {}
                }
            }
            Sub::AddBrowser(mut name, program) => {
                let taken = |name: &str| {
                    self.s
                        .browsers
                        .iter()
                        .any(|(b, _)| b.eq_ignore_ascii_case(name.trim()))
                };
                let out = window(
                    ctx,
                    "opt-add-browser",
                    // i18n: window title: Options → General → Add browser... (its name, after its program was chosen)
                    "Add browser",
                    [420.0, 150.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| {
                        let p = ui.painter().clone();
                        winui::text_right(
                            &p,
                            o.x + 80.0,
                            o.y + 26.0,
                            crate::i18n::t("Name:"),
                            LIGHT_TEXT(),
                        );
                        winui::edit_light(
                            ui,
                            r(o, 88.0, 12.0, 318.0, 23.0),
                            &mut name,
                            true,
                            false,
                        );
                        winui::text_right(
                            &p,
                            o.x + 80.0,
                            o.y + 57.0,
                            // i18n: Options → General → Add browser: label before the browser's program (its file)
                            crate::i18n::t("Program:"),
                            LIGHT_TEXT(),
                        );
                        readonly_ltr(ui, r(o, 88.0, 43.0, 318.0, 23.0), &program);
                        let valid = !name.trim().is_empty() && !taken(&name);
                        if taken(&name) {
                            winui::text(
                                &p,
                                o.x + 88.0,
                                o.y + 86.0,
                                // i18n: Options → General → Add browser: why OK is greyed: the name typed is one the browsers list has already
                                crate::i18n::t("The list has a browser of that name already."),
                                winui::ERROR(),
                            );
                        }
                        ok_cancel(ui, o, 256.0, 117.0, valid)
                    },
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::AddBrowser(name, program)),
                    Outcome::Done(()) => {
                        let name = name.trim().to_owned();
                        self.s.custom_browsers.push((name.clone(), program));
                        self.s.browsers.push((name, true));
                        self.browser_rows.selected = Some(self.s.browsers.len() - 1);
                    }
                    Outcome::Closed => {}
                }
            }
            Sub::Socks(mut e) => {
                let out = window(
                    ctx,
                    "opt-socks",
                    // i18n: window title: Options → Proxy / Socks → Advanced / Socks...
                    "Advanced / Socks",
                    [420.0, 262.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| socks_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Socks(e)),
                    Outcome::Done(()) => e.save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Virus(mut e) => {
                let out = window(
                    ctx,
                    "opt-virus",
                    // i18n: window title: Options → Downloads → Virus checking settings → Edit...
                    "Virus checking",
                    [470.0, 262.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| virus_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Virus(e)),
                    Outcome::Done(()) => e.save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Progress(mut e) => {
                let out = window(
                    ctx,
                    "opt-progress",
                    // i18n: window title: how a download's progress window opens (Options → Downloads → The progress window → Edit...)
                    "Progress window",
                    [380.0, 280.0],
                    false,
                    LIGHT_BG(),
                    |ui, o| progress_dialog(ui, o, &mut e),
                );
                match out {
                    Outcome::Open => self.sub = Some(Sub::Progress(e)),
                    Outcome::Done(()) => e.save(&mut self.s),
                    Outcome::Closed => {}
                }
            }
            Sub::Test(mut checks) => {
                let h = 120.0 + checks.len() as f32 * 22.0;
                let mut set_up = false;
                let mut offer = false;
                let out = window(
                    ctx,
                    "opt-test",
                    // i18n: window title: Options → General → Test browser integration
                    "Test browser integration",
                    [440.0, h],
                    false,
                    LIGHT_BG(),
                    |ui, o| {
                        let p = ui.painter().clone();
                        for (k, c) in checks.iter().enumerate() {
                            let y = o.y + 22.0 + k as f32 * 22.0;
                            let (mark, color) = if c.ok {
                                ("\u{2714}", winui::SUCCESS())
                            } else {
                                ("\u{2716}", winui::ERROR())
                            };
                            winui::text_in_window(ui, &p, o.x + 14.0, y, mark, color);
                            winui::text_in_window(ui, &p, o.x + 34.0, y, &c.text, LIGHT_TEXT());
                        }
                        let first = checks.iter().find(|c| !c.ok);
                        let y = h - 46.0;
                        // As wide as their labels in this language (#192), short of Close.
                        let [fix_at, set_up_at] = winui::button_row_from_left(
                            &p,
                            &[("How to fix it", 14.0, 110.0), ("Set it up", 132.0, 110.0)],
                            8.0,
                            440.0 - 14.0 - 73.0 - 8.0,
                            200.0,
                        )[..] else {
                            unreachable!("two buttons")
                        };
                        if let Some(c) = first {
                            if let Some(why) = c.why
                                && winui::button_light(
                                    ui,
                                    r(o, fix_at.0, y, fix_at.1, 25.0),
                                    // i18n: button in Test browser integration: open the help page's section for the problem
                                    "How to fix it",
                                    ButtonKind::Normal,
                                    false,
                                )
                                .clicked()
                            {
                                crate::help::open_page(&mdm_ipc::extension::help_doc(why));
                            }
                            if c.why == Some("not-installed")
                                && mdm_ipc::portable_dir().is_none()
                                && winui::button_light(
                                    ui,
                                    r(o, set_up_at.0, y, set_up_at.1, 25.0),
                                    // i18n: button in Test browser integration: register DDM's helper with the browsers
                                    "Set it up",
                                    ButtonKind::Normal,
                                    false,
                                )
                                .clicked()
                            {
                                set_up = true;
                            }
                            // No extension yet: the window that adds it, which each start offers
                            // until its "Don't ask again" (this is the way back to it).
                            if c.offer {
                                let (x, w) = winui::button_row_from_left(
                                    &p,
                                    &[("Add to your browser...", 14.0, 160.0)],
                                    8.0,
                                    440.0 - 14.0 - 73.0 - 8.0,
                                    280.0,
                                )[0];
                                if winui::button_light(
                                    ui,
                                    r(o, x, y, w, 25.0),
                                    // i18n: button in Test browser integration when no browser's extension has connected: opens the window that adds DDM's extension to the browsers installed
                                    "Add to your browser...",
                                    ButtonKind::Normal,
                                    false,
                                )
                                .clicked()
                                {
                                    offer = true;
                                }
                            }
                        }
                        let close = winui::button_light(
                            ui,
                            r(o, 440.0 - 14.0 - 73.0, y, 73.0, 25.0),
                            "Close",
                            ButtonKind::Default,
                            false,
                        );
                        if close.clicked() || winui::enter_pressed(ui) || winui::escape_pressed(ui)
                        {
                            Outcome::<()>::Closed
                        } else {
                            Outcome::Open
                        }
                    },
                );
                // Set it up runs on a thread of its own; the checks are read again once it's done.
                if set_up {
                    crate::extension::set_up(ctx);
                }
                // The app opens the extension's window (`offer_extension`).
                self.offer_extension |= offer;
                if crate::extension::set_up_finished(&mut self.set_up_seen) {
                    checks = crate::extension::checks(&self.s);
                }
                if matches!(out, Outcome::Open) {
                    self.sub = Some(Sub::Test(checks));
                }
            }
        }
    }
}
