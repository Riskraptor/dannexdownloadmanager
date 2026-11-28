// The lab page's own players for the browser extension's tests (tools/extension/e2e.mjs), as
// sites play: a sound out of the document (`new Audio()`, a music site's or a radio's), a hidden
// <audio> behind the page's own controls (a podcast's), a file fetched in ranges and played from
// a blob: (TikTok's), what the Media Session says plays, MediaKeys set on a player (EME, as
// copy-protected players do; Clear Key, every browser has it). Its script tag's data says which.
// Nothing plays before window.labPlay() (a test's click: browsers play sound only after one);
// <body data-playing> says it plays, data-eme what MediaKeys came to, data-ranged the ranges.
(() => {
  const d = document.currentScript.dataset;
  const body = document.body;
  const steps = [];
  if (d.session) {
    const [title, artist = ""] = d.session.split("|");
    try {
      navigator.mediaSession.metadata = new MediaMetadata({ title, artist });
    } catch {
      body.dataset.session = "unsupported";
    }
  }
  let loose = null;
  if (d.loose) {
    loose = new Audio(d.loose);
    window.labAudio = loose;
  }
  const hidden = d.hidden ? document.getElementById(d.hidden) : null;
  const video = document.getElementById("video");
  // Clear Key MediaKeys on the player `eme` names (video: #video; loose: the sound out of the
  // document): the page decrypts, as a DRM player does.
  const target = d.eme === "loose" ? loose : d.eme === "video" ? video : null;
  if (target) {
    steps.push(async () => {
      try {
        const access = await navigator.requestMediaKeySystemAccess("org.w3.clearkey", [
          {
            initDataTypes: ["keyids", "cenc"],
            audioCapabilities: [{ contentType: 'audio/mp4; codecs="mp4a.40.2"' }],
            videoCapabilities: [{ contentType: 'video/mp4; codecs="avc1.42E01E"' }],
          },
        ]);
        await target.setMediaKeys(await access.createMediaKeys());
        body.dataset.eme = "set";
      } catch (e) {
        body.dataset.eme = `error: ${e && e.message}`;
      }
    });
  }
  // TikTok's way: the whole file fetched 64 KB at a time with Range, then played from a blob:.
  if (d.ranged && video) {
    steps.push(async () => {
      const parts = [];
      let at = 0;
      let total = Infinity;
      while (at < total) {
        const r = await fetch(d.ranged, { headers: { Range: `bytes=${at}-${at + 65535}` } });
        const m = /\/(\d+)\s*$/.exec(r.headers.get("content-range") || "");
        const b = await r.arrayBuffer();
        if (!b.byteLength) break;
        total = m ? Number(m[1]) : at + b.byteLength;
        parts.push(b);
        at += b.byteLength;
      }
      video.src = URL.createObjectURL(new Blob(parts, { type: "video/mp4" }));
      body.dataset.ranged = `${at}`;
    });
  }
  window.labPlay = async () => {
    for (const step of steps) await step();
    for (const p of [loose, hidden, d.ranged ? video : null]) if (p) await p.play();
    body.dataset.playing = String(performance.now());
    return body.dataset.playing;
  };
  window.labPause = () => {
    for (const p of [loose, hidden, video]) if (p) p.pause();
  };
})();
