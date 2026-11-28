//! Options → Accounts (#44, ours): debrid service accounts, added and edited in a window of their
//! own.

use super::*;

/// Accounts → Add...: the service, a token typed in, and the sign-in on its thread (#44).
pub(super) struct AccountEdit {
    pub(super) service: mdm_debrid::ServiceKind,
    pub(super) token: String,
    pub(super) signin: crate::debrid::SignIn,
}

/// It starts on the first service Options lists (not `ServiceKind`'s default, Real-Debrid,
/// which saved accounts fall back to).
impl Default for AccountEdit {
    fn default() -> Self {
        AccountEdit {
            service: mdm_debrid::ServiceKind::ALL[0],
            token: String::new(),
            signin: Default::default(),
        }
    }
}

impl OptionsDialog {
    /// Accounts (#44, ours): debrid service accounts that download file-host links. The list is
    /// asked in its order; a link goes through the first account whose service takes its host.
    pub(super) fn accounts(&mut self, ui: &mut Ui, o: Pos2) {
        header(ui, o, Icon::PageLogins, "Debrid service accounts");
        let vault = crate::secrets::vault();
        let rows: Vec<Vec<String>> = self
            .s
            .debrid_accounts
            .iter()
            .map(|a| {
                vec![
                    a.service.name().to_owned(),
                    a.user.clone(),
                    if a.account().is_none() {
                        if vault.unread(&format!("debrid/{}", a.id))
                            && vault.kept() == crate::secrets::Kept::Locked
                        {
                            // i18n: Options → Accounts: the account's sign-in is in the system's keyring (Linux), which is locked now; DDM never unlocks it
                            crate::i18n::t("in the locked keyring").to_owned()
                        } else {
                            // i18n: Options → Accounts: the account's sign-in can't be read (signed out, or the settings came from another computer)
                            crate::i18n::t("signed out: sign in again").to_owned()
                        }
                    } else {
                        a.state.clone()
                    },
                ]
            })
            .collect();
        let (_, del_key) = report(
            ui,
            o,
            r(o, 21.0, 93.0, 383.0, 124.0),
            &[
                // i18n: column title: the debrid service (Real-Debrid…)
                (crate::i18n::mark("Service"), 110.0),
                // i18n: column title: the account's name at the service
                (crate::i18n::mark("Account"), 205.0),
                // i18n: column title: the account's state (premium, days left)
                (crate::i18n::mark("State"), 387.0),
            ],
            &rows,
            &mut self.account_rows,
            false,
            "opt-accounts",
        );
        let sel = self.account_rows.selected.filter(|i| *i < rows.len());
        // i18n: button: add a debrid service account (opens the sign-in)
        let add = winui::button_light(
            ui,
            r(o, 21.0, 225.0, 90.0, 21.0),
            "Add...",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        let kind = |on: bool| {
            if on {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            }
        };
        let remove = winui::button_light(
            ui,
            r(o, 117.0, 225.0, 90.0, 21.0),
            "Remove",
            kind(sel.is_some()),
            false,
        )
        .clicked()
            || del_key;
        // i18n: button (short): move the account up the list (accounts are asked in the list's order)
        let up = winui::button_light(
            ui,
            r(o, 213.0, 225.0, 90.0, 21.0),
            "Up",
            kind(sel.is_some_and(|i| i > 0)),
            false,
        )
        .clicked();
        // i18n: button (short): move the account down the list
        let down = winui::button_light(
            ui,
            r(o, 309.0, 225.0, 95.0, 21.0),
            "Down",
            kind(sel.is_some_and(|i| i + 1 < rows.len())),
            false,
        )
        .clicked();
        if add {
            self.sub = Some(Sub::Account(AccountEdit::default()));
        }
        if let Some(i) = sel {
            let list = &mut self.s.debrid_accounts;
            if remove {
                list.remove(i);
                self.account_rows.selected = None;
            } else if up && i > 0 {
                list.swap(i, i - 1);
                self.account_rows.selected = Some(i - 1);
            } else if down && i + 1 < list.len() {
                list.swap(i, i + 1);
                self.account_rows.selected = Some(i + 1);
            }
        }
        // Experimental until each service is confirmed with a real account (#44).
        let p = ui.painter().clone();
        let note = crate::i18n::t(
            // i18n: Options → Accounts: a note that the debrid services' support (Real-Debrid, AllDebrid, Premiumize) is new and not yet tried with real paid accounts
            "Experimental: built from the services' public API documentation, not yet confirmed with real accounts.",
        );
        winui::paragraph(
            &p,
            o.x + 21.0,
            o.y + 256.0,
            note,
            crate::theme::colors().warning,
            o.x + 404.0,
            14.0,
        );
        winui::checkbox_light(
            ui,
            o + vec2(21.0, 302.0),
            &mut self.s.debrid_on,
            // i18n: Options → Accounts: checkbox: links of the file hosts these services support are downloaded through the accounts
            "Download supported file-host links through these accounts",
            true,
        );
        winui::checkbox_light(
            ui,
            o + vec2(21.0, 324.0),
            &mut self.s.debrid_torrents,
            // i18n: Options → Accounts: checkbox: magnet links and .torrent files go to the first account's service, which downloads them; DDM then downloads the files from it (no connections to peers)
            "Send torrents and magnet links to the first account instead of to peers",
            self.s.debrid_on,
        );
        let p = ui.painter().clone();
        let lines = winui::paragraph(
            &p,
            o.x + 21.0,
            o.y + 358.0,
            // i18n: Options → Accounts: what these accounts are, and the services' rule against sharing
            crate::i18n::t(
                "A debrid service downloads a file host's link with your own paid account and gives DDM a direct link. Use only your own account: sharing an account or its links breaks the services' terms and can get it suspended. Downloads the browser already started stay as they are.",
            ),
            LIGHT_TEXT(),
            o.x + 404.0,
            14.0,
        );
        // Where the sign-ins are kept (#44, secrets.rs); the list is two rows shorter for it.
        let kept = vault.kept();
        if kept == crate::secrets::Kept::Looking {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(text) = kept_text(kept) {
            winui::paragraph(
                &p,
                o.x + 21.0,
                o.y + 358.0 + lines as f32 * 14.0 + 6.0,
                crate::i18n::t(text),
                LIGHT_TEXT(),
                o.x + 404.0,
                14.0,
            );
        }
    }
}

/// Accounts → Add...: sign in by code in the browser (Real-Debrid's device flow, AllDebrid's PIN),
/// or paste the API token (Premiumize: its API key only); the account is checked before it is kept.
pub(super) fn account_dialog(
    ui: &mut Ui,
    o: Pos2,
    e: &mut AccountEdit,
) -> Outcome<settings::DebridAccount> {
    use crate::debrid::SignInState as S;
    let p = ui.painter().clone();
    let ctx = ui.ctx().clone();
    let state = e.signin.get();
    let busy = matches!(state, S::Starting | S::Waiting { .. } | S::Checking);
    // What the sign-in text took beyond its three lines: everything under it moves down.
    let extra = crate::dialogs::grow_extra(&ctx, "opt-account");
    // i18n: Options → Add an account: label before the choice of debrid service
    text(
        &p,
        o.x + 14.0,
        o.y + 26.0,
        crate::i18n::t("Service:"),
        LIGHT_TEXT(),
    );
    let all = mdm_debrid::ServiceKind::ALL;
    let names: Vec<&str> = all.iter().map(|k| k.name()).collect();
    let at = all.iter().position(|k| *k == e.service).unwrap_or(0);
    if let Some(i) = winui::combo_light(
        ui,
        r(o, 90.0, 12.0, 200.0, 21.0),
        &names,
        at,
        !busy,
        "opt-account-service",
    ) && all[i] != e.service
    {
        // Another service: what was signed in to (or typed) belongs to the one before.
        e.service = all[i];
        e.token.clear();
        *e.signin.state.lock().unwrap() = S::Idle;
    }
    let by_code = e.service.signs_in_by_code();
    let kind = |on: bool| {
        if on {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        }
    };
    // i18n: button: sign in to the debrid service in the browser, by typing a code there
    if winui::button_light(
        ui,
        r(o, 14.0, 44.0, 220.0, 23.0),
        "Sign in in the browser",
        kind(!busy && by_code),
        false,
    )
    .clicked()
    {
        e.signin
            .stop
            .store(false, std::sync::atomic::Ordering::Relaxed);
        e.signin.by_code(e.service, &ctx);
    }
    if let S::Waiting { code, page, until } = &state {
        // The code, large, to type at the service's page.
        let code_font = egui::FontId::monospace(crate::theme::metrics().font_code);
        let code_w = winui::text_width_font(&p, code_font.clone(), code);
        // At the leading edge: mirrored in a right-to-left interface, as the buttons are.
        p.text(
            winui::at(o + vec2(14.0, 78.0), code_w),
            egui::Align2::LEFT_TOP,
            code,
            code_font,
            LIGHT_TEXT(),
        );
        // The buttons as wide as their labels in this language (#192), short of the code.
        let code_end = 14.0 + code_w;
        let [copy_at, open_at] = winui::button_row(
            &p,
            &[("Copy code", 200.0, 90.0), ("Open the page", 296.0, 110.0)],
            6.0,
            (code_end + 12.0).min(200.0),
            200.0,
        )[..] else {
            unreachable!("two buttons")
        };
        // i18n: button: copy the sign-in code
        if winui::button_light(
            ui,
            r(o, copy_at.0, 80.0, copy_at.1, 21.0),
            "Copy code",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            crate::copy_text(&ctx, code.clone());
        }
        // i18n: button: open the service's page where the code is typed
        if winui::button_light(
            ui,
            r(o, open_at.0, 80.0, open_at.1, 21.0),
            "Open the page",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            open_page(page);
        }
        let left = until
            .saturating_duration_since(std::time::Instant::now())
            .as_secs();
        let lines = winui::paragraph(
            &p,
            o.x + 14.0,
            o.y + 120.0,
            &crate::i18n::fmt(
                // i18n: Options → Add an account: waiting while the user types the code at the service's page; {page} its address, {time} minutes:seconds left
                crate::i18n::t(
                    "Type this code at {page} and allow DDM; waiting for you (the code expires in {time}).",
                ),
                &[
                    ("page", &crate::i18n::ltr(page)),
                    ("time", &format!("{}:{:02}", left / 60, left % 60)),
                ],
            ),
            LIGHT_TEXT(),
            o.x + 406.0,
            14.0,
        );
        // Three lines fit above the token's label; a longer page or language grows the dialog.
        crate::dialogs::grow_px(&ctx, "opt-account", lines.saturating_sub(3) as f32 * 14.0);
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    } else {
        crate::dialogs::grow_px(&ctx, "opt-account", 0.0);
        // Before the sign-in: what it does, where its code will show.
        if by_code && matches!(state, S::Idle) {
            winui::paragraph(
                &p,
                o.x + 14.0,
                o.y + 120.0,
                // i18n: Options → Add an account: under "Sign in in the browser", before it is pressed: what happens then
                crate::i18n::t(
                    "The service's page opens in your browser, and a code shows here: type it there and allow DDM.",
                ),
                winui::LIGHT_DISABLED(),
                o.x + 406.0,
                14.0,
            );
        }
    }
    let label = if by_code {
        // i18n: Options → Add an account: label before the field for the service's API token (the other way to sign in)
        crate::i18n::t("Or paste your API token:")
    } else {
        // i18n: Options → Add an account: label before the field for the service's API key (Premiumize: the only way to sign in; the key is on the service's Account page)
        crate::i18n::t("Paste your API key (from the service's Account page):")
    };
    text(&p, o.x + 14.0, o.y + 178.0 + extra, label, LIGHT_TEXT());
    // As wide as its label in this language (#192); the box ends 6 px before it.
    let (use_x, use_w) =
        winui::buttons_to_right(&p, 406.0, 0.0, &[("Use the token", 106.0)], 200.0)[0];
    winui::password_light(
        ui,
        r(o, 14.0, 186.0 + extra, use_x - 6.0 - 14.0, 21.0),
        &mut e.token,
        !busy,
    );
    // i18n: button: sign in with the token typed in
    if winui::button_light(
        ui,
        r(o, use_x, 186.0 + extra, use_w, 21.0),
        "Use the token",
        kind(!busy && !e.token.trim().is_empty()),
        false,
    )
    .clicked()
    {
        e.signin.by_token(e.service, e.token.clone(), &ctx);
    }
    // i18n: button: open the service's page that shows the user's API token
    if winui::button_light(
        ui,
        r(
            o,
            14.0,
            212.0 + extra,
            winui::button_fit_width(&p, "Find my token", 160.0, 392.0),
            21.0,
        ),
        "Find my token",
        ButtonKind::Normal,
        false,
    )
    .clicked()
    {
        open_page(e.service.token_page());
    }
    let (line, ok) = match &state {
        // i18n: Options → Add an account: the sign-in is starting or being checked
        S::Starting | S::Checking => (crate::i18n::t("Asking the service...").to_owned(), true),
        S::Done { user, state, .. } => (
            crate::i18n::fmt(
                // i18n: Options → Add an account: signed in; {user} the account's name, {state} premium or not
                crate::i18n::t("Signed in as {user} ({state})"),
                &[("user", &crate::i18n::ltr(user)), ("state", state)],
            ),
            true,
        ),
        S::Failed(why) => (why.clone(), false),
        _ => (String::new(), true),
    };
    let color = if ok {
        LIGHT_TEXT()
    } else {
        crate::theme::colors().error_strong
    };
    let (rect, cut) = winui::label_into(
        &p,
        o.x + 14.0,
        o.y + 252.0 + extra,
        &line,
        color,
        o.x + 406.0,
        o.x + 14.0,
    );
    if cut {
        winui::full_on_hover(ui, rect, "opt-account-state", &line);
    }
    let done = matches!(state, S::Done { .. });
    match ok_cancel(ui, o, 252.0, 266.0 + extra, done) {
        Outcome::Done(()) => match state {
            S::Done {
                user,
                state,
                credential,
            } => Outcome::Done(settings::DebridAccount {
                id: 0,
                service: e.service,
                enabled: true,
                user,
                state,
                secret: credential,
            }),
            _ => Outcome::Open,
        },
        Outcome::Closed => Outcome::Closed,
        Outcome::Open => Outcome::Open,
    }
}

/// Open a service's web page in the browser; never in an off-screen test run (MDM_OFFSCREEN),
/// whose clicks come from scripts, not from a person at the browser.
pub(super) fn open_page(url: &str) {
    if !crate::offscreen() {
        crate::launch::open(url);
    }
}
