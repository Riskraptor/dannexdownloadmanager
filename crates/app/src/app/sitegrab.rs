//! Site grabber in the main window (Tasks → Run site grabber…, #site-grabber): the projects
//! (grabproj.rs) under "Grabber projects" in the Categories tree and in the Tasks menu, their
//! right-click menu, the wizard (sitegrabdlg.rs), the grab under way, and the files it hands over
//! added to the download list as one batch, as Link grabber's links are (`push_item`, into a
//! queue, no window for each). A child of `app` so that app.rs only calls in here; its texts live
//! in sitegrabdlg.rs (the catalog's extraction reads the top-level files).
//!
//! The grab is the app's, not the window's: closing the window leaves it exploring, the
//! project's node says so, and its files are kept in the project when it ends, to open again
//! from there. A project whose pages work offline has each page and style sheet made to lead to
//! the copies on disk as its download completes (`Rewriter`, beside the window).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use mdm_sitegrab::offline::{self, Copy, Text};
use mdm_sitegrab::{Crawl, End, Found, Net, Phase};

use super::{App, Branch, Cmd, View};
use crate::chrome::{IconKey, MenuEntry, TreeRow};
use crate::dialogs::{self, Confirm, Outcome};
use crate::grabproj::{Project, Store};
use crate::model::{Category, Item, Status};
use crate::sitegrabdlg::{Ask, SiteGrabDlg};
use mdm_ipc as ipc;

/// The projects, the wizard when it is open, the grab under way, and the question before a
/// project is deleted.
pub(super) struct Grabs {
    pub store: Store,
    saver: crate::store::Saver,
    pub dlg: Option<SiteGrabDlg>,
    ask_delete: Option<(u64, Confirm)>,
    /// The grab under way (one at a time): its project, and the grab. It goes on when its window
    /// is closed.
    crawl: Option<(u64, Crawl)>,
    /// The cookies each project's site set during its last run in this session (and the user's):
    /// the downloads of its files carry them (memory only, as the browser's do).
    site_cookies: HashMap<u64, String>,
    /// Downloads kept out of All Downloads (`Project::listed`), worked out when projects change.
    apart: HashSet<u64>,
    /// Each offline project's copy: its downloads' addresses and files, made when needed.
    copies: HashMap<u64, Arc<Copy>>,
    rewriter: Rewriter,
}

impl Grabs {
    pub fn load() -> Grabs {
        let store = Store::path().map(|p| Store::load(&p)).unwrap_or_default();
        Grabs {
            apart: store.kept_apart(),
            store,
            saver: crate::store::Saver::writing("grabber saver", crate::grabproj::write),
            dlg: None,
            ask_delete: None,
            crawl: None,
            site_cookies: HashMap::new(),
            copies: HashMap::new(),
            rewriter: Rewriter::default(),
        }
    }

    /// On the saver's thread; how it went is logged at the next tick.
    fn save(&mut self) {
        self.apart = self.store.kept_apart();
        let Some(path) = Store::path() else { return };
        match self.store.encode() {
            Ok(json) => self.saver.queue(path, json),
            Err(e) => tracing::warn!("grabber: {} not saved: {e}", path.display()),
        }
    }

    /// What the saver has in hand, written now (DDM is closing: its thread may not get to it).
    pub fn flush(&self) {
        self.saver.flush();
    }

    /// Project `id` is exploring its site.
    fn running(&self, id: u64) -> bool {
        self.crawl.as_ref().is_some_and(|(p, _)| *p == id)
    }
}

/// A project's command (its right-click menu, the Tasks menu, the empty list's link).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum GrabAct {
    /// Its window: the grab under way, else the files it found, else its settings.
    Open(u64),
    /// Go on from where its last run stopped.
    Continue(u64),
    Run(u64),
    Edit(u64),
    /// Open its offline copy's start page.
    OpenCopy(u64),
    Delete(u64),
    /// Show its downloads (its node in the Categories tree).
    Show(u64),
}

impl App {
    /// Tasks → Run site grabber… (`None`), or a project's command.
    pub(super) fn grab_cmd(&mut self, act: Option<GrabAct>, ctx: &egui::Context) {
        let viewport = dialogs::dialog_viewport("site-grabber");
        let project = |id: u64| self.grabs.store.get(id).cloned();
        match act {
            None => {
                if self.grabs.dlg.is_none() {
                    match self.grabs.crawl.as_ref().map(|(id, _)| *id) {
                        // A grab under way: its window again.
                        Some(id) => self.grab_open(id),
                        None => {
                            // A new project joins the queue the last one used.
                            let queue = self.grabs.store.projects.last().map_or(0, |p| p.queue);
                            self.grabs.dlg = Some(SiteGrabDlg::new(
                                Project {
                                    queue,
                                    ..Project::default()
                                },
                                self.grab_default_folder(),
                            ));
                        }
                    }
                }
            }
            Some(GrabAct::Open(id)) => self.grab_open(id),
            Some(GrabAct::Edit(id)) => {
                let Some(p) = project(id) else { return };
                if !self.grabs.running(id) {
                    self.grabs.dlg = Some(SiteGrabDlg::edit(p, self.grab_default_folder()));
                }
            }
            Some(GrabAct::Run(id)) => {
                let Some(p) = project(id) else { return };
                if !self.grabs.running(id) {
                    self.grabs.dlg = Some(SiteGrabDlg::edit(p, self.grab_default_folder()));
                    self.grab_crawl(false);
                }
            }
            Some(GrabAct::Continue(id)) => {
                let Some(p) = project(id) else { return };
                if !self.grabs.running(id) {
                    self.grabs.dlg = Some(self.grab_files_dlg(p));
                    self.grab_crawl(true);
                }
            }
            Some(GrabAct::OpenCopy(id)) => {
                self.grab_open_copy(id);
                return;
            }
            Some(GrabAct::Show(id)) => {
                if self.grabs.store.get(id).is_some() {
                    self.grabs.store.open = true;
                    self.view = View::Grabber(id);
                }
                return;
            }
            Some(GrabAct::Delete(id)) => {
                let Some(p) = project(id) else { return };
                self.grabs.ask_delete = Some((id, crate::sitegrabdlg::delete_question(&p.name)));
                return;
            }
        }
        dialogs::raise(ctx, viewport);
    }

    /// Project `id`'s window: on its grab while it explores, else on the files it found (or its
    /// settings, when it found none yet). A window open on another project that isn't
    /// exploring gives way to it.
    fn grab_open(&mut self, id: u64) {
        if self.grabs.dlg.as_ref().is_some_and(|d| d.id() == id) {
            return;
        }
        let Some(p) = self.grabs.store.get(id).cloned() else {
            return;
        };
        let mut d = if self.grabs.running(id) || (p.found.is_empty() && p.ended.is_none()) {
            SiteGrabDlg::edit(p, self.grab_default_folder())
        } else {
            self.grab_files_dlg(p)
        };
        if let Some((_, c)) = self.grabs.crawl.as_ref().filter(|(p, _)| *p == id) {
            d.run_started();
            d.set_status(Some(c.status()));
        }
        self.grabs.dlg = Some(d);
    }

    /// The window on project `p`'s files found, those already in the list shown so.
    fn grab_files_dlg(&self, p: Project) -> SiteGrabDlg {
        let mut d = SiteGrabDlg::files_of(p, self.grab_default_folder());
        let known = self.known_urls();
        d.mark_known(|u| known.contains(u));
        d
    }

    /// Every download address in the list (a look-up for thousands of files found at once).
    fn known_urls(&self) -> HashSet<&str> {
        self.items.iter().map(|i| i.url.as_str()).collect()
    }

    /// The folder an offline copy goes to when none is typed: General's (one folder for the
    /// whole site, not one per category).
    fn grab_default_folder(&self) -> String {
        self.settings
            .category_dir(Category::General, &self.downloads)
            .display()
            .to_string()
    }

    /// Download `i` was added by project `id` (its node in the Categories tree).
    pub(super) fn grab_in_view(&self, id: u64, i: &Item) -> bool {
        self.grabs
            .store
            .get(id)
            .is_some_and(|p| p.added.contains(&i.id))
    }

    /// Download `id` is a project's that keeps its downloads out of All Downloads: shown under
    /// the project and its queue only.
    pub(super) fn grab_kept_apart(&self, id: u64) -> bool {
        self.grabs.apart.contains(&id)
    }

    /// Download `id` came in a batch (a grabber project's, a Link grabber package's) or a running
    /// queue started it: its end opens no Download complete window, as a thousand files would
    /// open a thousand (IDM's queue downloads open none either).
    pub(super) fn batch_quiet(&self, id: u64) -> bool {
        self.grabs.store.owner(id).is_some()
            || self.queue_started.contains(&id)
            || self.items.iter().any(|i| i.id == id && i.package.is_some())
    }

    /// "Grabber projects" and each project, after the queues (once there is a project).
    pub(super) fn grab_tree_rows(&self, rows: &mut Vec<(TreeRow<'static>, Option<View>, Branch)>) {
        let store = &self.grabs.store;
        if store.projects.is_empty() {
            return;
        }
        rows.push((
            TreeRow {
                level: 0,
                label: crate::sitegrabdlg::tree_label().into(),
                expander: Some(store.open),
                selected: false,
                icon: IconKey::Themed("cat-grabber", "folder"),
                badge: None,
                count: None,
            },
            None,
            Branch::Grabber,
        ));
        if !store.open {
            return;
        }
        for p in &store.projects {
            let view = View::Grabber(p.id);
            let n = self
                .items
                .iter()
                .filter(|i| p.added.contains(&i.id))
                .count();
            let label = if self.grabs.running(p.id) {
                crate::sitegrabdlg::exploring_label(&p.name)
            } else {
                p.name.clone()
            };
            rows.push((
                TreeRow {
                    level: 1,
                    label: label.into(),
                    expander: None,
                    selected: self.view == view,
                    icon: IconKey::Silk("folder_page"),
                    badge: self.grabs.running(p.id).then_some("arrow_down"),
                    count: (n > 0).then_some(n),
                },
                Some(view),
                Branch::Leaf,
            ));
        }
    }

    /// Open or close the "Grabber projects" node (kept with the projects).
    pub(super) fn grab_toggle(&mut self) {
        self.grabs.store.open = !self.grabs.store.open;
        self.grabs.save();
    }

    /// A project's right-click menu (None: the node isn't a project).
    pub(super) fn grab_node_entries(&self, node: Option<View>) -> Option<Vec<MenuEntry<Cmd>>> {
        let Some(View::Grabber(id)) = node else {
            return None;
        };
        let p = self.grabs.store.get(id)?;
        let running = self.grabs.running(id);
        Some(crate::sitegrabdlg::project_menu(
            |act| {
                use crate::sitegrabdlg::MenuAct as M;
                Cmd::GrabProject(match act {
                    M::Open => GrabAct::Open(id),
                    M::Continue => GrabAct::Continue(id),
                    M::Run => GrabAct::Run(id),
                    M::Edit => GrabAct::Edit(id),
                    M::OpenCopy => GrabAct::OpenCopy(id),
                    M::Delete => GrabAct::Delete(id),
                })
            },
            crate::sitegrabdlg::MenuState {
                running,
                can_continue: p.can_continue() && !running,
                offline: p.offline,
                copy_ready: self.grab_copy_ready(p),
            },
        ))
    }

    /// Tasks → Grabber projects: the latest projects, each opening its window.
    pub(super) fn grab_menu_entries(&self) -> Vec<MenuEntry<Cmd>> {
        self.grabs
            .store
            .recent(10)
            .into_iter()
            .map(|p| {
                let label = if self.grabs.running(p.id) {
                    crate::sitegrabdlg::exploring_label(&p.name)
                } else {
                    p.name.clone()
                };
                MenuEntry::Item(
                    crate::chrome::label(&label.replace('&', "&&")),
                    Cmd::GrabProject(GrabAct::Open(p.id)),
                    true,
                    false,
                )
            })
            .collect()
    }

    /// The empty list of project `id`'s node: what to say and the link that opens the project
    /// (its texts, English).
    pub(super) fn grab_empty_state(
        &self,
        id: u64,
    ) -> Option<(&'static str, &'static str, &'static str, Cmd)> {
        self.grabs.store.get(id)?;
        let (title, why, action) = crate::sitegrabdlg::empty_project(self.grabs.running(id));
        Some((title, why, action, Cmd::GrabProject(GrabAct::Open(id))))
    }

    /// An empty list (All Downloads, a category) whose downloads a project keeps apart under its
    /// node: what to say, and the link that shows them (the latest such project's).
    pub(super) fn grab_apart_state(
        &self,
        view: View,
    ) -> Option<(&'static str, &'static str, &'static str, Cmd)> {
        if matches!(view, View::Grabber(_) | View::Queue(_)) || self.grabs.apart.is_empty() {
            return None;
        }
        // Only when one of them would be in this view (a category of their type, say).
        let in_cat = |c: Option<Category>, i: &Item| c.is_none_or(|c| i.category == c);
        let would_show = |i: &Item| match view {
            View::All => true,
            View::Cat(c) => i.category == c,
            View::Unfinished(c) => i.status != Status::Complete && in_cat(c, i),
            View::Finished(c) => i.status == Status::Complete && in_cat(c, i),
            View::Queue(_) | View::Grabber(_) => false,
        };
        if !self
            .items
            .iter()
            .any(|i| self.grabs.apart.contains(&i.id) && would_show(i))
        {
            return None;
        }
        let p = self
            .grabs
            .store
            .recent(usize::MAX)
            .into_iter()
            .find(|p| !p.listed && !p.added.is_empty())?;
        let (title, why, action) = crate::sitegrabdlg::empty_apart();
        Some((title, why, action, Cmd::GrabProject(GrabAct::Show(p.id))))
    }

    /// A queue was deleted: the projects that add to it, or to one after it, follow.
    pub(super) fn grab_queue_removed(&mut self, q: usize) {
        self.grabs.store.queue_removed(q);
        self.grabs.save();
    }

    /// Each frame: the grab under way, the wizard and what it asks, the delete question, the
    /// saver's news.
    pub(super) fn grab_tick(&mut self, ctx: &egui::Context) {
        if let Some(Err(e)) = self.grabs.saver.outcome() {
            tracing::warn!("grabber: not saved: {e}");
        }
        self.grab_poll(ctx);
        if let Some((id, q)) = &mut self.grabs.ask_delete {
            let id = *id;
            match q.show(ctx) {
                Outcome::Open => {}
                Outcome::Closed => self.grabs.ask_delete = None,
                Outcome::Done(()) => {
                    self.grabs.ask_delete = None;
                    if self.grabs.dlg.as_ref().is_some_and(|d| d.id() == id) {
                        self.grabs.dlg = None;
                    }
                    if self.grabs.running(id) {
                        // Dropping it stops it.
                        self.grabs.crawl = None;
                    }
                    self.grabs.store.remove(id);
                    self.grabs.copies.remove(&id);
                    self.grabs.site_cookies.remove(&id);
                    if self.view == View::Grabber(id) {
                        self.view = View::All;
                    }
                    self.grabs.save();
                }
            }
        }
        if self.grabs.dlg.is_none() {
            return;
        }
        // The queues by the names the interface shows.
        let queues: Vec<String> = self
            .queues
            .iter()
            .map(|q| crate::i18n::t(&q.name).to_owned())
            .collect();
        let Some(d) = self.grabs.dlg.as_mut() else {
            return;
        };
        let out = d.show(ctx, &queues);
        let project = match &out {
            Outcome::Done(Ask::Download { .. }) => {
                self.grabs.dlg.as_ref().map(SiteGrabDlg::project)
            }
            _ => None,
        };
        match out {
            Outcome::Open => {}
            // The grab under way goes on: the project's node shows it, and opens it again.
            Outcome::Closed => self.grabs.dlg = None,
            Outcome::Done(Ask::Crawl) => self.grab_crawl(false),
            Outcome::Done(Ask::Continue) => self.grab_crawl(true),
            Outcome::Done(Ask::Stop) => {
                if let Some((_, c)) = &self.grabs.crawl {
                    c.stop();
                }
            }
            Outcome::Done(Ask::Download {
                files,
                queue,
                start,
            }) => {
                self.grabs.dlg = None;
                if let Some(p) = project {
                    self.grab_add(p, files, queue, start);
                }
            }
        }
    }

    /// The grab under way: its progress to its window, and once it has ended, what it found
    /// kept in its project (the window, open or opened later, lists it).
    fn grab_poll(&mut self, ctx: &egui::Context) {
        let Some((id, c)) = &self.grabs.crawl else {
            return;
        };
        let id = *id;
        let status = c.status();
        let Phase::Ended(end) = status.phase.clone() else {
            if let Some(d) = self.grabs.dlg.as_mut().filter(|d| d.id() == id) {
                d.set_status(Some(status));
            }
            // The window's figures, and the node's label once it ends.
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        let Some((_, c)) = self.grabs.crawl.take() else {
            return;
        };
        // A run that failed from the start (the server down) found nothing: the files the
        // project's last good run found are kept, not replaced by an empty list.
        let failed = matches!(end, End::Failed(_));
        if let Some(p) = self.grabs.store.get_mut(id) {
            if !failed {
                p.found = c.files();
                p.frontier = c.frontier();
            }
            p.ended = Some(end);
            p.last_run = Some(crate::model::now_stamp());
        }
        match c.site_cookies() {
            Some(cookies) => self.grabs.site_cookies.insert(id, cookies),
            None => self.grabs.site_cookies.remove(&id),
        };
        self.grab_save();
        let Some(p) = self.grabs.store.get(id).cloned() else {
            return;
        };
        let known: HashSet<&str> = self.items.iter().map(|i| i.url.as_str()).collect();
        if let Some(d) = self.grabs.dlg.as_mut().filter(|d| d.id() == id) {
            d.set_status(Some(status));
            d.crawl_ended(p, |u| known.contains(u));
        }
    }

    /// Keep the wizard's project and start its grab (`resume`: from where its last run
    /// stopped), with the connection settings downloads use. One grab at a time.
    fn grab_crawl(&mut self, resume: bool) {
        let Some(p) = self.grabs.dlg.as_ref().map(SiteGrabDlg::project) else {
            return;
        };
        if let Some((other, _)) = &self.grabs.crawl {
            if *other != p.id
                && let Some(d) = &mut self.grabs.dlg
            {
                d.refuse_busy();
            }
            return;
        }
        let net = self.grab_net(&p);
        let frontier = p.frontier.clone().filter(|_| resume);
        let mut kept = Project {
            last_run: Some(crate::model::now_stamp()),
            ..p.clone()
        };
        if frontier.is_none() {
            kept.frontier = None;
            kept.ended = None;
        }
        let id = self.grabs.store.put(kept);
        self.grab_save();
        let crawl = match frontier {
            Some(f) => Crawl::resume(&self.rt, &p.spec, net, p.found.clone(), f),
            None => Crawl::start(&self.rt, &p.spec, net),
        };
        let Some(d) = &mut self.grabs.dlg else { return };
        d.set_id(id);
        match crawl {
            Ok(c) => {
                d.run_started();
                d.set_status(Some(c.status()));
                self.grabs.crawl = Some((id, c));
            }
            Err(e) => d.refused(e),
        }
    }

    /// How a grab connects: as downloads do (proxy, DNS, adapter, User-Agent), with the
    /// project's login, else Sites Logins' for its site, and its cookies.
    fn grab_net(&self, p: &Project) -> Net {
        let Some(start) = mdm_sitegrab::start_url(&p.spec.start) else {
            return Net::default();
        };
        let login = p.login();
        let parts = self.request_parts(&start, login.as_ref(), &ipc::Command::default(), true);
        let mut opts = parts.opts;
        // Pages are read, not saved: nothing to mark, no debrid service, no shared speed cap.
        opts.on_file_created = None;
        opts.unlocker = None;
        opts.global = None;
        opts.post = None;
        Net {
            opts,
            login: login.or_else(|| self.settings.site_login(&start)),
            cookies: p.cookies(),
        }
    }

    /// The folder file `f` of project `p` goes under: the project's, else (an offline copy)
    /// General's for the whole site, else its category's.
    fn grab_base(&self, p: &Project, category: Category) -> PathBuf {
        match p.folder.trim() {
            "" if p.offline => PathBuf::from(self.grab_default_folder()),
            "" => self.settings.category_dir(category, &self.downloads),
            typed => PathBuf::from(typed),
        }
    }

    /// The files chosen, added to the list as one batch, as Link grabber adds its links: each in
    /// its place (`Project::place`), the page it was found on as its referrer, the start site's
    /// login and cookies with the start site's files, into queue `queue`, which starts when
    /// `start`. No window opens for any of them; the list shows the project's node.
    fn grab_add(&mut self, project: Project, files: Vec<Found>, queue: usize, start: bool) {
        let id = self.grabs.store.put(Project {
            queue,
            start_now: start,
            ..project.clone()
        });
        let q = queue.min(self.queues.len().saturating_sub(1));
        let start_url = mdm_sitegrab::start_url(&project.spec.start);
        let login = project.login();
        let cookies = self
            .grabs
            .site_cookies
            .get(&id)
            .cloned()
            .or_else(|| project.cookies());
        let mut added = Vec::new();
        let mut guessed = Vec::new();
        // An offline copy's places: each its own, as the disk tells them apart (two addresses
        // that differ in letter case only are one file to Windows), none another row's.
        let mut taken: HashSet<String> = if project.offline {
            self.items
                .iter()
                .map(|i| offline::place_key(&i.dest))
                .collect()
        } else {
            HashSet::new()
        };
        for f in files {
            let Ok(url) = url::Url::parse(&f.url) else {
                continue;
            };
            let category = project.category_of(&f.name);
            let base = self.grab_base(&project, category);
            let mut dest = project.place(&base, &f);
            if project.offline {
                // Too deep for Windows' limit on a path: a shorter place on the site's folder
                // (the links are made for wherever it is).
                if !mdm_engine::naming::path_fits(&dest) {
                    dest = base.join(offline::short_path(&f));
                }
                dest = offline::unique_in(dest, &mut taken);
            }
            let context = ipc::Command {
                referer: (!f.page.is_empty()).then(|| f.page.clone()),
                ..Default::default()
            };
            // The login and cookies go with the start site's files only.
            let same_site = start_url
                .as_ref()
                .is_some_and(|s| mdm_sitegrab::Scope::SameHost.allows(s, &url));
            let context = ipc::Command {
                cookies: cookies.clone().filter(|_| same_site),
                ..context
            };
            let new = self.push_item(
                url,
                dest,
                category,
                String::new(),
                f.size,
                login.clone().filter(|_| same_site),
                context,
                crate::sitegrabdlg::added_by(),
            );
            if let Some(queue) = self.queues.get_mut(q) {
                queue.items.push(new);
            }
            added.push(new);
            // An offline copy's file goes exactly where its links are made for (and replaces
            // an older copy's file there, as a site saved again should); any other is named
            // after its address, the server's own name may still replace it, and a name taken
            // is numbered.
            if !project.offline {
                guessed.push(new);
            }
        }
        for &new in &guessed {
            self.name_guessed(new, true);
        }
        self.number_beside(&guessed);
        if let Some(p) = self.grabs.store.get_mut(id) {
            p.added.extend(added.iter().copied());
        }
        self.grabs.copies.remove(&id);
        self.grab_save();
        self.dirty = true;
        if project.offline {
            self.grab_offline_added(id);
        }
        // Where they went: the project's node, open.
        self.grabs.store.open = true;
        self.view = View::Grabber(id);
        if start && !added.is_empty() {
            self.start_queue(q);
        }
    }

    /// An offline project's files were added: the page at its folder's top that opens the
    /// copy, and the pages already downloaded made to lead to the new files too.
    fn grab_offline_added(&mut self, id: u64) {
        let Some(p) = self.grabs.store.get(id).cloned() else {
            return;
        };
        let base = self.grab_base(&p, Category::General);
        // The start page where its download puts it (its place may have had to change), else
        // where the copy puts it.
        let address = p
            .found
            .iter()
            .find(|f| f.kind == mdm_sitegrab::FoundKind::Page)
            .map(|f| f.url.clone());
        let start = address
            .as_deref()
            .and_then(|a| {
                self.items
                    .iter()
                    .find(|i| p.added.contains(&i.id) && i.url.as_str() == a)
            })
            .map(|i| i.dest.clone())
            .or_else(|| p.start_file(&base));
        if let (Some(start), Some(address)) = (start, address) {
            let shortcut = p.shortcut(&base);
            let name = p.name.clone();
            // Made beside the window: its folder is made once, without a question (#743).
            crate::launch::spawn("grabber copy", move || {
                write_shortcut(&shortcut, &name, &address, &start);
            });
        }
        let copy = self.grab_copy(id);
        let done: Vec<(PathBuf, url::Url)> = self
            .items
            .iter()
            .filter(|i| p.added.contains(&i.id) && i.status == Status::Complete)
            .map(|i| (i.dest.clone(), i.url.clone()))
            .collect();
        for (file, url) in done {
            if let Some(text) = offline::text_of(&file) {
                self.grabs.rewriter.send(Rewrite {
                    file,
                    url,
                    text,
                    copy: copy.clone(),
                });
            }
        }
    }

    /// Project `id`'s copy: each of its downloads' address and file, and the addresses that
    /// led to its pages (made once, until it adds files).
    fn grab_copy(&mut self, id: u64) -> Arc<Copy> {
        if let Some(c) = self.grabs.copies.get(&id) {
            return c.clone();
        }
        let mut copy = Copy::default();
        if let Some(p) = self.grabs.store.get(id) {
            let aliases: HashMap<&str, &Vec<String>> = p
                .found
                .iter()
                .filter(|f| !f.also.is_empty())
                .map(|f| (f.url.as_str(), &f.also))
                .collect();
            for i in self.items.iter().filter(|i| p.added.contains(&i.id)) {
                for also in aliases
                    .get(i.url.as_str())
                    .into_iter()
                    .flat_map(|a| a.iter())
                {
                    copy.add(also, i.dest.clone());
                }
                copy.add(i.url.as_str(), i.dest.clone());
            }
        }
        let copy = Arc::new(copy);
        self.grabs.copies.insert(id, copy.clone());
        copy
    }

    /// A download finished: when it is a page or style sheet of an offline project's, its
    /// links are made to lead to the copy's files (beside the window).
    pub(super) fn grab_finished(&mut self, id: u64) {
        let Some(pid) = self
            .grabs
            .store
            .owner(id)
            .filter(|p| p.offline)
            .map(|p| p.id)
        else {
            return;
        };
        let Some((file, url)) = self
            .items
            .iter()
            .find(|i| i.id == id)
            .map(|i| (i.dest.clone(), i.url.clone()))
        else {
            return;
        };
        let Some(text) = offline::text_of(&file) else {
            return;
        };
        let copy = self.grab_copy(pid);
        self.grabs.rewriter.send(Rewrite {
            file,
            url,
            text,
            copy,
        });
    }

    /// Whether project `p`'s offline copy has its start page downloaded.
    fn grab_copy_ready(&self, p: &Project) -> bool {
        let Some(first) = p
            .found
            .iter()
            .find(|f| f.kind == mdm_sitegrab::FoundKind::Page)
        else {
            return false;
        };
        p.offline
            && self.items.iter().any(|i| {
                p.added.contains(&i.id)
                    && i.url.as_str() == first.url
                    && i.status == Status::Complete
            })
    }

    /// Open project `id`'s offline copy: its start page, in the browser.
    fn grab_open_copy(&self, id: u64) {
        let Some(p) = self.grabs.store.get(id) else {
            return;
        };
        let Some(first) = p
            .found
            .iter()
            .find(|f| f.kind == mdm_sitegrab::FoundKind::Page)
        else {
            return;
        };
        let file = self
            .items
            .iter()
            .find(|i| p.added.contains(&i.id) && i.url.as_str() == first.url)
            .map(|i| i.dest.clone());
        if let Some(file) = file {
            crate::launch::open(file);
        }
    }

    /// DDM is closing: a grab under way stops (what it found and where it stood are kept, to
    /// continue from at the next start), and the projects are written now.
    pub(super) fn grab_exit(&mut self) {
        if let Some((id, c)) = self.grabs.crawl.take() {
            c.stop();
            // It ends at once when stopped (its requests are dropped); a moment at most.
            let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !c.ended() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if let Phase::Ended(end) = c.status().phase
                && let Some(p) = self.grabs.store.get_mut(id)
            {
                if !matches!(end, End::Failed(_)) {
                    p.found = c.files();
                    p.frontier = c.frontier();
                }
                p.ended = Some(end);
                p.last_run = Some(crate::model::now_stamp());
            }
            self.grab_save();
        }
        self.grabs.flush();
    }

    /// Save the projects, forgetting downloads no longer in the list.
    fn grab_save(&mut self) {
        let alive: HashSet<u64> = self.items.iter().map(|i| i.id).collect();
        self.grabs.store.keep_only(|id| alive.contains(&id));
        self.grabs.save();
    }

    /// `/show site-grabber`: the wizard on its list of files found (screenshots);
    /// `site-grabber:start` and the like, on that page.
    pub(super) fn grab_sample(&mut self, page: &str) {
        self.grabs.dlg = Some(SiteGrabDlg::sample_on(page));
    }

    /// Off-screen test copies (tests/site_grab.rs): a new project grabbing `address` with the
    /// "Whole site for offline reading" template, as the wizard's Start does (no pause between
    /// requests: the lab's site is on this computer).
    pub(super) fn grab_test_run(&mut self, address: &str) {
        let mut p = Project {
            name: mdm_sitegrab::start_url(address)
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_default(),
            template: mdm_sitegrab::Template::Offline,
            offline: true,
            // As the template fills it in the wizard.
            folder: self.grab_default_folder(),
            ..Project::default()
        };
        p.spec.start = address.to_owned();
        mdm_sitegrab::Template::Offline.apply(&mut p.spec);
        p.spec.delay_ms = 0;
        p.spec.connections = 4;
        self.grabs.dlg = Some(SiteGrabDlg::edit(p, self.grab_default_folder()));
        self.grab_crawl(false);
    }

    /// Off-screen test copies: the open window's Download all, as its button does.
    pub(super) fn grab_test_download(&mut self) {
        let Some(d) = self.grabs.dlg.take() else {
            return;
        };
        if let Ask::Download {
            files,
            queue,
            start,
        } = d.download_all()
        {
            self.grab_add(d.project(), files, queue, start);
        }
    }
}

/// The page at an offline copy's top that opens its start page; a file of that name that isn't
/// one DDM made is left alone.
fn write_shortcut(at: &Path, name: &str, address: &str, start: &Path) {
    if let Ok(old) = std::fs::read_to_string(at)
        && !offline::is_start_page(&old)
    {
        tracing::info!("grabber: {} is someone else's: not replaced", at.display());
        return;
    }
    let Some(dir) = at.parent() else { return };
    let target = offline::relative(dir, start);
    if let Err(e) = std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(at, offline::start_page(name, address, &target)))
    {
        tracing::warn!("grabber: {} not written: {e}", at.display());
    }
}

/// One saved page or style sheet whose links are to lead to its copy's files.
struct Rewrite {
    file: PathBuf,
    url: url::Url,
    text: Text,
    copy: Arc<Copy>,
}

/// The thread that changes offline copies' files, one at a time (two changes of one file never
/// meet), started with the first.
#[derive(Default)]
struct Rewriter {
    tx: Option<mpsc::Sender<Rewrite>>,
}

impl Rewriter {
    fn send(&mut self, job: Rewrite) {
        if self.tx.is_none() {
            let (tx, rx) = mpsc::channel::<Rewrite>();
            crate::launch::spawn("grabber offline copy", move || {
                for job in rx {
                    rewrite(&job);
                }
            });
            self.tx = Some(tx);
        }
        if let Some(tx) = &self.tx
            && let Err(mpsc::SendError(job)) = tx.send(job)
        {
            // The thread is gone (it panicked): this one is done here, the next starts another.
            self.tx = None;
            rewrite(&job);
        }
    }
}

/// `job`'s file with its links leading to the copy's files: written in place (its mark as from
/// the internet stays on it); a file gone or unreadable is left.
fn rewrite(job: &Rewrite) {
    let Ok(bytes) = std::fs::read(&job.file) else {
        return;
    };
    let new = match job.text {
        Text::Html => offline::rewrite_html(&bytes, &job.url, &job.file, &job.copy),
        Text::Css => offline::rewrite_css(&bytes, &job.url, &job.file, &job.copy),
    };
    if let Some(new) = new
        && let Err(e) = std::fs::write(&job.file, new)
    {
        tracing::warn!(
            "grabber: {} not made to work offline: {e}",
            job.file.display()
        );
    }
}
