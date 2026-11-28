// Dannex Download Manager: what a page's own players do out of the content scripts' sight (a
// content script in the page's own world, at its start, in every frame; video.js, in the
// extension's world, can't see either):
//  - a player the page keeps out of the document (`new Audio()`, as music sites, podcasts and radio
//    pages play their sound) or in a shadow root: its address, its length, whether it plays, each
//    time that changes;
//  - a player given MediaKeys (EME: the page decrypts copy-protected media, DRM; Spotify's, Netflix's).
// Each is told to video.js as a "ddm-media" event on the window, its detail a JSON string. Nothing
// else: the two methods it wraps do what they did, with the same `this`, arguments and result, and a
// page's own scripts see nothing new but that event.
(() => {
  const P = globalThis.HTMLMediaElement && globalThis.HTMLMediaElement.prototype;
  if (!P || typeof P.play !== "function") return;
  // Once per page, whichever copy of the extension came first (two copies: the store's and one
  // loaded from DDM's folder).
  const MARK = Symbol.for("ddm.media");
  if (P[MARK]) return;
  try {
    Object.defineProperty(P, MARK, { value: true });
  } catch {
    return;
  }
  const ids = new WeakMap();
  let next = 1;
  const idOf = (el) => {
    let id = ids.get(el);
    if (!id) {
      id = next++;
      ids.set(el, id);
    }
    return id;
  };
  // Out of the document (never added, or taken out) or inside a shadow root: where video.js's
  // listeners on the window don't reach.
  const outside = (el) => !el.isConnected || el.getRootNode() !== el.ownerDocument;
  const tell = (el, what) => {
    try {
      const d = Number(el.duration);
      const detail = {
        id: idOf(el),
        what,
        src: String(el.currentSrc || el.src || "").slice(0, 8192),
        // Seconds; -1 a live stream's (endless), 0 not known yet.
        duration: Number.isFinite(d) && d > 0 ? d : d === Infinity ? -1 : 0,
        paused: Boolean(el.paused),
        muted: Boolean(el.muted) || el.volume === 0,
        // In a shadow root of the page (video.js can look for it there), not out of the document.
        shadow: el.isConnected && el.getRootNode() !== el.ownerDocument,
        // In the document itself (video.js sees it, and its MediaKeys, on its own).
        own: el.isConnected && el.getRootNode() === el.ownerDocument,
        audio: el.localName === "audio",
      };
      window.dispatchEvent(new CustomEvent("ddm-media", { detail: JSON.stringify(detail) }));
    } catch {
      // A page that took the window's events away: nothing told.
    }
  };
  const watched = new WeakSet();
  const watch = (el) => {
    if (watched.has(el)) return;
    watched.add(el);
    const changed = () => {
      if (outside(el)) tell(el, "state");
    };
    for (const type of ["playing", "pause", "ended", "emptied", "volumechange", "durationchange"]) {
      el.addEventListener(type, changed, { passive: true });
    }
  };
  const realPlay = P.play;
  const play = function play() {
    try {
      // Told at once too (not playing yet): video.js asks DDM meanwhile whether the button may show.
      if (outside(this)) {
        watch(this);
        tell(this, "state");
      }
    } catch {
      // Not a media element after all: the browser's own answer follows.
    }
    return realPlay.apply(this, arguments);
  };
  P.play = play;
  const realSetKeys = P.setMediaKeys;
  if (typeof realSetKeys === "function") {
    const setMediaKeys = function setMediaKeys(keys) {
      try {
        if (keys) tell(this, "keys");
      } catch {
        // As above.
      }
      return realSetKeys.apply(this, arguments);
    };
    P.setMediaKeys = setMediaKeys;
  }
})();
