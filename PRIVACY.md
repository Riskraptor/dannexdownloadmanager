# Privacy policy

Dannex Download Manager and its browser extension, the DDM Integration Module, don't collect anything about you: no accounts, no analytics, no telemetry, no ads. This page says exactly what each of them sends, and to whom.

_Applies to the app and the extension (Chrome Web Store, Microsoft Edge Add-ons, Firefox add-ons, and the copies that come with the app). Last updated 21 November 2026._


## In short

- The app and the extension **send nothing about you to us**. We don't run servers that could receive it.
- The app talks to **the servers you download from**, to **GitHub** to check for updates when it starts (you can turn that off), and to GitHub again for **yt-dlp and ffmpeg** if you download videos.
- **Torrents** are peer to peer: other people in a torrent's swarm see your IP address, as with every torrent client.
- The **browser extension** talks only to Dannex Download Manager on your own computer.
- Your download list and settings stay **on your computer**.


Dannex Download Manager ("DDM") is free, open-source software (GPL-3.0-or-later) made by its contributors. Because its whole source code is public at [github.com/Riskraptor/dannexdownloadmanager](https://github.com/Riskraptor/dannexdownloadmanager), you don't have to take this page's word for it: every network connection described here can be checked in the code.

## The app

### What stays on your computer

Your download list, settings, logins you save in **Sites Logins**, the history of each download and the files you download are stored only on your computer: in `%APPDATA%\dannexdownloadmanager` on Windows (or the `data` folder of the portable copy), and in the usual application-data folder on macOS and Linux. Nothing is uploaded anywhere. Uninstalling offers to delete them. The passwords of Sites Logins, the proxy and debrid accounts go into the system's keychain instead (Windows Credential Manager, the keyring on Linux, the macOS keychain), still on your computer, when one is there and unlocked.

### Who the app talks to, and why

- **The servers you download from.** To download a file, DDM connects to the server that holds it and sends what any browser would: the request, and the cookies, referrer and browser identification the extension passed along, so downloads that need you to be signed in keep working. Those servers see your IP address, as they would if your browser downloaded the file.
- **Your proxy**, if you set one in **Options → Proxy / Socks**, and your network's proxy auto-config script, if you point DDM at one.
- **GitHub, to check for updates.** Each time it starts, and once a day while it stays open, DDM asks GitHub's public API for the latest release of DDM (`api.github.com/repos/Riskraptor/dannexdownloadmanager/releases/latest`). The request carries DDM's version in its User-Agent and nothing else. Turn the check off in **Help → Check for updates** (**Check for updates at every start**). If you choose **Update now**, DDM downloads the new version's file and its signed checksum list from the same GitHub release. GitHub's own privacy statement applies to these requests.
- **GitHub, for video helpers.** If you use **Download video from a page**, DDM downloads yt-dlp (and ffmpeg, if you want merged qualities) from their official GitHub releases, checks them, and once per session asks GitHub whether yt-dlp has a newer release. yt-dlp then talks to the video site you asked for, as your browser would.
- **Torrent peers and trackers.** When you download a torrent, DDM connects to the trackers listed in the torrent, to the BitTorrent DHT network and to other peers, who see your IP address and which pieces you have. To accept connections it may ask your router to open a port (UPnP). This is how BitTorrent works; it ends when the torrent stops.
- **Your AI assistant, only if you connect one.** DDM's MCP server (`dannexdownloadmanager-mcp`) talks only to the DDM on your computer. But if you add it to an AI assistant (Claude, Cursor, VS Code…) and turn on the automation API, the assistant can read your download list, and what it reads (file names, addresses, folders) goes to that assistant's AI service, under its privacy terms. Nothing of this happens unless you set it up; see [AI assistants](https://github.com/Riskraptor/dannexdownloadmanager/blob/main/docs/ai-assistants.md).

Only if you choose them:

- **The DNS you choose.** With a DNS other than the system's in **Options → Connection → DNS** (Cloudflare, Google, Quad9, AdGuard, NextDNS or your own), DDM asks it, over HTTPS, TLS or TCP, for the addresses of the servers it downloads from. That service sees those names and your IP address, under its own privacy terms.
- **Your debrid accounts.** A link or torrent you send through a Real-Debrid, AllDebrid or Premiumize account in **Options → Accounts** goes to that service, which then serves the file. Its sign-in stays on your computer, with the passwords above.
- **Codeberg, for gallery-dl.** If you download an image gallery, DDM installs gallery-dl from its official releases on Codeberg and checks its author's signature; gallery-dl then talks to the site you asked for.
- **A torrent IP blocklist**, if you give one: DDM fetches it from the address you entered, again every 7 days.
- **Your own devices, for the remote control.** Off until you turn it on in **Options → Remote**: then DDM answers, over HTTPS on your local network, only the phones and computers you paired with its code.

The server's country that **Download File Info** and the progress window show comes from a table inside DDM: no address is sent anywhere to look it up.

DDM has **no analytics, no automatic crash reporting, no telemetry and no advertising** (a crash report goes only where you send it yourself). It never sends us your download list, the addresses you download, or anything else.

### Reporting a problem

**Help → Report a problem** opens GitHub's bug report form in your browser with DDM's version, your system and its version, how DDM was installed and your language filled in. Nothing is sent until you choose to submit it on GitHub, where it becomes public; read it before you post.

### The automation API

The automation API is off by default. When you turn it on, it listens only on `127.0.0.1` (your own computer) and answers only programs that present the secret token from DDM's data folder.

## The browser extension

The **Dannex Download Manager** extension for Chrome, Edge, Brave, Opera, Vivaldi and Firefox:

- When you start a download that DDM should take, it passes **the download's address (and where its redirects led), the page it came from, the site's cookies for that address, your browser's User-Agent, and the file's name, size and type** to DDM on your computer, through the browser's native-messaging channel. Nothing goes over the internet.
- Once you switch on **Download through this browser's VPN or proxy** in its options page (under **More**; Firefox asks you first), when the browser uses a proxy for a download (set by a VPN or proxy extension, or in its settings), the extension passes **the proxy's address**, or its **proxy auto-config script**, so DDM downloads through the same proxy. It never reads or passes the proxy's login. It notes only that a proxy asked for one, so DDM can tell you it can't use that proxy. Switched off (as installed), it reads nothing of the browser's proxy.
- When a download comes from a form you submitted, it passes **the form's data** too, so DDM can submit it again. It notes the data of forms you submit (in memory, used for two minutes at most, never stored) only for that.
- It notes the addresses of video streams (HLS and DASH playlists) and of video and sound files a page's players load, in memory for that tab, so the **Download video** button and the right-click menu can pass them on. Its button reads the page only to find video and sound players and place itself, and, for its menu, the qualities the player or its stream lists (on YouTube, what the player says it plays and its formats). Clicking it passes the page's address, the video's address, the page's title, the site's cookies, the page that embeds the player and your browser's User-Agent to DDM. So that the qualities are ready when you click, it passes the same when the pointer reaches the button, or rests over the video on that video's own page; never from a private window.
- **Download selected links**, **Download all links**, **Download all images** and **Download all videos and audio** read the page (its links, pictures or players, or your selection) when you choose them, and pass the addresses to DDM with the page's address and its site's cookies; the last one adds the video and sound files the tab's players played.
- It notes where page loads' redirects started and ended (in memory, for two minutes), so DDM gets the address you clicked, not only where it led.
- A small script in every page notes only the links you click while holding **Alt**, so those downloads stay with the browser; the addresses are kept in memory for a few seconds.
- Every message to DDM also says which browser it comes from (its name, as the browser tells it) and the extension's version.
- With **Send downloads to → Dannex Download Manager on another computer** in its options page (under **More**; off until you pair one with the code that DDM shows), the extension sends the address of each download it takes to that DDM of yours, over HTTPS, with the pairing's token; nothing else, and never to anyone else.
- It stores its own settings (paused or not, DDM's file types and excluded sites, its connection status, the name and site of the last download it left to the browser so its status page can say why, where you dragged the video button on each of the last 200 sites; nothing from private windows) in the browser's extension storage, on your computer.
- When the Chrome Web Store's copy and the copy from DDM's folder are both in one Chromium browser, they ask each other (by their IDs, inside the browser) whether the other is there, so only one takes downloads. Nothing leaves the browser for that.
- It has **no remote code, no analytics and no ads**, and sends nothing to us or anyone else. Its status page links to DDM's download page, and its pages' **Support DDM** to the maintainer's GitHub Sponsors page; the browser opens them only when you click.

Its permissions, and why: *downloads* to see and hand over downloads; *cookies* so signed-in downloads work in DDM; *nativeMessaging* to talk to DDM; *contextMenus* for the right-click items; *storage* for its settings; *webRequest* to see forms submitted and video streams loaded (it changes nothing); *declarativeNetRequestWithHostAccess* (Chrome, Edge and other Chromium browsers) to have the files the browser would open (a PDF) downloaded instead, for the types DDM takes; in Firefox, *webRequestBlocking* does that; *scripting* to read a page's links when you ask, and to add the video button to pages already open when the extension is installed; *proxy* to read which proxy the browser uses, so downloads taken from a browser behind a VPN or proxy extension go through it too, only while you have that switched on in its options page (it never changes the browser's proxy; in Firefox it is optional, asked for when you switch it on; Chrome and the other Chromium browsers can't make it optional, and count it under the access to all sites); access to all sites because downloads and videos can come from any site.
