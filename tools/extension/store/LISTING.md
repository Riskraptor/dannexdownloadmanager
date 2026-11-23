# The extension's store listings

What to type into each store's form for the **DDM Integration Module**, the same everywhere so
the listings match. The pictures are in [images/](images/) (`make_images.py` makes them again
from DDM's own windows). The step-by-step of each store is in
[PUBLISHING.md](../../../PUBLISHING.md).

## Files to upload

| Store | File (from each release, or `tools/extension/pack.ps1` → `target/extension/`) |
|---|---|
| Chrome Web Store | `DannexDownloadManager-<version>-chrome-web-store.zip` (`ddm-chrome-store.zip`: no `key` in its manifest, the store signs it) |
| Microsoft Edge Add-ons | the same Chrome Web Store zip |
| Firefox add-ons (AMO) | `DannexDownloadManager-<version>-extension-firefox.xpi` (`ddm-firefox.xpi`) |

## Name and short description

- **Name:** DDM Integration Module (it comes from the extension's own `_locales`, in 51 languages)
- **Short description / summary** (from the manifest, also translated):
  Hands downloads from the browser to Dannex Download Manager.

## Detailed description

```
The DDM Integration Module connects your browser to Dannex Download Manager (DDM), a free,
open-source download manager for Windows, Linux and macOS. It needs DDM installed on your
computer: get it from https://dannex.kavangi.co.ke/download.html

What it does
• Downloads you start in the browser go to DDM, which downloads each file over up to 8
  connections, pauses and resumes them (also after a restart), and sorts them into categories.
• Right-click a link: Download with DDM. Or Download selected links, Download all links,
  Download all images, Download all videos and audio with DDM.
• A Download button over videos and sound players, with the qualities DDM finds.
• Hold Alt while clicking to leave a download to the browser, or Ctrl+Alt to send any link to
  DDM (both keys can be changed in DDM's Options).
• One click on its toolbar button turns it off or on (the button grey with an X and OFF while
  off). Everything else is set in DDM itself.
• If DDM isn't installed or can't be reached, the button says so, and a click shows what to do.
• Signed-in downloads keep working: the site's cookies and the page go with the download to
  DDM on your computer.
• Optional: download through the browser's VPN or proxy extension, or send downloads to another
  DDM of yours (a home server, a NAS) through its remote control.

Privacy
The extension talks only to DDM on your own computer (or to another DDM you paired yourself).
No accounts, no analytics, no ads, no remote code. Free and open source (GPL-3.0-or-later):
every line can be read at https://github.com/Riskraptor/dannexdownloadmanager
```

## Category, language, links

- **Chrome Web Store category:** Workflow & Planning → *Tools* (or *Productivity*)
- **Edge category:** Productivity
- **Firefox categories:** Download Management (main), Privacy & Security is **not** right for it
- **Language:** English (the extension carries 51 translations itself)
- **Homepage:** https://dannex.kavangi.co.ke/
- **Help:** https://dannex.kavangi.co.ke/help/browser-help.html
- **Support (issues):** https://github.com/Riskraptor/dannexdownloadmanager/issues
- **Privacy policy URL:** https://dannex.kavangi.co.ke/privacy.html
- **License (Firefox asks):** GNU General Public License v3.0 or later

## Chrome Web Store: Privacy practices tab

**Single purpose:**
```
Hands the downloads you start in the browser (and links you choose from its right-click menu)
to Dannex Download Manager, the download manager installed on the same computer.
```

**Permission justifications** (the *alarms* permission is gone since the toolbar button stopped
counting DDM's downloads: nothing to justify for it any more):

| Permission | Why |
|---|---|
| downloads | To see a download as the browser starts it, hand it to DDM, and cancel the browser's copy once DDM took it. |
| nativeMessaging | To talk to Dannex Download Manager on the same computer (its native messaging host, installed with it). |
| cookies | So downloads that need you to be signed in keep working in DDM: the site's cookies for that address go with it to DDM on your computer. |
| contextMenus | For the right-click items: Download with DDM, selected links, all links, all images, all videos and audio. |
| storage | For the extension's own settings (paused or not, DDM's file types and excluded sites), on your computer. |
| webRequest | To see the forms submitted and the video streams and files a page's players load, so DDM gets the same request; it changes nothing. |
| scripting | To read a page's links, pictures or videos when you choose Download all links / selected links / all images / all videos and audio, and to add the Download button over videos on pages already open when the extension is installed. |
| declarativeNetRequestWithHostAccess | So the file types you told DDM to take (a PDF, an MP3) download instead of opening in the browser; for those types only, never for sites you excluded. |
| proxy | Only while you switch on "Download through this browser's VPN or proxy" in its options page: to read which proxy the browser uses, so DDM downloads through it too. It never changes the browser's proxy. |
| Host permission (all sites) | Downloads and videos can come from any site. |
| Remote code | No: all its code is in the package. |

**Data usage** (tick these, then certify): it handles *Website content* and *Authentication
information* (the cookies of a download's site) and *Web history* (the address of a download and
the page it came from), only to perform the download in DDM on the user's own computer.
Certify: not sold to third parties; not used or transferred for purposes unrelated to the single
purpose; not used to determine creditworthiness or for lending.

## Firefox add-ons: notes for the reviewer

```
The extension needs Dannex Download Manager installed (its native messaging host,
com.dannexdownloadmanager.host). Without it, the toolbar button opens a page that shows how to install it; the
download hand-off can't be tried. The code is not minified or bundled: the package is the source
(https://github.com/Riskraptor/dannexdownloadmanager/tree/main/extension).
data_collection_permissions: none.
```
