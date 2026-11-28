//! Options → Sites Logins: the sites' user names and passwords, their list read from a file, and the
//! browser whose cookies video sites get.

use super::*;

impl OptionsDialog {
    pub(super) fn logins(&mut self, ui: &mut Ui, o: Pos2) {
        header(
            ui,
            o,
            Icon::PageLogins,
            // i18n: Options → Sites Logins: the page's title
            "Sites that need a login",
        );
        let rows: Vec<Vec<String>> = self
            .s
            .site_logins
            .iter()
            .map(|(site, user, pass)| {
                vec![site.clone(), user.clone(), "*".repeat(pass.chars().count())]
            })
            .collect();
        // The list leaves the page's foot to Cookies for video sites (#229).
        let (dbl, del_key) = report(
            ui,
            o,
            r(o, 21.0, 93.0, 383.0, 140.0),
            &[
                // i18n: column title: the site (and path) a login is for
                (crate::i18n::mark("Site/path"), 236.0),
                // i18n: column title: the user name
                (crate::i18n::mark("User"), 312.0),
                (crate::i18n::mark("Password"), 387.0),
            ],
            &rows,
            &mut self.login_rows,
            false,
            "opt-logins",
        );
        let sel = self.login_rows.selected.filter(|i| *i < rows.len());
        // As wide as their labels in this language (#192), under the list (it ends at 404).
        let [new_at, edit_at, remove_at] = winui::button_row_from_left(
            &ui.painter().clone(),
            &[
                ("New", 31.0, 73.0),
                ("Edit", 125.0, 73.0),
                ("Remove", 220.0, 73.0),
            ],
            21.0,
            404.0,
            140.0,
        )[..] else {
            unreachable!("three buttons")
        };
        let new = winui::button_light(
            ui,
            r(o, new_at.0, 241.0, new_at.1, 21.0),
            // i18n: button: create a new item
            "New",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        let kind = if sel.is_some() {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        let edit =
            // i18n: verb: edit the item
            winui::button_light(ui, r(o, edit_at.0, 241.0, edit_at.1, 21.0), "Edit", kind, false).clicked();
        // i18n: verb: remove the item
        let remove = winui::button_light(
            ui,
            r(o, remove_at.0, 241.0, remove_at.1, 21.0),
            "Remove",
            kind,
            false,
        )
        .clicked()
            || del_key;
        if new {
            self.sub = Some(Sub::Login(
                None,
                String::new(),
                String::new(),
                String::new(),
            ));
        }
        if let Some(i) = sel {
            if remove {
                self.s.site_logins.remove(i);
                self.login_rows.selected = None;
            } else if edit || dbl {
                let (site, user, pass) = self.s.site_logins[i].clone();
                self.sub = Some(Sub::Login(Some(i), site, user, pass));
            }
        }
        self.video_cookies(ui, o);
    }

    /// Sites Logins → Cookies for video sites (#229): Off, or the browser whose cookies yt-dlp
    /// reads on this computer (`--cookies-from-browser`) when the extension passed none, and its
    /// profile (a name, or a folder: a Chromium fork's).
    pub(super) fn video_cookies(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options → Sites Logins: group box title: yt-dlp signs in to video sites with a browser's cookies
        group(ui, o, 16.0, 270.0, 392.0, 130.0, "Cookies for video sites");
        let browsers = mdm_tools::proc::cookie_browsers();
        let at = self.cookie_browser.min(browsers.len() - 1);
        let on = !self.s.video_cookies.is_empty();
        // i18n: Options → Sites Logins → Cookies for video sites: the tooltip of its choices
        let tip = crate::i18n::t(
            "yt-dlp signs in to video sites with this browser's cookies, read from its files on this computer only. Used when the browser's extension didn't pass the page's own: a pasted address, a playlist.",
        );
        // i18n: Options → Sites Logins → Cookies for video sites: don't read any browser's cookies (the default)
        let off =
            winui::radio_light(ui, o + vec2(30.0, 288.0), !on, "Off", true).on_hover_text(tip);
        let from = winui::radio_light_until(
            ui,
            o + vec2(30.0, 314.0),
            on,
            // i18n: Options → Sites Logins → Cookies for video sites: read the cookies of the browser chosen in the list after it
            "From browser:",
            true,
            Some(194.0),
        )
        .on_hover_text(tip);
        let choose = |s: &mut Settings, i: usize| s.video_cookies = browsers[i].to_owned();
        match winui::radio_keys(ui, &[off.clone(), from.clone()]) {
            Some(0) => self.s.video_cookies.clear(),
            Some(_) => choose(&mut self.s, at),
            None if off.clicked() => self.s.video_cookies.clear(),
            None if from.clicked() => choose(&mut self.s, at),
            None => {}
        }
        let on = !self.s.video_cookies.is_empty();
        let names: Vec<&str> = browsers
            .iter()
            .map(|b| crate::videodlg::browser_label(b))
            .collect();
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 200.0, 311.0, 192.0, 21.0),
            &names,
            at,
            on,
            "opt-cookie-browser",
        ) {
            self.cookie_browser = i;
            choose(&mut self.s, i);
        }
        // The profile: a name (Firefox's, "Profile 1") or a folder; empty, the last one used.
        let ink = if on { LIGHT_TEXT() } else { LIGHT_DISABLED() };
        // i18n: Options → Sites Logins → Cookies for video sites: label before the browser profile's name or folder (may stay empty)
        let label = crate::i18n::t("Profile (optional):");
        let (rect, cut) =
            winui::text_right_within(&p, o.x + 194.0, o.y + 354.0, label, ink, o.x + 30.0);
        if cut {
            winui::full_on_hover(ui, rect, "cookie-profile", label);
        }
        winui::edit_light_on(
            ui,
            r(o, 200.0, 338.0, 160.0, 23.0),
            &mut self.s.video_cookies_profile,
            on,
            false,
            LIGHT_PAGE(),
        )
        .on_hover_text(crate::i18n::t(
            // i18n: Options → Sites Logins → Cookies for video sites: the tooltip of the profile box
            "A profile's name or its folder. Empty: the profile used last. For a browser built on Chromium that isn't listed (Helium, Thorium...), choose Chromium and its profile's folder.",
        ));
        // i18n: Options → Sites Logins → Cookies for video sites: the title of the window, and the tooltip of the button, that pick a browser profile's folder
        let pick_title = crate::i18n::t("Choose the profile's folder");
        let kind = if on {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        if winui::button_light(ui, r(o, 365.0, 339.0, 27.0, 21.0), "...", kind, false)
            .on_hover_text(pick_title)
            .clicked()
        {
            let title = pick_title.to_owned();
            winui::start_pick(ui.ctx(), "options-cookie-profile", move || {
                rfd::FileDialog::new().set_title(title).pick_folder()
            });
        }
        if let Some(dir) = winui::picked(ui.ctx(), "options-cookie-profile") {
            self.s.video_cookies_profile = dir.display().to_string();
        }
        // What to expect: Chrome-family browsers on Windows lock their cookies (Chrome 127 and
        // later's app-bound encryption); yt-dlp says so when it meets it, Options says so first.
        let chromium = on && !matches!(self.s.video_cookies.as_str(), "firefox" | "safari");
        let (note, ink) = if cfg!(windows) && chromium {
            (
                // i18n: Options → Sites Logins → Cookies for video sites, a Chrome-like browser chosen on Windows
                crate::i18n::t(
                    "On Windows, Chrome, Edge and Brave may lock their cookies. Firefox works.",
                ),
                winui::ERROR(),
            )
        } else {
            (
                // i18n: Options → Sites Logins → Cookies for video sites: a note under the choices
                crate::i18n::t("Read on this computer only, and sent only to the video's site."),
                LIGHT_DISABLED(),
            )
        };
        // Two lines at most: a longer language wraps rather than being cut.
        winui::paragraph(&p, o.x + 30.0, o.y + 380.0, note, ink, o.x + 400.0, 13.0);
    }
}

/// Sites Logins → Cookies for video sites' browser at first (#229): the one chosen, else the
/// first browser whose extension reached DDM when yt-dlp reads it (DDM doesn't know the
/// system's default browser), else Firefox, which yt-dlp reads on every system.
pub(super) fn cookie_browser_at(s: &Settings) -> usize {
    let browsers = mdm_tools::proc::cookie_browsers();
    let seen = mdm_ipc::extension::seen_browsers();
    let by_extension = seen.iter().find_map(|(name, _)| {
        let ytdlp = match name.as_str() {
            "Mozilla Firefox" => "firefox",
            "Google Chrome" => "chrome",
            "Microsoft Edge" => "edge",
            "Brave Browser" => "brave",
            "Opera" => "opera",
            "Apple Safari" => "safari",
            _ => return None,
        };
        browsers.iter().position(|b| *b == ytdlp)
    });
    browsers
        .iter()
        .position(|b| *b == s.video_cookies)
        .or(by_extension)
        .unwrap_or(0)
}

/// A list of sites from a file (Download panels' Import, #424): text, and no more than a list
/// could be.
pub(super) fn read_sites_file(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut text = Vec::new();
    std::fs::File::open(path)?
        .take(1 << 20)
        .read_to_end(&mut text)?;
    String::from_utf8(text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
