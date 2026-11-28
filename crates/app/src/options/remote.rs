//! Options → Remote (#141, ours): the remote control's state, address and port, the paired devices,
//! and Pair a device....

use super::*;

/// Options → Remote's page number (the tabs' numbers).
pub(super) const REMOTE_PAGE: usize = 11;

impl OptionsDialog {
    /// Remote's port as typed, or the one saved when it isn't a port DDM takes.
    pub(super) fn remote_port(&self) -> u16 {
        self.port
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p >= 1024)
            .unwrap_or(self.from.remote_port)
    }

    /// The remote control as this draft says (on, address, port).
    pub(super) fn remote_config(&self) -> mdm_remote::Config {
        let mut s = self.s.clone();
        s.remote_port = self.remote_port();
        crate::remote::config(&s)
    }

    /// Remote → Pair a device...: the remote control starts as the page says (Cancel puts it
    /// back as it was), a new code is shown.
    pub(super) fn pair(&mut self) {
        let config = self.remote_config();
        if let Some(r) = &self.remote {
            r.apply(config);
            r.start_pairing();
        }
        self.sub = Some(Sub::Pair);
    }

    /// Remote (#141, ours): phones and computers on the local network add and control
    /// downloads from a page DDM serves, once paired with a code shown here.
    pub(super) fn remote_page(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options page title: control DDM from a phone or another computer on the local network
        let title = crate::i18n::mark("Remote control");
        header(ui, o, Icon::PageRemote, title);
        let adapters = self.adapters_now();
        winui::checkbox_light(
            ui,
            o + vec2(21.0, 92.0),
            &mut self.s.remote_on,
            // i18n: Options → Remote check box (off by default)
            "Let phones and computers on the local network control DDM",
            true,
        );
        let on = self.s.remote_on;
        let color = if on { LIGHT_TEXT() } else { LIGHT_DISABLED() };
        text(
            &p,
            o.x + 21.0,
            o.y + 131.0,
            // i18n: Options → Remote: label before the choice of the address DDM listens on
            crate::i18n::t("Listen on:"),
            color,
        );
        let chosen = self.s.remote_listen.trim().to_owned();
        let mut values = vec![String::new(), mdm_remote::addrs::THIS_COMPUTER.to_owned()];
        let mut labels = vec![
            // i18n: Options → Remote → Listen on: the default: each of the computer's private local network addresses (never one the internet reaches)
            crate::i18n::t("Every local network address").to_owned(),
            // i18n: Options → Remote → Listen on: only programs on this computer can connect (127.0.0.1)
            crate::i18n::t("This computer only (127.0.0.1)").to_owned(),
        ];
        for a in &adapters {
            for ip in
                a.v4.iter()
                    .map(|v| v.to_string())
                    .chain(a.v6.iter().map(|v| v.to_string()))
            {
                labels.push(format!("{} \u{2014} {ip}", a.name));
                values.push(ip);
            }
        }
        if !values.contains(&chosen) {
            labels.push(crate::i18n::fmt(
                crate::i18n::t("{adapter} (not connected)"),
                &[("adapter", &chosen)],
            ));
            values.push(chosen.clone());
        }
        let at = values.iter().position(|v| *v == chosen).unwrap_or(0);
        let shown: Vec<&str> = labels.iter().map(String::as_str).collect();
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 130.0, 118.0, 250.0, 21.0),
            &shown,
            at,
            on,
            "opt-remote-at",
        ) {
            self.s.remote_listen = values[i].clone();
        }
        // i18n: Options → Remote: label before the port number box
        text(&p, o.x + 21.0, o.y + 159.0, crate::i18n::t("Port:"), color);
        winui::spin_box(
            ui,
            r(o, 130.0, 146.0, 80.0, 21.0),
            &mut self.port,
            1024,
            65535,
            on,
            "opt-remote-port",
        );
        // What it does now: as saved, or what OK changes.
        group(ui, o, 16.0, 178.0, 392.0, 96.0, "Status");
        let running = self.remote.as_ref().map(|r| r.status());
        let urls = self.remote.as_ref().map(|r| r.urls()).unwrap_or_default();
        let changed = self.remote_config() != crate::remote::config(&self.from);
        use mdm_remote::Status;
        let (line, ok) = match &running {
            _ if changed => (
                // i18n: Options → Remote → Status: the settings were changed; they apply on OK
                crate::i18n::t("The changes apply when you click OK.").to_owned(),
                true,
            ),
            None | Some(Status::Off) => (
                // i18n: Options → Remote → Status: the remote control is off
                crate::i18n::t("Off: nothing on the network can reach DDM.").to_owned(),
                true,
            ),
            Some(Status::Listening(_)) => (
                // i18n: Options → Remote → Status: on; the address follows in a box below
                crate::i18n::t("On. On the phone, open this address:").to_owned(),
                true,
            ),
            Some(Status::NoAddress) => (
                // i18n: Options → Remote → Status: on, but the computer has no local network address now
                crate::i18n::t("No local network now: it starts when the computer joins one.")
                    .to_owned(),
                false,
            ),
            Some(Status::PortInUse(port)) => (
                crate::i18n::fmt(
                    // i18n: Options → Remote → Status: another program uses the port; {port} = its number
                    crate::i18n::t("Port {port} is used by another program: choose another."),
                    &[("port", &port.to_string())],
                ),
                false,
            ),
            Some(Status::Failed(e)) => (
                crate::i18n::fmt(
                    // i18n: Options → Remote → Status: it could not start; {error} = the system's reason
                    crate::i18n::t("It could not start: {error}"),
                    &[("error", e)],
                ),
                false,
            ),
        };
        let ink = if ok {
            LIGHT_TEXT()
        } else {
            crate::theme::colors().error_strong
        };
        let (rect, cut) = winui::label_into(
            &p,
            o.x + 30.0,
            o.y + 207.0,
            &line,
            ink,
            o.x + 400.0,
            o.x + 30.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, "opt-remote-state", &line);
        }
        if !changed && let Some(first) = urls.first() {
            readonly_ltr(ui, r(o, 30.0, 215.0, 364.0, 21.0), first);
        }
        if let Some(fp) = self.remote.as_ref().and_then(|r| r.fingerprint()) {
            let label = crate::i18n::fmt(
                // i18n: Options → Remote → Status: the certificate's fingerprint, which the phone's browser shows when it warns about it; {fingerprint} = hex digits
                crate::i18n::t("Certificate (SHA-256): {fingerprint}"),
                &[("fingerprint", &crate::i18n::ltr(&fp))],
            );
            let (rect, cut) = winui::label_into(
                &p,
                o.x + 30.0,
                o.y + 260.0,
                &label,
                LIGHT_TEXT(),
                o.x + 400.0,
                o.x + 30.0,
            );
            if cut {
                winui::full_on_hover(ui, rect, "opt-remote-cert", &label);
            }
        }
        // The paired devices.
        text(
            &p,
            o.x + 21.0,
            o.y + 296.0,
            // i18n: Options → Remote: label above the list of paired phones and computers
            crate::i18n::t("Paired devices:"),
            LIGHT_TEXT(),
        );
        let devices = self
            .remote
            .as_ref()
            .map(|r| r.devices())
            .unwrap_or_default();
        let when = |secs: i64| {
            crate::clock::local_at(secs)
                .map(|t| crate::regional::short_date_time(t.naive_local()))
                .unwrap_or_default()
        };
        let rows: Vec<Vec<String>> = devices
            .iter()
            .map(|d| vec![d.name.clone(), when(d.added), when(d.seen)])
            .collect();
        let (_, del_key) = report(
            ui,
            o,
            r(o, 16.0, 302.0, 392.0, 88.0),
            &[
                // i18n: column title: a paired phone's or computer's name
                (crate::i18n::mark("Device"), 120.0),
                // i18n: column title: when the device was paired
                (crate::i18n::mark("Paired"), 256.0),
                // i18n: column title: when the device last reached DDM
                (crate::i18n::mark("Last seen"), 392.0),
            ],
            &rows,
            &mut self.device_rows,
            false,
            "opt-remote-devices",
        );
        let sel = self.device_rows.selected.filter(|i| *i < rows.len());
        let pair_kind = if on {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        let pair = winui::why_disabled(
            winui::button_light(
                ui,
                r(o, 16.0, 398.0, 130.0, 21.0),
                // i18n: Options → Remote button: shows a code (and a QR code) that pairs a phone
                "Pair a device...",
                pair_kind,
                false,
            ),
            !on,
            // i18n: why Options → Remote → Pair a device... is greyed
            "Turn on the remote control first.",
        )
        .clicked();
        let remove_kind = if sel.is_some() {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        let remove = winui::button_light(
            ui,
            r(o, 154.0, 398.0, 90.0, 21.0),
            "Remove",
            remove_kind,
            false,
        )
        .clicked()
            || del_key;
        if pair && on {
            self.pair();
        }
        // Unpaired at once (not on OK): a lost phone is refused from its next request on.
        if remove
            && let (Some(i), Some(r)) = (sel, &self.remote)
            && let Some(d) = devices.get(i)
        {
            r.remove(&d.id);
            self.device_rows.selected = None;
        }
        winui::paragraph(
            &p,
            o.x + 21.0,
            o.y + 438.0,
            // i18n: Options → Remote, at the bottom of the page
            crate::i18n::t(
                "Only paired devices see or change anything. The first time, the phone's browser warns about DDM's own certificate: its fingerprint is the one above.",
            ),
            LIGHT_TEXT(),
            o.x + 406.0,
            13.0,
        );
    }

    /// Remote → Pair a device...: the address (and a QR code of it), the code, and the device
    /// once it paired. False when it closes.
    pub(super) fn pair_window(&mut self, ctx: &egui::Context) -> bool {
        let Some(remote) = self.remote.clone() else {
            return false;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        let out = window(
            ctx,
            "opt-pair",
            // i18n: window title: Options → Remote → Pair a device...
            "Pair a device",
            [360.0, 420.0],
            false,
            LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let urls = remote.urls();
                let (shown, paired) = remote.pairing();
                let close_label = if paired.is_some() {
                    // i18n: button: close the window (the device is paired)
                    "Done"
                } else {
                    "Close"
                };
                let close = winui::button_light(
                    ui,
                    r(o, 274.0, 387.0, 73.0, 21.0),
                    close_label,
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let Some(url) = urls.first() else {
                    winui::paragraph(
                        &p,
                        o.x + 14.0,
                        o.y + 30.0,
                        // i18n: Options → Remote → Pair a device: the remote control isn't listening (no network, port taken): the Remote page says why
                        crate::i18n::t(
                            "DDM can't be reached on the network now: Options → Remote says why.",
                        ),
                        crate::theme::colors().error_strong,
                        o.x + 346.0,
                        14.0,
                    );
                    return if close || winui::escape_pressed(ui) {
                        Outcome::Closed
                    } else {
                        Outcome::Open
                    };
                };
                if let Some(name) = &paired {
                    let line = crate::i18n::fmt(
                        // i18n: Options → Remote → Pair a device: done; {device} = the name it was given
                        crate::i18n::t("{device} is paired. It can control DDM now."),
                        &[("device", &crate::i18n::isolate(name))],
                    );
                    winui::paragraph(
                        &p,
                        o.x + 14.0,
                        o.y + 30.0,
                        &line,
                        LIGHT_TEXT(),
                        o.x + 346.0,
                        14.0,
                    );
                    return if close || winui::escape_pressed(ui) || winui::enter_pressed(ui) {
                        Outcome::Done(())
                    } else {
                        Outcome::Open
                    };
                }
                winui::paragraph(
                    &p,
                    o.x + 14.0,
                    o.y + 24.0,
                    // i18n: Options → Remote → Pair a device: how to pair
                    crate::i18n::t(
                        "On the phone, scan this code with the camera, or open the address and enter the code.",
                    ),
                    LIGHT_TEXT(),
                    o.x + 346.0,
                    14.0,
                );
                let link = match &shown {
                    Some((code, _)) => format!("{url}#pair={code}"),
                    None => url.clone(),
                };
                crate::remote::paint_qr(&p, r(o, 90.0, 56.0, 180.0, 180.0), &link);
                readonly_ltr(ui, r(o, 14.0, 246.0, 332.0, 21.0), url);
                match &shown {
                    Some((code, left)) => {
                        text(
                            &p,
                            o.x + 14.0,
                            o.y + 300.0,
                            // i18n: Options → Remote → Pair a device: label before the 8-digit code
                            crate::i18n::t("Code:"),
                            LIGHT_TEXT(),
                        );
                        let big = egui::FontId::new(26.0, winui::tahoma_bold().family);
                        winui::text_font(
                            &p,
                            big,
                            o.x + 80.0,
                            o.y + 304.0,
                            &crate::i18n::ltr(&format!("{} {}", &code[..4], &code[4..])),
                            LIGHT_TEXT(),
                        );
                        let secs = left.as_secs();
                        let line = crate::i18n::fmt(
                            // i18n: Options → Remote → Pair a device: how long the code stays valid; {time} = minutes:seconds ("9:58")
                            crate::i18n::t("Waiting for the device. The code works for {time}."),
                            &[("time", &format!("{}:{:02}", secs / 60, secs % 60))],
                        );
                        winui::paragraph(
                            &p,
                            o.x + 14.0,
                            o.y + 336.0,
                            &line,
                            LIGHT_TEXT(),
                            o.x + 346.0,
                            14.0,
                        );
                    }
                    None => {
                        winui::paragraph(
                            &p,
                            o.x + 14.0,
                            o.y + 300.0,
                            // i18n: Options → Remote → Pair a device: the code ran out (10 minutes, or 5 wrong tries)
                            crate::i18n::t("This code can't be used any more."),
                            crate::theme::colors().error_strong,
                            o.x + 346.0,
                            14.0,
                        );
                        if winui::button_light(
                            ui,
                            r(o, 14.0, 318.0, 100.0, 21.0),
                            // i18n: Options → Remote → Pair a device: button: show a new code
                            "New code",
                            ButtonKind::Normal,
                            false,
                        )
                        .clicked()
                        {
                            remote.start_pairing();
                        }
                    }
                }
                if close || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else {
                    Outcome::Open
                }
            },
        );
        matches!(out, Outcome::Open)
    }
}
