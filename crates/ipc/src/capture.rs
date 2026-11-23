//! What the browser extension may hand to Dannex Download Manager: Options → General ("Use
//! advanced browser integration", the browsers to capture from, the keys a click is held with,
//! the right-click items and the button over videos) and Options → File types (the types to
//! take, the sites and addresses to leave alone). One implementation for the app and
//! the native messaging host: the host reads the app's saved settings at every hand-off (so a
//! change applies at once) and answers "not taken" for the rest, and the extension then lets the
//! browser download it (extension/background.js).

use serde::Deserialize;

/// Options → File types defaults: IDM 6.42's; TORRENT (a .torrent the browser hands over opens
/// Add torrent, #39); and today's installers, packages and archives (macOS DMG and PKG, Linux
/// DEB, RPM and AppImage, Windows MSIX, APPX and CAB, Java's JAR, XZ, Zstandard and their tarballs)
/// and open audio and video formats (WEBM, FLAC, OPUS; #208); and what people fetch by the
/// gigabyte today (#382): AI models and datasets, 3D and CAD, camera RAW and design files, books
/// and Office files, disk images and split archives. Never what a browser saves as part of
/// everyday browsing (pages, scripts, plain text, web images) or installs itself (XPI, CRX).
pub const DEFAULT_AUTO_TYPES: &str = "00* 3DM 3DS 3GP 3MF 7Z AAB AAC ABC ACE AFDESIGN AFPHOTO AI AIF AIFF APE APK APKM APKS APPIMAGE APPX APPXBUNDLE ARJ ARROW ARW ASF AU AVI AVRO AZW AZW3 BAK BAM BIN BLEND BZ BZ2 C4D CAB CAF CBR CBZ CCD CHD CKPT CPIO CR2 CR3 CRAM CSO DAE DAT DCM DDS DEB DFF DIVX DJVU DMG DNG DOC DOCX DSF DWG DXF EPS EPUB EXE EXR F3D F4V FASTA FASTQ FB2 FBX FEATHER FIG FITS FLAC FLATPAK FQ GCODE GCZ GEM GGML GGUF GLB GLTF GPKG GRIB GRIB2 GZ GZIP H5 HDF5 HDR IGES IGS IMG INDD IPA ISO ISZ JAR JOBLIB JSONL KERAS KRA LAS LAZ LLAMAFILE LWO LZ LZ4 LZH LZMA M2T M2TS M2V M4A M4B M4V MA MAT MAX MB MBTILES MDF MDS MID MIDI MKA MKV MLMODEL MOBI MOV MP3 MP4 MPA MPE MPEG MPG MSI MSIX MSIXBUNDLE MSP MSU MTS MXF NC NDJSON NEF NEMO NII NPY NPZ NRG NSP NSZ NUPKG OBJ ODP ODS ODT OGG OGM OGV ONNX OPUS ORC ORF OTF OVA OVF PAK PARQUET PB PBF PDF PICKLE PK3 PK4 PKG PKL PLJ PLY POTX PPS PPSX PPT PPTX PSB PSD PT PTH QCOW QCOW2 QT R0* R1* R2* R3* R4* R5* R6* R7* R8* R9* RA RAF RAR RDS RM RMVB RPM RTF RUN RVZ RW2 SAFETENSORS SEA SF2 SFZ SHP SIT SITX SKETCH SKP SLDASM SLDPRT SNAP SQLITE SQLITE3 STEP STL STP TAR TBZ TBZ2 TFLITE TGA TGZ TIF TIFF TLZ TORRENT TTA TTC TTF TXZ TZST UNITYPACKAGE USD USDA USDC USDZ VDB VDI VHD VHDX VMDK VOB VPK VSIX WAV WBFS WEBM WHL WIM WMA WMV WRL WTV WUA WV X3D XAPK XCF XCI XLS XLSB XLSM XLSX XZ Z Z0* ZIP ZIPX ZPR ZST ZTL";

/// The lists earlier versions started with: IDM 6.42's (DDM 0.2.x), with TORRENT (#39), then
/// with today's packages and open formats (#208). A saved list equal to one of them was never edited, and becomes today's (`upgraded_types`).
const EARLIER_AUTO_TYPES: [&str; 3] = [
    "3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR TIF TIFF WAV WMA WMV Z ZIP",
    "3GP 7Z AAC ACE AIF APK ARJ ASF AVI BIN BZ2 EXE GZ GZIP IMG ISO LZH M4A M4V MKV MOV MP3 MP4 MPA MPE MPEG MPG MSI MSU OGG OGV PDF PLJ PPS PPT QT R0* R1* RA RAR RM RMVB SEA SIT SITX TAR TIF TIFF TORRENT WAV WMA WMV Z ZIP",
    "3GP 7Z AAC ACE AIF APK APPIMAGE APPX APPXBUNDLE ARJ ASF AVI BIN BZ2 CAB DEB DMG EXE FLAC GZ GZIP IMG ISO JAR LZH M4A M4V MKV MOV MP3 MP4 MPA MPE MPEG MPG MSI MSIX MSU OGG OGV OPUS PDF PKG PLJ PPS PPT QT R0* R1* RA RAR RM RMVB RPM SEA SIT SITX TAR TGZ TIF TIFF TORRENT TXZ WAV WEBM WMA WMV XZ Z ZIP ZST",
];

/// Today's default list when `saved` (Options → File types as saved) is an earlier version's
/// default, untouched (the same types, in any order or case); `None` when it was edited, or is
/// today's already: the user's own list never changes.
pub fn upgraded_types(saved: &str) -> Option<&'static str> {
    let set = |s: &str| {
        let mut v: Vec<String> = s.split_whitespace().map(str::to_ascii_uppercase).collect();
        v.sort();
        v.dedup();
        v
    };
    let saved = set(saved);
    EARLIER_AUTO_TYPES
        .iter()
        .any(|old| set(old) == saved)
        .then_some(DEFAULT_AUTO_TYPES)
}
pub const DEFAULT_EXCLUDED_SITES: &str = "*.update.microsoft.com download.windowsupdate.com *.download.windowsupdate.com siteseal.thawte.com ecom.cimetz.com *.voice2page.com";

/// The capture settings, named as the app saves them (`Settings` in downloads.json).
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct CaptureRules {
    pub advanced_integration: bool,
    /// (browser name as Options lists it, capture from it?)
    pub browsers: Vec<(String, bool)>,
    /// Options → General → Add browser...: (name, program) of the browsers the user added, each
    /// in `browsers` by that name. The native host knows one by the program that started it
    /// ([`CaptureRules::custom_name`]): such a browser says it is Chrome or Firefox in its
    /// User-Agent, and has its own switch here.
    pub custom_browsers: Vec<(String, String)>,
    pub auto_types: String,
    pub excluded_sites: String,
    pub excluded_addresses: Vec<String>,
    /// Options → General → Download panels: the download button over videos in the browsers.
    pub video_panel: bool,
    /// The sites it stays off (space-separated, `*.example.com` takes subdomains too).
    pub video_panel_sites: String,
    /// Download panels: the button over videos, over sound players (`<audio>` with controls).
    pub video_panel_video: bool,
    pub video_panel_audio: bool,
    /// Download panels: not over a video or sound shorter than this (seconds; 0: any). The
    /// extension knows a length once the player has loaded it; until then the button shows.
    pub video_panel_min_s: u32,
    /// Download panels: the corner it shows in first (`PANEL_CORNERS`); dragged elsewhere on a
    /// site, it stays there on that site (the extension keeps that).
    pub video_panel_corner: String,
    /// Options → General: also take the listed types the browser would open itself (a PDF in
    /// its viewer, an MP3 in its player), not only what it downloads.
    pub capture_opened: bool,
    /// Options → File types: files smaller than this (KB) stay with the browser (a receipt, an
    /// icon); 0 takes any size. Only when the browser knows the size.
    pub capture_min_kb: u64,
    /// Options → General → Keys...: the keys that, held while clicking a link, leave it to the
    /// browser (`key_prevent`) or have DDM take it whatever its type (`key_force`); one of
    /// `KEY_CHOICES` each ("" none).
    pub key_prevent: String,
    pub key_force: String,
    /// Options → General → Customize DDM menu items...: the right-click items the browsers
    /// don't show (`MENU_ITEMS` names). The ones it lists are off; an item added later shows.
    pub browser_menu_hidden: Vec<String>,
}

impl Default for CaptureRules {
    fn default() -> Self {
        CaptureRules {
            advanced_integration: true,
            browsers: Vec::new(),
            custom_browsers: Vec::new(),
            auto_types: DEFAULT_AUTO_TYPES.into(),
            excluded_sites: DEFAULT_EXCLUDED_SITES.into(),
            excluded_addresses: Vec::new(),
            video_panel: true,
            video_panel_sites: String::new(),
            video_panel_video: true,
            video_panel_audio: true,
            video_panel_min_s: 0,
            video_panel_corner: PANEL_CORNERS[0].into(),
            capture_opened: true,
            capture_min_kb: 0,
            key_prevent: DEFAULT_KEY_PREVENT.into(),
            key_force: DEFAULT_KEY_FORCE.into(),
            browser_menu_hidden: Vec::new(),
        }
    }
}

/// Options → General → Keys...: the key combinations a click can be held with, as saved and as
/// the extension reads them (names joined by `+`, always in this order); "" is none. The
/// extension's page.js compares one with exactly the keys held. On macOS "ctrl" is the Command
/// key there: a Control-click is the secondary click (the right-click menu), and browsers give
/// Command what Ctrl does elsewhere.
pub const KEY_CHOICES: [&str; 8] = [
    "",
    "alt",
    "ctrl",
    "shift",
    "ctrl+alt",
    "alt+shift",
    "ctrl+shift",
    "ctrl+alt+shift",
];
/// Alt+click leaves a link to the browser, as it always did (Chrome downloads an Alt-clicked
/// link itself).
pub const DEFAULT_KEY_PREVENT: &str = "alt";
/// Ctrl+Alt+click (Command+Option on macOS): a click nobody makes for anything else. The browsers'
/// own modifier clicks are Ctrl (a new tab), Ctrl+Shift (a tab in front), Shift (a new window)
/// and Alt (a download), and the pair isn't one of them; Alt+Shift would be, on Windows, the
/// keyboard-layout switch, which holding it changes. The extension stops the browser's own
/// handling of the click (the tab Ctrl would open), so only DDM gets the link.
pub const DEFAULT_KEY_FORCE: &str = "ctrl+alt";

/// `saved` as one of `KEY_CHOICES` (the names in any order or case, joined by `+`), or "" when
/// it names anything else.
pub fn key_choice(saved: &str) -> &'static str {
    let mut held = [false; 3];
    for name in saved.split('+').map(str::trim).filter(|n| !n.is_empty()) {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => held[0] = true,
            "alt" | "option" => held[1] = true,
            "shift" => held[2] = true,
            _ => return "",
        }
    }
    let names = ["ctrl", "alt", "shift"];
    let joined: Vec<&str> = (0..3).filter(|&i| held[i]).map(|i| names[i]).collect();
    let joined = joined.join("+");
    KEY_CHOICES
        .iter()
        .find(|c| **c == joined)
        .copied()
        .unwrap_or("")
}

/// The right-click items the extension can show, by their names between the app and the
/// extension (background.js's menu ids are `mdm-<name>`): Download with DDM (a link, picture,
/// video or sound), Download selected links, all links, all images, all videos and audio.
pub const MENU_ITEMS: [&str; 5] = ["link", "selected", "all", "images", "media"];

/// Download panels: the corners the button can show in first, as the extension names them
/// (video.js), the default first.
pub const PANEL_CORNERS: [&str; 4] = ["top-right", "top-left", "bottom-left", "bottom-right"];

/// Why a download is left to the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declined {
    IntegrationOff,
    Browser,
    FileType,
    Site,
    Address,
    Small,
}

impl Declined {
    pub const ALL: [Declined; 6] = [
        Declined::IntegrationOff,
        Declined::Browser,
        Declined::FileType,
        Declined::Site,
        Declined::Address,
        Declined::Small,
    ];

    /// Its name between the native host and the extension: the host's reply (`declined`), and
    /// the extension's notice of a download it left alone by the same rules
    /// (extension/background.js).
    pub fn code(self) -> &'static str {
        match self {
            Declined::IntegrationOff => "integration-off",
            Declined::Browser => "browser",
            Declined::FileType => "file-type",
            Declined::Site => "site",
            Declined::Address => "address",
            Declined::Small => "small",
        }
    }

    pub fn from_code(code: &str) -> Option<Declined> {
        Declined::ALL.into_iter().find(|d| d.code() == code)
    }

    /// Why, for DDM's log (English, as the rest of it), naming the setting that decides.
    pub fn reason(self) -> &'static str {
        match self {
            Declined::IntegrationOff => {
                "Options > General > Use advanced browser integration is off"
            }
            Declined::Browser => {
                "the browser is unticked in Options > General > Capture downloads from the following browsers"
            }
            Declined::FileType => "its type isn't in Options > File types",
            Declined::Site => {
                "its site is in Options > File types > Don't start downloading automatically from the following sites"
            }
            Declined::Address => {
                "its address is in Options > File types > Edit list (addresses left to the browser)"
            }
            Declined::Small => {
                "it is smaller than Options > File types > Don't take files smaller than"
            }
        }
    }
}

impl CaptureRules {
    /// The rules as the app last saved them (defaults when there is no list yet).
    pub fn load() -> CaptureRules {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Stored {
            settings: CaptureRules,
        }
        crate::data_dir()
            .and_then(|d| std::fs::read(d.join("downloads.json")).ok())
            .and_then(|b| serde_json::from_slice::<Stored>(&b).ok())
            .map(|s| s.settings.upgraded())
            .unwrap_or_default()
    }

    /// An earlier version's default File types list, untouched, as today's (the app does the
    /// same when it reads its list, and saves it so).
    pub fn upgraded(mut self) -> CaptureRules {
        if let Some(types) = upgraded_types(&self.auto_types) {
            self.auto_types = types.into();
        }
        self
    }

    /// Is a file of `size` bytes (the browser's figure, when it has one) too small to take?
    pub fn too_small(&self, size: Option<u64>) -> bool {
        size.is_some_and(|s| s > 0 && s < self.capture_min_kb.saturating_mul(1024))
    }

    /// Does the file name's extension appear in File types (`R0*` patterns too)?
    pub fn auto_type(&self, name: &str) -> bool {
        type_listed(&self.auto_types, name)
    }

    /// The name of the browser the user added (Add browser...) whose program `running` is (the
    /// program that started the native host), if one is: the same file (by its path as saved,
    /// or with its links resolved), or a file inside the macOS app chosen.
    pub fn custom_name(&self, running: &std::path::Path) -> Option<&str> {
        self.custom_browsers
            .iter()
            .find(|(_, chosen)| {
                let chosen = std::path::Path::new(chosen);
                same_program(chosen, running)
                    || std::fs::canonicalize(chosen).is_ok_and(|c| same_program(&c, running))
            })
            .map(|(name, _)| name.as_str())
    }

    /// Are downloads from `browser` (a name `browser_named` gives) taken at all?
    pub fn captures(&self, browser: &str) -> bool {
        self.capture_off(browser).is_none()
    }

    /// Why no download from `browser` is taken (integration off, the browser unticked), if so.
    pub fn capture_off(&self, browser: &str) -> Option<Declined> {
        if !self.advanced_integration {
            Some(Declined::IntegrationOff)
        } else if self.browsers.iter().any(|(b, on)| b == browser && !on) {
            Some(Declined::Browser)
        } else {
            None
        }
    }

    /// May `browser` hand `url` (saved as `file_name`) over?
    pub fn accepts(&self, url: &str, file_name: &str, browser: &str) -> Result<(), Declined> {
        if let Some(off) = self.capture_off(browser) {
            return Err(off);
        }
        if !self.auto_type(file_name) {
            return Err(Declined::FileType);
        }
        let host = host_of(url);
        if self
            .excluded_sites
            .split_whitespace()
            .any(|p| host_matches(&host, p))
        {
            return Err(Declined::Site);
        }
        if self
            .excluded_addresses
            .iter()
            .any(|p| address_matches(p, url))
        {
            return Err(Declined::Address);
        }
        Ok(())
    }
}

impl CaptureRules {
    /// The keys page.js goes by: (leave the link to the browser, have DDM take it), each one of
    /// `KEY_CHOICES`. The same keys for both (a settings file edited by hand) only leave the
    /// link to the browser, as Alt always did.
    pub fn click_keys(&self) -> (&'static str, &'static str) {
        let prevent = key_choice(&self.key_prevent);
        let force = key_choice(&self.key_force);
        (prevent, if force == prevent { "" } else { force })
    }

    /// The right-click items the browsers show (`MENU_ITEMS`' names, in its order).
    pub fn menu_shown(&self) -> Vec<&'static str> {
        MENU_ITEMS
            .iter()
            .copied()
            .filter(|item| !self.browser_menu_hidden.iter().any(|h| h == item))
            .collect()
    }

    /// The corner the button shows in first (one of `PANEL_CORNERS`; anything else is the
    /// default).
    pub fn panel_corner(&self) -> &'static str {
        PANEL_CORNERS
            .iter()
            .copied()
            .find(|c| *c == self.video_panel_corner)
            .unwrap_or(PANEL_CORNERS[0])
    }

    /// May the download button show over videos on `page` (its address) in `browser`?
    /// Integration on, that browser captured, the button on (over videos or sound players) and
    /// the site not one it stays off. `top`: the tab's address when `page` is a frame of another
    /// page's (a player embedded from another site): the site in the address bar is the one a
    /// person lists (#532).
    pub fn panel_on(&self, page: &str, top: Option<&str>, browser: &str) -> bool {
        self.captures(browser)
            && self.video_panel
            && (self.video_panel_video || self.video_panel_audio)
            && !self.panel_hidden_on(page)
            && !top.is_some_and(|t| !t.is_empty() && self.panel_hidden_on(t))
    }

    /// Is `page` (an address) on a site the button stays off? A name without `*.` takes its
    /// subdomains too (#532: `youtube.com` listed, the button still showed on www.youtube.com).
    pub fn panel_hidden_on(&self, page: &str) -> bool {
        let host = host_of(page);
        self.video_panel_sites
            .split_whitespace()
            .any(|p| host_matches(&host, p) || host_matches(&host, &format!("*.{p}")))
    }
}

/// `sites` (space-separated) with `host` added, unless a pattern already covers it or it isn't
/// one host name (a space would add several patterns, `*` all sites).
pub fn add_site(sites: &str, host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    let one_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']'));
    if !one_host || sites.split_whitespace().any(|p| host_matches(&host, p)) {
        return sites.to_owned();
    }
    let mut out = sites.trim().to_owned();
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(&host);
    out
}

/// The sites a list holds (space-separated), as a file: one a line (Options → Download panels,
/// #424).
pub fn sites_to_text(sites: &str) -> String {
    sites.split_whitespace().fold(String::new(), |mut out, s| {
        out.push_str(s);
        out.push('\n');
        out
    })
}

/// `sites` with the patterns of a file (one or more names a line, or space-separated) added, those
/// not there already whatever their case, and how many that was. A name holding anything else
/// than a host pattern's characters is left out.
pub fn merge_sites(sites: &str, text: &str) -> (String, usize) {
    let mut out: Vec<String> = sites.split_whitespace().map(str::to_owned).collect();
    let mut added = 0;
    for name in text.split_whitespace().take(5000) {
        let usable = name.len() <= 253
            && name.chars().all(|c| {
                c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']' | '*' | '/')
            });
        if usable && !out.iter().any(|s| s.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
            added += 1;
        }
    }
    (out.join(" "), added)
}

/// Does `name`'s extension appear in `types` (Options → File types: `EXE ZIP R0*`)?
pub fn type_listed(types: &str, name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let ext = ext.to_ascii_uppercase();
    types.split_whitespace().any(|t| {
        let t = t.to_ascii_uppercase();
        match t.strip_suffix('*') {
            Some(prefix) => ext.starts_with(prefix),
            None => ext == t,
        }
    })
}

/// The browser by the names Options lists, from its User-Agent and, for those that say who
/// they are only there, its `brands`: `navigator.userAgentData`'s (Brave, Opera), and for
/// Firefox's forks the name `runtime.getBrowserInfo` gives (Zen, LibreWolf, Floorp, Waterfox;
/// extension/background.js). Helium names itself Google Chrome on purpose.
pub fn browser_named(user_agent: &str, brands: &[String]) -> &'static str {
    let brand = |b: &str| brands.iter().any(|x| x == b);
    if brand("Zen") {
        "Zen Browser"
    } else if brand("LibreWolf") {
        "LibreWolf"
    } else if brand("Floorp") {
        "Floorp"
    } else if brand("Waterfox") {
        "Waterfox"
    } else if brand("Brave") {
        "Brave Browser"
    } else if brand("Opera") || brand("Opera GX") {
        "Opera"
    } else if brand("Microsoft Edge") {
        "Microsoft Edge"
    } else {
        browser_of(user_agent)
    }
}

/// Whether `chosen` (a program as Add browser... saved it) is `running` (the program that started
/// the native host): the same path, compared as the system compares them (in any case on Windows
/// and macOS, either slash, without Windows' `\\?\`), or `running` inside the app bundle chosen
/// (macOS: `Vivaldi.app` runs `Vivaldi.app/Contents/MacOS/Vivaldi`).
pub fn same_program(chosen: &std::path::Path, running: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s
            .strip_prefix("//?/")
            .unwrap_or(&s)
            .trim_end_matches('/')
            .to_owned();
        if cfg!(any(windows, target_os = "macos")) {
            s.to_lowercase()
        } else {
            s
        }
    };
    let (chosen, running) = (norm(chosen), norm(running));
    !chosen.is_empty()
        && (chosen == running
            || (chosen.ends_with(".app") && running.starts_with(&format!("{chosen}/"))))
}

/// The browser a User-Agent belongs to, by the names Options lists. Brave says it is Chrome.
pub fn browser_of(user_agent: &str) -> &'static str {
    if user_agent.contains("Edg/") {
        "Microsoft Edge"
    } else if user_agent.contains("OPR/") || user_agent.contains("Opera") {
        "Opera"
    } else if user_agent.contains("Firefox/") {
        "Mozilla Firefox"
    } else if user_agent.contains("Chrome/") {
        "Google Chrome"
    } else if user_agent.contains("Safari/") {
        "Apple Safari"
    } else {
        ""
    }
}

/// The host of an http(s) address, lower case.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split(']').next().map_or(host, |h| &h[1..])
    } else {
        host.split(':').next().unwrap_or(host)
    };
    host.to_ascii_lowercase()
}

/// `host` is `pattern` (case-insensitive) or, for `*.example.com`, one of its subdomains.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let pattern = pattern
        .split_once("://")
        .map_or(pattern.as_str(), |(_, r)| r);
    let pattern = pattern.split('/').next().unwrap_or("");
    match pattern.strip_prefix("*.") {
        Some(base) => host == base || host.ends_with(&format!(".{base}")),
        None => !pattern.is_empty() && host == pattern,
    }
}

/// An address pattern (Options → File types) against `url`, case-insensitive: written with a
/// scheme (`https://example.com/private/*`) it is the whole address; without one
/// (`example.com/private/*`), the address after its `scheme://`.
pub fn address_matches(pattern: &str, url: &str) -> bool {
    let (pattern, url) = (
        pattern.trim().to_ascii_lowercase(),
        url.to_ascii_lowercase(),
    );
    if pattern.is_empty() {
        return false;
    }
    let text = if pattern.contains("://") {
        url.as_str()
    } else {
        url.split_once("://").map_or(url.as_str(), |(_, rest)| rest)
    };
    wildcard(&pattern, text)
}

/// Is a copied link named `name` a file to offer? A copied address is often not meant to be
/// downloaded at all (a picture, a font or a model to paste into code), and it says nothing about
/// what is behind it, unlike a browser that is really downloading. So the clipboard keeps to the
/// list as it stood before #382, plus what the user typed in themselves; the types #382 added are
/// the browser's alone, and a lone `*` (every type) doesn't count.
pub fn clipboard_type(types: &str, name: &str) -> bool {
    let own: Vec<&str> = types.split_whitespace().filter(|t| *t != "*").collect();
    let browser_only =
        type_listed(DEFAULT_AUTO_TYPES, name) && !type_listed(EARLIER_AUTO_TYPES[2], name);
    !browser_only && type_listed(&own.join(" "), name)
}

/// `*` matches any run of characters (IDM's address patterns).
fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
    const EDGE_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
    const FIREFOX_UA: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:145.0) Gecko/20100101 Firefox/145.0";
    const CHROME: &str = "Google Chrome";
    const EDGE: &str = "Microsoft Edge";
    const FIREFOX: &str = "Mozilla Firefox";

    /// #424: the list of sites as a file and back: one a line, duplicates (whatever their case)
    /// not added, what isn't a site left out.
    #[test]
    fn sites_export_and_import() {
        assert_eq!(sites_to_text("a.test *.b.test"), "a.test\n*.b.test\n");
        assert_eq!(sites_to_text(""), "");
        let (merged, added) = merge_sites(
            "a.test b.test",
            "A.TEST\r\nc.test  d.test\n*.e.test\nbad<name\n",
        );
        assert_eq!(merged, "a.test b.test c.test d.test *.e.test");
        assert_eq!(added, 3);
        assert_eq!(merge_sites("a.test", "").0, "a.test");
    }

    #[test]
    fn browsers_by_name() {
        assert_eq!(browser_named(CHROME_UA, &[]), CHROME);
        assert_eq!(browser_named(EDGE_UA, &[]), EDGE);
        assert_eq!(browser_named(FIREFOX_UA, &[]), FIREFOX);
        let brands = |b: &[&str]| b.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            browser_named(CHROME_UA, &brands(&["Not.A/Brand", "Chromium", "Brave"])),
            "Brave Browser"
        );
        assert_eq!(
            browser_named(CHROME_UA, &brands(&["Chromium", "Google Chrome"])),
            CHROME
        );
        assert_eq!(browser_named(CHROME_UA, &brands(&["Opera"])), "Opera");
        // #41: Firefox's forks by runtime.getBrowserInfo's name; Firefox itself by its UA.
        assert_eq!(browser_named(FIREFOX_UA, &brands(&["Zen"])), "Zen Browser");
        assert_eq!(
            browser_named(FIREFOX_UA, &brands(&["LibreWolf"])),
            "LibreWolf"
        );
        assert_eq!(browser_named(FIREFOX_UA, &brands(&["Firefox"])), FIREFOX);
        // Helium's brands are Chrome's (spoofed on purpose).
        assert_eq!(
            browser_named(
                CHROME_UA,
                &brands(&["Chromium", "Google Chrome", "Not.A/Brand"])
            ),
            CHROME
        );
    }

    #[test]
    fn the_video_button_follows_its_options() {
        let mut r = CaptureRules {
            video_panel_sites: "*.example.com tube.test".into(),
            ..CaptureRules::default()
        };
        assert!(r.panel_on("https://videos.site/watch?v=1", None, CHROME));
        assert!(
            !r.panel_on("https://www.example.com/v", None, CHROME),
            "a listed site"
        );
        assert!(
            !r.panel_on("https://example.com/v", None, CHROME),
            "*. takes the domain too"
        );
        assert!(!r.panel_on("https://TUBE.test/v", None, FIREFOX));
        r.browsers = vec![("Mozilla Firefox".into(), false)];
        assert!(
            !r.panel_on("https://videos.site/", None, FIREFOX),
            "a browser not captured"
        );
        assert!(r.panel_on("https://videos.site/", None, CHROME));
        r.video_panel = false;
        assert!(!r.panel_on("https://videos.site/", None, CHROME));
        // #532: a name without `*.` takes its subdomains (www. above all); a player embedded
        // from another site follows the tab's site.
        r.video_panel = true;
        r.video_panel_sites = "quiet.test".into();
        assert!(!r.panel_on("https://www.quiet.test/v", None, CHROME));
        assert!(!r.panel_on("https://quiet.test/v", None, CHROME));
        assert!(r.panel_on("https://notquiet.test/v", None, CHROME));
        assert!(!r.panel_on(
            "https://videos.site/embed/1",
            Some("https://www.quiet.test/post"),
            CHROME
        ));
        assert!(r.panel_on(
            "https://videos.site/embed/1",
            Some("https://blog.site/"),
            CHROME
        ));
        assert!(r.panel_on("https://videos.site/embed/1", Some(""), CHROME));
        r.video_panel = true;
        r.advanced_integration = false;
        assert!(!r.panel_on("https://videos.site/", None, CHROME));
    }

    /// Options → General → Keys...: a combination saved in any order or case is one of the
    /// choices; anything else is none; the same keys for both leave the link to the browser.
    #[test]
    fn the_click_keys_are_one_of_the_choices() {
        for c in KEY_CHOICES {
            assert_eq!(key_choice(c), c);
        }
        assert_eq!(key_choice("Alt+Ctrl"), "ctrl+alt");
        assert_eq!(key_choice(" shift + ALT "), "alt+shift");
        assert_eq!(key_choice("control+option"), "ctrl+alt");
        assert_eq!(key_choice("alt+alt"), "alt");
        assert_eq!(key_choice("meta"), "");
        assert_eq!(key_choice("alt+win"), "");
        assert_eq!(key_choice(""), "");
        let mut r = CaptureRules::default();
        assert_eq!(r.click_keys(), ("alt", "ctrl+alt"), "the defaults");
        r.key_force = "ALT".into();
        assert_eq!(r.click_keys(), ("alt", ""), "the same keys: the browser's");
        r.key_prevent = String::new();
        assert_eq!(r.click_keys(), ("", "alt"));
        // Saved before Keys... had them: the defaults.
        let old: CaptureRules = serde_json::from_str(r#"{"auto_types":"ZIP"}"#).unwrap();
        assert_eq!(old.click_keys(), ("alt", "ctrl+alt"));
    }

    /// Customize DDM menu items...: every item unless listed off; an item this version doesn't
    /// know (a later one's) changes nothing.
    #[test]
    fn the_menu_items_shown() {
        let mut r = CaptureRules::default();
        assert_eq!(r.menu_shown(), MENU_ITEMS.to_vec());
        r.browser_menu_hidden = vec!["images".into(), "later".into(), "all".into()];
        assert_eq!(r.menu_shown(), vec!["link", "selected", "media"]);
        r.browser_menu_hidden = MENU_ITEMS.iter().map(|s| s.to_string()).collect();
        assert!(r.menu_shown().is_empty());
    }

    /// Download panels: for videos, sounds or both (neither: off), its first corner.
    #[test]
    fn the_panels_kinds_and_corner() {
        let mut r = CaptureRules::default();
        assert_eq!(r.panel_corner(), "top-right");
        r.video_panel_corner = "bottom-left".into();
        assert_eq!(r.panel_corner(), "bottom-left");
        r.video_panel_corner = "middle".into();
        assert_eq!(r.panel_corner(), "top-right");
        r.video_panel_video = false;
        assert!(
            r.panel_on("https://videos.site/", None, CHROME),
            "sounds only"
        );
        r.video_panel_audio = false;
        assert!(!r.panel_on("https://videos.site/", None, CHROME), "neither");
        let old: CaptureRules = serde_json::from_str(r#"{"video_panel":true}"#).unwrap();
        assert!(old.video_panel_video && old.video_panel_audio && old.video_panel_min_s == 0);
    }

    #[test]
    fn small_files_by_options() {
        let mut r = CaptureRules::default();
        assert!(!r.too_small(Some(10)), "any size by default");
        r.capture_min_kb = 100;
        assert!(r.too_small(Some(15 * 1024)));
        assert!(!r.too_small(Some(100 * 1024)) && !r.too_small(None) && !r.too_small(Some(0)));
    }

    #[test]
    fn a_site_is_added_once() {
        assert_eq!(add_site("", "Video.Site"), "video.site");
        assert_eq!(add_site("a.test", "b.test"), "a.test b.test");
        assert_eq!(
            add_site("*.b.test", "www.b.test"),
            "*.b.test",
            "already covered"
        );
        assert_eq!(add_site("a.test", " "), "a.test");
        assert_eq!(add_site("a.test", "b.test *"), "a.test", "one host only");
        assert_eq!(add_site("a.test", "*"), "a.test");
    }

    #[test]
    fn the_defaults_take_listed_types_from_any_browser() {
        let r = CaptureRules::default();
        assert_eq!(
            r.accepts("https://example.com/a/setup.exe", "setup.exe", CHROME),
            Ok(())
        );
        assert_eq!(
            r.accepts(
                "https://example.com/movie.part.r01",
                "movie.part.r01",
                FIREFOX
            ),
            Ok(())
        );
        assert_eq!(
            r.accepts("https://example.com/page.html", "page.html", CHROME),
            Err(Declined::FileType)
        );
        // #39: a .torrent, by default (Add torrent shows what it holds); a list saved before keeps
        // what it had.
        assert_eq!(
            r.accepts("https://example.com/dl/set.torrent", "set.torrent", CHROME),
            Ok(())
        );
        let saved: CaptureRules = serde_json::from_str(r#"{"auto_types":"ZIP EXE"}"#).unwrap();
        assert_eq!(
            saved.accepts("https://example.com/dl/set.torrent", "set.torrent", CHROME),
            Err(Declined::FileType)
        );
    }

    #[test]
    fn excluded_sites_addresses_and_browsers_are_left_to_the_browser() {
        let mut r = CaptureRules::default();
        assert_eq!(
            r.accepts("https://dl.update.microsoft.com/x.exe", "x.exe", CHROME),
            Err(Declined::Site)
        );
        r.excluded_addresses = vec!["https://example.com/private/*".into()];
        assert_eq!(
            r.accepts("https://example.com/private/a.zip", "a.zip", CHROME),
            Err(Declined::Address)
        );
        assert_eq!(
            r.accepts("https://example.com/public/a.zip", "a.zip", CHROME),
            Ok(())
        );
        r.browsers = vec![
            ("Microsoft Edge".into(), false),
            ("Google Chrome".into(), true),
        ];
        assert_eq!(
            r.accepts("https://example.com/a.zip", "a.zip", EDGE),
            Err(Declined::Browser)
        );
        assert_eq!(
            r.accepts("https://example.com/a.zip", "a.zip", CHROME),
            Ok(())
        );
        r.advanced_integration = false;
        assert_eq!(
            r.accepts("https://example.com/a.zip", "a.zip", CHROME),
            Err(Declined::IntegrationOff)
        );
    }

    #[test]
    fn hosts_and_patterns() {
        assert_eq!(
            host_of("https://user:pw@Example.COM:8080/x?y"),
            "example.com"
        );
        assert_eq!(host_of("http://[::1]:80/"), "::1");
        assert!(host_matches("a.b.example.com", "*.example.com"));
        assert!(host_matches("example.com", "*.example.com"));
        assert!(!host_matches("badexample.com", "*.example.com"));
        assert!(wildcard("*/files/*.zip", "https://x.org/files/a.zip"));
        assert!(address_matches(
            "example.com/private/*",
            "https://Example.com/private/a.zip"
        ));
        assert!(address_matches(
            "https://example.com/private/*",
            "https://example.com/private/a"
        ));
        assert!(!address_matches(
            "https://example.com/private/*",
            "http://example.com/private/a"
        ));
        assert!(!address_matches(" ", "https://x.org/"));
        assert!(!wildcard("*/files/*.zip", "https://x.org/files/a.rar"));
    }

    #[test]
    fn an_earlier_default_list_becomes_todays_and_an_edited_one_stays() {
        for old in EARLIER_AUTO_TYPES {
            assert_eq!(upgraded_types(old), Some(DEFAULT_AUTO_TYPES));
            // In another order or case: the same list, never edited.
            let shuffled: Vec<String> = old.split(' ').rev().map(str::to_lowercase).collect();
            assert_eq!(
                upgraded_types(&shuffled.join("  ")),
                Some(DEFAULT_AUTO_TYPES)
            );
        }
        // 0.2.4's list: a .dmg, .deb or .webm is taken once upgraded (#208).
        let r = CaptureRules {
            auto_types: EARLIER_AUTO_TYPES[0].into(),
            ..CaptureRules::default()
        }
        .upgraded();
        for name in [
            "DDM.dmg",
            "mdm_0.3.0_amd64.deb",
            "a.AppImage",
            "clip.webm",
            "x.tar.zst",
        ] {
            assert!(r.auto_type(name), "{name}");
        }
        // Edited by the user: one type more, one less, or their own list: left as it is.
        let more = format!("{} DMG", EARLIER_AUTO_TYPES[0]);
        let less = EARLIER_AUTO_TYPES[1].replace(" ZIP", "");
        for edited in [more.as_str(), less.as_str(), "ZIP RAR", ""] {
            assert_eq!(upgraded_types(edited), None, "{edited:?}");
            let r = CaptureRules {
                auto_types: edited.into(),
                ..CaptureRules::default()
            }
            .upgraded();
            assert_eq!(r.auto_types, edited);
        }
        // Today's needs nothing; it holds every earlier default's types.
        assert_eq!(upgraded_types(DEFAULT_AUTO_TYPES), None);
        for old in EARLIER_AUTO_TYPES {
            for t in old.split(' ') {
                assert!(DEFAULT_AUTO_TYPES.split(' ').any(|n| n == t), "{t}");
            }
        }
    }

    #[test]
    fn todays_big_files_are_taken_and_everyday_browsing_is_not() {
        // #382: an untouched list from before gets them too.
        let earlier = CaptureRules {
            auto_types: EARLIER_AUTO_TYPES[2].into(),
            ..CaptureRules::default()
        };
        assert!(!earlier.auto_type("model.Q4_K_M.gguf"));
        let r = earlier.upgraded();
        for name in [
            "model.Q4_K_M.gguf",
            "model-00001-of-00004.safetensors",
            "sd15.ckpt",
            "weights.pth",
            "model.onnx",
            "train-00000.parquet",
            "scene.glb",
            "part.stl",
            "character.fbx",
            "house.blend",
            "bracket.step",
            "IMG_0001.CR3",
            "cover.psd",
            "book.epub",
            "report.docx",
            "budget.xlsx",
            "disk.vhdx",
            "backup.7z.001",
            "backup.z01",
            "movie.r27",
            "5GB-testfile.dat",
        ] {
            assert!(r.auto_type(name), "{name}");
        }
        for name in [
            "index.html",
            "app.js",
            "style.css",
            "config.json",
            "notes.txt",
            "photo.jpg",
            "logo.png",
            "icon.svg",
            "addon.xpi",
            "addon.crx",
        ] {
            assert!(!r.auto_type(name), "{name}");
        }
    }

    #[test]
    fn a_lone_star_takes_every_file_with_a_type() {
        let r = CaptureRules {
            auto_types: "*".into(),
            ..CaptureRules::default()
        };
        for name in ["model.gguf", "photo.jpg", "anything.xyz123"] {
            assert_eq!(r.accepts("https://x.test/f", name, "Google Chrome"), Ok(()));
        }
        assert!(!r.auto_type("no_extension"));
    }

    #[test]
    fn a_copied_address_is_offered_only_as_it_always_was() {
        // What people copy to paste somewhere else, never a download by default.
        for name in [
            "photo.jpg",
            "logo.png",
            "page.html",
            "font.ttf",
            "scene.glb",
            "model.gguf",
            "a.safetensors",
            "jane.ma",
            "site.ai",
            "cover.psd",
            "winmail.dat",
        ] {
            assert!(!clipboard_type(DEFAULT_AUTO_TYPES, name), "{name}");
            assert!(!clipboard_type("*", name), "{name}, a lone star");
            assert!(!clipboard_type("* ZIP", name), "{name}, a star and more");
        }
        // Every type offered before #382 still is.
        for t in EARLIER_AUTO_TYPES[2].split(' ') {
            let name = format!("a.{}", t.replace('*', "1"));
            assert!(clipboard_type(DEFAULT_AUTO_TYPES, &name), "{name}");
        }
        // A type the user listed themselves; and one they took off.
        assert!(clipboard_type("ZIP XYZ", "a.xyz"));
        assert!(!clipboard_type("ZIP", "a.exe"));
    }

    #[test]
    fn each_refusal_has_a_name_and_a_reason() {
        for d in Declined::ALL {
            assert_eq!(Declined::from_code(d.code()), Some(d));
            assert!(d.reason().contains("Options >"), "{d:?}");
        }
        assert_eq!(Declined::from_code("FileType"), None);
        let mut r = CaptureRules {
            browsers: vec![(EDGE.into(), false)],
            ..CaptureRules::default()
        };
        assert_eq!(r.capture_off(CHROME), None);
        assert_eq!(r.capture_off(EDGE), Some(Declined::Browser));
        r.advanced_integration = false;
        assert_eq!(r.capture_off(CHROME), Some(Declined::IntegrationOff));
        assert!(!r.captures(CHROME));
    }

    /// Add browser...: a browser the user added is known by its program, whatever its
    /// User-Agent says, and has its own switch.
    #[test]
    fn a_browser_the_user_added_is_known_by_its_program() {
        use std::path::Path;
        let vivaldi = if cfg!(windows) {
            r"C:\Users\me\AppData\Local\Vivaldi\Application\vivaldi.exe"
        } else {
            "/opt/vivaldi/vivaldi-bin"
        };
        let r = CaptureRules {
            browsers: vec![("Vivaldi".into(), false), (CHROME.into(), true)],
            custom_browsers: vec![("Vivaldi".into(), vivaldi.into())],
            ..CaptureRules::default()
        };
        assert_eq!(r.custom_name(Path::new(vivaldi)), Some("Vivaldi"));
        if cfg!(windows) {
            let same = r"\\?\c:/users/ME/AppData/Local/Vivaldi/Application/VIVALDI.EXE";
            assert_eq!(r.custom_name(Path::new(same)), Some("Vivaldi"));
        }
        assert_eq!(r.custom_name(Path::new("/usr/bin/chrome")), None);
        let name = r.custom_name(Path::new(vivaldi)).unwrap();
        assert_eq!(r.capture_off(name), Some(Declined::Browser));
        assert!(r.captures(CHROME));
        // macOS: the app chosen, the program inside it running.
        assert!(same_program(
            Path::new("/Applications/Arc.app"),
            Path::new("/Applications/Arc.app/Contents/MacOS/Arc")
        ));
        assert!(!same_program(
            Path::new("/Applications/Arc"),
            Path::new("/Applications/Arc.app/Contents/MacOS/Arc")
        ));
        assert!(!same_program(Path::new(""), Path::new("")));
    }
}
