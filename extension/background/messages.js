// Messages from the page (video.js, page.js) and the extension's own pages. A part of the
// background: background.js loads the parts in order, into one scope.

// ---- messages from the page (video.js, page.js) and the extension's own pages ---------------

// The download button over videos asks: may it show on this page, the button was clicked, its
// menu. The app decides through the native host, from Options as saved now; its answer for a
// site is kept a few seconds (the page's frames ask together, and each page asks again when its
// answer is older: video.js ASK_MS), so a change in Options reaches the pages already open
// (#532: it was kept a minute, and by the page for good). A failure to reach it isn't kept.
// `top`: the tab's address, when the frame asking is another page's (a player embedded from
// another site): the button stays off there too when the tab's site is listed.
const PANEL_KEEP_MS = 5000;
const panelCache = new Map();
// Download panels (Options): over videos, over sound players, not on media shorter than
// `minSeconds` (0: any), its first corner. A host before them says none: as before.
const PANEL_CORNERS = ["top-right", "top-left", "bottom-left", "bottom-right"];
function panelLook(said) {
  const p = said && typeof said === "object" ? said : {};
  return {
    video: p.video !== false,
    audio: p.audio !== false,
    minSeconds: Number.isInteger(p.minSeconds) && p.minSeconds > 0 ? p.minSeconds : 0,
    corner: PANEL_CORNERS.includes(p.corner) ? p.corner : PANEL_CORNERS[0],
  };
}
// {on, video, audio, minSeconds, corner}.
async function panelOn(page, top) {
  // The host name without a port: as video.js names the site, and as the app matches sites.
  const site = new URL(page).hostname;
  const tabSite = top ? new URL(top).hostname : "";
  const key = tabSite && tabSite !== site ? `${site} ${tabSite}` : site;
  forget(panelCache, PANEL_KEEP_MS);
  if (panelCache.get(site)?.hidden) return { on: false };
  const kept = panelCache.get(key);
  if (kept) return kept.answer;
  const reply = await native({ panel: true, url: page, ...(key !== site ? { referrer: top } : {}) });
  const answer = reply.ok ? { on: true, ...panelLook(reply.panel) } : { on: false };
  if (reply.ok || !reply.why) panelCache.set(key, { answer, at: Date.now() });
  return answer;
}

// The page the message came from, as the browser knows it (never what the page says). The top
// frame's is its tab's address, which follows a page that moves in place (history.pushState:
// YouTube, most video sites). `sender.url` is the document's and goes stale there: in Chromium it
// stays the address the document loaded at, in Firefox it lags a navigation behind, so the button
// sent the video watched before this one.
function senderPage(sender) {
  const top = sender.frameId === 0 && sender.tab && sender.tab.url;
  const u = top || sender.url || (sender.tab && sender.tab.url) || "";
  return /^https?:/i.test(u) ? u : null;
}

// A YouTube page's player, asked for what it plays. In the page's own world, the only place its
// player's API exists (content scripts are isolated from it, and an injected <script> is refused
// by its Trusted Types; `scripting` isn't). Run in the page: no outside names, only its arguments.
// With `withFormats` (the quality menu), {id, page}: also what the player says of the video's
// formats (its player response's streamingData: each format's type, height, frame rate, size and
// bitrate), the page's first answer (ytInitialPlayerResponse) only while it is still that video's
// (a page that moved in place keeps the first video's); `page` null when neither says.
function playingAt(x, y, withFormats) {
  let best = null;
  for (const v of document.querySelectorAll("video")) {
    const r = v.getBoundingClientRect();
    if (x < r.left || x > r.right || y < r.top || y > r.bottom) continue;
    if (!best || r.width * r.height < best.area) best = { v, area: r.width * r.height };
  }
  // Nothing there: the button stands on a player bar (YouTube Music playing a song with its
  // picture hidden, its player closed to the bar): the page's one video playing.
  if (!best) {
    const playing = [...document.querySelectorAll("video")].filter((v) => !v.paused && v.closest(".html5-video-player"));
    if (playing.length === 1) best = { v: playing[0], area: 0 };
  }
  const player = best && best.v.closest(".html5-video-player");
  let id = "";
  try {
    id = player && player.getVideoData ? String(player.getVideoData().video_id || "") : "";
  } catch {
    id = "";
  }
  if (!withFormats) return id;
  let page = null;
  try {
    const of = (r) => (r && r.videoDetails && String(r.videoDetails.videoId || "") === id && id ? r : null);
    const r = of(player && player.getPlayerResponse && player.getPlayerResponse()) || of(window.ytInitialPlayerResponse);
    if (r) {
      const d = r.videoDetails || {};
      const sd = r.streamingData || {};
      const track = (f) => (f.audioTrack && typeof f.audioTrack === "object" ? f.audioTrack : null);
      const one = (f) => ({
        mimeType: String(f.mimeType || "").slice(0, 200),
        height: f.height,
        fps: f.fps,
        contentLength: f.contentLength,
        bitrate: f.bitrate,
        averageBitrate: f.averageBitrate,
        approxDurationMs: f.approxDurationMs,
        drm: Boolean(f.drmFamilies),
        live: f.targetDurationSec !== undefined,
        // What yt-dlp's YouTube reader sorts by besides: the sound's quality and channels, a
        // dynamic-range-compressed copy (DRC), the track's language (the original, the default).
        audioQuality: String(f.audioQuality || "").slice(0, 40),
        audioChannels: f.audioChannels,
        drc: Boolean(f.isDrc),
        original: Boolean(track(f) && /original/i.test(String(track(f).displayName || ""))),
        defaultTrack: track(f) ? Boolean(track(f).audioIsDefault) : null,
      });
      page = {
        id,
        status: String((r.playabilityStatus || {}).status || ""),
        length: d.lengthSeconds,
        live: Boolean(d.isLive || d.isUpcoming || d.isPostLiveDvr),
        formats: [...(sd.formats || []), ...(sd.adaptiveFormats || [])].slice(0, 300).map(one),
      };
    }
  } catch {
    page = null;
  }
  return { id, page };
}

const isYouTube = (url) => /(^|\.)(youtube\.com|youtube-nocookie\.com|youtu\.be)$/i.test(siteOf(url));

// What the player at `at` (the video's middle, in the sender's frame) plays: {address, page}, its
// watch address ("": a page without such a player, one too old for `world`, a page extensions may
// not read), and with `withFormats`, what it says of the video's formats (playingAt's `page`,
// as the page wrote it: read with ytFormats).
async function playerAt(sender, at, withFormats) {
  if (!at || !ext.scripting || !sender.tab) return { address: "", page: null };
  try {
    const [found] = await withTimeout(
      ext.scripting.executeScript({
        target: { tabId: sender.tab.id, frameIds: [sender.frameId || 0] },
        world: "MAIN",
        func: playingAt,
        args: [Number(at.x) || 0, Number(at.y) || 0, Boolean(withFormats)],
      }),
      3000,
    );
    const r = found && found.result;
    const id = r && typeof r === "object" ? r.id : r;
    if (!/^[\w-]{6,32}$/.test(typeof id === "string" ? id : "")) return { address: "", page: null };
    return { address: `https://www.youtube.com/watch?v=${id}`, id, page: r && typeof r === "object" ? r.page : null };
  } catch {
    return { address: "", page: null };
  }
}

// The watch address of what the player at `at` plays, or "" (playerAt).
async function playingVideo(sender, at) {
  return (await playerAt(sender, at, false)).address;
}

// A site's name for comparing two addresses: without its www., mobile or other front host
// (old.reddit.com is reddit.com), its short-link hosts as the site (fb.watch is Facebook's,
// twitter.com is x.com).
const SITE_ALIASES = { "fb.watch": "facebook.com", "twitter.com": "x.com" };
function siteKey(url) {
  const host = siteOf(url).replace(/^(www|m|mobile|web|touch|old|new|sh|music|geo|player)\./, "");
  return SITE_ALIASES[host] || host;
}

// The address the app looks a video up by: the one video's, never a list's. On YouTube that is
// what the player plays; else the address the content script named (video.js videoPage: the post
// the video sits in on a feed, #689), if it is this site's; else the page's. "" where nothing
// names the video: on YouTube (its own address is a list, the home page, a channel, to yt-dlp),
// and on a feed (`url` null: video.js found the page is many videos', LinkedIn's or Facebook's
// home, and no post around the video naming it). `seen` (the quality menu's): the player's word on
// the video's formats is kept there too, with the video it is about.
async function videoAddress(m, page, sender, seen) {
  const named = typeof m.url === "string" && /^https?:/i.test(m.url) ? m.url : "";
  if (isYouTube(page)) {
    const playing = await playerAt(sender, m.at, Boolean(seen));
    if (seen && playing.address) Object.assign(seen, { id: playing.id, player: playing.page });
    return playing.address || (isYouTube(named) ? named : "");
  }
  if (m.url === null) return "";
  return named && siteKey(named) === siteKey(page) ? named : page;
}

// What the button's click is about, as the browser knows it: the address yt-dlp is given (`own`,
// the video's page), the video's own file or stream (`src`), the page's title, the tab as
// referrer, and the cookies of what yt-dlp asks. Null when nothing names the video. `seen`: see
// videoAddress.
async function videoTarget(m, page, sender, seen) {
  const tab = sender.tab || {};
  await mediaBack;
  const named = await videoAddress(m, page, sender, seen);
  const youtube = isYouTube(page);
  // The post a video in a feed sits in (#689): the app looks that up. Nothing names it (`url`
  // null): the feed is never sent as its page, only the video's own plain file, if it has one.
  // Either way no stream the tab loaded goes with it (a feed's first stream may be another
  // video's), nor the page's title (the feed's).
  const post = !youtube && Boolean(named) && named !== page;
  const feed = !youtube && m.url === null;
  // The video's own file (video.js leaves it out beside a post a big site's reader knows), or a
  // stream it plays, still goes as `src`, with the page for its Referer. In a feed or a post, only
  // a stream known to be that player's (its poster's id, its length: an X feed's video goes as its
  // own HLS stream, not through X's API).
  const file = typeof m.src === "string" && /^https?:/i.test(m.src) ? m.src : "";
  const poster = typeof m.poster === "string" && /^https?:/i.test(m.poster) ? m.poster : "";
  const src = file || (await streamFor(sender, { poster, duration: m.duration, strict: post || feed }));
  if (!named && !src) return null;
  const own = named || page;
  return {
    own,
    src,
    title: typeof m.title === "string" && !post && !feed ? m.title.slice(0, 300) : "",
    referrer: tab.url && tab.url !== own ? tab.url : "",
    // The cookies of what yt-dlp is given (the app files them under its host): the stream's
    // own server may not be the page's, and the page's cookies are not for it.
    ...(await videoCookies(src || own, { incognito: tab.incognito, storeId: tab.cookieStoreId })),
  };
}
