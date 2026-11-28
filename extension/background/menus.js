// The right-click menus: on pages, links, media and the toolbar button. A part of the background:
// background.js loads the parts in order, into one scope.

// ---- right-click menus -------------------------------------------------------------------

// The items, by their names between the app and the extension (mdm_ipc::capture::MENU_ITEMS;
// each item's id is `mdm-<name>`): where each shows, and its text.
const MENUS = [
  ["link", ["link", "video", "audio", "image"], "menu"],
  ["selected", ["selection"], "menu_selected"],
  ["all", ["page"], "menu_all"],
  ["images", ["page"], "menu_images"],
  ["media", ["page"], "menu_media"],
];
const MENU_NAMES = MENUS.map(([name]) => name);

// The items Options shows (Customize DDM menu items: the rules' `menu`), created anew (removed
// first: they persist, and a second create fails), in the browser's language (_locales, from the
// app's catalogs by tools/extension/locales.py). Which ones were made is kept (`menus`), so they
// are made again only when Options changes them (`syncMenus`), or always (`anew`: at install and
// update, for the new version's texts). One at a time, each going by the rules as they are when
// its turn comes: an install's and the first answer's never cross.
// The toolbar button's own right-click menu: only what a person needs beside its click. Don't take
// downloads from this site; Help (the status page: how things stand, what to do, the help page);
// and, for the copy loaded from DDM's folder once DDM says the Chrome Web Store lists the
// extension (`storeOffer`), the store's page. No Options item: the browser adds its own to an
// extension that has an options page. A copy standing aside for the store's (`copies`) keeps Help
// only.
const TOOLBAR_MENUS = [
  ["exclude", "toolbar_exclude"],
  ["help", "toolbar_help"],
  ["store", "toolbar_store"],
];
function toolbarMenus(r, aside) {
  if (aside) return ["help"];
  const store = storeOffer(r) ? ["store"] : [];
  return ["exclude", "help", ...store];
}

// The Chrome Web Store's page for the copy loaded unpacked from DDM's folder (it never updates
// itself), once DDM says the store lists the extension (the rules' `store`; an older DDM says
// nothing, nor one before the listing is public): offered once, in the toolbar button's menu and
// on the status and options pages, never by a badge. "" otherwise: the store's copy, Firefox's,
// Safari's.
function storeOffer(r) {
  if (FIREFOX || SAFARI || thisCopy() !== "unpacked" || !r || typeof r.store !== "string") return "";
  return /^https:\/\/chromewebstore\.google\.com\/detail\/[\w-]+\/[a-p]{32}$/.test(r.store) ? r.store : "";
}

let menuWork = Promise.resolve();
function createMenus(anew = true) {
  menuWork = menuWork
    .then(async () => {
      const r = await storedRules();
      // A copy standing aside shows none of the page's items: the other copy's are there.
      const aside = (await copyState()).dormant;
      const want = aside ? [] : r && Array.isArray(r.menu) ? r.menu : MENU_NAMES;
      const toolbar = toolbarMenus(r, aside);
      const made = [...MENU_NAMES.filter((n) => want.includes(n)), "|", ...toolbar].join(" ");
      if (!anew && (await ext.storage.local.get({ menus: null })).menus === made) return;
      await new Promise((done) =>
        ext.contextMenus.removeAll(() => {
          for (const [name, contexts, key] of MENUS) {
            if (!want.includes(name)) continue;
            ext.contextMenus.create({ id: `mdm-${name}`, title: t(key), contexts }, () => void ext.runtime.lastError);
          }
          // Safari has no menu on its toolbar button: refused there, quietly (its options page
          // has the same).
          for (const [name, key] of TOOLBAR_MENUS) {
            if (!toolbar.includes(name)) continue;
            try {
              ext.contextMenus.create({ id: `mdm-toolbar-${name}`, title: t(key), contexts: ["action"] }, () => void ext.runtime.lastError);
            } catch {
              // A browser that knows no "action" menu.
            }
          }
          done();
        }),
      );
      await ext.storage.local.set({ menus: made });
    })
    .catch(() => {});
  return menuWork;
}

// The right-click menu as the rules say, made again when it isn't.
const syncMenus = () => createMenus(false);

// The pictures (`kind` "images"), or the videos and sounds ("media"), of the page (the frame
// clicked), collected in the page: what its elements show or play (a picture at the largest size
// its srcset offers, or the address a lazy page keeps for it) and its links to such files. Never
// data: or blob: (nothing to download by address), each address once.
function pageMedia(kind) {
  const images = kind === "images";
  const type = images
    ? /\.(jpe?g|png|gif|webp|avif|bmp|svg|tiff?|heic|heif|jxl|ico)$/i
    : /\.(mp4|m4v|webm|mkv|mov|avi|wmv|flv|3gp|ogv|mpe?g|mp3|m4a|aac|ogg|oga|opus|flac|wav|wma|mka)$/i;
  const seen = new Set();
  const out = [];
  // True when `address` is a web or FTP address (listed now or before).
  const add = (address) => {
    const text = String(address || "").trim();
    if (!text) return false;
    let u;
    try {
      u = new URL(text, document.baseURI);
    } catch {
      return false;
    }
    if (!/^(https?|ftps?|ftpes):$/i.test(u.protocol)) return false;
    if (!seen.has(u.href)) {
      seen.add(u.href);
      out.push(u.href);
    }
    return true;
  };
  // A srcset's largest picture: by its width (`800w`) or its density (`2x`).
  const largest = (srcset) => {
    let best = null;
    for (const part of String(srcset || "").split(/,\s+/)) {
      const [address, size = "1x"] = part.trim().split(/\s+/);
      const score = parseFloat(size) || 1;
      if (address && (!best || score > best.score)) best = { address, score };
    }
    return best ? best.address : "";
  };
  const attr = (el, name) => el.getAttribute(name) || "";
  if (images) {
    for (const img of document.querySelectorAll("img")) {
      [largest(attr(img, "srcset")), largest(attr(img, "data-srcset")), attr(img, "data-src"), attr(img, "data-lazy-src"), attr(img, "data-original"), img.currentSrc, img.src].some(add);
    }
    for (const s of document.querySelectorAll("picture source[srcset]")) add(largest(attr(s, "srcset")));
  } else {
    for (const m of document.querySelectorAll("video, audio")) {
      if (!add(m.currentSrc) && attr(m, "src")) add(m.src);
    }
    for (const s of document.querySelectorAll("video source[src], audio source[src]")) add(s.src);
  }
  for (const a of document.querySelectorAll("a[href]")) {
    if (type.test(String(a.href).split(/[?#]/)[0])) add(a.href);
  }
  return out;
}

// The video and sound files the tab played (#38): their own addresses.
// A stream's manifest is no file to download as one: those go to the Download video window.
async function playedFiles(tabId) {
  await mediaBack;
  const kept = media.get(tabId);
  // Not a file fetched in ranges (a DASH stream's own files come so: a picture without its sound),
  // nor one of the page's before it moved in place, nor one read as encrypted (#drm).
  const encrypted = (it) => Boolean((keptHead(it.url) || {}).head && keptHead(it.url).head.encrypted);
  return kept ? kept.items.filter((it) => (it.kind === "video" || it.kind === "audio") && !it.ranged && isHere(kept, it) && !encrypted(it)).map((it) => it.url) : [];
}

// The links in the page (all of them, or those in the selection), collected in the page, each
// with the text the page shows for it (its words, else a picture's description or its title):
// Link grabber's Link text column. `{ links, texts }`, in step.
function pageLinks(selectedOnly) {
  const ok = (h) => /^(https?|ftps?|ftpes):/i.test(h);
  // textContent, not innerText: no layout for a page of thousands of links.
  const words = (s) => String(s || "").replace(/\s+/g, " ").trim().slice(0, 200);
  const textOf = (a) => {
    const own = words(a.textContent);
    if (own) return own;
    const img = a.querySelector("img[alt], img[title]");
    return words((img && (img.alt || img.title)) || a.title || a.getAttribute("aria-label"));
  };
  let anchors = [...document.querySelectorAll("a[href]")];
  if (selectedOnly) {
    const sel = getSelection();
    anchors = sel.rangeCount ? anchors.filter((a) => sel.containsNode(a, true)) : [];
  }
  const seen = new Map();
  const links = [];
  const texts = [];
  for (const a of anchors) {
    if (!ok(a.href)) continue;
    const at = seen.get(a.href);
    if (at === undefined) {
      seen.set(a.href, links.length);
      links.push(a.href);
      texts.push(textOf(a));
    } else if (!texts[at]) {
      // A picture's link first, its words further on: the words.
      texts[at] = textOf(a);
    }
  }
  for (const m of document.querySelectorAll("video[src], audio[src], source[src]")) {
    if (ok(m.src) && !seen.has(m.src) && !selectedOnly) {
      seen.set(m.src, links.length);
      links.push(m.src);
      const media = m.closest("video, audio") || m;
      texts.push(words(media.title || media.getAttribute("aria-label")));
    }
  }
  return { links, texts };
}

// When the user asked and it couldn't be done: DDM not reached opens the status page (live: it
// says what is missing and turns green once DDM answers); anything else, the help page's section
// (a link DDM refused has none of its own: the connection's).
const CONNECTION = ["not-installed", "forbidden", "not-running", "host-failed"];
const SECTIONS = [...CONNECTION, "too-large", "update-app", "options"];
function explain(why) {
  if (!why || CONNECTION.includes(why)) {
    openStatusPage().catch(() => {});
    return;
  }
  const at = SECTIONS.includes(why) ? why : "host-failed";
  Promise.resolve(ext.tabs.create({ url: `${HELP}#${at}` })).catch(() => {});
}

// The site of a tab, for Never take downloads from this site: a web page's host, else "".
function tabSite(tab) {
  try {
    const u = new URL(tab && tab.url);
    return /^https?:$/.test(u.protocol) ? u.hostname : "";
  } catch {
    return "";
  }
}

// The toolbar button's right-click menu (TOOLBAR_MENUS).
async function toolbarMenu(name, tab) {
  if (name === "help") return openStatusPage();
  if (name === "store") {
    const page = storeOffer(await storedRules());
    if (page) await Promise.resolve(ext.tabs.create({ url: page })).catch(() => {});
    return;
  }
  if (name === "exclude") {
    const site = tabSite(tab);
    if (!site) return flashBadge("–", BADGE_GREY);
    const reply = await native({ excludeSite: site });
    if (!reply.ok) return explain(reply.why);
    await refreshRules();
    flashBadge("✓", "#107C10");
  }
}

ext.contextMenus.onClicked.addListener(async (info, tab) => {
  const incognito = Boolean(tab && tab.incognito);
  const storeId = tab && tab.cookieStoreId;
  const toolbar = /^mdm-toolbar-(\w+)$/.exec(String(info.menuItemId));
  if (toolbar) return toolbarMenu(toolbar[1], tab).catch(() => {});
  if (info.menuItemId === "mdm-link") {
    const url = info.linkUrl || info.srcUrl;
    // The tab's title, as the button's click sends it: the app names a video yt-dlp knows only
    // by its address (a stream's `master`, a long hash) after it (#345).
    const title = String((tab && tab.title) || "").slice(0, 300);
    // A player's own stream (blob:, a MediaSource) or a stream's manifest: the Download video
    // window, which reads streams (a manifest downloaded as a file is only a short list).
    // The manifest's own cookies (its server may not be the page's), as the media list sends.
    if (url && MANIFEST.test(url)) {
      const reply = await native({ video: true, explicit: true, url: info.pageUrl, src: url, title, ...(await videoCookies(url, { incognito, storeId })), private: incognito });
      if (!reply.ok && reply.why !== "options") explain(reply.why);
      return;
    }
    // A player playing from a blob: has no address yt-dlp can read: the stream its frame loaded,
    // as the button over videos sends (#155: the page alone was sent, and yt-dlp, not knowing
    // the site, answered "Unsupported URL"). Firefox names no address at all for a player fed
    // by a MediaSource (nothing was sent then).
    const player = info.mediaType === "video" || info.mediaType === "audio";
    if ((url && /^(blob|mediastream):/i.test(url)) || (player && !url)) {
      const page = info.frameUrl || info.pageUrl;
      await mediaBack;
      const src = await streamFor({ tab, frameId: info.frameId || 0 });
      const reply = await native({
        video: true,
        explicit: true,
        url: page,
        ...(src ? { src } : {}),
        ...(title ? { title } : {}),
        referrer: info.pageUrl && info.pageUrl !== page ? info.pageUrl : "",
        ...(await videoCookies(src || page, { incognito, storeId })),
        private: incognito,
      });
      if (!reply.ok && reply.why !== "options") explain(reply.why);
      return;
    }
    // ftp links too: browsers no longer download them, DDM does. A magnet link opens DDM's Add
    // torrent window (asked for only: a magnet clicked in a page stays the browser's to hand to
    // its program).
    if (!url || !(/^(https?|ftps?|ftpes):/i.test(url) || /^magnet:\?/i.test(url)) || url.length > URL_MAX) return;
    const reply = await handOff({ url, referrer: info.pageUrl, incognito, storeId, explicit: true });
    if (!reply.ok) explain(reply.why);
    return;
  }
  // A page's links, pictures, or videos and sounds: to DDM's Link grabber, to tick there.
  const grab = {
    "mdm-selected": [pageLinks, [true]],
    "mdm-all": [pageLinks, [false]],
    "mdm-images": [pageMedia, ["images"]],
    "mdm-media": [pageMedia, ["media"]],
  }[info.menuItemId];
  if (grab && tab) {
    let links = [];
    let texts = [];
    try {
      const [res] = await ext.scripting.executeScript({
        target: { tabId: tab.id, frameIds: [info.frameId || 0] },
        func: grab[0],
        args: grab[1],
      });
      const found = res && res.result;
      // pageLinks: `{ links, texts }`; pageMedia: the addresses alone.
      if (Array.isArray(found)) links = found;
      else if (found && Array.isArray(found.links)) {
        links = found.links;
        texts = Array.isArray(found.texts) ? found.texts : [];
      }
    } catch {
      // A page extensions may not read (the browser's own, a store).
    }
    // The files the page's players played without naming them in the page (a player's fetch).
    if (info.menuItemId === "mdm-media") links = [...new Set([...links, ...(await playedFiles(tab.id))])];
    if (!links.length) return;
    const message = { links, referrer: info.pageUrl, cookies: await cookieHeader(info.pageUrl, { incognito, storeId }), private: incognito };
    if (texts.some(Boolean)) message.texts = texts.slice(0, links.length);
    const reply = await native(message);
    if (!reply.ok) explain(reply.why);
  }
});
