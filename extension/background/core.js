// What every part uses: the browser's API, the host's name, the extension's copies, which browser
// this is, and its texts. A part of the background: background.js loads the parts in order, into
// one scope.

const ext = globalThis.browser ?? globalThis.chrome;
const HOST = "com.dannexdownloadmanager.host";
const VERSION = ext.runtime.getManifest().version;
const HELP = "https://dannex.kavangi.co.ke/help/browser-help.html";
const action = ext.action ?? ext.browserAction;
// The extension's own page that says how things stand: what the toolbar button opens when DDM
// can't be reached, and its right-click menu's Help.
const STATUS_PAGE = "install.html";
// The toolbar button's two looks: in colour while it takes downloads, grey with an X while it
// doesn't (off, DDM not reached, this copy standing aside). tools/icon/build_icon.py draws them.
const ICONS = {
  on: { 16: "icons/icon-16.png", 24: "icons/icon-24.png", 32: "icons/icon-32.png" },
  off: { 16: "icons/icon-off-16.png", 24: "icons/icon-off-24.png", 32: "icons/icon-off-32.png" },
};
// The copies of this extension a Chromium browser can have, the preferred first: the Chrome Web
// Store's (it updates itself) and the one loaded unpacked from DDM's folder
// (crates/ipc/src/extension.rs STORE_ID and UNPACKED_ID; a test keeps them the same). With both
// in one browser, only the first acts; the other stands aside and says to remove it (`copyState`).
const STORE_COPY = "plmckdlgphkcfkifahbkonbibfedckih";
const UNPACKED_COPY = "nphkobgebehllokfnmfdchmcbolllkif";
const COPIES = [STORE_COPY, UNPACKED_COPY];
// Safari has no downloads API at all (nor proxy): it can't take a download from the browser, so
// the links clicked in a page are handed over instead (`link`, below; page.js).
const SAFARI = !ext.downloads;
// Firefox has no pause before the file name (onDeterminingFilename): its own paths below.
const FIREFOX = !SAFARI && !ext.downloads.onDeterminingFilename;
// Brave and Opera say who they are only in the client hints, Firefox's forks (Zen, LibreWolf,
// Floorp, Waterfox) only in runtime.getBrowserInfo, which only Firefox's family has. Both go to
// the host as `brands` (Options lists them by name; crates/ipc capture::browser_named).
const BRANDS = (globalThis.navigator?.userAgentData?.brands || []).map((b) => b.brand);
const APP_NAME = ext.runtime.getBrowserInfo
  ? ext.runtime.getBrowserInfo().then((i) => (i && i.name) || null, () => null)
  : Promise.resolve(null);

const t = (key, ...subs) => ext.i18n.getMessage(key, subs) || key;
