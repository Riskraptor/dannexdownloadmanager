# Publishing Dannex Download Manager

How a new version reaches people, what is automatic, and the few steps only the maintainer can
do (accounts, store forms, payments). Secrets are named here, never written here.

| What | State | Who |
|---|---|---|
| App releases (Windows, Linux, macOS) on GitHub | automatic from a tag | GitHub's Release workflow |
| Signed releases, DDM updating itself | on from 0.6.2 (the `MDM_RELEASE_KEY` secret) | automatic |
| Browser extension in each release's files | automatic | Release workflow |
| Extension in the Chrome Web Store | listed (submitted 2026-11-22) | done: `STORE_LISTED = true` |
| Extension in Edge Add-ons / Firefox add-ons | not listed yet | you: one form per store (Edge also installs from the Chrome Web Store) |
| Website (https://dannex.kavangi.co.ke) | online: GitHub Pages, DNS in Vercel | automatic |
| Windows code signing (no SmartScreen warning) | not set up | optional, a certificate |
| Private vulnerability reporting, Dependabot alerts | on (since 2026-11-22) | done |

## 1. Releasing a new version

1. Changes are on `main`, with CI green (GitHub → Actions → CI).
2. Set the version in `Cargo.toml` (`version = "X.Y.Z"`, numbers only) and give `CHANGELOG.md`'s
   `## [Unreleased]` notes their heading `## [X.Y.Z] - YYYY-MM-DD`. Commit.
3. Tag and push:
   ```
   git tag -a vX.Y.Z -m "Dannex Download Manager X.Y.Z"
   git push origin main vX.Y.Z
   ```
4. The **Release** workflow (about 45 minutes) builds Windows, Linux, macOS and the extensions,
   runs the tests, signs `SHA256SUMS.txt` with the release key and publishes the GitHub release
   with the changelog's section as its notes. A failed macOS or Linux build only leaves those
   files out; Windows and the extensions must succeed.
5. Every DDM from 0.6.2 on finds the new version (at start and once a day, or **Help → Check for
   updates**) and installs it after checking the signature and each file's checksum.

Check a release by hand: download `SHA256SUMS.txt` and `SHA256SUMS.txt.sig`, then
`gpg --import dannex-release-public.asc` and `gpg --verify SHA256SUMS.txt.sig SHA256SUMS.txt`
(must say *Good signature from "Dannex Download Manager Releases"*).

## 2. The release key

- Ed25519, fingerprint `72A4 3319 B2A7 06C5 BF9C  435A 37D0 745C 2ABF A010`, made 21 November 2026.
- The program carries its public half (`crates/tools/keys/mdm-release.asc`) and installs an update
  only from a release whose checksum list this key signed.
- The private key lives in the maintainer's offline folder (with its README, the public key and
  a revocation certificate) and in the repository secret `MDM_RELEASE_KEY`. Keep at least two
  copies off the computer (USB stick, password manager). If both are lost, no update that existing
  copies accept can ever be signed again.
- If the key leaks: publish the revocation certificate, make a new key, put its public half in
  the program and the secret, update the fingerprint in `.github/workflows/release.yml` and
  `crates/tools/src/openpgp.rs`'s test, release; copies with the old key update once by hand.
- 0.6.0 and 0.6.1 carry the original project's key: their users install 0.6.2 once from the
  Releases page.

## 3. The browser extension

The extension (named **Dannex Download Manager**, like the app) is in every release (`…-extension-chromium.zip`,
`…-extension-firefox.xpi`) and inside the app, which offers it at the
first start. Until it is in the stores:

- **Chrome, Edge, Brave, Opera, Vivaldi:** DDM's first-start window opens the browser's
  extensions page and the extension's folder; or by hand: `chrome://extensions` (or
  `brave://`, `edge://`…) → **Developer mode** on → **Load unpacked** → the folder
  `%LOCALAPPDATA%\Programs\Dannex Download Manager\extension\chromium`. It stays installed;
  the browser may remind you that developer-mode extensions are on.
- **Firefox:** an add-on must be signed by Mozilla to stay installed, so until it is on
  addons.mozilla.org it can only be loaded for one session (`about:debugging` → This Firefox →
  Load Temporary Add-on → the .xpi).

The listing texts, permission justifications and pictures for every store are in
[Store listing texts](#store-listing-texts), at the end of this file, and
[tools/extension/store/images/](tools/extension/store/images/). The privacy policy the stores
ask for: https://dannex.kavangi.co.ke/privacy.html

### 3a. Chrome Web Store (Chrome, Brave, Opera, Vivaldi, Edge too)

1. Go to https://chrome.google.com/webstore/devconsole, sign in with the Google account that will
   own the listing, accept the developer agreement and pay the one-time **US$5** registration.
   Verify the contact email it asks for.
2. **New item** → upload `DannexDownloadManager-<version>-chrome-web-store.zip`: the **Release**
   run's artifact `chrome-web-store` (GitHub → Actions → Release → the version's run →
   Artifacts; `gh run download <run> -n chrome-web-store`), or `tools/extension/pack.ps1`'s
   `target/extension/ddm-chrome-store.zip`. It is the chromium folder without the `key` line the
   store refuses.
3. **Store listing:** description, category, language, the icon `icon-128.png`, screenshots
   `screenshot-1..5.png`, small promo tile `promo-small.png` (and marquee `promo-marquee.png`),
   homepage and support links: all from [Store listing texts](#store-listing-texts).
4. **Privacy practices:** the single purpose, each permission's justification, *No* to remote
   code, the data usage ticks and the three certifications, the privacy policy URL: all in
   [Store listing texts](#store-listing-texts).
5. **Distribution:** Public, all regions. **Submit for review** (a few days for a new item; its
   broad host access means a closer look).
6. **Submitted on 2026-11-22**; the item's ID is `plmckdlgphkcfkifahbkonbibfedckih`. It is already
   `STORE_ID` in `crates/ipc/src/extension.rs` (so the native host serves the store's copy the
   moment it is published) and the repository variable `CWS_EXTENSION_ID`. **Listed:**
   `STORE_LISTED = true` in `crates/ipc/src/extension.rs`, so the website's Add to Chrome
   buttons and wording follow it (the Website workflow), DDM's window offers the store's copy,
   Chrome is offered it at its next start, and the user guide gives the store's steps first
   (Load unpacked only for a browser that can't use its store).
7. Automatic uploads with each release (optional): in Google Cloud, a project with the **Chrome
   Web Store API** enabled, an OAuth client of type *Desktop app*, and a refresh token for it (the
   `chrome-webstore-upload-keys` tool walks you through it); your **publisher ID** is on the
   developer console's account page. Then save them yourself as repository secrets (each command
   asks for the value; paste it there, never in a file or chat):
   ```
   gh secret set CWS_CLIENT_ID -R Riskraptor/dannexdownloadmanager
   gh secret set CWS_CLIENT_SECRET -R Riskraptor/dannexdownloadmanager
   gh secret set CWS_REFRESH_TOKEN -R Riskraptor/dannexdownloadmanager
   gh secret set CWS_PUBLISHER_ID -R Riskraptor/dannexdownloadmanager
   ```
   From then on every release uploads the new zip and sends it for review by itself. Without
   them, upload each release's zip by hand in the console (**Package → Upload new package**).

### 3b. Microsoft Edge Add-ons (optional: Edge also installs from the Chrome Web Store)

1. https://partner.microsoft.com/dashboard/microsoftedge → sign in with a Microsoft account and
   register (free) as an individual developer.
2. **Create new extension** → upload the same Chrome Web Store zip → the same listing texts and
   pictures (1280×800 screenshots, the 128 px icon) → availability: Public → submit (review up to
   7 business days).
3. Once published, its **ID** goes into `crates/ipc/src/extension.rs` `OTHER_STORE_IDS` (in place
   of `EDGE_ADDONS_ID_PENDING`), then release. New versions: upload each release's zip in the
   dashboard.

### 3c. Firefox add-ons (addons.mozilla.org)

1. https://addons.mozilla.org/developers/ → sign in (or create a free Mozilla account).
2. **Submit a New Add-on** → *On this site* → upload
   `DannexDownloadManager-<version>-extension-firefox.xpi`.
3. "Do you need to submit source code?" → **No** (the package is the source, not minified).
4. Listing: name and summary come from the package; description, categories (*Download
   Management*), license (*GPL-3.0-or-later*), support and homepage links, the privacy policy,
   screenshots, and the notes for the reviewer: all in [Store listing texts](#store-listing-texts).
   Submit.
5. Once approved, its page (`https://addons.mozilla.org/firefox/addon/<its-slug>/`) goes into
   `crates/ipc/src/extension.rs` as `FIREFOX_PAGE`, then release (DDM's Firefox setup then opens
   it; the website follows it by itself; the guide's Firefox steps, in the same files as for the
   Chrome Web Store and `docs/browser-help.md`, go back to **Add to Firefox**). The add-on's ID is already fixed in the package: `integration@dannexdownloadmanager.com`.
6. Automatic submissions with each release (optional): Developer Hub → **Tools → Manage API
   Keys** → generate; save them yourself:
   ```
   gh secret set AMO_JWT_ISSUER -R Riskraptor/dannexdownloadmanager
   gh secret set AMO_JWT_SECRET -R Riskraptor/dannexdownloadmanager
   ```

### 3d. Opera add-ons (optional)

Opera installs Chrome Web Store extensions. A listing of its own: https://addons.opera.com/developer/
with the Chrome Web Store zip; its ID goes into `OTHER_STORE_IDS` in place of
`OPERA_ADDONS_ID_PENDING`.

## 4. The website

The site is at **https://dannex.kavangi.co.ke**, a subdomain of the maintainer's own domain
kavangi.co.ke. The **Website** workflow builds it on every change to `website/`, `docs/` or the
changelog, and once a day, and GitHub Pages publishes it.

- **DNS:** kavangi.co.ke's DNS is in Vercel (the team "DANIEL MWENDWA's projects"). One record
  makes the subdomain: `dannex` CNAME `riskraptor.github.io`. It is more specific than the
  domain's wildcard, so it wins over it; the domain's CAA records already allow Let's Encrypt,
  which GitHub uses for the certificate.
- **GitHub Pages:** Settings → Pages: Source **GitHub Actions**, custom domain
  `dannex.kavangi.co.ke`, **Enforce HTTPS**. The site is made for that address (`SITE` in
  `tools/website/build.py` and `crates/app/src/help.rs`; the `CNAME` file `build.py` writes).
- **Google Search Console:** kavangi.co.ke is a verified Domain property of the Google account
  that owns the Chrome Web Store listing (a `google-site-verification` TXT record in Vercel: keep
  it), with the site's `sitemap.xml` submitted. That verification is what the Web Store's
  **Official URL** list and its verified-publisher badge need: in the developer dashboard,
  **Account** → verified publisher → kavangi.co.ke, and the item's **Store listing** → Official
  URL.
- **The app and the extension** link to it (`SITE_LIVE` in `crates/app/src/help.rs`, the
  extension's `HELP` in `extension/background/core.js` and `popup.js`, `homepage_url` in its
  manifests).
- **Visit counts** (optional): off. With a Statcounter project of your own, set the repository
  variables `STATCOUNTER_PROJECT` and `STATCOUNTER_SECURITY`, and give
  `website/content/privacy.md`'s "This website" section its paragraph about it again.
- **dannexdownloadmanager.com**, the address the original plan named, is not registered: DDM
  0.6.0 and 0.6.1 still open it from their Help menu (0.6.2 and later open GitHub, then this
  site). Registering it some day and forwarding it to the site would catch those clicks.

New or changed English on the site is translated with `python tools/website/translate_site.py
--all` (`--status` shows what each language lacks); until then those sentences show in English.
Without an OpenRouter key: `--all --export DIR` writes what each language lacks, translators write
`DIR/out/<code>.out.json`, and `--all --import DIR` checks and keeps their answers, as the app's
`tools/i18n/translate.py` does.

## 4b. Security reports

`SECURITY.md` asks people to report vulnerabilities privately through GitHub: **Private
vulnerability reporting** is on (GitHub → the repository → **Settings → Code security**), so its
"Report a vulnerability" link opens the form. **Dependabot alerts** are on too: a known flaw in a
crate DDM uses shows in the repository's Security tab (its automatic fix pull requests are off).
Secret scanning and push protection were on already.

## 4c. Donations (optional)

Dannex has no donation page yet, so the site shows no Sponsor button and the repository none.
With one of your own (GitHub Sponsors, for example): set the repository variable
`SPONSORS_URL` to its address (the site's header, footer and home page then show it), and add
`.github/FUNDING.yml` (`github: <your login>`) for the repository's Sponsor button.

## 5. Windows code signing (optional)

Unsigned installers make Windows SmartScreen warn ("Windows protected your PC") until enough
people have run them. A code-signing certificate removes that:
- free for open-source projects through the SignPath Foundation (apply with the repository);
- Microsoft's Azure Trusted Signing (a monthly fee);
- a certificate from a commercial authority.
With a .pfx certificate, save it yourself as secrets `CODESIGN_PFX_BASE64` (the .pfx file in
base64) and `CODESIGN_PASSWORD`; the Release workflow then signs every program and the installer.

## 6. Checklist for each release

- [ ] CI green on `main`
- [ ] `Cargo.toml` version and `CHANGELOG.md` heading
- [ ] new texts translated (`python tools/i18n/translate.py --all`, or `--export`/`--import`)
- [ ] tag pushed; Release workflow green; the release has `SHA256SUMS.txt.sig`
- [ ] Chrome Web Store / Edge / Opera: new zip uploaded (automatic for Chrome with its secrets)
- [ ] Firefox add-ons: new version (automatic with its secrets)

## Store listing texts

What to type into each store's form for the **Dannex Download Manager** extension, the same
everywhere so the listings match. The pictures are in
[tools/extension/store/images/](tools/extension/store/images/)
(`tools/extension/store/make_images.py` makes them again from DDM's own windows). The
step-by-step of each store is in [3. The browser extension](#3-the-browser-extension) above.

### Files to upload

| Store | File (from each release, or `tools/extension/pack.ps1` → `target/extension/`) |
|---|---|
| Chrome Web Store | `DannexDownloadManager-<version>-chrome-web-store.zip` (`ddm-chrome-store.zip`: no `key` in its manifest, the store signs it) |
| Microsoft Edge Add-ons | the same Chrome Web Store zip |
| Firefox add-ons (AMO) | `DannexDownloadManager-<version>-extension-firefox.xpi` (`ddm-firefox.xpi`) |

### Name and short description

- **Name:** Dannex Download Manager (it comes from the extension's own `_locales`, in 51 languages;
  the extension has the app's name, it is the app's part in the browser)
- **Short description / summary** (from the manifest, also translated):
  Sends the files, links and videos you download in your browser to the Dannex Download Manager
  app on your computer.

### Detailed description

```
This is the browser part of Dannex Download Manager (DDM), a free, open-source download
manager for Windows, Linux and macOS made by Riskraptor. The extension does nothing alone: it
works with the DDM app installed on the same computer, which you get from
https://dannex.kavangi.co.ke/download.html

What it does
• Downloads you start in the browser go to DDM, which downloads each file in several parts at
  once, pauses and resumes them (also after a restart or a lost connection), and sorts them
  into categories.
• A Download video button over videos and sound players, in feeds too: pick a quality, with
  its size, or the sound alone.
• Right-click a link: Download with DDM. Or Download selected links, Download all links,
  Download all images, Download all videos and audio with DDM.
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

### Category, language, links

- **Chrome Web Store category:** Workflow & Planning → *Tools* (or *Productivity*)
- **Edge category:** Productivity
- **Firefox categories:** Download Management (main), Privacy & Security is **not** right for it
- **Language:** English (the extension carries 51 translations itself)
- **Homepage:** https://dannex.kavangi.co.ke/
- **Help:** https://dannex.kavangi.co.ke/help/browser-help.html
- **Support (issues):** https://github.com/Riskraptor/dannexdownloadmanager/issues
- **Privacy policy URL:** https://dannex.kavangi.co.ke/privacy.html
- **License (Firefox asks):** GNU General Public License v3.0 or later

### Chrome Web Store: Privacy practices tab

**Single purpose:**
```
Sends the downloads you start in the browser (and the links and videos you choose from its
right-click menu or its button over videos) to the Dannex Download Manager app installed on the
same computer, which downloads and saves them.
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

### Firefox add-ons: notes for the reviewer

```
The extension needs Dannex Download Manager installed (its native messaging host,
com.dannexdownloadmanager.host). Without it, the toolbar button opens a page that shows how to install it; the
download hand-off can't be tried. The code is not minified or bundled: the package is the source
(https://github.com/Riskraptor/dannexdownloadmanager/tree/main/extension).
data_collection_permissions: none.
```

### Chrome Web Store: appeal text (the 2026-11 "Yellow Nickel" rejection)

Version 1.1.1 was refused as a duplicate (Spam and placement in the Store). From 1.1.2 the
extension has its own name and description (above). In the dashboard: upload the new zip, replace
the listing's text and pictures with these, then **Build > Status > Appeal** with:

```
This is the only extension of this developer account. It is the browser part of Dannex Download
Manager, a desktop download manager published by Riskraptor
(https://dannex.kavangi.co.ke/, source at https://github.com/Riskraptor/dannexdownloadmanager).
It talks only to that app, through its own native messaging host com.dannexdownloadmanager.host,
installed with the app; it shows no ads and no notifications, and sends nothing on the user's behalf. Version 1.1.2 renames it from "DDM Integration Module" to "Dannex Download Manager", and
its description and pictures were rewritten so that it can't be mistaken for another product.
The app's code started from an open-source project (GPL-3.0), but Dannex is a separate product
with its own app, name, website and native host; it shares no listing, account or publisher with
any other store item.
```
