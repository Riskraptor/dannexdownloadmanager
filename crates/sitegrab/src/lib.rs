//! Site grabber: from a start address, explores a web site's pages breadth-first, a few levels
//! deep, and collects the files they link to (pictures, videos, documents…), for the app to
//! download. IDM's "Site grabber" does the same (behaviour only; DDM's own code).
//!
//! - [`extract`] reads a page's and a style sheet's links (pure).
//! - [`filter`] decides what is explored and what is collected (pure).
//! - [`robots`] reads robots.txt (pure).
//! - [`offline`] places a site's files on the disk and makes their links lead to each other, to
//!   read the site offline (pure).
//! - [`Crawl`] runs a grab on a Tokio runtime with the engine's HTTP client (the user's proxy,
//!   DNS, network adapter and User-Agent), politely: a few requests at once, a pause between
//!   two to one site, robots.txt followed unless the user says otherwise. A grab stopped (or
//!   cut short by its limits) can go on later from where it was ([`Frontier`]).

pub mod extract;
pub mod filter;
pub mod offline;
pub mod robots;

mod crawl;

pub use crawl::{
    Crawl, End, Fault, Found, FoundKind, Frontier, Invalid, Net, Phase, Status, Waiting,
};
pub use filter::{Group, Patterns, Scope, Types};

use serde::{Deserialize, Serialize};
use url::Url;

/// The most of each setting a grab takes (a hand-edited project can hold anything).
pub const MAX_DEPTH: u32 = 20;
pub const MAX_PAGES: u32 = 10_000;
pub const MAX_FILES: u32 = 50_000;
pub const MAX_CONNECTIONS: u32 = 8;
pub const MAX_DELAY_MS: u32 = 60_000;

/// What a grab explores and collects: a project's settings, as they are saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spec {
    /// The start address, as typed (`example.com/gallery` is read as `https://`).
    pub start: String,
    /// How many links away from the start page pages are explored (0: the start page only).
    pub depth: u32,
    /// Which sites pages are explored on.
    pub scope: Scope,
    /// Pages are explored under the start address's folder only (`/docs/` and below for
    /// `https://example.com/docs/intro.html`): part of a site. Files are collected wherever they
    /// are on it.
    pub below: bool,
    /// Files linked from explored pages are collected wherever they are (a site's pictures on
    /// another host), not only within `scope`.
    pub files_anywhere: bool,
    /// The file types collected: groups, and extensions of the user's own.
    pub groups: Vec<Group>,
    pub custom: String,
    /// Wildcard patterns (`Patterns`): only files matching one are collected (none: every
    /// file); addresses matching `exclude` are neither explored nor collected.
    pub include: String,
    pub exclude: String,
    /// Stop after exploring this many pages, or collecting this many files.
    pub max_pages: u32,
    pub max_files: u32,
    /// Follow the site's robots.txt (pages it disallows aren't explored).
    pub robots: bool,
    /// The pause between two requests to one site, in milliseconds (a robots.txt's
    /// `Crawl-delay` makes it longer).
    pub delay_ms: u32,
    /// Requests at once.
    pub connections: u32,
    /// Ask the server each file's size (a `HEAD` request each) once exploring is done.
    pub sizes: bool,
}

impl Default for Spec {
    fn default() -> Self {
        let mut s = Spec {
            start: String::new(),
            depth: 2,
            scope: Scope::SameHost,
            below: false,
            files_anywhere: true,
            groups: Vec::new(),
            custom: String::new(),
            include: String::new(),
            exclude: String::new(),
            max_pages: 200,
            max_files: 1000,
            robots: true,
            delay_ms: 500,
            connections: 2,
            sizes: true,
        };
        Template::Images.apply(&mut s);
        s
    }
}

impl Spec {
    /// The settings brought into their ranges.
    pub fn clamped(&self) -> Spec {
        Spec {
            depth: self.depth.min(MAX_DEPTH),
            max_pages: self.max_pages.clamp(1, MAX_PAGES),
            max_files: self.max_files.clamp(1, MAX_FILES),
            delay_ms: self.delay_ms.min(MAX_DELAY_MS),
            connections: self.connections.clamp(1, MAX_CONNECTIONS),
            ..self.clone()
        }
    }

    pub fn types(&self) -> Types {
        Types::new(&self.groups, &self.custom)
    }
}

/// A grab's starting point: what it collects and how deep it goes (IDM's site grabber offers
/// such templates; these are DDM's own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Template {
    #[default]
    Images,
    Audio,
    Video,
    Documents,
    /// The pages themselves, with what they need to show, to read the site offline.
    Offline,
    /// Whatever the user sets.
    Custom,
}

impl Template {
    pub const ALL: [Template; 6] = [
        Template::Images,
        Template::Audio,
        Template::Video,
        Template::Documents,
        Template::Offline,
        Template::Custom,
    ];

    /// Its file types and depth into `spec` (Custom changes nothing). A site saved to read
    /// offline doesn't ask each file's size first: thousands of files would wait for as many
    /// questions, and their downloads say it anyway.
    pub fn apply(self, spec: &mut Spec) {
        let (groups, depth): (&[Group], u32) = match self {
            Template::Images => (&[Group::Images], 2),
            Template::Audio => (&[Group::Audio], 2),
            Template::Video => (&[Group::Video], 2),
            Template::Documents => (&[Group::Documents], 2),
            Template::Offline => (
                &[
                    Group::Pages,
                    Group::Assets,
                    Group::Images,
                    Group::Documents,
                    Group::Audio,
                    Group::Video,
                ],
                3,
            ),
            Template::Custom => return,
        };
        spec.groups = groups.to_vec();
        spec.custom.clear();
        spec.depth = depth;
        spec.sizes = self != Template::Offline;
        if self == Template::Offline {
            spec.max_pages = spec.max_pages.max(500);
            spec.max_files = spec.max_files.max(5000);
        }
    }

    /// The template whose file types `spec` has (Custom when none has them).
    pub fn of(spec: &Spec) -> Template {
        let mut mine = spec.groups.clone();
        mine.sort_by_key(|g| *g as u8);
        mine.dedup();
        Template::ALL
            .into_iter()
            .filter(|t| *t != Template::Custom)
            .find(|t| {
                let mut s = Spec::default();
                t.apply(&mut s);
                let mut theirs = s.groups;
                theirs.sort_by_key(|g| *g as u8);
                theirs == mine && spec.custom.trim().is_empty()
            })
            .unwrap_or(Template::Custom)
    }
}

/// The start address as typed, made a web address: `https://` when it names no scheme; only
/// `http` and `https` with a host.
pub fn start_url(typed: &str) -> Option<Url> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let with_scheme = if typed.contains("://") {
        typed.to_owned()
    } else {
        format!("https://{typed}")
    };
    let mut url = Url::parse(&with_scheme).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_addresses() {
        assert_eq!(
            start_url(" example.com/gallery#x ").map(|u| u.to_string()),
            Some("https://example.com/gallery".into())
        );
        assert_eq!(
            start_url("http://127.0.0.1:8080/").map(|u| u.to_string()),
            Some("http://127.0.0.1:8080/".into())
        );
        assert!(start_url("ftp://example.com/").is_none());
        assert!(start_url("").is_none());
        assert!(start_url("https://").is_none());
    }

    #[test]
    fn templates_set_the_types_and_are_recognised_again() {
        for t in Template::ALL {
            let mut s = Spec::default();
            t.apply(&mut s);
            let want = if t == Template::Custom {
                Template::Images
            } else {
                t
            };
            assert_eq!(Template::of(&s), want, "{t:?}");
        }
        let mut s = Spec {
            custom: "psd".into(),
            ..Default::default()
        };
        assert_eq!(Template::of(&s), Template::Custom);
        s.custom.clear();
        s.groups = vec![Group::Images, Group::Audio];
        assert_eq!(Template::of(&s), Template::Custom);
    }

    #[test]
    fn a_saved_spec_reads_back_and_an_older_one_takes_the_defaults() {
        let s = Spec {
            start: "https://example.com/".into(),
            scope: Scope::SameDomain,
            ..Spec::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Spec>(&json).unwrap(), s);
        let old: Spec = serde_json::from_str(r#"{"start":"x.org"}"#).unwrap();
        assert_eq!(old.depth, Spec::default().depth);
        let wild = Spec {
            connections: 99,
            max_pages: 0,
            depth: 1000,
            ..Spec::default()
        }
        .clamped();
        assert_eq!((wild.connections, wild.max_pages, wild.depth), (8, 1, 20));
    }
}
