// Install and start: the content scripts put into open tabs, the rules asked when the browser's
// window comes back. A part of the background: background.js loads the parts in order, into one
// scope.

// ---- install, start --------------------------------------------------------------------------

ext.runtime.onInstalled.addListener(async (details) => {
  // Firefox says when it was loaded as a temporary add-on (gone when Firefox closes): the status
  // page says so.
  if (details.temporary !== undefined) await ext.storage.local.set({ temporary: Boolean(details.temporary) }).catch(() => {});
  // A copy after this one in the browser asks again whether to stand aside: told first, as DDM
  // may take a while to answer below.
  tellOtherCopies();
  // Whether the Chrome Web Store's copy is here too, then the items Options showed when last
  // asked (all of them at a first install), in this version's texts; the rules asked next make
  // them again if Options changed them meanwhile.
  const state = await copyState(0);
  await createMenus();
  // Hello: the app learns the extension is there (its offer window says Connected).
  const r = await refreshRules();
  // Installed where DDM isn't (or can't be reached): the status page, once, says what to do and
  // turns green by itself once DDM answers.
  if (details.reason === "install" && !r && !state.dormant) await openStatusPage();
  injectOpenTabs();
});

ext.runtime.onStartup.addListener(() => {
  // The copies after this one ask again whether to stand aside.
  tellOtherCopies();
  refreshRules();
  showStatus();
});

// Back in the browser (from DDM's Options, where its keys or right-click items may have changed):
// the rules asked again when a few seconds old, so the next click and the next right-click go by
// them.
if (ext.windows && ext.windows.onFocusChanged) {
  ext.windows.onFocusChanged.addListener((id) => {
    if (id !== ext.windows.WINDOW_ID_NONE) freshRules(FOCUS_RULES_MS).catch(() => {});
  });
}

// A match pattern's host and path as a test (`*://*.youtube.com/*`).
function matcher(pattern) {
  const [, host = "", path = "/*"] = /^[^:]+:\/\/([^/]*)(\/.*)?$/.exec(pattern) || [];
  const glob = (s) => s.split("*").map((x) => x.replace(/[.+?^${}()|[\]\\]/g, "\\$&")).join(".*");
  const h = host.startsWith("*.") ? new RegExp(`^(.*\\.)?${glob(host.slice(2))}$`, "i") : new RegExp(`^${glob(host)}$`, "i");
  const pth = new RegExp(`^${glob(path)}$`);
  return (u) => h.test(u.hostname) && pth.test(u.pathname + u.search);
}

// Pages open before the extension was installed or updated get its scripts now (else the
// button waits for a reload).
async function injectOpenTabs() {
  if (!ext.scripting) return;
  const tabs = await ext.tabs.query({ url: ["http://*/*", "https://*/*"] }).catch(() => []);
  for (const entry of ext.runtime.getManifest().content_scripts || []) {
    const excludes = (entry.exclude_matches || []).map(matcher);
    for (const tab of tabs) {
      let u;
      try {
        u = new URL(tab.url);
      } catch {
        continue;
      }
      if (excludes.some((m) => m(u))) continue;
      // In the world the manifest gives it (media.js: the page's own).
      ext.scripting
        .executeScript({ target: { tabId: tab.id, allFrames: Boolean(entry.all_frames) }, files: entry.js, ...(entry.world ? { world: entry.world } : {}) })
        .catch(() => {});
    }
  }
}
