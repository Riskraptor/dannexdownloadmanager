//! Resume state: `<file>.ddm.json` beside the part file (notes/ENGINE.md §3).
//!
//! Request headers (cookies, authorization) are deliberately not stored: whoever resumes the
//! download supplies them again, so no credentials sit in the download folder.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::planner::SavedSegment;

const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeState {
    pub version: u32,
    /// The URL the user asked for.
    pub url: Url,
    /// The URL after redirects; segments are fetched from here.
    pub final_url: Url,
    /// The finished file's path (the part file is `naming::part_path(dest)`).
    pub dest: PathBuf,
    pub size: u64,
    /// What identifies the version of the file the data so far was fetched under.
    #[serde(default)]
    pub validators: Validators,
    /// Before `validators`: the one `If-Range` value used (read once, then migrated).
    #[serde(default, rename = "validator", skip_serializing)]
    legacy_validator: Option<String>,
    /// Bytes `[start, cursor)` of each segment are on disk.
    pub segments: Vec<SavedSegment>,
    /// The `Content-Type` the file was first served with (an HTML answer to a file that wasn't
    /// HTML is a sign-in wall, not data).
    #[serde(default)]
    pub content_type: Option<String>,
    /// The hash the finished file must have (the user's or the server's), if known.
    #[serde(default)]
    pub checksum: Option<crate::verify::Hash>,
    /// The first run got past a refusal by asking with `Options::agent_fallback` instead of the
    /// browser's User-Agent (#372): a resume asks the same way. Saved as `asked_as_ddm`; a state
    /// from Dannex up to 0.6.5 says `asked_as_mdm`.
    #[serde(
        default,
        rename = "asked_as_ddm",
        alias = "asked_as_mdm",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub asked_as_mdm: bool,
    /// The first run asked as DDM itself (`agent::OWN_USER_AGENT`): the server hung up on the
    /// browser's User-Agent without answering. A resume asks the same way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asked_as_own: bool,
    /// A POST download (`Options::post`, its body not stored) whose redirects turned it into
    /// a GET (301, 302, 303): `final_url` is asked with GET.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub final_get: bool,
    /// Only the browser-like connection got past the site's bot check (#668): a resume goes on
    /// through it, on one connection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub browser_like: bool,
    /// Where the part file is when it isn't beside the file (Options → Save to → Temporary
    /// directory, `naming::part_path`): a resume after that setting changed finds it, and brings
    /// it to the new place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<PathBuf>,
}

impl ResumeState {
    pub fn new(url: Url, final_url: Url, dest: PathBuf, size: u64, validators: Validators) -> Self {
        ResumeState {
            version: VERSION,
            url,
            final_url,
            dest,
            size,
            validators,
            legacy_validator: None,
            segments: Vec::new(),
            content_type: None,
            checksum: None,
            asked_as_mdm: false,
            asked_as_own: false,
            final_get: false,
            browser_like: false,
            part: None,
        }
    }

    /// Keep where the part file is (`part`): None when it is beside the file, as before.
    pub fn set_part(&mut self, part: &Path) {
        self.part = (part != crate::naming::beside_part_path(&self.dest)).then(|| part.to_owned());
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut s: ResumeState =
            serde_json::from_slice(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(v) = s.legacy_validator.take()
            && s.validators == Validators::default()
        {
            s.validators = Validators::migrate(&v, s.final_url.host_str());
        }
        if s.version != VERSION {
            return Err(format!(
                "{}: unsupported version {}",
                path.display(),
                s.version
            ));
        }
        Ok(s)
    }

    /// Persist with `segments`. `part` is flushed first, so the saved positions never claim
    /// bytes that are not on disk yet — even across a power loss.
    pub fn save(
        &mut self,
        path: &Path,
        segments: Vec<SavedSegment>,
        part: &File,
    ) -> io::Result<()> {
        part.sync_data()?;
        self.segments = segments;
        self.write(path)
    }

    /// Write as is (its segments must already be on disk).
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        craft_atomic::atomic_write(path, &json)
    }

    /// Bytes already on disk.
    pub fn downloaded(&self) -> u64 {
        self.segments.iter().map(|s| s.cursor - s.start).sum()
    }
}

/// What identifies a version of the file, for `If-Range` (RFC 9110 §13.1.5).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Validators {
    /// A strong ETag, and the host that issued it (CDN nodes may each have their own).
    pub etag: Option<String>,
    pub etag_host: Option<String>,
    /// Last-Modified, kept only when it is a strong validator (`Date` at least 1 s later).
    pub last_modified: Option<String>,
}

impl Validators {
    /// The validators a response offers. A weak ETag means none at all: the server says its
    /// bytes may differ between equivalent versions, and a date may then not stand in for the
    /// ETag (RFC 9110 §13.1.5: no date in If-Range while the client has an entity tag). A
    /// malformed one (unquoted, as Wikimedia's file servers send) is no entity tag at all: left
    /// out, and the date, when strong, still guards a resume against a file changed meanwhile.
    pub fn from_response(
        etag: Option<&str>,
        last_modified: Option<&str>,
        date: Option<&str>,
        host: Option<&str>,
    ) -> Self {
        let etag = match etag.map(str::trim) {
            Some(e) if is_strong_etag(e) => Some(e.to_owned()),
            Some(e) if e.starts_with("W/") => return Validators::default(),
            Some(_) | None => None,
        };
        let last_modified = last_modified
            .filter(|lm| is_strong_date(lm, date))
            .map(str::to_owned);
        Validators {
            etag_host: etag.as_ref().and(host.map(str::to_owned)),
            etag,
            last_modified,
        }
    }

    /// The `If-Range` value for a request to `host`: the ETag only to the host that issued it,
    /// else the date.
    pub fn if_range(&self, host: Option<&str>) -> Option<&str> {
        self.etag
            .as_deref()
            .filter(|_| self.etag_host.as_deref() == host)
            .or(self.last_modified.as_deref())
    }

    /// From the single value older versions stored.
    fn migrate(v: &str, host: Option<&str>) -> Self {
        if is_strong_etag(v) {
            Validators {
                etag: Some(v.to_owned()),
                etag_host: host.map(str::to_owned),
                last_modified: None,
            }
        } else {
            Validators {
                last_modified: httpdate::parse_http_date(v).is_ok().then(|| v.to_owned()),
                ..Validators::default()
            }
        }
    }
}

/// `"…"` without inner quotes (a weak `W/"…"` or an unquoted value is not).
fn is_strong_etag(e: &str) -> bool {
    e.len() >= 2 && e.starts_with('"') && e.ends_with('"') && !e[1..e.len() - 1].contains('"')
}

/// A Last-Modified is strong only if the response's `Date` is at least a second later (RFC
/// 9110 §8.8.2.2): a file modified within the second it was served may change again unseen.
fn is_strong_date(last_modified: &str, date: Option<&str>) -> bool {
    let (Ok(lm), Some(Ok(date))) = (
        httpdate::parse_http_date(last_modified),
        date.map(httpdate::parse_http_date),
    ) else {
        return false;
    };
    date >= lm + std::time::Duration::from_secs(1)
}

/// `<dest>.ddm.json`.
pub fn state_path(dest: &Path) -> PathBuf {
    crate::naming::beside(dest, ".ddm.json")
}

/// A download's state and part files under their former names, newest first: `<dest>.mdm.json`
/// and `<dest>.mdm.part` (Dannex up to 0.6.5, and Mister Download Manager, whose code DDM's
/// is), then `.idmnext.json` and `.idmnext.part` (the app's first codename).
fn legacy_pairs(dest: &Path) -> [(PathBuf, PathBuf); 2] {
    use crate::naming::{beside, legacy_part_path};
    [
        (beside(dest, ".mdm.json"), legacy_part_path(dest)),
        (beside(dest, ".idmnext.json"), beside(dest, ".idmnext.part")),
    ]
}

/// Every former name of `dest`'s state and part files ([`legacy_pairs`]): Delete with its files
/// and Redownload remove them with today's ones.
pub fn legacy_files(dest: &Path) -> Vec<PathBuf> {
    legacy_pairs(dest)
        .into_iter()
        .flat_map(|(state, part)| [state, part])
        .collect()
}

/// Whether `state_file` is `dest`'s state file under a former name (`<dest>.mdm.json`): what
/// `dannexdownloadmanager-cli resume` may be given for a download from before.
pub fn is_legacy_state(dest: &Path, state_file: &Path) -> bool {
    legacy_pairs(dest)
        .iter()
        .any(|(state, _)| state.as_path() == state_file)
}

/// A download whose files have a former name ([`legacy_pairs`]: `.mdm.json`, `.mdm.part`)
/// takes today's (`.ddm.json`, `.ddm.part`), so it resumes and nothing on the disk keeps the old
/// name. Only while today's names are free: a file of either is never replaced. Returns true
/// when something was renamed. Called wherever a download's state is looked for: the list as it
/// loads (every unfinished row: one look per former name when there is nothing to rename), a
/// start, a resume, the helpers' own downloads. A part file without its state can't resume
/// under any name: it keeps its own (Delete with its files removes it, `legacy_files`).
pub fn migrate_legacy(dest: &Path) -> bool {
    for (old_state, old_part) in legacy_pairs(dest) {
        if !old_state.exists() {
            continue;
        }
        let (new_state, new_part) = (state_path(dest), crate::naming::part_path(dest));
        if new_state.exists() || new_part.exists() {
            return false;
        }
        // Part first: a state file without its part would be rejected on resume anyway. None
        // at all: the download completed, its state left by a crash (`run_resume` sees to it).
        let part = old_part.exists();
        if part && std::fs::rename(&old_part, &new_part).is_err() {
            return false;
        }
        if std::fs::rename(&old_state, &new_state).is_err() {
            // Both under one name or neither: the part goes back beside its state.
            if part {
                let _ = std::fs::rename(&new_part, &old_part);
            }
            return false;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod migrate_tests {
    use super::*;

    #[test]
    fn legacy_files_are_renamed_once() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        std::fs::write(dir.path().join("a.zip.idmnext.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("a.zip.idmnext.part"), b"data").unwrap();
        assert!(migrate_legacy(&dest));
        assert!(state_path(&dest).exists());
        assert_eq!(
            std::fs::read(crate::naming::part_path(&dest)).unwrap(),
            b"data"
        );
        assert!(!dir.path().join("a.zip.idmnext.json").exists());
        assert!(!migrate_legacy(&dest), "nothing left to migrate");
    }

    /// Dannex up to 0.6.5 wrote `.mdm.json` and `.mdm.part`: they become `.ddm.json` and
    /// `.ddm.part`, the state's `asked_as_mdm` read as before.
    #[test]
    fn mdm_files_become_ddm_files() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let url: Url = "http://x/a.zip".parse().unwrap();
        let mut old = ResumeState::new(url.clone(), url, dest.clone(), 4, Validators::default());
        old.asked_as_mdm = true;
        let text = serde_json::to_string(&old)
            .unwrap()
            .replace("asked_as_ddm", "asked_as_mdm");
        assert!(text.contains("\"asked_as_mdm\":true"), "{text}");
        std::fs::write(dir.path().join("a.zip.mdm.json"), text).unwrap();
        std::fs::write(dir.path().join("a.zip.mdm.part"), b"data").unwrap();
        assert!(is_legacy_state(&dest, &dir.path().join("a.zip.mdm.json")));
        assert!(!is_legacy_state(&dest, &state_path(&dest)));

        assert!(migrate_legacy(&dest));
        assert_eq!(state_path(&dest), dir.path().join("a.zip.ddm.json"));
        let loaded = ResumeState::load(&state_path(&dest)).unwrap();
        assert!(loaded.asked_as_mdm);
        assert_eq!(
            std::fs::read(dir.path().join("a.zip.ddm.part")).unwrap(),
            b"data"
        );
        assert!(legacy_files(&dest).iter().all(|f| !f.exists()));
        assert!(!migrate_legacy(&dest), "nothing left to migrate");
        // Saved again, it says the new name.
        let again = serde_json::to_string(&loaded).unwrap();
        assert!(again.contains("\"asked_as_ddm\":true") && !again.contains("asked_as_mdm"));
    }

    /// Today's files are never replaced by former ones; a former state alone (the download
    /// completed, a crash left it) takes today's name; a former part alone stays as it is.
    #[test]
    fn todays_files_win_and_a_state_alone_is_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("b.bin");
        std::fs::write(dir.path().join("b.bin.ddm.part"), b"new").unwrap();
        std::fs::write(dir.path().join("b.bin.mdm.part"), b"old").unwrap();
        std::fs::write(dir.path().join("b.bin.mdm.json"), b"{}").unwrap();
        assert!(!migrate_legacy(&dest));
        assert_eq!(
            std::fs::read(crate::naming::part_path(&dest)).unwrap(),
            b"new"
        );
        assert!(dir.path().join("b.bin.mdm.json").exists());

        let done = dir.path().join("c.bin");
        std::fs::write(dir.path().join("c.bin.mdm.json"), b"{}").unwrap();
        assert!(migrate_legacy(&done));
        assert!(state_path(&done).exists() && !crate::naming::part_path(&done).exists());

        let lone = dir.path().join("d.bin");
        std::fs::write(dir.path().join("d.bin.mdm.part"), b"x").unwrap();
        assert!(!migrate_legacy(&lone));
        assert!(crate::naming::has_part(&lone), "its name stays taken");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let part = File::create(dir.path().join("a.bin.part")).unwrap();
        let url: Url = "http://x/a.bin".parse().unwrap();
        let v = Validators::from_response(Some("\"e\""), None, None, Some("x"));
        let mut s = ResumeState::new(url.clone(), url, dest.clone(), 100, v);
        let segs = vec![
            SavedSegment {
                start: 0,
                cursor: 10,
                end: 50,
            },
            SavedSegment {
                start: 50,
                cursor: 60,
                end: 100,
            },
        ];
        s.save(&state_path(&dest), segs, &part).unwrap();
        let loaded = ResumeState::load(&state_path(&dest)).unwrap();
        assert_eq!(loaded, s);
        assert_eq!(loaded.downloaded(), 20);
    }

    const LM: &str = "Wed, 21 Oct 2015 07:28:00 GMT";
    const LATER: &str = "Wed, 21 Oct 2015 07:28:01 GMT";

    #[test]
    fn a_weak_etag_means_no_validator() {
        let v = Validators::from_response(Some("W/\"x\""), Some(LM), Some(LATER), Some("h"));
        assert_eq!(v, Validators::default());
    }

    /// A malformed ETag (Wikimedia's file servers send theirs unquoted) is no entity tag: left
    /// out, the date standing in for it when strong; none without one.
    #[test]
    fn a_malformed_etag_leaves_the_date() {
        for etag in ["5fa5dc1dbaef0db2638780fa5f8c8342", "\"a\"b\"", "\""] {
            let v = Validators::from_response(Some(etag), Some(LM), Some(LATER), Some("h"));
            assert_eq!(v.etag, None, "{etag}");
            assert_eq!(v.if_range(Some("h")), Some(LM), "{etag}");
            let no_date = Validators::from_response(Some(etag), Some(LM), Some(LM), Some("h"));
            assert_eq!(no_date, Validators::default(), "{etag}");
        }
    }

    #[test]
    fn a_date_counts_only_when_served_a_second_later() {
        let strong = Validators::from_response(None, Some(LM), Some(LATER), Some("h"));
        assert_eq!(strong.if_range(Some("h")), Some(LM));
        let same_second = Validators::from_response(None, Some(LM), Some(LM), Some("h"));
        assert_eq!(same_second.if_range(Some("h")), None);
        let no_date = Validators::from_response(None, Some(LM), None, Some("h"));
        assert_eq!(no_date.if_range(Some("h")), None);
    }

    #[test]
    fn the_etag_goes_only_to_its_host() {
        let v = Validators::from_response(Some("\"e\""), Some(LM), Some(LATER), Some("cdn1"));
        assert_eq!(v.if_range(Some("cdn1")), Some("\"e\""));
        assert_eq!(v.if_range(Some("cdn2")), Some(LM));
    }

    #[test]
    fn the_old_single_validator_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin.mdm.json");
        let old = r#"{"version":1,"url":"http://x/a","final_url":"http://cdn/a","dest":"a.bin",
            "size":5,"validator":"\"e\"","segments":[]}"#;
        std::fs::write(&p, old).unwrap();
        let s = ResumeState::load(&p).unwrap();
        assert_eq!(s.validators.if_range(Some("cdn")), Some("\"e\""));
    }

    #[test]
    fn rejects_garbage_and_other_versions() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.json");
        std::fs::write(&p, b"{not json").unwrap();
        assert!(ResumeState::load(&p).is_err());
        std::fs::write(&p, br#"{"version":99}"#).unwrap();
        assert!(ResumeState::load(&p).is_err());
    }
}
