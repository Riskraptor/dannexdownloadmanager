// The video lookup: the app looks the video up, the page lists the qualities it names meanwhile,
// and lookups asked ahead. A part of the background: background.js loads the parts in order, into
// one scope.

// ---- the button's qualities: the app looks the video up, the page lists what it found --------
// The click asks the app to look the video up (yt-dlp, in the background), video.js asks how it
// is going until it is done, and the quality picked is downloaded without the app's window. Only
// with an app that says it can (`videoLookup`, noteLookup) and never for another DDM: else
// ({fallback: true}) the click opens the app's Download video window, as it always did. None of
// these messages has `video` or a top-level `url`: an older host refuses them and does nothing.
// What a lookup comes to travels in `lookup`, never in `why`: the toolbar button's status (and
// the rule for PDFs and music that follows it) is the host's, not a video's.

// ---- the qualities the page itself names, shown while the app looks -------------------------------
// yt-dlp takes seconds to look a video up; the page that plays it often says its qualities already:
// YouTube's player (its player response's formats), another site's HLS or DASH manifest the tab
// loaded. Read into yt-dlp's terms ({duration, formats: [{height, fps, video, audio, ext, size, tbr,
// hls}]}) and sent with the lookup's answer as `page`: video.js lists them as the app will
// (pageChoices) until the app's own list takes their place, row for row. Only shown: a choice
// still goes to the app as a place in its own list, downloaded with its own selector, so a page
// that says something else only shows something else for a moment. Anything missing or unusual
// (a live stream, a protected one, a video without its height): nothing, and the menu waits for
// the app as before.

// A positive number, or null.
const positive = (x) => {
  const n = Number(x);
  return Number.isFinite(n) && n > 0 ? n : null;
};

// yt-dlp's file type for a MIME type, as it names them (mimetype2ext).
const MIME_EXT = { "video/mp4": "mp4", "video/webm": "webm", "video/3gpp": "3gp", "audio/mp4": "m4a", "audio/webm": "webm", "audio/mpeg": "mp3" };

// The codecs of a CODECS list: [video, audio] (yt-dlp's parse_codecs, in short), null when none is
// known.
function codecKinds(list) {
  let video = false;
  let audio = false;
  let known = false;
  for (const c of String(list || "").split(",").map((s) => s.trim().toLowerCase().split(".")[0])) {
    if (/^(avc[1-4]|h26[345]|hvc1|hev1|dvh[1e]|vp0?[89]|vp09|av01|theora|mp4v)$/.test(c)) [video, known] = [true, true];
    else if (/^(mp4a|opus|vorbis|flac|ac-3|ec-3|eac3|mp3|alac|dtsc|aac)$/.test(c)) [audio, known] = [true, true];
  }
  return known ? [video, audio] : null;
}

// The video and sound codecs a CODECS list names, as yt-dlp's parse_codecs splits them ({vcodec,
// acodec}, each "" when unnamed): what the menu ranks a height's formats by, as yt-dlp does.
function codecNames(list) {
  let vcodec = "";
  let acodec = "";
  for (const c of String(list || "").split(",").map((s) => s.trim().toLowerCase())) {
    const base = c.split(".")[0];
    if (!vcodec && /^(avc[1-4]|h26[345]|hvc1|hev1|dvh[1e]|vp0?[89]|vp09|av01|theora|mp4v)$/.test(base)) vcodec = c.slice(0, 40);
    else if (!acodec && /^(mp4a|opus|vorbis|flac|ac-3|ec-3|eac3|mp3|alac|dtsc|aac)$/.test(base)) acodec = c.slice(0, 40);
  }
  return { vcodec, acodec };
}

// YouTube's words for a sound's quality, as yt-dlp's YouTube reader ranks them (its `quality`).
const AUDIO_QUALITY = { audio_quality_ultralow: 1, audio_quality_low: 2, audio_quality_medium: 3, audio_quality_high: 4 };

// YouTube's player response, as playingAt read it in the page, for the video `id`: its formats in
// yt-dlp's terms (its YouTube reader's: the type from the MIME type, the height, the frame rate,
// the size from contentLength, else from the bitrate and the format's length). Null for another
// video, one that doesn't play (signed-in only, unavailable), a live or upcoming one, a protected
// format, or a video format without its height.
function ytFormats(p, id) {
  if (!p || typeof p !== "object" || !id || p.id !== id || p.status !== "OK" || p.live) return null;
  const formats = [];
  for (const f of Array.isArray(p.formats) ? p.formats.slice(0, 300) : []) {
    if (!f || typeof f !== "object" || f.drm || f.live) return null;
    const m = /^\s*(video|audio)\/([\w.+-]+)\s*(?:;\s*codecs\s*=\s*"([^"]*)")?/i.exec(String(f.mimeType || ""));
    const ext = m && MIME_EXT[`${m[1]}/${m[2]}`.toLowerCase()];
    if (!ext) continue;
    const kinds = codecKinds(m[3]);
    const video = m[1].toLowerCase() === "video";
    const audio = !video || Boolean(kinds && kinds[1]);
    const height = video ? positive(f.height) : null;
    if (video && !height) return null;
    const rate = positive(f.averageBitrate) || positive(f.bitrate);
    const tbr = rate ? rate / 1000 : null;
    const length = positive(f.approxDurationMs);
    const bytes = positive(f.contentLength);
    const size = bytes ? Math.floor(bytes) : tbr && length ? Math.floor((length / 1000) * tbr * 125) : null;
    const { vcodec, acodec } = codecNames(m[3]);
    // How yt-dlp ranks the sound (its YouTube reader): the original or default track first, a
    // DRC copy half a quality below the plain one.
    const aq = AUDIO_QUALITY[String(f.audioQuality || "").toLowerCase()] || 0;
    formats.push({
      height: height && Math.round(height),
      fps: video ? positive(f.fps) : null,
      video,
      audio,
      ext,
      size,
      tbr,
      hls: false,
      vcodec: video ? vcodec : "",
      acodec: audio ? acodec : "",
      quality: video ? 0 : aq - (f.drc ? 0.5 : 0),
      channels: audio ? positive(f.audioChannels) : null,
      lang: f.original ? 2 : f.defaultTrack === false ? -1 : f.defaultTrack ? 1 : 0,
    });
  }
  if (!formats.some((f) => f.video)) return null;
  return { duration: positive(p.length), formats };
}

// An HLS master playlist's variants in yt-dlp's terms (its m3u8 reader's: the height of their
// RESOLUTION, FRAME-RATE, the bitrate of AVERAGE-BANDWIDTH or BANDWIDTH, the codecs; a variant whose
// sound is a rendition of its own has none, and the renditions are sound alone), `duration` the
// player's length. Null for anything else: a media playlist, a protected stream, a variant
// without its height.
function hlsFormats(text, duration) {
  const lines = String(text || "").replace(/^﻿/, "").split(/\r?\n/).map((l) => l.trim());
  if (!lines[0] || !lines[0].startsWith("#EXTM3U")) return null;
  const audioGroups = new Set();
  const formats = [];
  for (const l of lines) {
    const a = hlsAttrs(l.slice(l.indexOf(":") + 1));
    if ((l.startsWith("#EXT-X-KEY:") || l.startsWith("#EXT-X-SESSION-KEY:")) && drmKey(a)) return null;
    if (l.startsWith("#EXT-X-MEDIA:") && (a.TYPE || "").toUpperCase() === "AUDIO" && a.URI) {
      audioGroups.add(a["GROUP-ID"]);
      formats.push({ height: null, fps: null, video: false, audio: true, ext: "mp4", size: null, tbr: null, hls: true });
    }
  }
  const length = positive(duration);
  for (const l of lines) {
    if (!l.startsWith("#EXT-X-STREAM-INF:")) continue;
    const a = hlsAttrs(l.slice(l.indexOf(":") + 1));
    const height = positive((/x(\d+)$/i.exec(a.RESOLUTION || "") || [])[1]);
    if (!height) return null;
    const rate = positive(a["AVERAGE-BANDWIDTH"]) || positive(a.BANDWIDTH);
    const tbr = rate ? rate / 1000 : null;
    const kinds = codecKinds(a.CODECS);
    let audio = kinds ? kinds[1] : true;
    if (kinds && kinds[0] && a.AUDIO && audioGroups.has(a.AUDIO)) audio = false;
    formats.push({ height, fps: positive(a["FRAME-RATE"]), video: kinds ? kinds[0] : true, audio, ext: "mp4", size: tbr && length ? Math.floor(length * tbr * 125) : null, tbr, hls: true, ...codecNames(a.CODECS) });
  }
  if (!formats.some((f) => f.video)) return null;
  return { duration: length, formats };
}

// A DASH manifest's representations in yt-dlp's terms (its MPD reader's: each one's type, height,
// frame rate and bandwidth, its adaptation set's where it says none), read as text (a service
// worker has no XML parser). Null for a live or protected manifest, or a video without its height.
function dashFormats(text) {
  const s = String(text || "");
  if (!/<(?:\w+:)?MPD[\s>]/.test(s) || /<(?:\w+:)?ContentProtection\b/.test(s)) return null;
  const attr = (tag, name) => (new RegExp(`\\s${name}\\s*=\\s*["']([^"']*)["']`).exec(tag) || [])[1];
  const mpd = (/<(?:\w+:)?MPD\b[^>]*>/.exec(s) || [""])[0];
  if (attr(mpd, "type") === "dynamic") return null;
  const duration = positive(isoSeconds(attr(mpd, "mediaPresentationDuration")));
  const formats = [];
  for (const [, setTag, body] of s.matchAll(/(<(?:\w+:)?AdaptationSet\b[^>]*>)([\s\S]*?)<\/(?:\w+:)?AdaptationSet>/g)) {
    for (const [rep] of body.matchAll(/<(?:\w+:)?Representation\b[^>]*>/g)) {
      const get = (name) => attr(rep, name) ?? attr(setTag, name);
      const mime = String(get("mimeType") || "").toLowerCase();
      const type = (/^(video|audio)\//.exec(mime) || [])[1] || String(get("contentType") || "").toLowerCase();
      if (type !== "video" && type !== "audio") continue;
      const ext = MIME_EXT[mime] || (type === "audio" ? "m4a" : "mp4");
      const height = type === "video" ? positive(get("height")) : null;
      if (type === "video" && !height) return null;
      const [num, den] = String(get("frameRate") || "").split("/").map(Number);
      const fps = positive(den ? num / den : num);
      const rate = positive(get("bandwidth"));
      const tbr = rate ? rate / 1000 : null;
      formats.push({ height, fps: type === "video" ? fps : null, video: type === "video", audio: type === "audio", ext, size: tbr && duration ? Math.floor(duration * tbr * 125) : null, tbr, hls: false, ...codecNames(get("codecs")) });
    }
  }
  if (!formats.some((f) => f.video)) return null;
  return { duration, formats };
}

// The qualities of the stream `src` the tab loaded (an HLS or DASH manifest it noted), fetched
// again as `probe` does, 1.5 s at most (the app's answer doesn't wait longer for them); null when
// it isn't one or doesn't say, or is copy-protected (then kept so, and never sent as the video's
// stream: #drm; a site may serve the same video clear another way, which yt-dlp finds).
// `duration`: the player's length (an HLS master playlist has none).
async function streamFormats(tabId, src, duration) {
  const item = ((media.get(tabId) || { items: [] }).items || []).find((it) => it.url === src && isStream(it));
  const kind = item ? item.kind : /\.m3u8(\?|$)/i.test(src) ? "hls" : /\.mpd(\?|$)/i.test(src) ? "dash" : "";
  if (kind !== "hls" && kind !== "dash") return null;
  if (item && item.drm) return null;
  try {
    const r = await withTimeout(fetch(src, { credentials: "include" }), 1500);
    if (!r.ok || Number(r.headers.get("content-length")) > MANIFEST_MAX) return null;
    const text = await withTimeout(r.text(), 1500);
    if (text.length > MANIFEST_MAX) return null;
    if (manifestProtected(text)) {
      if (item) item.drm = true;
      return null;
    }
    return kind === "hls" ? hlsFormats(text, duration) : dashFormats(text);
  } catch {
    return null;
  }
}

// ---- asking ahead -----------------------------------------------------------------------------
// video.js asks before a click (`prefetch`) when the pointer rests on the button, or over the video
// on that video's own page: the app's answer is then ready, or nearly, when the menu opens. Never
// from a private window. One at a time per tab: a new one for another video takes the old one's
// place, and one the tab left (another page, the tab closed) is called off (`videoCancel`: the app
// stops its yt-dlp while nothing else holds it). The app itself starts one only while no other
// lookup runs.
const ahead = new Map(); // tab id -> {token, key, url: the tab's address then}

// Calls off tab `tabId`'s lookup asked ahead, unless it is `keep`'s.
function cancelAhead(tabId, keep) {
  const a = ahead.get(tabId);
  if (!a || a.key === keep) return;
  ahead.delete(tabId);
  if (a.token) native({ videoCancel: { token: a.token } }).catch(() => {});
}
ext.tabs.onRemoved.addListener((id) => cancelAhead(id));
ext.tabs.onUpdated.addListener((id, change) => {
  const a = ahead.get(id);
  // Another page (a page that moves in place says so too); a fragment of the same one isn't.
  if (a && typeof change.url === "string" && change.url.split("#")[0] !== a.url.split("#")[0]) cancelAhead(id);
});

async function videoLookup(m, page, sender) {
  if (await remoteTarget()) return { fallback: true };
  const tab = sender.tab || {};
  const prefetch = m.prefetch === true;
  // Never asked ahead from a private window: only a click sends its page to the app there.
  if (prefetch && tab.incognito) return { skipped: true };
  if (!(await readStatus()).videoLookup) {
    // An DDM updated since its rules were last asked may look up now: they are asked again,
    // for the next click.
    const r = await storedRules();
    if (!r || Date.now() - r.at > 60000) refreshRules();
    return { fallback: true };
  }
  const seen = {};
  const v = await videoTarget(m, page, sender, seen);
  if (!v) return { ok: false, why: isYouTube(page) ? "unnamed" : "feed" };
  const key = `${v.own} ${v.src}`;
  if (prefetch) cancelAhead(tab.id, key);
  // The page's own word on the qualities, read while the app is asked.
  const pageSays = seen.id ? Promise.resolve(ytFormats(seen.player, seen.id)) : v.src ? streamFormats(tab.id, v.src, m.duration) : Promise.resolve(null);
  const asking = native({
    videoLookup: {
      page: v.own,
      ...(v.src ? { src: v.src } : {}),
      ...(v.title ? { title: v.title } : {}),
      ...(v.referrer ? { referrer: v.referrer } : {}),
      // The lookup is the window's and the container's it was asked from (the app gives its
      // pick to no other).
      ...(tab.cookieStoreId ? { storeId: tab.cookieStoreId } : {}),
      ...(prefetch ? { prefetch: true } : {}),
    },
    cookies: v.cookies,
    cookieJar: v.cookieJar,
    private: Boolean(tab.incognito),
  });
  const [reply, said] = await Promise.all([asking, pageSays.catch(() => null)]);
  if (reply.lookup && typeof reply.lookup === "object") {
    if (prefetch && lookupToken(reply.lookup.token) && tab.id !== undefined) ahead.set(tab.id, { token: reply.lookup.token, key, url: tab.url || page });
    return { lookup: reply.lookup, ...(said ? { page: said } : {}) };
  }
  // No `lookup`: a host or an app that can't look up after all (an older one refuses the message),
  // or a host that failed. Not kept as able; its rules are asked again, and this click opens
  // the window.
  await noteLookup(false);
  refreshRules();
  return { fallback: true };
}

// A lookup asked ahead that video.js no longer wants (its page or its video changed).
async function videoCancel(m, sender) {
  if (!lookupToken(m.token) || (await remoteTarget())) return { ok: true };
  const tab = sender.tab || {};
  const a = ahead.get(tab.id);
  if (a && a.token === m.token) ahead.delete(tab.id);
  await native({ videoCancel: { token: m.token } });
  return { ok: true };
}

// The token is the app's, from the lookup's answer; it means nothing to anyone else.
const lookupToken = (token) => typeof token === "string" && token.length > 0 && token.length <= 200;

async function videoPoll(m, sender) {
  if (await remoteTarget()) return { fallback: true };
  if (!lookupToken(m.token)) return { lookup: { state: "gone" } };
  // A lookup asked ahead that a click now follows is the click's: never called off for the tab.
  const tab = (sender && sender.tab) || {};
  const a = ahead.get(tab.id);
  if (a && a.token === m.token) ahead.delete(tab.id);
  const reply = await native({ videoPoll: { token: m.token } });
  return { lookup: reply.lookup && typeof reply.lookup === "object" ? reply.lookup : { state: "gone" } };
}

async function videoPick(m, sender) {
  if (await remoteTarget()) return { fallback: true };
  if (!lookupToken(m.token) || !Number.isInteger(m.choice) || m.choice < 0) return { fallback: true };
  const tab = sender.tab || {};
  // The token and the row's number from the page, nothing else: private or not and the
  // container are the tab's, as the browser says; the app checks them against the lookup's.
  const reply = await native({
    videoPick: { token: m.token, choice: m.choice, ...(tab.cookieStoreId ? { storeId: tab.cookieStoreId } : {}) },
    private: Boolean(tab.incognito),
  });
  return reply.lookup && typeof reply.lookup === "object" ? { lookup: reply.lookup } : { fallback: true };
}
