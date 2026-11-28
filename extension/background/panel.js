// The toolbar button's panel (popup.html). A part of the background: background.js loads the parts
// in order, into one scope.

// ---- the toolbar button's panel (popup.html) -------------------------------------------------

// The tab the panel was opened over (it names it: a panel has no tab of its own), a web page's.
async function popupTab(id) {
  if (!Number.isInteger(id) || id < 0) return null;
  const tab = await Promise.resolve(ext.tabs.get(id)).catch(() => null);
  return tab && /^https?:/i.test(String(tab.url || "")) ? tab : null;
}

// The panel: how things stand (as the button's tooltip says it), the switch, and what can be
// taken from the tab under it (a video it plays, its links). Never reached yet: asked first.
async function popupMessage(m) {
  if (m.type === "popup") {
    let look = await toolbarLook();
    if (look.click === "check") {
      await refreshRules();
      look = await toolbarLook();
    }
    const tab = await popupTab(m.tab);
    let videos = 0;
    if (tab) {
      await mediaBack;
      const kept = media.get(tab.id) || { items: [] };
      videos = (kept.items || []).filter((it) => it.kind !== "image" && !it.ranged && isHere(kept, it)).length;
    }
    const { paused } = await ext.storage.local.get({ paused: false });
    return { ok: true, state: look.state, title: look.title, toggle: look.click !== "status", paused, web: Boolean(tab), site: tab ? tabSite(tab) : "", videos };
  }
  if (m.type === "popup-status") {
    await openStatusPage();
    return { ok: true };
  }
  const tab = await popupTab(m.tab);
  if (!tab) return { ok: false };
  const incognito = Boolean(tab.incognito);
  const storeId = tab.cookieStoreId;
  // The tab's video: the Download video window, as the button over a player sends it (the stream
  // the tab loaded, when one is known; else the page, for yt-dlp to read).
  if (m.type === "popup-video") {
    await mediaBack;
    // Copy-protected (#drm): said in the panel, nothing sent.
    if ((media.get(tab.id) || {}).drm) return { ok: false, why: "drm" };
    const src = await streamFor({ tab, frameId: 0 });
    const title = String(tab.title || "").slice(0, 300);
    const reply = await native({ video: true, explicit: true, url: tab.url, ...(src ? { src } : {}), ...(title ? { title } : {}), ...(await videoCookies(src || tab.url, { incognito, storeId })), private: incognito });
    if (!reply.ok && reply.why !== "options") explain(reply.why);
    return { ok: Boolean(reply.ok) };
  }
  // The tab's links: DDM's Link grabber, as the page's right-click menu's Download all links.
  if (m.type === "popup-all") {
    let links = [];
    try {
      const [res] = await ext.scripting.executeScript({ target: { tabId: tab.id, frameIds: [0] }, func: pageLinks, args: [false] });
      links = (res && Array.isArray(res.result) && res.result) || [];
    } catch {
      // A page extensions may not read.
    }
    if (!links.length) return { ok: false, why: "no-links" };
    const reply = await native({ links, referrer: tab.url, cookies: await cookieHeader(tab.url, { incognito, storeId }), private: incognito });
    if (!reply.ok) explain(reply.why);
    return { ok: Boolean(reply.ok) };
  }
  return { ok: false };
}
