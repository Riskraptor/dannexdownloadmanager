# Roadmap

What's done, what's being built and what's planned for Dannex Download Manager. Released
changes are in [CHANGELOG.md](CHANGELOG.md). Ideas and bug reports are welcome in
[Issues](https://github.com/Riskraptor/dannexdownloadmanager/issues).

✅ done · 🔄 being built · 📋 planned · 🔍 looking into it

## Dannex 2.0.0

The first release.

### Done

- ✅ Downloads in several parts at once, each over its own connection, re-balanced to the last
  byte; a connection that goes quiet replaced within seconds; small files at full speed from the
  start; FTP too
- ✅ Resumes that hold: after a crash, a restart, a sleeping computer or a dropped connection;
  stopped downloads go on by themselves when the network is back; a server that stops sending
  parts sends the file again whole
- ✅ Download File Info as IDM users know it, opened the moment a download is handed over: the file
  starts arriving while you choose, the row joins the list only on Start Download or Download
  Later, and Cancel leaves nothing behind
- ✅ Every connection numbered in the progress window, however many are allowed
- ✅ Calm windows: a queue's downloads open none, downloads that finish together share one
  Download complete window, and nothing takes the keyboard from the program you're in
- ✅ File Properties on double-click, with the page each download came from; Show progress; X on a
  progress window stops the download; Columns... and calm list headers; IDM's default columns
- ✅ Queues and the Scheduler, the Speed Limiter, categories, duplicate handling, virus checking,
  a temporary folder whose leftovers are always cleaned up, Add browser..., a SOCKS proxy, Arrange
  files by any column
- ✅ Metalink files: Tasks → Open Metalink file... and Add Metalink for files of several parts,
  the publisher's mirrors, every file checked and only its bad parts fetched again
- ✅ File missing: a finished download whose file was moved or deleted offers Redownload or
  Locate...
- ✅ The browser extension, in the Chrome Web Store (Chrome, Edge, Brave, Opera, Vivaldi) and for
  Firefox: one click turns it off or on; the Download video button over videos and in feeds
  (YouTube, TikTok, X and more), with qualities at once and the sizes DDM downloads; right-click
  for any link (magnet links too) and for a page's links, images, videos and audio
- ✅ Videos and music from thousands of sites (YouTube Music, SoundCloud, Bandcamp, Boomplay among
  them), galleries, and copy-protected music and video said plainly
- ✅ Torrents and magnet links that say what they're waiting for, web seeds, and Torrent → Play
  while downloading (a player on this computer only, with seeking)
- ✅ The Link Grabber: columns, a button per kind of file, search, duplicates grouped, thousands of
  links at once
- ✅ The Site Grabber: sites that read offline, projects you can always come back to, one quiet batch
- ✅ Themes chosen from live pictures, with their own accent, icons, text size and row height, shared
  as a file; the notification area's tip and menu; What's new once after an update
- ✅ Comet, Dannex's own icon, everywhere; a new website of its own
- ✅ A setup in your language that installs everything for videos and music, finishing by itself
  once online, and adds the extension to your browser
- ✅ DDM's own plain wording in every window; Dannex's names on disk and in pages (.ddm.part,
  ddm.log), with earlier files carried over
- ✅ Help → Support DDM, About and Options link to Dannex's page on GitHub Sponsors
- ✅ 86 languages, right to left, keyboard and screen readers, day and dark mode, a portable
  version, a phone remote and a server mode

## Being worked on

These ship in a later version, when they're ready.

### Planned

- 📋 **Major:** the setup closes a running DDM by itself. Today, running the downloaded setup
  while DDM is open leaves DDM running (its request to quit doesn't reach it), so a silent setup
  stops and an interactive one asks to close DDM and retry. Updating from inside DDM (Help →
  Check for updates) isn't affected
- 📋 Linux, what's left:
  screen readers (Orca) reading DDM's windows; browsers installed as a Flatpak (Firefox from
  Flathub may reach DDM through the desktop, as the Snap does: to check); native Wayland windows
  (`DDM_WAYLAND=1`) placing their menus, which needs popup windows the window library doesn't
  have yet, so XWayland stays the default; Vulkan as the default renderer (below)

### Shipped, still to confirm

In the code of Dannex 2.0.0; each waits on a check that only real hardware, a real site or the
person who reported it can make.

- 🔍 Helium on Linux and macOS: DDM offers it the extension through Helium's own services, now that
  the extension is in the Chrome Web Store; to confirm in Helium itself
- 🔍 Videos and music on sites that need a sign-in or show a robot check (Instagram, Facebook,
  LinkedIn, Pinterest, Reddit, SoundCloud, Boomplay): built and checked on test pages; to confirm
  signed in on each site

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

- 📋 Pictures of DDM on Linux (GNOME, KDE Plasma, Cinnamon, Xfce) taken again with 2.0.0 on the
  Linux test machines (`tools/lab/linux-vm`), for the README and the getting-started help
- 📋 Vulkan the default on Linux and Metal on macOS, once real graphics cards have tried them (AMD,
  Intel and NVIDIA, X11 and Wayland, older cards without Vulkan)
- 📋 Benchmarks against Motrix and Free Download Manager: `tools/bench/others.py` times DDM, IDM,
  aria2 and curl today ([docs/bench](docs/bench/)); Motrix needs its own RPC running, and Free
  Download Manager has no command line to time, so it is compared by hand
- 🔍 HTTP/3: not now. DDM's HTTP library offers it only as unstable code, and on the measured line
  it brought nothing over DDM's separate connections
  ([docs/bench/speed-2026-11-27.md](docs/bench/speed-2026-11-27.md#appendix-a-http3-an-evaluation))
- 📋 Torrent web seeds of the older, rarer kind (BEP 17): DDM reads the common kind (BEP 19)

### Waiting on outside accounts

- 📋 The extension in Firefox Add-ons and Microsoft Edge Add-ons: one form per store
  ([PUBLISHING.md](PUBLISHING.md)); Edge already installs it from the Chrome Web Store
- 📋 The Safari extension and the Mac version signed with an Apple Developer ID
