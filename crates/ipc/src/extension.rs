//! The browser extension's identity and whether a browser has it, shared by the native messaging
//! host (which serves it and notes each browser that starts it) and the app (which offers to
//! install it until one has).

/// The extension's ID loaded unpacked (the public key in extension/manifest.json).
pub const UNPACKED_ID: &str = "nphkobgebehllokfnmfdchmcbolllkif";
/// Its ID in the Chrome Web Store, which signs it with the store's own key
/// (tools/extension/pack.ps1 leaves the manifest's key out of the store's copy). [`store_id`] is
/// `None` while this is not a valid ID (a placeholder), and then nothing names it. Submitted to
/// the store on 2026-11-22 and listed once its review passed ([`STORE_LISTED`]): the native host
/// serves it, and the app, the website and the browsers offer it.
pub const STORE_ID: &str = "plmckdlgphkcfkifahbkonbibfedckih";
/// Whether the store shows the listing (its review passed): the app and the website then link to
/// it, and the browsers are offered it ([`store_page`], [`listed_store_id`]). Until then the app
/// offers the bundled (unpacked) extension and its tutorial.
pub const STORE_LISTED: bool = true;
/// Its IDs in Microsoft Edge Add-ons and Opera add-ons, which give their own; placeholders until
/// listed there, never served then (`valid_id`).
pub const OTHER_STORE_IDS: [&str; 2] = ["EDGE_ADDONS_ID_PENDING", "OPERA_ADDONS_ID_PENDING"];
/// The Firefox add-on's page on addons.mozilla.org, once it is listed there (Firefox then installs
/// and updates it from there); empty until then, and [`firefox_page`] is `None`.
pub const FIREFOX_PAGE: &str = "";

/// The Firefox add-on's page, once listed.
pub fn firefox_page() -> Option<&'static str> {
    (!FIREFOX_PAGE.is_empty()).then_some(FIREFOX_PAGE)
}
/// The Firefox add-on's ID (extension/manifest.firefox.json).
pub const FIREFOX_ID: &str = "integration@dannexdownloadmanager.com";
/// Chrome's update service: an external-install entry naming it makes Chrome offer the store's
/// extension at its next start (developer.chrome.com, "Alternative extension distribution
/// options").
pub const STORE_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";
/// Helium's name for the Chrome Web Store's update service (its proxy-extension-downloads.patch,
/// `kChromeWebstoreUpdateURL`): an entry naming it goes through Helium's own services, while
/// Google's address, asked directly without the browser's version, answers "no update" (#685).
pub const HELIUM_STORE_UPDATE_URL: &str = "https://update-url-to-be-replaced.qjz9zk";

/// A Chromium extension ID: 32 letters from a to p.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// The store's ID, once it is a real one (served by the native host from then on).
pub fn store_id() -> Option<&'static str> {
    valid_id(STORE_ID).then_some(STORE_ID)
}

/// The store's ID once its listing is public ([`STORE_LISTED`]): what may be linked and offered.
pub fn listed_store_id() -> Option<&'static str> {
    store_id().filter(|_| STORE_LISTED)
}

/// The Chromium extensions the native host serves: unpacked, and each store's once it has an ID
/// (before its listing is public, so an install from the store reaches DDM at once).
pub fn chromium_ids() -> Vec<&'static str> {
    std::iter::once(UNPACKED_ID)
        .chain(store_id())
        .chain(OTHER_STORE_IDS.into_iter().filter(|id| valid_id(id)))
        .collect()
}

/// The extension's page in the Chrome Web Store (Chrome, Edge, Brave, Opera and Vivaldi all
/// install from it), once it is listed: the listing's own address, its name then its ID.
pub fn store_page() -> Option<String> {
    listed_store_id()
        .map(|id| format!("https://chromewebstore.google.com/detail/dannex-download-manager/{id}"))
}

/// The tutorial that adds the extension: from the stores, and by hand (a build of its own, or
/// a browser that can't use its store). Its file in docs/: the app opens it on the website, in
/// the interface language (the app's help::page_url).
pub const GUIDE_DOC: &str = "guide-catch-browser-downloads.md";

/// The help page for a browser that doesn't reach Dannex Download Manager, at the section for
/// `why` (the native host's reasons, and the app's own checks): its file in docs/ and the
/// section, as GUIDE_DOC.
pub fn help_doc(why: &str) -> String {
    format!("browser-help.md#{why}")
}

/// Which copy of the extension a browser started the host for, by the caller it names (a
/// Chromium extension's ID, Firefox's add-on ID): `store` (the Chrome Web Store's), `unpacked`
/// (the bundled one, loaded from DDM's folder), `edge-store` and `opera-store` once those stores
/// give theirs, `firefox`; "" for none of ours. Safari's has no caller to name (the host's
/// `--safari`): `safari`.
pub fn copy_of(id: &str) -> &'static str {
    if id == UNPACKED_ID {
        "unpacked"
    } else if store_id() == Some(id) {
        "store"
    } else if id == FIREFOX_ID {
        "firefox"
    } else if valid_id(id) && id == OTHER_STORE_IDS[0] {
        "edge-store"
    } else if valid_id(id) && id == OTHER_STORE_IDS[1] {
        "opera-store"
    } else {
        ""
    }
}

/// Marks the data folder once a browser has started the host for the extension.
const SEEN: &str = "extension-seen";

/// One line of `extension-seen`: a browser (`capture::browser_named`) whose extension reached
/// the app, the extension's version (empty when unknown), which copy it is ([`copy_of`], empty
/// from a host before copies were told apart) and the day (since 1970) it was last seen (0 when
/// unknown).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    pub browser: String,
    pub version: String,
    pub copy: String,
    pub day: u64,
}

/// Today, in days since 1970 (UTC).
fn today() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400)
}

/// The extension's version as its message names it, if it is one (1 to 32 of `0-9 A-Z a-z . + -`,
/// as "0.6.5" or "0.7.0-rc.1"), else none: it goes into a file of tab-separated lines, where a tab
/// or a line break would add a field or a line of the sender's choosing.
fn clean_version(version: &str) -> &str {
    let ok = (1..=32).contains(&version.len())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'));
    if ok { version } else { "" }
}

/// The host, started by a browser for the extension: the extension is installed there. The
/// file holds one line per browser and copy, with the extension's version and the day; it is
/// written only when one of them changed (at most once a day per line, then). The browser's
/// name and the copy are the host's own words; the version is the message's, kept only if it is
/// one (`clean_version`).
pub fn mark_seen(browser: &str, version: &str, copy: &str) {
    let version = clean_version(version);
    let Some(dir) = crate::data_dir() else {
        return;
    };
    let file = dir.join(SEEN);
    let known = std::fs::read_to_string(&file).ok();
    let lines = known.as_deref().map(parse_seen).unwrap_or_default();
    let Some(lines) = seen_with(lines, browser, version, copy, today()) else {
        if known.is_none() {
            // The mark itself: a browser that names itself in no way the app knows.
            let _ = crate::private_dir(&dir);
            let _ = std::fs::write(file, "");
        }
        return;
    };
    let text: String = lines
        .iter()
        .map(|s| format!("{}\t{}\t{}\t{}\n", s.browser, s.version, s.copy, s.day))
        .collect();
    let _ = crate::private_dir(&dir);
    let _ = std::fs::write(file, text);
}

/// `lines` with `browser`'s `copy` seen on `day` at `version`, or None when that changes
/// nothing to keep. A line of the same browser from a host that didn't tell copies apart is the
/// same install: replaced.
fn seen_with(
    mut lines: Vec<Seen>,
    browser: &str,
    version: &str,
    copy: &str,
    day: u64,
) -> Option<Vec<Seen>> {
    if browser.is_empty() {
        return None;
    }
    let entry = Seen {
        browser: browser.to_owned(),
        version: version.to_owned(),
        copy: copy.to_owned(),
        day,
    };
    if lines.contains(&entry) {
        return None;
    }
    lines.retain(|s| !(s.browser == browser && (s.copy == copy || s.copy.is_empty())));
    lines.push(entry);
    Some(lines)
}

/// The lines of `extension-seen`. An older file holds a browser's name and version only, or
/// only its name.
fn parse_seen(text: &str) -> Vec<Seen> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.split('\t');
            Seen {
                browser: parts.next().unwrap_or_default().to_owned(),
                version: parts.next().unwrap_or_default().to_owned(),
                copy: parts.next().unwrap_or_default().to_owned(),
                day: parts.next().and_then(|d| d.parse().ok()).unwrap_or(0),
            }
        })
        .collect()
}

/// Has any browser's extension reached the app?
pub fn seen() -> bool {
    crate::data_dir().is_some_and(|d| d.join(SEEN).exists())
}

fn seen_lines() -> Vec<Seen> {
    crate::data_dir()
        .and_then(|d| std::fs::read_to_string(d.join(SEEN)).ok())
        .as_deref()
        .map(parse_seen)
        .unwrap_or_default()
}

/// The browsers whose extension has reached the app, each once, with the version of its copy
/// seen last (empty when unknown).
pub fn seen_browsers() -> Vec<(String, String)> {
    let mut out: Vec<(String, String, u64)> = Vec::new();
    for s in seen_lines() {
        match out.iter_mut().find(|(b, _, _)| *b == s.browser) {
            Some(known) if s.day >= known.2 => *known = (s.browser, s.version, s.day),
            Some(_) => {}
            None => out.push((s.browser, s.version, s.day)),
        }
    }
    out.into_iter().map(|(b, v, _)| (b, v)).collect()
}

/// A copy not seen for this many days while another copy of the same browser was: gone (the
/// user moved from one to the other), and no longer listed by [`seen_copies`].
const SUPERSEDED_DAYS: u64 = 30;

/// Each copy of the extension that has reached the app, per browser (Test browser integration
/// names them), without those another copy of the same browser replaced long since.
pub fn seen_copies() -> Vec<Seen> {
    current_copies(seen_lines())
}

fn current_copies(lines: Vec<Seen>) -> Vec<Seen> {
    let newest = |browser: &str| {
        lines
            .iter()
            .filter(|s| s.browser == browser)
            .map(|s| s.day)
            .max()
            .unwrap_or(0)
    };
    lines
        .iter()
        .filter(|s| s.day + SUPERSEDED_DAYS >= newest(&s.browser))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_chromium_ids() {
        assert!(valid_id(UNPACKED_ID));
        assert!(!valid_id("STORE_ID_PENDING") && !valid_id("abc"));
        // A placeholder store ID is never served, registered or linked; a real one is served
        // at once, and linked and offered only once its listing is public.
        assert_eq!(store_id().is_some(), valid_id(STORE_ID));
        assert!(chromium_ids().iter().all(|id| valid_id(id)));
        assert_eq!(
            listed_store_id().is_some(),
            valid_id(STORE_ID) && STORE_LISTED
        );
        assert_eq!(store_page().is_some(), listed_store_id().is_some());
    }

    /// The unpacked copy (the one the app installs) always reaches the host; a store's copy once
    /// it has its ID, before the listing is public (the manifests' allowed_origins and the host's
    /// own caller check are this list).
    #[test]
    fn unpacked_served_store_once_listed() {
        assert_eq!(UNPACKED_ID, "nphkobgebehllokfnmfdchmcbolllkif");
        let ids = chromium_ids();
        assert!(ids.contains(&UNPACKED_ID), "{ids:?}");
        assert_eq!(ids.contains(&STORE_ID), valid_id(STORE_ID));
        assert_eq!(firefox_page().is_some(), !FIREFOX_PAGE.is_empty());
    }

    #[test]
    fn seen_lines_old_and_new() {
        let chrome = |version: &str, copy: &str, day: u64| Seen {
            browser: "Google Chrome".into(),
            version: version.into(),
            copy: copy.into(),
            day,
        };
        assert_eq!(parse_seen("Google Chrome\n"), vec![chrome("", "", 0)]);
        assert_eq!(
            parse_seen("Google Chrome\t0.2.5\nMozilla Firefox\t0.2.5\n").len(),
            2
        );
        assert_eq!(
            parse_seen("Google Chrome\t0.6.5\tstore\t20400\n"),
            vec![chrome("0.6.5", "store", 20400)]
        );
        // A line from a host before copies: the same install, replaced by the copy it is.
        let lines = seen_with(
            parse_seen("Google Chrome\t0.6.4\n"),
            "Google Chrome",
            "0.6.5",
            "unpacked",
            20400,
        )
        .unwrap();
        assert_eq!(lines, vec![chrome("0.6.5", "unpacked", 20400)]);
        // Seen again the same day at the same version: nothing to write.
        assert_eq!(
            seen_with(lines.clone(), "Google Chrome", "0.6.5", "unpacked", 20400),
            None
        );
        // The store's copy beside it (another profile, or both in one): both kept, each its own.
        let both = seen_with(lines, "Google Chrome", "0.6.5", "store", 20401).unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(current_copies(both.clone()).len(), 2);
        // The copy from DDM's folder not seen for long while the store's is: no longer listed.
        let later = seen_with(both, "Google Chrome", "0.6.6", "store", 20401 + 31).unwrap();
        assert_eq!(current_copies(later), vec![chrome("0.6.6", "store", 20432)]);
        assert_eq!(seen_with(Vec::new(), "", "0.6.5", "store", 1), None);
    }

    /// A version as the extension names it, or none: never a tab, a line break or anything long
    /// in the tab-separated file.
    #[test]
    fn only_a_version_is_kept() {
        for v in ["0.6.5", "1.2.3.4", "0.7.0-rc.1", "2.0+build.5"] {
            assert_eq!(clean_version(v), v);
        }
        for v in [
            "0.6.5\tstore",
            "0.6.5\nX\t1",
            "",
            "a b",
            "é",
            &"9".repeat(33),
        ] {
            assert_eq!(clean_version(v), "", "{v:?}");
        }
    }

    /// Each caller the host serves is one copy, by its ID.
    #[test]
    fn copies_by_their_caller() {
        assert_eq!(copy_of(UNPACKED_ID), "unpacked");
        assert_eq!(copy_of(FIREFOX_ID), "firefox");
        assert_eq!(
            copy_of(STORE_ID),
            if valid_id(STORE_ID) { "store" } else { "" }
        );
        assert_eq!(copy_of("EDGE_ADDONS_ID_PENDING"), "");
        assert_eq!(copy_of("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"), "");
    }
}
