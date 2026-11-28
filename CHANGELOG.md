# Changelog

Each release's notes on GitHub come from its section here (`## [X.Y.Z]`, see
`.github/workflows/release.yml`).

## [2.0.1] - 2026-11-28

- **Play while downloading** no longer closes DDM: it opens the torrent's video or sound in your
  browser's player as it should.
- **Easier to find:** a running torrent's progress window has a **Play while downloading** button,
  and it is the first item when you right-click a torrent with a video or sound file.
- **Updates on Linux (.deb and .rpm):** **Update now** installs the new version itself after your
  system asks for your password, and starts it. It used to hand the file to the system's app
  store, which on Ubuntu opened and installed nothing.

## [2.0.0] - 2026-11-28

Dannex Download Manager 2.0.0: a free, open-source download manager for Windows (with Linux and an
experimental macOS version), laid out the way Internet Download Manager users expect, and faster.

### Downloads

- **Fast:** every download comes in several parts at once, each over its own connection, and is
  re-balanced until the last byte. A connection that goes quiet is replaced within seconds, small
  files start at full speed, and a server that turns DDM away is asked again as a browser would.
- **Resumes that hold:** after a crash, a restart, a sleeping computer or a dropped connection, a
  download goes on from where it stopped. When the network comes back, stopped downloads start
  again by themselves; a server that can no longer send parts of a file sends it again whole.
- **Download File Info, the way you know it:** it opens the moment a download is handed over, the
  file starts arriving while you choose where it goes, and it joins the list only once you click
  **Start Download** or **Download Later**; **Cancel** leaves nothing behind.
- **Every connection numbered** in the progress window, however many you allow.
- **Calm when many arrive at once:** a queue's downloads run without opening windows, and downloads
  that finish together share one **Download complete** window.
- **Queues and the Scheduler** start and stop on their own, can wake the computer and turn it off
  when done; **Speed Limiter**, categories with their own folders, and duplicate handling.
- **Metalink files:** open a `.metalink` or `.meta4` file (**Tasks → Open Metalink file...**) or
  address, and pick its files in **Add Metalink**. DDM uses the publisher's mirrors, checks every
  file and fetches again only the parts that fail the check.
- **Virus checking** after each download with the antivirus you choose, and a **temporary folder**
  of your choice. Unfinished pieces are always cleaned up; **Downloads → Clean Up Temporary Files**
  says how much it freed.
- **File missing:** a finished download whose file was moved or deleted says so and offers
  **Redownload** or **Locate...**.

### In your browser

- **The Dannex Download Manager extension**, from the Chrome Web Store for Chrome, Edge, Brave,
  Opera and Vivaldi, and for Firefox, hands your downloads to DDM with their sign-in. One click on
  its toolbar button turns it off or on.
- **The Download video button** over videos, in feeds too (YouTube, TikTok, X and many more): its
  qualities show at once, with the sizes DDM will download, and the sound alone under **Audio**.
  A quality opens **Download File Info** for that file straight away.
- Right-click **Download with DDM** for any link, magnet links included, and **Download all links**,
  images or videos and audio of a page.
- Music and video protected against copying, such as Spotify's, say so plainly instead of failing.

### Video, music, galleries, torrents and sites

- **Videos and music** from thousands of sites, YouTube Music, SoundCloud, Bandcamp and Boomplay
  among them, with playlists, subtitles and the sound alone; **image galleries** too.
- **Torrents and magnet links**, with sequential order, RSS feeds, sharing limits and web seeds.
  **Torrent → Play while downloading** plays a torrent's video or sound in your browser while it
  arrives, and skipping ahead fetches that part first.
- **Link Grabber:** a page's links with their names, sites, sizes and types, sorted by any column,
  filtered by kind with one button each, searched, and grouped so a file linked twice comes once.
- **Site Grabber:** a whole site or part of one, to read offline, in projects you can always come
  back to.

### The app

- **Themes:** **View → Theme → Choose a theme...** shows each theme as a live picture of the window,
  with its own accent colour, icons, text size and row height, and themes can be shared as a file.
- **The notification area** lists what is downloading and how far it has got; its menu pauses or
  resumes everything and sets the speed limit.
- **What's new** shows once after an update, with that version's notes.
- **File Properties** on double-click, **Show progress**, and **Columns...** to show, hide and order
  the list's columns. Minimize keeps DDM on the taskbar; closing it keeps it running in the
  notification area.
- Day and dark mode, 86 languages including right to left, keyboard and screen-reader access, a
  portable version, a phone remote and a server mode.
- **Help → Support DDM** opens Dannex's page on GitHub Sponsors.

### Setup

- The setup speaks your language and asks only what it needs. Everything for videos and music
  installs with it; without a connection, it finishes by itself the next time you're online. At
  the end, one tick adds Dannex Download Manager to your browser.
