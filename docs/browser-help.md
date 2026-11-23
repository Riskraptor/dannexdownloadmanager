# The browser and DDM aren't connected

[← Back to docs index](README.md)

When the browser can't hand a download to DDM, it keeps the download itself, so nothing is lost. The extension's toolbar button then turns grey with a red **!**. Click it: the extension's status page opens, says in a sentence or two what's wrong and the one thing to do, and turns into **Dannex Download Manager is installed** by itself once DDM answers (it asks again every two seconds while it is open). Its **Help** link brings you to the matching section below. You can open it any time: right-click the toolbar button → **Help**.

To check DDM's side, open **Options → General** in DDM and click **Test browser integration...**. It checks that browser integration is on, that this is the installed copy, that the browsers can find DDM, and which browsers' extension has connected (and which copy: the Chrome Web Store's, or the one loaded from DDM's folder); **How to fix it** opens the section for the first that fails.

## What the toolbar button shows {#toolbar}

A click on the toolbar button turns taking downloads from this browser off and on while DDM is reached; otherwise it opens the status page. Point at the button: its tooltip says the state and what a click does. Every state, what you see and what to do:

| State | The button | A click | The status page says | What to do |
|---|---|---|---|---|
| On | In colour, no badge; the tooltip: *Dannex Download Manager is on. Click to turn it off.* | Turns it off | **Dannex Download Manager is installed**, its switch on | Nothing |
| Off (you turned it off) | Grey with an **X**, **OFF** | Turns it on | Installed; *It's turned off, so downloads stay in this browser.* | Click it again, or the page's switch |
| Off in DDM's Options | Grey, **OFF** in amber | Opens the status page | **Turned off in Dannex Download Manager**, and which option | **Open Dannex Download Manager's Options**, see [below](#options) |
| DDM isn't installed, or browser integration isn't set up | Grey, red **!** | Opens the status page | **Dannex Download Manager isn't installed**, a **Download Dannex Download Manager** button for your system (Windows setup, macOS .dmg, Linux AppImage, .deb or .rpm) and three short steps | Install DDM; the page changes by itself. [More](#not-installed) |
| DDM didn't start in time | Grey, red **!** | Opens the status page | **Dannex Download Manager didn't start**: start it yourself, then **Try again** | [More](#not-running) |
| The helper program stopped | Grey, red **!** | Opens the status page | **Something is blocking Dannex Download Manager**: security software; or install it again, with the download | [More](#host-failed) |
| DDM doesn't accept this copy (DDM older than it) | Grey, red **!** | Opens the status page | **Update Dannex Download Manager**, with the download | [More](#forbidden) |
| A download's details were too large | Grey, red **!** | Opens the status page | **The last download couldn't go to Dannex Download Manager**, and **Try again** | [More](#too-large) |
| DDM older than the extension | In colour; the tooltip says to update DDM | Turns it off and on | Installed, and **Help → Check for updates...** in DDM | [More](#update-app) |
| DDM newer than the extension | In colour | Turns it off and on | Installed, and how to get the newer extension | [More](#extension-behind) |
| Two copies in one Chrome, Edge or Brave | The Chrome Web Store's: as above. The one from DDM's folder: grey, amber **!** | The other copy: opens the status page | **Remove this copy**, with a button that removes it | [More](#two-copies) |
| The Chrome Web Store lists the extension, and yours came from DDM's folder | As above | As above | **Get automatic updates**, once, quietly (also in the right-click menu) | [More](#two-copies) |
| The browser limits the extension's site access | As above | As above | **Allow on all sites** | [More](#site-access) |
| Not allowed in private windows | As above | As above | How to allow it | [More](#private) |
| Firefox's temporary add-on | As above | As above | Firefox removes it when it closes | [More](#firefox-temporary) |
| DDM's Options left the last download to the browser | As above | As above | *Your last download stayed in the browser*, its name, and which option | [More](#options) |
| Another DDM chosen (**Send downloads to**) | In colour; the tooltip names it | Turns it off and on | **Downloads go to another Dannex Download Manager**, and its address | Nothing |

Safari gives an extension's button no right-click menu: the click works the same, and **Safari → Settings → Extensions → DDM Integration Module → Settings** opens the options page, which says when DDM can't be reached and links to the status page.

### Older and newer {#versions}

The extension and DDM tell each other their versions at each connection, and every pair keeps working:

- **The extension newer than DDM** (the store updates it before DDM's next release): downloads go on. What only a newer DDM can do is asked under names an older DDM ignores, so it simply isn't offered. When the first two numbers of the versions differ, the tooltip and the status page say to update DDM.
- **DDM newer than the extension** (the store's review lagging, or the copy from DDM's folder not reloaded): downloads go on, since DDM answers every older extension as before, and the status page says how to get the newer extension.
- **DDM from before the Chrome Web Store's copy** (0.2.x) refuses that copy: [update DDM](#forbidden). The copy from DDM's folder keeps working meanwhile, and doesn't stand aside for a store copy DDM refuses.
- **An older DDM says nothing of the store's listing**: nothing is offered.

## DDM isn't installed, or browser integration isn't set up {#not-installed}

The browser can't find the small helper program it talks to. It is registered when DDM is installed.

1. Install DDM with the setup from [the Releases page](https://github.com/Riskraptor/dannexdownloadmanager/releases/latest), and keep **Set up browser integration** ticked.
2. Already installed? Open DDM, then **Options → General → Test browser integration**, and click **Set it up** if it offers to.
3. The portable copy can't be connected: it writes nothing outside its own folder. Use the installed copy for browser integration. See [Portable mode](portable.md).
4. On Linux and macOS, DDM sets browser integration up itself each time it starts: start DDM once. **Test browser integration** says what went wrong if it couldn't, such as **The helper program the browsers start is missing**: **Set it up** puts it back.
5. Restart the browser after setting it up.
6. **Zen, Helium, Brave Origin or another browser built on Firefox or Chromium, as your only browser on Linux or macOS?** Older versions of DDM set browser integration up only for the browsers they knew; the current one also knows Brave Origin and the Beta, Dev and Nightly versions of Brave, Chrome and Edge. Update DDM and start it once.
7. **The AppImage on Linux:** the AppImage's files are gone once it quits, so DDM keeps a copy of its helper program in its data folder (`~/.local/share/dannexdownloadmanager/browser-host`) and points the browsers there. Start DDM once after moving or renaming the AppImage, or after an AppImage manager (Gear Lever, AppImageLauncher and the like) moved it, so the helper knows where to start it from.
8. **Zen on Windows:** Zen 1.21.15b and later have a bug of their own that keeps add-ons from reaching their helper program ([zen-browser/desktop#15432](https://github.com/zen-browser/desktop/issues/15432)), with any add-on that uses one. Until Zen fixes it, Zen 1.21.14b works.

## DDM didn't start in time {#not-running}

When DDM isn't running, the browser starts it for each download and waits for it. It waited about 20 seconds this time and gave up.

- Start DDM yourself and try the download again.
- A first start, a slow disk, or an antivirus scanning DDM can take that long. Next time it is usually quick.
- With a VPN or proxy program, allow connections to `127.0.0.1` (your own computer): the browser's helper reaches DDM through one.
- If DDM never opens, see [The setup or DDM won't start](troubleshooting.md).

## Something is blocking DDM {#host-failed}

The browser started DDM's helper, but it stopped before answering. Security software is the usual cause.

- Allow `dannexdownloadmanager-nmhost.exe` (in DDM's folder) in your antivirus or anti-malware program.
- Reinstall DDM to restore its files.
- On Linux, with Firefox or Chromium installed as a Snap, see [Flatpak and Snap browsers](#linux-sandbox).
- Uninstalled DDM on Linux? The .deb and .rpm take their browser files with them; the AppImage's and the tar.gz's stay in your home folder: [remove them](getting-started.md#uninstall-linux), and the extension too.

## DDM doesn't accept this copy of the extension {#forbidden}

DDM's helper serves only its own extension. The browser says this copy isn't one it knows.

- **DDM 0.2.x with the extension from the Chrome Web Store?** The store's copy needs a newer DDM:
  0.2.x knows only the copy in its own folder. Update DDM with **Help → Check for updates...**.
  Until then, remove the store's copy
  and load the one in DDM's folder instead (`extension\chromium`, see
  [Browser integration](browser-integration.md#chrome-edge-brave-opera-and-vivaldi)).
- **Two copies installed?** Open the browser's extensions page (`chrome://extensions`, `edge://extensions`, `brave://extensions`) and keep only one **Dannex Download Manager**.
- **A copy from elsewhere?** Remove it and add the extension as DDM's **Add Dannex Download Manager to your browser** window shows (see [Browser integration](browser-integration.md)).
- **An older DDM?** Update DDM: a newer extension may have an ID an old helper doesn't know.

## The extension is newer than DDM {#update-app}

Downloads still work, but some of the extension's features need a newer DDM. Update it with **Help → Check for updates...** in DDM.

If DDM says it's up to date, there's nothing for you to do. The extension updates by itself through its store and can get ahead of the app for a while. Everything keeps working, and the message goes away with the next version of DDM.

## DDM is newer than the extension {#extension-behind}

Downloads still work. DDM came with a newer extension, and the status page says how to get it:

- **From the Chrome Web Store:** the browser updates it by itself within a few hours. **Check for updates** on the status page asks now.
- **Loaded from DDM's folder:** DDM's update replaced the files there, but the browser runs the copy it loaded until you reload it. **Reload the extension** on the status page does that (or the reload arrow on the browser's extensions page).
- **Firefox:** install the new `extension\ddm-firefox.xpi` from DDM's folder as you installed this one, until Firefox installs it from addons.mozilla.org and updates it itself.

## Two copies of the extension in one browser {#two-copies}

Chrome, Edge, Brave, Opera and Vivaldi can have the extension twice: the one from the Chrome Web Store and the one loaded from DDM's folder (**Load unpacked**). Both would take each download, so they ask each other: the Chrome Web Store's acts (it updates itself), and the other stands aside. It takes nothing, shows no right-click items and no **Download video** button, and its toolbar button is grey with an amber **!**: click it and **Remove this copy** removes it (the browser asks first). Or remove it on the browser's extensions page.

The copy from DDM's folder stands aside only while the store's copy reaches DDM. It stays the one that acts when the store's copy is turned off on the browser's extensions page, or can't reach DDM (when DDM is too old to accept the store's copy, [update DDM](#forbidden)). Removed or turned off there, the store's copy gives the work back at the next download, and turned on again it takes it back the same way: never both at once. Switched OFF with its toolbar button, the store's copy keeps the other copy off too: OFF means OFF in that browser.

Once the Chrome Web Store lists the extension, DDM tells the copy loaded from its folder, which never updates itself: its right-click menu gains **Get automatic updates from the Chrome Web Store...** and its status and options pages say so once, with no badge and nothing opening by itself. Add the extension from the store, and this copy stands aside and offers to be removed.

## Private windows {#private}

Chrome, Edge and Brave run an extension in private (incognito) windows only when you allow it, and Firefox asks too. Until then, downloads from private windows stay with the browser. To allow it: on the browser's extensions page, open **Details** and turn on **Allow in Incognito**; in Firefox, **Add-ons and themes** → the extension → **Allow** beside **Run in Private Windows**. Downloads from a private window use that window's cookies, and DDM doesn't keep them in its list once they're done.

## A download's details were too large {#too-large}

A download carries its site's cookies (and a form's data) to DDM. A few sites set so many that together they exceed what DDM accepts from the browser (1 MB). The browser kept this one. Signing out and back in to the site usually trims them.

## Downloads stay with the browser although DDM is installed {#options}

DDM is installed and reached, but its own settings leave this download to the browser. The extension's status page (right-click its toolbar button → **Help**) names the file and which setting it was. Check, in DDM:

1. **Options → General → Take downloads from web browsers** is on, and your browser is ticked under **Take downloads from these browsers**.
2. The file's type is listed in **Options → File types**.
3. The site isn't in **Leave downloads from these sites to the browser**, nor the address in **Leave these addresses to the browser**.
4. The file isn't smaller than **Don't take files smaller than** in **Options → File types**.
5. You didn't just choose **Download with the browser** for it in DDM: for 10 minutes after that, DDM leaves the browser's next download of that address to the browser, once ([A site that lets only browsers through](browser-integration.md#a-site-that-lets-only-browsers-through)).

Also check the extension's toolbar button: grey with **OFF** means it's off, and everything stays with the browser; click it to turn it on. When DDM's Options take nothing from this browser (**Take downloads from web browsers** off, or the browser unticked), the button is grey with **OFF** in amber, and its status page has a button that opens DDM's Options.

## The browser opens PDFs and music instead of sending them {#opened}

DDM also takes the file types the browser would show or play itself (a PDF, an MP3), when they're listed in **Options → File types** and **Also take files the browser would open** is ticked in **Options → General → Keys...**. This needs a Chromium browser (Chrome, Edge, Brave, Opera, Vivaldi) version 128 or later, or Firefox. Sites in DDM's excluded sites keep opening them in the browser. Hold **Alt** while clicking a link to leave it to the browser: Firefox then opens it, while Chromium browsers download it themselves (Alt+click is their own "download link").

## No Download video button on YouTube in Chrome, Edge or Brave {#store-youtube}

Your browser updates the copy of the extension from the Chrome Web Store by itself; to get an update sooner, open its extensions page and update them there.

You can always save a video without the button: in DDM, open **Tasks → Download video from a page** and paste the video's address.

## The extension can't read this site {#site-access}

In Chrome, Edge and Brave you can limit an extension to some sites (right-click its toolbar button → **This can read and change site data**), and in Firefox take its access away in **Add-ons and themes** → the extension → **Permissions**. On a site it may not read, the extension can't pass your sign-in on, so it leaves the download to the browser. Choose **On all sites** to let DDM take downloads everywhere, or click **Allow on all sites** on the extension's status page (right-click its toolbar button → **Help**), which asks the browser for it.

## Firefox forgets the extension after a restart {#firefox-temporary}

An add-on loaded with **Load Temporary Add-on** (from `about:debugging`) is removed when Firefox closes; its status page says so. Until Dannex's add-on is on addons.mozilla.org (on its way), Firefox Developer Edition, Nightly and ESR can keep it: set `xpinstall.signatures.required` to `false` in `about:config` and install `extension\ddm-firefox.xpi` with **Install Add-on From File…** in `about:addons` ([Browser integration](browser-integration.md#firefox)). Once it is listed there, install it from its page instead: Firefox keeps that one, and updates it.

## Linux: Flatpak and Snap browsers {#linux-sandbox}

Browsers installed as a Flatpak or a Snap run in a sandbox, which decides whether they can reach DDM.

- **Firefox as a Snap** (Ubuntu's own Firefox) reaches DDM through the desktop, which reads the browser files in your home folder and those the .deb or .rpm installs for everyone. The first time the extension calls DDM, the desktop asks **Allow Firefox to start WebExtension backend?** for Dannex Download Manager: choose **Allow**. After **Don't allow**, the extension says the connection failed and the desktop doesn't ask again; to be asked again, run this in a terminal, then use the extension again:

  `gdbus call --session --dest org.freedesktop.impl.portal.PermissionStore --object-path /org/freedesktop/impl/portal/PermissionStore --method org.freedesktop.impl.portal.PermissionStore.Delete webextensions com.dannexdownloadmanager.host`
- **Chromium as a Snap** (Ubuntu's Chromium) starts DDM's helper inside its sandbox, where it can't reach DDM, so DDM doesn't list it in **Add Dannex Download Manager to your browser**. Use Firefox, or Google Chrome from its own `.deb`.
- **Flatpak browsers** (such as Zen's `app.zen_browser.zen`) can't start DDM's helper, so DDM doesn't list them either. Install the browser from your distribution's packages (or the browser maker's own `.deb`, `.rpm`, tarball or AppImage) to use browser integration.

## Next steps

- [Browser integration →](browser-integration.md)
- [Troubleshooting →](troubleshooting.md)
- [Back to the docs index →](README.md)
