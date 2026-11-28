// Media detection: the media each tab loaded (#38), kept across the script's stops, and what a
// stream's manifest says. A part of the background: background.js loads the parts in order, into
// one scope.

// ---- the media a tab loaded (#38) ------------------------------------------------------------

// What the tab's pages loaded that can be downloaded, per tab, in the order seen: streams'
// manifests (HLS .m3u8, DASH .mpd; a master playlist comes before its variants) and plain video
// and sound files (a <video>'s or <audio>'s own file, or a large whole file a player fetched). The
// download button over a video, and the right-click menu on one, send the stream its frame loaded
// first (`streamFor`): a player that plays from a blob: has no address of its own, and yt-dlp
// reads the manifest (qualities, separate audio). "Download all videos and audio with DDM" adds
// the files the tab played (`playedFiles`). Segments and byte ranges are pieces of a stream, never
// kept. Bounded: MEDIA_MAX per tab, MEDIA_TABS tabs; forgotten when the tab goes to another page
// or closes.
const media = new Map(); // tab id -> {page, items: [{url, kind, frameId, mime, size}], maybe}
const MEDIA_MAX = 60;
const MEDIA_TABS = 50;
const URL_MAX = 8192;
// A player's own fetch of a whole file is listed from this size (smaller ones are pieces).
const FETCHED_MIN = 1 << 20;
// Less than this of a single file, played by the page itself: a page's sound effect (mediaKind).
const SOUND_MIN = 64 << 10;
const MANIFEST = /\.(m3u8|mpd)(?:[?#]|$)/i;
const SEGMENT_TYPE = /^(video\/mp2t|video\/iso\.segment|audio\/iso\.segment)$/;
const SEGMENT_EXT = /\.(ts|m4s|m4f|cmfv|cmfa|cmft)$/;
const MEDIA_EXT = /\.(mp4|m4v|webm|mkv|mov|avi|wmv|flv|f4v|3gp|3g2|ogv|mpg|mpeg|mp3|m4a|m4b|aac|ogg|oga|opus|flac|wav|wma|weba|mka|aif|aiff|amr|alac)$/;
const AUDIO_EXT = /^(mp3|m4a|m4b|aac|oga|ogg|opus|flac|wav|wma|weba|mka|aif|aiff|amr|alac)$/;
// The types a manifest is sometimes served as, besides its own (#171): plain text, no type, a
// file's, XML. Such an answer is only noted (`maybe`, MAYBE_MAX a tab), never listed unread.
const MAYBE_TYPE = /^(text\/plain|(application|binary)\/octet-stream|(application|text)\/xml|)$/;
const MAYBE_MAX = 20;
// A manifest's own types: HLS's (Apple's and the x- ones), DASH's (and its older video/ name).
const MANIFEST_TYPE = /mpegurl|dash\+xml|dash\.mpd/;
const MANIFEST_MAX = 2 << 20;

const pathOf = (url) => url.split(/[?#]/)[0];
const headerOf = (d, name) => ((d.responseHeaders || []).find((h) => h.name.toLowerCase() === name) || {}).value || "";

// A page that moves in place (history.pushState: YouTube, X, TikTok, SoundCloud) keeps what it
// loaded: its player may play on what came before (X's post opened from its feed plays the stream
// the feed loaded). Each move counts (`nav`); what was seen before it counts only where it is tied
// to the player (its poster's id, its length), never as this page's own, and goes after NAVS_KEPT
// more moves. A new document (its page's HTML loaded in the tab) starts again from nothing.
const NAVS_KEPT = 3;
const navOf = (tab) => (tab && tab.nav) || 0;
// Seen on the page as it is now (since its last move).
const isHere = (tab, it) => (it.nav || 0) === navOf(tab);

// What a response the tab got is, for the list: {kind: "hls" | "dash" | "video" | "audio", mime,
// size}, or null (not media, a segment, a piece of a file, too small to be a file of its own).
function mediaKind(d) {
  if (d.tabId < 0 || !/^https?:/i.test(d.url) || d.url.length > URL_MAX) return null;
  if (d.statusCode && (d.statusCode < 200 || d.statusCode > 299)) return null;
  const mime = headerOf(d, "content-type").split(";")[0].trim().toLowerCase();
  const path = pathOf(d.url).toLowerCase();
  // The address says manifest, the type says it isn't: a page or JSON that names one (SVT Play's
  // switcher answers .../dash-full.mpd with JSON naming the real manifest; loaded first, it went
  // to the app and yt-dlp found no video in it, #377). Read before believed, like any `maybe`.
  if (/\.(m3u8|mpd)$/.test(path) && !MANIFEST_TYPE.test(mime) && !MAYBE_TYPE.test(mime)) {
    return Number(headerOf(d, "content-length")) > MANIFEST_MAX ? null : { kind: "maybe", mime, size: 0 };
  }
  if (/\.m3u8$/.test(path) || /mpegurl/.test(mime)) return { kind: "hls", mime, size: 0 };
  if (/\.mpd$/.test(path) || /dash\+xml/.test(mime)) return { kind: "dash", mime, size: 0 };
  if (SEGMENT_TYPE.test(mime) || SEGMENT_EXT.test(path)) return null;
  const ext = (MEDIA_EXT.exec(path) || [])[1] || "";
  // Maybe a manifest whose address and type don't say so (#171): a player's own small text or
  // untyped answer, read only when a stream is asked for and none is known (`sniffTab`).
  if (d.type !== "media" && !ext && MAYBE_TYPE.test(mime) && d.statusCode !== 206 && !headerOf(d, "content-range")) {
    const len = Number(headerOf(d, "content-length"));
    return len > MANIFEST_MAX ? null : { kind: "maybe", mime, size: 0 };
  }
  // A page's own short sound (a click, a voice-search chime: 6 kB on YouTube in Edge), not media
  // anyone came for: the list would show nothing else on a page whose video is streamed.
  const whole = Number(headerOf(d, "content-length"));
  if (d.type === "media" && whole > 0 && whole < SOUND_MIN) return null;
  const typed = /^(video|audio)\//.test(mime) || mime === "application/ogg";
  const untyped = !mime || /^(application|binary)\/octet-stream$/.test(mime);
  // A media element's own answer is media whatever its address and however little its server says
  // (`<audio src="/play?id=5">` served as a file, or with no type): `typeless`, a sound or a picture
  // as the element that plays it says (videoFiles takes it for either).
  const element = d.type === "media" && untyped && !ext;
  if (!typed && !(ext && untyped) && !element) return null;
  // The whole file's size: a range's total, else the length of a whole answer.
  const range = headerOf(d, "content-range");
  const total = /\/\s*(\d+)\s*$/.exec(range);
  const size = total ? Number(total[1]) : d.statusCode === 206 ? 0 : Number(headerOf(d, "content-length")) || 0;
  // A stream with no end (a radio station's, from Icecast or SHOUTcast: no length, its station
  // named in icy-name): kept with that name, which names its file.
  const station = headerOf(d, "icy-name") || (headerOf(d, "icy-br") || headerOf(d, "icy-metaint") ? " " : "");
  const live = Boolean(station) && !range && !whole;
  const name = live ? String(station).replace(/[\u0000-\u001f\u007f]+/g, " ").replace(/\s+/g, " ").trim().slice(0, 100) : "";
  // A <video>'s or <audio>'s own file comes in ranges. A player's fetch of a small file, or of a
  // piece its address names (?bytestart=), is a piece of a stream. Its fetch of a range of a large
  // file (the Range in its header, the whole file's size in the answer's) is of that file:
  // TikTok's player fetches its MP4 so, 45 kB at a time, remuxing it for a MediaSource. Such a
  // file is `ranged`: a DASH stream's own files come the same way (a picture without its sound),
  // so it is offered only once its header says it is the clicked player's whole video
  // (videoFiles), never listed unread.
  if (d.type !== "media" && (/[?&](range|bytestart|byterange)=/i.test(d.url) || size < FETCHED_MIN)) return null;
  const audio = mime.startsWith("audio/") || (!mime.startsWith("video/") && AUDIO_EXT.test(ext));
  const ranged = d.type !== "media" && (d.statusCode === 206 || Boolean(range));
  return {
    kind: audio ? "audio" : "video",
    mime: element ? "" : mime,
    size,
    ...(ranged ? { ranged: true } : {}),
    ...(element && !audio ? { typeless: true } : {}),
    ...(live ? { live: true } : {}),
    ...(name ? { name } : {}),
  };
}

const isStream = (it) => it.kind === "hls" || it.kind === "dash";
// The folder a manifest's pieces are in ("https://cdn/v/"), or null for a site's root (every
// file of the site would be its piece).
function folderOf(url) {
  const p = pathOf(url);
  const dir = p.slice(0, p.lastIndexOf("/") + 1);
  return /^https?:\/\/[^/]+\/.+/i.test(dir) ? dir : null;
}

function noteMedia(d, found) {
  const tab = media.get(d.tabId) || { page: null, items: [], nav: 0 };
  const now = Date.now();
  // The same file again (a player's next range of it: TikTok's 45 kB at a time, a <video>'s
  // seeks): one file, seen once more, and of this page now (one that moved in place, below).
  const old = tab.items.find((it) => it.url === d.url);
  if (old) {
    if (found.size && !old.size) old.size = found.size;
    old.seen = now;
    old.hits = (old.hits || 1) + 1;
    old.nav = navOf(tab);
    return;
  }
  // A file in a stream's own folder is one of its pieces (a DASH init segment, a fragment).
  const inStream = (it) => isStream(it) && folderOf(it.url) && d.url.startsWith(folderOf(it.url));
  if (!isStream(found) && tab.items.some(inStream)) return;
  // A playlist in a folder under a stream's own, with the same video id, is its variant (X:
  // …/amplify_video/<id>/pl/<token>.m3u8, then …/pl/avc1/480x270/….m3u8 and …/pl/mp4a/…): not
  // noted apart, or a feed's many would push out its masters.
  const variantOf = (it) => {
    const [top, mine] = [folderOf(it.url), folderOf(d.url)];
    return isStream(it) && top && mine && mine !== top && mine.startsWith(top) && [...idsIn(it.url)].some((id) => idsIn(d.url).has(id));
  };
  if (isStream(found) && tab.items.some(variantOf)) return;
  tab.items.push({
    url: d.url,
    kind: found.kind,
    frameId: d.frameId || 0,
    mime: found.mime,
    size: found.size,
    ...(found.ranged ? { ranged: true } : {}),
    ...(found.typeless ? { typeless: true } : {}),
    ...(found.live ? { live: true } : {}),
    ...(found.name ? { name: found.name } : {}),
    ...(found.drm ? { drm: true } : {}),
    // When it was first and last seen, and on which of the page's places (`nav`).
    at: now,
    seen: now,
    hits: 1,
    nav: navOf(tab),
  });
  // Over the cap: the oldest go, but each frame's first stream stays (the button sends it).
  while (tab.items.length > MEDIA_MAX) {
    const firsts = new Set();
    const i = tab.items.findIndex((it) => {
      if (!isStream(it) || firsts.has(it.frameId)) return true;
      firsts.add(it.frameId);
      return false;
    });
    tab.items.splice(Math.max(i, 0), 1);
  }
  // The tab seen last goes last; the least recent ones go past MEDIA_TABS.
  media.delete(d.tabId);
  media.set(d.tabId, tab);
  for (const id of media.keys()) {
    if (media.size <= MEDIA_TABS) break;
    media.delete(id);
  }
  keepMedia();
}

// A response that may be a manifest (#171), noted apart: the newest MAYBE_MAX of the tab.
function noteMaybe(d, found) {
  const tab = media.get(d.tabId) || { page: null, items: [] };
  if (tab.items.some((it) => it.url === d.url)) return;
  tab.maybe = (tab.maybe || []).filter((m) => m.url !== d.url);
  tab.maybe.push({ url: d.url, frameId: d.frameId || 0, mime: found.mime });
  if (tab.maybe.length > MAYBE_MAX) tab.maybe.splice(0, tab.maybe.length - MAYBE_MAX);
  if (!media.has(d.tabId)) {
    media.set(d.tabId, tab);
    for (const id of media.keys()) {
      if (media.size <= MEDIA_TABS) break;
      media.delete(id);
    }
  }
  keepMedia();
}

// What a text starts as: an HLS playlist ("hls"), a DASH manifest ("dash"), or neither ("").
function manifestKind(text) {
  const t = String(text).replace(/^﻿/, "").trimStart();
  if (t.startsWith("#EXTM3U")) return "hls";
  // An XML declaration, comments, then the MPD element (with a namespace prefix or not).
  if (/^(?:<\?xml[^>]*\?>\s*)?(?:<!--[\s\S]*?-->\s*)*<(?:[\w-]+:)?MPD[\s>/]/.test(t)) return "dash";
  return "";
}

// The start of a response (SNIFF_BYTES), fetched again with the site's cookies as `probe` does:
// what it is (`manifestKind`; with `drm` when that start already says it is copy-protected), or
// {kind: ""} (not a manifest, refused, too slow).
const SNIFF_BYTES = 4096;
async function sniff(url) {
  let reader = null;
  const seen = (text) => {
    const kind = manifestKind(text);
    return { kind, ...(kind && manifestProtected(text) ? { drm: true } : {}) };
  };
  try {
    const r = await withTimeout(fetch(url, { credentials: "include" }), 5000);
    if (!r.ok) return { kind: "" };
    if (!r.body || !r.body.getReader) return seen((await withTimeout(r.text(), 5000)).slice(0, SNIFF_BYTES));
    reader = r.body.getReader();
    const dec = new TextDecoder();
    let text = "";
    while (text.length < SNIFF_BYTES) {
      const { done, value } = await withTimeout(reader.read(), 5000);
      if (done) break;
      text += dec.decode(value, { stream: true });
    }
    return seen(text);
  } catch {
    return { kind: "" };
  } finally {
    // The rest is never read.
    if (reader) reader.cancel().catch(() => {});
  }
}

// The tab's possible manifests read, those of `frames` only (a Set of frame ids, or null: all),
// each once: the ones that are become its streams, in the order they were loaded. One reading at
// a time per tab: a second ask waits for the first.
const sniffing = new Map();
function sniffTab(tabId, frames) {
  const run = async () => {
    const tab = media.get(tabId);
    if (!tab || !tab.maybe || !tab.maybe.length) return;
    const asked = tab.maybe.filter((m) => !frames || frames.has(m.frameId));
    if (!asked.length) return;
    tab.maybe = tab.maybe.filter((m) => !asked.includes(m));
    const kinds = await Promise.all(asked.map((m) => sniff(m.url)));
    // The tab went to another page meanwhile: its old page's answers don't count.
    if (media.get(tabId) !== tab) return;
    asked.forEach((m, i) => {
      const k = kinds[i];
      if (k.kind) noteMedia({ tabId, url: m.url, frameId: m.frameId }, { kind: k.kind, mime: m.mime, size: 0, ...(k.drm ? { drm: true } : {}) });
    });
    keepMedia();
  };
  const next = (sniffing.get(tabId) || Promise.resolve()).then(run, run);
  sniffing.set(tabId, next);
  next.finally(() => sniffing.get(tabId) === next && sniffing.delete(tabId)).catch(() => {});
  return next;
}

if (ext.webRequest && ext.webRequest.onResponseStarted) {
  ext.webRequest.onResponseStarted.addListener(
    (d) => {
      if (d.type === "main_frame" || d.type === "sub_frame") {
        const type = (d.responseHeaders || []).find((h) => h.name.toLowerCase() === "content-type");
        if (d.statusCode === 200 && type && /^\s*(text\/html|application\/xhtml)/i.test(type.value || "")) {
          forget(loadedPages, 120000);
          loadedPages.set(d.url, { at: Date.now() });
          // A new document in the tab: what the last one loaded goes.
          if (d.type === "main_frame" && d.tabId >= 0) mediaBack.then(() => newDocument(d.tabId, d.url));
        }
        return;
      }
      const found = mediaKind(d);
      if (found) mediaBack.then(() => (found.kind === "maybe" ? noteMaybe(d, found) : noteMedia(d, found)));
    },
    { urls: ["<all_urls>"], types: ["main_frame", "sub_frame", "xmlhttprequest", "media", "other"] },
    ["responseHeaders"],
  );
}
ext.tabs.onRemoved.addListener((id) => mediaBack.then(() => media.delete(id) && keepMedia()));

// The tab loaded a new document at `url`: nothing of the last one's is kept.
function newDocument(tabId, url) {
  media.set(tabId, { page: url.split("#")[0], items: [], nav: 0 });
  keepMedia();
}

// Another address in the tab (not a move to a part of the page, #part). With no new document
// loaded for it, the page moved in place: what it loaded stays, as before the move (NAVS_KEPT).
ext.tabs.onUpdated.addListener((id, change) => {
  if (!change.url) return;
  mediaBack.then(() => {
    const page = change.url.split("#")[0];
    const tab = media.get(id);
    if (tab && tab.page === page) return;
    if (!tab) {
      media.set(id, { page, items: [], nav: 0 });
      return keepMedia();
    }
    tab.page = page;
    tab.nav = navOf(tab) + 1;
    tab.items = tab.items.filter((it) => (it.nav || 0) >= tab.nav - NAVS_KEPT);
    tab.maybe = [];
    delete tab.drm;
    keepMedia();
  });
});

// The tab started or stopped making sound (the browser's speaker icon on it): video.js is told in
// the frames whose players loaded media on this page, the most recent first (else the top frame),
// so a player with nothing to point at (a music site's `new Audio()`, a hidden <audio>, sound
// a page decodes itself) gets the button on its bar (video.js `heard`). `media`: that frame loaded
// something to offer.
const AUDIBLE_FRAMES = 3;
function tellAudible(tabId, on) {
  const tab = media.get(tabId);
  const frames = new Map();
  for (const it of (tab && tab.items) || []) {
    if (isHere(tab, it)) frames.set(it.frameId, Math.max(frames.get(it.frameId) || 0, it.seen || 0));
  }
  const order = [...frames].sort((a, b) => b[1] - a[1]).map(([f]) => f).slice(0, AUDIBLE_FRAMES);
  if (!order.includes(0)) order.push(0);
  for (const frameId of order) {
    try {
      Promise.resolve(ext.tabs.sendMessage(tabId, { type: "ddm-audible", on, media: frames.has(frameId) }, { frameId })).catch(() => {});
    } catch {
      // No script of ours in that frame (a page extensions may not read).
    }
  }
}
ext.tabs.onUpdated.addListener((id, change) => {
  if (typeof change.audible !== "boolean" || !ext.tabs.sendMessage) return;
  mediaBack.then(() => tellAudible(id, change.audible));
});

// Kept in the session's storage (memory only, gone with the browser): the browser stops this
// script when idle, and a tab's player loaded its stream long before it is asked for. Read back
// once at start, merged with what was seen meanwhile; everything else waits for it.
const session = ext.storage.session;
let keeping = null;
function keepMedia() {
  if (!session || keeping) return;
  keeping = setTimeout(() => {
    keeping = null;
    session.set({ media: [...media] }).catch(() => {});
  }, 200);
}
const mediaBack = session
  ? session
      .get({ media: [] })
      .then((v) => {
        for (const [id, kept] of v.media || []) {
          const now = media.get(id);
          const fresh = ((now && now.items) || []).filter((s) => !kept.items.some((o) => o.url === s.url));
          const maybe = [...(kept.maybe || []), ...((now && now.maybe) || [])].filter((m, i, all) => all.findIndex((o) => o.url === m.url) === i);
          media.set(id, { ...kept, page: (now && now.page) || kept.page, items: [...kept.items, ...fresh].slice(-MEDIA_MAX), maybe: maybe.slice(-MAYBE_MAX) });
        }
      })
      .catch(() => {})
  : Promise.resolve();

// The long numbers (10 digits or more) in an address's path: a video's id, which its player's
// poster and its stream often both carry (X: pbs.twimg.com/ext_tw_video_thumb/<id>/pu/img/… and
// video.twimg.com/ext_tw_video/<id>/pu/pl/….m3u8).
function idsIn(url) {
  try {
    return new Set(new URL(url).pathname.match(/\d{10,}/g) || []);
  } catch {
    return new Set();
  }
}

// Of `list` (streams, in the order loaded), the one whose address shares an id with the player's
// `poster`, or null: the master playlist (highest in its folders: its variants sit below it), the
// first loaded of equals. Never the variant the player plays (it starts at the lowest).
function posterStream(list, poster) {
  const ids = idsIn(poster);
  if (!ids.size) return null;
  const depth = (s) => pathOf(s.url).split("/").length;
  return list.filter((s) => [...idsIn(s.url)].some((id) => ids.has(id))).reduce((best, s) => (!best || depth(s) < depth(best) ? s : best), null);
}

// The stream a frame's player loaded first (its master playlist), else the tab's: of the page as
// it is now (a stream seen before it moved in place is another video's, unless tied: streamPlayed).
// Never one known to be copy-protected.
function streamOf(sender) {
  const tab = (sender.tab && media.get(sender.tab.id)) || { items: [] };
  const list = tab.items.filter((it) => isStream(it) && isHere(tab, it) && !it.drm);
  const mine = list.find((s) => s.frameId === sender.frameId);
  return (mine || list[0] || {}).url || "";
}

// A stream's length in seconds, as its manifest says (fetched again as `probe` does, kept): a DASH
// manifest's duration; an HLS master playlist's first variant's, or a media playlist's, pieces
// added up. 0 when it doesn't say (a live stream, a refusal, 1.5 s gone).
const lengths = new Map();
const LENGTHS_MAX = 200;
async function streamLength(item) {
  if (lengths.has(item.url)) return lengths.get(item.url);
  const text = async (url) => {
    const r = await withTimeout(fetch(url, { credentials: "include" }), 1500);
    if (!r.ok || Number(r.headers.get("content-length")) > MANIFEST_MAX) return "";
    const t = await withTimeout(r.text(), 1500);
    return t.length > MANIFEST_MAX ? "" : t;
  };
  let seconds = 0;
  try {
    let t = await text(item.url);
    if (manifestProtected(t)) item.drm = true;
    if (item.kind === "dash") {
      const mpd = (/<(?:\w+:)?MPD\b[^>]*>/.exec(t) || [""])[0];
      if (!/type\s*=\s*["']dynamic/.test(mpd)) seconds = isoSeconds((/mediaPresentationDuration\s*=\s*["']([^"']*)/.exec(mpd) || [])[1]);
    } else {
      const lines = t.split(/\r?\n/).map((l) => l.trim());
      const at = lines.findIndex((l) => l.startsWith("#EXT-X-STREAM-INF:"));
      const variant = at >= 0 ? lines.slice(at + 1).find((l) => l && !l.startsWith("#")) : "";
      if (variant) t = await text(new URL(variant, item.url).href);
      // A variant's own keys say it too (a master names none of them).
      if (manifestProtected(t)) item.drm = true;
      if (/#EXT-X-ENDLIST/.test(t)) seconds = [...t.matchAll(/#EXTINF:\s*([\d.]+)/g)].reduce((n, m) => n + Number(m[1]), 0);
    }
  } catch {
    seconds = 0;
  }
  lengths.set(item.url, seconds);
  for (const k of lengths.keys()) {
    if (lengths.size <= LENGTHS_MAX) break;
    lengths.delete(k);
  }
  return seconds;
}

// The stream the player of `want` plays, of the tab's (its frame's first): the one whose address
// shares an id with its `poster` (X: a post's and a promoted one under it, in no set order), else,
// of several (`strict`: of any), the one alone as long as the player (`duration`, half a second
// or 1%), else (not `strict`) the frame's first. `strict` (a video in a feed or a post: the feed's
// first stream may be another video's): "" rather than a guess.
async function streamPlayed(sender, { poster = "", duration = 0, strict = false }) {
  const tab = (sender.tab && media.get(sender.tab.id)) || { items: [] };
  const every = tab.items.filter(isStream);
  // This page's, and those seen before it moved in place (its player may play on with one of
  // them): those only by its poster or its length, never as the frame's first.
  const list = every.filter((s) => isHere(tab, s));
  const before = every.filter((s) => !isHere(tab, s) && s.frameId === sender.frameId);
  const own = list.filter((s) => s.frameId === sender.frameId);
  const named = posterStream(own, poster) || posterStream(list, poster) || posterStream(before, poster);
  if (named) return named.drm ? "" : named.url;
  const want = positive(duration);
  const fit = async (among) => {
    const seconds = await Promise.all(among.map(streamLength));
    const fits = among.filter((s, i) => Math.abs(seconds[i] - want) <= Math.max(0.5, want * 0.01));
    return fits.length === 1 ? fits[0] : null;
  };
  const among = (own.length ? own : list).slice(-8);
  if (want && (strict ? among.length : among.length > 1)) {
    const one = await fit(among);
    if (one) return one.drm ? "" : one.url;
  }
  if (want && !among.length && before.length) {
    const one = await fit(before.slice(-8));
    if (one) return one.drm ? "" : one.url;
  }
  const first = own[0] || list[0];
  return strict || !first || first.drm ? "" : first.url;
}

// The same for a player that plays from a blob: (asked by the button over it or the right-click
// menu), reading the tab's possible manifests when no stream is known (#171): the frame's own
// first, then the tab's other frames'. With `want` (the button's: the player's poster and
// length; `strict` in a feed), the stream that player plays (streamPlayed), and in a feed none is
// read: only what the tab already knows as streams counts.
async function streamFor(sender, want = null) {
  const tabId = sender.tab && sender.tab.id;
  if (want && want.strict) return streamPlayed(sender, want);
  if (tabId === undefined || tabId === null) return streamOf(sender);
  const frameId = sender.frameId || 0;
  const own = () => {
    const tab = media.get(tabId) || { items: [] };
    return tab.items.some((it) => isStream(it) && it.frameId === frameId && isHere(tab, it));
  };
  if (!own()) await sniffTab(tabId, new Set([frameId]));
  if (!own() && !streamOf(sender)) await sniffTab(tabId, null);
  return want ? streamPlayed(sender, want) : streamOf(sender);
}

// ---- a stream's manifest: what its tags say (the qualities the page names, below, read them) ----

// An HLS tag's attributes (KEY=value,KEY="value").
function hlsAttrs(s) {
  const out = {};
  for (const [, k, v] of s.matchAll(/([A-Z0-9-]+)=("[^"]*"|[^,]*)/g)) out[k] = v.replace(/^"|"$/g, "");
  return out;
}

// A copy-protected (DRM) key: a key system's (Apple's FairPlay, Google's Widevine, Microsoft's
// PlayReady), or SAMPLE-AES, which no download tool decrypts. AES-128 with a plain key
// ("identity") is not DRM: yt-dlp fetches its key as the player does.
function drmKey(a) {
  const method = (a.METHOD || "").toUpperCase();
  if (!method || method === "NONE") return false;
  return method.startsWith("SAMPLE-AES") || (a.KEYFORMAT !== undefined && a.KEYFORMAT !== "identity");
}

// Is a manifest's text a copy-protected stream's? An HLS playlist with a DRM key (drmKey), a DASH
// manifest with ContentProtection (Widevine, PlayReady, FairPlay, or CENC's own scheme: the
// pieces are encrypted). No tool may break that, and DDM doesn't try (#drm).
function manifestProtected(text) {
  const s = String(text || "");
  if (/<(?:\w+:)?ContentProtection\b/.test(s)) return true;
  for (const [, attrs] of s.matchAll(/^\s*#EXT-X-(?:SESSION-)?KEY:(.*)$/gm)) if (drmKey(hlsAttrs(attrs.trim()))) return true;
  return false;
}

// Seconds of an ISO 8601 duration (PT1H2M3.5S).
function isoSeconds(d) {
  const m = /P(?:(\d+)D)?T?(?:(\d+)H)?(?:(\d+)M)?(?:([\d.]+)S)?/.exec(d || "") || [];
  return (+m[1] || 0) * 86400 + (+m[2] || 0) * 3600 + (+m[3] || 0) * 60 + (+m[4] || 0);
}
