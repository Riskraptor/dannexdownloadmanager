# Changelog

Each release's notes on GitHub come from its section here (`## [X.Y.Z]`, see
`.github/workflows/release.yml`).

## [Unreleased]

- Fixed dark-mode access-key cues so mnemonic letters are underlined only after the keyboard
  is used, instead of being always underlined.
- Completed the website translations for Arabic, Persian, Portuguese (Portugal), Polish,
  Bengali, Turkish and Thai. Claude Sonnet 5.5 filled the remaining texts and the catalog
  validator and strict kept-name checks passed.

## [1.0.0] - 2026-11-23

The first official release of Dannex Download Manager: a free, open-source download manager for
Windows (with Linux and an experimental macOS version), laid out the way Internet Download Manager
users expect, and faster.

### Downloads

- **Fast:** every download is fetched in several parts at once, re-balanced until the last byte, and
  resumes after a crash, a restart or a dropped connection without starting over.
- **Download File Info, the way you know it:** the file starts arriving while you choose where it
  goes, and it joins the list only once you click **Start Download** or **Download Later**;
  **Cancel** leaves nothing behind.
- **Calm when many arrive at once:** a queue's downloads run without opening windows, and downloads
  that finish together share one **Download complete** window ("3 more downloads complete").
- **Queues and the Scheduler** start and stop on their own, can wake the computer, and turn it off
  when done. **Speed Limiter**, categories with their own folders, and duplicate handling.
- **Virus checking** after each download with the antivirus you choose (one click for Microsoft
  Defender), and a **temporary folder** of your choice for files still downloading.

### In your browser

- **The DDM Integration Module** for Chrome, Edge, Brave, Opera, Vivaldi and Firefox hands your
  downloads to DDM with their sign-in. One click on its toolbar button turns it off or on; when
  DDM isn't installed, its page says so and turns green by itself once it is.
- **A Download video button** over videos, whose qualities show at once on YouTube.
- Right-click **Download with DDM**, for any link, magnet links included, and **Download all
  links, images or videos and audio** of a page.

### Video, galleries, torrents and sites

- **Videos** from thousands of sites with yt-dlp, with playlists, subtitles (embedded or beside the
  file) and the sound alone; **image galleries** with gallery-dl, through DDM's own downloader.
- **Torrents and magnet links**, with sequential order, RSS feeds and sharing limits; the Add
  torrent window says what it is waiting for.
- **Site Grabber:** a whole site or part of one, to read offline, in projects you can always come
  back to under **Grabber projects**.

### The app

- **A new icon, Comet**, everywhere: the app, the tray, the extension, the setup and the website.
- **File Properties** on double-click, with the page each download came from; **Show progress**
  brings back a hidden progress window; **Columns...** shows, hides and orders the list's columns.
- Minimize keeps DDM on the taskbar; closing it keeps it running in the notification area.
- Day and dark mode and themes, 86 languages including right to left, keyboard and screen-reader
  access, a portable version, a phone remote and a server mode.
- **Help → Support DDM** opens Dannex's page on GitHub Sponsors.
