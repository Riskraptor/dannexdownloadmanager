// The files a player played, offered as they are. A part of the background: background.js loads the
// parts in order, into one scope.

// ---- the files a player played, offered as they are ------------------------------------------
// A video the page already played came with the browser's cookies and referrer and passed the
// site's checks: its own file needs no extractor. The menu lists such files at once beside the
// app's qualities (video.js `video-files`), and the one picked goes to the app as a download of
// its own (`video-file`: the browser's cookies for its server, the page as its Referer, the
// browser's User-Agent), Download File Info and DDM's engine as for a link, yt-dlp never asked.
// TikTok's player fetches a whole MP4 and plays it from a blob: (its address is in no element;
// yt-dlp reading the page meets TikTok's bot check). Which file a player plays is known for sure
// only from its own address; a file it was fed is checked by its header: an MP4's length (and its
// picture's size) must be the player's, read in the page's frame (its origin, cookies and
// Referer, as the player's own fetch), 64 KB at most.
const OFFER_MS = 10 * 60000;
const HEAD_BYTES = 64 << 10;
// The most files read for one player, the newest the tab loaded (a feed fetches the next few
// videos ahead of the one playing).
const HEAD_FILES = 8;
const offers = new Map(); // tab id -> token -> {token, page, title, audio, at, files: [{url, mime, size, ext}]}
const OFFERS_KEPT = 4;
ext.tabs.onRemoved.addListener((id) => offers.delete(id));

// Run in the page (its content scripts' world: the page's origin and cookies, not its own
// scripts' fetch): what each address's first `bytes` say of an MP4 whose header comes first (as
// files for the web are made), {duration (seconds), width, height (its first picture track's;
// 0: none), encrypted (its tracks are: CENC's encv/enca sample entries, a pssh box; #drm)}, or
// null (another kind of file, its header at the end, refused, 4 s gone). No outside names: only
// its arguments.
async function mediaHead(urls, bytes) {
  const facts = (b) => {
    const u32 = (i) => b[i] * 2 ** 24 + (b[i + 1] << 16) + (b[i + 2] << 8) + b[i + 3];
    const boxes = (from, to) => {
      const out = [];
      for (let i = from; i + 8 <= to; ) {
        let size = u32(i);
        let head = 8;
        if (size === 1) {
          if (i + 16 > to) break;
          size = u32(i + 8) * 2 ** 32 + u32(i + 12);
          head = 16;
        } else if (size === 0) size = to - i;
        if (size < head) break;
        out.push({ type: String.fromCharCode(b[i + 4], b[i + 5], b[i + 6], b[i + 7]), at: i, size, head });
        i += size;
      }
      return out;
    };
    const top = boxes(0, b.length);
    const moov = top.find((x) => x.type === "moov");
    if (!top.length || top[0].type !== "ftyp" || !moov) return null;
    const end = Math.min(b.length, moov.at + moov.size);
    const inner = boxes(moov.at + moov.head, end);
    // Encrypted (Common Encryption: the key system's pssh box, a protected sample entry): its
    // picture and sound can't be played without the page's DRM, so it is never offered.
    const tag = (s) => {
      const c = [...s].map((ch) => ch.charCodeAt(0));
      for (let i = moov.at + 8; i + 4 <= end; i += 1) if (b[i] === c[0] && b[i + 1] === c[1] && b[i + 2] === c[2] && b[i + 3] === c[3]) return true;
      return false;
    };
    if (inner.some((x) => x.type === "pssh") || tag("encv") || tag("enca") || tag("tenc")) return { duration: 0, width: 0, height: 0, encrypted: true };
    // A fragmented MP4 (mvex, moof) is a stream's piece or a DASH representation, not a file
    // that plays on its own: its picture often comes without its sound.
    if (inner.some((x) => x.type === "mvex") || top.some((x) => x.type === "moof")) return null;
    const mvhd = inner.find((x) => x.type === "mvhd");
    const p = mvhd ? mvhd.at + mvhd.head : 0;
    if (!mvhd || p + 32 > end) return null;
    const v1 = b[p] === 1;
    const scale = u32(p + (v1 ? 20 : 12));
    const length = v1 ? u32(p + 24) * 2 ** 32 + u32(p + 28) : u32(p + 16);
    if (!scale || !length) return null;
    let width = 0;
    let height = 0;
    for (const trak of inner.filter((x) => x.type === "trak")) {
      const tkhd = boxes(trak.at + trak.head, Math.min(end, trak.at + trak.size)).find((x) => x.type === "tkhd");
      if (!tkhd) continue;
      const q = tkhd.at + tkhd.head;
      const at = q + (b[q] === 1 ? 88 : 76);
      if (at + 8 > end) break;
      const [w, h] = [u32(at) / 65536, u32(at + 4) / 65536];
      if (w > 0 && h > 0) {
        [width, height] = [Math.round(w), Math.round(h)];
        break;
      }
    }
    return { duration: length / scale, width, height };
  };
  const read = async (url) => {
    const stop = new AbortController();
    const timer = setTimeout(() => stop.abort(), 4000);
    let reader = null;
    try {
      const r = await fetch(url, { headers: { Range: `bytes=0-${bytes - 1}` }, credentials: "include", signal: stop.signal });
      if (!r.ok || !r.body) return null;
      reader = r.body.getReader();
      const b = new Uint8Array(bytes);
      let got = 0;
      while (got < bytes) {
        const { done, value } = await reader.read();
        if (done) break;
        const n = Math.min(value.length, bytes - got);
        b.set(value.subarray(0, n), got);
        got += n;
      }
      return facts(b.subarray(0, got));
    } catch {
      return null;
    } finally {
      clearTimeout(timer);
      if (reader) reader.cancel().catch(() => {});
    }
  };
  return Promise.all(urls.map(read));
}

// A file's type as a file name ends: from its MIME type, else its address's, else the player's
// kind's usual one.
const FILE_EXT = { "video/mp4": "mp4", "video/webm": "webm", "video/quicktime": "mov", "video/x-matroska": "mkv", "video/x-flv": "flv", "video/3gpp": "3gp", "audio/mp4": "m4a", "audio/mpeg": "mp3", "audio/webm": "weba", "audio/ogg": "ogg", "audio/aac": "aac", "audio/flac": "flac", "audio/wav": "wav", "application/ogg": "ogg" };
function fileExt(url, mime, audio) {
  return FILE_EXT[String(mime || "").toLowerCase()] || (MEDIA_EXT.exec(pathOf(String(url)).toLowerCase()) || [])[1] || (audio ? "m4a" : "mp4");
}

// Of the files `heads` describe (the tab's, `candidates`), those the player of `want` ({duration,
// width, height}) plays: the same length (half a second or 1%); of several, those of its picture's
// size; still several, only when they are one video's (the same length to 50 ms: its qualities),
// else none (two videos of one length in a feed: neither is sure).
function playedBy(candidates, heads, want) {
  const duration = positive(want.duration);
  if (!duration) return [];
  const near = (d) => Math.abs(d - duration) <= Math.max(0.5, duration * 0.01);
  const same = candidates.map((f, i) => ({ f, h: heads[i] })).filter((x) => x.h && near(x.h.duration));
  if (same.length <= 1) return same;
  const sized = same.filter((x) => want.width > 0 && x.h.width === want.width && x.h.height === want.height);
  if (sized.length === 1) return sized;
  return same.every((x) => Math.abs(x.h.duration - same[0].h.duration) < 0.05) ? same : [];
}

// Is `named` (the post a video sits in) the page `page` itself? The same address but for what the
// page's own adds (`?is_from_webapp=1`): its path, and each of its query's names with its value.
function samePage(named, page) {
  try {
    const [a, b] = [new URL(named), new URL(page)];
    if (a.origin !== b.origin || a.pathname.replace(/\/$/, "") !== b.pathname.replace(/\/$/, "")) return false;
    return [...a.searchParams].every(([k, v]) => b.searchParams.getAll(k).includes(v));
  } catch {
    return false;
  }
}

// The headers read (mediaHead), by address, for HEADS_MS: a file read once is not read again for
// the next player, menu or prefetch (a feed asks for each video it plays). HEADS_MAX addresses.
const heads = new Map(); // url -> {head, at}
const HEADS_MS = 10 * 60000;
const HEADS_MAX = 100;
function keptHead(url) {
  const h = heads.get(url);
  return h && Date.now() - h.at < HEADS_MS ? h : null;
}
function keepHead(url, head) {
  heads.delete(url);
  heads.set(url, { head, at: Date.now() });
  for (const k of heads.keys()) {
    if (heads.size <= HEADS_MAX) break;
    heads.delete(k);
  }
}

// The heads of `candidates` (the tab's files) in frame `frameId`: read in the page, then from here
// what the page couldn't read, each address once (keptHead). [head or null] in their order.
async function headsOf(tabId, frameId, candidates) {
  const out = candidates.map((c) => (keptHead(c.url) || {}).head || null);
  const toRead = candidates.filter((c) => !keptHead(c.url));
  if (!toRead.length) return out;
  let read = [];
  try {
    const [res] = await withTimeout(
      ext.scripting.executeScript({ target: { tabId, frameIds: [frameId] }, func: mediaHead, args: [toRead.map((c) => c.url), HEAD_BYTES] }),
      6000,
    );
    read = (res && Array.isArray(res.result) && res.result) || [];
  } catch {
    read = [];
  }
  // What the page couldn't read (a server whose CORS allows no Range from the page: Reddit's),
  // read from here: the site's cookies, no CORS, but no page for a Referer (TikTok's servers
  // want one: the page's own read serves them).
  const unread = toRead.filter((c, i) => !read[i]);
  if (unread.length) {
    const again = await withTimeout(mediaHead(unread.map((c) => c.url), HEAD_BYTES), 6000).catch(() => []);
    read = toRead.map((c, i) => read[i] || again[unread.indexOf(c)] || null);
  }
  toRead.forEach((c, i) => {
    // Only what was read is kept: a refusal or a time-out may read next time.
    if (read[i]) keepHead(c.url, read[i]);
  });
  return candidates.map((c, i) => out[i] || read[toRead.indexOf(c)] || null);
}

// video.js, as the menu opens (or the player starts playing in view): the files the clicked
// player plays, {token, files: [{n, ext, size, height, audio, own}]} (`own`: its own address,
// which the app looks up as well); {files: []} when none is sure, or downloads go to another DDM.
// A file read as encrypted (#drm) is never offered.
async function videoFiles(m, page, sender) {
  const tab = sender.tab || {};
  if (tab.id === undefined || tab.id === null || (await remoteTarget())) return { files: [] };
  await mediaBack;
  const kept = media.get(tab.id) || { items: [] };
  const items = kept.items || [];
  const audio = m.kind === "audio";
  const played = [m.played, m.src].find((u) => typeof u === "string" && /^https?:/i.test(u) && u.length <= URL_MAX && !MANIFEST.test(u) && !items.some((it) => it.url === u && isStream(it)));
  let found = [];
  if (played) {
    const it = items.find((x) => x.url === played) || {};
    found = [{ url: played, mime: it.mime || "", size: it.size || 0, height: audio ? null : positive(m.height), own: true, name: it.name || "" }];
  } else if (ext.scripting) {
    const others = new Set(Array.isArray(m.others) ? m.others.filter((u) => typeof u === "string") : []);
    const frameId = sender.frameId || 0;
    // The frame's files of the player's kind (an element's own typeless answer may be either),
    // the most recently fetched first: a feed fetches the next videos ahead of the one playing.
    // Those seen before the page moved in place too: its player may play on (X, TikTok).
    const candidates = items
      .filter((it) => (it.kind === (audio ? "audio" : "video") || it.typeless) && it.frameId === frameId && !others.has(it.url) && !it.live)
      .sort((a, b) => (b.seen || 0) - (a.seen || 0))
      .slice(0, HEAD_FILES);
    if (candidates.length && positive(m.duration)) {
      const read = await headsOf(tab.id, frameId, candidates);
      const want = { duration: m.duration, width: Number(m.width) || 0, height: Number(m.height) || 0 };
      const usable = read.map((h) => (h && h.encrypted ? null : h));
      found = playedBy(candidates, usable, want).map(({ f, h }) => ({ url: f.url, mime: f.mime, size: f.size, height: audio ? null : h.height || null, own: false, name: "" }));
    }
  }
  if (!found.length) return { files: [] };
  const files = found
    .map((f) => ({ ...f, ext: fileExt(f.url, f.mime, audio) }))
    .sort((a, b) => (b.height || 0) - (a.height || 0) || (b.size || 0) - (a.size || 0));
  // The page's title names the file only on the video's own page (not a feed's, nor a post's in it);
  // what the page's player says it plays (`track`: its Media Session's title, a sound's) anywhere;
  // a radio station's name.
  const named = typeof m.url === "string" ? m.url : "";
  const track = typeof m.track === "string" ? m.track.trim().slice(0, 300) : "";
  const title = track || (typeof m.title === "string" && named && samePage(named, page) ? m.title.trim().slice(0, 300) : "") || (files[0] && files[0].name) || "";
  // Which offer a pick is from (a newer one takes its place): no secret, the page never sees it.
  const token = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  // The tab's last few offers: a menu's, and one asked meanwhile for the same player (as it plays
  // on) don't take each other's place.
  const recent = offers.get(tab.id) || new Map();
  recent.set(token, { token, page, title, audio, at: Date.now(), files });
  for (const k of recent.keys()) {
    if (recent.size <= OFFERS_KEPT) break;
    recent.delete(k);
  }
  offers.set(tab.id, recent);
  return {
    token,
    files: files.map((f, n) => ({ n, ext: f.ext, ...(f.size > 0 ? { size: f.size } : {}), ...(f.height ? { height: f.height } : {}), audio, own: f.own })),
  };
}

// A file name of `title` and `ext`: no character a file system refuses, 150 at most.
function fileName(title, ext) {
  const stem = String(title).replace(/[\\/:*?"<>|\u0000-\u001f]+/g, " ").replace(/\s+/g, " ").trim().slice(0, 150).trim();
  return `${stem || "video"}.${ext}`;
}

// The file picked from those videoFiles offered this tab (`file`: its number), handed to the app
// as a download (Download File Info), named after `title` (the app's lookup's, when the menu had
// it: TikTok's page names only the site), else what the page's player said it plays or the
// page's title, else the site's.
async function videoFile(m, page, sender) {
  const tab = sender.tab || {};
  const o = typeof m.token === "string" ? (offers.get(tab.id) || new Map()).get(m.token) : null;
  if (!o || o.token !== m.token || Date.now() - o.at > OFFER_MS || o.page.split("#")[0] !== page.split("#")[0]) return { ok: false, why: "gone" };
  const f = Number.isInteger(m.file) ? o.files[m.file] : null;
  if (!f) return { ok: false, why: "gone" };
  const given = typeof m.title === "string" ? m.title.trim().slice(0, 300) : "";
  const reply = await handOff({
    url: f.url,
    referrer: page,
    incognito: Boolean(tab.incognito),
    ...(tab.cookieStoreId ? { storeId: tab.cookieStoreId } : {}),
    filename: fileName(given || o.title || `${siteOf(page).replace(/^www./, "")} ${o.audio ? "audio" : "video"}`, f.ext),
    ...(f.size > 0 ? { size: f.size } : {}),
    ...(f.mime ? { mime: f.mime } : {}),
    explicit: true,
  });
  if (!reply.ok) explain(reply.why);
  return { ok: Boolean(reply.ok), why: reply.why };
}

async function pageMessage(m, sender) {
  // A copy standing aside for the Chrome Web Store's answers its pages nothing: the other copy's
  // scripts in the same page do the work (one Download video button, one hand-off per click).
  if ((await copyState()).dormant) return m.type === "panel" ? { on: false } : { ok: false };
  const page = senderPage(sender);
  const tab = sender.tab || {};
  if (m.type === "panel") {
    const top = tab.url && /^https?:/i.test(tab.url) ? tab.url : "";
    return page ? panelOn(page, top) : { on: false };
  }
  if (m.type === "link" && typeof m.url === "string") {
    const name = typeof m.name === "string" ? m.name.split(/[\\/]/).pop().slice(0, 255) : "";
    return link(m.url, page || "", Boolean(tab.incognito), name);
  }
  // page.js, at a page's start: the rules it goes by (the keys; Safari's file types), asked again
  // when old. Nothing else asks them in Safari, whose background sees no downloads (see `link`).
  if (m.type === "rules") {
    await freshRules(PAGE_RULES_MS);
    return { ok: true };
  }
  // page.js: a link clicked with Options' keys for it (Keys...): DDM takes it whatever its type,
  // as Download with DDM in the right-click menu does. The page did nothing with the click.
  if (m.type === "force" && typeof m.url === "string" && /^(https?|ftps?|ftpes):/i.test(m.url) && m.url.length <= URL_MAX) {
    const name = typeof m.name === "string" ? m.name.split(/[\\/]/).pop().slice(0, 255) : "";
    const reply = await handOff({
      url: m.url,
      referrer: page || "",
      incognito: Boolean(tab.incognito),
      ...(tab.cookieStoreId ? { storeId: tab.cookieStoreId } : {}),
      ...(name ? { filename: name } : {}),
      explicit: true,
    });
    if (!reply.ok) explain(reply.why);
    return { ok: Boolean(reply.ok), why: reply.why };
  }
  if (m.type === "bypass" && typeof m.url === "string") {
    bypass.set(m.url, { at: Date.now() });
    return { ok: true };
  }
  // video.js: a player in this frame decrypts its media (#drm: MediaKeys, encrypted data). The
  // toolbar panel's Download video then says so for the tab, rather than send it.
  if (m.type === "drm") {
    await mediaBack;
    const kept = tab.id !== undefined && media.get(tab.id);
    if (kept) kept.drm = true;
    return { ok: true };
  }
  if (m.type === "video" && page) {
    const v = await videoTarget(m, page, sender);
    if (!v) return { ok: false, why: isYouTube(page) ? "unnamed" : "feed" };
    const reply = await native({
      video: true,
      url: v.own,
      ...(v.src ? { src: v.src } : {}),
      title: v.title,
      referrer: v.referrer,
      cookies: v.cookies,
      cookieJar: v.cookieJar,
      ...(tab.incognito ? { private: true } : {}),
    });
    return { ok: Boolean(reply.ok), why: reply.why };
  }
  if (m.type === "video-lookup" && page) return videoLookup(m, page, sender);
  if (m.type === "video-poll") return videoPoll(m, sender);
  if (m.type === "video-pick") return videoPick(m, sender);
  if (m.type === "video-cancel") return videoCancel(m, sender);
  if (m.type === "video-files" && page) return videoFiles(m, page, sender);
  if (m.type === "video-file" && page) return videoFile(m, page, sender);
  if (m.type === "hide-site" && page) {
    const host = new URL(page).hostname;
    // Off at once on this site, whatever the tab around it, while the app saves it.
    panelCache.set(host, { answer: { on: false }, hidden: true, at: Date.now() });
    // The site without its www.: the app's list takes a name's subdomains too, so the button
    // stays off www., m. and the bare name alike (video.js names the site so in its menu).
    const reply = await native({ hideSite: siteName(host) });
    return { ok: Boolean(reply.ok) };
  }
  if (m.type === "options") {
    const reply = await native({ openOptions: true });
    return { ok: Boolean(reply.ok), why: reply.why };
  }
  return { ok: false };
}

// A site as the button's "Never on" names it: its host without www.
const siteName = (host) => String(host || "").replace(/^www\./i, "");

// What the browser lets this extension do, for the status page: the install's kind (Firefox's
// temporary add-on is "development"), private windows, every site.
async function selfInfo() {
  let installType = "";
  try {
    installType = (ext.management && ext.management.getSelf && (await ext.management.getSelf()).installType) || "";
  } catch {
    installType = "";
  }
  let incognito = null;
  try {
    incognito = ext.extension && ext.extension.isAllowedIncognitoAccess ? Boolean(await ext.extension.isAllowedIncognitoAccess()) : null;
  } catch {
    incognito = null;
  }
  let allSites = true;
  try {
    allSites = Boolean(await ext.permissions.contains({ origins: ["<all_urls>"] }));
  } catch {
    allSites = true;
  }
  const { temporary } = await ext.storage.local.get({ temporary: false });
  return { installType, incognito, allSites, temporary: FIREFOX && (installType === "development" || Boolean(temporary)) };
}

// Everything the status page (install.html) and the options page show: whether DDM is reached and
// why not (`why`, the help page's sections), the app's and this extension's versions, the switch,
// DDM's own Options (`captureOff`), another DDM chosen (`remote`), this copy standing aside
// (`duplicate`), what the browser allows (`selfInfo`), the store's page to offer (`store`),
// whether DDM came with a newer extension than this one (`behind`), and the last download left to
// the browser and why (`declined`, #208).
async function fullStatus() {
  const r = await storedRules();
  const s = await readStatus();
  const { paused, declined } = await ext.storage.local.get({ paused: false, declined: null });
  const remote = await remoteTarget();
  const state = await copyState();
  const info = await selfInfo();
  const reached = remote ? true : Boolean(r) && (!s.why || s.why === "update-app");
  // Never reached and nothing said why (the host answered without rules): as not installed.
  const why = remote ? null : s.why || (r ? null : "not-installed");
  return {
    connected: reached,
    why,
    app: s.app || null,
    version: VERSION,
    browser: SAFARI ? "safari" : FIREFOX ? "firefox" : "chromium",
    copy: thisCopy(),
    paused,
    declined,
    remote: remote ? remote.url : null,
    captureOff: !remote && r && r.capture === false ? r.captureOff || "browser" : "",
    duplicate: state.dormant ? "store" : "",
    store: storeOffer(r),
    behind: Boolean(reached && s.app && older(VERSION, s.app)),
    look: (await toolbarLook()).state,
    ...info,
  };
}

// The status page (install.html) and the options page.
async function ownPageMessage(m) {
  // The status page asks again every few seconds while it is open: the host is asked each time
  // (it answers without starting DDM), so the page turns green by itself once DDM is installed.
  if (m.type === "status" || m.type === "probe") {
    if (m.type === "probe") {
      await copyState(0);
      await refreshRules();
    }
    return fullStatus();
  }
  // The options page, as it opens: the rules asked again, then all of it.
  if (m.type === "state") {
    await refreshRules();
    return fullStatus();
  }
  // The status and options pages' switch (the same as a click on the toolbar button).
  if (m.type === "pause") {
    await setPaused(m.paused);
    return { ok: true };
  }
  if (m.type === "remote-pair" && typeof m.url === "string" && typeof m.code === "string") return remotePair(m.url, m.code);
  if (m.type === "remote-off") return remoteOff();
  // Turned off in DDM's Options: the status page's one button opens them there.
  if (m.type === "open-options") return { ok: Boolean((await native({ openOptions: true, page: "general" })).ok) };
  if (/^popup(-\w+)?$/.test(m.type)) return popupMessage(m);
  return { ok: false };
}
