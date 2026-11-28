//! Site grabber (Tasks → Run site grabber…): a wizard as IDM's is laid out (behaviour only):
//! 1. a template (All images, All audio, All video, Documents, Whole site for offline reading,
//!    Custom), the start address, a login and cookies; 2. where the files go (a folder, the
//!    site's own folders kept or not, a category, the pages made to work offline, listed under
//!    All Downloads or under the project only); 3. how far to explore and what to collect
//!    (mdm_sitegrab::Spec). Start explores the site (live: pages explored, files found, the
//!    address being read), then lists the files found, ticked, to filter, sort and pick by type,
//!    for Download selected / Download all to add to a queue as one batch, through the same path
//!    Link grabber's links take (app/sitegrab.rs). Each grab is a project (grabproj.rs), listed
//!    in the Categories tree and the Tasks menu; its grab is the app's, so this window can close
//!    while it explores and open again on it.

use std::path::Path;

use egui::{Pos2, Rect, Ui, pos2, vec2};
use mdm_sitegrab::{End, Fault, Found, Group, Invalid, Phase, Scope, Status, Template};

use crate::chrome::MenuEntry;
use crate::dialogs::{Confirm, Outcome, window};
use crate::grabproj::Project;
use crate::model::Category;
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, r, text};

const W: f32 = 580.0;
const H: f32 = 462.0;
/// The top of the window's row of buttons.
const BUTTONS_Y: f32 = 430.0;
const ROW: f32 = 18.0;
/// The right edge of the content.
const RIGHT: f32 = W - 14.0;

crate::theme::themed! {
    // The file list's header colour, as Link grabber's.
    LIST_HEAD => header;
    LIST_LINE => table_line;
}

/// The wizard's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Start,
    Save,
    Explore,
    Run,
    Files,
}

/// The file list's columns, by which it sorts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Col {
    Name,
    Address,
    Type,
    Size,
}

/// What the wizard asks of the app.
pub enum Ask {
    /// Start (or run again): the app keeps the project and starts the grab.
    Crawl,
    /// Go on from where the last run stopped.
    Continue,
    /// Stop the grab (what it found stays).
    Stop,
    /// Add these files to the download list, into queue `queue`, started when `start`.
    Download {
        files: Vec<Found>,
        queue: usize,
        start: bool,
    },
}

pub struct SiteGrabDlg {
    project: Project,
    page: Page,
    /// The number boxes as typed.
    depth: String,
    max_pages: String,
    max_files: String,
    delay: String,
    connections: String,
    /// The project's name was typed (else it follows the start address's host).
    named: bool,
    /// Its project is exploring (the app's grab), and how far it is.
    running: bool,
    status: Status,
    /// The files found: ticked, and already in the download list.
    checked: Vec<bool>,
    known: Vec<bool>,
    filter: String,
    /// The type of file shown: 0 every type, then each of `KINDS`, then the others.
    kind: usize,
    /// The sizes shown, in KB as typed: larger than one, smaller than the other (empty: any).
    larger: String,
    smaller: String,
    sort: Option<(Col, bool)>,
    /// The rows shown, in order (worked out again when the filter, type, order or files change).
    order: Option<Vec<usize>>,
    rows: winui::RowList,
    /// What the page's own check found wrong (shown in red), until it is fixed.
    error: Option<&'static str>,
    focused: bool,
    /// Where an offline copy goes when no folder is typed (General's folder).
    default_folder: String,
}

/// The types of file the list can show alone (its Type drop-down).
const KINDS: [Group; 8] = [
    Group::Images,
    Group::Video,
    Group::Audio,
    Group::Documents,
    Group::Archives,
    Group::Programs,
    Group::Pages,
    Group::Assets,
];

impl SiteGrabDlg {
    /// A new project, starting from `defaults` (the last queue chosen, say).
    pub fn new(defaults: Project, default_folder: String) -> Self {
        let mut d = Self::edit(defaults, default_folder);
        d.named = false;
        d
    }

    /// Edit `project`'s settings, from the first page.
    pub fn edit(project: Project, default_folder: String) -> Self {
        let n = project.found.len();
        let spec = &project.spec;
        SiteGrabDlg {
            depth: spec.depth.to_string(),
            max_pages: spec.max_pages.to_string(),
            max_files: spec.max_files.to_string(),
            delay: spec.delay_ms.to_string(),
            connections: spec.connections.to_string(),
            named: !project.name.trim().is_empty(),
            page: Page::Start,
            running: false,
            status: Status::default(),
            checked: vec![true; n],
            known: vec![false; n],
            filter: String::new(),
            kind: 0,
            larger: String::new(),
            smaller: String::new(),
            sort: None,
            order: None,
            rows: Default::default(),
            error: None,
            focused: false,
            project,
            default_folder,
        }
    }

    /// The files `project`'s last run found, to download.
    pub fn files_of(project: Project, default_folder: String) -> Self {
        let mut d = Self::edit(project, default_folder);
        d.page = Page::Files;
        d
    }

    /// A project with files found, without the network (`/show site-grabber`: its screenshots
    /// and the window's check in every language).
    pub fn sample() -> Self {
        let mut p = Project {
            name: "photos.example".into(),
            ..Project::default()
        };
        p.spec.start = "https://photos.example/gallery/".into();
        let files = [
            ("gallery/harbour.jpg", Some(1_843_200)),
            ("gallery/harbour-wide.jpg", Some(3_276_800)),
            ("gallery/lighthouse.png", Some(942_080)),
            ("gallery/2025/market.jpg", Some(2_097_152)),
            ("gallery/2025/night.webp", None),
            ("images/logo.svg", Some(5_120)),
        ];
        p.found = files
            .iter()
            .map(|(path, size)| Found {
                url: format!("https://photos.example/{path}"),
                page: "https://photos.example/gallery/".into(),
                name: path.rsplit('/').next().unwrap_or(path).into(),
                size: *size,
                kind: Default::default(),
                also: Vec::new(),
            })
            .collect();
        p.ended = Some(End::Finished);
        let mut d = Self::files_of(p, String::new());
        if let Some(k) = d.checked.last_mut() {
            *k = false;
        }
        d
    }

    /// [`Self::sample`] on one of its pages (`start`, `save`, `explore`, `run`; else the files):
    /// each page's screenshot. `run` shows a grab under way.
    pub fn sample_on(page: &str) -> Self {
        let mut d = Self::sample();
        d.project.offline = page == "save";
        d.page = match page {
            "start" => Page::Start,
            "save" => Page::Save,
            "explore" => Page::Explore,
            "run" => {
                d.running = true;
                d.status = Status {
                    pages: 42,
                    waiting: 17,
                    files: 318,
                    current: Some("https://photos.example/gallery/2025/".into()),
                    ..Status::default()
                };
                Page::Run
            }
            _ => Page::Files,
        };
        if page == "files-early" {
            d.project.ended = Some(End::MaxPages);
            d.project.frontier = Some(Default::default());
        }
        d
    }

    /// Download all, as its button asks (an off-screen test's: tests/site_grab.rs).
    pub fn download_all(&self) -> Ask {
        let files = self
            .project
            .found
            .iter()
            .enumerate()
            .filter(|&(i, _)| !self.known.get(i).copied().unwrap_or(false))
            .map(|(_, f)| f.clone())
            .collect();
        Ask::Download {
            files,
            queue: self.project.queue,
            start: self.project.start_now,
        }
    }

    pub fn id(&self) -> u64 {
        self.project.id
    }

    pub fn set_id(&mut self, id: u64) {
        self.project.id = id;
    }

    /// The project as the pages have it now (the number boxes read).
    pub fn project(&self) -> Project {
        let mut p = self.project.clone();
        let n = |s: &str, min: u32, max: u32| winui::number_in(s, min.into(), max.into()) as u32;
        p.spec.depth = n(&self.depth, 0, mdm_sitegrab::MAX_DEPTH);
        p.spec.max_pages = n(&self.max_pages, 1, mdm_sitegrab::MAX_PAGES);
        p.spec.max_files = n(&self.max_files, 1, mdm_sitegrab::MAX_FILES);
        p.spec.delay_ms = n(&self.delay, 0, mdm_sitegrab::MAX_DELAY_MS);
        p.spec.connections = n(&self.connections, 1, mdm_sitegrab::MAX_CONNECTIONS);
        p.spec.start = p.spec.start.trim().to_owned();
        p.name = p.name.trim().to_owned();
        p.folder = p.folder.trim().to_owned();
        p
    }

    /// The app started its project's grab (Start, Continue, Run again): its progress is shown.
    pub fn run_started(&mut self) {
        self.running = true;
        self.status = Status::default();
        self.page = Page::Run;
        self.error = None;
    }

    /// How far the grab is (each frame while it runs, from the app).
    pub fn set_status(&mut self, status: Option<Status>) {
        match status {
            Some(s) => {
                self.running = !matches!(s.phase, Phase::Ended(_));
                self.status = s;
            }
            None => self.running = false,
        }
    }

    /// The grab couldn't start: the page to fix shows why.
    pub fn refused(&mut self, why: Invalid) {
        self.running = false;
        match why {
            Invalid::Address => {
                self.page = Page::Start;
                self.error = Some(bad_address());
            }
            Invalid::NoTypes => {
                self.page = Page::Explore;
                self.error = Some(no_types());
            }
        }
    }

    /// Another project's grab is under way: this one waits.
    pub fn refuse_busy(&mut self) {
        // i18n: Site grabber: Start or Continue while another project explores its site
        self.error = Some(crate::i18n::mark(
            "Another project is exploring a site. Wait for it to end, or stop it first.",
        ));
    }

    /// The grab ended: `p` holds what it found (the app kept it). A start page that gave
    /// nothing stays on the progress page, which says why; else the files are listed.
    pub fn crawl_ended(&mut self, p: Project, known: impl Fn(&str) -> bool) {
        self.running = false;
        let failed = matches!(p.ended, Some(End::Failed(_)));
        self.project.found = p.found;
        self.project.ended = p.ended;
        self.project.frontier = p.frontier;
        self.project.last_run = p.last_run;
        let n = self.project.found.len();
        self.checked = vec![true; n];
        self.mark_known(known);
        self.rows = Default::default();
        self.order = None;
        if !failed {
            self.page = Page::Files;
        }
    }

    /// Files already in the download list are shown so and left unticked.
    pub fn mark_known(&mut self, known: impl Fn(&str) -> bool) {
        self.known = self.project.found.iter().map(|f| known(&f.url)).collect();
        self.checked.resize(self.project.found.len(), true);
        for (c, k) in self.checked.iter_mut().zip(&self.known) {
            if *k {
                *c = false;
            }
        }
    }

    /// `queues`: the queues' names, in order (the files join one of them).
    pub fn show(&mut self, ctx: &egui::Context, queues: &[String]) -> Outcome<Ask> {
        if self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        window(
            ctx,
            "site-grabber",
            // i18n: title of the Site grabber window (Tasks → Run site grabber)
            "Site grabber",
            [W, H],
            true,
            LIGHT_BG(),
            |ui, o| self.body(ui, o, queues),
        )
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2, queues: &[String]) -> Outcome<Ask> {
        let p = ui.painter().clone();
        let ended_early = self.project.ended.as_ref().filter(|e| e.early()).cloned();
        let (title, about) = match self.page {
            Page::Start => (
                // i18n: Site grabber, first page's heading
                crate::i18n::t("What to grab, and where to start"),
                // i18n: Site grabber, first page's explanation
                crate::i18n::t("Choose a template and the page the grabber starts from.")
                    .to_owned(),
            ),
            Page::Save => (
                // i18n: Site grabber, second page's heading
                crate::i18n::t("Where to save the files"),
                // i18n: Site grabber, second page's explanation
                crate::i18n::t("Choose the folder and the category of the files it downloads.")
                    .to_owned(),
            ),
            Page::Explore => (
                // i18n: Site grabber, third page's heading
                crate::i18n::t("What to explore and collect"),
                // i18n: Site grabber, third page's explanation
                crate::i18n::t("Choose how far to follow links and which files to collect.")
                    .to_owned(),
            ),
            Page::Run => (
                // i18n: Site grabber, the page shown while it explores the site
                crate::i18n::t("Exploring the site"),
                // i18n: Site grabber, the explanation while it explores the site
                crate::i18n::t("Pages are read a few at a time, with a pause between requests.")
                    .to_owned(),
            ),
            Page::Files => (
                // i18n: Site grabber, the page listing what it found
                crate::i18n::t("Files found"),
                match &ended_early {
                    // How the last run ended, when it stopped before the end: Continue goes on.
                    Some(end) => crate::i18n::fmt(
                        // i18n: Site grabber, above the files found when the last run stopped early; {end} says how ("Stopped at the most pages to explore.")
                        crate::i18n::t("{end} Continue explores further."),
                        &[("end", &end_text(end))],
                    ),
                    // i18n: Site grabber, the explanation above the list of files found
                    None => crate::i18n::t("Tick the files to download, then add them to a queue.")
                        .to_owned(),
                },
            ),
        };
        winui::text_font(
            &p,
            winui::tahoma_bold(),
            o.x + 14.0,
            o.y + 22.0,
            title,
            LIGHT_TEXT(),
        );
        winui::fit(
            ui,
            &p,
            o.x + 14.0,
            o.y + 40.0,
            &about,
            winui::MUTED(),
            o.x + RIGHT,
            "sitegrab-about",
        );
        winui::etched_h(&p, o.x, o.y + 52.0, W);
        match self.page {
            Page::Start => self.start_page(ui, o),
            Page::Save => self.save_page(ui, o),
            Page::Explore => self.explore_page(ui, o),
            Page::Run => self.run_page(ui, o),
            Page::Files => self.files_page(ui, o, queues),
        }
        if let Some(e) = self.error {
            let rc = winui::fit(
                ui,
                &p,
                o.x + 14.0,
                o.y + BUTTONS_Y - 12.0,
                crate::i18n::t(e),
                winui::ERROR(),
                o.x + RIGHT,
                "sitegrab-error",
            );
            winui::readable(ui, rc, "sitegrab-error", crate::i18n::t(e));
        }
        winui::etched_h(&p, o.x, o.y + BUTTONS_Y - 8.0, W);
        self.buttons(ui, o)
    }

    // ---- page 1: template, start address, login and cookies --------------------------------

    fn start_page(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        let labels = [
            // i18n: Site grabber: label of the template drop-down (All images, Documents…)
            crate::i18n::t("Template:"),
            // i18n: Site grabber: label of the address of the page it starts from
            crate::i18n::t("Start address:"),
            // i18n: Site grabber: label of the project's name (shown under Grabber projects)
            crate::i18n::t("Project name:"),
        ];
        let fx = winui::label_column(&p, 14.0, &labels, 120.0);
        let label = |y: f32, s: &str| {
            winui::text_right(&p, o.x + fx - 7.0, o.y + y, s, LIGHT_TEXT());
        };
        label(80.0, labels[0]);
        let names: Vec<&str> = Template::ALL.iter().map(|t| template_name(*t)).collect();
        let now = Template::ALL
            .iter()
            .position(|t| *t == self.project.template)
            .unwrap_or(0);
        if let Some(i) = winui::combo_light(
            ui,
            r(o, fx, 64.0, 260.0, 23.0),
            &names,
            now,
            true,
            "sitegrab-template",
        ) {
            self.set_template(Template::ALL[i]);
        }
        winui::paragraph(
            &p,
            o.x + fx,
            o.y + 106.0,
            template_about(self.project.template),
            winui::MUTED(),
            o.x + RIGHT,
            15.0,
        );

        label(150.0, labels[1]);
        let before = self.project.spec.start.clone();
        let edit = winui::edit_light(
            ui,
            r(o, fx, 136.0, RIGHT - fx, 23.0),
            &mut self.project.spec.start,
            true,
            false,
        );
        if !self.focused {
            edit.request_focus();
            self.focused = true;
        }
        if self.project.spec.start != before {
            if self.error == Some(bad_address()) {
                self.error = None;
            }
            if !self.named {
                self.project.name = mdm_sitegrab::start_url(&self.project.spec.start)
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_default();
            }
        }
        winui::fit(
            ui,
            &p,
            o.x + fx,
            o.y + 176.0,
            // i18n: Site grabber: under the start address
            crate::i18n::t("For example https://example.com/gallery/"),
            winui::MUTED(),
            o.x + RIGHT,
            "sitegrab-start-hint",
        );

        label(212.0, labels[2]);
        let before = self.project.name.clone();
        winui::edit_light(
            ui,
            r(o, fx, 198.0, RIGHT - fx, 23.0),
            &mut self.project.name,
            true,
            false,
        );
        if self.project.name != before {
            self.named = !self.project.name.trim().is_empty();
        }

        group(
            ui,
            o,
            14.0,
            236.0,
            W - 28.0,
            128.0,
            // i18n: Site grabber: the box of the site's user name, password and cookies
            "Login and cookies, if the site asks for them",
        );
        let inner = [
            // i18n: Site grabber: label of the site's user name
            crate::i18n::t("User name:"),
            // i18n: Site grabber: label of the site's password
            crate::i18n::t("Password:"),
            // i18n: Site grabber: label of the cookies sent to the site (as a browser sends them: name=value; name2=value2)
            crate::i18n::t("Cookies:"),
        ];
        let lx = winui::label_column(&p, 26.0, &inner, 120.0).max(fx);
        winui::text_right(&p, o.x + lx - 7.0, o.y + 274.0, inner[0], LIGHT_TEXT());
        winui::edit_light(
            ui,
            r(o, lx, 260.0, 220.0, 23.0),
            &mut self.project.user,
            true,
            false,
        );
        winui::text_right(&p, o.x + lx - 7.0, o.y + 303.0, inner[1], LIGHT_TEXT());
        winui::password_light(
            ui,
            r(o, lx, 289.0, 220.0, 23.0),
            &mut self.project.password,
            true,
        );
        winui::text_right(&p, o.x + lx - 7.0, o.y + 332.0, inner[2], LIGHT_TEXT());
        winui::edit_light(
            ui,
            r(o, lx, 318.0, RIGHT - 12.0 - lx, 23.0),
            &mut self.project.cookies,
            true,
            false,
        );
        winui::fit(
            ui,
            &p,
            o.x + lx,
            o.y + 356.0,
            // i18n: Site grabber: under the Cookies box: how cookies are written
            crate::i18n::t("As the browser sends them: name=value; name2=value2"),
            winui::MUTED(),
            o.x + RIGHT - 12.0,
            "sitegrab-cookies-hint",
        );
        winui::paragraph(
            &p,
            o.x + 26.0,
            o.y + 384.0,
            // i18n: Site grabber: where its login and cookies go
            crate::i18n::t(
                "The login and cookies go to the start address's site only. Sites Logins in Options are used too.",
            ),
            winui::MUTED(),
            o.x + RIGHT,
            15.0,
        );
    }

    fn set_template(&mut self, t: Template) {
        let mut spec = self.project().spec;
        t.apply(&mut spec);
        self.depth = spec.depth.to_string();
        self.max_pages = spec.max_pages.to_string();
        self.max_files = spec.max_files.to_string();
        self.project.spec.groups = spec.groups;
        self.project.spec.custom = spec.custom;
        self.project.spec.sizes = spec.sizes;
        self.project.template = t;
        // A site to read offline: its pages made to work from the disk, under one folder.
        if t != Template::Custom {
            self.project.offline = t == Template::Offline;
        }
        if self.project.offline {
            self.project.keep_folders = true;
            if self.project.folder.trim().is_empty() {
                self.project.folder = self.default_folder.clone();
            }
        }
        if self.error == Some(no_types()) {
            self.error = None;
        }
    }

    // ---- page 2: folder, folders kept, category, offline, listed ---------------------------

    fn save_page(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        let labels = [
            // i18n: Site grabber: label of the folder the files go to
            crate::i18n::t("Save to:"),
            // i18n: Site grabber: label of the category its downloads are listed under
            crate::i18n::t("Category:"),
        ];
        let fx = winui::label_column(&p, 14.0, &labels, 90.0);
        winui::text_right(&p, o.x + fx - 7.0, o.y + 80.0, labels[0], LIGHT_TEXT());
        let (browse_x, browse_w) =
            winui::buttons_to_right(&p, RIGHT, 8.0, &[("Browse...", 73.0)], 160.0)[0];
        winui::edit_light(
            ui,
            r(o, fx, 66.0, browse_x - 8.0 - fx, 23.0),
            &mut self.project.folder,
            true,
            false,
        );
        if winui::button_light(
            ui,
            r(o, browse_x, 67.0, browse_w, 21.0),
            "Browse...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            // i18n: title of the folder picker of Site grabber
            let title = crate::i18n::t("Site grabber folder").to_owned();
            let typed = Path::new(self.project.folder.trim()).to_path_buf();
            winui::start_pick(ui.ctx(), "sitegrab-folder", move || {
                let mut pick = rfd::FileDialog::new().set_title(title);
                if typed.is_dir() {
                    pick = pick.set_directory(&typed);
                }
                pick.pick_folder()
            });
        }
        if let Some(dir) = winui::picked(ui.ctx(), "sitegrab-folder") {
            self.project.folder = dir.display().to_string();
        }
        winui::paragraph(
            &p,
            o.x + fx,
            o.y + 108.0,
            if self.project.offline {
                // i18n: Site grabber: under Save to, when the pages are made to work offline
                crate::i18n::t("Left empty, the site goes to the General category's folder.")
            } else {
                // i18n: Site grabber: under Save to
                crate::i18n::t(
                    "Left empty, each file goes to its category's folder (Options → Save to).",
                )
            },
            winui::MUTED(),
            o.x + RIGHT,
            15.0,
        );
        let offline = self.project.offline;
        if winui::radio_light(
            ui,
            o + vec2(fx, 132.0),
            self.project.keep_folders || offline,
            // i18n: Site grabber: a choice of where files go: under folders named after the site and the address's folders
            "Keep the site's folders (its name, then each folder of the address)",
            !offline,
        )
        .clicked()
        {
            self.project.keep_folders = true;
        }
        if winui::radio_light(
            ui,
            o + vec2(fx, 152.0),
            !self.project.keep_folders && !offline,
            // i18n: Site grabber: a choice of where files go: all in the folder chosen
            "Put every file in this one folder",
            !offline,
        )
        .clicked()
        {
            self.project.keep_folders = false;
        }

        winui::text_right(&p, o.x + fx - 7.0, o.y + 196.0, labels[1], LIGHT_TEXT());
        let cats = Category::all();
        let shown: Vec<String> = cats.iter().map(|c| c.shown()).collect();
        let mut items: Vec<&str> = vec![
            // i18n: Site grabber: the category choice that lists each file under its type's category
            crate::i18n::t("Each by its file type"),
        ];
        items.extend(shown.iter().map(String::as_str));
        let now = self
            .project
            .category
            .and_then(|c| cats.iter().position(|x| *x == c))
            .map_or(0, |i| i + 1);
        if let Some(i) = winui::combo_light(
            ui,
            r(o, fx, 180.0, 240.0, 23.0),
            &items,
            now,
            true,
            "sitegrab-category",
        ) {
            self.project.category = i.checked_sub(1).and_then(|i| cats.get(i).copied());
        }

        if winui::checkbox_light(
            ui,
            o + vec2(14.0, 226.0),
            &mut self.project.offline,
            // i18n: Site grabber: check box: the downloaded pages' links are changed to open the saved copies, to read the site without a connection
            "Make the pages work offline: their links open the saved copies",
            true,
        )
        .clicked()
            && self.project.offline
        {
            self.project.keep_folders = true;
            if self.project.folder.trim().is_empty() {
                self.project.folder = self.default_folder.clone();
            }
        }
        winui::paragraph(
            &p,
            o.x + 32.0,
            o.y + 254.0,
            // i18n: Site grabber: under "Make the pages work offline"
            crate::i18n::t(
                "Each file is saved where its address says, and each page and style sheet is changed once downloaded. Right-click the project, then Open offline copy.",
            ),
            winui::MUTED(),
            o.x + RIGHT,
            15.0,
        );
        winui::checkbox_light(
            ui,
            o + vec2(14.0, 306.0),
            &mut self.project.listed,
            // i18n: Site grabber: check box: the downloads are listed under All Downloads as well as under the project
            "List the downloads under All Downloads too",
            true,
        );
        winui::paragraph(
            &p,
            o.x + 32.0,
            o.y + 334.0,
            // i18n: Site grabber: under "List the downloads under All Downloads too"
            crate::i18n::t(
                "Else they show under the project, in Grabber projects, and in their queue.",
            ),
            winui::MUTED(),
            o.x + RIGHT,
            15.0,
        );
    }

    // ---- page 3: depth, scope, types, patterns, limits -------------------------------------

    fn explore_page(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        let mid = 300.0;
        // Left: how far, where, which types.
        let labels = [
            // i18n: Site grabber: label of how many links deep it explores
            crate::i18n::t("Depth:"),
            // i18n: Site grabber: label of the user's own file extensions to collect
            crate::i18n::t("Other types:"),
        ];
        let fx = winui::label_column(&p, 14.0, &labels, 70.0);
        winui::text_right(&p, o.x + fx - 7.0, o.y + 80.0, labels[0], LIGHT_TEXT());
        winui::spin_box(
            ui,
            r(o, fx, 64.0, 64.0, 23.0),
            &mut self.depth,
            0,
            u64::from(mdm_sitegrab::MAX_DEPTH),
            true,
            "sitegrab-depth",
        );
        winui::fit(
            ui,
            &p,
            o.x + fx + 72.0,
            o.y + 80.0,
            // i18n: Site grabber: after the Depth box: how many links away from the start page pages are explored
            crate::i18n::t("links from the start page"),
            LIGHT_TEXT(),
            o.x + mid - 12.0,
            "sitegrab-depth-after",
        );
        text(
            &p,
            o.x + 14.0,
            o.y + 110.0,
            // i18n: Site grabber: before the choice of which sites' pages are explored
            crate::i18n::t("Explore pages on:"),
            LIGHT_TEXT(),
        );
        let scopes = [
            // i18n: Site grabber: explore pages on the start address's host only
            (Scope::SameHost, crate::i18n::mark("The same host")),
            // i18n: Site grabber: explore pages on the start address's domain and its subdomains
            (
                Scope::SameDomain,
                crate::i18n::mark("The same domain and its subdomains"),
            ),
            // i18n: Site grabber: explore pages on any site the links lead to
            (Scope::Any, crate::i18n::mark("Any site")),
        ];
        for (k, (scope, caption)) in scopes.into_iter().enumerate() {
            if winui::radio_light_until(
                ui,
                o + vec2(24.0, 116.0 + 19.0 * k as f32),
                self.project.spec.scope == scope,
                caption,
                true,
                Some(o.x + mid - 8.0),
            )
            .clicked()
            {
                self.project.spec.scope = scope;
            }
        }
        winui::checkbox_light_until(
            ui,
            o + vec2(14.0, 178.0),
            &mut self.project.spec.below,
            // i18n: Site grabber: explore only the pages in the start address's folder and below it (part of a site), not those above it
            "Only pages under the start address's folder",
            true,
            Some(o.x + mid - 8.0),
        );
        winui::checkbox_light_until(
            ui,
            o + vec2(14.0, 198.0),
            &mut self.project.spec.files_anywhere,
            // i18n: Site grabber: collect the files explored pages link to on other sites too (pictures on another host)
            "Collect files from other sites too",
            true,
            Some(o.x + mid - 8.0),
        );
        text(
            &p,
            o.x + 14.0,
            o.y + 234.0,
            // i18n: Site grabber: before the file types to collect
            crate::i18n::t("File types:"),
            LIGHT_TEXT(),
        );
        let groups = [
            // i18n: Site grabber: a file type to collect
            (Group::Images, crate::i18n::mark("Images")),
            // i18n: Site grabber: a file type to collect
            (Group::Video, crate::i18n::mark("Video")),
            // i18n: Site grabber: a file type to collect
            (Group::Audio, crate::i18n::mark("Audio")),
            // i18n: Site grabber: a file type to collect
            (Group::Documents, crate::i18n::mark("Documents")),
            // i18n: Site grabber: a file type to collect (zip, rar, 7z…)
            (Group::Archives, crate::i18n::mark("Archives")),
            // i18n: Site grabber: a file type to collect (exe, msi, dmg…)
            (Group::Programs, crate::i18n::mark("Programs")),
            // i18n: Site grabber: a file type to collect: the pages themselves
            (Group::Pages, crate::i18n::mark("Web pages")),
            // i18n: Site grabber: a file type to collect: what pages need to show
            (Group::Assets, crate::i18n::mark("Styles, scripts, fonts")),
        ];
        let mut changed = false;
        for (k, (g, caption)) in groups.into_iter().enumerate() {
            let (x, until) = if k < 4 {
                (24.0, 150.0)
            } else {
                (156.0, mid - 8.0)
            };
            let mut on = self.project.spec.groups.contains(&g);
            if winui::checkbox_light_until(
                ui,
                o + vec2(x, 242.0 + 19.0 * (k % 4) as f32),
                &mut on,
                caption,
                true,
                Some(o.x + until),
            )
            .clicked()
            {
                changed = true;
                if on {
                    self.project.spec.groups.push(g);
                } else {
                    self.project.spec.groups.retain(|x| *x != g);
                }
            }
        }
        winui::text_right(&p, o.x + fx - 7.0, o.y + 342.0, labels[1], LIGHT_TEXT());
        let before = self.project.spec.custom.clone();
        winui::edit_light(
            ui,
            r(o, fx, 328.0, mid - 12.0 - fx, 23.0),
            &mut self.project.spec.custom,
            true,
            false,
        );
        changed |= self.project.spec.custom != before;
        winui::fit(
            ui,
            &p,
            o.x + fx,
            o.y + 368.0,
            // i18n: Site grabber: under Other types
            crate::i18n::t("Extensions separated by spaces, for example PSD RAW"),
            winui::MUTED(),
            o.x + mid - 12.0,
            "sitegrab-custom-hint",
        );
        if changed {
            // Types chosen by hand: the template that has them, else Custom.
            self.project.template = Template::of(&self.project.spec);
            if self.error == Some(no_types()) {
                self.error = None;
            }
        }

        // Right: the addresses taken or left, the limits, politeness.
        let rx = mid + 6.0;
        text(
            &p,
            o.x + rx,
            o.y + 76.0,
            // i18n: Site grabber: label of the patterns a file's address must match to be collected
            crate::i18n::t("Only files whose address is like:"),
            LIGHT_TEXT(),
        );
        winui::edit_light(
            ui,
            r(o, rx, 82.0, RIGHT - rx, 23.0),
            &mut self.project.spec.include,
            true,
            false,
        );
        text(
            &p,
            o.x + rx,
            o.y + 126.0,
            // i18n: Site grabber: label of the patterns of addresses neither explored nor collected
            crate::i18n::t("Leave out addresses like:"),
            LIGHT_TEXT(),
        );
        winui::edit_light(
            ui,
            r(o, rx, 132.0, RIGHT - rx, 23.0),
            &mut self.project.spec.exclude,
            true,
            false,
        );
        winui::paragraph(
            &p,
            o.x + rx,
            o.y + 172.0,
            // i18n: Site grabber: how the address patterns are written
            crate::i18n::t(
                "* stands for any text and ? for one character; a pattern without them matches any address that contains it. Separate patterns with spaces.",
            ),
            winui::MUTED(),
            o.x + RIGHT,
            14.0,
        );
        let limits: [(&str, u32, u32); 4] = [
            // i18n: Site grabber: label of the most pages it explores
            (crate::i18n::t("Most pages:"), 1, mdm_sitegrab::MAX_PAGES),
            // i18n: Site grabber: label of the most files it collects
            (crate::i18n::t("Most files:"), 1, mdm_sitegrab::MAX_FILES),
            // i18n: Site grabber: label of the pause between two requests to one site, in milliseconds
            (
                crate::i18n::t("Pause between requests (ms):"),
                0,
                mdm_sitegrab::MAX_DELAY_MS,
            ),
            // i18n: Site grabber: label of how many requests it makes at once
            (
                crate::i18n::t("Requests at once:"),
                1,
                mdm_sitegrab::MAX_CONNECTIONS,
            ),
        ];
        let boxes = [
            &mut self.max_pages,
            &mut self.max_files,
            &mut self.delay,
            &mut self.connections,
        ];
        let bx = RIGHT - 74.0;
        for (k, ((caption, min, max), value)) in limits.into_iter().zip(boxes).enumerate() {
            let y = 236.0 + 28.0 * k as f32;
            let (rc, cut) = winui::text_right_within(
                &p,
                o.x + bx - 7.0,
                o.y + y + 14.0,
                caption,
                LIGHT_TEXT(),
                o.x + rx,
            );
            if cut {
                winui::full_on_hover(ui, rc, ("sitegrab-limit", k), caption);
            }
            winui::spin_box(
                ui,
                r(o, bx, y, 74.0, 23.0),
                value,
                u64::from(min),
                u64::from(max),
                true,
                &format!("sitegrab-limit-{k}"),
            );
        }
        winui::checkbox_light(
            ui,
            o + vec2(rx, 352.0),
            &mut self.project.spec.robots,
            // i18n: Site grabber: check box: leave out the pages the site's robots.txt asks robots not to visit
            "Follow the site's robots.txt",
            true,
        );
        winui::checkbox_light(
            ui,
            o + vec2(rx, 374.0),
            &mut self.project.spec.sizes,
            // i18n: Site grabber: check box: ask the server each file's size once exploring is done
            "Look up each file's size",
            true,
        );
    }

    // ---- while it explores ------------------------------------------------------------------

    fn run_page(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        let s = &self.status;
        let int = |n: u64| crate::i18n::integer(n);
        let (line, color) = match &s.phase {
            Phase::Exploring => (
                // i18n: Site grabber: while it reads the site's pages
                crate::i18n::t("Reading the pages...").to_owned(),
                LIGHT_TEXT(),
            ),
            Phase::Sizing { done, total } => (
                crate::i18n::fmt(
                    // i18n: Site grabber: after exploring, while it asks each file's size; {done} and {total} are numbers
                    crate::i18n::t("Looking up the files' sizes: {done} of {total}"),
                    &[("done", &int(*done as u64)), ("total", &int(*total as u64))],
                ),
                LIGHT_TEXT(),
            ),
            Phase::Ended(end) => (
                end_text(end),
                match end {
                    End::Failed(_) => winui::ERROR(),
                    _ => LIGHT_TEXT(),
                },
            ),
        };
        let running = self.running && !matches!(s.phase, Phase::Ended(_));
        let x = if running {
            crate::torrentdlg::spinner(ui, pos2(winui::mx(o.x + 20.0), o.y + 76.0), 6.0);
            34.0
        } else {
            14.0
        };
        let rc = winui::fit(
            ui,
            &p,
            o.x + x,
            o.y + 80.0,
            &line,
            color,
            o.x + RIGHT,
            "sitegrab-phase",
        );
        winui::readable(ui, rc, "sitegrab-phase", &line);
        if running {
            let fraction = match s.phase {
                Phase::Sizing { done, total } if total > 0 => Some(done as f32 / total as f32),
                _ => None,
            };
            winui::progress_bar(
                &p,
                r(o, 14.0, 92.0, W - 28.0, 4.0),
                fraction,
                ui.input(|i| i.time),
            );
        }
        let counts = [
            // i18n: Site grabber: how many pages it has read; {n} is a number
            (crate::i18n::t("Pages explored: {n}"), u64::from(s.pages)),
            // i18n: Site grabber: how many pages are still to be read; {n} is a number
            (crate::i18n::t("Pages waiting: {n}"), u64::from(s.waiting)),
            // i18n: Site grabber: how many files it has collected; {n} is a number
            (crate::i18n::t("Files found: {n}"), s.files as u64),
            // i18n: Site grabber: how many addresses the site's robots.txt kept it from; {n} is a number
            (
                crate::i18n::t("Left out for robots.txt: {n}"),
                u64::from(s.robots_skipped),
            ),
            // i18n: Site grabber: how many addresses failed (an error, no answer); {n} is a number
            (
                crate::i18n::t("Could not be read: {n}"),
                u64::from(s.failed),
            ),
        ];
        for (k, (template, n)) in counts.iter().enumerate() {
            let line = crate::i18n::fmt(template, &[("n", &int(*n))]);
            let rc = winui::fit(
                ui,
                &p,
                o.x + 14.0,
                o.y + 128.0 + 20.0 * k as f32,
                &line,
                LIGHT_TEXT(),
                o.x + RIGHT,
                ("sitegrab-count", k),
            );
            winui::readable(ui, rc, ("sitegrab-count", k), &line);
        }
        if let Some(url) = &s.current {
            let shown = crate::i18n::fmt(
                // i18n: Site grabber: the address it is reading now; {address} is a web address
                crate::i18n::t("Now: {address}"),
                &[("address", &crate::i18n::ltr(url))],
            );
            winui::fit_middle(
                ui,
                &p,
                o.x + 14.0,
                o.y + 246.0,
                &shown,
                winui::MUTED(),
                o.x + RIGHT,
                "sitegrab-now",
            );
        }
        if let Phase::Ended(End::Failed(fault)) = &s.phase {
            let why = fault_text(fault);
            winui::paragraph(
                &p,
                o.x + 14.0,
                o.y + 246.0,
                &why,
                LIGHT_TEXT(),
                o.x + RIGHT,
                15.0,
            );
        }
        if running {
            winui::paragraph(
                &p,
                o.x + 14.0,
                o.y + 290.0,
                // i18n: Site grabber, while it explores: closing the window doesn't stop it
                crate::i18n::t(
                    "You can close this window: exploring goes on. The project shows under Grabber projects in Categories; open it there to come back here.",
                ),
                winui::MUTED(),
                o.x + RIGHT,
                15.0,
            );
        }
    }

    // ---- the files found --------------------------------------------------------------------

    /// The rows the filter and the type show, in the order chosen (indexes into the files).
    fn shown(&mut self) -> Vec<usize> {
        if let Some(o) = &self.order {
            return o.clone();
        }
        let f = self.filter.trim().to_lowercase();
        let kind = self.kind;
        // The sizes typed (KB); a file of no known size is shown whatever they say.
        let kb = |s: &str| s.trim().parse::<u64>().ok().map(|k| k.saturating_mul(1024));
        let (larger, smaller) = (kb(&self.larger), kb(&self.smaller));
        let found = &self.project.found;
        let mut rows: Vec<usize> = found
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                (f.is_empty()
                    || x.name.to_lowercase().contains(&f)
                    || x.url.to_lowercase().contains(&f))
                    && (kind == 0 || kind_of(&x.name) == kind)
                    && x.size.is_none_or(|s| {
                        larger.is_none_or(|l| s > l) && smaller.is_none_or(|m| s < m)
                    })
            })
            .map(|(i, _)| i)
            .collect();
        if let Some((col, asc)) = self.sort {
            match col {
                Col::Name => rows.sort_by_cached_key(|&i| found[i].name.to_lowercase()),
                Col::Address => rows.sort_by_cached_key(|&i| found[i].url.to_lowercase()),
                Col::Type => rows.sort_by_cached_key(|&i| {
                    (
                        mdm_sitegrab::filter::extension(&found[i].name).unwrap_or_default(),
                        found[i].name.to_lowercase(),
                    )
                }),
                Col::Size => rows.sort_by_key(|&i| found[i].size),
            }
            if !asc {
                rows.reverse();
            }
        }
        self.order = Some(rows.clone());
        rows
    }

    fn files_page(&mut self, ui: &mut Ui, o: Pos2, queues: &[String]) {
        let p = ui.painter().clone();
        // i18n: Site grabber: label of the box that filters the files found by name or address
        let filter_label = crate::i18n::t("Filter:");
        // i18n: Site grabber: label of the box that shows only the files found bigger than this many kilobytes
        let larger_label = crate::i18n::t("Larger than (KB):");
        // i18n: Site grabber: label of the box that shows only the files found smaller than this many kilobytes
        let smaller_label = crate::i18n::t("Smaller than (KB):");
        let fx = winui::label_column(&p, 14.0, &[filter_label, larger_label], 60.0);
        winui::text_right(&p, o.x + fx - 7.0, o.y + 80.0, filter_label, LIGHT_TEXT());
        // The sizes shown: more than one, less than the other (KB).
        winui::text_right(&p, o.x + fx - 7.0, o.y + 108.0, larger_label, LIGHT_TEXT());
        let (larger, smaller) = (self.larger.clone(), self.smaller.clone());
        winui::edit_light(
            ui,
            r(o, fx, 94.0, 70.0, 23.0),
            &mut self.larger,
            true,
            false,
        );
        let sx = fx + 70.0 + 14.0;
        winui::text(&p, o.x + sx, o.y + 108.0, smaller_label, LIGHT_TEXT());
        let sbox = sx + winui::text_width(&p, smaller_label) + 6.0;
        winui::edit_light(
            ui,
            r(o, sbox, 94.0, 70.0, 23.0),
            &mut self.smaller,
            true,
            false,
        );
        if self.larger != larger || self.smaller != smaller {
            self.rows = Default::default();
            self.order = None;
        }
        let before = self.filter.clone();
        winui::edit_light(
            ui,
            r(o, fx, 66.0, 150.0, 23.0),
            &mut self.filter,
            true,
            false,
        );
        if self.filter != before {
            self.rows = Default::default();
            self.order = None;
        }
        // The type shown: every type, one group, or the others.
        // i18n: Site grabber: label of the drop-down that shows the files found of one type only
        let type_label = crate::i18n::t("Type:");
        let tx = fx + 150.0 + 14.0;
        winui::text(&p, o.x + tx, o.y + 80.0, type_label, LIGHT_TEXT());
        let cx = tx + winui::text_width(&p, type_label) + 6.0;
        let mut kinds: Vec<&str> = vec![
            // i18n: Site grabber: the Type drop-down's choice that shows every file found
            crate::i18n::t("All types"),
        ];
        kinds.extend(KINDS.iter().map(|g| crate::i18n::t(group_name(*g))));
        // i18n: Site grabber: the Type drop-down's choice for the files of no type listed
        kinds.push(crate::i18n::t("Other files"));
        if let Some(k) = winui::combo_light(
            ui,
            r(o, cx, 66.0, 150.0, 23.0),
            &kinds,
            self.kind,
            true,
            "sitegrab-kind",
        ) {
            self.kind = k;
            self.rows = Default::default();
            self.order = None;
        }
        let shown = self.shown();
        let ticked = self.checked.iter().filter(|c| **c).count();
        let count = crate::i18n::fmt(
            // i18n: Site grabber: how many files are ticked, of how many found
            crate::i18n::t("{n} of {total} chosen"),
            &[
                ("n", &crate::i18n::integer(ticked as u64)),
                (
                    "total",
                    &crate::i18n::integer(self.project.found.len() as u64),
                ),
            ],
        );
        let (rc, cut) = winui::text_right_within(
            &p,
            o.x + RIGHT,
            o.y + 108.0,
            &count,
            LIGHT_TEXT(),
            o.x + sbox + 78.0,
        );
        if cut {
            winui::full_on_hover(ui, rc, "sitegrab-chosen", &count);
        }

        self.list(
            ui,
            Rect::from_min_size(o + vec2(14.0, 126.0), vec2(W - 28.0, 234.0)),
            &shown,
        );

        let [all_at, none_at] = winui::buttons_from_left(
            &p,
            14.0,
            8.0,
            &[("Select all", 90.0), ("Select none", 90.0)],
            150.0,
        )[..] else {
            unreachable!("two buttons")
        };
        let all = winui::button_light(
            ui,
            r(o, all_at.0, 368.0, all_at.1, 21.0),
            // i18n: Site grabber button: tick every file the filter shows
            "Select all",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        let none = winui::button_light(
            ui,
            r(o, none_at.0, 368.0, none_at.1, 21.0),
            // i18n: Site grabber button: untick every file the filter shows
            "Select none",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        if all || none {
            for &i in &shown {
                self.checked[i] = all;
            }
        }
        // The queue they join, and whether it starts.
        // i18n: Site grabber: label of the queue the files are added to
        let queue_label = crate::i18n::t("Add to queue:");
        let cx = RIGHT - 190.0;
        winui::text_right_within(
            &p,
            o.x + cx - 7.0,
            o.y + 384.0,
            queue_label,
            LIGHT_TEXT(),
            o.x + none_at.0 + none_at.1 + 8.0,
        );
        let names: Vec<&str> = queues.iter().map(String::as_str).collect();
        let q = self.project.queue.min(names.len().saturating_sub(1));
        if !names.is_empty()
            && let Some(i) = winui::combo_light(
                ui,
                r(o, cx, 368.0, 190.0, 23.0),
                &names,
                q,
                true,
                "sitegrab-queue",
            )
        {
            self.project.queue = i;
        }
        winui::checkbox_light(
            ui,
            o + vec2(cx, 400.0),
            &mut self.project.start_now,
            // i18n: Site grabber: check box: start the queue once the files are added
            "Start the queue now",
            true,
        );
    }

    fn list(&mut self, ui: &mut Ui, list: Rect, shown: &[usize]) {
        let p = ui.painter().clone();
        winui::solid(&p, winui::rect(list), winui::FIELD_EDGE());
        let inner = list.shrink(1.0);
        p.rect_filled(winui::rect(inner), 0.0, winui::FIELD());
        let head = Rect::from_min_size(inner.min, vec2(inner.width(), 20.0));
        p.rect_filled(winui::rect(head), 0.0, LIST_HEAD());
        winui::fill_l(
            &p,
            head.left(),
            head.bottom() - 1.0,
            head.right() - 1.0,
            head.bottom() - 1.0,
            LIST_LINE(),
        );
        let size_right = head.right() - 12.0;
        let type_x = head.right() - 120.0;
        let addr_x = head.left() + 200.0;
        // The columns' titles: a click sorts by it, a second the other way.
        let titles = [
            // i18n: column title: file name
            (
                Col::Name,
                crate::i18n::t("Name"),
                head.left() + 25.0,
                addr_x,
            ),
            // i18n: column title: Site grabber: the web address a file was found at
            (Col::Address, crate::i18n::t("Address"), addr_x, type_x),
            // i18n: column title: Site grabber: a file's type (its extension: JPG, PDF…)
            (Col::Type, crate::i18n::t("Type"), type_x, size_right - 60.0),
            // i18n: column title: file size
            (
                Col::Size,
                crate::i18n::t("Size"),
                size_right - 60.0,
                head.right(),
            ),
        ];
        for (col, title, x0, x1) in titles {
            let arrow = match self.sort {
                Some((c, true)) if c == col => " \u{25b4}",
                Some((c, false)) if c == col => " \u{25be}",
                _ => "",
            };
            let shown_title = format!("{title}{arrow}");
            if col == Col::Size {
                winui::text_right(
                    &p,
                    size_right,
                    head.top() + 14.0,
                    &shown_title,
                    winui::TEXT(),
                );
            } else {
                winui::fit(
                    ui,
                    &p,
                    x0,
                    head.top() + 14.0,
                    &shown_title,
                    winui::TEXT(),
                    x1 - 6.0,
                    ("sitegrab-col", col as u8),
                );
            }
            let hit = Rect::from_min_max(pos2(x0 - 4.0, head.top()), pos2(x1 - 2.0, head.bottom()));
            let resp = ui
                .interact(
                    winui::rect(hit),
                    ui.scope_id().with(("sitegrab-sort", col as u8)),
                    egui::Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            winui::expose(&resp, egui::Role::ColumnHeader, title, true, None);
            if resp.clicked() {
                self.sort = match self.sort {
                    Some((c, asc)) if c == col => Some((col, !asc)),
                    _ => Some((col, true)),
                };
                self.order = None;
            }
        }
        let body = Rect::from_min_max(pos2(inner.left(), head.bottom()), inner.max);
        if shown.is_empty() {
            let note = if self.project.found.is_empty() {
                // i18n: Site grabber: the list of files found, when it found none
                crate::i18n::t(
                    "No files were found. Go back to explore further or collect other types.",
                )
            } else {
                // i18n: Site grabber: the list of files found, when the filter hides them all
                crate::i18n::t("No file matches the filter.")
            };
            let w = winui::text_width(&p, note).min(body.width() - 20.0);
            winui::fit(
                ui,
                &p,
                body.center().x - w / 2.0,
                body.center().y + 4.0,
                note,
                winui::MUTED(),
                body.right() - 10.0,
                "sitegrab-empty",
            );
            return;
        }
        // i18n: what a screen reader calls Site grabber's list of files found
        self.rows.name = crate::i18n::t("Files");
        let (found, checked, known) = (&self.project.found, &self.checked, &self.known);
        let mut toggle = None;
        let ev = winui::row_list(
            ui,
            "sitegrab-files",
            winui::rect(body.expand(1.0)),
            shown.len(),
            ROW,
            &mut self.rows,
            |ui, clip, k, row, look| {
                let i = shown[k];
                let f = &found[i];
                let top = row.top();
                let glyph = pos2(row.left() + 6.0, top + 2.0);
                let mut on = checked[i];
                if winui::row_check(ui, clip, glyph, &mut on, &f.name, ("sitegrab-row", i)) {
                    toggle = Some(i);
                }
                let ink = if known.get(i).copied().unwrap_or(false) {
                    winui::MUTED()
                } else {
                    look.ink
                };
                crate::fileicons::paint_at(
                    ui,
                    clip,
                    crate::fileicons::Kind::File(&f.name),
                    pos2(glyph.x + 19.0, top + 1.0),
                    16.0,
                );
                let left = glyph.x + 19.0 + crate::fileicons::INDENT;
                let name = crate::i18n::isolate(&f.name);
                winui::fit(
                    ui,
                    clip,
                    left,
                    top + 13.0,
                    &name,
                    ink,
                    addr_x - 8.0,
                    ("sitegrab-name", i),
                );
                // The address without its scheme, cut in the middle (its end tells files of one
                // name apart: many pages are index.html).
                let address = f
                    .url
                    .split_once("://")
                    .map_or(f.url.as_str(), |(_, rest)| rest);
                winui::fit_middle(
                    ui,
                    clip,
                    addr_x,
                    top + 13.0,
                    &crate::i18n::ltr(address),
                    winui::MUTED(),
                    type_x - 8.0,
                    ("sitegrab-address", i),
                );
                let kind = mdm_sitegrab::filter::extension(&f.name)
                    .map(|e| e.to_uppercase())
                    .unwrap_or_default();
                text(clip, type_x, top + 13.0, &kind, ink);
                let size = f.size.map(crate::model::size).unwrap_or_default();
                winui::text_right(clip, size_right, top + 13.0, &size, ink);
                look.name = if size.is_empty() {
                    f.name.clone()
                } else {
                    crate::regional::list(&[&f.name, &size])
                };
            },
        );
        if let Some(k) = ev.space.or(ev.double_clicked) {
            toggle = shown.get(k).copied();
        }
        if let Some(i) = toggle {
            self.checked[i] = !self.checked[i];
        }
    }

    // ---- the buttons ------------------------------------------------------------------------

    fn buttons(&mut self, ui: &mut Ui, o: Pos2) -> Outcome<Ask> {
        let p = ui.painter().clone();
        let y = BUTTONS_Y;
        let back_w = winui::button_width(&p, "< Back", 73.0);
        let back_on = match self.page {
            Page::Start => false,
            Page::Run => !self.running,
            _ => true,
        };
        let back = winui::button_light(
            ui,
            r(o, 14.0, y, back_w, 21.0),
            // i18n: Site grabber button: the wizard's previous page
            "< Back",
            if back_on {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        )
        .clicked()
            && back_on;
        if back {
            self.error = None;
            self.page = match self.page {
                Page::Save => Page::Start,
                Page::Explore => Page::Save,
                // From the results or a failure: the settings, to change them and start again.
                _ => Page::Explore,
            };
            return Outcome::Open;
        }
        match self.page {
            Page::Start | Page::Save | Page::Explore => {
                let last = self.page == Page::Explore;
                let go = if last {
                    // i18n: Site grabber button: start exploring the site
                    crate::i18n::mark("Start")
                } else {
                    // i18n: Site grabber button: the wizard's next page
                    crate::i18n::mark("Next >")
                };
                let at =
                    winui::buttons_to_right(&p, RIGHT, 8.0, &[(go, 73.0), ("Cancel", 73.0)], 180.0);
                let next = winui::button_light(
                    ui,
                    r(o, at[0].0, y, at[0].1, 21.0),
                    go,
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, at[1].0, y, at[1].1, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if cancel || winui::escape_pressed(ui) {
                    return Outcome::Closed;
                }
                if next || winui::enter_pressed(ui) {
                    return self.next();
                }
                Outcome::Open
            }
            Page::Run => {
                let running = self.running;
                let at = winui::buttons_to_right(
                    &p,
                    RIGHT,
                    8.0,
                    &[("Stop", 73.0), ("Close", 73.0)],
                    180.0,
                );
                let stop = winui::button_light(
                    ui,
                    r(o, at[0].0, y, at[0].1, 21.0),
                    // i18n: Site grabber button: stop exploring (what was found stays)
                    "Stop",
                    if running {
                        ButtonKind::Default
                    } else {
                        ButtonKind::Disabled
                    },
                    false,
                )
                .clicked();
                let close = winui::button_light(
                    ui,
                    r(o, at[1].0, y, at[1].1, 21.0),
                    "Close",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if stop && running {
                    return Outcome::Done(Ask::Stop);
                }
                if close || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else {
                    Outcome::Open
                }
            }
            Page::Files => {
                // Continue: on from where the last run stopped (it ended early).
                if self.project.can_continue() {
                    let x = 14.0 + back_w + 8.0;
                    // i18n: Site grabber button: explore further, from where the last run stopped
                    let label = "Continue";
                    let w = winui::button_width(&p, label, 73.0);
                    if winui::button_light(
                        ui,
                        r(o, x, y, w, 21.0),
                        label,
                        ButtonKind::Normal,
                        false,
                    )
                    .clicked()
                    {
                        self.error = None;
                        return Outcome::Done(Ask::Continue);
                    }
                }
                let any = self.checked.iter().any(|c| *c);
                // "All" is every file not in the download list already (those are greyed): a
                // project's files opened again never add a second copy of what it added.
                let known = |i: usize| self.known.get(i).copied().unwrap_or(false);
                let some = (0..self.project.found.len()).any(|i| !known(i));
                let at = winui::buttons_to_right(
                    &p,
                    RIGHT,
                    8.0,
                    &[
                        ("Download selected", 73.0),
                        ("Download all", 73.0),
                        ("Close", 73.0),
                    ],
                    200.0,
                );
                let kind = |on: bool, default: bool| match (on, default) {
                    (false, _) => ButtonKind::Disabled,
                    (true, true) => ButtonKind::Default,
                    (true, false) => ButtonKind::Normal,
                };
                let selected = winui::button_light(
                    ui,
                    r(o, at[0].0, y, at[0].1, 21.0),
                    // i18n: Site grabber button: add the ticked files to the download list
                    "Download selected",
                    kind(any, true),
                    false,
                )
                .clicked()
                    && any;
                let all = winui::button_light(
                    ui,
                    r(o, at[1].0, y, at[1].1, 21.0),
                    // i18n: Site grabber button: add every file found to the download list
                    "Download all",
                    kind(some, false),
                    false,
                )
                .clicked()
                    && some;
                let close = winui::button_light(
                    ui,
                    r(o, at[2].0, y, at[2].1, 21.0),
                    "Close",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if close || winui::escape_pressed(ui) {
                    return Outcome::Closed;
                }
                // Not on Enter: it may be the filter's.
                if selected || all {
                    let files = self
                        .project
                        .found
                        .iter()
                        .enumerate()
                        .filter(|&(i, _)| {
                            if all {
                                !known(i)
                            } else {
                                self.checked.get(i).copied().unwrap_or(false)
                            }
                        })
                        .map(|(_, f)| f.clone())
                        .collect();
                    return Outcome::Done(Ask::Download {
                        files,
                        queue: self.project.queue,
                        start: self.project.start_now,
                    });
                }
                Outcome::Open
            }
        }
    }

    /// Next (or Start on the last settings page): the page's own check first.
    fn next(&mut self) -> Outcome<Ask> {
        match self.page {
            Page::Start => {
                let Some(url) = mdm_sitegrab::start_url(&self.project.spec.start) else {
                    self.error = Some(bad_address());
                    return Outcome::Open;
                };
                if self.project.name.trim().is_empty() {
                    self.project.name = url.host_str().unwrap_or_default().to_owned();
                }
                self.error = None;
                self.page = Page::Save;
                Outcome::Open
            }
            Page::Save => {
                self.page = Page::Explore;
                Outcome::Open
            }
            Page::Explore => {
                if self.project().spec.types().is_empty() {
                    self.error = Some(no_types());
                    return Outcome::Open;
                }
                self.error = None;
                Outcome::Done(Ask::Crawl)
            }
            Page::Run | Page::Files => Outcome::Open,
        }
    }
}

/// Where file `name` is in the Type drop-down: 1 + its group's place in `KINDS`, or the last
/// (other files).
fn kind_of(name: &str) -> usize {
    mdm_sitegrab::filter::extension(name)
        .and_then(|e| Group::of(&e))
        .and_then(|g| KINDS.iter().position(|k| *k == g))
        .map_or(KINDS.len() + 1, |i| i + 1)
}

/// A file type group's name (as the Explore page's check boxes call it).
fn group_name(g: Group) -> &'static str {
    match g {
        Group::Images => "Images",
        Group::Video => "Video",
        Group::Audio => "Audio",
        Group::Documents => "Documents",
        Group::Archives => "Archives",
        Group::Programs => "Programs",
        Group::Pages => "Web pages",
        Group::Assets => "Styles, scripts, fonts",
    }
}

fn bad_address() -> &'static str {
    // i18n: Site grabber: the start address isn't a web address
    crate::i18n::mark("Type the address of the page to start from (https://…).")
}

fn no_types() -> &'static str {
    // i18n: Site grabber: no file type ticked and none typed
    crate::i18n::mark("Choose at least one type of file to collect.")
}

/// A template's name in the drop-down.
fn template_name(t: Template) -> &'static str {
    match t {
        // i18n: Site grabber template: every picture of the site
        Template::Images => crate::i18n::t("All images"),
        // i18n: Site grabber template: every sound file of the site
        Template::Audio => crate::i18n::t("All audio"),
        // i18n: Site grabber template: every video file of the site
        Template::Video => crate::i18n::t("All video"),
        // i18n: Site grabber template: every document of the site (PDF, Word…)
        Template::Documents => crate::i18n::t("All documents"),
        // i18n: Site grabber template: the pages and what they need, to read the site without a connection
        Template::Offline => crate::i18n::t("Whole site for offline reading"),
        // i18n: Site grabber template: the user's own settings
        Template::Custom => crate::i18n::t("Custom"),
    }
}

/// What a template collects, under the drop-down.
fn template_about(t: Template) -> &'static str {
    match t {
        // i18n: Site grabber: what the All images template collects
        Template::Images => crate::i18n::t(
            "Pictures (JPG, PNG, GIF, WebP and more) on the pages up to two links from the start page.",
        ),
        // i18n: Site grabber: what the All audio template collects
        Template::Audio => crate::i18n::t(
            "Sound files (MP3, M4A, FLAC and more) on the pages up to two links from the start page.",
        ),
        // i18n: Site grabber: what the All video template collects
        Template::Video => crate::i18n::t(
            "Video files (MP4, MKV, WebM and more) on the pages up to two links from the start page.",
        ),
        // i18n: Site grabber: what the All documents template collects
        Template::Documents => crate::i18n::t(
            "Documents (PDF, Word, Excel, text and more) on the pages up to two links from the start page.",
        ),
        // i18n: Site grabber: what the Whole site template collects
        Template::Offline => crate::i18n::t(
            "The pages, three links deep, with their pictures, styles, scripts and media, saved in the site's folders with their links changed to open the saved copies: the site reads offline.",
        ),
        // i18n: Site grabber: what the Custom template collects
        Template::Custom => {
            crate::i18n::t("The file types and settings you choose on the next pages.")
        }
    }
}

/// How a grab ended, in a line.
fn end_text(end: &End) -> String {
    match end {
        // i18n: Site grabber: it explored every page within reach
        End::Finished => crate::i18n::t("Done: every page within reach was explored."),
        // i18n: Site grabber: it stopped at the most pages it may explore
        End::MaxPages => crate::i18n::t("Stopped at the most pages to explore."),
        // i18n: Site grabber: it stopped at the most files it may collect
        End::MaxFiles => crate::i18n::t("Stopped at the most files to collect."),
        // i18n: Site grabber: Stop was clicked
        End::Stopped => crate::i18n::t("Stopped. What was found so far is kept."),
        // i18n: Site grabber: the start page gave nothing
        End::Failed(_) => crate::i18n::t("The start page could not be read."),
    }
    .to_owned()
}

/// Why the start page gave nothing.
fn fault_text(fault: &Fault) -> String {
    match fault {
        Fault::Status(code) => crate::i18n::fmt(
            // i18n: Site grabber: the server answered the start page with an error; {status} is its number (404)
            crate::i18n::t("The server answered with error {status}. Check the start address."),
            &[("status", &code.to_string())],
        ),
        Fault::Network(e) => {
            tracing::info!("site grabber: the start page failed: {e}");
            // i18n: Site grabber: the start page's server didn't answer
            crate::i18n::t(
                "The server didn't answer. Check the address and your connection (and the proxy in Options).",
            )
            .to_owned()
        }
        // i18n: Site grabber: the start page is one robots.txt keeps robots from
        Fault::Robots => crate::i18n::t(
            "The site's robots.txt asks robots not to visit the start page. Untick Follow the site's robots.txt to grab it anyway.",
        )
        .to_owned(),
        // i18n: Site grabber: the start address leads to a file, not a page
        Fault::NotPage => crate::i18n::t(
            "The start address is a file, not a page, and not of a type being collected.",
        )
        .to_owned(),
    }
}

/// A group box with its caption in the top line (as Options draws them).
fn group(ui: &Ui, o: Pos2, x: f32, y: f32, w: f32, h: f32, caption: &str) {
    let p = ui.painter();
    let full = crate::i18n::t(caption);
    let caption = winui::ellipsize(p, &winui::tahoma(), full, w - 18.0);
    let cw = winui::text_width(p, &caption);
    winui::group_box(
        p,
        r(o, x, y, w, h),
        Some((o.x + x + 7.0, o.x + x + 9.0 + cw + 2.0)),
    );
    let rc = text(p, o.x + x + 9.0, o.y + y + 4.0, &caption, LIGHT_TEXT());
    if caption != full {
        winui::full_on_hover(ui, rc, ("sitegrab-group", caption.as_str()), full);
    }
}

// ---- what the rest of the app shows of the grabber ----------------------------------------------

/// The Categories tree's node of the projects.
pub fn tree_label() -> &'static str {
    // i18n: the Categories tree's node that lists the Site grabber's projects
    "Grabber projects"
}

/// A project's name in the Categories tree and the Tasks menu while it explores its site.
pub fn exploring_label(name: &str) -> String {
    crate::i18n::fmt(
        // i18n: a grabber project in the Categories tree and the Tasks menu while it explores its site; {name} is the project's name
        crate::i18n::t("{name} (exploring)"),
        &[("name", &crate::i18n::isolate(name))],
    )
}

/// What a download added by the grabber says it came from ("Added {source}").
pub fn added_by() -> &'static str {
    // i18n: a download's log: "Added by Site grabber"
    crate::i18n::mark("by Site grabber")
}

/// What a project's right-click menu offers.
pub struct MenuState {
    /// It is exploring its site.
    pub running: bool,
    /// Its last run stopped early and can go on.
    pub can_continue: bool,
    /// Its pages are made to work offline.
    pub offline: bool,
    /// Its offline copy's start page is downloaded.
    pub copy_ready: bool,
}

/// A project's right-click command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAct {
    Open,
    Continue,
    Run,
    Edit,
    OpenCopy,
    Delete,
}

/// A project's right-click menu in the Categories tree; `cmd` makes each command for it.
pub fn project_menu<C>(cmd: impl Fn(MenuAct) -> C, s: MenuState) -> Vec<MenuEntry<C>> {
    use MenuAct as A;
    use MenuEntry::{Item as I, Sep};
    let mut menu = vec![
        // i18n: right-click on a grabber project: its window (its exploring, or the files it found)
        I("Open", cmd(A::Open), true, false),
        // i18n: right-click on a grabber project: explore further, from where its last run stopped
        I(
            "Continue exploring",
            cmd(A::Continue),
            s.can_continue,
            false,
        ),
        // i18n: right-click on a grabber project: explore its site again with its settings
        I("Run again", cmd(A::Run), !s.running, false),
        // i18n: right-click on a grabber project: open its settings in Site grabber
        I("Edit...", cmd(A::Edit), !s.running, false),
    ];
    if s.offline {
        // i18n: right-click on a grabber project whose pages work offline: open its saved start page in the browser
        menu.push(I(
            "Open offline copy",
            cmd(A::OpenCopy),
            s.copy_ready,
            false,
        ));
    }
    menu.push(Sep);
    // i18n: right-click on a grabber project: delete the project (its downloads stay)
    menu.push(I("Delete", cmd(A::Delete), true, false));
    menu
}

/// A project's node when none of its files is downloaded yet (or none is left): the empty
/// list's title, its reason, and its link (which opens the project).
pub fn empty_project(running: bool) -> (&'static str, &'static str, &'static str) {
    (
        // i18n: title in the empty download list of a grabber project's node
        "Nothing downloaded from this project yet",
        if running {
            // i18n: in the empty download list of a grabber project's node, while it explores its site
            "It is exploring its site now. Open it to see how far it is."
        } else {
            // i18n: in the empty download list of a grabber project's node
            "Open the project to see the files it found and download them."
        },
        // i18n: link in the empty download list of a grabber project's node: opens the project's window
        "Open the project",
    )
}

/// An empty list (All Downloads, a category) while a grabber project keeps its downloads under
/// its own node: the title, the reason, and the link that shows them.
pub fn empty_apart() -> (&'static str, &'static str, &'static str) {
    (
        // i18n: title in the empty download list (All Downloads or a category) while Site grabber projects keep their downloads apart
        "No downloads here",
        // i18n: in the empty download list, under its title, while Site grabber projects keep their downloads apart
        "Site grabber's downloads show under Grabber projects, in the Categories pane.",
        // i18n: link in the empty download list: shows the latest grabber project's downloads
        "Show the grabber's downloads",
    )
}

/// The question before a project is deleted.
pub fn delete_question(name: &str) -> Confirm {
    let text = crate::i18n::fmt(
        // i18n: asked before a grabber project is deleted; {name} is its name
        crate::i18n::t(
            "Delete the grabber project \"{name}\"? The files it downloaded stay in the list and on the disk.",
        ),
        &[("name", &crate::i18n::isolate(name))],
    );
    // i18n: title of the question asked before a grabber project is deleted
    Confirm::new(crate::i18n::mark("Delete grabber project"), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_check_what_they_are_given() {
        let mut d = SiteGrabDlg::new(Project::default(), String::new());
        assert!(matches!(d.next(), Outcome::Open));
        assert_eq!(d.page, Page::Start, "no address: it stays");
        assert_eq!(d.error, Some(bad_address()));
        d.project.spec.start = "example.com/photos".into();
        assert!(matches!(d.next(), Outcome::Open));
        assert_eq!(d.page, Page::Save);
        assert_eq!(d.project.name, "example.com", "named after its host");
        assert!(matches!(d.next(), Outcome::Open));
        assert_eq!(d.page, Page::Explore);
        d.project.spec.groups.clear();
        assert!(matches!(d.next(), Outcome::Open));
        assert_eq!(d.error, Some(no_types()));
        d.project.spec.custom = "psd".into();
        assert!(matches!(d.next(), Outcome::Done(Ask::Crawl)));
    }

    #[test]
    fn number_boxes_are_read_into_their_ranges_and_templates_set_them() {
        let mut d = SiteGrabDlg::new(Project::default(), "C:\\Downloads".into());
        d.depth = "99".into();
        d.connections = "0".into();
        d.max_pages = String::new();
        let p = d.project();
        assert_eq!(p.spec.depth, mdm_sitegrab::MAX_DEPTH);
        assert_eq!(p.spec.connections, 1);
        assert_eq!(p.spec.max_pages, 1);
        d.set_template(Template::Offline);
        assert_eq!(d.depth, "3");
        let p = d.project();
        assert!(p.spec.groups.contains(&Group::Pages));
        assert_eq!(p.template, Template::Offline);
        // A site to read offline: its pages work offline, in one folder (General's when none
        // was typed), its sizes not asked first.
        assert!(p.offline && p.keep_folders && !p.spec.sizes);
        assert_eq!(p.folder, "C:\\Downloads");
        d.set_template(Template::Images);
        assert!(!d.project().offline);
        assert!(d.project().spec.sizes);
    }

    #[test]
    fn the_filter_types_order_and_known_files() {
        let mut d = SiteGrabDlg::sample();
        assert_eq!(d.shown().len(), 6);
        d.filter = "HARBOUR".into();
        d.order = None;
        assert_eq!(d.shown(), [0, 1]);
        d.filter = "2025/".into();
        d.order = None;
        assert_eq!(d.shown(), [3, 4], "the address counts too");
        d.filter.clear();
        // Pictures only: the SVG is one too.
        d.kind = 1;
        d.order = None;
        assert_eq!(d.shown().len(), 6);
        d.kind = KINDS.len() + 1;
        d.order = None;
        assert!(d.shown().is_empty(), "no file of another type");
        d.kind = 0;
        // By size: one of no known size is shown whatever is typed.
        d.larger = "1000".into();
        d.order = None;
        assert_eq!(d.shown(), [0, 1, 3, 4]);
        d.smaller = "2048".into();
        d.order = None;
        assert_eq!(d.shown(), [0, 4]);
        d.larger.clear();
        d.smaller = "x".into();
        d.order = None;
        assert_eq!(d.shown().len(), 6, "not a number: no limit");
        d.smaller.clear();
        d.sort = Some((Col::Size, false));
        d.order = None;
        assert_eq!(d.shown()[0], 1, "the biggest first");
        assert_eq!(*d.shown().last().unwrap(), 4, "no size last");
        d.sort = Some((Col::Name, true));
        d.order = None;
        assert_eq!(d.shown()[0], 1, "harbour-wide.jpg");
        d.mark_known(|u| u.ends_with("harbour.jpg"));
        assert!(!d.checked[0] && d.checked[1]);
    }

    #[test]
    fn a_grab_that_ended_lists_its_files_and_one_that_failed_says_why() {
        let mut d = SiteGrabDlg::new(Project::default(), String::new());
        d.run_started();
        assert!(d.running && d.page == Page::Run);
        let mut p = SiteGrabDlg::sample().project();
        p.ended = Some(End::MaxPages);
        p.frontier = Some(Default::default());
        d.crawl_ended(p.clone(), |u| u.ends_with("logo.svg"));
        assert_eq!(d.page, Page::Files);
        assert!(!d.running);
        assert_eq!(d.checked.iter().filter(|c| **c).count(), 5);
        assert!(d.project.can_continue());
        let mut d = SiteGrabDlg::new(Project::default(), String::new());
        d.run_started();
        p.ended = Some(End::Failed(Fault::Status(404)));
        d.crawl_ended(p, |_| false);
        assert_eq!(d.page, Page::Run, "the reason stays in sight");
    }

    #[test]
    fn the_project_menu_offers_what_applies() {
        let menu = project_menu(
            |a| a,
            MenuState {
                running: false,
                can_continue: true,
                offline: true,
                copy_ready: false,
            },
        );
        let labels: Vec<(&str, bool)> = menu
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(l, _, on, _) => Some((*l, *on)),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                ("Open", true),
                ("Continue exploring", true),
                ("Run again", true),
                ("Edit...", true),
                ("Open offline copy", false),
                ("Delete", true)
            ]
        );
    }
}
