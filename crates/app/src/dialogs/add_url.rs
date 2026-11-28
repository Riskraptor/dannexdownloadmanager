//! Add URL (Tasks → Add new download): the address, its login, and a new address taken from the
//! clipboard.

use super::*;

/// The checkbox over a user name and password (Add URL, Batch download): the server's login is
/// sent with the download.
// i18n: checkbox (Add URL, Batch download) over the user name and password boxes: the server asks for a login
pub const LOG_IN: &str = crate::i18n::mark("Log in to the server");

// ---- Add a download (Add URL; client 570 × 112) ---------------------------------------------

#[derive(Default)]
pub struct AddUrl {
    pub address: String,
    pub auth: bool,
    pub login: String,
    pub password: String,
    pub(super) error: Option<String>,
    /// The addresses entered before, newest first (the Address drop list).
    pub(super) history: Vec<String>,
    pub(super) list_open: bool,
    /// The address Options → Sites Logins was last looked up for, and the login it gave
    /// (#511): "Log in to the server" shows it while the box is left as it was filled.
    pub(super) looked_up: String,
    pub(super) filled: Option<(String, String)>,
    /// The clipboard's link, read off the window's thread (#661).
    pub(super) clip: crate::clipboard::Prefill,
}

pub struct NewUrl {
    pub url: Url,
    pub login: Option<(String, String)>,
}

impl AddUrl {
    /// Opens like IDM's: pre-filled with the clipboard's text when it is an http(s) URL (or a
    /// magnet link: OK then opens Add torrent). The clipboard is read off the window's thread,
    /// and the address put in the box on the frame it comes ([`AddUrl::fill_from_clipboard`]).
    pub fn from_clipboard(history: &[String]) -> Self {
        AddUrl {
            history: history.to_vec(),
            clip: crate::clipboard::Prefill::ask(),
            ..AddUrl::default()
        }
    }

    /// The clipboard's text in the box, when it is an address DDM downloads and the box is
    /// still empty (nothing typed or picked meanwhile).
    pub(super) fn fill_from_clipboard(&mut self, clip: &str) {
        let clip = clip.trim();
        if !self.address.is_empty() {
            return;
        }
        if let Ok(u) = Url::parse(clip)
            && (mdm_engine::supported(&u) || u.scheme() == "magnet")
        {
            self.address = clip.to_owned();
        }
    }

    /// "Log in to the server" shows the Sites Logins entry the download will use for the address
    /// (#511): `site_login` is that same match (`Settings::site_login`). A login typed or
    /// changed in the box is left alone; one filled from Options follows the address.
    pub(super) fn follow_site_login(
        &mut self,
        site_login: impl Fn(&Url) -> Option<(String, String)>,
    ) {
        if self.looked_up == self.address {
            return;
        }
        self.looked_up.clone_from(&self.address);
        let found = crate::model::parse_address(&self.address)
            .ok()
            .and_then(|u| site_login(&u));
        if found == self.filled {
            return;
        }
        let as_filled = match &self.filled {
            Some((user, pass)) => self.auth && self.login == *user && self.password == *pass,
            None => !self.auth && self.login.is_empty() && self.password.is_empty(),
        };
        if as_filled {
            self.auth = found.is_some();
            (self.login, self.password) = found.clone().unwrap_or_default();
        }
        self.filled = found;
    }

    /// The login OK hands on: none while the box shows the Sites Logins entry as it was filled,
    /// so the download keeps following Options, as it does without one.
    pub(super) fn typed_login(&self) -> Option<(String, String)> {
        let own = (self.login.clone(), self.password.clone());
        (self.auth && self.filled.as_ref() != Some(&own)).then_some(own)
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        site_login: impl Fn(&Url) -> Option<(String, String)>,
    ) -> Outcome<NewUrl> {
        if let Some(clip) = self.clip.take() {
            self.fill_from_clipboard(&clip);
        }
        self.follow_site_login(site_login);
        window(
            ctx,
            "add-url",
            // i18n: title of the window Add URL opens: one address to download
            "Add a download",
            [570.0, 112.0],
            false,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                // i18n: noun: the web address (URL)
                // The address's caption ends before its box; a longer translation moves the box
                // along (it keeps its right end, under OK's left).
                let address = crate::i18n::t("Address");
                let addr_x = (12.0 + winui::text_width(&p, address).ceil() + 8.0).max(70.0);
                winui::text(&p, o.x + 12.0, o.y + 27.0, address, winui::TEXT());
                let addr_rc = r(o, addr_x, 12.0, 466.0 - addr_x, 21.0);
                let addr = winui::combo_edit(ui, addr_rc, &mut self.address);
                // An error about the address goes once it is edited.
                if addr.changed() {
                    self.error = None;
                }
                if ui.memory(|m| m.focused().is_none()) {
                    addr.request_focus();
                }
                // The arrow: the addresses entered before.
                if winui::combo_arrow(ui, addr_rc, "add-url", self.list_open).clicked()
                    || winui::combo_keys(ui, &addr)
                {
                    self.list_open = !self.list_open;
                }
                // Up / Down step through the addresses entered before.
                let at = self.history.iter().position(|h| *h == self.address.trim());
                if !self.list_open
                    && let Some(i) = winui::history_keys(ui, &addr, self.history.len(), at)
                {
                    self.address = self.history[i].clone();
                    self.error = None;
                }
                // While its list is open, Escape closes the list and the box keeps the keys.
                winui::keep_keys(ui, &addr, false, true, self.list_open);
                let list_was_open = self.list_open;
                if self.list_open
                    && let Some(i) = winui::drop_list(
                        ui,
                        "add-url-history",
                        addr_rc,
                        &self.history,
                        crate::i18n::t("No addresses yet"),
                        &mut self.list_open,
                    )
                {
                    self.address = self.history[i].clone();
                    addr.request_focus();
                }
                // Enter presses the default button (OK), as in IDM, wherever the focus is.
                let _ = &addr;
                let enter = winui::enter_pressed(ui);

                // The login: a group under the address, its title the checkbox, as wide as the
                // address's row; 12 px round everything.
                let gap = winui::caption_gap(&p, o.x + 21.0, o.x + 124.0, LOG_IN, false);
                winui::group_box(&p, r(o, 12.0, 52.0, 454.0, 48.0), Some(gap));
                winui::checkbox(ui, o + vec2(21.0, 46.0), &mut self.auth, LOG_IN, true);
                let label = if self.auth {
                    winui::TEXT()
                } else {
                    winui::TEXT_DISABLED()
                };
                // Each caption ends before its box: a longer translation starts earlier (back
                // to the group's inside, or to the box before it), cut with "..." past that.
                winui::label_before(
                    ui,
                    &p,
                    o.x + 108.0,
                    o.y + 82.0,
                    // i18n: noun: the user name and password section
                    crate::i18n::t("Login"),
                    label,
                    o.x + 116.0,
                    o.x + 21.0,
                );
                winui::edit(
                    ui,
                    r(o, 116.0, 68.0, 120.0, 23.0),
                    &mut self.login,
                    self.auth,
                    false,
                );
                winui::label_before(
                    ui,
                    &p,
                    o.x + 334.0,
                    o.y + 82.0,
                    crate::i18n::t("Password"),
                    label,
                    o.x + 342.0,
                    o.x + 244.0,
                );
                winui::edit(
                    ui,
                    r(o, 342.0, 68.0, 115.0, 23.0),
                    &mut self.password,
                    self.auth,
                    true,
                );

                // OK and Cancel on the right, one size, OK on the address's row.
                let ok = winui::button(
                    ui,
                    r(o, 478.0, 12.0, 80.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                    None,
                )
                .clicked();
                let cancel = winui::button(
                    ui,
                    r(o, 478.0, 41.0, 80.0, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked();
                if let Some(e) = &self.error {
                    let red = winui::ERROR();
                    // After the group's title, before the group's end; whole on hover when cut.
                    // On the group box's top line, over a strip of the dialog's colour (as a
                    // caption sits on its line), not struck through by it.
                    let x = gap.1 + 8.0;
                    let w = winui::text_width(&p, e).min(o.x + 460.0 - x) + 6.0;
                    winui::fill_l(
                        &p,
                        x - 3.0,
                        o.y + 46.0,
                        x - 3.0 + w,
                        o.y + 59.0,
                        winui::BG(),
                    );
                    winui::fit(ui, &p, x, o.y + 56.0, e, red, o.x + 460.0, "add-url-error");
                }
                // Escape closes an open drop list first, then the dialog.
                if cancel || (!list_was_open && winui::escape_pressed(ui)) {
                    return Outcome::Closed;
                }
                if ok || (enter && !list_was_open) {
                    match crate::model::parse_address(&self.address) {
                        Ok(url) if mdm_engine::supported(&url) || url.scheme() == "magnet" => {
                            return Outcome::Done(NewUrl {
                                url,
                                login: self.typed_login(),
                            });
                        }
                        _ => self.error = Some(crate::i18n::t("Invalid address").into()),
                    }
                }
                Outcome::Open
            },
        )
    }
}
