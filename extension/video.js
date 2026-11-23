// Dannex Download Manager's download button over videos (a content script, in every page and
// frame). Our own design: a small pill at the video's edge that appears while the pointer is
// over the video (or it has the keyboard's focus) and fades when the pointer rests or leaves.
// Clicking it has the app look the video up (an embedded player's own address, which yt-dlp
// understands, with the video's own address when it has a plain one) and lists its qualities in a
// menu over the page; the one picked downloads without the app's window. An app that can't look
// up (an older one, another DDM chosen) opens its Download video window for this page instead,
// as does the menu's last command. It can be dragged along the video's edge, and stays where
// it was put on that site. Its menu hides it on this video, keeps it off the site (the app's
// Options → General → Download panels lists those sites), or opens those settings. The app
// decides where it may show at all (the native host: integration on, this browser captured,
// the button on, the site not excluded), and Download panels what over (videos, sound players,
// from what length) and in which corner first.
(() => {
  // One button per page: a copy cut off from the extension (after an update) gives way to the
  // new one.
  const prev = globalThis.__mdmVideoButton;
  if (prev && typeof prev === "object" && prev.alive()) return;
  if (prev && typeof prev.teardown === "function") prev.teardown();
  const ext = globalThis.browser ?? globalThis.chrome;

  // A video worth a button: big enough to be content, not a short muted looping clip (an
  // animated picture or a background). A sound player (<audio> with controls) gets one too.
  const MIN_W = 200;
  const MIN_H = 112;
  const MIN_AUDIO_H = 24;
  const CLIP_S = 15;
  // Shown this long after the pointer last moved over the video, kept while it is on the button.
  const IDLE_MS = 2600;
  // The pointer this near the video still counts as over it, and just gone from it the button
  // stays a moment: a player whose controls or a page whose layout leave the pointer a little
  // outside the frame (YouTube's) on its way to the button shouldn't lose it.
  const NEAR_PX = 48;
  const LINGER_MS = 1600;
  const INSET = 10;
  const DRAG_PX = 4;
  // Our app icon (extension/icons/icon-48.png, as data: no extension file is exposed to pages).
  const LOGO = "data:image/png;base64,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";

  const t = (key, ...subs) => ext.i18n.getMessage(key, subs) || FALLBACK[key] || key;
  const FALLBACK = {
    video_download: "Download video",
    video_more: "More options",
    video_hide_here: "Hide on this video",
    video_close: "Close",
    video_settings: "Button settings...",
    video_sent: "Sent to DDM",
    video_failed: "Dannex Download Manager isn't installed or didn't start",
    video_off: "The download button is off on this site",
    video_drm: "Protected video: it can't be saved",
    video_unnamed: "Can't tell which video this is: open its own page and try there",
    video_open_post: "Open the post, then use the button there",
    video_drag: "Drag to move it along the video's edge",
    video_looking: "Looking up the qualities…",
    video_open_mdm: "Open in DDM…",
    video_no_choices: "No video to download here",
    video_playlist: "A list of videos: choose them in Dannex Download Manager",
    video_no_ytdlp: "Dannex Download Manager needs yt-dlp for this video. Open in DDM… to install it.",
    video_too_long: "Looking up took too long",
    video_busy: "Dannex Download Manager is busy with other videos",
    video_queued: "Added to the queue",
    video_failed_lookup: "Could not look it up",
    video_tab_video: "Video",
    video_tab_audio: "Audio",
    video_audio_original: "Audio only, original $1",
    video_audio_only: "Audio only $1",
    video_pick_waiting: "Starting as soon as DDM is ready…",
    video_pick_gone: "That quality isn't available: choose another",
  };
  // A text with its one part put in ($1 in the fallback).
  const tWith = (key, part) => ext.i18n.getMessage(key, [part]) || (FALLBACK[key] || key).replace("$1", part);
  // Sizes as the browser's language writes them.
  const lang = ext.i18n.getUILanguage?.() || undefined;
  // To the background. After the extension was updated or removed, this page's copy of the script
  // is cut off from it (calls throw at once): then it takes itself away.
  const send = (m) => {
    try {
      if (!ext.runtime?.id) throw new Error("gone");
      return ext.runtime.sendMessage(m).catch(() => ({ ok: false }));
    } catch {
      teardown();
      return Promise.resolve({ ok: false, gone: true });
    }
  };
  const host = location.hostname;
  // The address the app looks the video up by — the video's, not the page's. Most pages that play
  // a video are not the video's page: YouTube's home, a search, a channel or a playlist carries a
  // list in its address ("recommended", the results, the channel's videos), and yt-dlp reads such
  // an address as the list, so the app offered the whole feed instead of the video the button was
  // on. A watch page opened from a mix or a playlist has the same problem one level down
  // (…&list=…&index=…): its own page is watch?v=<id>, and nothing else.
  //
  // Null when the page plays a video but its DOM doesn't name it: YouTube's channel masthead player
  // has no video-id attribute, no link to the video inside the player and a blob: source. The page
  // itself never goes to the app then (a channel is a whole list to yt-dlp): the background asks
  // the player what it plays, in the page's own world (background.js playingVideo), and this
  // answer is only what it falls back on.
  //
  // Elsewhere the same, generalised (#689, #643): a video in a feed (LinkedIn's, Facebook's, X's,
  // Reddit's…) sits in the post it belongs to, and that post names its own address (its
  // permalink, its timestamp link, the post id the page gives it); `ownAddress` finds it the way
  // a person would. Null there too when nothing names it: a feed's address is never the video's.
  //
  // ---- the video's own address: from here to its end mark, run as is on saved pages by
  // tools/extension/test.mjs (only `location`, `document` and the elements' parentElement,
  // parentNode.host, localName, getAttribute, querySelector(All) with simple selectors).
  function videoPage(v) {
    const watch = (id) =>
      /^[\w-]+$/.test(id || "") ? `https://www.youtube.com/watch?v=${id}` : null;
    try {
      const url = new URL(location.href);
      const name = url.hostname.replace(/^(www|m|music)\./, "");
      if (name === "youtu.be") {
        const id = url.pathname.split("/")[1];
        if (watch(id)) return `https://youtu.be/${id}`;
      }
      if (name === "youtube.com" || name === "youtube-nocookie.com") {
        // An embed's own page is the video's (…/embed/<id>).
        if (url.pathname.startsWith("/embed/")) {
          const id = watch(url.pathname.split("/")[2]);
          if (id) return id;
        }
        // A Short's or a live stream's own page (/shorts/<id>, /live/<id>). yt-dlp reads these as
        // the same one video as watch?v=<id>.
        const [, kind, pathId] = url.pathname.split("/");
        if (kind === "shorts" || kind === "live") {
          const id = watch(pathId);
          if (id) return id;
        }
        // The page is this video's own: a watch page, with or without a list on it.
        const own = watch(url.searchParams.get("v"));
        if (own && url.pathname.startsWith("/watch")) return own;
        const shown = watch(document.querySelector("ytd-watch-flexy")?.getAttribute("video-id"));
        if (shown && url.pathname.startsWith("/watch")) return shown;
        const player = watch(document.querySelector("#movie_player")?.getAttribute("video-id"));
        if (player) return player;
        // A player inside the card it belongs to (a feed, a search, a playlist): the card links to
        // the video. Only a container that names exactly one video counts — walking on into the
        // page's column would find some other card's link and send the wrong video, which is worse
        // than sending none.
        for (let node = v, i = 0; node && i < 12; i += 1, node = node.parentElement) {
          const ids = new Set();
          for (const a of node.querySelectorAll?.('a[href*="/watch?"], a[href*="youtu.be/"]') || []) {
            const link = new URL(a.href, location.href);
            const id = link.hostname.endsWith("youtu.be")
              ? link.pathname.split("/")[1]
              : link.searchParams.get("v");
            if (watch(id)) ids.add(id);
          }
          if (ids.size === 1) return watch([...ids][0]);
          if (ids.size > 1) break;
        }
        // A page that plays a video and names none: not the video's page, so not the app's.
        return null;
      }
      return ownAddress(v, url);
    } catch {
      // Not an address this page can reason about: its own follows.
    }
    return location.href;
  }

  // ---- the qualities: the app looks the video up (yt-dlp), the user picks one here ------------
  // How long a lookup is followed, from the click.
  const LOOKUP_MS = 60000;

  // The wait before asking how a lookup is going: 100 ms before the first question. An app that
  // held the last question while it looked (a newer one, up to ~1.5 s: `took` ms of HELD_MS or
  // more) is asked again at once (50 ms), so the answer reaches the page as soon as it is ready.
  // One that answers at once (an older one) is asked as seldom as ever: the old backoff's target
  // (1 s, a little longer each time, 2 s at most) less the time the question took, 50 ms at least.
  function pollDelay(n, took) {
    const HELD_MS = 1000;
    if (n <= 0) return 100;
    const spent = Number.isFinite(took) ? Math.max(0, took) : 0;
    if (spent >= HELD_MS) return 50;
    const target = Math.min(2000, Math.round(1000 * 1.25 ** n));
    return Math.max(50, Math.round(target - spent));
  }

  // A size as the browser's language writes it ("178.8 MB"); "" when unknown.
  function sizeText(bytes, lang) {
    const n = Number(bytes);
    if (!Number.isFinite(n) || n <= 0) return "";
    const units = [["gigabyte", 2 ** 30], ["megabyte", 2 ** 20], ["kilobyte", 2 ** 10]];
    const [unit, per] = units.find(([, u]) => n >= u) || ["byte", 1];
    try {
      return new Intl.NumberFormat(lang, { style: "unit", unit, unitDisplay: "short", maximumFractionDigits: 1 }).format(n / per);
    } catch {
      return `${(n / per).toFixed(1)} ${unit}`;
    }
  }

  // What a quality's row says: the app's name for it (in the app's language) and its size. Only
  // ever set as text, never as markup.
  function rowText(choice, lang) {
    const c = choice && typeof choice === "object" ? choice : {};
    return { name: String(c.label ?? "").slice(0, 200), size: sizeText(c.size, lang) };
  }

  // Which of the app's qualities go in which tab: `choice.audio === true` is sound only, anything
  // else (an app that doesn't say, an older one) is video. Tabs only when both kinds are there;
  // otherwise one list (`all`) in the app's order. Every number is the quality's place in the
  // app's own list (what a pick sends), never its place in a tab.
  function groupChoices(choices, max) {
    const list = Array.isArray(choices) ? choices.slice(0, Math.max(0, Number.isFinite(max) ? max : 100)) : [];
    const video = [];
    const audio = [];
    list.forEach((c, i) => (c && typeof c === "object" && c.audio === true ? audio : video).push(i));
    return {
      tabbed: video.length > 0 && audio.length > 0,
      video,
      audio,
      all: list.map((_, i) => i),
      start: video.length || !audio.length ? "video" : "audio",
    };
  }

  // The qualities the page itself names (the background's `page`, in yt-dlp's terms: {duration,
  // formats: [{height, fps, video, audio, ext, size, hls}]}), listed as the app will list them
  // (mdm_tools::resolve::choices, the same steps in the same order): a row per height, best first,
  // sized as the largest format of that height plus the best sound when it has none (`merge`:
  // ffmpeg joins them; without it, only the formats that carry their sound); the sound as the site
  // has it; with ffmpeg the MP3, Opus and FLAC conversions, sized from the length. Each row has the
  // app's key for it (`Choice::key`), and `choice` -1: it isn't a place in the app's list yet.
  // `audioLabel(kind, type)`: the sound's rows' names ("original" or "only", and the type). []
  // when the page names nothing usable, or a video the app would list by its format alone (no
  // height): the menu then waits for the app's list.
  function pageChoices(page, merge, audioLabel) {
    const p = page && typeof page === "object" ? page : {};
    const num = (x) => (typeof x === "number" && Number.isFinite(x) && x > 0 ? x : null);
    const fs = (Array.isArray(p.formats) ? p.formats.slice(0, 300) : [])
      .filter((f) => f && typeof f === "object")
      .map((f) => ({
        height: num(f.height) ? Math.round(f.height) : null,
        fps: num(f.fps),
        video: f.video === true,
        audio: f.audio === true,
        ext: typeof f.ext === "string" ? f.ext.replace(/\W/g, "").slice(0, 12) : "",
        size: num(f.size) ? Math.floor(f.size) : null,
        hls: f.hls === true,
      }));
    const soundOnly = (ext) => /^(m4a|mp3|opus|aac|flac|wav|weba|oga|mka)$/i.test(ext);
    if (typeof merge !== "boolean" || !fs.some((f) => f.video)) return [];
    if (fs.some((f) => f.video && f.audio && !f.height && !soundOnly(f.ext))) return [];
    const duration = num(p.duration);
    // Read left to right in every language, as the app writes them (a left-to-right isolate).
    const ltr = (text) => `\u2066${text}\u2069`;
    // The largest by `rank` ([a, b], compared in order), the last of equals (as max_by_key).
    const largest = (list, rank) =>
      list.reduce((best, f) => {
        if (!best) return f;
        const [a, b] = [rank(f), rank(best)];
        return (a[0] - b[0] || a[1] - b[1]) >= 0 ? f : best;
      }, null);
    const heights = [...new Set(fs.filter((f) => f.video && (merge || f.audio) && f.height).map((f) => f.height))].sort((a, b) => b - a);
    // Without ffmpeg, a sound that comes as a file: an HLS one stays its raw pieces.
    const bestAudio = largest(fs.filter((f) => f.audio && !f.video), (f) => [merge || !f.hls ? 1 : 0, f.size || 0]);
    const rows = heights.map((h) => {
      const best = largest(fs.filter((f) => f.video && f.height === h && (merge || f.audio)), (f) => [0, f.size || 0]);
      const fps = best && best.fps > 31 ? ` ${Math.round(best.fps)}fps` : "";
      const ext = merge ? "mp4" : (best && best.ext) || "mp4";
      const sound = merge && best && !best.audio && bestAudio ? bestAudio.size || 0 : 0;
      const size = best && best.size ? best.size + sound : null;
      return { key: `v${h}`, label: ltr(`${h}p${fps} ${ext.toUpperCase()}`), ext, size, audio: false, choice: -1 };
    });
    if (bestAudio) rows.push({ key: "audio", label: audioLabel("original", ltr(bestAudio.ext.toUpperCase())), ext: bestAudio.ext, size: bestAudio.size, audio: true, choice: -1 });
    if (merge && fs.some((f) => f.audio)) {
      const at = (kbps) => (duration ? Math.floor(duration * kbps * 125) : null);
      const row = (key, type, ext, size) => rows.push({ key, label: audioLabel("only", ltr(type)), ext, size, audio: true, choice: -1 });
      row("mp3-0", "MP3 VBR", "mp3", at(245));
      for (const kbps of [320, 256, 192, 128]) row(`mp3-${kbps}`, `MP3 ${kbps} kbps`, "mp3", at(kbps));
      row("opus", "OPUS", "opus", bestAudio && /^(webm|opus)$/i.test(bestAudio.ext) && bestAudio.size ? bestAudio.size : at(160));
      row("flac", "FLAC", "flac", at(900));
    }
    return rows;
  }

  // The app's qualities as the menu's rows: each with its place in the app's list (`choice`, what a
  // pick sends) and what it is (`key`: the app's, else its place, so it never matches a page's).
  function appRows(choices, max) {
    const list = Array.isArray(choices) ? choices.slice(0, Math.max(0, Number.isFinite(max) ? max : 100)) : [];
    return list.map((c, i) => {
      const o = c && typeof c === "object" ? c : {};
      const key = typeof o.key === "string" && /^[\w-]{1,24}$/.test(o.key) ? o.key : `#${i}`;
      return { key, label: o.label, ext: o.ext, size: o.size, audio: o.audio === true, choice: i };
    });
  }

  // Whether the video named `named` on the page at `href` is that page's own, worth asking about
  // before a click while the pointer rests over it: YouTube's main player on a watch, embed or
  // live page (never Shorts, an endless feed); elsewhere the page that is the video's own address,
  // with no other video (`many`), not a feed (`feed`: role="feed", or a feed's, a home's or a
  // search's address).
  function asksAhead(href, named, { many, main, feed }) {
    let url;
    try {
      url = new URL(href);
    } catch {
      return false;
    }
    const name = url.hostname.replace(/^(www|m|music)\./, "");
    if (name === "youtube.com" || name === "youtube-nocookie.com") return Boolean(main && named) && /^\/(?:watch|embed\/|live\/)/.test(url.pathname);
    if (name === "youtu.be") return false;
    return Boolean(named) && named === href && !many && !feed;
  }

  // Options → General → Download panels, from the app's answer to "may it show here" (the
  // background's `panelLook`): over videos, over sound players, the least length in seconds (0:
  // any), the corner it shows in first. What an app before them says (nothing): as it always was.
  function panelLook(r) {
    const p = r && typeof r === "object" ? r : {};
    const corners = ["top-right", "top-left", "bottom-left", "bottom-right"];
    return {
      video: p.video !== false,
      audio: p.audio !== false,
      minSeconds: Number.isFinite(p.minSeconds) && p.minSeconds > 0 ? p.minSeconds : 0,
      corner: corners.includes(p.corner) ? p.corner : corners[0],
    };
  }

  // Is a video or sound of `duration` seconds long enough for the button (Options' least length,
  // `min`; 0: any)? One whose length isn't known yet (NaN: its player hasn't loaded it) or has
  // none (a live stream's is endless) is.
  function longEnough(duration, min) {
    return !(min > 0) || !Number.isFinite(duration) || duration >= min;
  }

  // Follows a lookup the app is running, from its first answer: asks again only once the last
  // question was answered (never on a timer of its own), pollDelay later, until it is no longer
  // pending (that answer, with its token), the minute is up ({state: "late"}) or it is no longer
  // wanted (null: the menu closed, the video or the page changed). `ask(token, ms)`: the
  // background's answer, or {late: true} after `ms`; `sleep(ms)` may end early when dropped.
  async function followLookup(first, { ask, alive, sleep, now, end }) {
    let s = first;
    let token = "";
    let took = 0;
    for (let n = 0; ; n += 1) {
      if (!alive()) return null;
      if (!s || typeof s !== "object") return { state: "gone" };
      if (typeof s.token === "string" && s.token) token = s.token;
      if (s.state !== "pending") return { ...s, token };
      if (!token) return { state: "gone" };
      const left = end - now();
      if (left <= 0) return { state: "late" };
      await sleep(Math.min(pollDelay(n, took), left));
      if (!alive()) return null;
      if (end - now() <= 0) return { state: "late" };
      const asked = now();
      const r = await ask(token, end - asked);
      took = now() - asked;
      if (!alive() || !r || r.gone) return null;
      if (r.late) return { state: "late" };
      // Another DDM chosen meanwhile: the window, as for a click.
      s = r.fallback ? { state: "use-window" } : r.lookup;
    }
  }

  // The background's answer to a lookup or to a choice, as a lookup's state: null when there is
  // nothing to show (the extension gone); "late" past the minute, "unnamed" for a video it can't
  // tell, "feed" for a video in a feed no post names (#689), and "use-window" for no lookups here
  // (another DDM, an older one) or an answer that isn't one.
  function replyState(r) {
    if (r && r.gone) return null;
    if (r && r.late) return { state: "late" };
    if (r && (r.why === "unnamed" || r.why === "feed")) return { state: r.why };
    if (!r || r.fallback || !r.lookup || typeof r.lookup !== "object") return { state: "use-window" };
    return r.lookup;
  }

  // What a lookup's state (or replyState's) comes to, as the menu does it: "follow" it while the
  // app looks, "offer" its qualities, "say" a message in the menu (with the app's own words under
  // it), "flash" a word on the button (the menu closed), or "window": the click as it always was,
  // the app's Download video window. `t`: the texts. `picked`: the answer to a choice, which only
  // a download confirms; anything else there is the window's.
  function lookupAction(s, t, picked) {
    const state = s && typeof s === "object" && typeof s.state === "string" ? s.state : "";
    // A choice taken: downloading, in the queue, or handed to the app, which didn't say in time
    // what became of it ("sent": it will download it; the window would download it again).
    if (state === "started" || state === "sent") return { act: "flash", kind: "ok", text: t("video_sent") };
    if (state === "queued") return { act: "flash", kind: "ok", text: t("video_queued") };
    if (!picked) {
      if (state === "pending") return { act: "follow" };
      if (state === "ready" && Array.isArray(s.choices) && s.choices.length) return { act: "offer" };
      if (state === "ready" || (state === "failed" && s.reason === "no-choices")) return { act: "say", text: t("video_no_choices") };
      if (state === "failed" && s.reason === "no-ytdlp") return { act: "say", text: t("video_no_ytdlp") };
      if (state === "failed") return { act: "say", text: t("video_failed_lookup"), detail: typeof s.message === "string" ? s.message : "" };
      if (state === "late") return { act: "say", text: t("video_too_long") };
      if (state === "list") return { act: "say", text: t("video_playlist") };
      if (state === "busy") return { act: "say", text: t("video_busy") };
      if (state === "off") return { act: "flash", kind: "err", text: t("video_off") };
      if (state === "unnamed") return { act: "flash", kind: "err", text: t("video_unnamed") };
      if (state === "feed") return { act: "flash", kind: "err", text: t("video_open_post") };
    }
    // "use-window", "gone" (the app forgot it: restarted), a choice the app didn't take, and
    // anything newer: the window.
    return { act: "window" };
  }

  // The click's window at most once, whichever way the click comes to it (an answer that isn't a
  // lookup, the lookup's end, a choice the app didn't take, "Open in…"): true the first time for
  // `j`, the click's lookup.
  function firstWindow(j) {
    if (!j || j.windowed) return false;
    j.windowed = true;
    return true;
  }

  // The video the pointer takes the button to: the one under it (`found`), but not another than
  // its own (`shown`) while the qualities' menu is open (`picking`): the menu reaches over the
  // videos around its own (a feed's), and the pointer crosses them on its way to a row. The menu
  // still closes when its video plays something else or the page changes (tick), or on a click
  // elsewhere.
  function pointerTarget(found, shown, picking) {
    if (!found || (picking && found !== shown)) return null;
    return found;
  }

  // Up from a node: its parent, or the element whose shadow root holds it (Reddit's player keeps
  // its <video> in one).
  const upFrom = (n) => n.parentElement || (n.parentNode && n.parentNode.host) || null;

  // `href` as an address (relative to the page), or null.
  function linkTo(href) {
    try {
      return href ? new URL(href, location.href) : null;
    } catch {
      return null;
    }
  }

  // The nearest container around the video that names its post: `read(node)` gives what a
  // container names, { sure, maybe } (Maps of an id to the address it stands for), `sure` the
  // post's or the video's own ids, `maybe` weaker ones (a link that may be the post's). The
  // innermost container naming anything decides, and only when it names exactly one: one naming
  // two has taken in the next post or the page's column, and a wrong video is worse than none.
  // The walk ends at the page's column (a feed, role="feed") and at a container holding another
  // video: from there up, what it names may be that one's (a post naming nothing next to one
  // that names its own). A page that breaks a reader's expectations names nothing (never the
  // page's own address instead).
  function nearest(v, read) {
    try {
      for (let node = v, i = 0; node && i < 16; i += 1, node = upFrom(node)) {
        if (node.localName === "body" || node.localName === "html") break;
        if (node !== v && (node.getAttribute?.("role") === "feed" || manyVideos(node))) break;
        const { sure, maybe } = read(node);
        if (sure.size) return sure.size === 1 ? [...sure.values()][0] : null;
        if (maybe.size) return maybe.size === 1 ? [...maybe.values()][0] : null;
      }
    } catch {
      // Nothing named.
    }
    return null;
  }

  const decoded = (s) => {
    try {
      return decodeURIComponent(s);
    } catch {
      return s;
    }
  };

  // The links in a container (the container itself when it is one), as addresses, with whether
  // each holds a <time> (a timestamp link: a post's own permalink).
  function linksIn(node, sel = "a[href]") {
    const found = [];
    const own = node.localName === "a" ? [node] : [];
    for (const a of [...own, ...(node.querySelectorAll ? node.querySelectorAll(sel) : [])]) {
      const u = linkTo(a.getAttribute && a.getAttribute("href"));
      if (u && /^https?:$/.test(u.protocol)) found.push({ u, time: Boolean(a.querySelector && a.querySelector("time")) });
    }
    return found;
  }

  // The page's site without its www. or mobile host (old.reddit.com is reddit.com).
  const siteName = (host) => host.replace(/^(www|m|mobile|web|touch|old|new|sh|music|geo|player)\./, "");

  // A video's own address on this page, by site; the page's own when it is that one video's page;
  // null when the page is a feed, a list or a search and nothing names the video.
  function ownAddress(v, url) {
    const site = SITES.find((s) => s.hosts.test(siteName(url.hostname)));
    if (site) {
      // A site's own player in another page (an embed) is its video's page: its address holds
      // what the links around it may lack (a private video's key).
      if (site.embed && site.embed(url)) return location.href;
      const found = nearest(v, (node) => site.read(node, url));
      if (found) return found;
      return site.own(url) ? location.href : null;
    }
    // Any other site: a player embedded in another page (its own frame), or the one video of its
    // page, is its video's own page, as before.
    if (framed() || (!inFeed(v) && document.querySelectorAll?.("video").length <= 1)) return location.href;
    // A page of many videos: the post around this one, when one names it; else the page in
    // general (an article with two players is still theirs to yt-dlp), but never a feed's
    // (role="feed") or a page whose address is a feed's, a home's or a search's.
    const found = nearest(v, (node) => genericRead(node, url));
    if (found) return found;
    return inFeed(v) || FEED_PATH.test(url.pathname) ? null : location.href;
  }

  // A feed's, a home page's, a search's address: many videos, none of them the page's.
  const FEED_PATH = /^\/(?:$|home\/?$|feed|explore|search|discover|trending|popular|latest|timeline|for-?you|following|watch\/?$)/i;
  // The video sits in a feed (role="feed": Mastodon's, the accessible feeds).
  function inFeed(v) {
    for (let node = upFrom(v); node; node = upFrom(node)) if (node.getAttribute?.("role") === "feed") return true;
    return false;
  }

  const framed = () => {
    try {
      return window.top !== window;
    } catch {
      return false;
    }
  };
  const manyVideos = (node) => Boolean(node.querySelectorAll && node.querySelectorAll("video").length > 1);

  // Any other site: a link of this site that has a post's or a video's shape, or that holds the
  // post's timestamp (Mastodon's, a forum's), and isn't the page itself.
  function genericRead(node, url) {
    const sure = new Map();
    const maybe = new Map();
    const here = `${url.origin}${url.pathname}${url.search}`;
    for (const { u, time } of linksIn(node)) {
      if (siteName(u.hostname) !== siteName(url.hostname)) continue;
      const address = `${u.origin}${u.pathname}${u.search}`;
      if (address === here) continue;
      if (time) sure.set(address, address);
      else if (/\/(?:status|statuses|videos?|watch|posts?|p|reels?|comments|clips?|v)(?:\/|\?|$)/i.test(u.pathname + (u.search ? "?" : ""))) maybe.set(address, address);
    }
    const permalink = linkTo(node.getAttribute?.("data-permalink"));
    if (permalink && siteName(permalink.hostname) === siteName(url.hostname)) sure.set(permalink.href, permalink.href);
    return { sure, maybe };
  }

  // LinkedIn's comment tools' key: base64url of a protobuf naming the post (field 1 its
  // activity, field 2 its ugcPost), whose one field is the id as a zigzag varint. [kind, id] or
  // null.
  function commentToolsUrn(key) {
    try {
      const bytes = [...atob(key.replace(/-/g, "+").replace(/_/g, "/"))].map((c) => c.charCodeAt(0));
      const kind = { 10: "activity", 18: "ugcPost" }[bytes[0]];
      if (!kind || bytes[1] !== bytes.length - 2 || bytes[2] !== 8) return null;
      let n = 0n;
      let shift = 0n;
      for (let i = 3; i < bytes.length; i += 1) {
        n |= BigInt(bytes[i] & 0x7f) << shift;
        shift += 7n;
        if (!(bytes[i] & 0x80)) {
          if (i !== bytes.length - 1 || n & 1n) return null;
          const id = String(n >> 1n);
          return /^\d{15,20}$/.test(id) ? [kind, id] : null;
        }
      }
    } catch {
      // Not such a key.
    }
    return null;
  }

  // The big sites with feeds: what names a post in their pages, the address yt-dlp reads it by
  // (its extractors' own address shapes), and their pages that are one video's own.
  const SITES = [
    {
      // A post's container carries its URN (data-urn, data-id); its links name it too. yt-dlp's
      // LinkedIn extractor reads /feed/update/urn:li:activity:<id>/ (and /posts/…); a post known
      // only by its ugcPost URN goes as /feed/update/urn:li:ugcPost:<id>/, the same public page,
      // where yt-dlp's generic reader finds the video.
      //
      // LinkedIn's feed since 2026 (its server-driven app: role="listitem" posts in a
      // data-testid="mainFeed" list) names a post nowhere near its video, only in the keys of
      // the post's parts (componentkey): its comments' URNs (urn:li:comment:(activity:<id>,…)),
      // its text's translation key (…userGeneratedContentId=<id>…, its ugcPost), and its comment
      // tools' key, <base64>-replaceableCommentTools, a protobuf of the post's URN (field 1 its
      // activity, field 2 its ugcPost, the id zigzag-encoded). Checked against a signed-in feed:
      // each decodes to the post its comments name, and each ugcPost's page plays the video.
      hosts: /^linkedin\.com$/,
      embed: (u) => /^\/embed\//.test(u.pathname),
      read(node) {
        const sure = new Map();
        const maybe = new Map();
        const post = (kind, id) =>
          (kind === "activity" ? sure : maybe).set(`${kind}:${id}`, `https://www.linkedin.com/feed/update/urn:li:${kind}:${id}/`);
        const add = (s) => {
          s = String(s || "");
          for (const m of s.matchAll(/urn:li:(?:comment:\()?(activity|ugcPost|share):(\d{6,})/g)) post(m[1], m[2]);
          for (const m of s.matchAll(/\b(userGeneratedContent|share|activity)Id=(\d{6,})/g)) post(m[1] === "userGeneratedContent" ? "ugcPost" : m[1], m[2]);
          // A post's public address: /posts/<who>_<words>-activity-<id>-<four>.
          const p = /\/posts\/[^/?#]*-(activity|ugcPost)-(\d{6,})-\w{4}/.exec(s);
          if (p) post(p[1], p[2]);
          const k = /^([\w-]{12,40})-replaceableCommentTools/.exec(s);
          const urn = k && commentToolsUrn(k[1]);
          if (urn) post(urn[0], urn[1]);
        };
        // The container's own attributes, whatever their names (the page renames them), and its
        // insides' known ones.
        for (const a of node.attributes || []) add(a.value);
        for (const el of node.querySelectorAll?.("[data-urn], [data-id], [data-activity-urn], [data-entity-urn], [componentkey]") || []) {
          for (const a of ["data-urn", "data-id", "data-activity-urn", "data-entity-urn", "componentkey"]) add(el.getAttribute?.(a));
        }
        for (const { u } of linksIn(node)) if (/linkedin\.com$/.test(u.hostname)) add(decoded(u.pathname));
        return { sure, maybe };
      },
      own: (u) => /^\/(?:feed\/update\/urn:li:|posts\/|embed\/feed\/update\/|events\/|learning\/)/.test(u.pathname),
    },
    {
      // A video's own links (a reel, a watch page, a page's video, data-video-id) before a post's
      // (its permalink); fb.watch short links as they are (they lead to the video's page).
      hosts: /^(?:[\w-]+\.)?facebook\.com$/,
      embed: (u) => /^\/plugins\/video\.php$/.test(u.pathname),
      read(node) {
        const sure = new Map();
        const maybe = new Map();
        const video = (id) => sure.set(`v:${id}`, `https://www.facebook.com/watch/?v=${id}`);
        const id = node.getAttribute?.("data-video-id");
        if (/^\d{6,}$/.test(id || "")) video(id);
        for (const el of node.querySelectorAll?.("[data-video-id]") || []) {
          const d = el.getAttribute("data-video-id");
          if (/^\d{6,}$/.test(d || "")) video(d);
        }
        for (const { u } of linksIn(node)) {
          if (u.hostname === "fb.watch") {
            const code = u.pathname.split("/")[1];
            if (/^\w+$/.test(code || "")) sure.set(`w:${code}`, `https://fb.watch/${code}/`);
            continue;
          }
          if (!/(^|\.)facebook\.com$/.test(u.hostname)) continue;
          const p = u.pathname;
          let m;
          if ((m = /^\/reel\/(\d+)/.exec(p))) sure.set(`v:${m[1]}`, `https://www.facebook.com/reel/${m[1]}/`);
          else if (/^\/watch\/?$/.test(p) && /^\d+$/.test(u.searchParams.get("v") || "")) video(u.searchParams.get("v"));
          else if (/^\/video\.php$/.test(p) && /^\d+$/.test(u.searchParams.get("v") || "")) video(u.searchParams.get("v"));
          else if ((m = /^\/[^/]+\/videos\/(?:[^/]+\/)?(\d+)/.exec(p))) video(m[1]);
          else if ((m = /^\/([^/]+)\/posts\/(pfbid\w+|\d+)/.exec(p))) maybe.set(`p:${m[2]}`, `https://www.facebook.com/${m[1]}/posts/${m[2]}`);
          else if ((m = /^\/groups\/([^/]+)\/(?:permalink|posts)\/(\d+)/.exec(p))) maybe.set(`p:${m[2]}`, `https://www.facebook.com/groups/${m[1]}/posts/${m[2]}/`);
          else if (/^\/(?:permalink|story)\.php$/.test(p) && /^(?:pfbid\w+|\d+)$/.test(u.searchParams.get("story_fbid") || "")) {
            const story = u.searchParams.get("story_fbid");
            const owner = u.searchParams.get("id");
            maybe.set(`p:${story}`, `https://www.facebook.com/permalink.php?story_fbid=${story}${/^\d+$/.test(owner || "") ? `&id=${owner}` : ""}`);
          }
        }
        return { sure, maybe };
      },
      own: (u) =>
        /^\/(?:reel\/\d|watch\/?$|video\.php$|[^/]+\/videos\/|[^/]+\/posts\/|permalink\.php$|story\.php$|groups\/[^/]+\/(?:permalink|posts)\/|plugins\/video\.php$|share\/[vrp]\/)/.test(u.pathname) &&
        (!/^\/watch\/?$/.test(u.pathname) || /^\d+$/.test(u.searchParams.get("v") || "")),
    },
    {
      // A post's timestamp links to it (/<user>/status/<id>); other status links (a quoted post,
      // its photos and its analytics) count only when there is no timestamp.
      hosts: /^(?:x|twitter)\.com$/,
      read(node) {
        const sure = new Map();
        const maybe = new Map();
        for (const { u, time } of linksIn(node, 'a[href*="/status/"]')) {
          const m = /^\/(\w{1,15}|i\/web)\/status\/(\d+)/.exec(u.pathname);
          if (!m || !/(^|\.)(?:x|twitter)\.com$/.test(u.hostname)) continue;
          (time ? sure : maybe).set(m[2], `${location.origin}/${m[1]}/status/${m[2]}`);
        }
        return { sure, maybe };
      },
      own: (u) => /^\/(?:\w{1,15}|i\/web)\/status\/\d+|^\/i\/(?:broadcasts|spaces)\//.test(u.pathname),
    },
    {
      // A post's links (/p/<code>/, a reel's /reel/<code>/): its timestamp, its comments.
      hosts: /^instagram\.com$/,
      embed: (u) => /\/embed\/?$/.test(u.pathname),
      read(node) {
        const sure = new Map();
        for (const { u } of linksIn(node)) {
          // Not a reel's sound (/reels/audio/<id>/, the other reels using it).
          const m = /^(?:\/[\w.]+)?\/(p|tv|reels?)\/(?!audio\/)([\w-]{5,})/.exec(u.pathname);
          if (!m || !/instagram\.com$/.test(u.hostname)) continue;
          sure.set(m[2], `https://www.instagram.com/${m[1] === "tv" ? "p" : m[1].replace(/s$/, "")}/${m[2]}/`);
        }
        return { sure, maybe: new Map() };
      },
      own: (u) => /^(?:\/[\w.]+)?\/(?:p|tv|reels?)\/[\w-]{5,}|^\/stories\/[^/]+\/\d/.test(u.pathname),
    },
    {
      // A post (shreddit-post, old Reddit's .thing) names its permalink in its attributes; its
      // links (title, comments) name the same /comments/<id>/.
      hosts: /^reddit\.com$/,
      read(node) {
        const sure = new Map();
        const add = (s) => {
          const m = /^(\/(?:r|u|user)\/[^/]+)?\/comments\/(\w+)/.exec(linkTo(s)?.pathname || "");
          if (m) sure.set(m[2], `${location.origin}${m[1] || ""}/comments/${m[2]}/`);
        };
        add(node.getAttribute?.("permalink"));
        add(node.getAttribute?.("data-permalink"));
        for (const el of node.querySelectorAll?.("[permalink], [data-permalink]") || []) {
          add(el.getAttribute("permalink"));
          add(el.getAttribute("data-permalink"));
        }
        for (const { u } of linksIn(node, 'a[href*="/comments/"]')) if (/reddit\.com$/.test(u.hostname)) add(u.href);
        return { sure, maybe: new Map() };
      },
      own: (u) => /\/comments\/\w+|^\/r\/[^/]+\/s\/\w+/.test(u.pathname),
    },
    {
      // A video's link (/@<user>/video/<id>); else its player's id (xgwrapper-<n>-<id>), sent in
      // yt-dlp's own form for a video whose author isn't known (/@_/video/<id>).
      hosts: /^tiktok\.com$/,
      embed: (u) => /^\/embed\//.test(u.pathname),
      read(node) {
        const sure = new Map();
        for (const { u } of linksIn(node, 'a[href*="/video/"]')) {
          const m = /^\/@([\w.-]+)\/video\/(\d+)/.exec(u.pathname);
          if (m && /tiktok\.com$/.test(u.hostname)) sure.set(m[2], `https://www.tiktok.com/@${m[1]}/video/${m[2]}`);
        }
        if (!sure.size) {
          for (const el of [node, ...(node.querySelectorAll?.('[id^="xgwrapper-"]') || [])]) {
            const m = /^xgwrapper-\d+-(\d{15,})$/.exec(el.getAttribute?.("id") || "");
            if (m) sure.set(m[1], `https://www.tiktok.com/@_/video/${m[1]}`);
          }
        }
        return { sure, maybe: new Map() };
      },
      own: (u) => /^\/@[\w.-]+\/(?:video|photo)\/\d+|^\/embed\//.test(u.pathname),
    },
    {
      // A clip's links: vimeo.com/<id> (with an unlisted video's hash), in a channel, a group, a
      // showcase.
      hosts: /^(?:player\.)?vimeo\.com$/,
      embed: (u) => u.hostname.startsWith("player."),
      read(node) {
        const sure = new Map();
        for (const { u } of linksIn(node)) {
          if (!/(^|\.)vimeo\.com$/.test(u.hostname)) continue;
          const m = /^(?:\/video|\/channels\/[^/]+|\/groups\/[^/]+\/videos|\/showcase\/[^/]+\/video)?\/(\d{4,})(\/[\da-f]{10})?\/?$/.exec(u.pathname);
          if (m) sure.set(m[1], `https://vimeo.com/${m[1]}${m[2] || ""}`);
        }
        return { sure, maybe: new Map() };
      },
      own: (u) =>
        u.hostname.startsWith("player.") ||
        /^(?:\/channels\/[^/]+|\/groups\/[^/]+\/videos|\/showcase\/[^/]+\/video|\/[^/]+\/review)?\/\d{4,}|^\/ondemand\//.test(u.pathname),
    },
    {
      hosts: /^(?:geo\.)?dailymotion\.com$/,
      embed: (u) => u.hostname.startsWith("geo.") || /^\/(?:embed|player)\b/.test(u.pathname),
      read(node) {
        const sure = new Map();
        for (const { u } of linksIn(node, 'a[href*="/video/"]')) {
          const m = /^\/(?:embed\/)?video\/([a-z0-9]+)/i.exec(u.pathname);
          if (m && /dailymotion\.com$/.test(u.hostname)) sure.set(m[1], `https://www.dailymotion.com/video/${m[1]}`);
        }
        return { sure, maybe: new Map() };
      },
      own: (u) => u.hostname.startsWith("geo.") || /^\/(?:embed\/)?video\/[a-z0-9]+|^\/player/i.test(u.pathname),
    },
  ];
  // ---- the video's own address: end.

  // The address this page first loaded at (`download`: a page that moves in place keeps its first
  // page's og:title).
  const landedAt = location.href;
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // "Hide on this video": the video's address (players reuse one element for the next video),
  // else the element.
  const dismissed = new WeakSet();
  const dismissedSrc = new Set();
  const isDismissed = (v) => dismissed.has(v) || (v.currentSrc && dismissedSrc.has(v.currentSrc));
  // May it show here? The app's answer (Options: the button on, the site not listed), asked
  // again when older than ASK_MS, so a change in Options reaches a page already open (#532: one
  // that never reloads, as YouTube's, kept the button for good). `never`: this page's own
  // "Never on this site".
  let allowed = null;
  let askedAt = 0;
  let asking = null;
  let never = false;
  let placed = false;
  const ASK_MS = 5000;
  const answered = () => allowed !== null && Date.now() - askedAt < ASK_MS;
  // The corners Options → Download panels names, as spots on the video's edge (fractions of the
  // room, below), the default first.
  const CORNER_SPOTS = { "top-right": { x: 1, y: 0 }, "top-left": { x: 0, y: 0 }, "bottom-left": { x: 0, y: 1 }, "bottom-right": { x: 1, y: 1 } };
  // Download panels' settings, with the app's answer (`panelLook`): over videos, over sound
  // players, the least length (seconds, 0: any), the corner it shows in first.
  let panelOpts = panelLook(null);
  let place = CORNER_SPOTS["top-right"]; // where on the edge: Options' corner, or where it was dragged on this site
  let chosen = false; // the user put it where it is (dragged it, on this site): theirs, never moved for a page's control
  let auto = { v: null, at: null, t: 0 }; // the spot found for this video, kept for a moment
  const AUTO_MS = 400;
  // Where it is on the screen, while the pointer is on it (#560): { v, left, top }.
  let pinned = null;
  let video = null; // the video it is over
  // A video gone from under its button (#503): a hover-to-play preview (YouTube's thumbnails)
  // stops as soon as the pointer leaves it for the button, since the page sees the pointer go.
  // The button stays where it was, for that video, while the pointer is on it (or its menu is
  // open), or for HOLD_MS while the pointer is still over where the video was, on its way to it;
  // a click downloads what the video was playing.
  let held = null; // { r: the video's last rectangle, at: the button's spot, d: what download sends, since }
  const HOLD_MS = 1500;
  let lastRect = null; // the video's rectangle when the button was last placed over it
  let entered = null; // what download would send, taken as the pointer reached the button: { v, d, t }
  let lastMove = 0;
  // When the pointer was last near the shown video (NEAR_PX, LINGER_MS).
  let nearAt = 0;
  let pointer = { x: -1, y: -1 };
  let menuOpen = false;
  let dragging = null;
  let busy = false;
  // The lookup the qualities menu is for (one at a time), until the menu closes.
  let job = null;
  // The lookup asked ahead of a click (askAhead), one at a time: {v, src, href, asked, key, at,
  // promise, reply, token, taken}. `aheadOff`: no lookups here (an older app, another DDM): never
  // asked ahead again on this page. Never from a private window.
  let aheadJob = null;
  let aheadOff = false;
  // The video clicked last ({key, at}): not asked ahead again (the app has it).
  let clicked = null;
  // What the person used last, "key" or "pointer": the keyboard's focus follows the keyboard only.
  let lastInput = "pointer";
  let aheadTimer = 0;
  const privateWindow = Boolean(ext.extension?.inIncognitoContext);
  // How long the pointer rests over a video on its own page before it is asked ahead, and how long
  // an answer asked ahead is taken (the app keeps a lookup ten minutes).
  const AHEAD_DWELL_MS = 800;
  const AHEAD_KEEP_MS = 9 * 60000;
  let dwell = { v: null, key: "", since: 0, done: false };
  let flashing = false;
  let raf = 0;
  let scan = 0;
  // Kept on screen while the keyboard is on it (not after a mouse click: that leaves focus too),
  // or on the video's player (its own controls).
  const keyboardOn = () =>
    Boolean(shadow.activeElement && shadow.activeElement.matches(":focus-visible")) || playerFocused();
  let focusedPlayer = null;
  const playerFocused = () =>
    Boolean(video && focusedPlayer && document.activeElement === focusedPlayer &&
      focusedPlayer.contains(video) && focusedPlayer.matches(":focus-visible"));

  // ---- the button: a closed shadow tree the page's styles and scripts can't reach ----------
  const root = document.createElement("ddm-video-button");
  // Each with !important: a page's own rules can't hide or move it.
  for (const [k, v] of Object.entries({
    all: "initial", position: "fixed", "z-index": "2147483647", left: "0", top: "0", display: "none",
    contain: "layout style", opacity: "1", visibility: "visible", "pointer-events": "auto",
  })) root.style.setProperty(k, v, "important");
  const shadow = root.attachShadow({ mode: "closed" });
  const CSS = `
:host { all: initial; }
* { box-sizing: border-box; }
/* A light, glossy panel that stands out over a dark video, with our icon: the classic download
   button's feel, made modern (rounded, crisp type, soft shadow and motion). */
.pill {
  display: flex; align-items: stretch; height: 34px; border-radius: 9px;
  background: linear-gradient(#ffffff, #f1f4f8 48%, #e3e8ef 52%, #eef2f6);
  color: #17324d; border: 1px solid rgba(23, 50, 77, .28);
  box-shadow: 0 4px 14px rgba(0, 0, 0, .32), 0 1px 0 rgba(255, 255, 255, .9) inset;
  font: 600 13px/1 "Segoe UI", system-ui, -apple-system, Roboto, sans-serif;
  opacity: 0; transform: translateY(-3px) scale(.98);
  transition: opacity .16s ease-out, transform .16s ease-out, filter .12s;
  user-select: none; -webkit-user-select: none; touch-action: none; white-space: nowrap;
}
.pill.shown { opacity: 1; transform: none; }
.pill.dragging { cursor: grabbing; box-shadow: 0 10px 28px rgba(0, 0, 0, .45); transition: none; }
.pill.dragging .go { cursor: grabbing; }
.pill.ok { background: linear-gradient(#f2fff6, #d4f3df); border-color: #2f9a5b; color: #135c33; }
.pill.err { background: linear-gradient(#fff4f4, #f6d6d6); border-color: #c24848; color: #7d1f1f; }
button {
  /* all: unset resets box-sizing too, past the rule above: without it a menu item's 100 % width
     plus its padding ran past the menu's edge (#81). */
  all: unset; box-sizing: border-box; display: flex; align-items: center; gap: 8px;
  color: inherit; cursor: pointer; font: inherit;
}
.go { padding-block: 0; padding-inline: 8px 12px; border-start-start-radius: 8px; border-end-start-radius: 8px; cursor: grab; }
.more { padding: 0 9px; border-inline-start: 1px solid rgba(23, 50, 77, .18); }
.close { padding: 0 8px; border-inline-start: 1px solid rgba(23, 50, 77, .18); border-start-end-radius: 8px; border-end-end-radius: 8px; }
.go:hover, .more:hover { background: linear-gradient(rgba(214, 232, 255, .9), rgba(182, 212, 250, .9)); }
.close:hover { background: linear-gradient(rgba(255, 226, 226, .95), rgba(246, 196, 196, .95)); color: #9b1c1c; }
.go:active, .more:active { background: rgba(160, 196, 242, .9); }
.close:active { background: rgba(240, 170, 170, .95); }
.close svg { width: 10px; height: 10px; color: inherit; }
button:focus-visible { outline: 2px solid #2a78d6; outline-offset: -2px; }
.go img { width: 22px; height: 22px; flex: none; filter: drop-shadow(0 1px 1px rgba(0, 0, 0, .25)); }
.more svg { width: 12px; height: 12px; color: #3b5876; }
.compact .label { display: none; }
.compact .go { padding: 0 6px; border-start-start-radius: 8px; border-end-start-radius: 8px; }
.menu {
  position: absolute; min-width: 210px; padding: 4px; border-radius: 9px;
  background: #fbfcfe; color: #17324d; border: 1px solid rgba(23, 50, 77, .22);
  box-shadow: 0 12px 32px rgba(0, 0, 0, .35);
  font: 400 13px/1.2 "Segoe UI", system-ui, -apple-system, Roboto, sans-serif;
  display: none;
}
.menu.open { display: block; }
.menu button { display: block; width: 100%; padding: 8px 12px; border-radius: 6px; cursor: default; }
.menu button:hover, .menu button:focus-visible { background: #dceafb; outline: none; }
.menu hr { border: 0; border-top: 1px solid rgba(23, 50, 77, .12); margin: 4px 2px; }
/* The qualities: a row each (the app's name for it, its size at the end), a message while it
   looks or when it found none, "Open in…" last. Scrolls when there are many. */
.picks { min-width: 240px; max-width: 360px; max-height: min(300px, 45vh); overflow: auto; overscroll-behavior: contain; }
.picks:focus { outline: none; }
.menu .pick { display: flex; align-items: baseline; gap: 16px; }
.pick .name { overflow-wrap: anywhere; }
.pick .size { margin-inline-start: auto; color: #4a6681; font-variant-numeric: tabular-nums; white-space: nowrap; }
/* Video / Audio tabs over the rows, when the video has both. */
.tabs { display: flex; gap: 4px; padding: 0 0 4px; }
.tabs[hidden] { display: none; }
.menu .tab { flex: 1 1 0; width: auto; text-align: center; font-weight: 600; color: #3b5876; border-radius: 6px 6px 0 0; border-bottom: 2px solid transparent; }
.menu .tab[aria-selected="true"] { color: #17324d; border-bottom-color: #2a78d6; }
.note { padding: 8px 12px; color: #3b5876; overflow-wrap: anywhere; }
.note .why { display: block; margin-top: 4px; font-size: 12px; color: #5a7086; }
/* The page's own qualities while the app looks: a thin line moving along the menu's top edge (it
   stays there while the rows scroll) says the list is being checked; a row picked meanwhile pulses
   until the app's list comes and it is sent. */
.picks.refining {
  background: linear-gradient(90deg, rgba(42, 120, 214, 0), rgba(42, 120, 214, .7), rgba(42, 120, 214, 0)) -40% 1px / 40% 2px no-repeat, #fbfcfe;
  animation: ddm-refine 1.1s linear infinite; }
@keyframes ddm-refine { from { background-position: -40% 1px, 0 0; } to { background-position: 140% 1px, 0 0; } }
.menu .pick.waiting { background: #dceafb; animation: ddm-wait 1s ease-in-out infinite alternate; }
@keyframes ddm-wait { from { background: #dceafb; } to { background: #b9d5f6; } }
.live { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
@media (prefers-reduced-motion: reduce) { .pill { transition: none; transform: none; } .picks.refining, .menu .pick.waiting { animation: none; } .picks.refining { background-position: 50% 1px, 0 0; } }
`;
  // A constructed sheet: a page whose Content-Security-Policy refuses inline styles can't strip
  // it (Firefox applies the page's policy to what a content script adds).
  try {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(CSS);
    shadow.adoptedStyleSheets = [sheet];
  } catch {
    const style = document.createElement("style");
    style.textContent = CSS;
    shadow.appendChild(style);
  }
  shadow.innerHTML += `
<div class="pill" part="pill">
  <button class="go" type="button">
    <img alt="" draggable="false">
    <span class="label"></span>
  </button>
  <button class="more" type="button" aria-haspopup="menu" aria-expanded="false">
    <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6"
      stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 4.5 6 7.5l3-3"/></svg>
  </button>
  <button class="close" type="button">
    <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6"
      stroke-linecap="round" aria-hidden="true"><path d="M2 2l6 6M8 2 2 8"/></svg>
  </button>
</div>
<div class="menu" role="menu">
  <button role="menuitem" data-act="download"></button>
  <hr>
  <button role="menuitem" data-act="hide"></button>
  <button role="menuitem" data-act="never"></button>
  <hr>
  <button role="menuitem" data-act="settings"></button>
</div>
<div class="menu picks" role="group" tabindex="-1" aria-describedby="said">
  <div class="note" id="said" role="status"></div>
  <div class="tabs" role="tablist" hidden>
    <button type="button" class="tab" role="tab" id="tab-video" aria-controls="qualities" data-tab="video"></button>
    <button type="button" class="tab" role="tab" id="tab-audio" aria-controls="qualities" data-tab="audio"></button>
  </div>
  <div class="qualities" id="qualities" role="none">
    <div class="rows" role="menu"></div>
  </div>
  <hr>
  <button data-act="window"></button>
  <div class="live" role="status" aria-live="polite"></div>
</div>`;
  shadow.querySelector(".go img").src = LOGO;
  const pill = shadow.querySelector(".pill");
  const go = shadow.querySelector(".go");
  const label = shadow.querySelector(".label");
  const more = shadow.querySelector(".more");
  const closeBtn = shadow.querySelector(".close");
  const menu = shadow.querySelector(".menu");
  const item = (act) => menu.querySelector(`[data-act="${act}"]`);
  // The qualities' menu: its message, its rows, "Open in…".
  const picks = shadow.querySelector(".picks");
  const note = picks.querySelector(".note");
  const rows = picks.querySelector(".rows");
  const tabs = picks.querySelector(".tabs");
  const tabList = [...tabs.querySelectorAll("[data-tab]")];
  const qualities = picks.querySelector(".qualities");
  const openRow = picks.querySelector('[data-act="window"]');
  // Said to a screen reader only (a row picked before the app's list came).
  const live = picks.querySelector(".live");
  // Which of the two menus is open (`menuOpen`).
  let panel = menu;
  pill.dir = ext.i18n.getMessage("@@bidi_dir") || "ltr";
  menu.dir = pill.dir;
  picks.dir = pill.dir;
  const idleLabel = t("video_download");
  label.textContent = idleLabel;
  go.title = `${idleLabel} — ${t("video_drag")}`;
  go.setAttribute("aria-label", idleLabel);
  more.title = t("video_more");
  more.setAttribute("aria-label", t("video_more"));
  closeBtn.title = `${t("video_close")} — ${t("video_hide_here")}`;
  closeBtn.setAttribute("aria-label", t("video_close"));
  item("download").textContent = idleLabel;
  item("hide").textContent = t("video_hide_here");
  // The site without its www.: the app's list takes a name's subdomains too (background.js
  // sends it so), so it stays off www., m. and the bare name alike.
  const site = host.replace(/^www\./i, "");
  item("never").textContent = ext.i18n.getMessage("video_never_site", [site]) || `Never on ${site}`;
  item("settings").textContent = t("video_settings");
  picks.setAttribute("aria-label", idleLabel);
  rows.setAttribute("aria-label", idleLabel);
  tabs.setAttribute("aria-label", idleLabel);
  tabs.dir = pill.dir;
  for (const b of tabList) b.textContent = t(b.dataset.tab === "audio" ? "video_tab_audio" : "video_tab_video");
  openRow.textContent = t("video_open_mdm");

  // ---- which video ----------------------------------------------------------------------------
  const videos = document.getElementsByTagName("video");
  const audios = document.getElementsByTagName("audio");

  // Videos inside open shadow roots (players built as web components), looked for again every
  // few seconds at most.
  let shadowed = [];
  let shadowedAt = 0;
  function shadowVideos() {
    if (Date.now() - shadowedAt < 3000) return shadowed;
    shadowedAt = Date.now();
    const found = [];
    const walk = (root, depth) => {
      if (depth > 4) return;
      const w = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
      for (let n = w.nextNode(), seen = 0; n && seen < 20000; n = w.nextNode(), seen++) {
        if (n.shadowRoot) {
          found.push(...n.shadowRoot.querySelectorAll("video"));
          walk(n.shadowRoot, depth + 1);
        }
      }
    };
    walk(document.documentElement, 0);
    shadowed = found;
    return found;
  }

  function eligible(v) {
    if (!v.isConnected || isDismissed(v)) return false;
    const r = v.getBoundingClientRect();
    const audio = v instanceof HTMLAudioElement;
    // Options → Download panels: over videos, over sound players, or one of them.
    if (!(audio ? panelOpts.audio : panelOpts.video)) return false;
    if (audio ? !v.controls || r.width < MIN_W || r.height < MIN_AUDIO_H : r.width < MIN_W || r.height < MIN_H) {
      return false;
    }
    // A short muted loop without controls: a moving picture, not something to save.
    if (v.muted && v.loop && !v.controls && !(v.duration > CLIP_S)) return false;
    // Shorter than Options' least length (known once the player has loaded it; a live stream's is
    // endless): no button.
    if (!longEnough(v.duration, panelOpts.minSeconds)) return false;
    const s = getComputedStyle(v);
    return s.visibility !== "hidden" && s.display !== "none" && Number(s.opacity) > 0.05;
  }

  // The topmost eligible video under (x, y): players lay their controls over the video, so the
  // video itself rarely gets the pointer's events.
  function videoAt(x, y) {
    let best = null;
    for (const v of [...videos, ...audios, ...shadowVideos()]) {
      const r = v.getBoundingClientRect();
      if (x < r.left || x > r.right || y < r.top || y > r.bottom || !eligible(v)) continue;
      if (!best || r.width * r.height < best.r.width * best.r.height) best = { v, r };
    }
    return best && best.v;
  }

  // ---- placing it -------------------------------------------------------------------------
  // `place` is where it sits in the room the video leaves around it: x and y from 0 to 1, one of
  // them 0 or 1 (on an edge).
  function rects() {
    const r = held ? held.r : (lastRect = video.getBoundingClientRect());
    const w = pill.offsetWidth || 150;
    const h = pill.offsetHeight || 32;
    return { r, w, h, roomX: Math.max(0, r.width - w - 2 * INSET), roomY: Math.max(0, r.height - h - 2 * INSET) };
  }

  // Is a control of the page under this spot (a viewport rectangle)? What counts is something
  // on top of the video that the user clicks and that is much smaller than the video: a button,
  // a small link, a watch-later or menu button on a preview (#421); a link or layer over the
  // whole video isn't one. On the video itself, and below it, is nothing to cover.
  function onControl(b, video, videoRect) {
    const area = (x) => Math.max(0, x.width) * Math.max(0, x.height);
    const small = (el) => area(el.getBoundingClientRect()) < area(videoRect) / 5;
    const points = [
      [(b.left + b.right) / 2, (b.top + b.bottom) / 2],
      [b.left + 4, b.top + 4],
      [b.right - 4, b.top + 4],
      [b.left + 4, b.bottom - 4],
      [b.right - 4, b.bottom - 4],
    ];
    for (const [x, y] of points) {
      for (const el of document.elementsFromPoint(x, y)) {
        if (el === root) continue;
        if (el === video || el.contains(video)) break;
        const control = el.closest("button, [role=button], a[href], input, select, [onclick]");
        if (control && !control.contains(video) && small(control)) return true;
        if (small(el) && getComputedStyle(el).cursor === "pointer") return true;
      }
    }
    return false;
  }

  // Where it goes now: the place the user gave it, else the default corner, or the next corner
  // free of the page's own controls (a thumbnail's watch-later button at the top right, #421).
  // Over the same video it only ever goes on to the next corner, never back to one it left: a
  // hover-to-play preview (YouTube's thumbnails, #560) shows its sound and captions buttons while
  // the pointer is over the video and hides them as the pointer reaches the button, so going back
  // when they hide, then away again when they show, kept the button jumping from under the pointer.
  const CORNERS = Object.values(CORNER_SPOTS);
  function spot(r, w, h, roomX, roomY) {
    if (chosen) return place;
    // Still while its menu is open: the player's own controls come and go as the pointer moves
    // over the video, and the menu's rows would move away from under it.
    if (menuOpen && auto.v === video && auto.at) return auto.at;
    const now = Date.now();
    const same = auto.v === video;
    if (same && now - auto.t < AUTO_MS) return auto.at;
    // Options' corner, then the others (top left, bottom left, bottom right).
    const order = [place, ...CORNERS.filter((c) => c !== place)];
    const from = same ? Math.max(0, order.indexOf(auto.at)) : 0;
    let at = same ? auto.at : place;
    for (const c of order.slice(from)) {
      const left = r.left + INSET + c.x * roomX;
      const top = r.top + INSET + c.y * roomY;
      if (!onControl({ left, top, right: left + w, bottom: top + h }, video, r)) {
        at = c;
        break;
      }
    }
    auto = { v: video, at, t: now };
    return at;
  }

  function position() {
    if (!video) return;
    const { r, w, roomX, roomY } = rects();
    // The pointer on it (or its menu open): it stays where it is, whatever the video does
    // meanwhile. A preview that grows while the pointer is over it and shrinks back when the
    // pointer goes (#560) would otherwise take the button from under the pointer.
    if (!dragging && pinned && pinned.v === video && (menuOpen || root.matches(":hover"))) {
      if (menuOpen) placeMenu(pinned.top, pinned.left, w);
      return;
    }
    // A narrow video gets the icon only.
    pill.classList.toggle("compact", r.width < 360);
    // Held for a video gone (#503): where it was, not a corner found anew over what the page
    // shows there now.
    const at = dragging ? dragging.at : held && !chosen ? held.at : spot(r, w, pill.offsetHeight || 32, roomX, roomY);
    // In the window, however narrow (a popup player) or far the video runs past its side.
    const left = Math.min(
      Math.max(r.left + INSET + at.x * roomX, Math.max(r.left, 0) + 2),
      Math.max(2, Math.min(r.right, innerWidth) - w - 2),
    );
    // In the part of the video on screen: one scrolled half out keeps its button in view.
    const h = pill.offsetHeight || 32;
    const visTop = Math.max(r.top, 0) + INSET;
    const visBottom = Math.min(r.bottom, innerHeight) - INSET - h;
    const top = Math.min(Math.max(r.top + INSET + at.y * roomY, visTop), Math.max(visTop, visBottom));
    root.style.setProperty("transform", `translate(${Math.round(left)}px, ${Math.round(top)}px)`, "important");
    pinned = { v: video, left, top };
    if (menuOpen) placeMenu(top, left, w);
  }

  // The open menu opens below the button, or above it near the bottom of the window; aligned
  // with whichever of its sides is nearer the video's middle.
  // In a small window (a popup player) it fits on the side with more room, scrolling when even
  // that is short, and never runs past the window's left or right edge.
  const EDGE = 6;
  function placeMenu(top, left, w) {
    panel.style.maxHeight = "";
    let h = panel.offsetHeight;
    const roomBelow = innerHeight - (top + 36) - EDGE;
    const roomAbove = top - 4 - EDGE;
    const below = h <= roomBelow || roomBelow >= roomAbove;
    const room = Math.max(80, below ? roomBelow : roomAbove);
    if (h > room) {
      panel.style.maxHeight = `${room}px`;
      panel.style.overflowY = "auto";
      h = room;
    } else if (panel === menu) {
      panel.style.overflowY = "";
    }
    panel.style.top = below ? "36px" : `${-h - 4}px`;
    const { r } = rects();
    const rightHalf = left + w / 2 > r.left + r.width / 2;
    panel.style.left = rightHalf ? "" : "0";
    panel.style.right = rightHalf ? "0" : "";
    const pw = panel.offsetWidth;
    const start = rightHalf ? left + w - pw : left;
    const fixed = Math.min(Math.max(start, EDGE), Math.max(EDGE, innerWidth - EDGE - pw));
    if (fixed !== start) {
      panel.style.left = `${Math.round(fixed - left)}px`;
      panel.style.right = "";
    }
  }

  // Snap a free spot (fractions, after a drag) to the nearest edge.
  function snap(at) {
    const d = { left: at.x, right: 1 - at.x, top: at.y, bottom: 1 - at.y };
    const edge = Object.keys(d).reduce((a, b) => (d[a] <= d[b] ? a : b));
    // Along the edge it stays where it was dropped, but close to a corner it goes into it.
    const c = (n) => (n < 0.08 ? 0 : n > 0.92 ? 1 : n);
    if (edge === "left") return { x: 0, y: c(at.y) };
    if (edge === "right") return { x: 1, y: c(at.y) };
    if (edge === "top") return { x: c(at.x), y: 0 };
    return { x: c(at.x), y: 1 };
  }

  // ---- showing and hiding ---------------------------------------------------------------------
  function attach() {
    // Over the video in fullscreen: inside the fullscreen element (a fullscreen <video> itself
    // can't hold it: then it hides).
    const fs = document.fullscreenElement;
    const parent = fs && fs !== video && fs.contains(video) ? fs : document.documentElement;
    if (root.parentNode !== parent) parent.appendChild(root);
    return !(fs && (fs === video || !fs.contains(video)));
  }

  function show(v) {
    if (video !== v) {
      video = v;
      held = null;
      closeMenu();
    }
    if (!attach()) return hide(true);
    root.style.setProperty("display", "block", "important");
    position();
    if (!pill.classList.contains("shown")) requestAnimationFrame(() => pill.classList.add("shown"));
    if (!raf) raf = requestAnimationFrame(tick);
  }

  // `now`: at once and whatever has the focus (dismissed, the video gone).
  function hide(now) {
    if (!now && (menuOpen || dragging || keyboardOn())) return;
    pinned = null;
    if (now) {
      held = null;
      closeMenu();
      shadow.activeElement?.blur();
    }
    pill.classList.remove("shown");
    cancelAnimationFrame(raf);
    raf = 0;
    const was = video;
    setTimeout(() => {
      if (!pill.classList.contains("shown") && video === was) root.style.setProperty("display", "none", "important");
    }, now || reduced ? 0 : 180);
  }

  // What the button sends for `v` (its rectangle `r`): see `download`.
  function describe(v, r) {
    const url = videoPage(v);
    // Its own plain file (not a stream the page assembles, blob:). Not beside a post a big site's
    // reader knows (#689): the app would take the file over the post, its title and qualities.
    const file = /^https?:/i.test(v.currentSrc || "") ? v.currentSrc : "";
    const post = Boolean(url) && url !== location.href && SITES.some((s) => s.hosts.test(siteName(host)));
    return {
      drm: Boolean(v.mediaKeys),
      src: post ? "" : file,
      url,
      // The middle of the video: where the background asks the page's own player what it plays.
      at: { x: r.left + r.width / 2, y: r.top + r.height / 2 },
    };
  }

  // The video just went from under the button (#503): what it was, kept. What was taken as the
  // pointer reached the button, when that was a moment ago; else the video as it is now.
  function hold() {
    const now = Date.now();
    const d = entered && entered.v === video && now - entered.t < 1000 ? entered.d : describe(video, lastRect);
    return { r: lastRect, at: chosen ? place : (auto.v === video && auto.at) || place, d, since: now };
  }

  // While shown: follow the video (scrolling, layout) and fade when the pointer rests.
  function tick() {
    raf = 0;
    if (!video) return hide(true);
    const onButton = root.matches(":hover");
    if (!video.isConnected || !eligible(video)) {
      if (!held && lastRect && !isDismissed(video)) held = hold();
      const r = held && held.r;
      const over =
        r && pointer.x >= r.left && pointer.x <= r.right && pointer.y >= r.top && pointer.y <= r.bottom;
      if (!held || !(onButton || menuOpen || dragging || keyboardOn() || (over && Date.now() - held.since < HOLD_MS))) {
        const was = held;
        held = null;
        video = null;
        // Left after it was held: it fades, as when the pointer leaves a video.
        return was ? hide() : hide(true);
      }
      position();
      raf = requestAnimationFrame(tick);
      return;
    }
    held = null;
    const r = video.getBoundingClientRect();
    const over =
      pointer.x >= r.left && pointer.x <= r.right && pointer.y >= r.top && pointer.y <= r.bottom;
    // The qualities' menu for a video this element no longer plays, or a page left: it goes.
    if (menuOpen && panel === picks && !(job && current(job))) closeMenu();
    // What was asked ahead, for another page or video: called off. The pointer resting over the
    // video a moment: its qualities asked ahead (on the video's own page only, askAhead).
    checkAhead();
    const resting = `${location.href}\n${video.currentSrc}`;
    if (dwell.v !== video || dwell.key !== resting) dwell = { v: video, key: resting, since: Date.now(), done: false };
    else if (!dwell.done && over && Date.now() - dwell.since >= AHEAD_DWELL_MS) {
      dwell.done = true;
      askAhead(video, "rest");
    }
    const near =
      pointer.x >= r.left - NEAR_PX && pointer.x <= r.right + NEAR_PX &&
      pointer.y >= r.top - NEAR_PX && pointer.y <= r.bottom + NEAR_PX;
    const now = Date.now();
    if (near || onButton) nearAt = now;
    const stays = near || now - nearAt < LINGER_MS;
    if (!onButton && !menuOpen && !dragging && !keyboardOn() && (!stays || now - lastMove > IDLE_MS)) {
      return hide();
    }
    position();
    raf = requestAnimationFrame(tick);
  }

  // At most one look per frame, however fast the pointer moves.
  function onMove(e) {
    pointer = { x: e.clientX, y: e.clientY };
    lastMove = Date.now();
    if (!scan) scan = requestAnimationFrame(look);
  }

  function look() {
    scan = 0;
    if (dragging || never || (allowed === false && answered())) return;
    // Kept on its video while the qualities' menu is open (pointerTarget).
    const v = pointerTarget(videoAt(pointer.x, pointer.y), video, menuOpen && panel === picks);
    if (!v) return;
    if (!answered()) return void ask().then(() => allowed && look());
    show(v);
  }

  // One question at a time; while it is out, the button waits for its answer.
  function ask() {
    if (never) {
      allowed = false;
      return Promise.resolve();
    }
    if (answered()) return Promise.resolve();
    asking = asking || send({ type: "panel" }).then(async (r) => {
      allowed = !never && Boolean(r && r.on);
      askedAt = Date.now();
      if (allowed) {
        panelOpts = panelLook(r);
        // Options' corner, unless it was dragged somewhere on this site (below).
        if (!chosen) place = CORNER_SPOTS[panelOpts.corner];
      }
      // Switched off meanwhile: gone now, unless the menu or a drag has it in hand.
      if (!allowed && video && !menuOpen && !dragging) {
        video = null;
        hide(true);
      }
      if (allowed && !placed) {
        placed = true;
        try {
          const { positions } = await ext.storage.local.get({ positions: {} });
          if (positions[host]) {
            place = snap(positions[host]);
            chosen = true;
          }
        } catch {
          // Default spot.
        }
      }
    }).finally(() => (asking = null));
    return asking;
  }

  // ---- acting ---------------------------------------------------------------------------------
  // The middle of the video: where the background asks the page's own player what it plays.
  function middle(v) {
    const box = v.getBoundingClientRect();
    return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  }

  // A promise's answer, or {late: true} once `ms` is up.
  const within = (promise, ms) => {
    let timer = 0;
    const late = new Promise((done) => (timer = setTimeout(() => done({ late: true }), Math.max(0, ms))));
    return Promise.race([promise, late]).finally(() => clearTimeout(timer));
  };

  // What the button asks the app about, for video `v` as `describe` found it (`d`): the video's
  // file or stream, its page and where it is, the page's title (a page that moved in place keeps
  // its first page's og:title; its document title follows), and the player's length (an HLS
  // stream's master playlist has none, for the page's own qualities).
  function askedOf(d, v) {
    const og = location.href === landedAt ? document.querySelector('meta[property="og:title"]')?.content : "";
    const title = (og || document.title || "").trim();
    const duration = v && Number.isFinite(v.duration) && v.duration > 0 ? v.duration : 0;
    return { src: d.src, title, url: d.url, at: d.at, ...(duration ? { duration } : {}) };
  }

  // The lookup's question to the app, the click's or one asked ahead (`prefetch`): the one place
  // it is asked.
  const askLookup = (asked, prefetch) => send({ type: "video-lookup", ...asked, ...(prefetch ? { prefetch: true } : {}) });

  // What a lookup asked ahead is for: the page, what the player plays, and what is asked about it.
  const aheadKey = (v, asked) => JSON.stringify([location.href, v.currentSrc || "", asked.url, asked.src]);

  // The sound's rows' names in the page's own list, as the app names them.
  const audioLabel = (kind, type) => tWith(kind === "original" ? "video_audio_original" : "video_audio_only", type);

  // The click: the app looks the video up and its qualities open in a menu here. An app that can't
  // (an older one, another DDM chosen) gets the click as it always was: its Download video window.
  async function download() {
    // The button again while its qualities are open: they close.
    if (menuOpen && panel === picks) return closeMenu();
    if (busy || !video) return;
    busy = true;
    try {
      closeMenu();
      // Asked now, not kept: one <video> plays many videos at the same address (a feed's
      // previews). A video gone from under the button: what it played then (#503).
      const d = held ? held.d : describe(video, video.getBoundingClientRect());
      // Encrypted (DRM): no tool can save it; say so rather than send it.
      if (d.drm) return flash("err", t("video_drm"));
      const j = { video, src: video.currentSrc || "", href: location.href, asked: askedOf(d, video), started: Date.now(), token: "", count: 0 };
      job = j;
      clicked = { key: aheadKey(video, j.asked), at: j.started };
      // The menu opens at once, saying it is looking; an answer that needs none and comes first
      // (no lookups here: the window) shows none.
      const opening = setTimeout(() => openPicks(j), 100);
      // Asked ahead already (the pointer rested on the button, or over the video on its page):
      // that answer, as soon as it is there; else asked now.
      const early = await takeAhead(j);
      const r = early.reply || (await within(askLookup(j.asked), LOOKUP_MS));
      clearTimeout(opening);
      await firstAnswer(j, r, (r && r.page) || early.page || null);
    } finally {
      busy = false;
    }
  }

  // What the lookup's first answer `r` comes to (the click's own, or the one asked ahead): the
  // menu follows it, and shows the page's own qualities (`page`, pageChoices) at once while the
  // app looks, from an app that says how it will list them (`merge`).
  async function firstAnswer(j, r, page) {
    const s = replyState(r);
    if (!s || !current(j)) return;
    const a = lookupAction(s, t);
    // No lookups here, or an answer that isn't one: the click as it was, once.
    if (a.act === "window") return await useWindow(j);
    // A video it can't tell, or the button off here: a word on the button, no menu.
    if (a.act === "flash") {
      closeMenu();
      dropLookup();
      return flash(a.kind, a.text);
    }
    openPicks(j);
    if (a.act === "follow") offerPage(j, pageChoices(page, s.merge, audioLabel));
    follow(j, s);
  }

  // The app forgot the lookup asked ahead (it restarted, or kept it past its ten minutes): asked
  // again now, once.
  async function relook(j) {
    j.retried = true;
    j.ahead = false;
    const r = await within(askLookup(j.asked), Math.max(0, j.started + LOOKUP_MS - Date.now()));
    return firstAnswer(j, r, (r && r.page) || null);
  }

  // ---- asking ahead -----------------------------------------------------------------------------
  // The app's lookup takes seconds (yt-dlp reads the page), so it is asked before the click: when
  // the pointer reaches the button ("hover", anywhere), or rests over the video on that video's own
  // page ("rest": a watch page, never a feed). One at a time; called off when the page or the video
  // changes (the app then stops it, while nothing else holds it), never from a private window, and
  // the app starts one only while no other lookup runs. The click takes it (takeAhead).
  function askAhead(v, why) {
    if (aheadOff || privateWindow || never || allowed !== true || !v || !v.isConnected || (menuOpen && panel === picks)) return;
    checkAhead();
    // Still waiting for the last one's answer.
    if (aheadJob && !aheadJob.reply) return;
    const d = describe(v, v.getBoundingClientRect());
    if (d.drm || (!d.url && !d.src)) return;
    const own = { many: document.querySelectorAll("video").length > 1, main: Boolean(v.closest && v.closest("#movie_player")), feed: inFeed(v) || FEED_PATH.test(location.pathname) };
    if (why === "rest" && !asksAhead(location.href, d.url, own)) return;
    const asked = askedOf(d, v);
    const key = aheadKey(v, asked);
    // Asked already: taken while fresh; one the app didn't take (busy) asked again after a while.
    if (aheadJob && aheadJob.key === key && Date.now() - aheadJob.at < (aheadJob.usable ? AHEAD_KEEP_MS : 15000)) return;
    // Clicked already: the app has it (a click shares it), nothing to ask ahead.
    if (clicked && clicked.key === key && Date.now() - clicked.at < AHEAD_KEEP_MS) return;
    dropAhead();
    const a = { v, src: v.currentSrc || "", href: location.href, asked, key, at: Date.now(), reply: null, token: "", usable: false, taken: false };
    a.promise = askLookup(asked, true).then((r) => {
      a.reply = r || {};
      // No lookups here: not asked ahead again on this page.
      if (!r || r.fallback || r.gone) aheadOff = true;
      const s = r && r.lookup && typeof r.lookup === "object" ? r.lookup : {};
      a.token = typeof s.token === "string" ? s.token : "";
      a.usable = Boolean(a.token) && (s.state === "pending" || s.state === "ready");
      // Called off while it was asked (another page, another video): its lookup too.
      if (aheadJob !== a && !a.taken && a.token) send({ type: "video-cancel", token: a.token });
      return r;
    });
    aheadJob = a;
    watchAhead();
  }

  // The lookup asked ahead is no longer wanted (its page or its video changed, the button went):
  // called off, unless a click took it. One still being asked is called off as its answer comes.
  function dropAhead() {
    const a = aheadJob;
    aheadJob = null;
    clearTimeout(aheadTimer);
    if (a && !a.taken && a.token) send({ type: "video-cancel", token: a.token });
  }

  // Still the page, the video and the time it was asked for? Else called off.
  function checkAhead() {
    const a = aheadJob;
    if (a && (location.href !== a.href || !a.v.isConnected || (a.src && a.v.currentSrc !== a.src) || Date.now() - a.at > AHEAD_KEEP_MS)) dropAhead();
  }

  // Looked at every second while one is asked ahead: a page that moves in place says nothing.
  function watchAhead() {
    clearTimeout(aheadTimer);
    aheadTimer = setTimeout(() => {
      checkAhead();
      if (aheadJob) watchAhead();
    }, 1000);
  }

  // The lookup asked ahead for the clicked video, if any, and still the same video on the same
  // page: {reply} when the app took it (`pending` or `ready`, with its token), and the page's
  // qualities it came with either way ({page}). It is the click's from now on.
  async function takeAhead(j) {
    const a = aheadJob;
    if (!a || a.v !== j.video || a.src !== j.src || a.href !== j.href || a.asked.url !== j.asked.url || a.asked.src !== j.asked.src || Date.now() - a.at > AHEAD_KEEP_MS) return {};
    a.taken = true;
    aheadJob = null;
    clearTimeout(aheadTimer);
    const r = await a.promise;
    const s = r && r.lookup && typeof r.lookup === "object" ? r.lookup : {};
    j.ahead = Boolean(typeof s.token === "string" && s.token && (s.state === "pending" || s.state === "ready"));
    return { reply: j.ahead ? r : null, page: (r && r.page) || null };
  }

  // Still the lookup the menu is for: the menu open on the same video (the same element playing
  // the same thing) on the same page. Whatever answers a lookup after that is dropped.
  const current = (j) =>
    job === j && video === j.video && (!j.src || video.currentSrc === j.src) && location.href === j.href;

  function openPicks(j) {
    if (!current(j) || (menuOpen && panel === picks)) return;
    closeMenu();
    panel = picks;
    menuOpen = true;
    clearRows();
    said(t("video_looking"));
    picks.setAttribute("aria-busy", "true");
    picks.classList.add("open");
    go.setAttribute("aria-expanded", "true");
    lastMove = Date.now();
    position();
    picks.focus({ preventScroll: true });
  }

  // No rows, no tabs, nothing being checked.
  function clearRows() {
    rows.replaceChildren();
    tabs.hidden = true;
    qualities.setAttribute("role", "none");
    qualities.removeAttribute("aria-labelledby");
    picks.classList.remove("refining");
    live.textContent = "";
  }

  // The menu's message (plain text), with the app's own words under it.
  function said(text, detail) {
    note.textContent = text;
    if (detail) {
      const why = document.createElement("span");
      why.className = "why";
      why.textContent = String(detail).slice(0, 400);
      note.append(why);
    }
    note.hidden = !text;
  }

  // The keyboard's focus goes to the menu's new rows while it is in the menu or on the button
  // (not when it was taken elsewhere meanwhile), and only for someone using the keyboard: moved
  // there after a mouse click, it made the button look keyboard-focused once the menu closed, and
  // the button then stayed after the pointer left (an answer that came before the click, asked
  // ahead, showed it).
  const focusHere = () => {
    const a = shadow.activeElement;
    return lastInput === "key" && Boolean(a) && (a === go || picks.contains(a));
  };

  // A message instead of qualities, "Open in…" under it. A row picked from the page's own list
  // meanwhile is not sent.
  function say(j, text, detail) {
    if (!current(j)) return;
    picks.setAttribute("aria-busy", "false");
    j.provisional = false;
    j.wanted = null;
    clearRows();
    said(text, detail);
    position();
    if (focusHere()) openRow.focus();
  }

  // The lookup's end, in the menu (lookupAction).
  function settle(j, s) {
    if (!current(j)) return;
    if (s && s.state === "gone" && j.ahead && !j.retried) return void relook(j);
    const a = lookupAction(s, t);
    if (a.act === "offer") return offer(j, s);
    if (a.act === "say") return say(j, a.text, a.detail);
    if (a.act === "flash") {
      closeMenu();
      dropLookup();
      return flash(a.kind, a.text);
    }
    // "use-window", "gone" (the app forgot it: restarted) and anything newer: the window.
    return useWindow(j);
  }

  // The rows for these qualities (their numbers in the menu's list, `j.rows`), a row each: the
  // quality's name and its size, as text. A row already there for the same quality (`key`) stays
  // where it is and is only brought up to date (the app's list taking the page's place): the one
  // under the pointer or with the keyboard's focus is never moved nor made anew.
  function showRows(j, numbers) {
    const old = new Map([...rows.children].map((b) => [b.dataset.key, b]));
    const els = numbers.map((i) => {
      const row = j.rows[i];
      const { name, size } = rowText(row, lang);
      let b = old.get(row.key);
      if (b) {
        old.delete(row.key);
      } else {
        b = document.createElement("button");
        b.type = "button";
        b.className = "pick";
        b.setAttribute("role", "menuitem");
        const n = document.createElement("span");
        n.className = "name";
        b.append(n);
      }
      b.dataset.row = String(i);
      b.dataset.key = row.key;
      b.firstElementChild.textContent = name;
      let z = b.querySelector(".size");
      if (size && !z) {
        z = document.createElement("span");
        z.className = "size";
        b.append(z);
      }
      if (z && size) z.textContent = size;
      else if (z) z.remove();
      const waiting = Boolean(j.wanted) && j.wanted === row.key;
      b.classList.toggle("waiting", waiting);
      if (waiting) b.setAttribute("aria-busy", "true");
      else b.removeAttribute("aria-busy");
      return b;
    });
    for (const b of old.values()) b.remove();
    els.forEach((b, k) => {
      if (rows.children[k] !== b) rows.insertBefore(b, rows.children[k] || null);
    });
  }

  // One of the two tabs shown: it is the selected one (the only one in the tab order), its
  // qualities are the rows.
  function showTab(j, kind) {
    for (const b of tabList) {
      const on = b.dataset.tab === kind;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
    }
    qualities.setAttribute("aria-labelledby", `tab-${kind}`);
    j.tab = kind;
    showRows(j, j.groups[kind]);
    position();
  }

  // The menu's rows become `list` (pageChoices, then appRows): in one list, or in the Video and
  // Audio tabs when there are both (the tab shown stays). What is under the pointer (a row, a tab,
  // "Open in…"), else what has the keyboard's focus where it can be seen, else the first thing
  // seen in the menu's scrolled view, keeps its place on the screen; while the pointer is on the
  // menu, the menu keeps its height (rows that came scroll), so nothing moves under it. True when
  // what is under the pointer stayed put; else a click waits a moment (`armed`), so it never lands
  // on what just came under it.
  function applyRows(j, list) {
    const focused = shadow.activeElement;
    const view = picks.getBoundingClientRect();
    const seen = (b) => {
      const r = b.getBoundingClientRect();
      return r.height > 0 && r.top >= view.top - 1 && r.bottom <= view.bottom + 1;
    };
    const within = (b) => Boolean(b) && b !== picks && picks.contains(b) && seen(b);
    const hovered = picks.querySelector("button:hover");
    const anchor = within(hovered) ? hovered : within(focused) ? focused : [...picks.querySelectorAll("button")].find((b) => !b.closest("[hidden]") && seen(b)) || null;
    // A row by its quality (it may be made anew in another tab), anything else by itself.
    const key = anchor && anchor.dataset.key;
    const find = () => (key ? [...rows.children].find((b) => b.dataset.key === key) : anchor && anchor.isConnected ? anchor : null);
    const before = anchor ? anchor.getBoundingClientRect().top : 0;
    const pointed = picks.matches(":hover");
    if (pointed) picks.style.height = `${picks.offsetHeight}px`;
    j.rows = list;
    j.groups = groupChoices(list, list.length);
    if (j.groups.tabbed) {
      tabs.hidden = false;
      qualities.setAttribute("role", "tabpanel");
      showTab(j, j.groups[j.tab] && j.groups[j.tab].length ? j.tab : j.groups.start);
    } else {
      tabs.hidden = true;
      qualities.setAttribute("role", "none");
      qualities.removeAttribute("aria-labelledby");
      j.tab = "";
      showRows(j, j.groups.all);
    }
    const kept = find();
    if (kept) picks.scrollTop += kept.getBoundingClientRect().top - before;
    const steady = Boolean(kept) && pointed && Math.abs(kept.getBoundingClientRect().top - before) < 1;
    if (!steady) j.armed = Date.now() + 300;
    // The row with the keyboard's focus gone (the app doesn't have that quality): the first one.
    if (focused && focused.classList && focused.classList.contains("pick") && !focused.isConnected) (rows.firstElementChild || openRow).focus();
    return steady;
  }

  // The page's own qualities (pageChoices), shown at once while the app looks, a line moving along
  // the menu's top edge: a row picked meanwhile waits for the app's list (wantRow).
  function offerPage(j, list) {
    if (!current(j) || !list.length || j.count) return;
    j.provisional = true;
    picks.classList.add("refining");
    // The message goes first: the rows take its place where applyRows measures them.
    said("");
    applyRows(j, list);
    position();
    if (focusHere()) (rows.firstElementChild || openRow).focus();
  }

  // The app's qualities, in the page's own rows' place or in an empty menu. A row picked from the
  // page's list meanwhile is sent now when the app has that quality; else the menu says so.
  function offer(j, s) {
    j.count = Math.min(s.choices.length, 100);
    j.token = s.token;
    const was = Boolean(j.provisional);
    j.provisional = false;
    picks.classList.remove("refining");
    said("");
    applyRows(j, appRows(s.choices, j.count));
    picks.setAttribute("aria-busy", "false");
    if (j.wanted) {
      const row = j.rows.find((r) => r.key === j.wanted);
      j.wanted = null;
      live.textContent = "";
      if (row) return void sendPick(j, row.choice);
      said(t("video_pick_gone"));
      j.armed = Date.now() + 300;
    }
    // Rows that just pushed "Open in…" down under the pointer aren't picked by a click meant for
    // it: applyRows holds clicks a moment unless what is under the pointer stayed put.
    position();
    if (!was && focusHere()) (rows.firstElementChild || openRow).focus();
  }

  // A row of the page's own list picked while the app looks: it waits, marked, for the app's list
  // (offer); another row picked meanwhile takes its place.
  function wantRow(j, key) {
    j.wanted = key;
    for (const b of rows.children) {
      const on = b.dataset.key === key;
      b.classList.toggle("waiting", on);
      if (on) b.setAttribute("aria-busy", "true");
      else b.removeAttribute("aria-busy");
    }
    live.textContent = t("video_pick_waiting");
  }

  // Asks how the lookup is going until it ends (followLookup), then shows what it came to.
  async function follow(j, first) {
    const s = await followLookup(first, {
      ask: (token, ms) => within(send({ type: "video-poll", token }), ms),
      alive: () => current(j),
      sleep: (ms) =>
        new Promise((wake) => {
          j.wake = wake;
          j.timer = setTimeout(wake, ms);
        }),
      now: Date.now,
      end: j.started + LOOKUP_MS,
    });
    if (!s || !current(j)) return;
    settle(j, s);
  }

  // The lookup is no longer wanted (its menu closed): no more questions about it.
  function dropLookup() {
    const j = job;
    job = null;
    if (!j) return;
    clearTimeout(j.timer);
    if (j.wake) j.wake();
  }

  // A row picked (its number in the menu's list): a quality of the app's list is sent; one of the
  // page's own list waits for the app's (wantRow).
  function pick(j, n) {
    if (!current(j) || j.picked || Date.now() < j.armed) return;
    const row = j.rows && j.rows[n];
    if (!row) return;
    if (row.choice < 0) return wantRow(j, row.key);
    return sendPick(j, row.choice);
  }

  // A quality picked: the app downloads it (its own choice of it, with what it looked the video
  // up with) without its window. Only the token and its place in the app's list go.
  async function sendPick(j, i) {
    if (!current(j) || !j.count || j.picked || !(i >= 0 && i < j.count)) return;
    j.picked = true;
    closeMenu();
    busy = true;
    try {
      const s = replyState(await send({ type: "video-pick", token: j.token, choice: i }));
      if (!s) return;
      // Downloading, queued, or handed to the app ("sent"): a word on the button. Anything else
      // (the lookup forgotten or another window's, an older app): the window.
      const a = lookupAction(s, t, true);
      if (a.act === "flash") return flash(a.kind, a.text);
      return await useWindow(j);
    } finally {
      busy = false;
    }
  }

  // The click as it always was: the app's Download video window for this video. Once per click.
  async function useWindow(j) {
    if (job === j) {
      closeMenu();
      dropLookup();
    }
    if (!firstWindow(j)) return;
    // Where the video is now, for the background to ask the page's player there.
    const r = await send({ type: "video", ...j.asked, ...(j.video.isConnected ? { at: middle(j.video) } : {}) });
    if (r.gone) return;
    const why = { options: "video_off", unnamed: "video_unnamed", feed: "video_open_post" }[r.why];
    flash(r.ok ? "ok" : "err", t(r.ok ? "video_sent" : why || "video_failed"));
  }

  // A row of the qualities' menu, clicked or by key.
  function choosePick(b) {
    const j = job;
    if (!j || !b) return;
    if (b.dataset.act === "window") return useWindow(j);
    // A tab: its qualities are the rows now (the keyboard stays on the tab).
    if (b.dataset.tab !== undefined) {
      if (j.groups && j.groups.tabbed && j.groups[b.dataset.tab] && j.rows) showTab(j, b.dataset.tab);
      return;
    }
    if (b.dataset.row !== undefined) return pick(j, Number(b.dataset.row));
  }

  // A short confirmation on the button itself, then back to normal.
  function flash(kind, text) {
    flashing = true;
    pill.classList.remove("ok", "err");
    pill.classList.add(kind);
    label.textContent = text;
    lastMove = Date.now();
    position();
    setTimeout(() => {
      flashing = false;
      pill.classList.remove(kind);
      label.textContent = idleLabel;
      if (video) position();
    }, 2200);
  }

  function openMenu() {
    closeMenu();
    panel = menu;
    menuOpen = true;
    menu.classList.add("open");
    more.setAttribute("aria-expanded", "true");
    position();
    menu.querySelector("button").focus();
  }

  // Closes whichever menu is open; the qualities' takes its lookup with it.
  function closeMenu() {
    if (!menuOpen) return;
    const back = panel === picks && picks.contains(shadow.activeElement);
    menuOpen = false;
    panel.classList.remove("open");
    more.setAttribute("aria-expanded", "false");
    if (panel !== picks) return;
    go.setAttribute("aria-expanded", "false");
    picks.setAttribute("aria-busy", "false");
    picks.style.height = "";
    dropLookup();
    // The keyboard's focus back on the button, not lost with the menu (the mouse's goes with it).
    if (back && lastInput === "key") go.focus();
    else if (back) shadow.activeElement.blur();
  }

  menu.addEventListener("click", (e) => {
    if (!e.isTrusted) return;
    const act = e.target.closest("[data-act]")?.dataset.act;
    if (act) run(act);
  });

  async function run(act) {
    if (act === "download") return download();
    closeMenu();
    if (act === "hide") {
      held = null;
      dismissed.add(video);
      if (video.currentSrc) dismissedSrc.add(video.currentSrc);
      video = null;
      return hide(true);
    }
    if (act === "never") {
      never = true;
      allowed = false;
      video = null;
      hide(true);
      return send({ type: "hide-site" });
    }
    if (act === "settings") {
      // Options open in DDM; when it can't be reached, the button says so (nothing opened).
      const r = await send({ type: "options" });
      if (!r.ok && !r.gone && video) flash("err", t("video_failed"));
    }
  }

  more.addEventListener("click", () => (menuOpen && panel === menu ? closeMenu() : openMenu()));
  // X: gone from this video (as the menu's "Hide on this video"); the next video gets it again.
  closeBtn.addEventListener("click", (e) => {
    if (e.isTrusted && video) run("hide");
  });

  // The qualities' rows: only the user's own clicks.
  picks.addEventListener("click", (e) => {
    if (!e.isTrusted) return;
    choosePick(e.target.closest("button"));
  });
  // The pointer off the menu: it may take the height its rows need again (applyRows held it).
  picks.addEventListener("pointerleave", () => {
    if (!picks.style.height) return;
    picks.style.height = "";
    position();
  });

  // The pointer reached the button: what the video plays now, kept in case the page stops it for
  // that (a hover-to-play preview, #503); and its qualities asked ahead, for the click.
  root.addEventListener("pointerenter", () => {
    if (video && !held && video.isConnected && eligible(video)) {
      entered = { v: video, d: describe(video, video.getBoundingClientRect()), t: Date.now() };
      askAhead(video, "hover");
    }
  });

  // Keys (only the user's own, never ones a page script makes up): Escape closes the open menu
  // (the keyboard's focus back on its button), arrows move through it, Enter and Space run the
  // focused command. Each stops there: a key the open menu used is not the page's too (a player's
  // Space and arrows would pause it and change its volume behind the menu).
  shadow.addEventListener("keydown", (e) => {
    if (!e.isTrusted) return;
    const focused = shadow.activeElement;
    if (e.key === "Escape" && menuOpen) {
      const opener = panel === picks ? go : more;
      closeMenu();
      opener.focus();
      e.stopPropagation();
    } else if (menuOpen && (e.key === "Enter" || e.key === " ") && focused && focused !== panel && panel.contains(focused)) {
      // The focused command, whatever the browser does with a key on a button.
      if (panel === picks) choosePick(focused);
      else run(focused.dataset.act);
      e.preventDefault();
      e.stopPropagation();
    } else if (menuOpen && panel === picks && !tabs.hidden && focused && focused.dataset.tab !== undefined && ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
      // Between the tabs (not on to the rows); the arrows follow the reading direction.
      const i = tabList.indexOf(focused);
      const step = (e.key === "ArrowRight") === (pill.dir !== "rtl") ? 1 : -1;
      const next = e.key === "Home" ? 0 : e.key === "End" ? tabList.length - 1 : (i + step + tabList.length) % tabList.length;
      tabList[next].focus();
      e.preventDefault();
      e.stopPropagation();
    } else if (menuOpen && panel === picks && !tabs.hidden && focused && (e.key === "ArrowDown" || e.key === "ArrowUp") && (focused.dataset.tab !== undefined || (e.key === "ArrowUp" && focused === rows.firstElementChild))) {
      // From a tab down to the rows; up from the first row to the selected tab.
      if (focused.dataset.tab !== undefined) {
        if (e.key === "ArrowDown") (rows.firstElementChild || openRow).focus();
      } else {
        (tabList.find((b) => b.tabIndex === 0) || tabList[0]).focus();
      }
      e.preventDefault();
      e.stopPropagation();
    } else if (menuOpen && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      // The rows (and "Open in…"), never the tabs.
      const items = [...panel.querySelectorAll("button:not([data-tab])")];
      const i = items.indexOf(focused);
      const down = e.key === "ArrowDown";
      // From the menu itself (the qualities' while it looks): the first, or the last.
      const next = i < 0 ? (down ? 0 : items.length - 1) : (i + (down ? 1 : items.length - 1)) % items.length;
      items[next]?.focus();
      e.preventDefault();
      e.stopPropagation();
    }
  });

  // The main part: a click downloads, a drag moves it (and is not a click).
  // Only the user's own clicks and keys (never events a page script makes up).
  go.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !video || !e.isTrusted) return;
    const { r, roomX, roomY } = rects();
    dragging = {
      id: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      // Where in the button it was taken.
      dx: e.clientX - (r.left + INSET + (chosen ? place : auto.at || place).x * roomX),
      dy: e.clientY - (r.top + INSET + (chosen ? place : auto.at || place).y * roomY),
      at: chosen ? place : auto.at || place,
      moved: false,
    };
    go.setPointerCapture(e.pointerId);
    e.preventDefault();
  });
  go.addEventListener("pointermove", (e) => {
    if (!dragging || e.pointerId !== dragging.id) return;
    if (!dragging.moved && Math.hypot(e.clientX - dragging.startX, e.clientY - dragging.startY) < DRAG_PX) return;
    if (!dragging.moved) {
      dragging.moved = true;
      closeMenu();
      pill.classList.add("dragging");
    }
    const { r, roomX, roomY } = rects();
    const c = (n) => Math.min(1, Math.max(0, n));
    dragging.at = {
      x: roomX ? c((e.clientX - dragging.dx - r.left - INSET) / roomX) : 0,
      y: roomY ? c((e.clientY - dragging.dy - r.top - INSET) / roomY) : 0,
    };
    position();
  });
  const endDrag = (e) => {
    if (!dragging || e.pointerId !== dragging.id) return;
    const d = dragging;
    dragging = null;
    pill.classList.remove("dragging");
    if (!d.moved) {
      if (e.type === "pointerup") download();
      return;
    }
    place = snap(d.at);
    chosen = true;
    // It snaps to its corner although the pointer is still on it (#560's pin is for a video
    // that changes under a still pointer, not for the user's own move).
    pinned = null;
    position();
    lastMove = Date.now();
    // Remembered per site, not in a private window (it would keep the site's name), and the
    // 200 sites used last only.
    if (ext.extension?.inIncognitoContext) return;
    try {
      ext.storage.local
        .get({ positions: {} })
        .then(({ positions }) => {
          delete positions[host];
          positions[host] = place;
          const sites = Object.keys(positions);
          for (const s of sites.slice(0, Math.max(0, sites.length - 200))) delete positions[s];
          return ext.storage.local.set({ positions });
        })
        .catch(() => {});
    } catch {
      teardown();
    }
  };
  go.addEventListener("pointerup", endDrag);
  go.addEventListener("pointercancel", endDrag);
  // Keyboard: Enter / Space (a real button's click without a pointer).
  go.addEventListener("click", (e) => {
    if (e.detail === 0 && e.isTrusted) download();
  });

  // ---- wiring -----------------------------------------------------------------------------------
  // The extension updated or removed under this page: the button and its listeners go (a fresh
  // copy of this script comes with the new extension).
  function teardown() {
    dropLookup();
    // Cut off from the extension: nothing more can be said to the app (dropAhead's message goes
    // nowhere), and the background calls it off with the tab's next page.
    aheadJob = null;
    clearTimeout(aheadTimer);
    cancelAnimationFrame(raf);
    cancelAnimationFrame(scan);
    removeEventListener("pointermove", onMove, true);
    removeEventListener("keydown", onTab, true);
    root.remove();
    never = true;
    allowed = false;
    video = null;
  }
  globalThis.__mdmVideoButton = { alive: () => Boolean(ext.runtime?.id), teardown };
  addEventListener("pointermove", onMove, { capture: true, passive: true });
  addEventListener("pointerdown", (e) => {
    if (e.isTrusted) lastInput = "pointer";
    if (menuOpen && !e.composedPath().includes(root)) closeMenu();
  }, { capture: true, passive: true });
  // The keyboard in use (any key, in the page or the button's menu): see focusHere.
  addEventListener("keydown", (e) => {
    if (e.isTrusted) lastInput = "key";
  }, { capture: true, passive: true });
  // A video with the keyboard's focus, or a player around one (its controls), gets the button
  // too, kept while the focus stays there; Tab goes from there to the button.
  addEventListener("focusin", (e) => {
    if (!(e.target instanceof Element) || e.composedPath().includes(root)) return;
    const t = e.target;
    const v =
      t instanceof HTMLMediaElement ? t : [...t.querySelectorAll("video, audio")].find(eligible) || null;
    if (!v || !eligible(v)) return;
    focusedPlayer = t;
    ask().then(() => {
      if (!allowed) return;
      lastMove = Date.now();
      show(v);
    });
  }, true);
  // Tab on the focused player moves into the button first; Tab from the button goes back to
  // the player and on from there (its own controls next), as if the button sat at its start.
  let tabOn = 0;
  function onTab(e) {
    if (e.key !== "Tab" || e.shiftKey) return;
    // The Tab that just left the button: on from the player, as the browser goes.
    if (Date.now() - tabOn < 100) {
      tabOn = 0;
      return;
    }
    if (video && playerFocused() && root.style.getPropertyValue("display") === "block") {
      go.focus();
      e.preventDefault();
    }
  }
  addEventListener("keydown", onTab, true);
  go.addEventListener("keydown", (e) => {
    if (e.key === "Tab" && !e.shiftKey && focusedPlayer && focusedPlayer.isConnected) {
      tabOn = Date.now();
      focusedPlayer.focus();
    }
  });
  document.addEventListener("fullscreenchange", () => video && show(video));
  // Not on the window's blur: a page in a window without the focus (a popup player beside
  // another window, another browser's window in front) still gets the pointer, and the button
  // with it. A hidden page (another tab, minimised) has it go.
  document.addEventListener("visibilitychange", () => document.hidden && video && hide(true));
  // The pointer out of the window: no longer over any video (the button lingers, then fades).
  document.addEventListener("pointerout", (e) => {
    if (!e.relatedTarget && e.isTrusted) pointer = { x: -1e6, y: -1e6 };
  }, { passive: true });
  // The page left: what was asked ahead for it is called off (the background does it too, with
  // the tab's next page).
  addEventListener("pagehide", () => dropAhead());
  // The keyboard on the button: as the pointer reaching it.
  go.addEventListener("focus", () => {
    if (video && !held && go.matches(":focus-visible")) askAhead(video, "hover");
  });
})();
