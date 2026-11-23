# Roadmap

What's done, what's being built and what's planned for Dannex Download Manager. Released
changes are in [CHANGELOG.md](CHANGELOG.md). Ideas and bug reports are welcome in
[Issues](https://github.com/Riskraptor/dannexdownloadmanager/issues).

✅ done · 🔄 being built · 📋 planned · 🔍 looking into it

## Dannex 1.0.0

The first official release.

### Done

- ✅ Downloads in several parts at once, re-balanced to the last byte, resumed after a crash, a
  restart or a dropped connection; FTP too
- ✅ Download File Info as IDM users know it: the file starts arriving while you choose, the row
  joins the list only on Start Download or Download Later, and Cancel leaves nothing behind
- ✅ Calm windows: a queue's downloads open none, downloads that finish together share one
  Download complete window, and nothing takes the keyboard from the program you're in
- ✅ File Properties on double-click, with the page each download came from; Show progress; X on a
  progress window stops the download; Columns... and calm list headers; IDM's default columns
- ✅ Queues and the Scheduler, the Speed Limiter, categories, duplicate handling, virus checking,
  a temporary folder, Add browser..., a SOCKS proxy, Arrange files by any column
- ✅ The browser extension for Chrome, Edge, Brave, Opera, Vivaldi and Firefox: one click turns it
  off or on, a status page that explains what to do, the Download video button with qualities at
  once on YouTube, right-click for any link (magnet links too) and for a page's links, images,
  videos and audio; ready for the Chrome Web Store
- ✅ Videos with yt-dlp (playlists, subtitles, the sound alone), galleries with gallery-dl, torrents
  and magnet links that say what they're waiting for
- ✅ The Site Grabber: sites that read offline, projects you can always come back to, one quiet batch
- ✅ Comet, Dannex's own icon, everywhere; the setup's own panels in light and dark; a new website
- ✅ DDM's own plain wording in every window; Dannex's names on disk and in pages (.ddm.part,
  ddm.log), with earlier files carried over
- ✅ Help → Support DDM, About and Options link to Dannex's page on GitHub Sponsors
- ✅ 86 languages, right to left, keyboard and screen readers, themes, day and dark mode, a portable
  version, a phone remote and a server mode

## Being worked on

These ship in a later version, when they're ready.

- ✅ Dark mode now shows menu and button access-key underlines only while keyboard cues are active.

- ✅ Website translations are complete for Arabic, Persian, Portuguese (Portugal), Polish,
  Bengali, Turkish and Thai.

### Planned

- 📋 Helium: DDM offering it the extension on Linux and macOS, through Helium's own services: built,
  and on once Dannex's extension is in the Chrome Web Store; until then it is added by hand
- 📋 Linux, what's left:
  screen readers (Orca) reading DDM's windows; browsers installed as a Flatpak (Firefox from
  Flathub may reach DDM through the desktop, as the Snap does: to check); native Wayland windows
  (`DDM_WAYLAND=1`) placing their menus, which needs popup windows the window library doesn't
  have yet, so XWayland stays the default; Vulkan as the default renderer (below)

### Shipped, still to confirm

In the code of Dannex 1.0.0; each waits on a check that only real hardware, a real site or the
person who reported it can make.

- 🔍 macOS: video look-ups in a copy of DDM a browser downloaded, without the Terminal command: yt-dlp's folder build,
  unpacked once and checked against its signed list; built and checked on macOS, still to confirm on
  a person's Mac
- 🔍 The Safari extension (macOS), part of the app: a click on a link to a listed file type, the
  right-click items, the button over videos and the toolbar button, from one
  source with the other browsers' extension. Safari gives extensions no way to see its downloads, so
  a download a page starts by itself stays with Safari. Built and its relay to the helper checked in
  the sandbox Safari uses; still to check in Safari itself, signed with a Developer ID
- 🔍 The browser's video button over a video in a feed sending the feed's address, where yt-dlp finds
  nothing (LinkedIn, Facebook, and other sites with feeds;
  ): it now sends the post the
  video sits in (LinkedIn, Facebook, X, Instagram, Reddit, TikTok, Vimeo, Dailymotion, and a post's
  own link on other sites), else the video's own file, else it says to open the post; fixed, checked
  on a signed-in LinkedIn feed and Facebook's video page; Facebook's feed posts still to check
- 🔍 Downloads from FileHippo failing at once with 412 Precondition Failed: a link the site refuses
  to DDM now goes on from where the browser's own download of it had reached; fixed, still to check
  on FileHippo itself. A link pasted into Add URL has no such address to go on from
- 🔍 The window not answering while a file was dragged out of Download complete, for as long as the
  program under the pointer took: the drag now runs beside
  the window; fixed, to check with a real drag
- 🔍 A progress window's title bar buttons not answering: a window with a title of
  256 characters or more got the system's title bar above DDM's own; that part is fixed, shipped.
  Still to find: whether the reporter's window was one of them
- 🔍 macOS: Persian and other Arabic-script file names drawn as boxes in the download list: fixed, to confirm on a
  Mac
- 🔍 macOS: no icon in the menu bar, so closing the window quit DDM: fixed, still to confirm
  on a Mac
- 🔍 Windows: closing the window left DDM minimized in the taskbar instead of the notification area
  while its icon was there, among the hidden icons behind ^ (): DDM now keeps the window
  in the taskbar only when Windows says it has no icon of DDM's, not when Windows can't say where
  the icon is; fixed, to confirm on Windows 10
- 🔍 The window not answering for a few seconds while Windows makes a snapshot of the disk (a restore
  point or a backup): the
  log and the RSS feeds are now written beside the window, as the list already was; fixed, to
  confirm on the reporter's computer
- 🔍 Malay: a native speaker's check. The menus, toolbar and list columns were reviewed; 28 rows
  added since (menu items, the Grabber button, the torrent columns) are still machine-made

### Open questions

- 🔍 A 10-second freeze of the window after a night left open: most likely the computer
  waking from sleep; waiting on the reporter's full report

## Planned

### Look and performance

- 📋 Vulkan the default on Linux and Metal on macOS, once real graphics cards have tried them (AMD,
  Intel and NVIDIA, X11 and Wayland, older cards without Vulkan)
- 📋 Benchmarks against other download managers: IDM's so far
  (`tools/bench/`); the others' still to add
