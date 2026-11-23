//! Site grabber projects (Tasks → Run site grabber…): each one's settings, the files its last run
//! found, where that run stopped when it ended early (to continue from there) and the downloads it
//! added, saved in the data folder's `grabber.json` (atomically, its owner's only: a project may
//! hold a site's password and cookies, sealed as the settings seal theirs). The Categories tree
//! lists them under "Grabber projects", and the Tasks menu the latest of them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mdm_sitegrab::{End, Found, Frontier, Spec, Template};
use serde::{Deserialize, Serialize};

use crate::model::Category;

fn yes() -> bool {
    true
}

/// One project: what the wizard was given, and what its last run found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub template: Template,
    #[serde(default)]
    pub spec: Spec,
    /// The site's login (sent to the start address's site only).
    #[serde(default)]
    pub user: String,
    #[serde(default, with = "crate::settings::sealed")]
    pub password: String,
    /// Cookies for the site, as a browser sends them (`name=value; name2=value2`): sent to the
    /// start address's site only, with its pages and its files' downloads.
    #[serde(default, with = "crate::settings::sealed")]
    pub cookies: String,
    /// Where the files go (empty: each to its category's folder).
    #[serde(default)]
    pub folder: String,
    /// Under the site's own folders (`example.com/photos/…`), else all in one.
    #[serde(default = "yes")]
    pub keep_folders: bool,
    /// Its pages are made to work offline: each file saved where its address says under the
    /// folder, and the links of its pages and style sheets made to lead to those files once they
    /// are downloaded (mdm_sitegrab::offline).
    #[serde(default)]
    pub offline: bool,
    /// Its downloads are listed under All Downloads too (else under the project's node and their
    /// queue only, as a big grab would bury the list). Projects from before the choice were.
    #[serde(default = "yes")]
    pub listed: bool,
    /// The category its downloads are listed under (None: each by its file type).
    #[serde(default)]
    pub category: Option<Category>,
    /// The queue its downloads join, and whether it starts then.
    #[serde(default)]
    pub queue: usize,
    #[serde(default = "yes")]
    pub start_now: bool,
    /// What its last run found.
    #[serde(default)]
    pub found: Vec<Found>,
    /// How its last run ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<End>,
    /// Where its last run stood when it ended early (Stop, Most pages, Most files): Continue
    /// goes on from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontier: Option<Frontier>,
    /// The downloads it added (ids in the download list): its node in the Categories tree shows
    /// them.
    #[serde(default)]
    pub added: BTreeSet<u64>,
    /// When it last ran (as the list writes dates).
    #[serde(default)]
    pub last_run: Option<String>,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            id: 0,
            name: String::new(),
            template: Template::default(),
            spec: Spec::default(),
            user: String::new(),
            password: String::new(),
            cookies: String::new(),
            folder: String::new(),
            keep_folders: true,
            offline: false,
            listed: false,
            category: None,
            queue: 0,
            start_now: true,
            found: Vec::new(),
            ended: None,
            frontier: None,
            added: BTreeSet::new(),
            last_run: None,
        }
    }
}

impl Project {
    /// The site's login, when one is given.
    pub fn login(&self) -> Option<(String, String)> {
        (!self.user.trim().is_empty()).then(|| (self.user.trim().to_owned(), self.password.clone()))
    }

    /// The cookies given for the site, when any.
    pub fn cookies(&self) -> Option<String> {
        let c = self.cookies.trim();
        (!c.is_empty()).then(|| c.to_owned())
    }

    /// Its last run ended early and can go on from where it stopped.
    pub fn can_continue(&self) -> bool {
        self.frontier.is_some()
    }

    /// Where file `found` goes under `base`: in an offline copy, where its address says
    /// (mdm_sitegrab::offline::local_path: the links of the copy's pages are made for that
    /// place); else under the site's folders when they are kept, under the name found.
    pub fn place(&self, base: &Path, found: &Found) -> PathBuf {
        if self.offline {
            return base.join(mdm_sitegrab::offline::local_path(found));
        }
        mdm_engine::naming::guess(&self.folder_for(base, found), &found.name)
    }

    /// The start page of its offline copy under `base`, once downloaded (the first page found:
    /// the start address, or where it led).
    pub fn start_file(&self, base: &Path) -> Option<PathBuf> {
        let first = self
            .found
            .iter()
            .find(|f| f.kind == mdm_sitegrab::FoundKind::Page)?;
        Some(self.place(base, first))
    }

    /// The page at the top of its offline copy's folder that opens its start page:
    /// `<folder>/<project name>.html`.
    pub fn shortcut(&self, base: &Path) -> PathBuf {
        let name = mdm_engine::naming::sanitize(&self.name);
        base.join(format!("{name}.html"))
    }

    /// The category file `name` is listed under: the project's (while it exists), else by its
    /// type.
    pub fn category_of(&self, name: &str) -> Category {
        self.category
            .filter(|c| Category::all().contains(c))
            .unwrap_or_else(|| Category::of(name))
    }

    /// The folder file `found` goes to, under `base` (the project's folder, or the file's
    /// category's): the site's own folders below it when they are kept.
    pub fn folder_for(&self, base: &Path, found: &Found) -> PathBuf {
        let mut dir = base.to_path_buf();
        if self.keep_folders
            && let Ok(url) = url::Url::parse(&found.url)
        {
            for part in mdm_sitegrab::filter::site_folders(&url) {
                dir.push(part);
            }
        }
        dir
    }
}

/// Every project, as `grabber.json` holds them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    pub projects: Vec<Project>,
    /// The Categories tree's "Grabber projects" node is open.
    #[serde(default = "yes")]
    pub open: bool,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            next_id: 1,
            projects: Vec::new(),
            open: true,
        }
    }
}

/// `grabber.json` written as `json`: atomically, its owner's only (it may hold a password).
pub fn write(path: &Path, json: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        mdm_ipc::private_dir(dir)?;
    }
    mdm_ipc::private_write(path, json)
}

impl Store {
    pub fn path() -> Option<PathBuf> {
        mdm_ipc::data_dir().map(|d| d.join("grabber.json"))
    }

    /// The saved one; a missing file is an empty store, an unreadable one is kept aside as
    /// `grabber.json.bad` (never overwritten) and the store starts empty.
    pub fn load(path: &Path) -> Store {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Store::default();
        };
        match serde_json::from_str::<Store>(text.trim_start_matches('\u{feff}')) {
            Ok(mut s) => {
                let top = s.projects.iter().map(|p| p.id).max().unwrap_or(0);
                s.next_id = s.next_id.max(top + 1);
                s
            }
            Err(e) => {
                tracing::warn!("grabber: {} unreadable ({e}): kept aside", path.display());
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                Store::default()
            }
        }
    }

    /// As `grabber.json` holds it ([`write`], on a saver thread).
    pub fn encode(&self) -> std::io::Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(std::io::Error::other)
    }

    pub fn get(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.id == id)
    }

    /// Keep `p`: a new one (id 0) gets its id, one known is replaced (what it found and added
    /// stays unless `p` brings its own). Returns its id.
    pub fn put(&mut self, mut p: Project) -> u64 {
        if p.id == 0 || self.get(p.id).is_none() {
            p.id = self.next_id.max(1);
            self.next_id = p.id + 1;
            let id = p.id;
            self.projects.push(p);
            return id;
        }
        let id = p.id;
        if let Some(old) = self.get_mut(id) {
            let added = std::mem::take(&mut old.added);
            *old = p;
            old.added.extend(added);
        }
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.projects.retain(|p| p.id != id);
    }

    /// The projects run last first (at most `n`): the Tasks menu's list.
    pub fn recent(&self, n: usize) -> Vec<&Project> {
        let mut all: Vec<&Project> = self.projects.iter().collect();
        // Newest first; a project never run (or of an unreadable date) after, the newest added
        // first among them.
        all.reverse();
        all.sort_by_key(|p| {
            std::cmp::Reverse(p.last_run.as_deref().and_then(crate::model::stamp_time))
        });
        all.truncate(n);
        all
    }

    /// The downloads of projects that keep them out of All Downloads (`Project::listed`).
    pub fn kept_apart(&self) -> std::collections::HashSet<u64> {
        self.projects
            .iter()
            .filter(|p| !p.listed)
            .flat_map(|p| p.added.iter().copied())
            .collect()
    }

    /// The project that added download `id`.
    pub fn owner(&self, id: u64) -> Option<&Project> {
        self.projects.iter().find(|p| p.added.contains(&id))
    }

    /// Forget the downloads that are no longer in the list.
    pub fn keep_only(&mut self, alive: impl Fn(u64) -> bool) {
        for p in &mut self.projects {
            p.added.retain(|id| alive(*id));
        }
    }

    /// A queue was deleted: projects that used it, or one after it, follow.
    pub fn queue_removed(&mut self, q: usize) {
        for p in &mut self.projects {
            if p.queue == q {
                p.queue = 0;
            } else if p.queue > q {
                p.queue -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(url: &str) -> Found {
        Found {
            url: url.into(),
            page: "https://example.com/".into(),
            name: url.rsplit('/').next().unwrap().into(),
            size: Some(10),
            kind: Default::default(),
            also: Vec::new(),
        }
    }

    #[test]
    fn projects_are_saved_atomically_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grabber.json");
        let mut s = Store::default();
        let id = s.put(Project {
            name: "Photos".into(),
            user: "me".into(),
            password: "pw".into(),
            found: vec![found("https://example.com/a/b.jpg")],
            ..Project::default()
        });
        assert_eq!(id, 1);
        s.get_mut(id).unwrap().added.insert(42);
        write(&path, &s.encode().unwrap()).unwrap();
        let back = Store::load(&path);
        assert_eq!(back, s);
        // A second project gets the next id, even after the first is gone.
        let mut back = back;
        back.remove(1);
        assert_eq!(back.put(Project::default()), 2);
    }

    #[test]
    fn a_broken_file_is_kept_aside_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grabber.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = Store::load(&path);
        assert!(s.projects.is_empty());
        assert!(path.with_extension("json.bad").exists());
        // An older file without the newer fields still reads.
        std::fs::write(&path, r#"{"projects":[{"id":7,"name":"Old"}]}"#).unwrap();
        let s = Store::load(&path);
        assert_eq!(s.projects[0].name, "Old");
        assert!(s.projects[0].keep_folders && s.projects[0].start_now && s.open);
        assert_eq!(s.projects[0].spec, Spec::default());
        let mut s = s;
        assert_eq!(s.put(Project::default()), 8);
    }

    #[test]
    fn editing_keeps_what_it_added_and_queues_follow_a_deletion() {
        let mut s = Store::default();
        let id = s.put(Project {
            queue: 3,
            ..Project::default()
        });
        s.get_mut(id).unwrap().added.extend([1, 2, 3]);
        let edited = Project {
            id,
            name: "Renamed".into(),
            queue: 3,
            ..Project::default()
        };
        s.put(edited);
        let p = s.get(id).unwrap();
        assert_eq!(p.name, "Renamed");
        assert_eq!(p.added.len(), 3);
        s.keep_only(|i| i != 2);
        assert_eq!(s.get(id).unwrap().added, BTreeSet::from([1, 3]));
        s.queue_removed(2);
        assert_eq!(s.get(id).unwrap().queue, 2);
        s.queue_removed(2);
        assert_eq!(s.get(id).unwrap().queue, 0);
    }

    #[test]
    fn an_offline_copys_files_go_where_their_links_expect_them() {
        let base = Path::new("sites");
        let mut p = Project {
            name: "Docs: example".into(),
            offline: true,
            // An offline copy keeps the site's folders whatever this says.
            keep_folders: false,
            ..Project::default()
        };
        let page = Found {
            kind: mdm_sitegrab::FoundKind::Page,
            name: "index.html".into(),
            ..found("https://example.com/docs/")
        };
        p.found = vec![found("https://example.com/a/b.jpg"), page.clone()];
        assert_eq!(
            p.place(base, &page),
            base.join("example.com").join("docs").join("index.html")
        );
        assert_eq!(
            p.place(base, &p.found[0]),
            base.join("example.com").join("a").join("b.jpg")
        );
        assert_eq!(
            p.start_file(base),
            Some(base.join("example.com").join("docs").join("index.html")),
            "the first page found"
        );
        assert_eq!(p.shortcut(base), base.join("Docs_ example.html"));
        // Not offline: the name found, under the folders kept or not.
        p.offline = false;
        assert_eq!(p.place(base, &page), base.join("index.html"));
    }

    #[test]
    fn projects_listed_and_kept_apart_recent_first() {
        let mut s = Store::default();
        let old = s.put(Project {
            name: "old".into(),
            listed: true,
            last_run: Some("Jan 02 10:00:00 2026".into()),
            ..Project::default()
        });
        let new = s.put(Project {
            name: "new".into(),
            last_run: Some("Nov 20 09:00:00 2026".into()),
            ..Project::default()
        });
        let never = s.put(Project {
            name: "never".into(),
            ..Project::default()
        });
        s.get_mut(old).unwrap().added.extend([1, 2]);
        s.get_mut(new).unwrap().added.extend([3]);
        assert_eq!(s.kept_apart(), std::collections::HashSet::from([3]));
        assert_eq!(s.owner(2).map(|p| p.id), Some(old));
        assert!(s.owner(9).is_none());
        let names: Vec<&str> = s.recent(10).iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["new", "old", "never"]);
        assert_eq!(s.recent(1).len(), 1);
        let _ = never;
        // A project saved before the choice existed had its downloads in All Downloads.
        let older: Project = serde_json::from_str(r#"{"id":4,"name":"x"}"#).unwrap();
        assert!(older.listed && !older.offline && older.frontier.is_none());
    }

    #[test]
    fn where_a_run_stopped_and_the_cookies_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grabber.json");
        let mut s = Store::default();
        s.put(Project {
            cookies: "session=abc".into(),
            ended: Some(mdm_sitegrab::End::MaxPages),
            frontier: Some(mdm_sitegrab::Frontier {
                waiting: vec![mdm_sitegrab::Waiting {
                    url: "https://example.com/next".into(),
                    depth: 2,
                    style: false,
                    page: None,
                }],
                seen: vec!["https://example.com/".into()],
                pages: 1,
            }),
            ..Project::default()
        });
        write(&path, &s.encode().unwrap()).unwrap();
        let back = Store::load(&path);
        assert_eq!(back, s);
        assert!(back.projects[0].can_continue());
        assert_eq!(back.projects[0].cookies().as_deref(), Some("session=abc"));
        // The cookies are sealed as the password is (for the Windows account): not written as
        // they are.
        #[cfg(windows)]
        {
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(!text.contains("session=abc"), "{text}");
        }
    }

    #[test]
    fn files_keep_the_sites_folders_when_asked() {
        let base = Path::new("downloads");
        let f = found("https://www.example.com/photos/2024/b.jpg");
        let mut p = Project::default();
        assert_eq!(
            p.folder_for(base, &f),
            base.join("www.example.com").join("photos").join("2024")
        );
        p.keep_folders = false;
        assert_eq!(p.folder_for(base, &f), base);
    }
}
