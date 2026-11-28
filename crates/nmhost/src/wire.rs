//! The native messages: what the extension sends (`Message` and its asks) and what the host answers
//! (`Reply`), read and written as the browsers frame them (a u32 length, then JSON).

use super::*;

/// A message from the extension.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Message {
    /// Only asking for the capture rules (the file types to offer). `hello` (the extension
    /// installed, or the browser started) asks the same and tells the app it is there.
    pub(super) rules: bool,
    pub(super) hello: bool,
    /// The extension's version.
    pub(super) v: Option<String>,
    pub(super) url: String,
    pub(super) referrer: Option<String>,
    pub(super) cookies: Option<String>,
    /// The same cookies with their domains and flags (`mdm_ipc::Command::cookie_jar`); an older
    /// extension sends none.
    pub(super) cookie_jar: Vec<mdm_ipc::BrowserCookie>,
    pub(super) user_agent: Option<String>,
    /// `navigator.userAgentData` brands: Brave and Opera name themselves there, not in the
    /// User-Agent; Firefox's forks add the name `runtime.getBrowserInfo` gives ("Zen").
    pub(super) brands: Vec<String>,
    pub(super) filename: Option<String>,
    /// Where the browser's redirects from `url` ended.
    pub(super) final_url: Option<String>,
    /// The browser's own download: its size (bytes, when known) and type.
    pub(super) size: Option<u64>,
    pub(super) mime: Option<String>,
    /// A form's download: the body to post again, and its Content-Type.
    pub(super) post: Option<String>,
    pub(super) post_type: Option<String>,
    /// From a private window.
    pub(super) private: bool,
    /// The browser's proxy for `url` (#42): its address, or its auto-config script, and whether it
    /// asked for a login (see mdm_ipc::Command).
    pub(super) proxy: Option<String>,
    pub(super) proxy_pac: Option<String>,
    pub(super) proxy_login: bool,
    /// The user asked for this download (right-click → Download with Dannex Download Manager):
    /// Options' automatic-capture rules don't apply, as IDM's "Download with IDM" takes any link.
    pub(super) explicit: bool,
    /// May the download button show over videos on page `url`? With `referrer`: the tab's
    /// address, when `url` is a frame embedded from another page (its site counts too, #532).
    pub(super) panel: bool,
    /// The download button was clicked: `url` is the video's page, `src` the video's own
    /// address when the page plays a plain file (not a stream assembled in the page), or the
    /// manifest of the stream its player loaded. With `explicit`, the toolbar button's list of
    /// the tab's media asked for it (#38).
    pub(super) video: bool,
    pub(super) src: Option<String>,
    /// The page's title (`video`).
    pub(super) title: Option<String>,
    /// The button's menu: keep it off this site (a host name).
    pub(super) hide_site: Option<String>,
    /// The button's menu: open its settings in Options (`page`: the status page asks for
    /// `general`, when DDM's Options take nothing from the browser).
    pub(super) open_options: bool,
    pub(super) page: Option<String>,
    /// The right-click menu: these links of a page or its selection.
    pub(super) links: Vec<String>,
    /// The text the page shows for each of `links`, in their order (an older extension sends
    /// none).
    pub(super) texts: Vec<String>,
    /// The toolbar button: leave this site (a host name) to the browser.
    pub(super) exclude_site: Option<String>,
    /// Bring Dannex Download Manager's window up: the toolbar button's Open Dannex Download
    /// Manager of extensions up to 0.6.5 (the extension no longer has it); kept so theirs still
    /// does what it says with this host.
    pub(super) open: bool,
    /// The extension left a download to the browser by Options' rules (it has them from
    /// `rules`) without asking about it: for DDM's log. Under its own key, with no `url`: an
    /// older host takes it for an address it can't download, and hands nothing over.
    pub(super) declined: Option<Notice>,
    /// The video button's quality menu: look this video up (with the message's `cookies`,
    /// `cookieJar`, `userAgent` and `private`), ask how it goes, download the choice made. Under
    /// their own keys, with no `url`: an older host refuses them without effect.
    pub(super) video_lookup: Option<LookupAsk>,
    pub(super) video_poll: Option<PollAsk>,
    pub(super) video_pick: Option<PickAsk>,
    /// The quality menu: a look-up asked ahead is no longer wanted (its page was left).
    pub(super) video_cancel: Option<PollAsk>,
    /// The host's own: the name of the browser the user added (Options → General → Add
    /// browser...) that started this host, found by its program (`custom_browser`); never sent.
    #[serde(skip)]
    pub(super) named: Option<&'static str>,
}

/// The quality menu's look-up: the video's page (as the button's click sends it), the video's
/// own file or its stream's manifest when the page plays one (`src`), the page's title, the
/// page's referrer, the cookie store the tab is in (Firefox's containers), and whether it is
/// asked ahead of a click (`prefetch`: `mdm_ipc::VideoLookup::prefetch`).
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct LookupAsk {
    pub(super) page: String,
    pub(super) src: Option<String>,
    pub(super) title: Option<String>,
    pub(super) referrer: Option<String>,
    pub(super) store_id: Option<String>,
    pub(super) prefetch: bool,
}

/// The quality menu: how is the look-up of `token` going?
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct PollAsk {
    pub(super) token: String,
}

/// The quality menu: the choice at `choice` of look-up `token`, made in a tab of cookie store
/// `store_id` (the message's `private` says whether in a private window).
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct PickAsk {
    pub(super) token: String,
    pub(super) choice: usize,
    pub(super) store_id: Option<String>,
}

/// A download the extension left to the browser: why (a `Declined` code, or `site-access`:
/// the browser doesn't let it read the site), its site and file name.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Notice {
    pub(super) why: String,
    pub(super) site: String,
    pub(super) name: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reply {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    /// Why it failed, for the extension to say and to link the help page's section
    /// (docs/browser-help.md): `not-running`, `too-large`, `refused`, `options`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) why: Option<&'static str>,
    /// Dannex Download Manager's version (this host comes with it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) version: Option<&'static str>,
    /// For `rules`: Options → File types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) types: Option<String>,
    /// For `rules`: whether this browser's downloads are taken at all (integration on, the
    /// browser captured), and the sites and addresses left to the browser; the extension leaves
    /// those alone without asking (the host checks again at each hand-off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) capture: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) excluded_sites: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) excluded_addresses: Option<Vec<String>>,
    /// For `rules`: also take listed types the browser would open itself (a PDF, an MP3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) opened: Option<bool>,
    /// For `rules`: files smaller than this (KB) stay with the browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) min_kb: Option<u64>,
    /// For `rules` when `capture` is false: why (`integration-off`, `browser`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) capture_off: Option<&'static str>,
    /// A download left to the browser by Options: which rule (a `Declined` code), for the
    /// extension to say (its status page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) declined: Option<&'static str>,
    /// For `hello` and `rules`: this host and its app have the button's quality menu (they are
    /// installed together); without it the extension never asks for one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) video_lookup: Option<bool>,
    /// The quality menu's answer: where the look-up is, or what became of the choice. Its
    /// states are never a `why`: they say nothing of whether DDM is reachable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) lookup: Option<mdm_ipc::api::VideoLookupAnswer>,
    /// For `rules`: Options → General → Keys... (page.js holds a click by them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) keys: Option<ClickKeys>,
    /// For `rules`: the right-click items to show (`mdm_ipc::capture::MENU_ITEMS`' names); the
    /// extension builds its menu again when they change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) menu: Option<Vec<&'static str>>,
    /// For `panel`: the Download panels' settings beyond on or off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) panel: Option<PanelLook>,
    /// For `hello` and `rules`, once the Chrome Web Store shows the extension's listing
    /// (`listed_store_id`): its page. The copy loaded unpacked from DDM's folder, which never
    /// updates itself, then offers it once, quietly; absent until then (and from an older host),
    /// so nothing shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) store: Option<String>,
}

/// Options → General → Keys...: the keys that leave a clicked link to the browser, and those
/// that have DDM take it whatever its type (`mdm_ipc::capture::KEY_CHOICES`, "" none).
#[derive(Debug, Serialize, PartialEq)]
pub(super) struct ClickKeys {
    pub(super) prevent: &'static str,
    pub(super) force: &'static str,
}

/// Download panels: over videos, over sound players, not over media shorter than `min_seconds`
/// (0: any), and the corner it shows in first (`mdm_ipc::capture::PANEL_CORNERS`).
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct PanelLook {
    pub(super) video: bool,
    pub(super) audio: bool,
    pub(super) min_seconds: u32,
    pub(super) corner: &'static str,
}

impl Reply {
    pub(super) fn done() -> Reply {
        Reply {
            ok: true,
            ..Default::default()
        }
    }

    /// The quality menu's answer.
    pub(super) fn lookup(answer: mdm_ipc::api::VideoLookupAnswer) -> Reply {
        Reply {
            ok: true,
            lookup: Some(answer),
            ..Default::default()
        }
    }

    pub(super) fn failed(why: &'static str, error: impl Into<String>) -> Reply {
        Reply {
            why: Some(why),
            error: Some(error.into()),
            ..Default::default()
        }
    }
}

/// Read one native message; None at end of input.
/// Read one native message; None at end of input. `Err` inside: a message over the limit (its
/// size), read past and dropped, to be answered "too-large".
pub(super) fn read_message(r: &mut impl Read) -> io::Result<Option<Result<Vec<u8>, usize>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len) as usize;
    // Browsers send at most 4 GB, but a download request is small: refuse anything silly (the
    // app's channel takes this much, so whatever is read here reaches it). Read past it, so
    // the extension hears why instead of a closed host.
    if len > mdm_ipc::MAX_MESSAGE {
        io::copy(&mut r.take(len as u64), &mut io::sink())?;
        return Ok(Some(Err(len)));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(Ok(buf)))
}

pub(super) fn write_message(w: &mut impl Write, json: &[u8]) -> io::Result<()> {
    w.write_all(&(json.len() as u32).to_le_bytes())?;
    w.write_all(json)?;
    w.flush()
}
