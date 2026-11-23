//! File Properties (double-click / right-click → Properties;
//! notes/ui-spec/scheduler-and-misc.md §5). Light in both modes, as IDM's.

use std::path::{Path, PathBuf};

use egui::{Pos2, Ui, vec2};

use crate::chrome::{IconKey, ShellIcons};
use crate::dialogs::{Outcome, window};
use crate::model::{Item, Status};
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, r, text};

/// What OK changes.
pub struct Changes {
    pub id: u64,
    pub description: String,
    pub referer: Option<String>,
    pub login: Option<(String, String)>,
    /// A new address for a download that isn't finished (Refresh download address).
    pub address: Option<url::Url>,
    /// Move the file (finished) or its destination (not started) into this folder.
    pub move_to: Option<PathBuf>,
    /// "Through …" changed (#44): downloaded directly (true) or through the account again.
    pub direct: Option<bool>,
}

pub struct Properties {
    pub id: u64,
    address: String,
    description: String,
    referer: String,
    login: String,
    password: String,
    move_to: Option<PathBuf>,
    error: Option<String>,
    /// The debrid accounts (#44): "Through …" shows while one takes the address.
    route: crate::debrid::Route,
    direct: bool,
    files_requested: bool,
    /// The Server row is shown: there is something to say about where the download comes from
    /// ([`server_line`]). Once shown it stays while the window is open (a download that starts
    /// meanwhile makes it taller once, never shorter again).
    server_row: bool,
}

/// What File Properties' Server row says (beyond IDM, which has no such row, #140): where a
/// download comes from, when there is something to say.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerLine {
    /// The server its last run connected to (its country and round trip).
    Seen(crate::whereis::Seen),
    /// A video's or a gallery's site, looked up by a helper program (yt-dlp, gallery-dl).
    Helper { site: String, tool: &'static str },
    /// A torrent: the people sharing it, not one server.
    Peers,
}

/// The Server row for `item`, or none where it would mean nothing: a file download that hasn't
/// run yet (its server is known once it has), a torrent sent to a debrid account before it
/// has fetched anything.
pub fn server_line(item: &Item) -> Option<ServerLine> {
    let seen = item
        .snap
        .as_ref()
        .and_then(|s| s.server.as_ref())
        .map(crate::whereis::Seen::from)
        .or_else(|| item.server.clone());
    let site = || {
        item.url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_owned()
    };
    if item.torrent.is_some() {
        Some(ServerLine::Peers)
    } else if item.video.is_some() {
        Some(ServerLine::Helper {
            site: site(),
            tool: "yt-dlp",
        })
    } else if item.gallery.is_some() {
        Some(ServerLine::Helper {
            site: site(),
            tool: "gallery-dl",
        })
    } else {
        seen.map(ServerLine::Seen)
    }
}

/// How much higher everything under the Server row is drawn when there is no such row.
const NO_SERVER_ROW: f32 = 20.0;

impl Properties {
    pub fn new(item: &Item, route: crate::debrid::Route) -> Self {
        let (login, password) = item.login.clone().unwrap_or_default();
        Properties {
            id: item.id,
            address: item.url.to_string(),
            description: item.description.clone(),
            referer: item.context.referer.clone().unwrap_or_default(),
            login,
            password,
            move_to: None,
            error: None,
            route,
            direct: item.direct,
            files_requested: false,
            server_row: server_line(item).is_some(),
        }
    }

    pub fn take_files_requested(&mut self) -> bool {
        std::mem::take(&mut self.files_requested)
    }

    /// `item` is read every frame, so status and size stay live.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        icons: &mut ShellIcons,
        item: &Item,
    ) -> Outcome<Changes> {
        self.server_row |= server_line(item).is_some();
        let height = if self.server_row {
            420.0
        } else {
            420.0 - NO_SERVER_ROW
        };
        let outcome = window(
            ctx,
            &format!("properties-{}", self.id),
            "File Properties",
            [477.0, height],
            false,
            LIGHT_BG(),
            |ui, o| self.body(ui, o, icons, item),
        );
        match outcome {
            Outcome::Done(()) => {
                let address = url::Url::parse(self.address.trim())
                    .ok()
                    .filter(|u| *u != item.url);
                let referer = self.referer.trim();
                Outcome::Done(Changes {
                    id: self.id,
                    description: self.description.clone(),
                    referer: (!referer.is_empty()).then(|| referer.to_owned()),
                    login: (!self.login.is_empty())
                        .then(|| (self.login.clone(), self.password.clone())),
                    address,
                    move_to: self.move_to.take(),
                    direct: (self.direct != item.direct).then_some(self.direct),
                })
            }
            Outcome::Closed => Outcome::Closed,
            Outcome::Open => Outcome::Open,
        }
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2, icons: &mut ShellIcons, item: &Item) -> Outcome<()> {
        let p = ui.painter().clone();
        let complete = item.status == Status::Complete;
        let running = item.live.is_some();
        // The address can be replaced while the download isn't finished or running.
        let editable = !complete && !running;
        let ext = Path::new(&item.file_name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        icons.paint(ui, IconKey::ExtLarge(ext.clone()), o + vec2(11.0, 11.0));
        let fit = |ui: &mut Ui, x: f32, base: f32, s: &str| {
            let (rect, cut) =
                winui::text_fit(&p, o.x + x, o.y + base, s, LIGHT_TEXT(), o.x + 466.0);
            winui::copyable(ui, rect, ("props", base as i32), s, cut);
        };
        fit(ui, 87.0, 31.0, &item.file_name);
        winui::etched_h(&p, o.x + 11.0, o.y + 50.0, 456.0);
        let size = item.size.map_or_else(String::new, |s| {
            crate::i18n::fmt(
                crate::i18n::t("{size} ({bytes} Bytes)"),
                &[
                    ("size", &crate::dialogs::dialog_size(s)),
                    ("bytes", &crate::i18n::integer(s)),
                ],
            )
        });
        // The values' column: IDM's 87, or after the widest label when a translation is longer
        // (the boxes keep their right edges).
        let col = [
            "Type:",
            "Status:",
            "Size:",
            "Save To:",
            "Address:",
            "Description:",
            "Referer:",
            "Server:",
            "User name:",
            "Password:",
        ]
        .iter()
        .map(|l| 11.0 + winui::text_width(&p, crate::i18n::t(l)).ceil() + 6.0)
        .fold(87.0_f32, f32::max);
        for (label, base, value) in [
            // i18n: label before the kind of file ("ZIP File")
            (
                crate::i18n::mark("Type:"),
                69.0,
                file_type(&item.file_name, &ext),
            ),
            // i18n: label before the download's state
            (crate::i18n::mark("Status:"), 88.0, item.status_text()),
            (crate::i18n::mark("Size:"), 108.0, size),
        ] {
            text(
                &p,
                o.x + 11.0,
                o.y + base,
                crate::i18n::t(label),
                LIGHT_TEXT(),
            );
            fit(ui, col, base, &value);
        }
        // Where it comes from (#140, beyond IDM): the server its last run saw (or the running
        // one sees), a video's or a gallery's site and the program that looks it up, a
        // torrent's peers. No row where there is nothing to say yet (a file not run yet).
        if self.server_row {
            // i18n: label before where a download comes from: its server (country and round trip), a video's site, a torrent's peers
            text(
                &p,
                o.x + 11.0,
                o.y + 128.0,
                crate::i18n::t("Server:"),
                LIGHT_TEXT(),
            );
            match server_line(item) {
                Some(ServerLine::Seen(seen)) => crate::whereis::line(
                    ui,
                    o.x + col,
                    o.y + 128.0,
                    o.x + 466.0,
                    Some(&seen),
                    &format!("props-{}", self.id),
                ),
                Some(ServerLine::Helper { site, tool }) => fit(
                    ui,
                    col,
                    128.0,
                    &crate::i18n::fmt(
                        // i18n: File Properties → Server, for a video or a gallery: {site} the site's name ("youtube.com"), {tool} the program that looks the page up ("yt-dlp", "gallery-dl"; names, not translated)
                        crate::i18n::t("{site}, through {tool}"),
                        &[("site", &crate::i18n::ltr(&site)), ("tool", tool)],
                    ),
                ),
                // i18n: File Properties → Server, for a torrent: it comes from the people sharing it, not from one server
                Some(ServerLine::Peers) => fit(ui, col, 128.0, crate::i18n::t("BitTorrent peers")),
                // Known before and not now (a run under way that hasn't connected yet).
                None => {}
            }
        }
        // Under the Server row; as much higher without it.
        let dy = if self.server_row { 0.0 } else { -NO_SERVER_ROW };

        text(
            &p,
            o.x + 11.0,
            o.y + 155.0 + dy,
            crate::i18n::t("Save To:"),
            LIGHT_TEXT(),
        );
        let dest = match &self.move_to {
            Some(dir) => dir.join(&item.file_name),
            None => item.dest.clone(),
        };
        readonly(
            ui,
            r(o, col, 140.0 + dy, 383.0 - col, 23.0),
            &dest.display().to_string(),
        );
        // A finished file is moved (Move); one that hasn't started gets a new folder (Browse...),
        // as IDM's File Properties has it.
        let movable = !running && (complete || item.downloaded == 0);
        let pick = winui::button_light(
            ui,
            r(o, 393.0, 141.0 + dy, 73.0, 21.0),
            if complete {
                // i18n: verb: move the file to another folder
                "Move"
            } else {
                // i18n: File Properties of a download not finished: choose another folder for it
                crate::i18n::mark("Browse...")
            },
            if movable {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        );
        let pick = winui::why_disabled(
            pick,
            !movable,
            if running {
                // i18n: tooltip of the greyed Move button in File Properties
                crate::i18n::mark("Pause the download first")
            } else {
                // i18n: tooltip of the greyed Move button in File Properties
                crate::i18n::mark("Only a finished download, or one not started yet, can be moved")
            },
        )
        .clicked();
        if pick && movable {
            let title = crate::i18n::t("Move to folder").to_owned();
            let from = item.dest.parent().unwrap_or(Path::new(".")).to_path_buf();
            winui::start_pick(ui.ctx(), &format!("move-to-{}", self.id), move || {
                rfd::FileDialog::new()
                    .set_title(title)
                    .set_directory(from)
                    .pick_folder()
            });
        }
        if let Some(dir) = winui::picked(ui.ctx(), &format!("move-to-{}", self.id)) {
            self.move_to = Some(dir);
        }

        text(
            &p,
            o.x + 11.0,
            o.y + 186.0 + dy,
            crate::i18n::t("Address:"),
            LIGHT_TEXT(),
        );
        if editable {
            let edit = winui::edit_light(
                ui,
                r(o, col, 171.0 + dy, 467.0 - col, 23.0),
                &mut self.address,
                true,
                false,
            );
            if edit.changed() {
                self.error = None;
            }
        } else {
            readonly(ui, r(o, col, 171.0 + dy, 467.0 - col, 23.0), &self.address);
        }
        text(
            &p,
            o.x + 11.0,
            o.y + 217.0 + dy,
            crate::i18n::t("Description:"),
            LIGHT_TEXT(),
        );
        winui::edit_light(
            ui,
            r(o, col, 202.0 + dy, 467.0 - col, 23.0),
            &mut self.description,
            true,
            false,
        );
        winui::etched_h(&p, o.x + 11.0, o.y + 233.0 + dy, 456.0);
        text(
            &p,
            o.x + 11.0,
            o.y + 251.0 + dy,
            // i18n: File Properties: label over the address of the web page the download came from
            crate::i18n::t("Web page it came from:"),
            LIGHT_TEXT(),
        );
        // The page it came from (the browser's, a site grabber project's, a video's own); a link
        // added by hand has none.
        let page = item.web_page();
        // i18n: the web page is not known
        let page = page.as_deref().unwrap_or(crate::i18n::t("Unknown"));
        fit(ui, 11.0, 272.0 + dy, page);
        winui::etched_h(&p, o.x + 11.0, o.y + 282.0 + dy, 456.0);
        // i18n: HTTP term (sic): the page the download was started from
        text(
            &p,
            o.x + 11.0,
            o.y + 308.0 + dy,
            crate::i18n::t("Referer:"),
            LIGHT_TEXT(),
        );
        winui::edit_light(
            ui,
            r(o, col, 291.0 + dy, 467.0 - col, 23.0),
            &mut self.referer,
            editable,
            false,
        );
        // The login's labels in the labels' column, its boxes in the values' (as every row
        // above), ending where they did.
        // i18n: label before the user name box
        text(
            &p,
            o.x + 11.0,
            o.y + 336.0 + dy,
            crate::i18n::t("User name:"),
            LIGHT_TEXT(),
        );
        winui::edit_light(
            ui,
            r(o, col, 321.0 + dy, 309.0 - col, 21.0),
            &mut self.login,
            editable,
            false,
        );
        text(
            &p,
            o.x + 11.0,
            o.y + 363.0 + dy,
            crate::i18n::t("Password:"),
            LIGHT_TEXT(),
        );
        password(
            ui,
            r(o, col, 350.0 + dy, 309.0 - col, 21.0),
            &mut self.password,
            editable,
        );
        // Through a debrid account (#44), beside the login: unticked, downloaded directly. It
        // changes while the download isn't running or finished.
        if let Some(service) = self.route.service(&item.url) {
            let mut on = !self.direct;
            let caption = crate::i18n::fmt(
                // i18n: File Properties: a check box beside the login, ticked when the download goes through the user's debrid account; {service} the service, "Real-Debrid". Unticked: downloaded directly
                crate::i18n::t("Through {service}"),
                &[("service", service)],
            );
            winui::checkbox_light_until(
                ui,
                o + vec2(322.0, 329.0 + dy),
                &mut on,
                &caption,
                editable,
                Some(o.x + 467.0),
            );
            self.direct = !on;
        }
        winui::etched_h(&p, o.x + 11.0, o.y + 378.0 + dy, 456.0);
        if !complete {
            let last = item.last_try.clone().unwrap_or_default();
            let last_try = crate::i18n::fmt(
                // i18n: {last} = the date and time of the last attempt to download
                crate::i18n::t("Last try  {last}"),
                &[("last", &last)],
            );
            fit(ui, 11.0, 396.0 + dy, &last_try);
        }
        // The dialog's own error (a bad address) takes the Result line's place while it lasts.
        if let Some(e) = &self.error {
            winui::fit(
                ui,
                &p,
                o.x + 11.0,
                o.y + 415.0 + dy,
                e,
                winui::ERROR(),
                o.x + 250.0,
                "props-error",
            );
        } else if !complete {
            // How the last try ended: its error, "Stopped" for one stopped (by the person, or
            // as DDM closed; the Status line has its share), else the status.
            let result = item.error.clone().unwrap_or_else(|| match item.status {
                Status::Stopped => crate::i18n::t("Stopped").to_owned(),
                _ => item.status_text(),
            });
            let x1 = o.x + 250.0;
            let (rect, cut) = winui::text_fit(
                &p,
                o.x + 11.0,
                o.y + 415.0 + dy,
                &crate::i18n::fmt(
                    // i18n: {result} = how the last attempt ended (a status or an error message)
                    crate::i18n::t("Result: {result}"),
                    &[("result", &result)],
                ),
                LIGHT_TEXT(),
                x1,
            );
            if cut {
                winui::full_on_hover(ui, rect, "props-result", &result);
            }
        }
        // As wide as its label in this language (#192), into the room before OK.
        let (open_x, open_w) = winui::button_row_from_left(
            &ui.painter().clone(),
            &[("Open", 259.0, 73.0)],
            8.0,
            385.0,
            120.0,
        )[0];
        let open = winui::button_light(
            ui,
            r(o, open_x, 390.0 + dy, open_w, 21.0),
            // i18n: verb: opens the file
            "Open",
            if complete {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        );
        let open = winui::why_disabled(
            open,
            !complete,
            // i18n: tooltip of the greyed Open button in File Properties
            "The download isn't complete yet",
        )
        .clicked();
        if open && complete {
            crate::dialogs::open_file(&item.dest);
        }
        if item.torrent.is_some()
            && winui::button_light(
                ui,
                r(o, 170.0, 390.0 + dy, 78.0, 21.0),
                // i18n: File Properties button that opens the selected torrent's file list
                "Files...",
                ButtonKind::Normal,
                false,
            )
            .clicked()
        {
            self.files_requested = true;
        }
        let ok = winui::button_light(
            ui,
            r(o, 393.0, 390.0 + dy, 73.0, 21.0),
            "OK",
            ButtonKind::Default,
            false,
        )
        .clicked();
        let address_ok = !editable
            || url::Url::parse(self.address.trim()).is_ok_and(|u| mdm_engine::supported(&u));
        if winui::escape_pressed(ui) {
            Outcome::Closed
        } else if ok || winui::enter_pressed(ui) {
            if address_ok {
                Outcome::Done(())
            } else {
                self.error =
                    Some(crate::i18n::t("The address must be an http(s) or ftp(s) address").into());
                Outcome::Open
            }
        } else {
            Outcome::Open
        }
    }
}

/// Windows' name for the file type ("BIN File", "Compressed (zipped) Folder").
fn file_type(name: &str, ext: &str) -> String {
    #[cfg(windows)]
    if let Some(t) = crate::win32::type_name(name) {
        return t;
    }
    let _ = name;
    if ext.is_empty() {
        // i18n: noun: a file of no known type
        crate::i18n::t("File").into()
    } else {
        crate::i18n::fmt(
            // i18n: a kind of file, by its extension: {ext} = "ZIP", "PDF"…
            crate::i18n::t("{ext} File"),
            &[("ext", &ext.to_ascii_uppercase())],
        )
    }
}

/// A read-only field holding a path or an address (left-to-right data): the shared one, so it
/// can be copied.
fn readonly(ui: &mut Ui, rc: egui::Rect, value: &str) {
    winui::readonly_field(ui, rc, value, LIGHT_BG(), true, ("props", rc.top() as i32));
}

fn password(ui: &mut Ui, rc: egui::Rect, value: &mut String, enabled: bool) {
    winui::password_light_on(ui, rc, value, enabled, LIGHT_BG());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Category, VideoSpec};

    fn item(url: &str) -> Item {
        let url = url::Url::parse(url).unwrap();
        Item::new(1, url, PathBuf::from("f.bin"), Category::General)
    }

    /// The owner's report: a finished YouTube video's File Properties said "Server: not one
    /// server (peers, or a helper's)". The Server row names where a download comes from, or isn't
    /// there.
    #[test]
    fn the_server_row_says_where_a_download_comes_from() {
        // A file not run yet: nothing to say, no row (IDM has none at all).
        assert_eq!(server_line(&item("https://files.test/a.zip")), None);
        // Once it ran: the server it connected to.
        let mut ran = item("https://files.test/a.zip");
        let seen = crate::whereis::Seen {
            ip: Some("192.0.2.7".parse().unwrap()),
            rtt_ms: Some(24),
            proxied: false,
            cdn: None,
        };
        ran.server = Some(seen.clone());
        assert_eq!(server_line(&ran), Some(ServerLine::Seen(seen)));
        // A video: its site, through yt-dlp (www. left out).
        let mut video = item("https://www.youtube.com/watch?v=abc");
        video.video = Some(VideoSpec {
            selector: "18".into(),
            subs: Default::default(),
        });
        assert_eq!(
            server_line(&video),
            Some(ServerLine::Helper {
                site: "youtube.com".into(),
                tool: "yt-dlp"
            })
        );
        // A gallery: through gallery-dl.
        let mut gallery = item("https://commons.wikimedia.org/wiki/Category:Sunsets");
        gallery.gallery = Some(Default::default());
        assert_eq!(
            server_line(&gallery),
            Some(ServerLine::Helper {
                site: "commons.wikimedia.org".into(),
                tool: "gallery-dl"
            })
        );
        // A torrent: its peers.
        let mut torrent = item("magnet:?xt=urn:btih:0000000000000000000000000000000000000000");
        torrent.torrent = Some(crate::torrents::TorrentSpec {
            hash: "00".repeat(20),
            files: None,
            folder: "f".into(),
            uploaded: 0,
            seeded_secs: 0,
            peers: 0,
            up_speed: 0.0,
            upload_limit: None,
            in_order: false,
            ends_first: true,
        });
        assert_eq!(server_line(&torrent), Some(ServerLine::Peers));
    }
}
