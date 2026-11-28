//! Options → Connection: the connection type and its connections, the speed, and how names are
//! looked up (DNS over HTTPS, tested from its window).

use super::*;

/// Connection → DNS's sub-dialog: a copy of the DNS fields, kept on OK.
pub(super) struct DnsEdit {
    pub(super) mode: u8,
    pub(super) provider: String,
    pub(super) servers: String,
    pub(super) tls_name: String,
    pub(super) doh_url: String,
    pub(super) nextdns_id: String,
    pub(super) fallback: bool,
    /// The Test button's answer (shared with its thread): None while it runs.
    pub(super) test: Option<TestSlot>,
}

/// A test's answer line and whether it worked, filled in by its thread.
pub(super) type TestSlot = std::sync::Arc<std::sync::Mutex<Option<(String, bool)>>>;

impl DnsEdit {
    pub(super) fn from(s: &Settings) -> Self {
        DnsEdit {
            mode: if s.dns_mode == 0 { 3 } else { s.dns_mode },
            provider: s.dns_provider.clone(),
            servers: s.dns_servers.clone(),
            tls_name: s.dns_tls_name.clone(),
            doh_url: s.dns_doh_url.clone(),
            nextdns_id: s.dns_nextdns_id.clone(),
            fallback: s.dns_fallback,
            test: None,
        }
    }

    pub(super) fn choice(&self) -> Result<mdm_engine::DnsChoice, String> {
        settings::dns_choice(
            self.mode,
            &self.provider,
            &self.servers,
            &self.tls_name,
            &self.doh_url,
            &self.nextdns_id,
            self.fallback,
        )
    }

    pub(super) fn save(self, s: &mut Settings) {
        s.dns_mode = self.mode;
        s.dns_provider = self.provider;
        s.dns_servers = self.servers.trim().to_owned();
        s.dns_tls_name = self.tls_name.trim().to_owned();
        s.dns_doh_url = self.doh_url.trim().to_owned();
        s.dns_nextdns_id = self.nextdns_id.trim().to_lowercase();
        s.dns_fallback = self.fallback;
    }
}

impl OptionsDialog {
    pub(super) fn connection(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options → Connection: the page's title
        header(ui, o, Icon::PageConnection, "Connections and limits");
        // i18n: Options → Connection: group box: how many connections a download opens to its server
        group(ui, o, 16.0, 102.0, 392.0, 195.0, "Connections per download");
        // i18n: Options → Connection: label before a number box: the connections each download opens (unless an exception below says otherwise)
        let label = crate::i18n::t("Each download uses up to");
        let (rect, cut) = winui::label_into(
            &p,
            o.x + 130.0,
            o.y + 126.0,
            label,
            LIGHT_TEXT(),
            o.x + 268.0,
            o.x + 30.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, "max-conns", label);
        }
        // Typed, or stepped with the arrows (IDM's drop-down offered 1 to 32; issue #12).
        winui::spin_box(
            ui,
            r(o, 268.0, 112.0, 101.0, 21.0),
            &mut self.conns,
            1,
            u64::from(self.s.connection_limit()),
            true,
            "opt-conns",
        );
        let locked = mdm_engine::download::MAX_CONNECTIONS;
        let line = crate::i18n::fmt(
            // i18n: checkbox under the number of connections per download; {n} is 32
            crate::i18n::t("Allow more than {n} connections"),
            &[("n", &crate::i18n::integer(locked as u64))],
        );
        let was = self.s.many_connections;
        winui::checkbox_light(
            ui,
            o + vec2(33.0, 142.0),
            &mut self.s.many_connections,
            &line,
            true,
        );
        // Locked again: what went beyond comes back to the ceiling.
        if was && !self.s.many_connections {
            if self.conns.parse::<u32>().is_ok_and(|n| n > locked) {
                self.conns = locked.to_string();
            }
            for (_, n) in &mut self.s.conn_exceptions {
                *n = (*n).min(locked);
            }
        }
        winui::etched_h(&p, o.x + 30.0, o.y + 163.0, 359.0);
        // i18n: plural noun: label before a list of exceptions
        text(
            &p,
            o.x + 33.0,
            o.y + 180.0,
            crate::i18n::t("Exceptions:"),
            LIGHT_TEXT(),
        );
        // The user's exceptions, then the caps the engine learned (read-only "(auto)" rows).
        let rows: Vec<Vec<String>> = self
            .s
            .conn_exceptions
            .iter()
            .map(|(server, n)| vec![server.clone(), n.to_string()])
            .chain(
                self.learned
                    .iter()
                    .filter(|(h, _)| {
                        !self
                            .s
                            .conn_exceptions
                            .iter()
                            .any(|(s, _)| s.eq_ignore_ascii_case(h))
                    })
                    .map(|(h, n)| {
                        // i18n: a number of connections the app learned by itself for a server
                        let auto = crate::i18n::t("{n} (auto)");
                        vec![
                            h.clone(),
                            crate::i18n::fmt(auto, &[("n", &crate::i18n::integer(u64::from(*n)))]),
                        ]
                    }),
            )
            .collect();
        // New, Delete and Edit: one width, as wide as the longest label in this language (#192),
        // the list ending 8 px before them (its Number column ends at 291).
        let (col_x, col_w) = winui::buttons_to_right(
            &p,
            392.0,
            0.0,
            &[(winui::widest_label(&p, &["New", "Delete", "Edit"]), 73.0)],
            92.0,
        )[0];
        let (dbl, del_key) = report(
            ui,
            o,
            r(o, 30.0, 184.0, col_x - 8.0 - 30.0, 100.0),
            // i18n: column titles: the server, and the number of connections allowed to it
            &[
                (crate::i18n::mark("Server"), 241.0),
                (crate::i18n::mark("Number"), 291.0),
            ],
            &rows,
            &mut self.exc_rows,
            true,
            "opt-exceptions",
        );
        // Learned rows can't be edited or deleted (they fade by themselves).
        let sel = self
            .exc_rows
            .selected
            .filter(|i| *i < self.s.conn_exceptions.len());
        let new = winui::button_light(
            ui,
            r(o, col_x, 195.0, col_w, 21.0),
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
        // i18n: verb: delete the item
        let del = winui::button_light(ui, r(o, col_x, 224.0, col_w, 21.0), "Delete", kind, false)
            .clicked()
            || del_key;
        let edit =
            // i18n: verb: edit the item
            winui::button_light(ui, r(o, col_x, 254.0, col_w, 21.0), "Edit", kind, false)
                .clicked();
        if new {
            self.sub = Some(Sub::Exception(None, String::new(), "1".into()));
        }
        if let Some(i) = sel {
            if del {
                self.s.conn_exceptions.remove(i);
                self.exc_rows.selected = None;
            } else if edit || dbl {
                let (server, n) = self.s.conn_exceptions[i].clone();
                self.sub = Some(Sub::Exception(Some(i), server, n.to_string()));
            }
        }
        // Download limits (shared with the Scheduler's page).
        let gp = r(o, 21.0, 315.0, 380.0, 82.0);
        let gap = winui::caption_gap(&p, o.x + 37.0, o.x + 132.0, "Download limits", true);
        winui::group_box(&p, gp, Some(gap));
        winui::checkbox_light(
            ui,
            o + vec2(37.0, 310.0),
            &mut self.s.limits_on,
            "Download limits",
            true,
        );
        let on = self.s.limits_on;
        let line =
            |lead: f32, lead_baseline: f32, y: f32, gap: f32, baseline: f32| winui::SentenceLine {
                lead: winui::Lead::Right(o.x + lead),
                lead_baseline: o.y + lead_baseline,
                boxes: o + vec2(178.0, y),
                gap,
                baseline: o.y + baseline,
            };
        let boxes = winui::sentence(
            ui,
            &p,
            crate::i18n::t(crate::quota::LIMITS_SENTENCE),
            // Each line's words on its box's text baseline (the 20 px boxes at 330 and 351:
            // winui::baseline_in gives 343 and 364; they sat 2–3 px below the numbers).
            &[
                line(170.0, 343.0, 330.0, 8.0, 343.0),
                line(170.0, 364.0, 351.0, 11.0, 364.0),
            ],
            &[("mb", vec2(36.0, 20.0)), ("hours", vec2(36.0, 20.0))],
            |p, x, baseline, s| {
                winui::label(p, x, baseline, s, on);
            },
        );
        winui::number_field(ui, boxes[0], &mut self.mb, 1, 999_999, on, LIGHT_PAGE());
        winui::number_field(ui, boxes[1], &mut self.hours, 1, 999, on, LIGHT_PAGE());
        winui::checkbox_light(
            ui,
            o + vec2(37.0, 376.0),
            &mut self.s.limits_warn,
            "Warn me before downloads are stopped",
            on,
        );
        // DNS (#36): who answers the names; the details in Settings....
        // i18n: label before the choice of DNS (the service that turns names into addresses)
        text(
            &p,
            o.x + 21.0,
            o.y + 421.0,
            crate::i18n::t("DNS:"),
            LIGHT_TEXT(),
        );
        let names = dns_names();
        let labels: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 70.0, 406.0, 211.0, 21.0),
            &labels,
            dns_index(&self.s),
            true,
            "opt-dns",
        ) {
            set_dns_index(&mut self.s, i);
            if i > 0 {
                self.sub = Some(Sub::Dns(DnsEdit::from(&self.s)));
            }
        }
        let kind = if self.s.dns_mode == 0 {
            ButtonKind::Disabled
        } else {
            ButtonKind::Normal
        };
        // i18n: button: opens the DNS's details (how it's asked, its servers, a test)
        if winui::button_light(
            ui,
            r(o, 289.0, 406.0, 103.0, 21.0),
            "Settings...",
            kind,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Dns(DnsEdit::from(&self.s)));
        }
    }
}

/// Connection → DNS's list: the system's, the services by name, the user's own.
pub(super) fn dns_names() -> Vec<String> {
    let mut out = vec![
        // i18n: Options → Connection → DNS: the operating system's own DNS (the default)
        crate::i18n::t("The system's DNS").to_owned(),
    ];
    out.extend(mdm_engine::dns::PRESETS.iter().map(|p| p.name.to_owned()));
    // i18n: Options → Connection → DNS: a DNS server the user types in
    out.push(crate::i18n::t("Another DNS...").to_owned());
    out
}

pub(super) fn dns_index(s: &Settings) -> usize {
    if s.dns_mode == 0 {
        return 0;
    }
    let presets = mdm_engine::dns::PRESETS;
    presets
        .iter()
        .position(|p| p.id == s.dns_provider)
        .map_or(presets.len() + 1, |i| i + 1)
}

pub(super) fn set_dns_index(s: &mut Settings, i: usize) {
    let presets = mdm_engine::dns::PRESETS;
    if i == 0 {
        s.dns_mode = 0;
        return;
    }
    s.dns_provider = presets
        .get(i - 1)
        .map_or("custom".to_owned(), |p| p.id.to_owned());
    if s.dns_mode == 0 {
        s.dns_mode = 3;
    }
    // A service that isn't offered this way (NextDNS plain) moves to DNS over HTTPS.
    if let (Some(p), Some(m)) = (presets.get(i - 1), settings::dns_mode(s.dns_mode))
        && !p.offers(m)
    {
        s.dns_mode = 3;
    }
}

/// OK / Cancel at (x, y), OK only when `valid`.
/// Connection → DNS → Settings...: how it's asked, its servers, the fallback, a test.
pub(super) fn dns_dialog(ui: &mut Ui, o: Pos2, e: &mut DnsEdit) -> Outcome<()> {
    use mdm_engine::dns::{Mode, Preset};
    let p = ui.painter().clone();
    let preset = Preset::by_id(&e.provider);
    // i18n: Options → DNS settings: heading over the ways of asking the DNS
    text(
        &p,
        o.x + 14.0,
        o.y + 22.0,
        crate::i18n::t("Ask it with:"),
        LIGHT_TEXT(),
    );
    let modes = [
        (1u8, Mode::Plain, "plain DNS"),
        (2, Mode::Tls, "DNS over TLS"),
        (3, Mode::Https, "DNS over HTTPS"),
    ];
    let mut clicked = Vec::new();
    for (row, (n, m, caption)) in modes.iter().enumerate() {
        let offered = preset.is_none_or(|p| p.offers(*m));
        let r = winui::radio_light(
            ui,
            o + vec2(24.0, 32.0 + 20.0 * row as f32),
            e.mode == *n,
            caption,
            offered,
        );
        if r.clicked() && offered {
            e.mode = *n;
        }
        clicked.push(r);
    }
    if let Some(i) = winui::radio_keys(ui, &clicked) {
        let (n, m, _) = modes[i];
        if preset.is_none_or(|p| p.offers(m)) {
            e.mode = n;
        }
    }
    let field = |ui: &mut Ui, y: f32, label: &str, value: &mut String, enabled: bool| {
        winui::text_right(
            &p,
            o.x + 130.0,
            o.y + y + 14.0,
            crate::i18n::t(label),
            LIGHT_TEXT(),
        );
        winui::edit_light(ui, r(o, 138.0, y, 248.0, 21.0), value, enabled, false);
    };
    let mode = settings::dns_mode(e.mode).unwrap_or(Mode::Https);
    match preset {
        // A service: its servers as they are, NextDNS's configuration ID to type.
        Some(pr) => {
            let shown = match mode {
                Mode::Plain => pr.plain.join(", "),
                Mode::Tls => pr.tls_name.to_owned(),
                Mode::Https => pr.doh.to_owned(),
            };
            // i18n: Options → DNS settings: label before the chosen service's server (an address)
            winui::text_right(
                &p,
                o.x + 130.0,
                o.y + 112.0,
                crate::i18n::t("Server:"),
                LIGHT_TEXT(),
            );
            let (rect, cut) = winui::label_into(
                &p,
                o.x + 138.0,
                o.y + 112.0,
                &shown,
                LIGHT_DISABLED(),
                o.x + 386.0,
                o.x + 138.0,
            );
            if cut {
                winui::full_on_hover(ui, rect, "dns-server", &shown);
            }
            if pr.id == "nextdns" {
                // i18n: Options → DNS settings: NextDNS's own configuration ID (optional)
                field(
                    ui,
                    122.0,
                    "Configuration ID:",
                    &mut e.nextdns_id,
                    mode != Mode::Plain,
                );
            }
        }
        // The user's own DNS.
        None => match mode {
            // i18n: Options → DNS settings: label before the DNS servers' addresses, as IP addresses
            Mode::Plain => field(ui, 98.0, "Servers:", &mut e.servers, true),
            Mode::Tls => {
                field(ui, 98.0, "Servers:", &mut e.servers, true);
                // i18n: Options → DNS over TLS: label before the name the server's certificate shows
                field(ui, 124.0, "Certificate name:", &mut e.tls_name, true);
            }
            Mode::Https => {
                // i18n: Options → DNS over HTTPS: label before the server's https:// address
                field(ui, 98.0, "Address:", &mut e.doh_url, true);
                // i18n: Options → DNS over HTTPS: label before the server's IP addresses, which may be left empty
                field(ui, 124.0, "IP addresses (optional):", &mut e.servers, true);
            }
        },
    }
    winui::checkbox_light(
        ui,
        o + vec2(14.0, 158.0),
        &mut e.fallback,
        // i18n: Options → DNS settings: checkbox: when the chosen DNS can't be reached, use the system's instead
        "If it can't be reached, use the system's DNS",
        true,
    );
    winui::paragraph(
        &p,
        o.x + 14.0,
        o.y + 192.0,
        // i18n: Options → DNS settings: what the DNS choice covers and what it doesn't
        crate::i18n::t(
            "Names in your hosts file and local names (single words, .local, .lan) stay with the system's DNS. Video pages and torrent trackers use the system's DNS for now.",
        ),
        LIGHT_TEXT(),
        o.x + 386.0,
        14.0,
    );
    let choice = e.choice();
    // Test: asks github.com through the choice, without falling back, on a thread of its own.
    let running = e.test.as_ref().is_some_and(|t| t.lock().unwrap().is_none());
    let kind = if choice.is_ok() && !running {
        ButtonKind::Normal
    } else {
        ButtonKind::Disabled
    };
    // i18n: button: asks the chosen DNS for an address, to see it works
    if winui::button_light(ui, r(o, 14.0, 232.0, 73.0, 21.0), "Test", kind, false).clicked()
        && let Ok(c) = &choice
    {
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        e.test = Some(slot.clone());
        let (c, ctx) = (c.clone(), ui.ctx().clone());
        // Through the network adapter downloads use (#35).
        let route = crate::settings::own_route();
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|rt| {
                    rt.block_on(mdm_engine::dns::test(&c, "github.com", &[], &route))
                        .map_err(|e| e.to_string())
                });
            let line = match result {
                Ok((took, ips)) => (
                    crate::i18n::fmt(
                        // i18n: Options → DNS settings: the test's answer; {ms} milliseconds, {ip} the address found
                        crate::i18n::t("Answered in {ms} ms: {ip}"),
                        &[
                            ("ms", &crate::i18n::integer(took.as_millis() as u64)),
                            (
                                "ip",
                                &ips.first().map(|i| i.to_string()).unwrap_or_default(),
                            ),
                        ],
                    ),
                    true,
                ),
                Err(why) => (why, false),
            };
            *slot.lock().unwrap() = Some(line);
            ctx.request_repaint();
        });
    }
    let status = match (&choice, &e.test) {
        (Err(why), _) => Some((why.clone(), false)),
        (Ok(_), Some(t)) => Some(t.lock().unwrap().clone().unwrap_or_else(|| {
            // i18n: Options → DNS settings: the test is running
            (crate::i18n::t("Asking...").to_owned(), true)
        })),
        _ => None,
    };
    if let Some((line, ok)) = status {
        let color = if ok {
            LIGHT_TEXT()
        } else {
            crate::theme::colors().error_strong
        };
        let (rect, cut) = winui::label_into(
            &p,
            o.x + 95.0,
            o.y + 247.0,
            &line,
            color,
            o.x + 386.0,
            o.x + 95.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, "dns-test", &line);
        }
    }
    ok_cancel(ui, o, 238.0, 278.0, choice.is_ok())
}
