# Architecture

How Dannex Download Manager's pieces fit together.

## Crates

| Path | Crate | Role |
|---|---|---|
| `crates/engine` | `mdm-engine` | Downloads one file over HTTP(S) or FTP(S): probe, segment planner, connection workers, resume state, per-host limits, speed limiter, checksums, proxy scripts |
| `crates/app` | `mdm-app` | The desktop app (`dannexdownloadmanager.exe`, egui): list, categories, queues, scheduler, dialogs, tray, settings |
| `crates/cli` | `mdm-cli` | Headless downloads with progress (`get`, `resume`), videos, torrents and the automation API's commands: a library the app's program runs (`dannexdownloadmanager --cli …`), and `dannexdownloadmanager-cli`, a small launcher that starts it with the same arguments, terminal and exit code |
| `crates/mcp` | `mdm-mcp` | The MCP server (`dannexdownloadmanager-mcp`): AI assistants list, add and control the running app's downloads through the automation API |
| `crates/remote` | `mdm-remote` | The remote control (#141): the HTTPS server a paired phone or computer on the local network uses (its page, pairing codes, device tokens, the self-signed certificate); every request becomes an automation API command the app answers |
| `crates/nmhost` | `mdm-nmhost` | The browser's native messaging host: takes captured downloads from the extension and hands them to the app |
| `crates/ipc` | `mdm-ipc` | The single-instance channel (a token-protected local socket), the data folder, and the capture rules shared by the app and the host |
| `crates/debrid` | `mdm-debrid` | Debrid accounts (#44): the services' official APIs (Real-Debrid, AllDebrid, Premiumize, TorBox) turn a file host's link into a direct one the engine downloads |
| `crates/extract` | `mdm-extract` | Archive extraction (#37): zip, tar, gz and xz in process; 7z and RAR through an installed 7-Zip, run as its own program |
| `crates/geo` | `mdm-geo` | Where a download's server is (#140): an IP-to-country range table built from DB-IP's Lite data (`tools/geo/build.py`), looked up on this computer only, and the countries' flags |
| `crates/tools` | `mdm-tools` | Video pages through yt-dlp and ffmpeg, image galleries through gallery-dl (run as separate programs), installed from their releases and checked against their publishers' signed checksums (`needed.rs`: the ones DDM can't do without); while a network adapter is chosen, their connections go through a proxy of DDM's on 127.0.0.1 that binds them to it (`relay.rs`) |
| `crates/crash` | `mdm-crash` | Crash and freeze reports: the monitor process, out-of-process minidumps, symbolicated stacks, the report's text and the release logs ([crash-reports.md](crash-reports.md)) |
| `crates/torrent` | `mdm-torrent` | BitTorrent and magnet links through librqbit; web seeds (BEP 19 `url-list`, which librqbit 9.1 doesn't read) served to it as a peer of DDM's own on 127.0.0.1 that fetches the pieces asked for by HTTP ranges, librqbit checking each (`webseed.rs`) |
| `crates/labserver` | `mdm-labserver` | An HTTP Range server with a request log and fault injection, Metalink documents of its files (`/metalink/`), and an FTP/FTPS server with its own faults, for tests and benchmarks |
| `extension/` | — | The Chromium (MV3) and Firefox extension |
| `docs/` | — | The user guide and tutorials, this file, the benchmark data; the website makes the guide its help pages, which Help opens |
| `website/` | — | dannex.kavangi.co.ke's own pages (home, download, features, themes, FAQ); `tools/website/build.py` adds the help from `docs/` and the news from the CHANGELOG, in 22 languages, and GitHub Pages publishes it |

Dependencies point inward: `app`, `cli`, `mcp`, `nmhost` and `remote` use `engine` and `ipc`; the engine knows
nothing of the UI. The app sees the engine only through its events (progress, segment table
snapshots, finished, failed).

## Engine

```
Download
 ├─ probe()              size, resumable?, validators (ETag / Last-Modified), file name
 ├─ Planner (pure)       segment table: when a connection is free, the largest remaining
 │                       part is split in half, unless a half would arrive sooner where it
 │                       is (planner.rs, tested by random simulation)
 ├─ Worker × N           one TCP connection each (its own client, a lane: HTTP/1.1, or
 │                       HTTP/2 when the server offers it), reused across segments; slow
 │                       or stalled ones reconnect for the rest of their part
 │    └─ writes at its offset into <name>.ddm.part (created sparse)
 ├─ HostLimits           learns how many connections each server accepts
 ├─ Limiter (shared)     per-download and global token buckets
 ├─ State                segment table saved atomically (craft-atomic) for crash-safe resume
 └─ events → channel     Progress, Done, Failed
```

When every segment is done, the part file is checked against the expected size and renamed into
place; the file's date can follow the server's.

Each connection has its own HTTP client (`Lanes`, `download/clients.rs`): hyper's pool sends every
request to a host over one HTTP/2 connection, so a shared client would turn an HTTP/2 server's
8 connections into 8 streams of one TCP connection, at one connection's speed. On its own
connection, HTTP/2 keeps its gain for a download: a part cut short by a split ends its stream,
and the warm connection serves the next request. The lanes time their own silence, the waits
for the disk left out (reqwest's read timeout runs while a write is held up). A connection that
goes silent while the others receive reconnects after a few seconds (`stalled`) instead of
waiting out the read timeout. A file up to 16 MiB starts on its first connection, and the others
open once its pace says they would finish it sooner (`ramp`: a new connection costs its time to
first byte); a Metalink download's mirrors are asked from the start.

A download's first request (`get`) follows redirects and the pages that lead on to a file by
themselves (a waiting page's refresh). Sites whose links aren't the file's address have a module
in `sites/` (Google Drive: `sites/gdrive.rs`): it turns the link into the file's address, reads
the site's own pages (a confirmation form, an error page) and names its refusals
(`Error::Refused`), with pure functions tested without a network. The lab server plays such a
site on its real host names for the end-to-end tests (`labserver/src/drive.rs`).

FTP and FTPS (`ftp.rs`) go through the same planner, workers and state: a segment is `REST` +
`RETR` on a control session instead of a ranged `GET`, `SIZE` and `MDTM` stand in for the
headers, and sessions are pooled per download and reused between segments.

Metalinks (`metalink.rs`: Metalink 3 and RFC 5854's Metalink 4) are read into files, each with
its sources (priority, country), size, strongest whole-file hash and piece hashes, and a name
that may not leave its folder. The app makes each file a download: `Options::mirrors` are its
other sources, used only when `Options::checksum` or `Options::pieces` can verify them. The
connections take turns among the sources (`Ctx::source`); a mirror is asked without the main
address's validators, and one that fails (no answer, a refusal, no ranges, another file) is left
out for the rest of the download (`Error::MirrorDropped`, never a failure of the download). At
the end the file is checked against its hash; with piece hashes, the pieces that fail
(`verify::bad_pieces`) are downloaded again in another round of connections over them alone,
the sources that sent them left out (`Ctx::blame`), up to three rounds (`verify_and_repair`).

## App

One egui window painted to fixed pixel coordinates, plus one immediate viewport per dialog.
`app.rs` and its modules (`app/`, see [Source layout](#source-layout)) hold the state
(downloads, queues, settings, the list's selection) and turn menu, toolbar, key and IPC commands
into actions. Each dialog is its own module and returns an
`Outcome` (open, closed, done with a value). A window that comes up by itself (Download
complete, a message DDM tells, a question while DDM isn't the program in front) shows above the
others without taking the keyboard from the program being typed in (`dialogs/viewport.rs`, `Kind`); those
the person asked for take it. The windows handed over one after another (Download File Info, Add
torrent, Download video) and Download complete boxes show one at a time, in one place, the next
when one closes. Windows-only calls are confined to `win32.rs` (and its modules in
`win32/`) and their Linux and macOS counterparts to `unix.rs` (starting with the session, the browsers' host,
sounds, show in folder, the crash handler); every other file is safe, portable Rust. Fonts come from the system:
Windows' Fonts folder, macOS's system fonts, fontconfig on Linux (`theme.rs`). On Windows, Latin,
Greek and Cyrillic text is drawn by DirectWrite (`win32/text.rs`, through egui's glyph rasterizer hook):
View → Text → Smooth in its natural mode, anti-aliased at the size asked for and placed at quarter
pixels with the font's own advances; Classic (the default) in GDI-classic mode, so each letter's
hinted shape and whole-pixel width come together. Other scripts are shaped and drawn by egui.

### Source layout

A large module is a folder of files by responsibility, its top-level file the module's root:
`app.rs` with `app/`, `winui.rs` with `winui/`. The root keeps the module's imports, what its
parts share and what the rest of the program uses (re-exported, so a path such as
`crate::winui::button` or `mdm_engine::download::Options` is the same as before); each part takes
the root's names with `use super::*`, and what one part gives the others is `pub(super)`. The
catalog's extractor reads a folder with its root as one module (`tools/i18n/sources.py`), so a
text's note says where it shows wherever its code sits.

| Module | Its parts |
|---|---|
| `crates/app/src/app.rs` (the main window) | `state` (the `App` struct, `new`, `Drop`), `frame` (what runs at every frame, saving), `commands` (`Cmd`, `run`); the window drawn: `list_view`, `tree`, `paint`, `menus`, `view_menu`; file downloads: `downloads`, `duplicates`, `disk`, `moves`, `renames`, `removal`, `speed`; `video_cmds`, `torrent_cmds`, `rss_feeds`; the windows: `windows` (opening them), `answers` (their outcomes); `import_export`, `incoming` (the command line, the host, the automation API, paired devices), `beside` (the disk asked beside the window), `options_apply`, `queues`, `tray_icon`, `watch`, `sitegrab`; `tests/`. Each is an `impl App` block with the types and helpers that go with it |
| `crates/app/src/winui.rs` (the controls) | `fonts`, `surface` (right to left), `paint`, `text`, `paragraph`, `bars`, `rows`, `edit`, `combo`, `buttons`, `checks`, `keys`, `a11y`, `wheel`, `pick`, `tabs`, `caption`, `tests`; the theme's colours stay in `winui.rs` |
| `crates/app/src/chrome.rs` (the main window's chrome) | `icons`, `menu_bar`, `popup` (popup menus), `toolbar`, `categories`, `tests` |
| `crates/app/src/progress.rs` (a download's progress window) | `glide` (the bar's glide), `status`, `limits` (Speed Limiter, on completion), `torrent`, `connections`, `tests` |
| `crates/app/src/options.rs` (Options) | one module per page (`general`, `file_types`, `save_to`, `downloads`, `connection`, `proxy`, `logins`, `accounts`, `sounds`, `regional`, `archives`, `remote`), `sub` (the windows over it), `controls` (the pages' own controls) |
| `crates/app/src/dialogs.rs` (the small dialogs) | `viewport` (each dialog's window), `add_url`, `file_info`, `complete`, `speed`, `turn_off`, `confirm`, `files`, `folders`, `tests` |
| `crates/app/src/win32.rs` (Windows' calls) | `own_windows`, `frame`, `caption`, `tray`, `icons`, `popup`, `drag_out`, `system`, `taskbar`, `registry`, `shell`, `startup`, `process`, `keychain`, `power`, `text`, `ole` |
| `crates/engine/src/download.rs` (one download) | the API: `options`, `error`, `events`, `handle`, `probing`; how it runs: `clients`, `connections`, `first` (the first request), `plan` (a fresh download to the planner), `mirrors`, `resume`, `repair` (verification and repair), `transfer` (one connection's work), `pages`; `tests` |
| `crates/tools/src/video.rs` (yt-dlp) | `subtitles`, `lookup`, `files`, `supervise`, `meter`, `by_engine`, `tests` |
| `crates/nmhost/src/main.rs` (the native host) | `wire` (the messages), `caller`, `answer`, `menu` (the video button's menu), `deliver` (to the app), `register`, `tests` |
| `extension/background.js` (the extension's background) | plain scripts in `extension/background/`, run in order in one scope: `core`, `native`, `remote`, `status`, `copies`, `rules`, `requests`, `tab-media`, `cookies`, `proxy`, `capture`, `opened`, `firefox-answers`, `menus`, `start`, `messages`, `lookup`, `played`, `panel`, `main`. Chromium's service worker (`background.js`) imports them; Firefox's and Safari's manifests list them; `tools/extension/test.mjs` and `check-pack.mjs` keep the three lists the same |

### Themes

Every colour and size a control is drawn with comes from the active theme (#45). A theme is a
light and a dark palette, metrics, control styles and an icon style (`palette.rs`):

- **Palette roles.** `palette.rs` declares each role with a stable key (`"button.hot"`,
  `"list.row"`, `"caption"`…), a description and the classic look's light and dark values,
  written out (the classic look isn't derived). The shared painters (`winui/`, `chrome.rs`,
  `list.rs`, `scrollbar.rs`) read them through `theme::colors()` and `theme::metrics()` and decide
  each control's states from them; no painting code holds a colour of its own.
- **Derived palettes.** `derive.rs` makes a whole palette from a few seeds (background, surface,
  text, accent…), mixing every other role in OKLab and keeping WCAG contrast (4.5:1 for text, 3:1
  for edges); High contrast strengthens every edge on top. `native.rs` seeds the Native theme from
  the system's accent and contrast (Windows' settings, the freedesktop portal on Linux, macOS's
  accent).
- **Control styles.** `palette::Controls` names each control's shape (text boxes, check boxes,
  buttons, progress bars, scroll bars, the focus mark, the title bar); the style themes (Metro,
  Adwaita, Breeze, Luna, Aero, Aqua, Emerald) pick theirs, and on Windows DDM then draws the
  title bar itself (the `caption.*` roles).
- **Theme files.** `themefile.rs` reads TOML or JSON from the data folder's `themes` (one file,
  or `<id>/theme.toml` with icons beside it). A file names only what it changes; the rest comes
  from the theme it `inherits` (a built-in's id or another file, the classic look by default),
  down a chain in which a cycle is refused. Unknown keys are warnings, not errors. The built-in
  style themes are such files too, carried in the program (`crates/app/assets/themes`).
- **Icon sets.** `themeicons.rs` draws a theme's icons (named by what they're for: `tb-add-url`,
  `cat-music`…, SVG or PNG at 16, 24, 32 and 48 px, `-symbolic` ones inked in the text colour)
  where the classic look draws its own; a need the set lacks keeps the classic icon.
- **Toolbar icon styles.** View → Toolbar → Line style (the outlines, inked as the theme says), 3D
  style (DDM's own glossy pictures, `assets/icons/toolbar3d`, drawn by `tools/icon/toolbar_3d.py`
  at 16 to 64 px, the one for the size on screen; greyed by `icons.rs` when disabled) or the
  theme's own icons (a style theme's set: its default). A theme inking icons in one colour (High
  contrast) keeps its outlines.
- **Each theme's own choices** (`look.rs`). The settings keep, by theme id, the accent colour and
  the toolbar's icons chosen for it, taken back when it is picked again. The accent is laid over
  the theme as it is made active (`theme::set`, so a pywal or Native theme followed again keeps
  it): every role of the accent's hue family (OKLCh, within about 34°) takes the new hue and
  colourfulness, strong ones its lightness too; semantic roles (errors, warnings, success) stay;
  the accent, links and text on accent fills are then kept readable (`derive::readable`).
  View → Font → Compact / Comfortable rows (`look::Density`) add room to the download list's and
  Link grabber's rows.
- **The theme picker** (`themepick.rs`): every theme drawn as a miniature of the main window from
  its own palettes (both sides, in its accent), not by switching the active theme; theme files are
  read beside the window. Its choices run as the View menu's commands. `themefile::export` writes
  the active theme out whole (every role of both sides, sizes, control styles, icon style and a
  built-in icon set's name) on the classic look, so the file reads back the same anywhere.

Where a window puts its controls stays its layout. So do the icons
before file names (`fileicons.rs`): the Windows shell's system image lists, macOS Finder's
(NSWorkspace), the Linux desktop's icon theme (`xdgicons.rs`, shared-mime-info and the Icon
Theme Specification). Each type is looked up once per pixel size, on a background thread. Tango's
icons (bundled) stand in until the system's icon is ready, and wherever the system has none.

### Drawing

The windows are drawn with OpenGL (eframe's glow) or with the system's own graphics API through
eframe's wgpu, with that one backend only: Direct3D 12 on Windows (the default there), Vulkan on
Linux, Metal on macOS (choices there; OpenGL stays their default until real graphics cards have
tried them, #206). View → Draw with picks one, from the next start. One code path does it on
every system (`crates/app/src/renderer.rs`): when the chosen one can't start, the other one is
tried, so the windows always open. A watched start with the system's API leaves a marker until
its first frame is drawn: a start that ends before it (a crash inside the driver), one whose
first frame doesn't come within 10 seconds (DDM then starts again with OpenGL), or a graphics
card lost twice in 10 minutes is remembered with its reason, which the menu shows.

View → Interface size (`scale.rs`, #262) is egui's zoom factor over the display's own scale:
every window is laid out in points, so it opens that much larger; one larger than the screen
scrolls.

### Data

The download list and settings are saved atomically to the data folder (`downloads.json`):
`%APPDATA%\dannexdownloadmanager\` on Windows, `~/.local/share/dannexdownloadmanager` on Linux,
`~/Library/Application Support/dannexdownloadmanager` on macOS, a `data` folder beside a portable
copy, or `DDM_DATA_DIR` (`mdm_ipc::data_dir`). The browser host reads the same file for the
capture rules, so a change in Options applies to the next captured download.

Sign-in secrets (debrid accounts, Sites Logins' and the proxy's passwords) live in the system's
keychain (`secrets.rs`, #44): Windows Credential Manager (`win32.rs`), the Secret Service on Linux
or the macOS keychain (`unix.rs`). The settings keep a `keychain:<service>/<scope>/<key>`
reference in their place, written only once the keychain has confirmed the secret; without a
keychain (or with a locked one, never unlocked for DDM) the secret stays in the settings as
before.

The video tools (yt-dlp, and ffmpeg on Windows) live in the data folder's `tools` and are never
asked for (`autohelpers.rs`). The Windows setup runs the installed program with
`--install-helpers`, which gets what is missing with `mdm_tools::needed` and prints its progress
line by line for the wizard's bar (`tools/installer/mdm.iss` reads it, under Inno's own
"Finishing installation...": the windows never name or explain the helpers, only Helpers... and
the logs do). Whatever is still missing
when the app runs (the setup was offline, a zip copy, Linux and macOS) a background task gets: at
once while the network is up (the adapter watch's route), again after a failure with a doubling
wait up to 10 minutes, and at once when the route changes. A video started meanwhile waits in the
list ("Waiting") and starts once yt-dlp is there; the browser's quality menu sends the click to
the Download video window, which says "Getting ready..." and looks the page up once it can.

A download's server location (#140) is the country of the address it connected to, looked up in
`mdm-geo`'s table on this computer (`countries.rs` names it in the interface language); the
release job builds the table again from the month's data (`tools/geo/build.py --release`).

## Browser integration

```
page ──► extension ──native messaging──► mdm-nmhost ──ipc──► app
```

The extension asks the host which file types to take, checks each download against them, and
cancels the browser's download only after the app has accepted it; otherwise the browser keeps
it, and the extension says why.

The same answer carries Options → General's keys (`keys`: what page.js holds a click by, to
leave a link to the browser or force it to the app) and the right-click items to show (`menu`:
the background makes its menu anew when they change, `background/menus.js`); the answer to whether the video button may
show carries Download panels' settings (`panel`: over videos or sound players, the least length,
the first corner). Their names and defaults are `mdm_ipc::capture`'s, read from the settings the
app saves. The extension asks again when the browser's window gets the focus back (a few seconds
after it last asked), when a page starts (15 s), and at a download (a minute).

The extension shows almost nothing of its own, as users of other download managers expect: a
click on its toolbar button turns taking downloads off and on, its right-click menu has "Don't take
downloads from this site" and Help (`extension/install.html`, the status page: installed, or what
is wrong and the one thing to do), and its options page (`options.html`) has only that switch, the
browser's proxy (#42) and another DDM (#157). It never asks the app for its downloads (extensions up
to 0.6.5 did, for a window of their own: the host refuses that question now, and the app answers
the automation API's `list` and actions only while the API is on).

The host serves only DDM's own extension. Its IDs are in one place, `mdm_ipc::extension`
(`crates/ipc/src/extension.rs`): the unpacked copy's (`UNPACKED_ID`, from the manifest's public
key), the Chrome Web Store's (`STORE_ID`, signed with the store's own key: `pack.ps1` leaves the
manifest's key out of the store's copy), the Firefox add-on's (`FIREFOX_ID`) and its
addons.mozilla.org page; the Edge and Opera stores' are placeholders, never served, until the
extension is listed there. The host writes each browser's manifest with those IDs, and the app
offers the store's copy (Add to Chrome, Add to Firefox) before the unpacked one.

## Remote control

```
phone ──HTTPS (paired token)──► mdm-remote ──ipc::Sink──► app (answer, remote_add)
```

`--headless` (`headless.rs`, #157) runs the same `App` without eframe: a loop calls its
`tick` (what `logic` runs at every frame) on a bare `egui::Context`, woken by the command
channel, and never its `ui`; commands are added as the remote control adds them (no window).
The `Dockerfile` and `docker/` (Compose, Unraid's template) run it on a server
([server.md](server.md)).

The server runs on the app's Tokio runtime, one listener per local network address. Each request
is checked (`Host`, `Origin`, token), turned into an `mdm_ipc::Command` marked `remote` (a field
no request line can set) and handed to the app through the same channel as the command line's;
the app answers it on its next frame, as it answers the automation API. Its page is built in
(`crates/remote/web`) and its texts come from the app's catalogs (`remote.rs`).

## Translations

Every text the UI shows goes through `crate::i18n` (craft-i18n catalogs, `i18n/<locale>.tsv`).
`tools/i18n/extract.py` rebuilds `i18n/en.tsv` from the source, with a note for translators on
each text (what it is, where it shows, its placeholders). `tools/i18n/translate.py` fills what
the other catalogs lack (rows marked `#, machine`), following each language's style guide
(`i18n/style.json`) and the glossary; a row a person reviewed is never replaced, and CI
(`reviewed_guard.py`) fails a change that drops one. The website has its own catalogs
(`i18n/site/`, `tools/website/translate_site.py`).

## Pictures

The app icon, Comet, is drawn from geometry as SVG (`tools/icon/comet_logo.py`, rendered by
resvg; `build_icon.py` writes every size, placed for whole pixels from 16 to 32 px). Every place that shows it takes the drawing
for its size on screen (`logo.rs`): on Windows each window's title bar and taskbar icons at its
display's scale (`win32::crisp_icons`, which also keeps them through a move to another display),
the tray's at the taskbar's. View → DDM tray icon shows it in full colour (3D style) or the mark
alone in the taskbar's ink (Flat style: white on a dark taskbar, dark on a light one; a
template image on macOS), with a slim bar along its foot while downloads run. Its tip lists each
running download (`tray::tip_text`, within Windows' 127 characters); its menu pauses and resumes
everything and sets the Speed Limiter's speed. Notify when downloads complete (`notify.rs`)
gathers downloads that end close together into one quiet notification: on Windows the shell's
(`Shell_NotifyIcon`'s NIF_INFO, no sound, respecting quiet time, from the tray's watcher thread
since it waits for Explorer), on Linux the desktop's (org.freedesktop.Notifications over D-Bus),
on macOS AppleScript's `display notification`.

Link grabber (`grabber.rs`, its window `grabview.rs`) is built for pages of thousands of links:
the rows, counts by kind of file and duplicates are made again only when something changed (a
generation number), and only the rows in view are drawn; look-ups run sixteen at a time (six per
host) and answer on a channel; whether a file is already on the disk is asked on a thread of its
own (#743). The extension sends each link's text with it (`links` and `texts` in step, the
native host keeping them so as it drops links it doesn't download: `Command::link_texts`).

The help pages' pictures (`docs/images/help/`) come from one list, `tools/shots/shots.toml`:
`tools/shots/make.py` opens each window through DDM's own `/show <window>` command line in a test
instance off the screen, crops and annotates it from the window's accessibility tree, and writes
lossless WebP; `make.py check` (CI) keeps the list, the pages and the files in step. The
published ones are taken on GitHub's Windows runner (`docs-shots.yml`).

## Contributors

`tools/contributors.py` reads who opened pull requests and issues, and who commented, from
GitHub's public API (the maintainer, Claude and bots left out) for every place that thanks them:
the website's home page, `/contributors.html` and each release on the News page (`build.py`, on
every build), each release's notes (`release.yml`, the people active since the release before)
and the README (`python tools/contributors.py readme` at each release's preparation: its round
photos are committed in `art/contributors/`, since GitHub can't round a picture in a README).

## Tests and benchmarks

`cargo test` runs unit tests and end-to-end downloads against `mdm-labserver` with injected
faults (slow, dropped and refused connections, servers without ranges, changing files).
`crates/engine/tests/resume.rs` pulls the network out mid-download (a proxy between the engine and
the lab server, its connections broken and refused, then put back) and checks that a resume
continues where it was, never from zero while it can, that an outage teaches no connection limit,
and that a server that stopped sending parts of the file gets it again from the start;
`tools/lab/resume-cases.ps1` runs those cases, a sleeping PC and a killed DDM in the real app.
`tools/bench` runs the published speed comparison; its raw data is in `docs/bench/`.

The windows' frames are measured, not guessed (`crates/app/src/framestats.rs`):
`MDM_FRAME_STATS=1` logs, every second, the main window's passes, how many came from input, what
they cost (the main window, the progress windows, their layout) and which lines of code asked for
the others; `MDM_SYNTHETIC_LIST=10000` (with a scratch `MDM_DATA_DIR`) adds that many made-up rows;
`MDM_BENCH_SCROLL=10` scrolls the list by itself for ten seconds and logs the frames' times. An idle
window has no frames but its clock's (one every 2 s); the list makes only the rows in view.

The time from a browser's download to Download File Info and to the file's first bytes is logged
step by step (`start time:` lines: the host's hand-over, the window's pass that takes it, the
dialog shown, the address answered, the early start, the first bytes; `starttime.rs`), and
`tools/lab/start-latency.ps1` reads them over a run of its own (a browser's download, an address,
or the video button's menu). eframe holds a hidden or minimized window's passes 100 ms apart; while
the main window is in the notification area, a command that arrives, a window that opens and an
answer a dialog waits for run its next pass at once instead (`win32::nudge_main_window`).
