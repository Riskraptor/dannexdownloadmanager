//! Options (Downloads → Options, the toolbar's Options): IDM's configuration dialog, nine pages
//! in a two-row tab control (notes/ui-spec/options-dialog.md). Light in both modes, as IDM's.
//! It edits a copy of the settings: OK applies it, the close box and Escape discard it.

use egui::{Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::dialogs::{Outcome, window};
use crate::icons::Icon;
use crate::model::Category;
use crate::regional::DateOrder;
use crate::settings::{self, Settings};
// Pages are laid out in English pixels: `fill` makes each one where it is on screen, `text`
// starts at an English pen, and a widget given a screen rect (from `r`) turns it back into its
// English rect (`winui::rect` both ways) before laying out its inside.
use crate::winui::{
    self, ButtonKind, LIGHT_BG, LIGHT_DISABLED, LIGHT_PAGE, LIGHT_TEXT, fill_l as fill, r, text,
};

// The pages, one module each (General … Remote).
mod accounts;
mod archives;
mod connection;
mod downloads;
mod file_types;
mod general;
mod logins;
mod proxy;
mod regional;
mod remote;
mod save_to;
mod sounds;
// The windows Options opens over itself, and the pages' own controls.
mod controls;
mod sub;

// The types and helpers the pages share: each takes them, and this module's imports, with
// `use super::*`.
use self::accounts::*;
use self::connection::*;
use self::controls::*;
use self::downloads::*;
use self::general::*;
use self::logins::*;
use self::proxy::*;
use self::remote::*;
use self::save_to::*;
use self::sub::*;

// What the rest of the app uses.
pub(crate) use self::controls::list_frame;
pub(crate) use self::downloads::DUPLICATE;
pub(crate) use self::sounds::play_sound;

/// The margin round the Options window's tabs, pages and buttons (IDM's had 6 px and less).
const FRAME_X: f32 = 10.0;
const FRAME_Y: f32 = 8.0;
/// How much taller the pages are than IDM's: room under every page's last row (Proxy's FTP box
/// sat on the frame) and Save to's Restore button inside its page.
const PAGE_GROW: f32 = 24.0;

pub struct OptionsDialog {
    s: Settings,
    /// The settings it opened with: OK applies only what changed from them.
    pub from: Settings,
    page: usize,
    /// The browsers check list's scroll and selection.
    browser_rows: winui::RowList,
    category: usize,
    cat_dir: String,
    /// Connection → "Default max. conn. number", as typed.
    conns: String,
    mb: String,
    /// File types → "Don't take files smaller than", as typed (KB).
    min_kb: String,
    hours: String,
    /// The exceptions' and site logins' lists: scroll and selection.
    exc_rows: winui::RowList,
    login_rows: winui::RowList,
    /// Accounts' list (#44).
    account_rows: winui::RowList,
    /// The Sounds list's scroll and selection.
    sound_rows: winui::RowList,
    sub: Option<Sub>,
    /// Save to → New / Edit...: the category editor (D-021).
    cat_edit: Option<crate::categories::CategoryDialog>,
    /// Save to → Restore the built-in categories: its question (#93).
    restore_ask: Option<crate::dialogs::Confirm>,
    /// Connection caps the engine learned, shown as "(auto)" rows under Exceptions.
    learned: Vec<(String, u32)>,
    /// 7-Zip's program (Archives: RAR goes through it), looked for when the window opens.
    seven_zip: Option<std::path::PathBuf>,
    /// Proxy / Socks → Network adapter (#35): the computer's adapters, read again every 2 s.
    adapters: Option<(std::time::Instant, Vec<mdm_engine::net::Adapter>)>,
    /// Remote (#141): the running remote control (its state, paired devices, pairing), the
    /// port as typed, and the paired devices list's scroll and selection.
    remote: Option<mdm_remote::Remote>,
    port: String,
    device_rows: winui::RowList,
    /// Test browser integration → Set it up's runs seen finished (crate::extension).
    set_up_seen: u64,
    /// Sites Logins → Cookies for video sites (#229): the browser in the list (an index into
    /// `mdm_tools::proc::cookie_browsers`), kept while Off is chosen.
    cookie_browser: usize,
    /// Downloads → User-Agent → Automatic (#668): the default browser's, the box then shows it;
    /// unticked, the box's own text (empty is automatic again when the window opens next).
    ua_auto: bool,
    /// Test browser integration → Add to your browser... was pressed: the app opens the
    /// extension's window (crate::extension) and clears this.
    pub offer_extension: bool,
}

impl OptionsDialog {
    /// Open on tab `page` (the numbers in `tabs`: 0 General … 4 Connection, 5 Proxy / Socks,
    /// 6 Sites Logins, 7 Accounts, 8 Sounds, 9 Regional, 10 Archives); for `/show options:<tab>`.
    pub fn on_page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// With the app's remote control (Remote: its state, devices and pairing).
    pub fn with_remote(mut self, remote: Option<mdm_remote::Remote>) -> Self {
        self.remote = remote;
        self
    }

    /// With Remote → Pair a device... open (`/show options:pair`).
    pub fn with_pairing(mut self) -> Self {
        self.page = REMOTE_PAGE;
        self.pair();
        self
    }

    /// With General → Download panels → Edit... open (`/show options:panels`).
    pub fn with_panels(mut self) -> Self {
        self.sub = Some(Sub::Panels(Box::new(PanelsEdit::from(&self.s))));
        self
    }

    /// With General → Test browser integration's window over it, Keys...'s, Customize DDM menu
    /// items' or Accounts → Add's (`/show options:test`, `options:keys`, `options:menu`,
    /// `options:account`: screenshots, #192).
    pub fn with_window(mut self, name: &str) -> Self {
        match name {
            "test" => self.sub = Some(Sub::Test(crate::extension::checks(&self.s))),
            "account" => self.sub = Some(Sub::Account(AccountEdit::default())),
            "keys" => self.sub = Some(Sub::Keys(KeysEdit::from(&self.s))),
            "menu" => self.sub = Some(Sub::Menu(menu_shown(&self.s))),
            "progress" => self.sub = Some(Sub::Progress(ProgressEdit::from(&self.s))),
            // The rest of its dialogs, for screenshots: Add browser (with a sample program),
            // Advanced / Socks, Virus checking, DNS, Sites Logins → New, an exception's New, and
            // the addresses left to the browser.
            "addbrowser" => {
                self.sub = Some(Sub::AddBrowser(
                    "Vivaldi".into(),
                    r"C:\Program Files\Vivaldi\Application\vivaldi.exe".into(),
                ))
            }
            "socks" => self.sub = Some(Sub::Socks(SocksEdit::from(&self.s))),
            "virus" => self.sub = Some(Sub::Virus(VirusEdit::from(&self.s))),
            "dns" => self.sub = Some(Sub::Dns(DnsEdit::from(&self.s))),
            "login" => {
                self.sub = Some(Sub::Login(
                    None,
                    String::new(),
                    String::new(),
                    String::new(),
                ))
            }
            "exception" => self.sub = Some(Sub::Exception(None, String::new(), "1".into())),
            "addresses" => {
                self.sub = Some(Sub::Addresses(self.s.excluded_addresses.join("\n")));
            }
            _ => {}
        }
        self
    }

    pub fn new(
        settings: &Settings,
        downloads: &std::path::Path,
        learned: Vec<(String, u32)>,
    ) -> Self {
        let s = settings.clone();
        OptionsDialog {
            from: s.clone(),
            browser_rows: Default::default(),
            cat_dir: display_dir(&s.category_dir(Category::General, downloads)),
            conns: s.max_connections.clamp(1, s.connection_limit()).to_string(),
            mb: s.limits_mb.to_string(),
            min_kb: s.capture_min_kb.to_string(),
            hours: s.limits_hours.to_string(),
            s,
            page: 0,
            category: 0,
            exc_rows: Default::default(),
            login_rows: Default::default(),
            account_rows: Default::default(),
            sound_rows: Default::default(),
            sub: None,
            set_up_seen: 0,
            cat_edit: None,
            restore_ask: None,
            learned,
            seven_zip: crate::extractq::seven_zip(),
            adapters: None,
            remote: None,
            port: settings.remote_port.to_string(),
            device_rows: Default::default(),
            cookie_browser: cookie_browser_at(settings),
            ua_auto: settings.own_user_agent().is_none(),
            offer_extension: false,
        }
    }

    /// `Done(settings)` on OK.
    pub fn show(&mut self, ctx: &egui::Context, downloads: &std::path::Path) -> Outcome<Settings> {
        let modal = self.sub.is_some() || self.cat_edit.is_some() || self.restore_ask.is_some();
        let outcome = window(
            ctx,
            "options",
            // i18n: the Options window's title
            "Options",
            [430.0 + 2.0 * FRAME_X, 506.0 + PAGE_GROW + 2.0 * FRAME_Y],
            false,
            LIGHT_BG(),
            |ui, o| {
                // The tabs, the pages and the buttons under them, laid out as IDM's, with a
                // margin all round.
                let o = o + vec2(FRAME_X, FRAME_Y);
                // Ctrl+Tab through the pages in the tabs' order (Archives, 10, came last).
                const ORDER: [usize; 12] = [0, 1, 2, 3, 4, 11, 5, 6, 7, 8, 9, 10];
                let at = ORDER.iter().position(|p| *p == self.page).unwrap_or(0);
                let by_key = winui::tab_keys(ui, ORDER.len(), at).map(|i| ORDER[i]);
                if let Some(p) = tabs(ui, o, self.page).or(by_key) {
                    self.page = p;
                }
                match self.page {
                    0 => self.general(ui, o),
                    1 => self.file_types(ui, o),
                    2 => self.save_to(ui, o, downloads),
                    3 => self.downloads(ui, o),
                    4 => self.connection(ui, o),
                    5 => self.proxy(ui, o),
                    6 => self.logins(ui, o),
                    7 => self.accounts(ui, o),
                    9 => self.regional(ui, o),
                    10 => self.archives(ui, o),
                    REMOTE_PAGE => self.remote_page(ui, o),
                    8 => self.sounds(ui, o),
                    _ => self.regional(ui, o),
                }
                let ok = winui::button_light(
                    ui,
                    r(o, 269.0, 479.0 + PAGE_GROW, 73.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                if winui::button_light(
                    ui,
                    r(o, 350.0, 479.0 + PAGE_GROW, 73.0, 21.0),
                    // i18n: noun: button that opens the help
                    "Help",
                    ButtonKind::Normal,
                    false,
                )
                .clicked()
                {
                    crate::help::open_page("options.md");
                }
                // Bottom left, across from OK and Help: Dannex's donation page, quietly.
                let painter = ui.painter().clone();
                if winui::link(
                    ui,
                    &painter,
                    o.x + 8.0,
                    o.y + 494.0 + PAGE_GROW,
                    // i18n: Options: link at the bottom to Dannex's donation page (GitHub Sponsors); keep the short name DDM as it is
                    "Support DDM",
                    "options-sponsor",
                ) {
                    crate::help::sponsor();
                }
                if modal {
                    Outcome::Open
                } else if winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if ok || winui::enter_pressed(ui) {
                    Outcome::Done(())
                } else {
                    Outcome::Open
                }
            },
        );
        if self.sub.is_some() {
            self.sub_dialog(ctx);
        }
        if let Some(d) = &mut self.cat_edit {
            match d.show(ctx, &crate::categories::names()) {
                Outcome::Open => {}
                Outcome::Closed => self.cat_edit = None,
                Outcome::Done(e) => {
                    self.cat_edit = None;
                    let cat = crate::categories::apply(&mut self.s, e, downloads);
                    // Show the category just made or changed.
                    if let Some(i) = Category::all().iter().position(|c| *c == cat) {
                        self.category = i;
                    }
                    self.cat_dir = display_dir(&self.s.category_dir(cat, downloads));
                }
            }
        }
        if let Some(d) = &mut self.restore_ask {
            match d.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.restore_ask = None,
                Outcome::Done(()) => {
                    self.restore_ask = None;
                    // In this draft: Cancel undoes it, as it undoes the dialog's other edits.
                    crate::categories::restore(&mut self.s);
                    let cats = Category::all();
                    self.category = self.category.min(cats.len() - 1);
                    self.cat_dir =
                        display_dir(&self.s.category_dir(cats[self.category], downloads));
                }
            }
        }
        match outcome {
            Outcome::Done(()) => {
                let mut s = self.s.clone();
                s.max_connections = self
                    .conns
                    .parse()
                    .unwrap_or(8)
                    .clamp(1, s.connection_limit());
                s.limits_mb = self.mb.parse().unwrap_or(200).max(1);
                s.capture_min_kb = self.min_kb.parse().unwrap_or(0);
                s.limits_hours = self.hours.parse().unwrap_or(5).max(1);
                s.remote_port = self.remote_port();
                Outcome::Done(s)
            }
            Outcome::Closed => Outcome::Closed,
            Outcome::Open => Outcome::Open,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #424: the list written and read back through a real file: nothing lost, nothing doubled.
    #[test]
    fn sites_round_trip_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sites.txt");
        let sites = "a.test *.b.test";
        std::fs::write(&file, mdm_ipc::capture::sites_to_text(sites)).unwrap();
        let text = read_sites_file(&file).unwrap();
        assert_eq!(text, "a.test\n*.b.test\n");
        let (back, added) = mdm_ipc::capture::merge_sites("", &text);
        assert_eq!((back.as_str(), added), (sites, 2));
        assert_eq!(mdm_ipc::capture::merge_sites(sites, &text).1, 0);
    }
}
