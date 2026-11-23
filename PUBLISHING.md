# Publishing Dannex Download Manager

How a new version reaches people, what is automatic, and the few steps only the maintainer can
do (accounts, store forms, payments). Secrets are named here, never written here.

| What | State | Who |
|---|---|---|
| App releases (Windows, Linux, macOS) on GitHub | automatic from a tag | GitHub's Release workflow |
| Signed releases, DDM updating itself | on from 0.6.2 (the `MDM_RELEASE_KEY` secret) | automatic |
| Browser extension in each release's files | automatic | Release workflow |
| Extension in the Chrome Web Store | submitted 2026-11-22, in review | then `STORE_LISTED = true` and a release |
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

The extension (the **DDM Integration Module**) is in every release (`…-extension-chromium.zip`,
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
[tools/extension/store/LISTING.md](tools/extension/store/LISTING.md) and
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
   homepage and support links: all from LISTING.md.
4. **Privacy practices:** the single purpose, each permission's justification, *No* to remote
   code, the data usage ticks and the three certifications, the privacy policy URL: all in
   LISTING.md.
5. **Distribution:** Public, all regions. **Submit for review** (a few days for a new item; its
   broad host access means a closer look).
6. **Submitted on 2026-11-22**; the item's ID is `plmckdlgphkcfkifahbkonbibfedckih`. It is already
   `STORE_ID` in `crates/ipc/src/extension.rs` (so the native host serves the store's copy the
   moment it is published) and the repository variable `CWS_EXTENSION_ID`. **The day the review
   passes:**
   - set `STORE_LISTED = true` in `crates/ipc/src/extension.rs`: the website's Add to Chrome
     buttons and wording follow it at once (the Website workflow), DDM's window offers the
     store's copy, and Chrome is offered it at its next start;
   - the user guide says the listing is "on its way": `docs/browser-integration.md`,
     `docs/getting-started.md`, `docs/guide-catch-browser-downloads.md`,
     `website/content/faq.md`; change those sentences to the store's steps;
   - release a new version.
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
   screenshots, and the notes for the reviewer: all in LISTING.md. Submit.
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
  extension's `HELP` in `extension/background.js` and `popup.js`, `homepage_url` in its
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
