---
name: Browser extension
title: Browser extension that sends downloads to a download manager
headline: Click a download. DDM takes it from there.
line: Downloads you click in Chrome, Edge, Brave, Opera, Vivaldi or Firefox go straight to DDM, sign-in included.
lede: The free DDM extension hands each download you click over to DDM, with the page it came from and your sign-in for the site. A button over videos sends them to DDM too.
description: The Dannex Download Manager extension for Chrome, Edge, Brave, Opera, Vivaldi and Firefox: downloads you click go to DDM with their cookies.
group: downloading
order: 20
since: 0.1.0
shot: images/help/catch-file-info.webp
shot_alt: Download File Info in DDM for a file clicked in the browser
where: Options → General → Take downloads from web browsers
help: browser-integration.md
---

## The browser's downloads, in DDM

Click a download link and **Download File Info** opens in DDM instead of the browser's own
download bar. DDM gets the address together with the page it came from, your cookies for the
site and the browser's User-Agent, so a download that needs you to be signed in still works.
The browser's copy is cancelled and taken off its download list.

You decide what DDM takes. **Options → File types** lists the file types it catches (ZIP, RAR,
EXE, ISO, MP4, PDF and many more) and the sites it leaves alone, and **Options → General** has
the browsers it takes downloads from. Everything else stays with the browser.

DDM doesn't need to be running first: the browser starts it. If DDM can't be reached at all, the
download stays with the browser, so nothing is lost.

## Right-click, or the button over a video

Right-click a link, a picture, a video or a sound and choose **Download with Dannex Download
Manager**: DDM downloads it whatever its type.

![The Download video button over a YouTube video in Chrome](images/help/video-button.webp)
*The button over a video. Drag it along the edge; it stays where you put it.*

Point at a video and a small **Download video** button appears at its edge. It opens DDM's
**Download video from a page** window with the qualities on offer (see
[video downloads](/features/video.html)). You can hide it on one video or one site, or turn it
off in **Options**.

## Comes with the app

Install DDM with its setup and keep **Set up browser integration** ticked. Then add the
extension: it comes with DDM, in its `extension` folder, and DDM's **Add Dannex Download
Manager to your browser** window shows how to add it in each browser (Firefox 140 or later).
Its listings in the Chrome Web Store and on addons.mozilla.org are on their way: from there,
the browser keeps it updated.

Some limits, plainly: the portable copy can't connect to a browser, since it writes nothing
outside its folder.

## More of the browser

<!--@coming 0.5-->

- **One click turns it off or on.** While it takes downloads, its toolbar button shows DDM's
  icon in colour; turned off, it's grey with an X, and every download stays with the browser
  until you click it again. If DDM isn't installed or can't be reached, the click opens a page
  that says what to do, and that page turns green by itself once DDM is ready.
- **Right-click a page** for **Download all links**, **Download selected links**, **Download all
  images** and **Download all videos and audio**: what the page holds goes to
  [Link grabber](/features/link-grabber.html), the files its players played included.
- PDFs, music and videos the browser would open itself, and downloads from forms, go to DDM
  too. Hold **Alt** while clicking to let the browser have one.
- Browsers built on Firefox or Chromium, such as Zen, LibreWolf, Floorp, Waterfox and Helium,
  reach DDM. Flatpak browsers on Linux can't start DDM's helper.
- **Test browser integration...** in **Options → General** checks DDM's side.
