//! Options → Proxy / Socks: the system's proxy or DDM's own (its protocols, SOCKS), and the network
//! adapter every connection goes through.

use super::*;

/// Proxy / Socks → Advanced / Socks...: the manual proxy's kind (`PROXY_KINDS`), whether a SOCKS
/// proxy looks the names up itself, its address, port and login.
pub(super) struct SocksEdit {
    pub(super) kind: usize,
    pub(super) remote_dns: bool,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) user: String,
    pub(super) pass: String,
}

/// The manual proxy's kinds, as Advanced / Socks... lists them: (the address's scheme, its
/// scheme when the proxy looks the names up itself, the label).
pub(super) const PROXY_KINDS: [(&str, &str, &str); 4] = [
    // i18n: Options → Proxy / Socks → Advanced / Socks...: a kind of proxy (a web proxy, plain)
    ("http", "http", crate::i18n::mark("HTTP proxy")),
    // i18n: Options → Proxy / Socks → Advanced / Socks...: a kind of proxy (a web proxy reached over TLS)
    ("https", "https", crate::i18n::mark("HTTPS proxy")),
    // i18n: Options → Proxy / Socks → Advanced / Socks...: a kind of proxy (SOCKS version 4); a name: usually kept
    ("socks4", "socks4a", crate::i18n::mark("SOCKS 4")),
    // i18n: Options → Proxy / Socks → Advanced / Socks...: a kind of proxy (SOCKS version 5); a name: usually kept
    ("socks5", "socks5h", crate::i18n::mark("SOCKS 5")),
];

impl SocksEdit {
    /// The manual proxy as the page holds it: its address's scheme says its kind (none: HTTP).
    pub(super) fn from(s: &Settings) -> Self {
        let (kind, remote_dns, host) = proxy_kind(&s.proxy_host);
        SocksEdit {
            kind,
            remote_dns,
            host: host.to_owned(),
            port: s.proxy_port.clone(),
            user: s.proxy_user.clone(),
            pass: s.proxy_pass.clone(),
        }
    }

    /// The address as saved: an HTTP proxy's plain (as it always was), the others with their
    /// scheme (`socks5h://host` when the proxy looks the names up).
    pub(super) fn address(&self) -> String {
        let host = self.host.trim();
        let (scheme, remote) = (PROXY_KINDS[self.kind].0, PROXY_KINDS[self.kind].1);
        match self.kind {
            0 => host.to_owned(),
            _ if self.remote_dns => format!("{remote}://{host}"),
            _ => format!("{scheme}://{host}"),
        }
    }

    /// OK: the manual proxy, with this kind and address; http and https ticked if neither was.
    pub(super) fn save(self, s: &mut Settings) {
        s.proxy_host = self.address();
        s.proxy_port = self.port.trim().to_owned();
        s.proxy_user = self.user;
        s.proxy_pass = self.pass;
        s.proxy_mode = 3;
        manual_protocols(s);
    }
}

impl OptionsDialog {
    pub(super) fn proxy(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        Icon::PageProxy
            .image(true)
            .paint_at(ui, r(o, 21.0, 54.0, 32.0, 32.0));
        // The title after the icon, as every page's; Get System at the content's right edge, as
        // wide as its label in this language (#192); the title in one line before it, or two.
        let font = winui::tahoma_bold();
        let get_w = winui::button_fit_width(&p, "Get System", 93.0, 160.0);
        let get_x = 406.0 - get_w;
        // i18n: Options → Proxy / Socks: the page's title
        let (lines, _) = page_title_lines(&p, font.clone(), "Proxy server", get_x - 8.0 - 63.0);
        let first = 69.0 - 7.5 * (lines.len() as f32 - 1.0);
        for (i, line) in lines.iter().enumerate() {
            let y = (first + 15.0 * i as f32).round();
            winui::text_font(&p, font.clone(), o.x + 63.0, o.y + y, line, LIGHT_TEXT());
        }
        let s = &mut self.s;
        if winui::button_light(
            ui,
            r(o, get_x, 58.0, get_w, 21.0),
            "Get System",
            ButtonKind::Normal,
            false,
        )
        .clicked()
            && let Some((host, port)) = system_proxy()
        {
            s.proxy_mode = 3;
            s.proxy_host = host;
            s.proxy_port = port;
            s.proxy_http = true;
            s.proxy_https = true;
        }
        winui::checkbox_light(
            ui,
            o + vec2(30.0, 100.0),
            &mut s.proxy_from_browser,
            "",
            true,
        );
        // The checkbox's caption, in two lines as IDM has it (clicking it ticks the box).
        let lines = winui::paragraph(
            &p,
            o.x + 46.0,
            o.y + 104.0,
            crate::i18n::t(
                // i18n: Options → Proxy / Socks: a checkbox's caption, wrapped to the page: the proxy the browser used is used for the downloads it hands over, and when a download fails
                "Use the browser's proxy for the downloads it hands over, and when a download fails",
            ),
            LIGHT_TEXT(),
            o.x + 404.0,
            13.0,
        );
        winui::caption_click(
            ui,
            r(o, 46.0, 99.0, 358.0, 13.0 * lines as f32 + 2.0),
            "proxy-from-browser",
            &mut s.proxy_from_browser,
            true,
        );
        winui::etched_h(&p, o.x + 22.0, o.y + 128.0, 380.0);
        let modes = [
            (21.0, 137.0, crate::i18n::mark("No proxy")),
            (21.0, 167.0, crate::i18n::mark("Use system settings")),
            (
                21.0,
                194.0,
                // i18n: Options → Proxy / Socks: choice: the proxy comes from a configuration script (a PAC file) at an address
                crate::i18n::mark("Use a configuration script"),
            ),
            (
                22.0,
                250.0,
                // i18n: Options → Proxy / Socks: choice: the proxy's address, port and login typed below
                crate::i18n::mark("Use this proxy server"),
            ),
        ];
        let mut group = Vec::new();
        let mode_was = s.proxy_mode;
        for (i, (x, y, label)) in modes.iter().enumerate() {
            let resp = winui::radio_light(
                ui,
                o + vec2(*x, *y),
                s.proxy_mode as usize == i,
                label,
                true,
            );
            if resp.clicked() {
                s.proxy_mode = i as u8;
            }
            group.push(resp);
        }
        if let Some(i) = winui::radio_keys(ui, &group) {
            s.proxy_mode = i as u8;
        }
        if s.proxy_mode != mode_was {
            manual_protocols(s);
        }
        winui::etched_h(&p, o.x + 22.0, o.y + 159.0, 380.0);
        winui::etched_h(&p, o.x + 21.0, o.y + 187.0, 380.0);
        winui::etched_h(&p, o.x + 21.0, o.y + 242.0, 380.0);
        // The script's address (http, https or file): DDM fetches and runs it (pacfetch.rs,
        // mdm_engine::pac).
        let script = s.proxy_mode == 2;
        // i18n: noun: the web address (URL)
        disabled_label(&p, o.x + 30.0, o.y + 228.0, "Address", script, false);
        winui::edit_light_on(
            ui,
            r(o, 90.0, 213.0, 305.0, 23.0),
            &mut s.proxy_script,
            script,
            false,
            LIGHT_PAGE(),
        );
        let manual = s.proxy_mode == 3;
        disabled_label(
            &p,
            o.x + 30.0,
            o.y + 280.0,
            "Proxy server address",
            manual,
            false,
        );
        disabled_label(&p, o.x + 172.0, o.y + 278.0, "Port", manual, false);
        disabled_label(&p, o.x + 229.0, o.y + 278.0, "User name", manual, false);
        disabled_label(&p, o.x + 313.0, o.y + 278.0, "Password", manual, false);
        winui::edit_light_on(
            ui,
            r(o, 30.0, 286.0, 137.0, 24.0),
            &mut s.proxy_host,
            manual,
            false,
            LIGHT_PAGE(),
        );
        winui::number_field(
            ui,
            r(o, 172.0, 286.0, 51.0, 24.0),
            &mut s.proxy_port,
            1,
            65535,
            manual,
            LIGHT_PAGE(),
        );
        winui::edit_light_on(
            ui,
            r(o, 229.0, 286.0, 78.0, 24.0),
            &mut s.proxy_user,
            manual,
            false,
            LIGHT_PAGE(),
        );
        password(
            ui,
            r(o, 312.0, 286.0, 83.0, 24.0),
            &mut s.proxy_pass,
            manual,
        );
        disabled_label(
            &p,
            o.x + 30.0,
            o.y + 326.0,
            // i18n: Options → Proxy / Socks: over the http and https checkboxes: which addresses go through the proxy
            "Use it for:",
            manual,
            false,
        );
        winui::checkbox_light(ui, o + vec2(30.0, 334.0), &mut s.proxy_http, "http", manual);
        winui::checkbox_light(
            ui,
            o + vec2(97.0, 334.0),
            &mut s.proxy_https,
            "https",
            manual,
        );
        // No ftp box: FTP connects to its servers directly, never through this proxy (IDM's
        // greyed box had nothing to do).
        // The proxy's kind (HTTP, HTTPS, SOCKS 4 or 5, names looked up by the proxy) with its
        // address and login, in a window of its own; the address keeps the kind as its scheme.
        let kind = PROXY_KINDS[proxy_kind(&s.proxy_host).0].2;
        let socks = winui::button_light(
            ui,
            r(o, 235.0, 339.0, 159.0, 21.0),
            "Advanced / Socks...",
            if manual {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        )
        // i18n: tooltip of Options → Proxy / Socks → Advanced / Socks...: {kind} is the manual proxy's kind now ("HTTP proxy", "SOCKS 5")
        .on_hover_text(crate::i18n::fmt(
            crate::i18n::t("The proxy's kind, now {kind}: HTTP, HTTPS or SOCKS"),
            &[("kind", crate::i18n::t(kind))],
        ));
        if socks.clicked() {
            self.sub = Some(Sub::Socks(SocksEdit::from(s)));
        }
        winui::etched_h(&p, o.x + 21.0, o.y + 447.0, 381.0);
        winui::checkbox_light(
            ui,
            o + vec2(27.0, 453.0),
            &mut s.ftp_passive,
            // i18n: Options → Proxy / Socks: checkbox; FTP downloads use passive mode (the computer opens the data connection)
            "Use passive mode for FTP",
            true,
        );
        self.adapter_row(ui, o);
    }

    /// Proxy / Socks → Network adapter (#35): the adapter every connection goes through (a VPN's),
    /// and what it is doing now. The list is the computer's adapters, read every 2 s; the one
    /// chosen stays in it while it's gone, "(not connected)".
    pub(super) fn adapter_row(&mut self, ui: &mut Ui, o: Pos2) {
        use mdm_engine::Route;
        let stale = self
            .adapters
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= std::time::Duration::from_secs(2));
        if stale {
            self.adapters = Some((std::time::Instant::now(), crate::netwatch::adapters()));
        }
        let adapters = self
            .adapters
            .as_ref()
            .map(|(_, a)| a.clone())
            .unwrap_or_default();
        let p = ui.painter().clone();
        winui::etched_h(&p, o.x + 21.0, o.y + 372.0, 381.0);
        // i18n: label before the choice of network adapter (Ethernet, Wi-Fi, a VPN's) every connection must go through
        text(
            &p,
            o.x + 27.0,
            o.y + 395.0,
            crate::i18n::t("Network adapter:"),
            LIGHT_TEXT(),
        );
        // The choices: any, then each adapter with its address (the ones with none too: a VPN
        // switched off), the chosen one kept when it's gone.
        let chosen = self.s.net_adapter.trim().to_owned();
        let mut values: Vec<String> = vec![String::new()];
        let mut labels: Vec<String> =
            // i18n: Options → Network adapter: the first choice: no adapter is chosen, the system picks one for each connection
            vec![crate::i18n::t("Any (the system chooses)").to_owned()];
        for a in &adapters {
            let ip =
                a.v4.first()
                    .map(|v| v.to_string())
                    .or(a.v6.first().map(|v| v.to_string()));
            values.push(a.name.clone());
            labels.push(match ip {
                Some(ip) => format!("{} \u{2014} {ip}", a.name),
                None => crate::i18n::fmt(
                    // i18n: Options → Network adapter: an adapter with no address now (a VPN switched off, a cable out); {adapter} its name
                    crate::i18n::t("{adapter} (not connected)"),
                    &[("adapter", &a.name)],
                ),
            });
        }
        if !chosen.is_empty() && !values.iter().any(|v| v.eq_ignore_ascii_case(&chosen)) {
            values.push(chosen.clone());
            labels.push(crate::i18n::fmt(
                crate::i18n::t("{adapter} (not connected)"),
                &[("adapter", &chosen)],
            ));
        }
        let at = values
            .iter()
            .position(|v| v.eq_ignore_ascii_case(&chosen))
            .unwrap_or(0);
        let shown: Vec<&str> = labels.iter().map(String::as_str).collect();
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 130.0, 380.0, 265.0, 21.0),
            &shown,
            at,
            true,
            "opt-adapter",
        ) {
            self.s.net_adapter = values[i].clone();
        }
        // What it does now.
        let (line, ok) = match mdm_engine::net::route_for(&self.s.net_adapter, &adapters) {
            Route::Any => (
                // i18n: Options → Network adapter: what "Any" means
                crate::i18n::t("Each connection takes the system's route.").to_owned(),
                true,
            ),
            Route::Down { label } => (
                crate::i18n::fmt(
                    // i18n: Options → Network adapter: the chosen adapter has no address now; nothing is sent until it's back; {adapter} its name
                    crate::i18n::t(
                        "{adapter} is not connected: nothing is downloaded until it is back.",
                    ),
                    &[("adapter", &label)],
                ),
                false,
            ),
            Route::Bound(b) => {
                let ip = b.v4.map(|v| v.to_string()).or(b.v6.map(|v| v.to_string()));
                let line = crate::i18n::fmt(
                    // i18n: Options → Network adapter: every connection goes out through the chosen adapter, from this address (none goes another way); {ip} its address
                    crate::i18n::t("Every connection goes through it, from {ip}."),
                    &[("ip", &crate::i18n::ltr(&ip.unwrap_or_default()))],
                );
                // Downloads, torrents (#143), video pages and galleries (through the helpers'
                // relay, #35) all go through it on every system: nothing more to say.
                (line, true)
            }
        };
        let color = if ok {
            LIGHT_TEXT()
        } else {
            crate::theme::colors().error_strong
        };
        // Up to two lines, each cut with "..." (whole on hover) when a translation is longer.
        for (row, part) in line.lines().take(2).enumerate() {
            let (rect, cut) = winui::label_into(
                &p,
                o.x + 27.0,
                o.y + 422.0 + 15.0 * row as f32,
                part,
                color,
                o.x + 400.0,
                o.x + 27.0,
            );
            if cut {
                winui::full_on_hover(ui, rect, ("opt-adapter-state", row), part);
            }
        }
    }

    // ---- entry dialogs --------------------------------------------------------------------

    /// The computer's adapters, read again every 2 s (Proxy / Socks' adapter, Remote's address).
    pub(super) fn adapters_now(&mut self) -> Vec<mdm_engine::net::Adapter> {
        let stale = self
            .adapters
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= std::time::Duration::from_secs(2));
        if stale {
            self.adapters = Some((std::time::Instant::now(), crate::netwatch::adapters()));
        }
        self.adapters
            .as_ref()
            .map(|(_, a)| a.clone())
            .unwrap_or_default()
    }
}

/// A manual proxy's address read as (its kind in `PROXY_KINDS`, the proxy looks names up, the
/// address without its scheme). An address without a scheme, or one DDM doesn't know, is HTTP's.
pub(super) fn proxy_kind(address: &str) -> (usize, bool, &str) {
    let address = address.trim();
    let Some((scheme, rest)) = address.split_once("://") else {
        return (0, false, address);
    };
    let scheme = scheme.to_ascii_lowercase();
    PROXY_KINDS
        .iter()
        .enumerate()
        .find_map(|(i, (plain, remote, _))| {
            if scheme == *plain {
                Some((i, false, rest))
            } else if scheme == *remote {
                Some((i, true, rest))
            } else {
                None
            }
        })
        .unwrap_or((0, false, address))
}

// ---- pieces -----------------------------------------------------------------------------

/// Proxy / Socks → Manual proxy/socks configuration chosen with neither http nor https ticked:
/// both ticked, as Get System ticks them. Unticked, the proxy typed carried nothing and every
/// download went around it, straight to the server, without a word. Either can be unticked again.
pub(super) fn manual_protocols(s: &mut Settings) {
    if s.proxy_mode == 3 && !s.proxy_http && !s.proxy_https {
        s.proxy_http = true;
        s.proxy_https = true;
    }
}

/// Proxy / Socks → Get System: Windows' own proxy (host, port), if one is set.
pub(super) fn system_proxy() -> Option<(String, String)> {
    #[cfg(windows)]
    return crate::win32::system_proxy();
    #[cfg(not(windows))]
    None
}

/// Proxy / Socks → Advanced / Socks...: the proxy's kind, whether a SOCKS proxy looks the names
/// up itself (DNS through the proxy), its address, port and login. OK makes it the manual proxy.
pub(super) fn socks_dialog(ui: &mut Ui, o: Pos2, e: &mut SocksEdit) -> Outcome<()> {
    let p = ui.painter().clone();
    text(
        &p,
        o.x + 14.0,
        o.y + 22.0,
        crate::i18n::t("Type:"),
        LIGHT_TEXT(),
    );
    let mut group = Vec::new();
    for (i, (_, _, label)) in PROXY_KINDS.iter().enumerate() {
        let resp = winui::radio_light(
            ui,
            o + vec2(30.0, 30.0 + 20.0 * i as f32),
            e.kind == i,
            label,
            true,
        );
        if resp.clicked() {
            e.kind = i;
        }
        group.push(resp);
    }
    if let Some(i) = winui::radio_keys(ui, &group) {
        e.kind = i;
    }
    let socks = e.kind >= 2;
    winui::checkbox_light(
        ui,
        o + vec2(30.0, 114.0),
        &mut e.remote_dns,
        // i18n: Options → Advanced / Socks: a SOCKS proxy looks the servers' names up itself (DNS through the proxy: SOCKS 4a, SOCKS 5 with remote names)
        "Let the proxy look up the servers' names (DNS through the proxy)",
        socks,
    );
    let label = |y: f32, s: &str| {
        winui::text_right(&p, o.x + 90.0, o.y + y, crate::i18n::t(s), LIGHT_TEXT());
    };
    label(158.0, "Server:");
    winui::edit_light(ui, r(o, 98.0, 144.0, 210.0, 23.0), &mut e.host, true, false);
    winui::text_right(
        &p,
        o.x + 358.0,
        o.y + 158.0,
        crate::i18n::t("Port:"),
        LIGHT_TEXT(),
    );
    winui::number_field(
        ui,
        r(o, 362.0, 144.0, 46.0, 23.0),
        &mut e.port,
        1,
        65535,
        true,
        LIGHT_PAGE(),
    );
    label(189.0, "User:");
    winui::edit_light(ui, r(o, 98.0, 175.0, 310.0, 23.0), &mut e.user, true, false);
    label(220.0, "Password:");
    password(ui, r(o, 98.0, 206.0, 310.0, 23.0), &mut e.pass, true);
    let port_ok = e.port.trim().is_empty() || e.port.trim().parse::<u16>().is_ok_and(|p| p > 0);
    // The name typed without a scheme (the kind gives it).
    let host_ok = !e.host.trim().is_empty() && !e.host.contains("://");
    ok_cancel(ui, o, 254.0, 236.0, host_ok && port_ok)
}
