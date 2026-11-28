//! Metalinks (ours; IDM reads none): a `.metalink` (Metalink 3) or `.meta4` (Metalink 4,
//! RFC 5854) lists files, each with the servers that have it (their priority and country) and
//! its hashes. From an address (Add URL, the browser, the command line, a dropped link)
//! Download File Info reads it (`dialogs::FileInfo`); from a file (dropped, Tasks → Open
//! Metalink file..., the command line) the app reads it beside the window (`read_file`). A
//! Metalink of one file goes on in Download File Info, as an address does; one of several
//! opens Add Metalink, this window, in the style of Add torrent and the Link grabber: its files
//! with their sizes, sources and checks, ticked, the category and folder they go to, Download
//! Now / Later / Cancel. Either way each file becomes a download from its first source, the
//! others its mirrors (`Item::mirrors`, by priority, then the user's country first), its hashes
//! checked (`Item::checksum`, `Item::pieces`), going direct (never through a debrid account:
//! these are the publisher's own addresses).

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Ui, pos2, vec2};
use mdm_engine::metalink::{File, Manifest, MetalinkError};
use url::Url;

use crate::dialogs::{Outcome, window};
use crate::model::{Category, category_folder};
use crate::winui::{self, ButtonKind, fill_l as fill, r, text};

const ROW: f32 = 18.0;

crate::theme::themed! {
    // The lists' header colour (winui::LIST_HEADER), as Add torrent's file list.
    LIST_HEAD => header;
    LIST_LINE => table_line;
}

/// Whether a web address is a Metalink's (Add URL, the browser): its path ends in `.metalink`
/// or `.meta4`, or the browser named the file so or gave its type.
pub fn is_metalink_url(url: &Url, name: Option<&str>, mime: Option<&str>) -> bool {
    let ends = |s: &str| {
        let s = s.to_ascii_lowercase();
        s.ends_with(".metalink") || s.ends_with(".meta4")
    };
    matches!(url.scheme(), "http" | "https")
        && (ends(url.path())
            || name.is_some_and(ends)
            || mime.is_some_and(|m| {
                m.split(';').next().is_some_and(|m| {
                    let m = m.trim();
                    m.eq_ignore_ascii_case("application/metalink4+xml")
                        || m.eq_ignore_ascii_case("application/metalink+xml")
                })
            }))
}

/// A command's address as a Metalink file's path (a command line, a file association, a
/// drop): one ending in `.metalink` or `.meta4` that is no web or FTP address (`file:` ones
/// become their path), its quotes taken off. Whether it is a file is the disk's to say.
pub fn metalink_path(u: &str) -> Option<PathBuf> {
    let s = u.trim().trim_matches('"');
    let lower = s.to_ascii_lowercase();
    if !(lower.ends_with(".metalink") || lower.ends_with(".meta4")) {
        return None;
    }
    match Url::parse(s) {
        Ok(url) if url.scheme() == "file" => url.to_file_path().ok(),
        Ok(url) if mdm_engine::supported(&url) => None,
        _ => Some(PathBuf::from(s)),
    }
}

/// Why a Metalink can't be used, in the interface language.
pub fn describe(e: &MetalinkError) -> String {
    use crate::i18n::{fmt, isolate, ltr, t};
    match e {
        MetalinkError::TooLarge => fmt(
            // i18n: why a Metalink (a list of files and their download addresses) can't be used; {size} = its size limit, "16 MiB"
            t("the Metalink is larger than {size}"),
            &[(
                "size",
                &crate::model::size(mdm_engine::metalink::MAX_MANIFEST),
            )],
        ),
        // i18n: why a Metalink (a list of files and their download addresses) can't be used: its bytes aren't text
        MetalinkError::NotText => t("the Metalink is not UTF-8 text").to_owned(),
        MetalinkError::InvalidXml(detail) => fmt(
            // i18n: why a Metalink can't be used: it isn't well-formed XML; {detail} = the XML reader's words, in English
            t("the Metalink is not valid XML ({detail})"),
            &[("detail", &ltr(detail))],
        ),
        // i18n: why a Metalink can't be used: it lists no file
        MetalinkError::NoFiles => t("the Metalink lists no files").to_owned(),
        // i18n: why a Metalink can't be used: it lists more files than DDM takes (10,000)
        MetalinkError::TooManyFiles => t("the Metalink lists too many files").to_owned(),
        MetalinkError::NoSource { file } => fmt(
            // i18n: why a Metalink can't be used: one of its files has no address DDM downloads from; {file} = that file's name
            t("{file} in the Metalink has no HTTP, HTTPS or FTP address"),
            &[("file", &isolate(file))],
        ),
        MetalinkError::UnsafeName { name } => fmt(
            // i18n: why a Metalink is refused: a file's name in it would be saved outside the chosen folder (a "../" name, an absolute path); {name} = that name
            t("the Metalink names a file outside its folder: {name}"),
            &[("name", &isolate(name))],
        ),
    }
}

/// A Metalink file on this computer, read and checked (at most `MAX_MANIFEST`): its files, or
/// why it can't be used. Reads the disk: for a thread other than the window's (#743).
pub fn read_file(path: &Path) -> Result<Manifest, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(mdm_engine::metalink::MAX_MANIFEST + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| {
            crate::i18n::fmt(
                crate::i18n::t("The file could not be read: {error}"),
                &[("error", &e.to_string())],
            )
        })?;
    // Relative addresses in a file resolve against its own place, which no download takes:
    // only the absolute ones are sources.
    let base = Url::from_file_path(path)
        .ok()
        .or_else(|| Url::parse("file:///metalink").ok())
        .expect("a file URL");
    mdm_engine::metalink::parse_bytes(&bytes, &base).map_err(|e| describe(&e))
}

/// A Metalink read, and where from (its address or path, as shown).
pub struct Opened {
    pub manifest: Manifest,
    pub origin: String,
}

/// One file of a Metalink, ready to become a download.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    /// Where it goes under the chosen folder (its name's folders kept).
    pub path: PathBuf,
    pub size: Option<u64>,
    /// Its sources in the order to use them: the first is the download's address, the rest its
    /// mirrors.
    pub urls: Vec<Url>,
    pub checksum: Option<mdm_engine::verify::Hash>,
    pub pieces: Option<mdm_engine::verify::Pieces>,
}

impl Pick {
    /// File `file` of a Metalink, its sources ordered for the user in `country`.
    pub fn of(file: &File, country: Option<&str>) -> Pick {
        Pick {
            path: file
                .relative_path()
                .unwrap_or_else(|| PathBuf::from(file.display_name())),
            size: file.size,
            urls: file.ordered_urls(country),
            checksum: file.checksum.clone(),
            pieces: file.pieces.clone(),
        }
    }

    fn name(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }

    /// How it is checked, as the list shows it: its whole-file hash's algorithm, its pieces'.
    fn check(&self) -> String {
        match (&self.checksum, &self.pieces) {
            (Some(h), _) => h.algo.name().to_owned(),
            (None, Some(p)) => crate::i18n::fmt(
                // i18n: Add Metalink, the Check column: the file is checked piece by piece; {algorithm} = "SHA-1", "SHA-256"
                crate::i18n::t("{algorithm} pieces"),
                &[("algorithm", p.algo.name())],
            ),
            // i18n: Add Metalink, the Check column: no hash to check the file with
            (None, None) => crate::i18n::t("none").to_owned(),
        }
    }
}

/// The name of the Metalink at `origin` (its address or path): the address's last part, else
/// the file's name.
fn origin_name(origin: &str) -> String {
    let from_url = Url::parse(origin)
        .ok()
        .filter(mdm_engine::supported)
        .and_then(|u| {
            let last = u.path_segments()?.rfind(|s| !s.is_empty())?.to_owned();
            Some(
                percent_encoding_decode(&last)
                    .unwrap_or(last)
                    .trim()
                    .to_owned(),
            )
        });
    from_url
        .or_else(|| {
            Path::new(origin)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| origin.to_owned())
}

/// `s` with its %-escapes decoded, when they make UTF-8.
fn percent_encoding_decode(s: &str) -> Option<String> {
    url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
}

/// What Download Now / Later adds.
pub struct MetalinkAdd {
    pub files: Vec<Pick>,
    /// The folder chosen; each file goes to its path under it.
    pub dir: PathBuf,
    pub category: Category,
    pub start_now: bool,
}

/// The downloads `add` makes, one per file: its address, where it goes, its size, its mirrors
/// and hashes (`App::add_metalink`).
pub fn downloads(add: &MetalinkAdd) -> Vec<(Url, PathBuf, Option<u64>, Pick)> {
    add.files
        .iter()
        .filter_map(|p| {
            let url = p.urls.first()?.clone();
            Some((url, add.dir.join(&p.path), p.size, p.clone()))
        })
        .collect()
}

/// The category for a set of files: theirs when they all share one, else General.
pub fn category_of(files: &[Pick]) -> Category {
    let mut cats = files
        .iter()
        .map(|p| Category::of(&p.path.file_name().unwrap_or_default().to_string_lossy()));
    let first = cats.next().unwrap_or(Category::General);
    if cats.all(|c| c == first) {
        first
    } else {
        Category::General
    }
}

pub struct MetalinkDialog {
    origin: String,
    files: Vec<Pick>,
    chosen: Vec<bool>,
    rows: winui::RowList,
    category: Category,
    category_open: bool,
    folder: String,
    folder_edited: bool,
    downloads: PathBuf,
    category_dirs: Vec<(String, String)>,
    start_now: Option<bool>,
    /// Asking to create a missing folder.
    asking: crate::dialogs::FolderGate<()>,
}

impl MetalinkDialog {
    /// The window for `opened`'s files, their sources ordered for the user in `country`, all
    /// ticked, into the folder of the category they share.
    pub fn new(
        opened: Opened,
        country: Option<&str>,
        downloads: PathBuf,
        category_dirs: Vec<(String, String)>,
    ) -> Self {
        let files: Vec<Pick> = opened
            .manifest
            .files
            .iter()
            .map(|f| Pick::of(f, country))
            .collect();
        let category = category_of(&files);
        let folder = category_folder(category, &downloads, &category_dirs);
        MetalinkDialog {
            origin: opened.origin,
            chosen: vec![true; files.len()],
            files,
            rows: Default::default(),
            category,
            category_open: false,
            folder: folder.display().to_string(),
            folder_edited: false,
            downloads,
            category_dirs,
            start_now: None,
            asking: Default::default(),
        }
    }

    /// The folder they go to.
    pub fn save_folder(&self) -> PathBuf {
        crate::dialogs::resolve_path(&self.folder)
    }

    /// Another category: the folder follows it, unless one was typed or picked.
    fn set_category(&mut self, c: Category) {
        self.category = c;
        if !self.folder_edited {
            self.folder = category_folder(c, &self.downloads, &self.category_dirs)
                .display()
                .to_string();
        }
    }

    /// The files ticked: how many, and their known size.
    fn selection(&self) -> (usize, u64) {
        self.files
            .iter()
            .zip(&self.chosen)
            .filter(|(_, c)| **c)
            .fold((0, 0), |(n, b), (f, _)| (n + 1, b + f.size.unwrap_or(0)))
    }

    /// What Download Now (`now`) or Later adds: the ticked files.
    fn add(&self, now: bool) -> MetalinkAdd {
        MetalinkAdd {
            files: self
                .files
                .iter()
                .zip(&self.chosen)
                .filter(|(_, c)| **c)
                .map(|(f, _)| f.clone())
                .collect(),
            dir: self.save_folder(),
            category: self.category,
            start_now: now,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<MetalinkAdd> {
        let out = window(
            ctx,
            "add-metalink",
            "Add Metalink",
            [600.0, 470.0],
            false,
            winui::BG(),
            |ui, o| self.body(ui, o),
        );
        let folder = self.save_folder();
        match crate::dialogs::ensure_folder(ctx, &mut self.asking, out, &folder) {
            Outcome::Open => Outcome::Open,
            Outcome::Closed => Outcome::Closed,
            Outcome::Done(()) => Outcome::Done(self.add(self.start_now.unwrap_or(true))),
        }
    }

    fn body(&mut self, ui: &mut Ui, o: Pos2) -> Outcome<()> {
        let p = ui.painter().clone();
        let kind = |on: bool, default: bool| match (on, default) {
            (false, _) => ButtonKind::Disabled,
            (true, true) => ButtonKind::Default,
            (true, false) => ButtonKind::Normal,
        };
        // i18n: noun: label before the Metalink's address or file (a list of files and their download addresses)
        let source_label = crate::i18n::t("Metalink");
        let category_label = crate::i18n::t("Category");
        let folder_label = crate::i18n::t("Save to");
        let fx = winui::label_column(
            &p,
            11.0,
            &[source_label, category_label, folder_label],
            70.0,
        );
        // Where it came from: shown, copyable, not edited (another Metalink is opened anew).
        text(&p, o.x + 11.0, o.y + 23.0, source_label, winui::TEXT());
        let mut origin = self.origin.clone();
        let origin_rc = r(o, fx, 9.0, 589.0 - fx, 21.0);
        winui::edit(ui, origin_rc, &mut origin, false, false);
        winui::copyable(ui, origin_rc, "metalink-origin", &self.origin, false);

        // What it holds: its name, how many files, their size.
        let line = o.y + 50.0;
        let total: u64 = self.files.iter().filter_map(|f| f.size).sum();
        let name = origin_name(&self.origin);
        let summary = crate::i18n::fmt(
            crate::i18n::t("{name}  ({files}, {size})"),
            &[
                ("name", &name),
                (
                    "files",
                    &crate::i18n::tn(self.files.len() as u64, "{n} file", "{n} files"),
                ),
                ("size", &crate::dialogs::dialog_size(total)),
            ],
        );
        crate::fileicons::paint_at(
            ui,
            &p,
            crate::fileicons::Kind::Folder,
            pos2(o.x + 11.0, line - 12.0),
            16.0,
        );
        let x = o.x + 11.0 + crate::fileicons::INDENT;
        let (rect, cut) = winui::text_fit(&p, x, line, &summary, winui::TEXT(), o.x + 589.0);
        if cut {
            winui::full_on_hover(ui, rect, "metalink-name", &summary);
        }

        self.file_list(ui, r(o, 11.0, 58.0, 578.0, 242.0));

        // Select all / none, each as wide as its label in this language (#192), and what is chosen.
        let [all_at, none_at] = winui::buttons_from_left(
            &p,
            11.0,
            6.0,
            &[("Select all", 90.0), ("Select none", 90.0)],
            160.0,
        )[..] else {
            unreachable!("two buttons")
        };
        if winui::button(
            ui,
            r(o, all_at.0, 308.0, all_at.1, 21.0),
            "Select all",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
        {
            self.chosen.iter_mut().for_each(|c| *c = true);
        }
        if winui::button(
            ui,
            r(o, none_at.0, 308.0, none_at.1, 21.0),
            "Select none",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
        {
            self.chosen.iter_mut().for_each(|c| *c = false);
        }
        let (count, bytes) = self.selection();
        winui::text_right(
            &p,
            o.x + 589.0,
            o.y + 323.0,
            &crate::i18n::fmt(
                crate::i18n::t("Selected: {files}, {size}"),
                &[
                    (
                        "files",
                        &crate::i18n::tn(count as u64, "{n} file", "{n} files"),
                    ),
                    ("size", &crate::dialogs::dialog_size(bytes)),
                ],
            ),
            winui::TEXT(),
        );

        // The category, and the folder it leads to.
        text(&p, o.x + 11.0, o.y + 356.0, category_label, winui::TEXT());
        let cat_rect = r(o, fx, 342.0, 174.0, 21.0);
        let shown = self.category.shown();
        let cat = winui::combo(ui, cat_rect, &shown, "metalink-category");
        if cat.clicked() || winui::combo_keys(ui, &cat) {
            self.category_open = !self.category_open;
        }
        let category_was_open = self.category_open;

        text(&p, o.x + 11.0, o.y + 384.0, folder_label, winui::TEXT());
        if winui::edit(
            ui,
            r(o, fx, 370.0, 561.0 - fx, 21.0),
            &mut self.folder,
            true,
            false,
        )
        .changed()
        {
            self.folder_edited = true;
        }
        if winui::button(
            ui,
            r(o, 567.0, 370.0, 22.0, 21.0),
            "...",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked()
        {
            let from = self.folder.clone();
            winui::start_pick(ui.ctx(), "metalink-folder", move || {
                rfd::FileDialog::new().set_directory(from).pick_folder()
            });
        }
        if let Some(d) = winui::picked(ui.ctx(), "metalink-folder") {
            self.folder = d.display().to_string();
            self.folder_edited = true;
        }
        let s = crate::i18n::fmt(
            crate::i18n::t("Files go to: {folder}"),
            &[("folder", &crate::i18n::ltr(&self.folder))],
        );
        let (rect, cut) =
            winui::text_fit(&p, o.x + fx, o.y + 411.0, &s, winui::MUTED(), o.x + 589.0);
        if cut {
            winui::full_on_hover(ui, rect, "metalink-goes", &s);
        }

        // The bottom row, each button as wide as its label in this language (#192).
        let [now_at, later_at, cancel_at] = winui::buttons_to_right(
            &p,
            589.0,
            6.0,
            &[("Download Now", 105.0), ("Later", 91.0), ("Cancel", 91.0)],
            160.0,
        )[..] else {
            unreachable!("three buttons")
        };
        let ready = count > 0 && !self.folder.trim().is_empty();
        let now = winui::button(
            ui,
            r(o, now_at.0, 438.0, now_at.1, 21.0),
            "Download Now",
            kind(ready, true),
            false,
            None,
        )
        .clicked();
        let later = winui::button(
            ui,
            r(o, later_at.0, 438.0, later_at.1, 21.0),
            "Later",
            kind(ready, false),
            false,
            None,
        )
        .clicked();
        let cancel = winui::button(
            ui,
            r(o, cancel_at.0, 438.0, cancel_at.1, 21.0),
            "Cancel",
            ButtonKind::Normal,
            false,
            None,
        )
        .clicked();

        // The categories in a list window of their own (the shared drop list), as Download File
        // Info's.
        if self.category_open {
            let all = Category::all();
            let labels: Vec<String> = all.iter().map(|c| c.shown()).collect();
            if let Some(i) = winui::drop_list(
                ui,
                "metalink-category-list",
                cat_rect,
                &labels,
                "",
                &mut self.category_open,
            ) && let Some(c) = all.get(i)
            {
                self.set_category(*c);
            }
        }
        // Escape closes an open drop list first, then the window.
        if cancel || (!category_was_open && winui::escape_pressed(ui)) {
            return Outcome::Closed;
        }
        let enter = !category_was_open && ready && winui::enter_pressed(ui);
        if ready && (now || later || enter) {
            self.start_now = Some(!later);
            return Outcome::Done(());
        }
        Outcome::Open
    }

    /// The files, each with its box, its name (folders and all), its sources (their hosts on
    /// hover), how it is checked and its size.
    fn file_list(&mut self, ui: &mut Ui, list: Rect) {
        let p = ui.painter().clone();
        winui::solid(&p, list, winui::FIELD_EDGE());
        // Laid out in English (`list` comes on screen); each part made where it is on screen.
        let list = winui::rect(list);
        let inner = list.shrink(1.0);
        p.rect_filled(winui::rect(inner), 0.0, winui::FIELD());
        let head = Rect::from_min_size(inner.min, vec2(inner.width(), 20.0));
        let body = Rect::from_min_max(pos2(inner.left(), head.bottom()), inner.max);
        // Column edges from the right: the size, the check, the sources.
        let size_right = head.right() - 24.0;
        let check_right = size_right - 84.0;
        let sources_right = check_right - 120.0;
        // The header: the columns' titles.
        p.rect_filled(winui::rect(head), 0.0, LIST_HEAD());
        fill(
            &p,
            head.left(),
            head.bottom() - 1.0,
            head.right() - 1.0,
            head.bottom() - 1.0,
            LIST_LINE(),
        );
        // i18n: column title: file name
        text(
            &p,
            head.left() + 25.0,
            head.top() + 14.0,
            crate::i18n::t("Name"),
            winui::TEXT(),
        );
        // i18n: Add Metalink, column title: how many servers have the file
        let sources = crate::i18n::t("Sources");
        winui::text_right(&p, sources_right, head.top() + 14.0, sources, winui::TEXT());
        // i18n: Add Metalink, column title: the hash the file is checked with
        let check = crate::i18n::t("Check");
        winui::text_right(&p, check_right, head.top() + 14.0, check, winui::TEXT());
        // i18n: column title: file size
        winui::text_right(
            &p,
            size_right,
            head.top() + 14.0,
            crate::i18n::t("Size"),
            winui::TEXT(),
        );
        self.rows.name = crate::i18n::t("Files");
        let (files, chosen) = (&self.files, &mut self.chosen);
        let mut toggle = None;
        let ev = winui::row_list(
            ui,
            "metalink-files",
            winui::rect(body.expand(1.0)),
            files.len(),
            ROW,
            &mut self.rows,
            |ui, clip, k, row, look| {
                let f = &files[k];
                let top = row.top();
                let glyph = pos2(row.left() + 6.0, top + 2.0);
                let name = f.name();
                let size = f.size.map_or(
                    crate::i18n::t("unknown").to_owned(),
                    crate::dialogs::dialog_size,
                );
                look.name = format!("{name}, {size}");
                let mut on = chosen[k];
                if winui::row_check(ui, clip, glyph, &mut on, &name, ("metalink-file", k)) {
                    toggle = Some(k);
                }
                crate::fileicons::paint_at(
                    ui,
                    clip,
                    crate::fileicons::Kind::File(&name),
                    pos2(glyph.x + 19.0, top + (ROW - 16.0) / 2.0),
                    16.0,
                );
                let left = glyph.x + 19.0 + crate::fileicons::INDENT;
                let shown = crate::i18n::isolate(&name);
                winui::fit(
                    ui,
                    clip,
                    left,
                    top + 12.0,
                    &shown,
                    look.ink,
                    sources_right - 60.0,
                    ("metalink-name", k),
                );
                // The sources: how many, their hosts on hover.
                let n = crate::i18n::integer(f.urls.len() as u64);
                let (at, _) = winui::text_right(clip, sources_right, top + 12.0, &n, look.ink);
                let hosts: Vec<String> = f
                    .urls
                    .iter()
                    .map(|u| crate::i18n::ltr(u.host_str().unwrap_or(u.as_str())))
                    .collect();
                winui::full_on_hover(ui, at, ("metalink-sources", k), &hosts.join("\n"));
                winui::text_right(clip, check_right, top + 12.0, &f.check(), look.ink);
                winui::text_right(clip, size_right, top + 12.0, &size, look.ink);
            },
        );
        if let Some(k) = ev.double_clicked.or(ev.space) {
            toggle = Some(k);
        }
        if let Some(k) = toggle {
            self.chosen[k] = !self.chosen[k];
        }
    }

    /// Two files of a made-up Metalink (off-screen pictures, `/show metalink`).
    pub fn sample(downloads: PathBuf, dirs: Vec<(String, String)>) -> MetalinkDialog {
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
          <file name="distro/disc1.iso"><size>4700000000</size>
            <hash type="sha-256">d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592</hash>
            <url priority="1">https://mirror-a.example/distro/disc1.iso</url>
            <url priority="2" location="de">https://mirror-b.example/distro/disc1.iso</url>
            <url priority="3">ftp://mirror-c.example/distro/disc1.iso</url></file>
          <file name="distro/disc2.iso"><size>3100000000</size>
            <hash type="sha-256">d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592</hash>
            <url priority="1">https://mirror-a.example/distro/disc2.iso</url>
            <url priority="2">https://mirror-b.example/distro/disc2.iso</url></file>
          <file name="distro/SHA256SUMS.txt"><size>212</size>
            <url>https://mirror-a.example/distro/SHA256SUMS.txt</url></file>
        </metalink>"#;
        let origin = "https://releases.example/distro.meta4";
        let manifest = mdm_engine::metalink::parse(xml, &Url::parse(origin).expect("an address"))
            .expect("the sample reads");
        MetalinkDialog::new(
            Opened {
                manifest,
                origin: origin.to_owned(),
            },
            None,
            downloads,
            dirs,
        )
    }

    /// Off-screen test copies: Later pressed (`/show metalink-add`).
    pub fn press_later(&mut self) -> MetalinkAdd {
        self.add(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(xml: &str) -> Opened {
        let origin = "https://releases.example/set.meta4";
        Opened {
            manifest: mdm_engine::metalink::parse(xml, &Url::parse(origin).unwrap()).unwrap(),
            origin: origin.to_owned(),
        }
    }

    const SET: &str = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
        <file name="set/a.zip"><size>100</size>
          <hash type="sha-256">d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592</hash>
          <url priority="1" location="us">https://us.example/a.zip</url>
          <url priority="1" location="ke">https://ke.example/a.zip</url>
          <url priority="2">https://slow.example/a.zip</url></file>
        <file name="set/b.zip"><size>50</size>
          <url>https://us.example/b.zip</url></file>
      </metalink>"#;

    #[test]
    fn addresses_and_paths_that_are_metalinks() {
        let u = |s: &str| Url::parse(s).unwrap();
        assert!(is_metalink_url(
            &u("https://r.example/set.meta4"),
            None,
            None
        ));
        assert!(is_metalink_url(
            &u("https://r.example/a.Metalink?x=1"),
            None,
            None
        ));
        assert!(is_metalink_url(
            &u("https://r.example/get?id=4"),
            Some("set.meta4"),
            None
        ));
        assert!(is_metalink_url(
            &u("https://r.example/get?id=4"),
            None,
            Some("application/metalink4+xml; charset=utf-8")
        ));
        assert!(!is_metalink_url(&u("https://r.example/a.zip"), None, None));
        assert!(!is_metalink_url(&u("ftp://r.example/a.meta4"), None, None));

        assert_eq!(
            metalink_path("\"C:\\Downloads\\set.meta4\""),
            Some(PathBuf::from("C:\\Downloads\\set.meta4"))
        );
        assert_eq!(
            metalink_path("/home/me/set.metalink"),
            Some(PathBuf::from("/home/me/set.metalink"))
        );
        assert_eq!(metalink_path("https://r.example/set.meta4"), None);
        assert_eq!(metalink_path("C:\\Downloads\\a.zip"), None);
    }

    #[test]
    fn the_window_ticks_every_file_with_its_sources_for_the_users_country() {
        let mut d = MetalinkDialog::new(opened(SET), Some("KE"), PathBuf::from("/dl"), Vec::new());
        assert_eq!(d.chosen, [true, true]);
        assert_eq!(d.category, Category::Compressed);
        assert_eq!(d.selection(), (2, 150));
        let add = d.add(true);
        assert_eq!(add.files.len(), 2);
        let a = &add.files[0];
        assert_eq!(a.path, PathBuf::from("set").join("a.zip"));
        // Kenya's server of the first priority first, then the American one, then the slow one.
        let hosts: Vec<_> = a.urls.iter().map(|u| u.host_str().unwrap()).collect();
        assert_eq!(hosts, ["ke.example", "us.example", "slow.example"]);
        assert!(a.checksum.is_some());
        assert_eq!(a.check(), "SHA-256");
        // Each download: its first source, its place under the folder.
        let made = downloads(&add);
        assert_eq!(made[0].0.host_str(), Some("ke.example"));
        assert_eq!(made[1].1, add.dir.join("set").join("b.zip"));
        assert_eq!(made[1].2, Some(50));

        // Unticked files stay out; Later doesn't start them.
        d.chosen[0] = false;
        let later = d.press_later();
        assert!(!later.start_now);
        assert_eq!(later.files.len(), 1);
        assert_eq!(later.files[0].path, PathBuf::from("set").join("b.zip"));
    }

    #[test]
    fn the_folder_follows_the_category_until_one_is_chosen() {
        let mut d = MetalinkDialog::new(opened(SET), None, PathBuf::from("/dl"), Vec::new());
        let compressed = d.folder.clone();
        d.set_category(Category::Documents);
        assert_ne!(d.folder, compressed, "Documents' folder");
        d.folder = "/elsewhere".into();
        d.folder_edited = true;
        d.set_category(Category::Video);
        assert_eq!(d.folder, "/elsewhere");
        assert_eq!(d.save_folder(), crate::dialogs::resolve_path("/elsewhere"));
    }

    #[test]
    fn a_metalinks_name_is_its_address_last_part_or_its_file_name() {
        assert_eq!(
            origin_name("https://r.example/get/set%20one.meta4?files=1M/a.bin,2M/b/c.bin"),
            "set one.meta4"
        );
        assert_eq!(origin_name("https://r.example/dir/"), "dir");
        assert_eq!(
            origin_name(
                &PathBuf::from("dl")
                    .join("set.metalink")
                    .display()
                    .to_string()
            ),
            "set.metalink"
        );
    }

    #[test]
    fn mixed_files_go_to_general() {
        let picks = |names: &[&str]| -> Vec<Pick> {
            names
                .iter()
                .map(|n| Pick {
                    path: PathBuf::from(n),
                    size: None,
                    urls: Vec::new(),
                    checksum: None,
                    pieces: None,
                })
                .collect()
        };
        assert_eq!(category_of(&picks(&["a.mp4", "b.mkv"])), Category::Video);
        assert_eq!(category_of(&picks(&["a.mp4", "b.zip"])), Category::General);
        assert_eq!(category_of(&[]), Category::General);
    }

    #[test]
    fn a_metalink_file_on_disk_is_read_and_its_relative_addresses_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("set.meta4");
        std::fs::write(
            &p,
            r#"<metalink><file name="a.bin"><url>relative/a.bin</url>
               <url>https://cdn.example/a.bin</url></file></metalink>"#,
        )
        .unwrap();
        let m = read_file(&p).unwrap();
        assert_eq!(m.files[0].urls.len(), 1, "only the absolute address");
        std::fs::write(&p, "<html>not found</html>").unwrap();
        assert!(read_file(&p).is_err());
        assert!(read_file(&dir.path().join("gone.meta4")).is_err());
    }
}
